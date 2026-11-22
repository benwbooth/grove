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

fn syntax_for<'a>(ss: &'a SyntaxSet, text: &str, name: &str) -> &'a syntect::parsing::SyntaxReference {
    let ext = Path::new(name).extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    ss.find_syntax_by_extension(&ext)
        .or_else(|| ss.find_syntax_by_token(name))
        .or_else(|| text.lines().next().and_then(|l| ss.find_syntax_by_first_line(l)))
        .unwrap_or_else(|| ss.find_syntax_plain_text())
}

/// The first `max_lines` lines as coloured runs (light theme), for text
/// thumbnails: [[["#rrggbb", "text"], ...], ...].
pub fn highlight_runs(text: &str, name: &str, max_lines: usize) -> Vec<Vec<(String, String)>> {
    let ss = syntaxes();
    let syntax = syntax_for(ss, text, name);
    let theme = &themes().themes["InspiredGitHub"];
    let mut h = syntect::easy::HighlightLines::new(syntax, theme);
    let mut out = vec![];
    for line in syntect::util::LinesWithEndings::from(text).take(max_lines) {
        let regions = h.highlight_line(line, ss).unwrap_or_default();
        let runs: Vec<(String, String)> = regions
            .into_iter()
            .map(|(st, piece)| {
                let c = st.foreground;
                (format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b), piece.trim_end_matches(['\n', '\r']).to_string())
            })
            .filter(|(_, t)| !t.is_empty())
            .collect();
        out.push(runs);
    }
    out
}

pub fn highlight(text: &str, name: &str, dark: bool) -> String {
    let ss = syntaxes();
    let syntax = syntax_for(ss, text, name);
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
    use grove_core::previews::{self, Kind};
    let never = std::sync::atomic::AtomicBool::new(false);
    match previews::kind_of(&parents, &name) {
        Kind::Picture => {
            if crate::bridge::ffi::grove_can_read_image(&crate::bridge::ffi::QString::from(path)) {
                return with(json!({"kind": "image", "src": path}));
            }
            // HEIC/AVIF/RAW/PSD...: a big thumbnail (system thumbnailers,
            // vips or ImageMagick), made by the thumbnail provider.
            return with(json!({"kind": "thumb"}));
        }
        Kind::Video | Kind::Audio => {
            // The length (for trimming before the player knows it).
            let secs = crate::media::probe(path).map(|x| x.2).unwrap_or(0.0);
            return with(json!({"kind": "media", "video": m.name.starts_with("video/"), "seconds": secs}));
        }
        Kind::Pdf => return with(pdf_json(p, &never)),
        Kind::Epub | Kind::Comic | Kind::Model => {
            let count = if previews::kind_of(&parents, &name) == Kind::Comic {
                previews::archive_names(p, &never).iter().filter(|n| previews::is_picture_name(n)).count()
            } else {
                0
            };
            return with(json!({"kind": "thumb", "pages": count}));
        }
        Kind::Font => return with(json!({"kind": "font"})),
        Kind::Office => {
            let text = previews::office_text(p, 200_000, &never).unwrap_or_default();
            return with(json!({"kind": "office", "text": text, "canConvert": previews::has_libreoffice()}));
        }
        _ => {}
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

/// A PDF for Quick Look's page view: page count and sizes (or, without
/// pdftoppm/pdfinfo, QtPdf's own view).
pub fn pdf_json(p: &Path, cancel: &std::sync::atomic::AtomicBool) -> Value {
    use grove_core::previews;
    if previews::can_draw_pdf() {
        if let Some(info) = previews::pdf_info(p, 3000, cancel) {
            let sizes: Vec<Value> = info.sizes.iter().map(|(w, h)| json!([w, h])).collect();
            return json!({"kind": "pdf", "paged": true, "src": p, "pages": info.pages, "sizes": sizes, "title": info.title});
        }
    }
    json!({"kind": "pdf", "paged": false, "src": p})
}

/// An office document as a PDF (LibreOffice), for Quick Look.
pub fn office_pdf(path: &str) -> Value {
    let never = std::sync::atomic::AtomicBool::new(false);
    match grove_core::previews::office_pdf(Path::new(path), &never) {
        Some(pdf) => pdf_json(&pdf, &never),
        None => json!({"error": "LibreOffice couldn't convert it"}),
    }
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
