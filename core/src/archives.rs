//! Every kind of archive grove opens like a folder, in-process: libarchive
//! (zip, tar.*, 7z, rar, iso, cab, deb, rpm...), Windows installers (.msi,
//! msi + cab crates), Apple disk images (.dmg, dmgwiz + hfsplus/apfs), and
//! CD images libarchive can't read (iso9660.rs: old ISOs, raw BIN-style
//! sectors, audio CD dumps).

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

/// CD images libarchive failed on (or found empty): grove's own reader.
fn own_iso() -> &'static std::sync::Mutex<std::collections::HashSet<std::path::PathBuf>> {
    static S: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<std::path::PathBuf>>> = std::sync::OnceLock::new();
    S.get_or_init(Default::default)
}

fn is_cd_image(path: &Path) -> bool {
    let n = path.to_string_lossy().to_lowercase();
    n.ends_with(".iso") || n.ends_with(".cdr") || n.ends_with(".toast")
}

/// Whether this image is read by iso9660.rs (decided by listing it).
fn uses_own_iso(path: &Path, cancel: &AtomicBool) -> bool {
    if !is_cd_image(path) {
        return false;
    }
    if own_iso().lock().unwrap().contains(path) {
        return true;
    }
    let _ = list(path, cancel);
    own_iso().lock().unwrap().contains(path)
}

pub fn list(path: &Path, cancel: &AtomicBool) -> Result<Vec<Member>, String> {
    if is_cd_image(path) {
        match crate::libarchive::list(path, cancel) {
            Ok(v) if !v.is_empty() => return Ok(v),
            r => {
                if crate::iso9660::readable(path) {
                    own_iso().lock().unwrap().insert(path.to_path_buf());
                    return crate::iso9660::list(path, cancel);
                }
                return r;
            }
        }
    }
    match reader(path) {
        Reader::Libarchive => crate::libarchive::list(path, cancel),
        Reader::Msi => crate::msi::list(path),
        Reader::Dmg => crate::dmg::list(path),
    }
}

pub fn extract(path: &Path, dest: &Path, only: &[&str], cancel: &AtomicBool) -> Result<(), String> {
    if uses_own_iso(path, cancel) {
        return crate::iso9660::extract(path, dest, only, cancel);
    }
    match reader(path) {
        Reader::Libarchive => crate::libarchive::extract(path, dest, only, cancel),
        Reader::Msi => crate::msi::extract(path, dest, only, cancel),
        Reader::Dmg => crate::dmg::extract(path, dest, only, cancel),
    }
}

pub fn read_member(path: &Path, name: &str, limit: usize, cancel: &AtomicBool) -> Option<Vec<u8>> {
    if uses_own_iso(path, cancel) {
        return crate::iso9660::read_member(path, name, limit, cancel);
    }
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
