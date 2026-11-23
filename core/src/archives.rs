//! Every kind of archive grove opens like a folder, in-process: libarchive
//! (zip, tar.*, 7z, rar, iso, cab, deb, rpm...), Windows installers (.msi,
//! msi + cab crates) and Apple disk images (.dmg, dmgwiz + hfsplus/apfs).

use crate::libarchive::Member;
use std::path::Path;
use std::sync::atomic::AtomicBool;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Reader {
    Libarchive,
    Msi,
    Dmg,
}

fn reader(path: &Path) -> Reader {
    let n = path.to_string_lossy().to_lowercase();
    if n.ends_with(".msi") || n.ends_with(".msp") {
        Reader::Msi
    } else if n.ends_with(".dmg") {
        Reader::Dmg
    } else {
        Reader::Libarchive
    }
}

/// Names that open as folders only through the readers here (libarchive
/// doesn't read them).
pub const EXTRA_EXTS: &[&str] = &[".msi", ".msp", ".dmg"];

pub fn list(path: &Path, cancel: &AtomicBool) -> Result<Vec<Member>, String> {
    match reader(path) {
        Reader::Libarchive => crate::libarchive::list(path, cancel),
        Reader::Msi => crate::msi::list(path),
        Reader::Dmg => crate::dmg::list(path),
    }
}

pub fn extract(path: &Path, dest: &Path, only: &[&str], cancel: &AtomicBool) -> Result<(), String> {
    match reader(path) {
        Reader::Libarchive => crate::libarchive::extract(path, dest, only, cancel),
        Reader::Msi => crate::msi::extract(path, dest, only, cancel),
        Reader::Dmg => crate::dmg::extract(path, dest, only, cancel),
    }
}

pub fn read_member(path: &Path, name: &str, limit: usize, cancel: &AtomicBool) -> Option<Vec<u8>> {
    match reader(path) {
        Reader::Libarchive => crate::libarchive::read_member(path, name, limit, cancel),
        Reader::Msi => crate::msi::read_member(path, name, limit, cancel),
        Reader::Dmg => crate::dmg::read_member(path, name, limit, cancel),
    }
}

/// The file names inside (files only).
pub fn names(path: &Path, cancel: &AtomicBool) -> Vec<String> {
    list(path, cancel).map(|v| v.into_iter().filter(|m| !m.is_dir).map(|m| m.path).collect()).unwrap_or_default()
}
