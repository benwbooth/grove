//! Grove Player's engine: libmpv, driven from Rust. mpv plays whatever
//! FFmpeg reads (every container and codec), subtitles (embedded and
//! sidecar), several audio tracks, URLs (yt-dlp, through mpv's ytdl hook),
//! DVD and Blu-ray folders and images, with hardware decoding (VA-API on
//! AMD).
//!
//!   MpvPlayer { id: mp }                       // the player (no picture)
//!   MpvGlView { handle: mp.handle }            // its picture (cpp/mpv_gl.cpp)
//!   mp.command(JSON.stringify(["loadfile", url]))
//!   mp.setProp("pause", "yes")
//!
//! Nothing waits on mpv on the UI thread: commands and property writes are
//! async, observed properties come back through mpv's wakeup callback,
//! which queues a drain onto the Qt thread.
//!
//! MpvJob is a second, windowless mpv in encoding mode (`o=`: FFmpeg's
//! encoders inside libmpv): Export As 4K/1080p/720p/Audio Only and audio
//! recording (av://pulse). Progress and the end come back as properties
//! and a signal.

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use serde_json::{json, Map, Value};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::pin::Pin;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(u64, handle)]
        #[qproperty(bool, paused)]
        #[qproperty(f64, position)]
        #[qproperty(f64, duration)]
        #[qproperty(f64, volume)]
        #[qproperty(bool, muted)]
        #[qproperty(f64, speed)]
        #[qproperty(bool, eof)]
        #[qproperty(bool, idle)]
        #[qproperty(bool, seekable)]
        #[qproperty(bool, buffering)]
        #[qproperty(QString, title)]
        #[qproperty(QString, path)]
        #[qproperty(i32, video_width, cxx_name = "videoWidth")]
        #[qproperty(i32, video_height, cxx_name = "videoHeight")]
        #[qproperty(bool, has_video, cxx_name = "hasVideo")]
        #[qproperty(bool, album_art, cxx_name = "albumArt")]
        #[qproperty(QString, tracks)]
        #[qproperty(i32, aid)]
        #[qproperty(i32, sid)]
        #[qproperty(i32, rotate)]
        #[qproperty(QString, chapters)]
        #[qproperty(QString, audio_devices, cxx_name = "audioDevices")]
        #[qproperty(QString, audio_device, cxx_name = "audioDevice")]
        #[qproperty(QString, info)]
        type MpvPlayer = super::MpvPlayerRust;

        #[qobject]
        #[qml_element]
        #[qproperty(bool, running)]
        #[qproperty(f64, progress)]
        #[qproperty(f64, position)]
        type MpvJob = super::MpvJobRust;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        /// mpv commands, async: command('["loadfile", "/x.mkv"]').
        #[qinvokable]
        fn command(self: &MpvPlayer, args: &QString);
        /// A property write, async (mpv's text form: "yes", "0.5", "auto").
        #[qinvokable]
        fn set_prop(self: &MpvPlayer, name: &QString, value: &QString);
        /// "file-loaded", "end-file" ({reason, error}), "playback-restart",
        /// "video-reconfig", "command-error" ({error}) ...
        #[qsignal]
        fn mpv_event(self: Pin<&mut MpvPlayer>, name: QString, data: QString);
        /// mpv's warnings and errors.
        #[qsignal]
        fn log_message(self: Pin<&mut MpvPlayer>, level: QString, text: QString);

        /// {input, output, options: {mpv option: value}, length (seconds;
        /// 0 for a recording)}. False if it couldn't start (finished says why).
        #[qinvokable]
        fn start(self: Pin<&mut MpvJob>, spec: &QString) -> bool;
        /// keep: finish the file (a recording); otherwise stop and delete it.
        #[qinvokable]
        fn stop(self: Pin<&mut MpvJob>, keep: bool);
        #[qsignal]
        fn finished(self: Pin<&mut MpvJob>, ok: bool, error: QString, output: QString);
    }

    impl cxx_qt::Threading for MpvPlayer {}
    impl cxx_qt::Initialize for MpvPlayer {}
    impl cxx_qt::Threading for MpvJob {}
}

