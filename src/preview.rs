//! Quick Look: what to show for a file (images, media, PDF, code with
//! syntax highlighting, Markdown, archive listings, folder summaries).

use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

fn syntaxes() -> &'static SyntaxSet {
    static S: OnceLock<SyntaxSet> = OnceLock::new();
    S.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn themes() -> &'static ThemeSet {
    static T: OnceLock<ThemeSet> = OnceLock::new();
    T.get_or_init(ThemeSet::load_defaults)
}

const TEXT_LIMIT: usize = 256 * 1024;

fn is_texty(mime: &str, parents: &[String]) -> bool {
    mime.starts_with("text/")
        || parents.iter().any(|p| p == "text/plain")
        || matches!(mime, "application/json" | "application/xml" | "application/javascript" | "application/x-shellscript" | "application/toml" | "application/x-yaml" | "application/sql" | "application/x-desktop")
}

fn read_head(p: &Path, limit: usize) -> Option<(String, bool)> {
    let mut f = std::fs::File::open(p).ok()?;
    let mut buf = vec![0u8; limit];
    let mut n = 0;
    while n < limit {
        let k = f.read(&mut buf[n..]).ok()?;
        if k == 0 {
            break;
        }
        n += k;
    }
    buf.truncate(n);
    if buf.iter().take(8000).any(|&b| b == 0) {
        return None;
    }
    let truncated = n == limit;
    Some((String::from_utf8_lossy(&buf).into_owned(), truncated))
}

pub fn highlight(text: &str, name: &str, dark: bool) -> String {
    let ss = syntaxes();
    let ext = Path::new(name).extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let syntax = ss
        .find_syntax_by_extension(&ext)
        .or_else(|| ss.find_syntax_by_token(name))
        .or_else(|| text.lines().next().and_then(|l| ss.find_syntax_by_first_line(l)))
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let theme = &themes().themes[if dark { "base16-ocean.dark" } else { "InspiredGitHub" }];
    // Limit lines so the rich text stays quick.
    let shown: String = text.lines().take(2000).map(|l| format!("{l}\n")).collect();
    let mut h = syntect::easy::HighlightLines::new(syntax, theme);
    let mut out = String::from("<pre style=\"margin:0\">");
    for line in syntect::util::LinesWithEndings::from(&shown) {
        let regions = h.highlight_line(line, ss).unwrap_or_default();
        for (style, piece) in regions {
            let c = style.foreground;
            let esc = piece.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
            out.push_str(&format!("<span style=\"color:#{:02x}{:02x}{:02x}\">{}</span>", c.r, c.g, c.b, esc));
        }
    }
    out.push_str("</pre>");
    out
}

pub fn preview(path: &str, dark: bool) -> Value {
    let p = Path::new(path);
    let meta = match std::fs::metadata(p) {
        Ok(m) => m,
        Err(e) => return json!({"kind": "error", "error": grove_core::util::plain_error(&e, p)}),
    };
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if meta.is_dir() {
        let mut files = 0u64;
        let mut dirs = 0u64;
        let mut names = vec![];
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if n.starts_with('.') {
                    continue;
                }
                if e.path().is_dir() {
                    dirs += 1
                } else {
                    files += 1
                }
                if names.len() < 12 {
                    names.push(n);
                }
            }
        }
        names.sort_by(|a, b| grove_core::util::natural_cmp(a, b));
        return json!({"kind": "folder", "name": name, "files": files, "folders": dirs, "sample": names});
    }
    let m = crate::mime::deep(path);
    let parents = crate::mime::with_parents(&m.name);
    let base = json!({"name": name, "mime": m.name, "kindText": m.comment, "size": meta.len(), "sizeText": grove_core::util::human_size(meta.len())});
    let with = |mut v: Value| {
        for (k, x) in base.as_object().unwrap() {
            v[k] = x.clone();
        }
        v
    };
    if m.name.starts_with("image/") {
        if crate::bridge::ffi::grove_can_read_image(&crate::bridge::ffi::QString::from(path)) {
            return with(json!({"kind": "image", "src": path}));
        }
        // HEIC/AVIF/RAW...: a big thumbnail from the system's thumbnailers.
        if let Some(t) = grove_core::thumbs::lookup(p, 1024) {
            return with(json!({"kind": "image", "src": t}));
        }
        let mimes = parents.clone();
        if let Some(th) = grove_core::thumbs::find_thumbnailer(grove_core::thumbs::thumbnailers(), &mimes) {
            let c = std::sync::atomic::AtomicBool::new(false);
            if let Some(t) = grove_core::thumbs::generate_external(th, p, 1024, &m.name, &c) {
                return with(json!({"kind": "image", "src": t}));
            }
        }
        return with(json!({"kind": "other", "note": "grove can't show this kind of picture. Installing libheif or a RAW thumbnailer adds support."}));
    }
    if m.name.starts_with("video/") || m.name.starts_with("audio/") {
        return with(json!({"kind": "media", "video": m.name.starts_with("video/")}));
    }
    if m.name == "application/pdf" {
        return with(json!({"kind": "pdf"}));
    }
    if crate::archive::is_archive(&m.name, &name) {
        let entries = crate::archive::list(path).unwrap_or_default();
        let total = entries.len();
        let shown: Vec<Value> = entries.into_iter().take(500).map(|e| json!({"path": e.path, "size": e.size, "dir": e.is_dir})).collect();
        return with(json!({"kind": "archive", "entries": shown, "total": total}));
    }
    if is_texty(&m.name, &parents) || meta.len() < TEXT_LIMIT as u64 {
        if let Some((text, truncated)) = read_head(p, TEXT_LIMIT) {
            if m.name == "text/markdown" || name.to_lowercase().ends_with(".md") {
                return with(json!({"kind": "markdown", "text": text, "truncated": truncated}));
            }
            if is_texty(&m.name, &parents) || !text.is_empty() {
                let lines = text.lines().count();
                return with(json!({"kind": "code", "html": highlight(&text, &name, dark), "lines": lines, "truncated": truncated || lines > 2000}));
            }
        }
    }
    with(json!({"kind": "other"}))
}

#[cfg(test)]
mod tests {
    #[test]
    fn highlights_rust() {
        let h = super::highlight("fn main() { let x = 1; }\n", "a.rs", true);
        assert!(h.contains("<span style=\"color:#"));
        assert!(h.contains("main"));
    }
}
