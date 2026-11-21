//! The Rust <-> C++/QML boundary: the `Grove` controller QML talks to,
//! the C++ helpers Rust calls, and the Rust functions the C++ thumbnail
//! provider calls.
//!
//! Every invokable that touches the disk, a process or the network returns
//! at once and answers later through a signal (`async_reply(key, json)`;
//! qml/Async.qml matches the key to its callback). The UI thread never
//! waits.

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "C++" {
        include!("native_helpers.h");
        fn grove_create_app();
        fn grove_run(url: &QString, import_path: &QString) -> i32;
        fn grove_mime_info(name: &QString, path: &QString, deep: bool) -> QString;
        fn grove_mime_parents(mime: &QString) -> QString;
        fn grove_set_clipboard(paths_newline: &QString, cut: bool);
        fn grove_clipboard() -> QString;
        fn grove_set_clipboard_text(text: &QString);
        fn grove_can_read_image(path: &QString) -> bool;
        fn grove_image_rotate(path: &QString, degrees: i32) -> QString;
        fn grove_image_convert(path: &QString, format: &QString) -> QString;
        fn grove_markup_render(src: &QString, ops: &QString, dest: &QString) -> QString;
        fn grove_test_key(key: i32, modifiers: i32, text: &QString);
        fn grove_set_icon_theme(dark: bool, mac: bool);
        fn grove_grab_windows(path: &QString);
    }

    // Called by the C++ thumbnail provider (on its worker threads).
    extern "Rust" {
        fn grove_thumb_lookup(path: &str, px: u32) -> String;
        fn grove_thumb_failed(path: &str) -> bool;
        fn grove_thumb_mark_failed(path: &str);
        fn grove_thumb_flavor(px: u32) -> u32;
        fn grove_thumb_store(path: &str, flavor: u32, png: &[u8], mime: &str);
        fn grove_thumb_external(path: &str, mimes: &str, px: u32) -> String;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, undo_label, cxx_name = "undoLabel")]
        #[qproperty(QString, redo_label, cxx_name = "redoLabel")]
        #[qproperty(i32, places_revision, cxx_name = "placesRevision")]
        #[qproperty(QString, settings_json, cxx_name = "settingsJson")]
        type Grove = super::GroveRust;
    }

    impl cxx_qt::Threading for Grove {}

    #[auto_cxx_name]
    extern "RustQt" {
        /// The answer to a background call (key, JSON).
        #[qsignal]
        fn async_reply(self: Pin<&mut Grove>, key: QString, json: QString);
        /// A job's state changed (JSON: id, title, progress, state...).
        #[qsignal]
        fn job_changed(self: Pin<&mut Grove>, id: i32, json: QString);
        /// A job needs a decision: an item already exists (JSON ConflictInfo).
        #[qsignal]
        fn job_conflict(self: Pin<&mut Grove>, id: i32, json: QString);
        /// Something to tell the user (plain language). level: info, error.
        #[qsignal]
        fn notify(self: Pin<&mut Grove>, level: QString, message: QString);
        /// Open folders/items (D-Bus FileManager1, a second launch): JSON
        /// {action: folders|items|properties, paths: [...]}.
        #[qsignal]
        fn open_request(self: Pin<&mut Grove>, json: QString);
        /// Files changed on disk by a grove action: views of these folders
        /// should refresh (the watcher catches most; this is immediate).
        #[qsignal]
        fn folders_touched(self: Pin<&mut Grove>, json: QString);
        /// Folder size while counting (Get Info).
        #[qsignal]
        fn size_ready(self: Pin<&mut Grove>, key: QString, bytes: f64, items: f64, done: bool);
        /// The portal wants a file chooser (JSON request).
        #[qsignal]
        fn chooser_request(self: Pin<&mut Grove>, json: QString);

        #[qinvokable]
        fn start(self: Pin<&mut Grove>);
        #[qinvokable]
        fn home(self: &Grove) -> QString;
        #[qinvokable]
        fn env(self: &Grove, name: &QString) -> QString;

        // --- opening ---
        #[qinvokable]
        fn open_paths(self: &Grove, paths: &QString);
        #[qinvokable]
        fn open_with(self: &Grove, paths: &QString, app_id: &QString);
        #[qinvokable]
        fn apps_for(self: &Grove, key: &QString, path: &QString);
        #[qinvokable]
        fn all_apps(self: &Grove, key: &QString);
        #[qinvokable]
        fn set_default_app(self: &Grove, key: &QString, mime: &QString, app_id: &QString);
        #[qinvokable]
        fn open_terminal(self: &Grove, dir: &QString);
        #[qinvokable]
        fn run_command(self: &Grove, argv: &QString, cwd: &QString);

        // --- file operations ---
        /// kind: copy, move, link, duplicate. Returns the job id.
        #[qinvokable]
        fn run_job(self: Pin<&mut Grove>, kind: &QString, sources: &QString, dest: &QString) -> i32;
        #[qinvokable]
        fn answer_conflict(self: &Grove, id: i32, resolution: &QString, apply_all: bool);
        #[qinvokable]
        fn pause_job(self: &Grove, id: i32, paused: bool);
        #[qinvokable]
        fn cancel_job(self: &Grove, id: i32);
        #[qinvokable]
        fn trash_paths(self: &Grove, key: &QString, paths: &QString);
        #[qinvokable]
        fn delete_paths(self: &Grove, key: &QString, paths: &QString);
        /// Put back items from the trash (their `key` JSON from the model).
        #[qinvokable]
        fn restore_trash(self: &Grove, key: &QString, items: &QString);
        #[qinvokable]
        fn empty_trash(self: &Grove, key: &QString);
        #[qinvokable]
        fn trash_count(self: &Grove, key: &QString);
        #[qinvokable]
        fn rename_path(self: &Grove, key: &QString, path: &QString, name: &QString);
        #[qinvokable]
        fn new_folder(self: &Grove, key: &QString, dir: &QString, name: &QString);
        #[qinvokable]
        fn new_file(self: &Grove, key: &QString, dir: &QString, name: &QString);
        #[qinvokable]
        fn templates(self: &Grove, key: &QString);
        #[qinvokable]
        fn undo(self: &Grove);
        #[qinvokable]
        fn redo(self: &Grove);
        #[qinvokable]
        fn copy_to_clipboard(self: &Grove, paths: &QString, cut: bool);
        #[qinvokable]
        fn copy_text(self: &Grove, text: &QString);
        /// {"cut": bool, "paths": [...]}
        #[qinvokable]
        fn clipboard(self: &Grove) -> QString;
        /// Paste the clipboard into `dest`; returns the job id (-1: nothing).
        #[qinvokable]
        fn paste_into(self: Pin<&mut Grove>, dest: &QString) -> i32;
        #[qinvokable]
        fn rename_preview(self: &Grove, key: &QString, paths: &QString, rule: &QString);
        #[qinvokable]
        fn rename_apply(self: &Grove, key: &QString, paths: &QString, rule: &QString);

        // --- tags, info ---
        #[qinvokable]
        fn toggle_tag(self: &Grove, key: &QString, paths: &QString, tag: &QString);
        #[qinvokable]
        fn set_tags(self: &Grove, key: &QString, path: &QString, tags: &QString);
        #[qinvokable]
        fn known_tags(self: &Grove, key: &QString);
        #[qinvokable]
        fn info(self: &Grove, key: &QString, path: &QString);
        #[qinvokable]
        fn folder_size(self: &Grove, key: &QString, path: &QString);
        #[qinvokable]
        fn set_mode(self: &Grove, key: &QString, path: &QString, mode: i32, recursive: bool);
        #[qinvokable]
        fn set_owner(self: &Grove, key: &QString, path: &QString, owner: &QString, group: &QString, recursive: bool);
        #[qinvokable]
        fn set_comment(self: &Grove, key: &QString, path: &QString, text: &QString);

        // --- places, settings ---
        #[qinvokable]
        fn places(self: &Grove, key: &QString);
        #[qinvokable]
        fn add_favourite(self: Pin<&mut Grove>, path: &QString, index: i32);
        #[qinvokable]
        fn remove_favourite(self: Pin<&mut Grove>, path: &QString);
        #[qinvokable]
        fn move_favourite(self: Pin<&mut Grove>, from: i32, to: i32);
        #[qinvokable]
        fn eject(self: &Grove, key: &QString, mount: &QString);
        #[qinvokable]
        fn mount_device(self: &Grove, key: &QString, device: &QString);
        #[qinvokable]
        fn set_setting(self: Pin<&mut Grove>, name: &QString, value_json: &QString);
        #[qinvokable]
        fn folder_view(self: &Grove, path: &QString) -> QString;
        #[qinvokable]
        fn set_folder_view(self: Pin<&mut Grove>, path: &QString, json: &QString);
        #[qinvokable]
        fn complete_path(self: &Grove, key: &QString, text: &QString);
        #[qinvokable]
        fn preview(self: &Grove, key: &QString, path: &QString, dark: bool);
        #[qinvokable]
        fn add_recent(self: &Grove, path: &QString);
        #[qinvokable]
        fn save_smart_folder(self: Pin<&mut Grove>, name: &QString, root: &QString, query: &QString);
        #[qinvokable]
        fn remove_smart_folder(self: &Grove, name: &QString);
        #[qinvokable]
        fn git_status(self: &Grove, key: &QString, dir: &QString);
        #[qinvokable]
        fn quick_action(self: &Grove, key: &QString, action: &QString, paths: &QString, arg: &QString);
        #[qinvokable]
        fn service_menus(self: &Grove, key: &QString, paths: &QString);
        #[qinvokable]
        fn run_service_menu(self: &Grove, exec: &QString, paths: &QString);

        // --- Dolphin/Finder extras ---
        #[qinvokable]
        fn checksum(self: &Grove, key: &QString, path: &QString, algo: &QString);
        #[qinvokable]
        fn versions(self: &Grove, key: &QString, path: &QString);
        #[qinvokable]
        fn git_action(self: &Grove, key: &QString, dir: &QString, action: &QString, paths: &QString, arg: &QString);
        #[qinvokable]
        fn compare_files(self: &Grove, key: &QString, a: &QString, b: &QString, dark: bool);
        #[qinvokable]
        fn mount_image(self: &Grove, key: &QString, path: &QString);
        #[qinvokable]
        fn set_rating(self: &Grove, key: &QString, paths: &QString, rating: i32);
        #[qinvokable]
        fn set_folder_icon(self: &Grove, key: &QString, dir: &QString, icon: &QString);
        /// Windows and tabs from last time (JSON), read at start-up.
        #[qinvokable]
        fn session(self: &Grove) -> QString;
        #[qinvokable]
        fn save_session(self: &Grove, json: &QString);

        /// LocalSend devices nearby (JSON list), after ~3 s.
        #[qinvokable]
        fn nearby_devices(self: &Grove, key: &QString);
        /// Send files to a LocalSend device; returns the job id.
        #[qinvokable]
        fn share_localsend(self: Pin<&mut Grove>, device: &QString, paths: &QString) -> i32;

        // --- remote places (phase 2) ---
        #[qinvokable]
        fn connect_server(self: &Grove, key: &QString, json: &QString);
        #[qinvokable]
        fn forget_server(self: &Grove, key: &QString, name: &QString);
        #[qinvokable]
        fn discover_network(self: &Grove, key: &QString);
        #[qinvokable]
        fn archive_list(self: &Grove, key: &QString, archive: &QString);
        #[qinvokable]
        fn extract(self: Pin<&mut Grove>, archive: &QString, dest: &QString) -> i32;
        #[qinvokable]
        fn compress(self: Pin<&mut Grove>, paths: &QString, format: &QString) -> i32;

        // --- Quick Look editing ---
        /// Save markup over a picture. mode: "replace" (the original goes to
        /// the Trash, Undo puts it back) or "copy" (a new file next to it).
        #[qinvokable]
        fn markup_save(self: &Grove, key: &QString, path: &QString, ops: &QString, mode: &QString);
        /// Cut a video/audio file to start..end (ms) with ffmpeg (stream copy
        /// when it can, re-encoding otherwise). mode as for markup_save.
        #[qinvokable]
        fn media_trim(self: &Grove, key: &QString, path: &QString, start_ms: f64, end_ms: f64, mode: &QString);

        // --- portal file chooser ---
        #[qinvokable]
        fn chooser_done(self: &Grove, id: &QString, ok: bool, uris: &QString, choices: &QString);

        // --- tests ---
        #[qinvokable]
        fn test_key(self: &Grove, key: i32, modifiers: i32, text: &QString);
        #[qinvokable]
        fn write_text(self: &Grove, path: &QString, text: &QString);
        #[qinvokable]
        fn quit_app(self: &Grove);
        #[qinvokable]
        fn set_icon_theme(self: &Grove, dark: bool, mac: bool);
        #[qinvokable]
        fn grab_windows(self: &Grove, path: &QString);
    }
}

