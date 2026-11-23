//! What the C++ thumbnail provider can't make by itself: the kind of
//! preview a file gets, pictures from external tools (grove_core::previews
//! and the system's thumbnailers), and text snippets for document-style
//! thumbnails. Runs on the provider's worker threads; every job has a
//! token the provider cancels when its item scrolls away.

use grove_core::previews::{self, Kind};
use parking_lot::Mutex;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

fn tokens() -> &'static Mutex<HashMap<u64, Arc<AtomicBool>>> {
    static T: OnceLock<Mutex<HashMap<u64, Arc<AtomicBool>>>> = OnceLock::new();
    T.get_or_init(Default::default)
}

static QUITTING: AtomicBool = AtomicBool::new(false);

/// The cancel flag of job `token` (made on first use; already set once
/// the app is quitting).
pub fn flag(token: u64) -> Arc<AtomicBool> {
    tokens().lock().entry(token).or_insert_with(|| Arc::new(AtomicBool::new(QUITTING.load(Ordering::Relaxed)))).clone()
}

/// The app is quitting: stop every job (their tools are killed).
pub fn cancel_all() {
    QUITTING.store(true, Ordering::Relaxed);
    for f in tokens().lock().values() {
        f.store(true, Ordering::Relaxed);
    }
}

pub fn cancel(token: u64) {
    flag(token).store(true, Ordering::Relaxed);
}

pub fn done(token: u64) {
    tokens().lock().remove(&token);
}

pub fn kind(path: &str, mimes: &[String]) -> Kind {
    let name = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    previews::kind_of(mimes, &name)
}

/// The kind as the provider sees it (it picks a thread pool by it).
pub fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Picture => "picture",
        Kind::Video => "video",
        Kind::Audio => "audio",
        Kind::Pdf => "pdf",
        Kind::Epub => "epub",
        Kind::Comic => "comic",
        Kind::Office => "office",
        Kind::Text => "text",
        Kind::Font => "font",
        Kind::Model => "model",
        Kind::Other => "other",
    }
}

fn image(p: Option<std::path::PathBuf>) -> Option<String> {
    p.map(|p| format!("image:{}", p.display()))
}

/// Make a picture for `path` (`px`: the flavor size). Answers:
/// "thumb:<path>" (stored already), "image:<temp file>" or "wave:<temp
/// file>" (the provider scales or frames it, stores it, deletes it),
/// "page:<json>" (text for the provider to draw as a page), "qtpdf" (draw
/// it with QtPdf), or "" (nothing).
pub fn make(path: &str, mimes: &[String], px: u32, token: u64) -> String {
    let c = flag(token);
    let cancel = c.as_ref();
    let p = Path::new(path);
    let r: Option<String> = match kind(path, mimes) {
        Kind::Pdf => {
            if previews::can_draw_pdf() {
                image(previews::pdf_page(p, 1, px, false, cancel))
            } else {
                Some("qtpdf".into())
            }
        }
        Kind::Epub => image(previews::epub_cover(p, cancel)),
        Kind::Comic => image(previews::comic_cover(p, cancel)),
        Kind::Office => image(previews::office_thumbnail(p, cancel))
            // Spreadsheets: their first rows as a table.
            .or_else(|| if crate::table::is_table_name(&path.to_lowercase()) { sheet_page(p).map(|j| format!("page:{j}")) } else { None })
            
            .or_else(|| previews::office_text(p, 6000, cancel).map(|t| format!("page:{}", prose_page(&t))))
            .or_else(|| image(previews::office_picture(p, cancel))),
        Kind::Text => text_page(p).map(|j| format!("page:{j}")),
        Kind::Picture => image(previews::picture(p, px, cancel)),
        Kind::Video => image(previews::video_frame(p, px, cancel)),
        Kind::Audio => image(previews::audio_cover(p, px, cancel))
            
            .or_else(|| previews::audio_waveform(p, px, px / 2, "0x3b82f6", cancel).map(|w| format!("wave:{}", w.display()))),
        Kind::Font | Kind::Model => None, // drawn by the provider itself
        Kind::Other => None,
    };
    r.unwrap_or_default()
}

