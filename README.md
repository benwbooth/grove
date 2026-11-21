# Grove

A Finder-style file manager with Dolphin's power, for Linux. Rust + Qt Quick
(cxx-qt), no KDE or GNOME libraries. Made for the arbor window manager and
the hearth shell (macOS 26 look), but it runs on any desktop.

See FEATURES.md for the Finder/Dolphin parity checklist, and press F1 in
grove for help and every shortcut.

## Build

    nix develop -c cargo build          # dev build (target/debug/grove)
    nix develop -c cargo test --workspace
    nix build                           # the package

## License

MIT

## What's in it

Finder's look and habits (glass sidebar, icon/list/column/gallery views,
Quick Look on Space, tags, Get Info, Smart Folders, previous versions from
snapshots, share to nearby devices over LocalSend) with Dolphin's power
(split view, filter bar, terminal/information/folders panels, tabs, git
actions, service menus, ratings, checksums, archives as folders, remote
places, undo for everything). FEATURES.md has the full checklist.

## Performance

A folder of 100 000 files (dev build, offscreen): listed, stat'ed, typed
and tagged in ~0.6 s on a background thread; the first items show after
~120 ms. UI-thread time per step (`GROVE_PERF=1`): batches 11–16 ms each,
the final sort + diff 54 ms, typing in the filter bar 26 ms. Views are
virtualized, so scrolling costs the same as in a small folder; thumbnails
are generated newest-request-first on a thread pool and dropped when they
scroll away before starting.

## Install (NixOS)

See docs/NIXOS.md.