// ---------------------------------------------------------------- libmpv

#[allow(non_camel_case_types)]
type mpv_handle = c_void;

#[repr(C)]
struct MpvNodeList {
    num: c_int,
    values: *mut MpvNode,
    keys: *mut *mut c_char,
}

#[repr(C)]
union MpvNodeU {
    string: *mut c_char,
    flag: c_int,
    int64: i64,
    double: f64,
    list: *mut MpvNodeList,
    ba: *mut c_void,
}

#[repr(C)]
struct MpvNode {
    u: MpvNodeU,
    format: c_int,
}

#[repr(C)]
struct MpvEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct MpvEventProperty {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct MpvEventLogMessage {
    prefix: *const c_char,
    level: *const c_char,
    text: *const c_char,
    log_level: c_int,
}

#[repr(C)]
struct MpvEventEndFile {
    reason: c_int,
    error: c_int,
}

const FORMAT_STRING: c_int = 1;
const FORMAT_FLAG: c_int = 3;
const FORMAT_INT64: c_int = 4;
const FORMAT_DOUBLE: c_int = 5;
const FORMAT_NODE: c_int = 6;
const FORMAT_NODE_ARRAY: c_int = 7;
const FORMAT_NODE_MAP: c_int = 8;

const EVENT_NONE: c_int = 0;
const EVENT_SHUTDOWN: c_int = 1;
const EVENT_LOG_MESSAGE: c_int = 2;
const EVENT_COMMAND_REPLY: c_int = 5;
const EVENT_END_FILE: c_int = 7;
const EVENT_PROPERTY_CHANGE: c_int = 22;

#[link(name = "mpv")]
extern "C" {
    fn mpv_create() -> *mut mpv_handle;
    fn mpv_initialize(ctx: *mut mpv_handle) -> c_int;
    fn mpv_destroy(ctx: *mut mpv_handle);
    fn mpv_set_option_string(ctx: *mut mpv_handle, name: *const c_char, data: *const c_char) -> c_int;
    fn mpv_command_async(ctx: *mut mpv_handle, reply: u64, args: *mut *const c_char) -> c_int;
    fn mpv_set_property_async(ctx: *mut mpv_handle, reply: u64, name: *const c_char, format: c_int, data: *mut c_void) -> c_int;
    fn mpv_observe_property(ctx: *mut mpv_handle, reply: u64, name: *const c_char, format: c_int) -> c_int;
    fn mpv_request_log_messages(ctx: *mut mpv_handle, min_level: *const c_char) -> c_int;
    fn mpv_wait_event(ctx: *mut mpv_handle, timeout: f64) -> *mut MpvEvent;
    fn mpv_set_wakeup_callback(ctx: *mut mpv_handle, cb: Option<extern "C" fn(*mut c_void)>, d: *mut c_void);
    fn mpv_error_string(error: c_int) -> *const c_char;
    fn mpv_event_name(event: c_int) -> *const c_char;
}

fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

fn c(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

fn error_string(e: c_int) -> String {
    cstr(unsafe { mpv_error_string(e) })
}

unsafe fn node_value(n: &MpvNode) -> Value {
    match n.format {
        FORMAT_STRING => Value::String(cstr(n.u.string)),
        FORMAT_FLAG => Value::Bool(n.u.flag != 0),
        FORMAT_INT64 => json!(n.u.int64),
        FORMAT_DOUBLE => json!(n.u.double),
        FORMAT_NODE_ARRAY | FORMAT_NODE_MAP if !n.u.list.is_null() => {
            let l = &*n.u.list;
            let vals = std::slice::from_raw_parts(l.values, l.num.max(0) as usize);
            if n.format == FORMAT_NODE_ARRAY {
                Value::Array(vals.iter().map(|v| node_value(v)).collect())
            } else {
                let keys = std::slice::from_raw_parts(l.keys, l.num.max(0) as usize);
                let mut m = Map::new();
                for (k, v) in keys.iter().zip(vals) {
                    m.insert(cstr(*k), node_value(v));
                }
                Value::Object(m)
            }
        }
        _ => Value::Null,
    }
}

enum Event {
    Property(u64, Value),
    Log(String, String),
    EndFile(&'static str, Option<String>),
    CommandError(String),
    Shutdown,
    Other(String),
}

/// One mpv client handle. Dropping it detaches (mpv_destroy, on a thread
/// of its own: the last client out tears the core down, which waits for
/// mpv's threads); the core lives on while another client (the picture's)
/// still holds it.
struct Mpv {
    h: *mut mpv_handle,
    wake: *mut c_void,
    wake_free: Option<unsafe fn(*mut c_void)>,
}

unsafe impl Send for Mpv {}

impl Mpv {
    fn create(options: &[(&str, &str)]) -> Result<Mpv, String> {
        // mpv refuses to start unless numbers are written the C way.
        unsafe { libc::setlocale(libc::LC_NUMERIC, c"C".as_ptr()) };
        let h = unsafe { mpv_create() };
        if h.is_null() {
            return Err("mpv couldn't start".into());
        }
        let m = Mpv { h, wake: std::ptr::null_mut(), wake_free: None };
        for (k, v) in options {
            m.option(k, v)?;
        }
        Ok(m)
    }

    fn option(&self, k: &str, v: &str) -> Result<(), String> {
        let r = unsafe { mpv_set_option_string(self.h, c(k).as_ptr(), c(v).as_ptr()) };
        if r < 0 {
            Err(format!("{k}={v}: {}", error_string(r)))
        } else {
            Ok(())
        }
    }

    fn init(&self, log: &str) -> Result<(), String> {
        let r = unsafe { mpv_initialize(self.h) };
        if r < 0 {
            return Err(error_string(r));
        }
        unsafe { mpv_request_log_messages(self.h, c(log).as_ptr()) };
        Ok(())
    }

    fn command(&self, args: &[String]) {
        let cs: Vec<CString> = args.iter().map(|a| c(a)).collect();
        let mut ptrs: Vec<*const c_char> = cs.iter().map(|s| s.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        unsafe { mpv_command_async(self.h, 0, ptrs.as_mut_ptr()) };
    }

    fn set(&self, name: &str, value: &str) {
        let v = c(value);
        let mut p = v.as_ptr();
        unsafe { mpv_set_property_async(self.h, 0, c(name).as_ptr(), FORMAT_STRING, &mut p as *mut *const c_char as *mut c_void) };
    }

    fn observe(&self, id: u64, name: &str) {
        unsafe { mpv_observe_property(self.h, id, c(name).as_ptr(), FORMAT_NODE) };
    }

    /// Call `on_wake` (on mpv's threads) whenever events wait.
    fn on_wake<T: Fn() + Send + Sync + 'static>(&mut self, on_wake: T) {
        extern "C" fn tramp<T: Fn()>(d: *mut c_void) {
            unsafe { (*(d as *const T))() }
        }
        unsafe fn free<T>(d: *mut c_void) {
            drop(Box::from_raw(d as *mut T));
        }
        let d = Box::into_raw(Box::new(on_wake)) as *mut c_void;
        self.wake = d;
        self.wake_free = Some(free::<T>);
        unsafe { mpv_set_wakeup_callback(self.h, Some(tramp::<T>), d) };
    }

    fn next(&self) -> Option<Event> {
        let e = unsafe { &*mpv_wait_event(self.h, 0.0) };
        Some(match e.event_id {
            EVENT_NONE => return None,
            EVENT_PROPERTY_CHANGE => {
                let p = unsafe { &*(e.data as *const MpvEventProperty) };
                let v = if p.format == FORMAT_NODE && !p.data.is_null() { unsafe { node_value(&*(p.data as *const MpvNode)) } } else { Value::Null };
                Event::Property(e.reply_userdata, v)
            }
            EVENT_LOG_MESSAGE => {
                let m = unsafe { &*(e.data as *const MpvEventLogMessage) };
                Event::Log(cstr(m.level), format!("{}: {}", cstr(m.prefix), cstr(m.text).trim()))
            }
            EVENT_END_FILE => {
                let f = unsafe { &*(e.data as *const MpvEventEndFile) };
                let reason = match f.reason {
                    0 => "eof",
                    2 => "stop",
                    3 => "quit",
                    4 => "error",
                    5 => "redirect",
                    _ => "unknown",
                };
                Event::EndFile(reason, (f.reason == 4).then(|| error_string(f.error)))
            }
            EVENT_COMMAND_REPLY if e.error < 0 => Event::CommandError(error_string(e.error)),
            EVENT_SHUTDOWN => Event::Shutdown,
            id => Event::Other(cstr(unsafe { mpv_event_name(id) })),
        })
    }
}

impl Mpv {
    fn unhook(&mut self) {
        unsafe { mpv_set_wakeup_callback(self.h, None, std::ptr::null_mut()) };
        if let Some(f) = self.wake_free.take() {
            unsafe { f(self.wake) };
        }
    }

    /// Detach and wait (an encoder writes the end of its file as the core
    /// goes). Call off the UI thread.
    fn destroy_now(mut self) {
        self.unhook();
        unsafe { mpv_destroy(self.h) };
        std::mem::forget(self);
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        self.unhook();
        let h = self.h as usize;
        std::thread::spawn(move || unsafe { mpv_destroy(h as *mut mpv_handle) });
    }
}

// ---------------------------------------------------------------- player

/// Watched for the window; the rest of these go into `info` (the Movie
/// Inspector, the cover's title and artist).
const WATCHED: &[&str] = &[
    "pause", "time-pos", "duration", "volume", "mute", "speed", "eof-reached", "idle-active", "seekable",
    "paused-for-cache", "media-title", "path", "dwidth", "dheight", "track-list",
    "current-tracks/audio", "current-tracks/sub", "current-tracks/video",
    "video-rotate", "chapter-list", "audio-device-list", "audio-device",
    // info:
    "video-codec", "video-format", "audio-codec-name", "audio-codec", "video-params", "audio-params", "container-fps",
    "estimated-vf-fps", "video-bitrate", "audio-bitrate", "file-format", "file-size", "metadata", "hwdec-current",
    "current-vo", "current-ao", "chapter", "demuxer-cache-duration", "sub-visibility", "panscan", "video-zoom",
];

#[derive(Default)]
pub struct MpvPlayerRust {
    handle: u64,
    paused: bool,
    position: f64,
    duration: f64,
    volume: f64,
    muted: bool,
    speed: f64,
    eof: bool,
    idle: bool,
    seekable: bool,
    buffering: bool,
    title: QString,
    path: QString,
    video_width: i32,
    video_height: i32,
    has_video: bool,
    album_art: bool,
    tracks: QString,
    aid: i32,
    sid: i32,
    rotate: i32,
    chapters: QString,
    audio_devices: QString,
    audio_device: QString,
    info: QString,
    mpv: Option<Mpv>,
    info_map: Map<String, Value>,
    track_list: Vec<Value>,
    vid: i64,
}

fn qs(s: &str) -> QString {
    QString::from(s)
}

/// A track id however mpv says it (a number, "3", or "no"/"auto": 0).
fn track_id(v: &Value) -> i64 {
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0)
}

impl cxx_qt::Initialize for qobject::MpvPlayer {
    fn initialize(mut self: Pin<&mut Self>) {
        let hwdec = std::env::var("GROVE_PLAYER_HWDEC").unwrap_or_else(|_| "auto-safe".into());
        let home = std::env::var("HOME").unwrap_or_default();
        let opts: Vec<(&str, &str)> = vec![
            ("vo", "libmpv"),
            // VA-API on the AMD GPU; software where it can't.
            ("hwdec", hwdec.as_str()),
            ("keep-open", "yes"),
            ("idle", "yes"),
            ("input-default-bindings", "no"),
            ("input-vo-keyboard", "no"),
            ("osc", "no"),
            ("osd-level", "0"),
            ("terminal", "no"),
            ("config", "no"),
            // URLs through yt-dlp (mpv's own ytdl hook).
            ("ytdl", "yes"),
            ("ytdl-format", "bestvideo[height<=?2160]+bestaudio/best"),
            ("sub-auto", "fuzzy"),
            ("cover-art-auto", "fuzzy"),
            ("audio-display", "embedded-first"),
            ("image-display-duration", "inf"),
            ("sub-font", "Inter"),
            ("sub-font-size", "46"),
            ("sub-border-size", "2.5"),
            ("sub-shadow-offset", "1"),
            ("screenshot-directory", home.as_str()),
            ("cache", "auto"),
            ("demuxer-max-bytes", "200MiB"),
            ("volume", "80"),
        ];
        // GROVE_PLAYER_DEBUG=1: mpv's log on the terminal.
        let debug = std::env::var_os("GROVE_PLAYER_DEBUG").is_some();
        // Tests: GROVE_PLAYER_AO=null keeps the sound off the real speakers.
        let ao = std::env::var("GROVE_PLAYER_AO").unwrap_or_default();
        let mut opts = opts;
        if !ao.is_empty() {
            opts.push(("ao", ao.as_str()));
        }
        if debug {
            opts.push(("terminal", "yes"));
            opts.push(("msg-level", "all=v"));
        }
        let mut mpv = match Mpv::create(&opts).and_then(|m| m.init("warn").map(|_| m)) {
            Ok(m) => m,
            Err(e) => {
                self.as_mut().log_message(qs("error"), qs(&e));
                return;
            }
        };
        for (i, name) in WATCHED.iter().enumerate() {
            mpv.observe(i as u64 + 1, name);
        }
        let qt: CxxQtThread<qobject::MpvPlayer> = self.qt_thread();
        mpv.on_wake(move || {
            let _ = qt.queue(|p: Pin<&mut qobject::MpvPlayer>| p.drain());
        });
        let h = mpv.h as u64;
        self.as_mut().rust_mut().speed = 1.0;
        self.as_mut().rust_mut().mpv = Some(mpv);
        self.as_mut().set_handle(h);
    }
}

impl Drop for MpvPlayerRust {
    fn drop(&mut self) {
        // Stop the sound now; the core goes once the picture lets go too.
        if let Some(m) = &self.mpv {
            m.command(&["quit".into()]);
        }
    }
}

impl qobject::MpvPlayer {
    pub fn command(&self, args: &QString) {
        let Some(m) = &self.rust().mpv else { return };
        let v: Value = serde_json::from_str(&args.to_string()).unwrap_or(Value::Null);
        let Some(a) = v.as_array() else { return };
        let args: Vec<String> = a
            .iter()
            .map(|x| match x {
                Value::String(s) => s.clone(),
                Value::Bool(b) => (if *b { "yes" } else { "no" }).into(),
                o => o.to_string(),
            })
            .collect();
        m.command(&args);
    }

    pub fn set_prop(&self, name: &QString, value: &QString) {
        if let Some(m) = &self.rust().mpv {
            m.set(&name.to_string(), &value.to_string());
        }
    }

    fn drain(mut self: Pin<&mut Self>) {
        let mut events = Vec::new();
        if let Some(m) = &self.rust().mpv {
            while let Some(e) = m.next() {
                events.push(e);
            }
        }
        let mut info_changed = false;
        for e in events {
            match e {
                Event::Property(id, v) => {
                    let Some(name) = WATCHED.get(id.wrapping_sub(1) as usize) else { continue };
                    info_changed |= self.as_mut().apply(name, v);
                }
                Event::Log(level, text) => self.as_mut().log_message(qs(&level), qs(&text)),
                Event::EndFile(reason, err) => {
                    let d = json!({"reason": reason, "error": err});
                    self.as_mut().mpv_event(qs("end-file"), qs(&d.to_string()));
                }
                Event::CommandError(err) => self.as_mut().mpv_event(qs("command-error"), qs(&json!({"error": err}).to_string())),
                Event::Shutdown => self.as_mut().mpv_event(qs("shutdown"), qs("{}")),
                Event::Other(name) => self.as_mut().mpv_event(qs(&name), qs("{}")),
            }
        }
        if info_changed {
            let s = Value::Object(self.rust().info_map.clone()).to_string();
            self.as_mut().set_info(qs(&s));
        }
    }

    /// One observed property into the window's properties; true if it
    /// went into `info`.
    fn apply(mut self: Pin<&mut Self>, name: &str, v: Value) -> bool {
        let f = v.as_f64().unwrap_or(0.0);
        let b = v.as_bool().unwrap_or(false);
        match name {
            "pause" => self.set_paused(b),
            "time-pos" => self.set_position(f),
            "duration" => self.set_duration(f),
            "volume" => self.set_volume(f),
            "mute" => self.set_muted(b),
            "speed" => self.set_speed(if f > 0.0 { f } else { 1.0 }),
            "eof-reached" => self.set_eof(b),
            "idle-active" => self.set_idle(b),
            "seekable" => self.set_seekable(b),
            "paused-for-cache" => self.set_buffering(b),
            "media-title" => self.set_title(qs(v.as_str().unwrap_or(""))),
            "path" => self.set_path(qs(v.as_str().unwrap_or(""))),
            "dwidth" => self.set_video_width(f as i32),
            "dheight" => self.set_video_height(f as i32),
            // The selected track (null when none).
            "current-tracks/audio" => self.set_aid(track_id(&v["id"]) as i32),
            "current-tracks/sub" => self.set_sid(track_id(&v["id"]) as i32),
            "video-rotate" => self.set_rotate(f as i32),
            "chapter-list" => self.set_chapters(qs(&v.to_string())),
            "audio-device-list" => self.set_audio_devices(qs(&v.to_string())),
            "audio-device" => self.set_audio_device(qs(v.as_str().unwrap_or(""))),
            "track-list" => {
                self.as_mut().rust_mut().track_list = v.as_array().cloned().unwrap_or_default();
                self.as_mut().set_tracks(qs(&v.to_string()));
                self.update_video();
            }
            "current-tracks/video" => {
                self.as_mut().rust_mut().vid = track_id(&v["id"]);
                self.update_video();
            }
            other => {
                self.as_mut().rust_mut().info_map.insert(other.replace('-', "_"), v);
                return true;
            }
        }
        false
    }

    /// A picture, or only cover art (music)?
    fn update_video(mut self: Pin<&mut Self>) {
        let vid = self.rust().vid;
        let t = self.rust().track_list.iter().find(|t| t["type"] == "video" && t["id"].as_i64() == Some(vid)).cloned();
        let art = t.as_ref().map(|t| t["albumart"].as_bool().unwrap_or(false) || t["image"].as_bool().unwrap_or(false)).unwrap_or(false);
        self.as_mut().set_has_video(t.is_some() && !art);
        self.as_mut().set_album_art(art);
    }
}

// ---------------------------------------------------------------- jobs

#[derive(Default)]
pub struct MpvJobRust {
    running: bool,
    progress: f64,
    position: f64,
    mpv: Option<Mpv>,
    output: String,
    length: f64,
    cancelled: bool,
    last_error: String,
}

impl Drop for MpvJobRust {
    fn drop(&mut self) {
        if let Some(m) = self.mpv.take() {
            m.command(&["quit".into()]);
            // An export left unfinished isn't a file anyone wants.
            if self.length > 0.0 {
                let out = self.output.clone();
                drop(m);
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    let _ = std::fs::remove_file(out);
                });
            }
        }
    }
}

