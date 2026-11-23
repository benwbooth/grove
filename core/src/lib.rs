//! grove's core: everything that isn't the Qt user interface.

pub mod archiver;
pub mod apps;
pub mod checksum;
pub mod du;
pub mod gdocs;
pub mod versions;
pub mod archives;
pub mod av;
pub mod burnlib;
pub mod dmg;
pub mod docs;
pub mod pdfedit;
pub mod officehtml;
pub mod iso9660;
pub mod textenc;
pub mod libarchive;
pub mod magika;
pub mod msi;
pub mod listing;
pub mod mounts;
pub mod previews;
pub mod ops;
pub mod recents;
pub mod rename;
pub mod search;
pub mod settings;
pub mod tags;
pub mod thumbs;
pub mod trash;
pub mod util;
pub mod worddoc;
pub mod vips;
pub mod winhelp;

/// Tests that change process-wide environment variables (XDG_*) take this.
#[cfg(test)]
pub(crate) fn test_env_lock() -> std::sync::MutexGuard<'static, ()> {
    static L: std::sync::Mutex<()> = std::sync::Mutex::new(());
    L.lock().unwrap_or_else(|e| e.into_inner())
}