use crate::controller::GroveRust;

// ---- thumbnails, for the C++ provider ----
use std::path::Path;

fn grove_thumb_lookup(path: &str, px: u32) -> String {
    let p = Path::new(path);
    if grove_core::thumbs::should_skip(p) {
        return path.to_string();
    }
    grove_core::thumbs::lookup(p, px).map(|t| t.to_string_lossy().into_owned()).unwrap_or_default()
}

fn grove_thumb_failed(path: &str) -> bool {
    grove_core::thumbs::failed(Path::new(path))
}

fn grove_thumb_mark_failed(path: &str) {
    grove_core::thumbs::mark_failed(Path::new(path))
}

fn grove_thumb_flavor(px: u32) -> u32 {
    grove_core::thumbs::flavor_for(px).1
}

fn grove_thumb_store(path: &str, flavor: u32, png: &[u8], mime: &str) {
    grove_core::thumbs::store(Path::new(path), flavor, png, mime);
}

fn grove_thumb_external(path: &str, mimes: &str, px: u32) -> String {
    let list: Vec<String> = mimes.split('\n').filter(|s| !s.is_empty()).map(String::from).collect();
    let Some(t) = grove_core::thumbs::find_thumbnailer(grove_core::thumbs::thumbnailers(), &list) else { return String::new() };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    grove_core::thumbs::generate_external(t, Path::new(path), px, list.first().map(String::as_str).unwrap_or(""), &cancel)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}
