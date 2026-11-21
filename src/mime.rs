//! File types (shared-mime-info through Qt's QMimeDatabase, cached by
//! extension so a 100 000-item folder costs one lookup per extension) and
//! the icon names the views ask the icon theme for.

use crate::bridge::ffi;
use cxx_qt_lib::QString;
use grove_core::listing::Entry;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug, Clone, Default)]
pub struct MimeRec {
    pub name: String,
    pub icon: String,
    pub generic: String,
    pub comment: String,
}

fn cache() -> &'static Mutex<HashMap<String, Arc<MimeRec>>> {
    static C: OnceLock<Mutex<HashMap<String, Arc<MimeRec>>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

fn lookup(name: &str, path: &str, deep: bool) -> MimeRec {
    let s = ffi::grove_mime_info(&QString::from(name), &QString::from(path), deep).to_string();
    let mut it = s.split('\n');
    MimeRec {
        name: it.next().unwrap_or("application/octet-stream").to_string(),
        icon: it.next().unwrap_or("").to_string(),
        generic: it.next().unwrap_or("").to_string(),
        comment: it.next().unwrap_or("").to_string(),
    }
}

pub fn folder() -> Arc<MimeRec> {
    static F: OnceLock<Arc<MimeRec>> = OnceLock::new();
    F.get_or_init(|| Arc::new(MimeRec { name: "inode/directory".into(), icon: "folder".into(), generic: "folder".into(), comment: "Folder".into() })).clone()
}

pub fn for_entry(e: &Entry) -> Arc<MimeRec> {
    if e.is_dir {
        return folder();
    }
    let ext = e.ext().to_ascii_lowercase();
    // Extension-less names ("Makefile", "README") are looked up by name.
    let key = if ext.is_empty() { format!("name:{}", e.name) } else { ext };
    if let Some(m) = cache().lock().unwrap().get(&key) {
        return m.clone();
    }
    let rec = Arc::new(lookup(&e.name, "", false));
    cache().lock().unwrap().insert(key, rec.clone());
    rec
}

/// Content-sniffed type (Get Info, Open With, previews).
pub fn deep(path: &str) -> MimeRec {
    let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    lookup(&name, path, true)
}

/// A type and its parents ("text/x-rust", "text/plain", ...).
pub fn with_parents(mime: &str) -> Vec<String> {
    let mut v = vec![mime.to_string()];
    for p in ffi::grove_mime_parents(&QString::from(mime)).to_string().split('\n') {
        if !p.is_empty() && !v.iter().any(|x| x == p) {
            v.push(p.to_string());
        }
    }
    v
}

/// The icon to ask the theme for, with fallbacks ("a|b|c").
pub fn icon_for(e: &Entry, m: &MimeRec) -> String {
    if e.is_dir {
        let home = grove_core::util::home();
        let p = std::path::Path::new(&e.path);
        if p == home {
            return "user-home|folder".into();
        }
        if p.parent() == Some(home.as_path()) {
            let special = match e.name.as_str() {
                "Desktop" => "user-desktop",
                "Documents" => "folder-documents",
                "Downloads" => "folder-download",
                "Music" => "folder-music",
                "Pictures" => "folder-pictures",
                "Videos" => "folder-videos",
                "Public" => "folder-publicshare",
                "Templates" => "folder-templates",
                "src" | "Projects" | "projects" => "folder-development",
                _ => "",
            };
            if !special.is_empty() {
                return format!("{special}|folder");
            }
        }
        if e.name == ".git" {
            return "folder-git|folder".into();
        }
        return "folder".into();
    }
    if e.broken {
        return "emblem-unreadable|application-x-zerosize".into();
    }
    if e.mode & 0o111 != 0 && (m.name == "application/x-executable" || m.name == "application/x-sharedlib" || m.name == "application/x-pie-executable") {
        return "application-x-executable|application-x-executable-script".into();
    }
    format!("{}|{}|unknown", m.icon, if m.generic.is_empty() { "unknown" } else { &m.generic })
}

/// Whether a thumbnail is worth asking for.
pub fn thumbable(e: &Entry, m: &MimeRec) -> bool {
    if e.is_dir || e.broken || e.size == 0 {
        return false;
    }
    let n = m.name.as_str();
    if n.starts_with("image/") || n.starts_with("video/") || n == "application/pdf" {
        return true;
    }
    let ts = grove_core::thumbs::thumbnailers();
    ts.iter().any(|t| t.mime_types.iter().any(|x| x == n))
}
