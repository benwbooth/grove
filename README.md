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
