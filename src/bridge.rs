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
        /// A PNG on the clipboard (Grove Preview's Copy of a selection).
        fn grove_set_clipboard_png(data: &[u8]);
        fn grove_can_read_image(path: &QString) -> bool;
        fn grove_image_rotate(path: &QString, degrees: i32) -> QString;
        fn grove_image_convert(path: &QString, format: &QString) -> QString;
        fn grove_markup_render(src: &QString, ops: &QString, dest: &QString) -> QString;
        fn grove_test_key(key: i32, modifiers: i32, text: &QString);
        fn grove_test_wheel(x: f64, y: f64, angle_y: i32, pixel_y: i32);
        fn grove_test_wheel_x(x: f64, y: f64, angle_x: i32);
        fn grove_test_touchpad(x: f64, y: f64, ax: i32, ay: i32, px: i32, py: i32, phase: i32);
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
        fn grove_thumb_kind(path: &str, mimes: &str) -> String;
        fn grove_thumb_make(path: &str, mimes: &str, px: u32, token: u64) -> String;
        fn grove_thumb_cancel(token: u64);
        fn grove_thumb_cancel_all();
        fn grove_thumb_done(token: u64);
        fn grove_pdf_page(path: &str, page: u32, width: u32, token: u64) -> String;
        fn grove_archive_local(path: &str, extract: bool) -> String;
        fn grove_du_treemap(scan: i32, node: u32, w: u32, h: u32, apparent: bool) -> Vec<u8>;
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
        /// hearth's accounts service (Settings > Accounts) appeared, went
        /// away, or its accounts changed.
        #[qsignal]
        fn accounts_changed(self: Pin<&mut Grove>, available: bool);

        #[qinvokable]
        fn start(self: Pin<&mut Grove>);
        #[qinvokable]
        fn home(self: &Grove) -> QString;
        /// The system's name (the system disk in Computer).
        #[qinvokable]
        fn os_name(self: &Grove) -> QString;
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
        /// btrfs: what `path` is ({btrfs, subvol, kind, info, snapper,
        /// place, snapshotName, needsRoot}).
        #[qinvokable]
        fn subvol_info(self: &Grove, key: &QString, path: &QString);
        /// btrfs: a new subvolume in `dir` ({path} or {error}).
        #[qinvokable]
        fn new_subvolume(self: &Grove, key: &QString, dir: &QString, name: &QString);
        /// btrfs: snapshot of the subvolume `src` in `dest` ("": the default place).
        #[qinvokable]
        fn snapshot_subvolume(self: &Grove, key: &QString, src: &QString, dest: &QString, name: &QString, read_only: bool);
        /// btrfs: delete a subvolume or snapshot (as root through pkexec when needed).
        #[qinvokable]
        fn delete_subvolume(self: &Grove, key: &QString, path: &QString);
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
        /// An office document converted to PDF (LibreOffice): {kind: "pdf", ...} or {error}.
        #[qinvokable]
        fn office_pdf(self: &Grove, key: &QString, path: &QString);
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
        /// Add Cloud Drive (rclone): kinds of drives, drives set up, one
        /// step of rclone's question flow, delete, cancel a waiting step.
        #[qinvokable]
        fn rclone_providers(self: &Grove, key: &QString);
        #[qinvokable]
        fn rclone_drives(self: &Grove, key: &QString);
        #[qinvokable]
        fn rclone_step(self: &Grove, key: &QString, json: &QString);
        #[qinvokable]
        fn rclone_delete(self: &Grove, key: &QString, name: &QString);
        #[qinvokable]
        fn rclone_cancel(self: &Grove);
        #[qinvokable]
        fn archive_list(self: &Grove, key: &QString, archive: &QString);
        /// Disk Usage (KDirStat-like): start a scan, poll it, read its tree
        /// (all in memory once scanned), types, treemap hit-testing.
        #[qinvokable]
        fn du_start(self: &Grove, path: &QString) -> i32;
        #[qinvokable]
        fn du_progress(self: &Grove, scan: i32) -> QString;
        #[qinvokable]
        fn du_cancel(self: &Grove, scan: i32);
        #[qinvokable]
        fn du_forget(self: &Grove, scan: i32);
        #[qinvokable]
        fn du_rows(self: &Grove, key: &QString, scan: i32, expanded: &QString, apparent: bool, sort: &QString);
        #[qinvokable]
        fn du_types(self: &Grove, key: &QString, scan: i32, node: i32, apparent: bool);
        #[qinvokable]
        fn du_path(self: &Grove, scan: i32, node: i32) -> QString;
        #[qinvokable]
        fn du_ancestors(self: &Grove, scan: i32, node: i32) -> QString;
        #[qinvokable]
        fn du_parent(self: &Grove, scan: i32, node: i32) -> i32;
        #[qinvokable]
        fn du_find(self: &Grove, scan: i32, path: &QString) -> i32;
        #[qinvokable]
        fn du_removed(self: &Grove, scan: i32, node: i32);
        #[qinvokable]
        fn du_hit(self: &Grove, scan: i32, x: f64, y: f64) -> i32;
        #[qinvokable]
        fn du_rect(self: &Grove, scan: i32, node: i32) -> QString;
        /// A document without a viewer of its own (DjVu, EPUB, MOBI, XPS,
        /// PostScript, CBR...) as a cached PDF to page through (async_reply {pdf}).
        #[qinvokable]
        fn doc_pdf(self: &Grove, key: &QString, path: &QString);
        /// A track's waveform for the player (async_reply {base, played}: PNGs).
        #[qinvokable]
        fn audio_waveform(self: &Grove, key: &QString, path: &QString, width: i32, height: i32, dark: bool);
        /// Grove Player's trim filmstrip (async_reply {path, frames} or {error}).
        #[qinvokable]
        fn player_filmstrip(self: &Grove, key: &QString, path: &QString, frames: i32, height: i32);
        /// Recently played movies and songs (newest first): JSON list of paths.
        #[qinvokable]
        fn player_recents(self: &Grove, key: &QString);
        /// What Grove Player should open for a path (async_reply {kind:
        /// "dvd" | "bluray" | "file" | "folder", device}): DVD and Blu-ray
        /// folders and disc images play through mpv's dvd:// and bd://.
        #[qinvokable]
        fn player_probe(self: &Grove, key: &QString, path: &QString);
        /// hearth's screen recorder (File › New Screen Recording).
        #[qinvokable]
        fn player_screen_recording(self: &Grove);
        /// Remember a played file in the desktop's recent files.
        #[qinvokable]
        fn player_add_recent(self: &Grove, path: &QString);
        /// A Google Docs link file's document, exported ({path} or {error}).
        #[qinvokable]
        #[cxx_name = "gdocExport"]
        fn gdoc_export(self: &Grove, key: &QString, path: &QString);
        /// Google sign-in: {configured, accounts}.
        #[qinvokable]
        #[cxx_name = "googleStatus"]
        fn google_status(self: &Grove) -> QString;
        /// Sign in with Google in the browser ({email} or {error}).
        #[qinvokable]
        #[cxx_name = "googleSignIn"]
        fn google_sign_in(self: &Grove, key: &QString);
        #[qinvokable]
        #[cxx_name = "googleSignOut"]
        fn google_sign_out(self: Pin<&mut Grove>, email: &QString);
        /// Whether hearth's accounts service is there (what grove last saw).
        #[qinvokable]
        #[cxx_name = "accountsAvailable"]
        fn accounts_available(self: &Grove) -> bool;
        /// Open hearth's Settings > Accounts (at an account id): {ok}; ok
        /// false means hearth can't show it (use grove's own setup).
        #[qinvokable]
        #[cxx_name = "accountsOpenSettings"]
        fn accounts_open_settings(self: &Grove, key: &QString, account: &QString);
        /// Sign in again through the accounts service (the browser opens):
        /// {session} or {error}; success shows as accountsChanged.
        #[qinvokable]
        #[cxx_name = "accountsSignIn"]
        fn accounts_sign_in(self: &Grove, key: &QString, provider: &QString, account: &QString);
        /// Open a web address in the default browser.
        #[qinvokable]
        #[cxx_name = "openUrl"]
        fn open_url(self: &Grove, url: &QString);
        /// A file inside an archive, taken out for previewing (async_reply {path}).
        #[qinvokable]
        fn archive_local(self: &Grove, key: &QString, path: &QString);
        /// Compressed and uncompressed sizes of an archive (async_reply).
        #[qinvokable]
        fn archive_sizes(self: &Grove, key: &QString, archive: &QString);
        #[qinvokable]
        fn extract(self: Pin<&mut Grove>, archive: &QString, dest: &QString) -> i32;
        #[qinvokable]
        fn compress(self: Pin<&mut Grove>, paths: &QString, format: &QString) -> i32;

        // --- Grove Archiver (7-Zip's features) ---
        /// An archive's full listing (async_reply: Listing or {error, code}).
        #[qinvokable]
        fn arc_list(self: &Grove, key: &QString, path: &QString, password: &QString);
        /// Start a job: kind extract | create | test | update, args JSON.
        #[qinvokable]
        fn arc_start(self: &Grove, kind: &QString, args: &QString) -> i32;
        /// A job's state now (JSON; never waits).
        #[qinvokable]
        fn arc_progress(self: &Grove, id: i32) -> QString;
        /// Answer a job's question (replace, skip, rename, ...All, cancel).
        #[qinvokable]
        fn arc_answer(self: &Grove, id: i32, answer: &QString);
        #[qinvokable]
        fn arc_cancel(self: &Grove, id: i32);
        #[qinvokable]
        fn arc_pause(self: &Grove, id: i32, paused: bool);
        #[qinvokable]
        fn arc_forget(self: &Grove, id: i32);
        /// One member taken out to the cache (async_reply {path} or {error, code}).
        /// A free name for a new file ("Archive 2.7z"): async_reply {path}.
        #[qinvokable]
        fn arc_unique(self: &Grove, key: &QString, path: &QString);
        #[qinvokable]
        fn arc_member(self: &Grove, key: &QString, archive: &QString, inner: &QString, password: &QString);

        // --- Grove Preview (macOS Preview's features; src/pvdoc.rs) ---
        /// A document or picture: {kind: "pdf"|"image", pages, sizes,
        /// outline, metadata, locked...} or picture info, or {error}.
        #[qinvokable]
        fn pv_info(self: &Grove, key: &QString, path: &QString, password: &QString);
        /// Text selection between two points (page space). mode: 0 chars,
        /// 1 words, 2 lines, 3 page. {quads, text}
        #[qinvokable]
        fn pv_select(self: &Grove, key: &QString, path: &QString, password: &QString, page: i32, ax: f64, ay: f64, bx: f64, by: f64, mode: i32);
        /// Every match: [{page, quads, line}].
        #[qinvokable]
        fn pv_search(self: &Grove, key: &QString, path: &QString, password: &QString, needle: &QString);
        #[qinvokable]
        fn pv_links(self: &Grove, key: &QString, path: &QString, password: &QString, page: i32);
        #[qinvokable]
        fn pv_widgets(self: &Grove, key: &QString, path: &QString, password: &QString);
        #[qinvokable]
        fn pv_annots(self: &Grove, key: &QString, path: &QString, password: &QString, page: i32);
        /// Write a PDF (pdfedit::Spec JSON). mode: replace (the original to
        /// the Trash, Undo puts it back), copy (beside it) or to (`dest`).
        #[qinvokable]
        fn pv_save_pdf(self: &Grove, key: &QString, path: &QString, spec: &QString, mode: &QString, dest: &QString);
        /// A picture through the pipeline (imgedit::Spec JSON) as `format`.
        #[qinvokable]
        fn pv_export_image(self: &Grove, key: &QString, src: &QString, spec: &QString, mode: &QString, dest: &QString, format: &QString, quality: i32);
        /// One page of the document (pdfedit::Spec JSON, with its changes)
        /// as a picture at `dpi`, written to `dest` as `format`.
        #[qinvokable]
        fn pv_export_page(self: &Grove, key: &QString, spec: &QString, page: i32, dest: &QString, format: &QString, quality: i32, dpi: i32);
        /// Pictures (each with its edits) as one PDF, a page each.
        #[qinvokable]
        fn pv_images_pdf(self: &Grove, key: &QString, items: &QString, dest: &QString);
        /// The printers: [{name, info, location, default, color, duplex, media}].
        #[qinvokable]
        fn pv_printers(self: &Grove, key: &QString);
        /// Print: {printer, path, title, pdf: Spec?, image: {spec}?, options: {copies, ...}}.
        #[qinvokable]
        fn pv_print(self: &Grove, key: &QString, job: &QString);
        /// A rectangle of a page or picture copied as a picture.
        #[qinvokable]
        fn pv_copy_region(self: &Grove, key: &QString, what: &QString);
        /// Saved signatures (JSON list) / save them.
        #[qinvokable]
        fn pv_signatures(self: &Grove, key: &QString);
        #[qinvokable]
        fn pv_set_signatures(self: &Grove, key: &QString, list: &QString);

        // --- Quick Look editing ---
        /// Save markup over a picture. mode: "replace" (the original goes to
        /// the Trash, Undo puts it back) or "copy" (a new file next to it).
        #[qinvokable]
        fn markup_save(self: &Grove, key: &QString, path: &QString, ops: &QString, mode: &QString);
        /// Cut a video/audio file to start..end (ms) with ffmpeg (stream copy
        /// when it can, re-encoding otherwise). mode as for markup_save.
        #[qinvokable]
        fn media_trim(self: &Grove, key: &QString, path: &QString, start_ms: f64, end_ms: f64, mode: &QString);

        /// Free and total bytes on the disk holding `path` ({free, total}).
        #[qinvokable]
        fn free_space(self: &Grove, key: &QString, path: &QString);

        // --- burning ---
        /// Optical drives: JSON [{dev, name, canWrite}].
        #[qinvokable]
        fn optical_drives(self: &Grove, key: &QString);
        /// Burn (JSON burn::Request) to a drive or an ISO file; the job id.
        #[qinvokable]
        fn burn(self: Pin<&mut Grove>, json: &QString) -> i32;

        // --- portal file chooser ---
        #[qinvokable]
        fn chooser_done(self: &Grove, id: &QString, ok: bool, uris: &QString, choices: &QString);

        // --- tests ---
        #[qinvokable]
        fn test_key(self: &Grove, key: i32, modifiers: i32, text: &QString);
        #[qinvokable]
        fn test_wheel(self: &Grove, x: f64, y: f64, angle_y: i32, pixel_y: i32);
        #[qinvokable]
        fn test_wheel_x(self: &Grove, x: f64, y: f64, angle_x: i32);
        #[qinvokable]
        fn test_touchpad(self: &Grove, x: f64, y: f64, ax: i32, ay: i32, px: i32, py: i32, phase: i32);
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