impl qobject::MpvJob {
    pub fn start(mut self: Pin<&mut Self>, spec: &QString) -> bool {
        if self.rust().mpv.is_some() {
            return false;
        }
        let spec: Value = serde_json::from_str(&spec.to_string()).unwrap_or(Value::Null);
        let output = spec["output"].as_str().unwrap_or("").to_string();
        let input = spec["input"].as_str().unwrap_or("").to_string();
        {
            let mut r = self.as_mut().rust_mut();
            r.output = output.clone();
            r.length = spec["length"].as_f64().unwrap_or(0.0);
            r.cancelled = false;
            r.last_error.clear();
        }
        let mut opts: Vec<(String, String)> = vec![
            ("terminal".into(), "no".into()),
            ("config".into(), "no".into()),
            ("idle".into(), "no".into()),
            ("ytdl".into(), "yes".into()),
            ("audio-display".into(), "no".into()),
            ("o".into(), output.clone()),
        ];
        if let Some(o) = spec["options"].as_object() {
            for (k, v) in o {
                opts.push((k.clone(), v.as_str().map(String::from).unwrap_or_else(|| v.to_string())));
            }
        }
        let refs: Vec<(&str, &str)> = opts.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let mut mpv = match Mpv::create(&refs).and_then(|m| m.init("error").map(|_| m)) {
            Ok(m) => m,
            Err(e) => {
                self.as_mut().finished(false, qs(&e), qs(&output));
                return false;
            }
        };
        unsafe { mpv_observe_property(mpv.h, 1, c"time-pos".as_ptr(), FORMAT_DOUBLE) };
        let qt: CxxQtThread<qobject::MpvJob> = self.qt_thread();
        mpv.on_wake(move || {
            let _ = qt.queue(|j: Pin<&mut qobject::MpvJob>| j.drain());
        });
        mpv.command(&["loadfile".into(), input]);
        self.as_mut().rust_mut().mpv = Some(mpv);
        self.as_mut().set_position(0.0);
        self.as_mut().set_progress(0.0);
        self.as_mut().set_running(true);
        true
    }

