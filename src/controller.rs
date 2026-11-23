//! The `Grove` controller: everything QML asks of the system, done in the
//! background and answered with signals.

use crate::bridge::ffi::{self, QString};
use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use grove_core::ops::{self, Action, Control, History, Kind, Resolution, Spec};
use grove_core::{apps, settings, tags, trash, util};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, OnceLock};

pub struct GroveRust {
    pub(crate) undo_label: QString,
    pub(crate) redo_label: QString,
    pub(crate) places_revision: i32,
    pub(crate) settings_json: QString,
    settings: settings::Settings,
    views: HashMap<String, settings::FolderView>,
    next_job: i32,
}

impl Default for GroveRust {
    fn default() -> Self {
        let s = startup().settings.clone();
        GroveRust {
            undo_label: QString::default(),
            redo_label: QString::default(),
            places_revision: 0,
            settings_json: QString::from(serde_json::to_string(&s).unwrap().as_str()),
            settings: s,
            views: startup().views.clone(),
            next_job: 1,
        }
    }
}

/// Loaded once in main() before any window exists.
pub struct Startup {
    pub settings: settings::Settings,
    pub views: HashMap<String, settings::FolderView>,
    pub session: String,
}

fn session_path() -> PathBuf {
    util::grove_state().join("session.json")
}

fn load_startup() -> Startup {
    Startup { settings: settings::load(), views: settings::folder_views(), session: std::fs::read_to_string(session_path()).unwrap_or_default() }
}

static STARTUP: OnceLock<Startup> = OnceLock::new();

/// The system's name (os-release NAME, e.g. "NixOS"): the system disk's
/// name in Computer. Read once (at startup, off the UI thread's work).
pub fn os_name() -> &'static str {
    static N: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    N.get_or_init(|| {
        std::fs::read_to_string("/etc/os-release").ok()
            .and_then(|t| t.lines().find_map(|l| l.strip_prefix("NAME=").map(|v| v.trim_matches('"').to_string())))
            .unwrap_or_else(|| "System".into())
    })
}

pub fn init_startup() {
    let _ = os_name();
    let _ = STARTUP.set(load_startup());
}

fn startup() -> &'static Startup {
    STARTUP.get_or_init(load_startup)
}

fn history() -> &'static Mutex<History> {
    static H: OnceLock<Mutex<History>> = OnceLock::new();
    H.get_or_init(Default::default)
}

struct JobState {
    ctl: Arc<Control>,
    answer: Mutex<Option<mpsc::Sender<(Resolution, bool)>>>,
}

fn jobs() -> &'static Mutex<HashMap<i32, Arc<JobState>>> {
    static J: OnceLock<Mutex<HashMap<i32, Arc<JobState>>>> = OnceLock::new();
    J.get_or_init(Default::default)
}

/// The controller's Qt thread handle, for D-Bus and the portal.
pub static QT: OnceLock<Mutex<Option<CxxQtThread<ffi::Grove>>>> = OnceLock::new();

fn qs(s: &str) -> QString {
    QString::from(s)
}

fn paths_of(json: &QString) -> Vec<PathBuf> {
    serde_json::from_str::<Vec<String>>(&json.to_string()).unwrap_or_default().into_iter().map(PathBuf::from).collect()
}

/// Run on a background lane ("files" keeps file changes in order).
fn lane(name: &'static str, f: impl FnOnce() + Send + 'static) {
    use std::sync::mpsc::Sender;
    type Job = Box<dyn FnOnce() + Send>;
    static LANES: OnceLock<Mutex<HashMap<&'static str, Sender<Job>>>> = OnceLock::new();
    let mut l = LANES.get_or_init(Default::default).lock();
    let tx = l.entry(name).or_insert_with(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name(format!("grove-{name}"))
            .spawn(move || {
                for j in rx {
                    j();
                }
            })
            .unwrap();
        tx
    });
    let _ = tx.send(Box::new(f));
}

struct JobHost {
    id: i32,
    title: String,
    qt: CxxQtThread<ffi::Grove>,
    state: Arc<JobState>,
}

impl ops::Host for JobHost {
    fn progress(&self, p: &ops::Progress) {
        let paused = self.state.ctl.is_paused();
        let j = json!({"id": self.id, "title": self.title, "state": if paused {"paused"} else {"running"},
            "totalBytes": p.total_bytes, "doneBytes": p.done_bytes, "totalItems": p.total_items, "doneItems": p.done_items, "current": p.current});
        let id = self.id;
        let s = j.to_string();
        let _ = self.qt.queue(move |g| g.job_changed(id, qs(&s)));
    }
    fn conflict(&self, c: &ops::ConflictInfo) -> (Resolution, bool) {
        let (tx, rx) = mpsc::channel();
        *self.state.answer.lock() = Some(tx);
        let id = self.id;
        let s = serde_json::to_string(c).unwrap();
        let _ = self.qt.queue(move |g| g.job_conflict(id, qs(&s)));
        loop {
            match rx.recv_timeout(std::time::Duration::from_millis(200)) {
                Ok(a) => return a,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if self.state.ctl.is_cancelled() {
                        return (Resolution::Cancel, false);
                    }
                }
                Err(_) => return (Resolution::Cancel, false),
            }
        }
    }
}