/// Page `page` of a PDF at `width` pixels wide, as a temporary PNG ("" if
/// it couldn't be drawn).
pub fn pdf_page(path: &str, page: u32, width: u32, token: u64) -> String {
    let c = flag(token);
    previews::pdf_page(Path::new(path), page, width.clamp(64, 4096), true, &c).map(|p| p.display().to_string()).unwrap_or_default()
}

/// The first lines of a text file, syntax-coloured (a light theme: the
/// thumbnail is a sheet of paper), as {"mono": true, "lines": [[[colour,
/// text], ...], ...]}.
pub fn text_page(p: &Path) -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(p).ok()?;
    let mut buf = vec![0u8; 6000];
    let mut n = 0;
    while n < buf.len() {
        let k = f.read(&mut buf[n..]).ok()?;
        if k == 0 {
            break;
        }
        n += k;
    }
    buf.truncate(n);
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let enc = grove_core::textenc::detect(&buf, &name);
    let utf16 = matches!(enc, grove_core::textenc::Enc::Utf16Le | grove_core::textenc::Enc::Utf16Be);
    if buf.is_empty() || (!utf16 && buf.contains(&0)) {
        return None;
    }
    let text = grove_core::textenc::decode(&buf, enc);
    let lower = name.to_lowercase();
    // Markdown rendered (the provider lays it out with QTextDocument).
    if crate::preview::is_markdown_name(&lower) {
        let md = match text.rfind('\n') {
            Some(i) if n == 6000 => &text[..i],
            _ => text.as_str(),
        };
        return Some(json!({"markdown": md}).to_string());
    }
    // Tables as aligned columns under a dark header.
    if let Some(delim) = crate::table::delimiter_for(&lower) {
        if let Some(j) = table_page(&crate::table::parse_delimited(&text, delim, 40)) {
            return Some(j);
        }
    }
    let shown: String = text.lines().take(48).map(|l| format!("{}\n", l.replace('\t', "    "))).collect();
    let name = crate::langguess::name_for(&p.to_string_lossy(), &text, &name);
    let lines = crate::preview::highlight_runs(&shown, &name, 48);
    Some(json!({"mono": true, "lines": lines}).to_string())
}

/// Rows as aligned columns under a blue header, for a text page.
pub fn table_page(rows: &[Vec<String>]) -> Option<String> {
    if rows.is_empty() {
        return None;
    }
    let rows = &rows[..rows.len().min(48)];
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let widths: Vec<usize> = (0..cols).map(|c| rows.iter().map(|r| r.get(c).map(|x| x.chars().count()).unwrap_or(0)).max().unwrap_or(0).min(14)).collect();
    let lines: Vec<serde_json::Value> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mut line = String::new();
            for (c, w) in widths.iter().enumerate() {
                let cell: String = r.get(c).map(|x| x.replace(['\n', '\r', '\t'], " ")).unwrap_or_default().chars().take(*w).collect();
                line.push_str(&format!("{cell:<w$}  ", w = *w));
            }
            json!([[if i == 0 { "#0a4fa8" } else { "#333333" }, line.trim_end()]])
        })
        .collect();
    Some(json!({"mono": true, "lines": lines}).to_string())
}

/// A spreadsheet's first sheet as a table page (not for huge files: the
/// reader takes the whole sheet).
fn sheet_page(p: &Path) -> Option<String> {
    if std::fs::metadata(p).ok()?.len() > 20 << 20 {
        return None;
    }
    let s = crate::table::read_sheets(&p.to_string_lossy()).ok()?.into_iter().next()?;
    let mut rows = vec![s.header];
    rows.extend(s.rows.into_iter().take(47));
    table_page(&rows)
}

/// Plain prose (an office document's text) wrapped into lines.
pub fn prose_page(text: &str) -> String {
    let mut lines: Vec<serde_json::Value> = vec![];
    for (i, para) in text.lines().enumerate() {
        let colour = if i == 0 { "#111111" } else { "#333333" };
        let mut cur = String::new();
        for w in para.split_whitespace() {
            if cur.chars().count() + w.chars().count() + 1 > 56 && !cur.is_empty() {
                lines.push(json!([[colour, cur]]));
                cur = String::new();
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(w);
        }
        lines.push(json!([[colour, cur]]));
        if lines.len() > 48 {
            break;
        }
    }
    json!({"mono": false, "lines": lines}).to_string()
}
