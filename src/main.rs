//! grove: a Finder-style file manager with Dolphin's power.
//!
//!   grove [PATH|URI ...]     open folders (or a file's folder, selected)
//!   grove --new-window       a window even when grove is already running
//!   grove --select PATH ...  show these items selected in their folders
//!   grove --service          started by D-Bus (FileManager1): no window
//!                            until asked
//!   grove --portal           the xdg-desktop-portal FileChooser backend
//!   grove --screenshot FILE  (tests) grab the first window and quit;
//!                            GROVE_TEST="view=list;ql" runs steps first

mod accounts;
mod actions;
mod archive;
mod archiver;
mod burn;
mod canopy;
mod bridge;
mod controller;
mod mtp;
mod usage;
mod dbus;
mod docconv;
mod du;
mod hexdoc;
mod langguess;
mod gitstatus;
mod info;
mod localsend;
mod media;
mod mpv;
mod mime;
mod model;
mod places;
mod portal;
mod preview;
mod pvcanvas;
mod pvdoc;
mod compare;
mod google;
mod remote;
mod vcs;
mod roothelper;
mod subvol;
mod table;
mod textdoc;
mod secrets;
mod terminal;
mod thumbgen;
mod watch;

use bridge::ffi;
use cxx_qt_lib::QString;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // As root through pkexec: change permissions/owners, nothing else.
    if args.first().map(String::as_str) == Some("--root-helper") {
        std::process::exit(roothelper::run(&args[1..]));
    }
    // LibreOfficeKit makes a PDF in a process of its own (previews::office_pdf).
    if args.first().map(String::as_str) == Some("--office-pdf") && args.len() >= 4 {
        let r = grove_core::docs::office_to_pdf_here(std::path::Path::new(&args[1]), std::path::Path::new(&args[2]), std::path::Path::new(&args[3]));
        if let Err(e) = &r {
            eprintln!("{e}");
        }
        // LibreOffice's own shutdown is slow and can hang: just leave.
        unsafe { libc::_exit(if r.is_ok() { 0 } else { 1 }) };
    }
    roothelper::apply_env_args(&mut args);
    let mut paths: Vec<String> = Vec::new();
    let mut action = "folders";
    let mut new_window = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--new-window" | "-n" => new_window = true,
            "--select" | "-s" => action = "items",
            // Grove Player: play the files in a QuickTime-style window.
            // Its own process (libmpv; a crash or a stuck stream never takes
            // the file manager with it), drawn with OpenGL (mpv's render API).
            "--play" | "-p" => {
                action = "play";
                std::env::set_var("GROVE_NO_DBUS", "1");
                // Its own app in the dock and menu bar (grove-player.desktop).
                std::env::set_var("GROVE_APP_ID", "grove-player");
                std::env::set_var("QSG_RHI_BACKEND", "opengl");
            }
            // Grove Preview (macOS Preview's features): PDFs and pictures.
            // Its own process, never handed to a running grove (one from
            // before Preview existed would drop the request).
            "--preview" | "-P" => {
                action = "preview";
                std::env::set_var("GROVE_NO_DBUS", "1");
                std::env::set_var("GROVE_APP_ID", "grove-preview");
            }
            // Grove Archiver (7-Zip's features, grove's look): browse an
            // archive, extract it beside itself or somewhere picked, add
            // files to a new archive, test it.
            "--archive" | "-a" => action = "archive",
            "--extract-here" => action = "extracthere",
            "--extract-to" => action = "extractto",
            "--compress" => action = "compress",
            "--test-archive" => action = "testarchive",
            "--service" => std::env::set_var("GROVE_SERVICE", "1"),
            "--portal" => std::env::set_var("GROVE_PORTAL", "1"),
            "--screenshot" => {
                if let Some(p) = args.get(i + 1) {
                    let abs = std::env::current_dir().unwrap_or_default().join(p);
                    std::env::set_var("GROVE_SCREENSHOT", abs);
                    std::env::set_var("GROVE_NO_DBUS", "1");
                }
                i += 1;
            }
            "--help" | "-h" => {
                println!("grove [--new-window] [--select] [--play] [PATH|URI ...]\n\nA Finder-style file manager. Press F1 in a window for help and shortcuts.\n--play opens the files in Grove Player.\n--preview opens PDFs and pictures in Grove Preview.\n\nGrove Archiver:\n  --archive [ARCHIVE ...]   browse archives (7-Zip-style window)\n  --extract-here ARCHIVE    extract next to it\n  --extract-to ARCHIVE      pick where and how to extract\n  --compress PATH ...       Add to Archive (format, level, password, volumes)\n  --test-archive ARCHIVE    check an archive for damage");
                return;
            }
            "--version" => {
                println!("grove {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            a if a.starts_with("--") => {}
            // A web address or a disc (Grove Player: https://, dvd://, bd://).
            a if action == "play" && a.contains("://") && !a.starts_with("file://") => paths.push(a.to_string()),
            a => {
                let p = grove_core::util::path_from_uri(a).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| a.to_string());
                let abs = std::env::current_dir().unwrap_or_default().join(&p);
                let abs = std::fs::canonicalize(&abs).unwrap_or(abs);
                // A file: open its folder with it selected.
                if abs.is_file() && action == "folders" {
                    action = "items";
                }
                paths.push(abs.to_string_lossy().into_owned());
            }
        }
        i += 1;
    }
    let portal = std::env::var_os("GROVE_PORTAL").is_some();
    let service = std::env::var_os("GROVE_SERVICE").is_some();
    // Already running? Hand over and exit.
    if !portal && !service {
        let a = if new_window && paths.is_empty() { "window" } else { action };
        if !new_window && dbus::forward(a, &paths) {
            return;
        }
        if new_window && paths.is_empty() && dbus::forward("window", &[]) {
            return;
        }
    }
    if !paths.is_empty() || action == "archive" || action == "preview" || action == "play" {
        std::env::set_var("GROVE_OPEN", serde_json::json!({"action": action, "paths": paths}).to_string());
    }
    // hearth's light/dark mode and accent, so grove matches the shell.
    let hearth = grove_core::util::config_home().join("hearth/settings.json");
    if let Ok(t) = std::fs::read_to_string(hearth) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
            std::env::set_var("GROVE_HEARTH", v["appearance"].to_string());
        }
    }
    controller::init_startup();
    std::env::set_var("QT_QUICK_CONTROLS_STYLE", "Basic");
    // Its own Qt's plugins first (a desktop QT_PLUGIN_PATH from another Qt
    // build would stop the window from opening).
    if let Some(p) = option_env!("GROVE_QT_PLUGINS").filter(|p| std::path::Path::new(p).is_dir()) {
        let old = std::env::var("QT_PLUGIN_PATH").unwrap_or_default();
        std::env::set_var("QT_PLUGIN_PATH", if old.is_empty() { p.to_string() } else { format!("{p}:{old}") });
    }
    ffi::grove_create_app();
    let import = option_env!("GROVE_QT_QML").filter(|p| std::path::Path::new(p).exists()).unwrap_or("");
    let code = ffi::grove_run(&QString::from("qrc:/qt/qml/Grove/qml/Main.qml"), &QString::from(import));
    std::process::exit(code);
}
