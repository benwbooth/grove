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

/// A folder's own icon from its .directory file (Dolphin's way: an
/// [Desktop Entry] Icon= theme name or image path).
pub fn custom_folder_icon(dir: &std::path::Path) -> String {
    let Ok(t) = std::fs::read_to_string(dir.join(".directory")) else { return String::new() };
    let mut in_entry = false;
    for l in t.lines() {
        let l = l.trim();
        if l.starts_with('[') {
            in_entry = l == "[Desktop Entry]";
        } else if in_entry {
            if let Some(v) = l.strip_prefix("Icon=") {
                let v = v.trim();
                if v.starts_with('/') || v.starts_with("./") {
                    let p = if let Some(r) = v.strip_prefix("./") { dir.join(r) } else { std::path::PathBuf::from(v) };
                    return format!("{}|folder", p.display());
                }
                return format!("{v}|folder");
            }
        }
    }
    String::new()
}

/// Set (or clear, with "") a folder's icon in its .directory file,
/// keeping whatever else is in it.
pub fn set_custom_folder_icon(dir: &std::path::Path, icon: &str) -> Result<(), String> {
    let f = dir.join(".directory");
    let text = std::fs::read_to_string(&f).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let mut start = lines.iter().position(|l| l.trim() == "[Desktop Entry]");
    if start.is_none() {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push("[Desktop Entry]".into());
        start = Some(lines.len() - 1);
    }
    let s = start.unwrap();
    let end = lines.iter().enumerate().skip(s + 1).find(|(_, l)| l.trim().starts_with('[')).map(|(i, _)| i).unwrap_or(lines.len());
    let existing = (s + 1..end).find(|&i| lines[i].trim_start().starts_with("Icon="));
    match (existing, icon.is_empty()) {
        (Some(i), true) => {
            lines.remove(i);
        }
        (Some(i), false) => lines[i] = format!("Icon={icon}"),
        (None, false) => lines.insert(s + 1, format!("Icon={icon}")),
        (None, true) => {}
    }
    let only_header = lines.iter().filter(|l| !l.trim().is_empty()).count() == 1;
    if only_header {
        let _ = std::fs::remove_file(&f);
        return Ok(());
    }
    std::fs::write(&f, lines.join("\n") + "\n").map_err(|e| grove_core::util::plain_error(&e, &f))
}
