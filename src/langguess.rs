//! What language a text file is in when its name doesn't say (flake.lock,
//! a script with no extension, a .conf, a log): clear-cut content checks
//! first (JSON, XML/HTML/SVG, CSV/TSV, Markdown, INI; shebangs are syntect's),
//! then Google's Magika, a small AI model (~1 MB) that recognises 200+ file
//! types from the content, run as its command-line tool. Results are kept
//! per file (path, size, time). Never called on the UI thread.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
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
    // A name that says plain text means plain text: no guessing (a .txt of
    // notes was coloured as INI or Markdown).
    if says_plain(path) {
        // Unless asked to (a setting): then only Magika, and only when it's
        // sure (the content rules took notes for INI or Markdown).
        // Java .properties files (settings saved as .txt) Magika doesn't
        // know: a strict rule, every line a comment or key=value.
        if !txt_guessing() {
            return None;
        }
        return magika(path, 0.9).or_else(|| looks_like_properties(text).then(|| "properties".to_string()));
    }
    if let Some(e) = by_rules(text) {
        return Some(e.to_string());
    }
    magika(path, 0.0)
}

/// The "colour .txt files by their contents" setting (read from the
/// settings at first use; the controller updates it when it changes).
fn txt_flag() -> &'static AtomicBool {
    static F: OnceLock<AtomicBool> = OnceLock::new();
    F.get_or_init(|| AtomicBool::new(grove_core::settings::load().guess_txt_language))
}
pub fn txt_guessing() -> bool {
    txt_flag().load(Ordering::Relaxed)
}
pub fn set_txt_guessing(on: bool) {
    txt_flag().store(on, Ordering::Relaxed)
}

/// Names that say "plain text": .txt, .text, .asc, .nfo, .diz.
pub fn says_plain(path: &str) -> bool {
    let n = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    matches!(n.rsplit_once('.').map(|(_, e)| e), Some("txt" | "text" | "asc" | "nfo" | "diz"))
}

/// Java-style properties: at least 5 settings, and every other non-blank
/// line a # or ! comment (no [sections]: that's INI).
fn looks_like_properties(text: &str) -> bool {
    let mut pairs = 0;
    for l in text.lines().map(str::trim).filter(|l| !l.is_empty()).take(400) {
        if l.starts_with('#') || l.starts_with('!') {
            continue;
        }
        let Some(i) = l.find(['=', ':']) else { return false };
        let key = l[..i].trim_end();
        if key.is_empty() || key.contains(char::is_whitespace) || key.starts_with('[') {
            return false;
        }
        pairs += 1;
    }
    pairs >= 5
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
    if looks_like_properties(t) {
        return Some("properties");
    }
    let md_headings = lines.iter().filter(|l| l.starts_with("# ") || l.starts_with("## ") || l.starts_with("### ")).count();
    if !t.starts_with("#!") && md_headings >= 2 && (t.contains("](") || t.contains("```") || t.contains("\n- ")) {
        return Some("md");
    }
    None
}

/// Magika's guess (an extension), if it's at least `min_score` sure.
fn magika(path: &str, min_score: f32) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let key = (path.to_string(), meta.len(), std::os::unix::fs::MetadataExt::mtime(&meta));
    static CACHE: OnceLock<Mutex<HashMap<(String, u64, i64), Option<(String, f32)>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(v) = cache.lock().unwrap().get(&key) {
        return v.clone().filter(|(_, s)| *s >= min_score).map(|(e, _)| e);
    }
    // Magika's model, in-process on tract (grove_core::magika; no
    // onnxruntime, no magika command).
    let g = grove_core::magika::identify(std::path::Path::new(path))?;
    // GROVE_DEBUG_LANG=1: what Magika said, and how sure it was.
    if std::env::var_os("GROVE_DEBUG_LANG").is_some() {
        eprintln!("grove magika {path}: {} ({:.2})", g.label, g.score);
    }
    let label = g.label;
    let ext = if g.is_text && !matches!(label, "txt" | "unknown" | "empty" | "undefined") {
        g.extensions.first().map(|e| e.to_string()).or_else(|| Some(label.to_string())).map(|e| (e, g.score))
    } else {
        None
    };
    let mut c = cache.lock().unwrap();
    if c.len() > 4096 {
        c.clear();
    }
    c.insert(key, ext.clone());
    ext.filter(|(_, s)| *s >= min_score).map(|(e, _)| e)
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
        assert_eq!(by_rules("# settings\n.level=WARNING\na.b=1\nc=2\nd: 3\ne=\n"), Some("properties"));
        assert_eq!(by_rules("Dear Bob: hi\nthanks=yes\n"), None);
    }
}