fn mime_list(mimes: &str) -> Vec<String> {
    mimes.split('\n').filter(|s| !s.is_empty()).map(String::from).collect()
}

fn grove_thumb_kind(path: &str, mimes: &str) -> String {
    crate::thumbgen::kind_name(crate::thumbgen::kind(path, &mime_list(mimes))).to_string()
}

fn grove_thumb_make(path: &str, mimes: &str, px: u32, token: u64) -> String {
    crate::thumbgen::make(path, &mime_list(mimes), px, token)
}

fn grove_thumb_cancel(token: u64) {
    crate::thumbgen::cancel(token)
}

fn grove_thumb_cancel_all() {
    crate::thumbgen::cancel_all()
}

fn grove_thumb_done(token: u64) {
    crate::thumbgen::done(token)
}

fn grove_archive_local(path: &str, extract: bool) -> String {
    crate::archive::local_copy(path, extract).map(|p| p.display().to_string()).unwrap_or_default()
}

fn grove_du_treemap(scan: i32, node: u32, w: u32, h: u32, apparent: bool) -> Vec<u8> {
    crate::du::treemap(scan, node, w, h, apparent)
}

fn grove_pdf_page(path: &str, page: u32, width: u32, token: u64) -> String {
    crate::thumbgen::pdf_page(path, page, width, token)
}

fn grove_thumb_external(path: &str, mimes: &str, px: u32) -> String {
    let list: Vec<String> = mimes.split('\n').filter(|s| !s.is_empty()).map(String::from).collect();
    let Some(t) = grove_core::thumbs::find_thumbnailer(grove_core::thumbs::thumbnailers(), &list) else { return String::new() };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    grove_core::thumbs::generate_external(t, Path::new(path), px, list.first().map(String::as_str).unwrap_or(""), &cancel)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}
