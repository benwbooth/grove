# grove conventions

Finder-style file manager with Dolphin's power. Rust + Qt Quick via cxx-qt.

- `core/` (crate grove-core): no Qt. Listing, file ops + undo, trash spec,
  tags/ratings xattrs, search, thumbnails spec, apps/mimeapps, batch rename,
  recents, settings, checksums, snapshot versions. Unit-tested
  (`cargo test -p grove-core`). Tests that change XDG_* env take
  `test_env_lock()` and must never touch real user data (trash tests use
  `empty_in(&[home_trash()])`, never `empty()`).
- `src/`: Qt side. `bridge.rs` (Grove controller declarations),
  `controller.rs` (implementations), `model.rs` (FolderModel, a
  QAbstractListModel), `terminal.rs` (F4 panel), plus helpers.
  `cpp/native.cpp` + `include/native_helpers.h`: app creation (native
  text), thumbnail/icon image providers, QMimeDatabase, clipboard.
- `qml/`: every QML file is registered automatically (build.rs).

Rules (Ben's):
- Native text: the app sets NativeTextRendering; build.rs rejects
  Text.QtRendering/CurveRendering. Window roots and popups carry a 0.001 px
  Translate / padding nudge.
- Never block the UI thread: invokables that touch disk/processes/network
  answer through `async_reply(key, json)` (qml/Async.qml) or signals.
- No KDE/GNOME libraries. Small standard tools (bsdtar, sshfs, rclone,
  udisksctl, git, pkexec) are fine; add them to `runtimeFor` in flake.nix.
- Tooltips on every control, shortcuts in qml/Help.qml, settings rows with
  a one-line explanation in SettingsWindow.qml.
- QML gotcha: never write `win: win` / `pane: pane` (binds to itself);
  window ids are `bw` (browser) and `fc` (chooser).
- Commit and push every working piece.

Testing: `nix develop -c cargo build`, then screenshots with throwaway XDG
dirs: `GROVE_TEST="view=list;ql" grove --screenshot out.png PATH` under
QT_QPA_PLATFORM=offscreen (GROVE_NO_DBUS is implied). Steps: see
BrowserWindow.testStep. GROVE_PERF=1 prints UI-thread time per model step.
