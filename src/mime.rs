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

/// Types grove knows before the system's MIME database does (it ships
/// data/mime/*.xml for the system; these cover a fresh dev build).
fn known_here(name: &str) -> Option<MimeRec> {
    let l = name.to_ascii_lowercase();
    // Google Docs/Sheets/Slides... link files (Insync, Google Drive).
    if let Some(k) = grove_core::gdocs::kind(&l) {
        return Some(MimeRec { name: k.mime.into(), icon: k.icon.into(), generic: k.icon.into(), comment: k.comment.into() });
    }
    if l.ends_with(".tsh") || l.ends_with(".tessh") {
        return Some(MimeRec { name: "text/x-tessh".into(), icon: "text-x-script".into(), generic: "text-x-script".into(), comment: "tessh script".into() });
    }
    None
}

fn lookup(name: &str, path: &str, deep: bool) -> MimeRec {
    if let Some(r) = known_here(name) {
        return r;
    }
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
    // Asked per row while views draw: remember each type's answer.
    thread_local! { static CACHE: std::cell::RefCell<std::collections::HashMap<String, Vec<String>>> = Default::default(); }
    if let Some(v) = CACHE.with(|c| c.borrow().get(mime).cloned()) {
        return v;
    }
    let v = parents_uncached(mime);
    CACHE.with(|c| c.borrow_mut().insert(mime.to_string(), v.clone()));
    v
}

fn parents_uncached(mime: &str) -> Vec<String> {
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
    let theme = format!("{}|{}|unknown", m.icon, if m.generic.is_empty() { "unknown" } else { &m.generic });
    match type_icon(e, m) {
        Some(t) => format!("{t}|{theme}"),
        None => theme,
    }
}

struct TypeTables {
    pictures: HashMap<&'static str, &'static [u8]>,
    exts: HashMap<&'static str, (&'static str, &'static str)>,
    names: HashMap<&'static str, (&'static str, &'static str)>,
    mimes: HashMap<&'static str, (&'static str, &'static str)>,
    generic: HashMap<&'static str, &'static str>,
}

fn type_tables() -> &'static TypeTables {
    use crate::filetype_table as t;
    static T: OnceLock<TypeTables> = OnceLock::new();
    T.get_or_init(|| TypeTables {
        pictures: t::PICTURES.iter().copied().collect(),
        exts: t::EXTS.iter().map(|&(k, p, l)| (k, (p, l))).collect(),
        names: t::NAMES.iter().map(|&(k, p, l)| (k, (p, l))).collect(),
        mimes: t::MIMES.iter().map(|&(k, p, l)| (k, (p, l))).collect(),
        generic: t::GENERIC.iter().copied().collect(),
    })
}

/// A file type picture's SVG (icons/filetypes), for the icon provider.
pub fn type_icon_svg(name: &str) -> &'static [u8] {
    type_tables().pictures.get(name).copied().unwrap_or(&[])
}

/// The label written on a type's page: the extension in capitals, up to
/// seven letters (a longer one says nothing useful at that size).
fn ext_label(ext: &str) -> String {
    let l: String = ext.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '#' | '-' | '_')).collect();
    if l.is_empty() || l.chars().count() > 7 {
        return String::new();
    }
    l.to_ascii_uppercase()
}

/// grove's own picture for a file ("grove-type:code-rust/RUST"): by whole
/// name, then the longest extension the table knows ("x.tar.gz" before
/// "gz"), then the MIME type, then its generic icon or family, which take
/// the extension as their label. None leaves it to the icon theme.
pub fn type_icon(e: &Entry, m: &MimeRec) -> Option<String> {
    let t = type_tables();
    let lname = e.name.to_ascii_lowercase();
    let mut hit = t.names.get(lname.as_str()).map(|&(p, l)| (p, l.to_string()));
    if hit.is_none() {
        // Every suffix after a dot, longest first (".gitignore" is "gitignore").
        hit = lname.match_indices('.').find_map(|(i, _)| t.exts.get(&lname[i + 1..]).map(|&(p, l)| (p, l.to_string())));
    }
    if hit.is_none() {
        hit = t.mimes.get(m.name.as_str()).map(|&(p, l)| (p, l.to_string()));
    }
    if hit.is_none() {
        let family = m.name.split('/').next().unwrap_or("");
        let p = t.generic.get(m.generic.as_str()).or_else(|| t.generic.get(m.icon.as_str())).or_else(|| t.generic.get(family)).or_else(|| t.generic.get("unknown"))?;
        hit = Some((p, ext_label(e.ext())));
    }
    let (pic, label) = hit?;
    // '#' (C#) would end the image URL: the provider decodes %23.
    Some(format!("grove-type:{pic}/{}", label.replace('%', "%25").replace('#', "%23").replace('|', "").replace('?', "")))
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
    // Everything grove draws itself or with its tools (thumbgen.rs).
    // With the type's parents: a .bat (application/x-bat) or .ps1 is text.
    if grove_core::previews::kind_of(&with_parents(&m.name), &e.name) != grove_core::previews::Kind::Other {
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