impl ffi::Grove {
    fn answer(&self, key: &QString, f: impl FnOnce() -> Value + Send + 'static) {
        let key = key.to_string();
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let v = f().to_string();
            let _ = qt.queue(move |g| g.async_reply(qs(&key), qs(&v)));
        });
    }

    fn answer_on(&self, name: &'static str, key: &QString, f: impl FnOnce() -> Value + Send + 'static) {
        let key = key.to_string();
        let qt = self.qt_thread();
        lane(name, move || {
            let v = f().to_string();
            let _ = qt.queue(move |g| g.async_reply(qs(&key), qs(&v)));
        });
    }

    fn refresh_history(mut self: Pin<&mut Self>) {
        let (u, r) = {
            let h = history().lock();
            (h.undo_label().unwrap_or("").to_string(), h.redo_label().unwrap_or("").to_string())
        };
        self.as_mut().set_undo_label(qs(&u));
        self.as_mut().set_redo_label(qs(&r));
    }

    /// Record an undoable action (from any thread).
    fn push_history(qt: &CxxQtThread<ffi::Grove>, label: &str, a: Action) {
        history().lock().push(label, a);
        let _ = qt.queue(|g| g.refresh_history());
    }

    fn touched(qt: &CxxQtThread<ffi::Grove>, dirs: Vec<String>) {
        let s = serde_json::to_string(&dirs).unwrap();
        let _ = qt.queue(move |g| g.folders_touched(qs(&s)));
    }

    pub fn start(self: Pin<&mut Self>) {
        *QT.get_or_init(|| Mutex::new(None)).lock() = Some(self.qt_thread());
        // Mounts come and go: watch /proc/self/mountinfo (POLLPRI).
        let qt = self.qt_thread();
        std::thread::spawn(move || unsafe {
            let c = std::ffi::CString::new("/proc/self/mountinfo").unwrap();
            let fd = libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC);
            if fd < 0 {
                return;
            }
            loop {
                let mut p = libc::pollfd { fd, events: libc::POLLPRI | libc::POLLERR, revents: 0 };
                if libc::poll(&mut p, 1, -1) <= 0 {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    continue;
                }
                let mut buf = [0u8; 4096];
                libc::lseek(fd, 0, libc::SEEK_SET);
                while libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) > 0 {}
                std::thread::sleep(std::time::Duration::from_millis(300));
                let _ = qt.queue(|mut g| {
                    let r = *g.places_revision() + 1;
                    g.as_mut().set_places_revision(r);
                });
            }
        });
        // hearth's accounts service: saved servers and Google sign-ins.
        let qt = self.qt_thread();
        crate::accounts::watch(move |available| {
            let _ = qt.queue(move |mut g| {
                g.as_mut().accounts_changed(available);
                let r = *g.places_revision() + 1;
                g.as_mut().set_places_revision(r);
            });
        });
        if std::env::var_os("GROVE_PORTAL").is_some() {
            crate::portal::serve(self.qt_thread());
        } else {
            crate::dbus::serve(self.qt_thread());
        }
    }

    pub fn os_name(&self) -> QString {
        qs(os_name())
    }

    pub fn home(&self) -> QString {
        qs(&util::home().to_string_lossy())
    }

    pub fn env(&self, name: &QString) -> QString {
        qs(&std::env::var(name.to_string()).unwrap_or_default())
    }

    // ---------------- opening ----------------

    pub fn open_paths(&self, paths: &QString) {
        let paths = paths_of(paths);
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let all = apps::all();
            // Group by default app so "open 5 photos" is one viewer.
            let mut groups: Vec<(Option<apps::App>, Vec<PathBuf>, String)> = Vec::new();
            for p in paths {
                // A file inside an archive: take it out first.
                let p = match p.to_string_lossy().strip_prefix("archive://") {
                    Some(rest) => {
                        match crate::archive::local_copy(&format!("archive://{rest}"), true).ok_or_else(|| "Couldn't take it out of the archive.".to_string()) {
                            Ok(x) => x,
                            Err(e) => {
                                let _ = qt.queue(move |g| g.notify(qs("error"), qs(&e)));
                                continue;
                            }
                        }
                    }
                    None => p,
                };
                // A Google Docs/Sheets/... link file: its page in the browser.
                if let Some(l) = grove_core::gdocs::link(&p) {
                    grove_core::recents::add(&p, "application/vnd.google-apps");
                    if let Err(e) = apps::open_url(&l.url) {
                        let _ = qt.queue(move |g| g.notify(qs("error"), qs(&e)));
                    }
                    continue;
                }
                let m = crate::mime::deep(&p.to_string_lossy());
                let mimes = crate::mime::with_parents(&m.name);
                let app = apps::default_for(&mimes, &all);
                grove_core::recents::add(&p, &m.name);
                match groups.iter_mut().find(|g| g.0.as_ref().map(|a| &a.id) == app.as_ref().map(|a| &a.id)) {
                    Some(g) => g.1.push(p),
                    None => groups.push((app, vec![p], m.comment.clone())),
                }
            }
            for (app, files, kind) in groups {
                let r = match app {
                    Some(a) => apps::launch(&a, &files),
                    None => Err(format!(
                        "There's no app set to open {} files (“{}”). Use Open With to choose one.",
                        kind,
                        files[0].file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
                    )),
                };
                if let Err(e) = r {
                    let _ = qt.queue(move |g| g.notify(qs("error"), qs(&e)));
                }
            }
        });
    }

    pub fn open_with(&self, paths: &QString, app_id: &QString) {
        let paths = paths_of(paths);
        let id = app_id.to_string();
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            // Files inside archives: their copies taken out to the cache
            // (an app can't open an archive:// path).
            let mut local = Vec::with_capacity(paths.len());
            for p in &paths {
                let s = p.to_string_lossy();
                if s.starts_with("archive://") {
                    match crate::archive::local_copy(&s, true) {
                        Some(l) => local.push(l),
                        None => {
                            let e = format!("Couldn't take “{}” out of the archive.", s.rsplit('/').next().unwrap_or(&s));
                            let _ = qt.queue(move |g| g.notify(qs("error"), qs(&e)));
                            return;
                        }
                    }
                } else {
                    local.push(p.clone());
                }
            }
            let paths = local;
            let r = match apps::by_id(&id) {
                Some(a) => apps::launch(&a, &paths),
                None => Err(format!("The app “{id}” isn't installed any more.")),
            };
            if let Err(e) = r {
                let _ = qt.queue(move |g| g.notify(qs("error"), qs(&e)));
            }
        });
    }

    pub fn apps_for(&self, key: &QString, path: &QString) {
        let p = path.to_string();
        self.answer(key, move || {
            let m = crate::mime::deep(&p);
            let mimes = crate::mime::with_parents(&m.name);
            let all = apps::all();
            let def = apps::default_for(&mimes, &all).map(|a| a.id);
            let list: Vec<Value> = apps::apps_for(&mimes, &all)
                .into_iter()
                .map(|a| json!({"id": a.id, "name": a.name, "icon": a.icon, "isDefault": Some(&a.id) == def.as_ref()}))
                .collect();
            json!({"mime": m.name, "kind": m.comment, "apps": list})
        });
    }

    pub fn all_apps(&self, key: &QString) {
        self.answer(key, || {
            let mut v: Vec<Value> = apps::all().into_iter().filter(|a| !a.no_display && !a.exec.is_empty()).map(|a| json!({"id": a.id, "name": a.name, "icon": a.icon, "comment": a.comment})).collect();
            v.sort_by(|a, b| a["name"].as_str().unwrap_or("").to_lowercase().cmp(&b["name"].as_str().unwrap_or("").to_lowercase()));
            Value::Array(v)
        });
    }

    pub fn set_default_app(&self, key: &QString, mime: &QString, app_id: &QString) {
        let (m, a) = (mime.to_string(), app_id.to_string());
        self.answer_on("settings", key, move || match apps::set_default(&m, &a) {
            Ok(()) => json!({"ok": true}),
            Err(e) => json!({"error": e}),
        });
    }

    pub fn open_terminal(&self, dir: &QString) {
        let d = PathBuf::from(dir.to_string());
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            if let Err(e) = apps::open_terminal(&d) {
                let _ = qt.queue(move |g| g.notify(qs("error"), qs(&e)));
            }
        });
    }

    pub fn run_command(&self, argv: &QString, cwd: &QString) {
        let argv: Vec<String> = serde_json::from_str(&argv.to_string()).unwrap_or_default();
        let cwd = cwd.to_string();
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let d = if cwd.is_empty() { None } else { Some(PathBuf::from(cwd)) };
            if let Err(e) = apps::spawn_detached(&argv, d.as_deref()) {
                let _ = qt.queue(move |g| g.notify(qs("error"), qs(&e)));
            }
        });
    }

    // ---------------- file operations ----------------

    pub fn run_job(mut self: Pin<&mut Self>, kind: &QString, sources: &QString, dest: &QString) -> i32 {
        let kind = Kind::from_str(&kind.to_string());
        let sources = paths_of(sources);
        if sources.is_empty() {
            return -1;
        }
        let dest = PathBuf::from(dest.to_string());
        let id = self.rust().next_job;
        self.as_mut().rust_mut().next_job += 1;
        let n = sources.len();
        let first = sources[0].file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let what = if n == 1 { format!("“{first}”") } else { format!("{n} items") };
        let title = match kind {
            Kind::Duplicate => format!("Duplicating {what}"),
            _ => format!("{} {what} to “{}”", kind.verb(), dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| dest.display().to_string())),
        };
        let state = Arc::new(JobState { ctl: Arc::new(Control::default()), answer: Mutex::new(None) });
        jobs().lock().insert(id, state.clone());
        let qt = self.qt_thread();
        let label = match kind {
            Kind::Copy => "Copy",
            Kind::Move => "Move",
            Kind::Link => "Make Alias",
            Kind::Duplicate => "Duplicate",
        };
        let t0 = json!({"id": id, "title": title, "state": "running", "totalBytes": 0, "doneBytes": 0, "totalItems": 0, "doneItems": 0, "current": ""}).to_string();
        self.as_mut().job_changed(id, qs(&t0));
        std::thread::Builder::new()
            .name(format!("grove-job-{id}"))
            .spawn(move || {
                let host = JobHost { id, title: title.clone(), qt: qt.clone(), state: state.clone() };
                let spec = Spec { kind, sources: sources.clone(), dest: dest.clone() };
                let out = ops::run(&spec, &state.ctl, &host);
                if let Some(a) = out.undo {
                    ffi::Grove::push_history(&qt, label, a);
                }
                let mut dirs: Vec<String> = vec![dest.to_string_lossy().into_owned()];
                if kind == Kind::Move || kind == Kind::Duplicate {
                    for s in &sources {
                        if let Some(p) = s.parent() {
                            dirs.push(p.to_string_lossy().into_owned());
                        }
                    }
                }
                ffi::Grove::touched(&qt, dirs);
                let results: Vec<String> = out.results.iter().map(|p| p.to_string_lossy().into_owned()).collect();
                let done = json!({"id": id, "title": title, "state": if out.cancelled {"cancelled"} else if out.errors.is_empty() {"done"} else {"failed"},
                    "errors": out.errors, "results": results, "dest": dest.to_string_lossy()});
                let s = done.to_string();
                jobs().lock().remove(&id);
                let _ = qt.queue(move |g| g.job_changed(id, qs(&s)));
            })
            .ok();
        id
    }

    pub fn answer_conflict(&self, id: i32, resolution: &QString, apply_all: bool) {
        let r = Resolution::from_str(&resolution.to_string());
        if let Some(j) = jobs().lock().get(&id) {
            if let Some(tx) = j.answer.lock().take() {
                let _ = tx.send((r, apply_all));
            }
        }
    }

    pub fn pause_job(&self, id: i32, paused: bool) {
        if let Some(j) = jobs().lock().get(&id) {
            if paused {
                j.ctl.pause()
            } else {
                j.ctl.resume()
            }
        }
    }

    pub fn cancel_job(&self, id: i32) {
        if let Some(j) = jobs().lock().get(&id) {
            j.ctl.cancel();
            if let Some(tx) = j.answer.lock().take() {
                let _ = tx.send((Resolution::Cancel, false));
            }
        }
    }

    pub fn trash_paths(&self, key: &QString, paths: &QString) {
        let paths = paths_of(paths);
        let qt = self.qt_thread();
        self.answer_on("files", key, move || {
            let mut items = Vec::new();
            let mut errors = Vec::new();
            for p in &paths {
                match trash::trash(p) {
                    Ok(it) => items.push(it),
                    Err(e) => errors.push(e),
                }
            }
            let n = items.len();
            if n > 0 {
                ffi::Grove::push_history(&qt, "Move to Trash", Action::Trashed { items });
            }
            ffi::Grove::touched(&qt, paths.iter().filter_map(|p| p.parent()).map(|p| p.to_string_lossy().into_owned()).collect());
            json!({"trashed": n, "errors": errors})
        });
    }

    pub fn delete_paths(&self, key: &QString, paths: &QString) {
        let paths = paths_of(paths);
        let qt = self.qt_thread();
        self.answer_on("files", key, move || {
            let mut errors = Vec::new();
            for p in &paths {
                if let Err(e) = ops::remove_tree(p) {
                    errors.push(util::plain_error(&e, p));
                }
            }
            ffi::Grove::touched(&qt, paths.iter().filter_map(|p| p.parent()).map(|p| p.to_string_lossy().into_owned()).collect());
            json!({"errors": errors})
        });
    }

    pub fn restore_trash(&self, key: &QString, items: &QString) {
        let list: Vec<String> = serde_json::from_str(&items.to_string()).unwrap_or_default();
        let qt = self.qt_thread();
        self.answer_on("files", key, move || {
            let mut errors = Vec::new();
            let mut restored = Vec::new();
            for k in list {
                let Ok(it) = serde_json::from_str::<trash::TrashItem>(&k) else { continue };
                match trash::restore(&it, None) {
                    Ok(p) => restored.push(p),
                    Err(e) => errors.push(e),
                }
            }
            if !restored.is_empty() {
                ffi::Grove::push_history(&qt, "Put Back", Action::Exists { paths: restored.clone() });
            }
            json!({"restored": restored, "errors": errors})
        });
    }

    pub fn empty_trash(&self, key: &QString) {
        self.answer_on("files", key, || {
            let (n, errors) = trash::empty();
            json!({"removed": n, "errors": errors})
        });
    }

    pub fn trash_count(&self, key: &QString) {
        self.answer(key, || json!({"count": trash::count()}));
    }

    pub fn rename_path(&self, key: &QString, path: &QString, name: &QString) {
        let (p, n) = (PathBuf::from(path.to_string()), name.to_string());
        let qt = self.qt_thread();
        self.answer_on("files", key, move || match ops::rename(&p, &n) {
            Ok(a) => {
                let to = match &a {
                    Action::Moved { pairs } => pairs.first().map(|x| x.1.clone()).unwrap_or(p.clone()),
                    _ => p.clone(),
                };
                ffi::Grove::push_history(&qt, "Rename", a);
                json!({"ok": true, "path": to})
            }
            Err(e) => json!({"error": e}),
        });
    }

    pub fn new_folder(&self, key: &QString, dir: &QString, name: &QString) {
        let d = PathBuf::from(dir.to_string());
        let n = name.to_string();
        let qt = self.qt_thread();
        self.answer_on("files", key, move || match ops::new_folder(&d, if n.is_empty() { None } else { Some(&n) }) {
            Ok((p, a)) => {
                ffi::Grove::push_history(&qt, "New Folder", a);
                json!({"path": p})
            }
            Err(e) => json!({"error": e}),
        });
    }

    pub fn new_file(&self, key: &QString, dir: &QString, name: &QString) {
        let d = PathBuf::from(dir.to_string());
        let n = name.to_string();
        let qt = self.qt_thread();
        self.answer_on("files", key, move || {
            // A template from ~/Templates when the name is one of them.
            let tpl = templates_dir().join(&n);
            let (name, template) = if tpl.is_file() { (n.clone(), Some(tpl)) } else { (if n.is_empty() { "untitled.txt".to_string() } else { n.clone() }, None) };
            match ops::new_file(&d, &name, template.as_deref()) {
                Ok((p, a)) => {
                    ffi::Grove::push_history(&qt, "New File", a);
                    json!({"path": p})
                }
                Err(e) => json!({"error": e}),
            }
        });
    }

    pub fn templates(&self, key: &QString) {
        self.answer(key, || {
            let mut v: Vec<String> = std::fs::read_dir(templates_dir()).map(|r| r.flatten().filter(|e| e.path().is_file()).map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
            v.sort();
            json!(v)
        });
    }

    pub fn undo(&self) {
        let qt = self.qt_thread();
        lane("files", move || {
            let r = history().lock().take_undo();
            let Some((label, a)) = r else { return };
            let msg = match a.invert() {
                Ok(inv) => {
                    history().lock().pushed_undone(label.clone(), inv);
                    None
                }
                Err(e) => Some(format!("Couldn't undo {label}: {e}")),
            };
            let _ = qt.queue(move |mut g| {
                g.as_mut().refresh_history();
                g.as_mut().folders_touched(qs("[\"*\"]"));
                if let Some(m) = msg {
                    g.notify(qs("error"), qs(&m));
                }
            });
        });
    }

    pub fn redo(&self) {
        let qt = self.qt_thread();
        lane("files", move || {
            let r = history().lock().take_redo();
            let Some((label, a)) = r else { return };
            let msg = match a.invert() {
                Ok(inv) => {
                    history().lock().pushed_redone(label.clone(), inv);
                    None
                }
                Err(e) => Some(format!("Couldn't redo {label}: {e}")),
            };
            let _ = qt.queue(move |mut g| {
                g.as_mut().refresh_history();
                g.as_mut().folders_touched(qs("[\"*\"]"));
                if let Some(m) = msg {
                    g.notify(qs("error"), qs(&m));
                }
            });
        });
    }

    pub fn copy_to_clipboard(&self, paths: &QString, cut: bool) {
        let p: Vec<String> = serde_json::from_str(&paths.to_string()).unwrap_or_default();
        ffi::grove_set_clipboard(&qs(&p.join("\n")), cut);
    }

    pub fn copy_text(&self, text: &QString) {
        ffi::grove_set_clipboard_text(text);
    }

    pub fn clipboard(&self) -> QString {
        ffi::grove_clipboard()
    }

    pub fn paste_into(mut self: Pin<&mut Self>, dest: &QString) -> i32 {
        let c: Value = serde_json::from_str(&ffi::grove_clipboard().to_string()).unwrap_or(json!({}));
        let paths = c["paths"].clone();
        if paths.as_array().map(|a| a.is_empty()).unwrap_or(true) {
            return -1;
        }
        let cut = c["cut"].as_bool().unwrap_or(false);
        let id = self.as_mut().run_job(&qs(if cut { "move" } else { "copy" }), &qs(&paths.to_string()), dest);
        if cut {
            // A cut is used up once pasted.
            ffi::grove_set_clipboard(&qs(""), false);
        }
        id
    }

    pub fn rename_preview(&self, key: &QString, paths: &QString, rule: &QString) {
        let paths = paths_of(paths);
        let rule: grove_core::rename::Rule = serde_json::from_str(&rule.to_string()).unwrap_or_default();
        self.answer(key, move || json!(grove_core::rename::plan(&rule, &paths)));
    }

    pub fn rename_apply(&self, key: &QString, paths: &QString, rule: &QString) {
        let paths = paths_of(paths);
        let rule: grove_core::rename::Rule = serde_json::from_str(&rule.to_string()).unwrap_or_default();
        let qt = self.qt_thread();
        self.answer_on("files", key, move || {
            let plan = grove_core::rename::plan(&rule, &paths);
            match grove_core::rename::execute(&plan) {
                Ok(a) => {
                    let to: Vec<String> = match &a {
                        Action::Moved { pairs } => pairs.iter().map(|p| p.1.to_string_lossy().into_owned()).collect(),
                        _ => vec![],
                    };
                    ffi::Grove::push_history(&qt, "Rename", a);
                    json!({"ok": true, "paths": to})
                }
                Err(e) => json!({"error": e}),
            }
        });
    }

    // ---------------- tags, info ----------------

    pub fn toggle_tag(&self, key: &QString, paths: &QString, tag: &QString) {
        let paths = paths_of(paths);
        let t = tag.to_string();
        self.answer_on("files", key, move || {
            tags::remember(&t);
            let refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
            match tags::toggle(&refs, &t) {
                Ok(on) => json!({"on": on}),
                Err(e) => json!({"error": e}),
            }
        });
    }

    pub fn set_tags(&self, key: &QString, path: &QString, tags_json: &QString) {
        let p = PathBuf::from(path.to_string());
        let list: Vec<String> = serde_json::from_str(&tags_json.to_string()).unwrap_or_default();
        self.answer_on("files", key, move || {
            for t in &list {
                tags::remember(t);
            }
            match tags::set(&p, &list) {
                Ok(()) => json!({"ok": true}),
                Err(e) => json!({"error": e}),
            }
        });
    }

    pub fn known_tags(&self, key: &QString) {
        self.answer(key, || {
            let v: Vec<Value> = tags::known().into_iter().map(|t| json!({"name": t, "color": tags::color_of(&t).unwrap_or("")})).collect();
            json!(v)
        });
    }

    pub fn info(&self, key: &QString, path: &QString) {
        let p = path.to_string();
        self.answer(key, move || crate::info::info(&p));
    }

    pub fn folder_size(&self, key: &QString, path: &QString) {
        let p = PathBuf::from(path.to_string());
        let key = key.to_string();
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let cancel = AtomicBool::new(false);
            let (k1, q1) = (key.clone(), qt.clone());
            let (b, n) = grove_core::listing::tree_size(&p, &cancel, move |b, n| {
                let k = k1.clone();
                let _ = q1.queue(move |g| g.size_ready(qs(&k), b as f64, n as f64, false));
            });
            let _ = qt.queue(move |g| g.size_ready(qs(&key), b as f64, n as f64, true));
            let _ = cancel.load(Ordering::Relaxed);
        });
    }

    pub fn set_mode(&self, key: &QString, path: &QString, mode: i32, recursive: bool) {
        let p = path.to_string();
        self.answer_on("files", key, move || crate::info::set_mode(&p, mode as u32, recursive));
    }

    pub fn set_owner(&self, key: &QString, path: &QString, owner: &QString, group: &QString, recursive: bool) {
        let (p, o, g) = (path.to_string(), owner.to_string(), group.to_string());
        self.answer_on("files", key, move || crate::info::set_owner(&p, &o, &g, recursive));
    }

    pub fn set_comment(&self, key: &QString, path: &QString, text: &QString) {
        let (p, t) = (PathBuf::from(path.to_string()), text.to_string());
        self.answer_on("files", key, move || match tags::set_comment(&p, &t) {
            Ok(()) => json!({"ok": true}),
            Err(e) => json!({"error": e}),
        });
    }

    // ---------------- places, settings ----------------

    pub fn places(&self, key: &QString) {
        let favs = self.rust().settings.favourites.clone();
        self.answer(key, move || crate::places::places(&favs));
    }

    fn save_settings(mut self: Pin<&mut Self>) {
        let s = self.rust().settings.clone();
        let j = serde_json::to_string(&s).unwrap();
        self.as_mut().set_settings_json(qs(&j));
        lane("settings", move || {
            let _ = settings::save(&s);
        });
    }

    pub fn add_favourite(mut self: Pin<&mut Self>, path: &QString, index: i32) {
        let p = path.to_string();
        {
            let mut r = self.as_mut().rust_mut();
            r.settings.favourites.retain(|x| x != &p);
            let i = if index < 0 { r.settings.favourites.len() } else { (index as usize).min(r.settings.favourites.len()) };
            r.settings.favourites.insert(i, p);
        }
        self.as_mut().save_settings();
        let r = *self.places_revision() + 1;
        self.set_places_revision(r);
    }

    pub fn remove_favourite(mut self: Pin<&mut Self>, path: &QString) {
        let p = path.to_string();
        self.as_mut().rust_mut().settings.favourites.retain(|x| x != &p);
        self.as_mut().save_settings();
        let r = *self.places_revision() + 1;
        self.set_places_revision(r);
    }

    pub fn move_favourite(mut self: Pin<&mut Self>, from: i32, to: i32) {
        {
            let mut r = self.as_mut().rust_mut();
            let n = r.settings.favourites.len() as i32;
            if from < 0 || from >= n || to < 0 || to >= n {
                return;
            }
            let x = r.settings.favourites.remove(from as usize);
            r.settings.favourites.insert(to as usize, x);
        }
        self.as_mut().save_settings();
        let r = *self.places_revision() + 1;
        self.set_places_revision(r);
    }

    pub fn eject(&self, key: &QString, mount: &QString) {
        let m = mount.to_string();
        self.answer(key, move || crate::places::eject(&m));
    }

    pub fn mount_device(&self, key: &QString, device: &QString) {
        let d = device.to_string();
        self.answer(key, move || crate::places::mount(&d));
    }

    pub fn set_setting(mut self: Pin<&mut Self>, name: &QString, value_json: &QString) {
        let mut v = serde_json::to_value(&self.rust().settings).unwrap();
        let val: Value = serde_json::from_str(&value_json.to_string()).unwrap_or(Value::Null);
        v[name.to_string()] = val;
        match serde_json::from_value::<settings::Settings>(v) {
            Ok(s) => {
                self.as_mut().rust_mut().settings = s;
                self.save_settings();
            }
            Err(e) => eprintln!("grove: setting {name} not saved: {e}"),
        }
    }

    pub fn folder_view(&self, path: &QString) -> QString {
        match self.rust().views.get(&path.to_string()) {
            Some(v) => qs(&serde_json::to_string(v).unwrap()),
            None => qs("{}"),
        }
    }

    pub fn set_folder_view(mut self: Pin<&mut Self>, path: &QString, json_s: &QString) {
        let Ok(v) = serde_json::from_str::<settings::FolderView>(&json_s.to_string()) else { return };
        self.as_mut().rust_mut().views.insert(path.to_string(), v);
        let all = self.rust().views.clone();
        lane("settings", move || settings::save_folder_views(&all));
    }

    pub fn complete_path(&self, key: &QString, text: &QString) {
        let t = text.to_string();
        self.answer(key, move || {
            let t = if let Some(rest) = t.strip_prefix('~') { format!("{}{rest}", util::home().display()) } else { t };
            let (dir, prefix) = match t.rfind('/') {
                Some(i) => (t[..=i].to_string(), t[i + 1..].to_lowercase()),
                None => return json!([]),
            };
            let mut v: Vec<String> = std::fs::read_dir(&dir)
                .map(|r| {
                    r.flatten()
                        .filter(|e| e.path().is_dir())
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .filter(|n| n.to_lowercase().starts_with(&prefix) && (prefix.starts_with('.') || !n.starts_with('.')))
                        .map(|n| format!("{dir}{n}"))
                        .collect()
                })
                .unwrap_or_default();
            v.sort_by(|a, b| util::natural_cmp(a, b));
            v.truncate(50);
            json!(v)
        });
    }

    pub fn preview(&self, key: &QString, path: &QString, dark: bool) {
        let p = path.to_string();
        self.answer(key, move || {
            let t = std::time::Instant::now();
            let v = crate::preview::preview(&p, dark);
            if std::env::var_os("GROVE_THUMB_LOG").is_some() {
                eprintln!("grove quick look {} ms {}", t.elapsed().as_millis(), p);
            }
            v
        });
    }

    pub fn office_pdf(&self, key: &QString, path: &QString) {
        let p = path.to_string();
        // One lane: LibreOffice converts one document at a time anyway.
        self.answer_on("office", key, move || crate::preview::office_pdf(&p));
    }

    pub fn add_recent(&self, path: &QString) {
        let p = PathBuf::from(path.to_string());
        lane("settings", move || {
            let m = crate::mime::deep(&p.to_string_lossy());
            grove_core::recents::add(&p, &m.name);
        });
    }

    pub fn save_smart_folder(mut self: Pin<&mut Self>, name: &QString, root: &QString, query: &QString) {
        let f = grove_core::search::SmartFolder { name: name.to_string(), root: root.to_string(), query: serde_json::from_str(&query.to_string()).unwrap_or_default() };
        let qt = self.qt_thread();
        lane("settings", move || {
            let _ = grove_core::search::save_smart_folder(f);
            let _ = qt.queue(|mut g| {
                let r = *g.places_revision() + 1;
                g.as_mut().set_places_revision(r);
            });
        });
        let _ = &mut self;
    }

    pub fn remove_smart_folder(&self, name: &QString) {
        let n = name.to_string();
        let qt = self.qt_thread();
        lane("settings", move || {
            let _ = grove_core::search::remove_smart_folder(&n);
            let _ = qt.queue(|mut g| {
                let r = *g.places_revision() + 1;
                g.as_mut().set_places_revision(r);
            });
        });
    }

    pub fn git_status(&self, key: &QString, dir: &QString) {
        let d = dir.to_string();
        self.answer(key, move || crate::vcs::status(&d));
    }

    pub fn quick_action(&self, key: &QString, action: &QString, paths: &QString, arg: &QString) {
        let (a, ps, arg) = (action.to_string(), paths_of(paths), arg.to_string());
        let qt = self.qt_thread();
        self.answer_on("files", key, move || crate::actions::quick(&qt, &a, &ps, &arg));
    }

    pub fn service_menus(&self, key: &QString, paths: &QString) {
        let ps = paths_of(paths);
        self.answer(key, move || crate::actions::service_menus(&ps));
    }

    pub fn run_service_menu(&self, exec: &QString, paths: &QString) {
        let (e, ps) = (exec.to_string(), paths_of(paths));
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            if let Err(err) = crate::actions::run_service_menu(&e, &ps) {
                let _ = qt.queue(move |g| g.notify(qs("error"), qs(&err)));
            }
        });
    }

    // ---------------- remote places, archives ----------------

    pub fn connect_server(&self, key: &QString, json_s: &QString) {
        let j = json_s.to_string();
        let qt = self.qt_thread();
        self.answer(key, move || {
            let r = crate::remote::connect(&j);
            let _ = qt.queue(|mut g| {
                let r = *g.places_revision() + 1;
                g.as_mut().set_places_revision(r);
            });
            r
        });
    }

    pub fn forget_server(&self, key: &QString, name: &QString) {
        let n = name.to_string();
        let qt = self.qt_thread();
        self.answer(key, move || {
            let r = crate::remote::forget(&n);
            let _ = qt.queue(|mut g| {
                let r = *g.places_revision() + 1;
                g.as_mut().set_places_revision(r);
            });
            r
        });
    }

    pub fn discover_network(&self, key: &QString) {
        self.answer(key, crate::remote::discover);
    }

    pub fn rclone_providers(&self, key: &QString) {
        self.answer(key, crate::remote::rclone_providers);
    }
    pub fn rclone_drives(&self, key: &QString) {
        self.answer(key, crate::remote::rclone_drives);
    }
    pub fn rclone_step(&self, key: &QString, json_s: &QString) {
        let j = json_s.to_string();
        let qt = self.qt_thread();
        self.answer(key, move || {
            let r = crate::remote::rclone_step(&j);
            if r["done"].as_bool() == Some(true) {
                let _ = qt.queue(|mut g| {
                    let r = *g.places_revision() + 1;
                    g.as_mut().set_places_revision(r);
                });
            }
            r
        });
    }
    pub fn rclone_delete(&self, key: &QString, name: &QString) {
        let n = name.to_string();
        let qt = self.qt_thread();
        self.answer(key, move || {
            let r = crate::remote::rclone_delete(&n);
            let _ = qt.queue(|mut g| {
                let r = *g.places_revision() + 1;
                g.as_mut().set_places_revision(r);
            });
            r
        });
    }
    pub fn rclone_cancel(&self) {
        crate::remote::rclone_cancel()
    }

    pub fn du_start(&self, path: &QString) -> i32 {
        crate::du::start(&path.to_string())
    }
    pub fn du_progress(&self, scan: i32) -> QString {
        qs(&crate::du::progress(scan).to_string())
    }
    pub fn du_cancel(&self, scan: i32) {
        crate::du::cancel(scan)
    }
    pub fn du_forget(&self, scan: i32) {
        crate::du::forget(scan)
    }
    pub fn du_rows(&self, key: &QString, scan: i32, expanded: &QString, apparent: bool, sort: &QString) {
        let (e, s) = (expanded.to_string(), sort.to_string());
        self.answer(key, move || crate::du::rows(scan, &e, apparent, &s));
    }
    pub fn du_types(&self, key: &QString, scan: i32, node: i32, apparent: bool) {
        self.answer(key, move || crate::du::types(scan, node.max(0) as u32, apparent));
    }
    pub fn du_path(&self, scan: i32, node: i32) -> QString {
        qs(&crate::du::path_of(scan, node.max(0) as u32))
    }
    pub fn du_ancestors(&self, scan: i32, node: i32) -> QString {
        qs(&crate::du::ancestors(scan, node.max(0) as u32).to_string())
    }
    pub fn du_parent(&self, scan: i32, node: i32) -> i32 {
        crate::du::parent_of(scan, node.max(0) as u32) as i32
    }
    pub fn du_find(&self, scan: i32, path: &QString) -> i32 {
        crate::du::find(scan, &path.to_string()) as i32
    }
    pub fn du_removed(&self, scan: i32, node: i32) {
        if node > 0 {
            crate::du::removed(scan, node as u32)
        }
    }
    pub fn du_hit(&self, scan: i32, x: f64, y: f64) -> i32 {
        crate::du::hit(scan, x, y) as i32
    }
    pub fn du_rect(&self, scan: i32, node: i32) -> QString {
        qs(&crate::du::rect(scan, node.max(0) as u32).to_string())
    }

    pub fn doc_pdf(&self, key: &QString, path: &QString) {
        let p = path.to_string();
        self.answer(key, move || match crate::docconv::to_pdf(&p) {
            Ok(pdf) => crate::preview::pdf_json(&pdf, &std::sync::atomic::AtomicBool::new(false)),
            Err(e) => serde_json::json!({"error": e}),
        });
    }

    pub fn gdoc_export(&self, key: &QString, path: &QString) {
        let p = PathBuf::from(path.to_string());
        let settings = self.rust().settings.clone();
        self.answer(key, move || match crate::remote::gdoc_export(&p, &settings) {
            Ok(f) => json!({"path": f.display().to_string()}),
            Err(e) => e,
        });
    }

    pub fn google_status(&self) -> QString {
        qs(&crate::google::status(&self.rust().settings).to_string())
    }

    pub fn google_sign_in(&self, key: &QString) {
        let settings = self.rust().settings.clone();
        let qt = self.qt_thread();
        self.answer(key, move || match crate::google::sign_in(&settings) {
            Ok(email) => {
                let e = email.clone();
                // Remember the account (its token is in the keyring).
                let _ = qt.queue(move |mut g| {
                    if !g.rust().settings.google_accounts.contains(&e) {
                        let mut s = g.rust().settings.clone();
                        s.google_accounts.push(e);
                        g.as_mut().rust_mut().settings = s;
                        g.save_settings();
                    }
                });
                json!({"email": email})
            }
            Err(e) => json!({"error": e}),
        });
    }

    pub fn google_sign_out(mut self: Pin<&mut Self>, email: &QString) {
        let e = email.to_string();
        crate::google::sign_out(&e);
        let mut s = self.rust().settings.clone();
        s.google_accounts.retain(|a| a != &e);
        self.as_mut().rust_mut().settings = s;
        self.save_settings();
    }

    pub fn accounts_available(&self) -> bool {
        crate::accounts::available()
    }

    pub fn accounts_open_settings(&self, key: &QString, account: &QString) {
        let a = account.to_string();
        self.answer(key, move || json!({"ok": crate::accounts::open_settings(&a)}));
    }

    pub fn accounts_sign_in(&self, key: &QString, provider: &QString, account: &QString) {
        let (p, a) = (provider.to_string(), account.to_string());
        self.answer(key, move || match crate::accounts::sign_in(&p, &a) {
            Ok(s) => json!({"session": s}),
            Err(e) => json!({"error": e.message()}),
        });
    }

    pub fn open_url(&self, url: &QString) {
        let u = url.to_string();
        // Tests (screenshot runs) don't open the real browser.
        if std::env::var_os("GROVE_SCREENSHOT").is_some() {
            eprintln!("grove test openUrl {u}");
            return;
        }
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            if let Err(e) = apps::open_url(&u) {
                let _ = qt.queue(move |g| g.notify(qs("error"), qs(&e)));
            }
        });
    }

    pub fn audio_waveform(&self, key: &QString, path: &QString, width: i32, height: i32, dark: bool) {
        let p = path.to_string();
        let (w, h) = (width.clamp(64, 4096) as u32, height.clamp(16, 512) as u32);
        let (base, played) = if dark { ("0x6b7280", "0x60a5fa") } else { ("0xb6bcc6", "0x2563eb") };
        self.answer(key, move || {
            let never = std::sync::atomic::AtomicBool::new(false);
            match grove_core::previews::player_waveform(std::path::Path::new(&p), w, h, base, played, &never) {
                Some((a, b)) => serde_json::json!({"base": a.display().to_string(), "played": b.display().to_string()}),
                None => serde_json::json!({"error": "no waveform"}),
            }
        });
    }

    pub fn archive_local(&self, key: &QString, path: &QString) {
        let p = path.to_string();
        self.answer(key, move || match crate::archive::local_copy(&p, true) {
            Some(l) => serde_json::json!({"path": l.display().to_string()}),
            None => serde_json::json!({"error": "Couldn't take it out of the archive."}),
        });
    }

    pub fn archive_sizes(&self, key: &QString, archive: &QString) {
        let a = archive.to_string();
        self.answer(key, move || crate::archive::sizes_json(&a));
    }

    pub fn archive_list(&self, key: &QString, archive: &QString) {
        let a = archive.to_string();
        self.answer(key, move || crate::archive::list_json(&a));
    }

    pub fn extract(mut self: Pin<&mut Self>, archive: &QString, dest: &QString) -> i32 {
        let id = self.rust().next_job;
        self.as_mut().rust_mut().next_job += 1;
        crate::archive::extract_job(self.qt_thread(), id, archive.to_string(), dest.to_string());
        id
    }

    pub fn compress(mut self: Pin<&mut Self>, paths: &QString, format: &QString) -> i32 {
        let id = self.rust().next_job;
        self.as_mut().rust_mut().next_job += 1;
        crate::archive::compress_job(self.qt_thread(), id, paths_of(paths), format.to_string());
        id
    }

    pub fn chooser_done(&self, id: &QString, ok: bool, uris: &QString, choices: &QString) {
        crate::portal::answer(&id.to_string(), ok, &uris.to_string(), &choices.to_string());
    }

    pub fn test_touchpad(&self, x: f64, y: f64, ax: i32, ay: i32, px: i32, py: i32, phase: i32) {
        ffi::grove_test_touchpad(x, y, ax, ay, px, py, phase);
    }

    pub fn test_wheel_x(&self, x: f64, y: f64, angle_x: i32) {
        ffi::grove_test_wheel_x(x, y, angle_x);
    }

    pub fn test_wheel(&self, x: f64, y: f64, angle_y: i32, pixel_y: i32) {
        ffi::grove_test_wheel(x, y, angle_y, pixel_y);
    }

    pub fn test_key(&self, key: i32, modifiers: i32, text: &QString) {
        ffi::grove_test_key(key, modifiers, text);
    }

    pub fn write_text(&self, path: &QString, text: &QString) {
        let (p, t) = (path.to_string(), text.to_string());
        lane("files", move || {
            let _ = std::fs::write(p, t);
        });
    }

    pub fn checksum(&self, key: &QString, path: &QString, algo: &QString) {
        let (p, a) = (PathBuf::from(path.to_string()), algo.to_string());
        self.answer(key, move || {
            let c = AtomicBool::new(false);
            match grove_core::checksum::compute(&p, &a, &c) {
                Ok(sum) => json!({"algo": a, "sum": sum}),
                Err(e) => json!({"error": e}),
            }
        });
    }

    pub fn versions(&self, key: &QString, path: &QString) {
        let p = PathBuf::from(path.to_string());
        self.answer(key, move || json!(grove_core::versions::versions(&p)));
    }

    pub fn git_action(&self, key: &QString, dir: &QString, action: &QString, paths: &QString, arg: &QString) {
        let (d, a, ps, arg) = (dir.to_string(), action.to_string(), paths_of(paths), arg.to_string());
        let dark = true;
        self.answer(key, move || crate::gitstatus::action(&d, &a, &ps, &arg, dark));
    }

    pub fn compare_files(&self, key: &QString, a: &QString, b: &QString, dark: bool) {
        let (a, b) = (a.to_string(), b.to_string());
        self.answer(key, move || {
            let tool = ["meld", "kompare", "kdiff3", "diffuse"].into_iter().find(|t| grove_core::thumbs::which(t).is_some());
            match crate::compare::compare(Path::new(&a), Path::new(&b)) {
                Ok((same, text)) => json!({"same": same, "text": text, "html": if same { String::new() } else { crate::preview::highlight(&text, "x.diff", dark) }, "tool": tool}),
                Err(e) => json!({"error": format!("Couldn't compare: {e}")}),
            }
        });
    }

    pub fn mount_image(&self, key: &QString, path: &QString) {
        let p = path.to_string();
        let qt = self.qt_thread();
        self.answer(key, move || {
            match crate::places::mount_image(&p) {
                Ok((at, d)) => {
                    let _ = qt.queue(|mut g| {
                        let r = *g.places_revision() + 1;
                        g.as_mut().set_places_revision(r);
                    });
                    json!({"ok": true, "path": at, "device": d})
                }
                Err(e) => json!({"error": e}),
            }
        });
    }

    pub fn set_rating(&self, key: &QString, paths: &QString, rating: i32) {
        let ps = paths_of(paths);
        self.answer_on("files", key, move || {
            let mut errors = vec![];
            for p in &ps {
                if let Err(e) = tags::set_rating(p, rating.clamp(0, 10) as u8) {
                    errors.push(e);
                }
            }
            json!({"errors": errors})
        });
    }

    pub fn set_folder_icon(&self, key: &QString, dir: &QString, icon: &QString) {
        let (d, i) = (PathBuf::from(dir.to_string()), icon.to_string());
        self.answer_on("files", key, move || match crate::mime::set_custom_folder_icon(&d, &i) {
            Ok(()) => json!({"ok": true}),
            Err(e) => json!({"error": e}),
        });
    }

    pub fn nearby_devices(&self, key: &QString) {
        self.answer(key, || match crate::localsend::discover(std::time::Duration::from_millis(3000)) {
            Ok(d) => json!({"devices": d}),
            Err(e) => json!({"error": e}),
        });
    }

    pub fn share_localsend(mut self: Pin<&mut Self>, device: &QString, paths: &QString) -> i32 {
        let Ok(dev) = serde_json::from_str::<Value>(&device.to_string()) else { return -1 };
        let dev = crate::localsend::Device {
            alias: dev["alias"].as_str().unwrap_or("").into(),
            ip: dev["ip"].as_str().unwrap_or("").into(),
            port: dev["port"].as_u64().unwrap_or(53317) as u16,
            protocol: dev["protocol"].as_str().unwrap_or("https").into(),
            device_type: String::new(),
            device_model: String::new(),
            fingerprint: dev["fingerprint"].as_str().unwrap_or("").into(),
        };
        let files = paths_of(paths);
        let id = self.rust().next_job;
        self.as_mut().rust_mut().next_job += 1;
        let state = Arc::new(JobState { ctl: Arc::new(Control::default()), answer: Mutex::new(None) });
        jobs().lock().insert(id, state.clone());
        let qt = self.qt_thread();
        let title = format!("Sending {} to “{}”", if files.len() == 1 { files[0].file_name().map(|n| format!("“{}”", n.to_string_lossy())).unwrap_or_default() } else { format!("{} files", files.len()) }, dev.alias);
        std::thread::spawn(move || {
            let send = |v: Value| {
                let s = v.to_string();
                let _ = qt.queue(move |g| g.job_changed(id, qs(&s)));
            };
            send(json!({"id": id, "title": title, "state": "running", "totalBytes": 1, "doneBytes": 0, "current": "Waiting for the other device to accept…"}));
            let mut last = std::time::Instant::now();
            let cancel = AtomicBool::new(false);
            let ctl = state.ctl.clone();
            let r = crate::localsend::send(&dev, &files, &cancel, |d, t| {
                if ctl.is_cancelled() {
                    cancel.store(true, Ordering::Relaxed);
                }
                if last.elapsed().as_millis() > 150 {
                    last = std::time::Instant::now();
                    send(json!({"id": id, "title": title, "state": "running", "totalBytes": t, "doneBytes": d, "current": ""}));
                }
            });
            jobs().lock().remove(&id);
            match r {
                Ok(()) => send(json!({"id": id, "title": title, "state": "done", "totalBytes": 1, "doneBytes": 1})),
                Err(e) => send(json!({"id": id, "title": title, "state": if e == "Cancelled" {"cancelled"} else {"failed"}, "errors": [e]})),
            }
        });
        id
    }

    pub fn markup_save(&self, key: &QString, path: &QString, ops: &QString, mode: &QString) {
        let (p, ops, mode) = (PathBuf::from(path.to_string()), ops.to_string(), mode.to_string());
        let qt = self.qt_thread();
        self.answer(key, move || {
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            // Formats Qt can write back; others become a PNG copy.
            let ext = if ["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff"].contains(&ext.as_str()) { ext } else { "png".into() };
            save_edit(&p, &mode, "edited", &ext, "Markup", &qt, |dest| {
                let e = ffi::grove_markup_render(&qs(&p.to_string_lossy()), &qs(&ops), &qs(&dest.to_string_lossy())).to_string();
                if e.is_empty() { Ok(()) } else { Err(e) }
            })
        });
    }

    pub fn media_trim(&self, key: &QString, path: &QString, start_ms: f64, end_ms: f64, mode: &QString) {
        let (p, mode) = (PathBuf::from(path.to_string()), mode.to_string());
        let qt = self.qt_thread();
        self.answer(key, move || {
            if end_ms <= start_ms {
                return json!({"error": "The end must come after the start."});
            }
            let ext = p.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "mkv".into());
            save_edit(&p, &mode, "trimmed", &ext, "Trim", &qt, |dest| trim_with_ffmpeg(&p, start_ms, end_ms, dest))
        });
    }

    pub fn free_space(&self, key: &QString, path: &QString) {
        let p = path.to_string();
        self.answer_on("statfs", key, move || {
            let Ok(c) = std::ffi::CString::new(p) else { return json!({}) };
            let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
            if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
                return json!({});
            }
            let f = st.f_frsize as u64;
            json!({"free": st.f_bavail as u64 * f, "total": st.f_blocks as u64 * f})
        });
    }

    pub fn optical_drives(&self, key: &QString) {
        self.answer(key, || json!(crate::burn::drives().into_iter().map(|d| json!({"dev": d.dev, "name": d.name, "canWrite": d.can_write})).collect::<Vec<_>>()));
    }

    pub fn burn(mut self: Pin<&mut Self>, json_s: &QString) -> i32 {
        let Ok(req) = serde_json::from_str::<crate::burn::Request>(&json_s.to_string()) else { return -1 };
        let id = self.rust().next_job;
        self.as_mut().rust_mut().next_job += 1;
        let state = Arc::new(JobState { ctl: Arc::new(Control::default()), answer: Mutex::new(None) });
        jobs().lock().insert(id, state.clone());
        let qt = self.qt_thread();
        let what = if req.paths.len() == 1 { Path::new(&req.paths[0]).file_name().map(|n| format!("“{}”", n.to_string_lossy())).unwrap_or_default() } else { format!("{} items", req.paths.len()) };
        let title = if req.device.is_empty() { format!("Making a disc image of {what}") } else if req.simulate { format!("Test-burning {what}") } else { format!("Burning {what}") };
        std::thread::spawn(move || {
            let send = |v: Value| {
                let s = v.to_string();
                let _ = qt.queue(move |g| g.job_changed(id, qs(&s)));
            };
            if req.dry_run {
                jobs().lock().remove(&id);
                let cmd = crate::burn::layout(&req).map(|l| l.iter().map(|(p, t)| format!("{p} -> {t}")).collect::<Vec<_>>().join(", ")).unwrap_or_else(|e| e);
                send(json!({"id": id, "title": title, "state": "done", "totalBytes": 1, "doneBytes": 1, "current": cmd}));
                notify_any("info", &format!("Dry run: {cmd}"));
                return;
            }
            send(json!({"id": id, "title": title, "state": "running", "indeterminate": true, "totalItems": 0, "doneItems": 0, "current": "Starting…"}));
            let cancel = AtomicBool::new(false);
            let ctl = state.ctl.clone();
            let mut last = std::time::Instant::now();
            let mut frac = 0.0;
            let r = crate::burn::run(&req, &cancel, |f, msg| {
                if ctl.is_cancelled() {
                    cancel.store(true, Ordering::Relaxed);
                }
                if f >= 0.0 {
                    frac = f;
                }
                if last.elapsed().as_millis() > 200 || !msg.is_empty() {
                    last = std::time::Instant::now();
                    send(json!({"id": id, "title": title, "state": "running", "totalBytes": 1000, "doneBytes": (frac * 1000.0) as u64, "current": msg}));
                }
            });
            jobs().lock().remove(&id);
            match r {
                Ok(()) => {
                    if !req.iso.is_empty() {
                        if let Some(d) = Path::new(&req.iso).parent() {
                            ffi::Grove::touched(&qt, vec![d.to_string_lossy().into_owned()]);
                        }
                        ffi::Grove::push_history(&qt, "Make Disc Image", Action::Exists { paths: vec![PathBuf::from(&req.iso)] });
                    }
                    send(json!({"id": id, "title": title, "state": "done", "totalBytes": 1, "doneBytes": 1, "results": if req.iso.is_empty() { json!([]) } else { json!([req.iso]) }, "dest": Path::new(&req.iso).parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default()}));
                }
                Err(e) => send(json!({"id": id, "title": title, "state": if e == "Cancelled" {"cancelled"} else {"failed"}, "errors": [e]})),
            }
        });
        id
    }

    pub fn session(&self) -> QString {
        qs(&startup().session)
    }

    pub fn save_session(&self, json_s: &QString) {
        let s = json_s.to_string();
        lane("settings", move || {
            let _ = util::write_atomic(&session_path(), s.as_bytes());
        });
    }

    pub fn grab_windows(&self, path: &QString) {
        ffi::grove_grab_windows(path);
    }

    pub fn set_icon_theme(&self, dark: bool, mac: bool) {
        ffi::grove_set_icon_theme(dark, mac);
    }

    pub fn quit_app(&self) {
        std::process::exit(0);
    }
}