    pub fn stop(mut self: Pin<&mut Self>, keep: bool) {
        if let Some(m) = &self.rust().mpv {
            // "quit" ends playback; the encoder finishes the file.
            m.command(&["quit".into()]);
        }
        self.as_mut().rust_mut().cancelled = !keep;
    }

    fn drain(mut self: Pin<&mut Self>) {
        loop {
            let e = match &self.rust().mpv {
                Some(m) => m.next(),
                None => return,
            };
            let Some(e) = e else { return };
            match e {
                Event::Property(_, v) => {
                    if let Some(p) = v.as_f64() {
                        self.as_mut().set_position(p);
                        let len = self.rust().length;
                        if len > 0.0 {
                            self.as_mut().set_progress((p / len).clamp(0.0, 1.0));
                        }
                    }
                }
                Event::Log(_, text) => self.as_mut().rust_mut().last_error = text,
                Event::EndFile("error", err) => {
                    let msg = if self.rust().last_error.is_empty() { err.unwrap_or_default() } else { self.rust().last_error.clone() };
                    return self.end(false, msg);
                }
                // idle=no: mpv quits once the file is done (or on stop()).
                Event::Shutdown => return self.end(true, String::new()),
                _ => {}
            }
        }
    }

    fn end(mut self: Pin<&mut Self>, ok: bool, error: String) {
        let Some(m) = self.as_mut().rust_mut().mpv.take() else { return };
        let cancelled = self.rust().cancelled;
        let output = self.rust().output.clone();
        let good = ok && !cancelled;
        let error = if cancelled { String::new() } else { error };
        // The file is finished once the core is gone: say so then.
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            m.destroy_now();
            let ok = good && std::fs::metadata(&output).map(|m| m.len() > 0).unwrap_or(false);
            if !ok {
                let _ = std::fs::remove_file(&output);
            }
            let error = if !ok && good { "Nothing was written.".to_string() } else { error };
            let _ = qt.queue(move |mut j: Pin<&mut qobject::MpvJob>| {
                if ok {
                    j.as_mut().set_progress(1.0);
                }
                j.as_mut().set_running(false);
                j.as_mut().finished(ok, qs(&error), qs(&output));
            });
        });
    }
}
