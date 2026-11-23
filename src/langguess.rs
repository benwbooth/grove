//! What language a text file is in when its name doesn't say (flake.lock,
//! a script with no extension, a .conf, a log): clear-cut content checks
//! first (JSON, XML/HTML/SVG, CSV/TSV, Markdown, INI; shebangs are syntect's),
//! then Google's Magika, a small AI model (~1 MB) that recognises 200+ file
//! types from the content, run as its command-line tool. Results are kept
//! per file (path, size, time). Never called on the UI thread.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// The name to pick syntax colouring by: `name` itself when it already
/// tells, else "guessed.<ext>" from the content.
pub fn name_for(path: &str, text: &str, name: &str) -> String {
    if crate::preview::known_by_name(name, text) {
        return name.to_string();
    }
    match guess_ext(path, text) {
        Some(ext) => format!("guessed.{ext}"),
        None => name.to_string(),
    }
}

/// The extension a text's content looks like (json, xml, py, rs...).
pub fn guess_ext(path: &str, text: &str) -> Option<String> {
    if let Some(e) = by_rules(text) {
        return Some(e.to_string());
    }
    magika(path)
}

fn by_rules(text: &str) -> Option<&'static str> {
    let t = text.trim_start_matches('\u{feff}').trim();
    if t.len() < 2 {
        return None;
    }
    let lower: String = t.chars().take(256).collect::<String>().to_lowercase();
    if lower.starts_with("<svg") || (lower.starts_with("<?xml") && lower.contains("<svg")) {
        return Some("svg");
    }
    if lower.starts_with("<!doctype html") || lower.starts_with("<html") {
        return Some("html");
    }
    if lower.starts_with("<?xml") || (lower.starts_with('<') && t.ends_with('>') && t.contains("</")) {
        return Some("xml");
    }
    if t.starts_with('{') || t.starts_with('[') {
        // The first JSON value parses (a whole document, or the start of a
        // big one cut off by the reader): JSON. JSON lines too.
        let mut de = serde_json::Deserializer::from_str(t).into_iter::<serde_json::Value>();
        match de.next() {
            Some(Ok(_)) => return Some("json"),
            Some(Err(e)) if e.is_eof() && t.len() > 64 * 1024 => return Some("json"),
            _ => {}
        }
    }
    let lines: Vec<&str> = t.lines().filter(|l| !l.trim().is_empty()).take(200).collect();
    // INI / desktop / systemd: [section] headers and key=value lines.
    let sections = lines.iter().filter(|l| { let l = l.trim(); l.starts_with('[') && l.ends_with(']') && !l.contains(' ') }).count();
    let pairs = lines.iter().filter(|l| { let l = l.trim(); !l.starts_with('#') && !l.starts_with(';') && l.contains('=') && !l.starts_with('[') }).count();
    if sections >= 1 && sections + pairs + lines.iter().filter(|l| l.trim_start().starts_with(['#', ';'])).count() == lines.len() && pairs >= 1 {
        return Some("ini");
    }
    if lines.len() >= 3 {
        for (sep, kind) in [('\t', "tsv"), (',', "csv")] {
            let counts: Vec<usize> = lines.iter().map(|l| l.matches(sep).count()).collect();
            if counts[0] >= 1 && counts.iter().all(|&c| c == counts[0]) {
                return Some(kind);
            }
        }
    }
    let md_headings = lines.iter().filter(|l| l.starts_with("# ") || l.starts_with("## ") || l.starts_with("### ")).count();
    if !t.starts_with("#!") && md_headings >= 2 && (t.contains("](") || t.contains("```") || t.contains("\n- ")) {
        return Some("md");
    }
    None
}

fn magika(path: &str) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let key = (path.to_string(), meta.len(), std::os::unix::fs::MetadataExt::mtime(&meta));
    static CACHE: OnceLock<Mutex<HashMap<(String, u64, i64), Option<String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(v) = cache.lock().unwrap().get(&key) {
        return v.clone();
    }
    // Magika's model in-process (magika crate, onnxruntime linked; no
    // magika command). One session, kept: loading the model takes a moment.
    static SESSION: OnceLock<Option<Mutex<magika::Session>>> = OnceLock::new();
    let session = SESSION.get_or_init(|| magika::Session::new().ok().map(Mutex::new)).as_ref()?;
    let ft = session.lock().unwrap_or_else(|e| e.into_inner()).identify_file_sync(path).ok()?;
    let info = ft.info();
    let label = info.label;
    let ext = if info.is_text && !matches!(label, "txt" | "unknown" | "empty" | "undefined") {
        info.extensions.first().map(|e| e.to_string()).or_else(|| Some(label.to_string()))
    } else {
        None
    };
    let mut c = cache.lock().unwrap();
    if c.len() > 4096 {
        c.clear();
    }
    c.insert(key, ext.clone());
    ext
}

#[cfg(test)]
mod tests {
    use super::by_rules;
    #[test]
    fn rules() {
        assert_eq!(by_rules("{\n  \"nodes\": {\"a\": 1}\n}\n"), Some("json"));
        assert_eq!(by_rules("<?xml version=\"1.0\"?><a></a>"), Some("xml"));
        assert_eq!(by_rules("[core]\nbare = false\n# c\n"), Some("ini"));
        assert_eq!(by_rules("a,b,c\n1,2,3\n4,5,6\n"), Some("csv"));
        assert_eq!(by_rules("just some words"), None);
    }
}