/// Stream copy first (fast, lossless, cuts at keyframes); re-encode the
/// video when the start isn't on one (FFmpeg linked: grove_core::av).
fn trim_with_ffmpeg(src: &Path, start_ms: f64, end_ms: f64, dest: &Path) -> Result<(), String> {
    let (a, b) = (start_ms / 1000.0, end_ms / 1000.0);
    if grove_core::av::keyframe_at(src, a) {
        if grove_core::av::trim_copy(src, a, b, dest).is_ok() && dest.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            return Ok(());
        }
    }
    grove_core::av::trim_encode(src, a, b, dest).map_err(|e| format!("Couldn't trim it: {e}"))
}

/// "photo (edited).jpg", "photo (edited 2).jpg"... next to `p`.
fn sibling_name(p: &Path, tag: &str, ext: &str) -> PathBuf {
    let dir = p.parent().unwrap_or(Path::new("/"));
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    for n in 1.. {
        let name = if n == 1 { format!("{stem} ({tag}).{ext}") } else { format!("{stem} ({tag} {n}).{ext}") };
        let c = dir.join(name);
        if c.symlink_metadata().is_err() {
            return c;
        }
    }
    unreachable!()
}

/// Write a new version of `orig` with `make(dest)`: next to it ("copy") or
/// in its place ("replace": the original goes to the Trash; Undo puts it
/// back). Answers {path} or {error}.
fn save_edit(orig: &Path, mode: &str, tag: &str, ext: &str, label: &str, qt: &CxxQtThread<ffi::Grove>, make: impl Fn(&Path) -> Result<(), String>) -> Value {
    let same_ext = orig.extension().map(|e| e.to_string_lossy().to_lowercase()) == Some(ext.to_lowercase());
    if mode == "replace" && same_ext {
        let dir = orig.parent().unwrap_or(Path::new("/"));
        let tmp = dir.join(format!(".{}.grove-edit.{ext}", orig.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
        if let Err(e) = make(&tmp) {
            let _ = std::fs::remove_file(&tmp);
            return json!({"error": e});
        }
        let item = match trash::trash(orig) {
            Ok(i) => i,
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return json!({"error": format!("Couldn't move the original to the Trash: {e}")});
            }
        };
        if let Err(e) = std::fs::rename(&tmp, orig) {
            let _ = trash::restore(&item, None);
            return json!({"error": e.to_string()});
        }
        ffi::Grove::push_history(qt, label, Action::Seq(vec![Action::Trashed { items: vec![item] }, Action::Moved { pairs: vec![(tmp, orig.to_path_buf())] }]));
        ffi::Grove::touched(qt, vec![dir.to_string_lossy().into_owned()]);
        return json!({"path": orig.to_string_lossy(), "replaced": true});
    }
    let dest = sibling_name(orig, tag, ext);
    if let Err(e) = make(&dest) {
        let _ = std::fs::remove_file(&dest);
        return json!({"error": e});
    }
    ffi::Grove::push_history(qt, label, Action::Exists { paths: vec![dest.clone()] });
    ffi::Grove::touched(qt, vec![dest.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default()]);
    json!({"path": dest.to_string_lossy(), "replaced": false, "converted": mode == "replace"})
}

pub fn templates_dir() -> PathBuf {
    // XDG_TEMPLATES_DIR from user-dirs.dirs, else ~/Templates.
    let home = util::home();
    if let Ok(t) = std::fs::read_to_string(util::config_home().join("user-dirs.dirs")) {
        for l in t.lines() {
            if let Some(v) = l.strip_prefix("XDG_TEMPLATES_DIR=") {
                let v = v.trim_matches('"').replace("$HOME", &home.to_string_lossy());
                return PathBuf::from(v);
            }
        }
    }
    home.join("Templates")
}

/// Send a notification from any thread.
pub fn notify_any(level: &str, msg: &str) {
    if let Some(m) = QT.get() {
        if let Some(qt) = m.lock().as_ref() {
            let (l, s) = (level.to_string(), msg.to_string());
            let _ = qt.queue(move |g| g.notify(qs(&l), qs(&s)));
        }
    }
}

/// Record an undoable action from any thread.
pub fn push_history_any(label: &str, a: Action) {
    history().lock().push(label, a);
    if let Some(m) = QT.get() {
        if let Some(qt) = m.lock().as_ref() {
            let _ = qt.queue(|g| g.refresh_history());
        }
    }
}
