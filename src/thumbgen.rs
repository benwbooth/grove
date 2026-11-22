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

fn system_thumbnailer(path: &Path, mimes: &[String], px: u32, cancel: &AtomicBool) -> Option<String> {
    let t = grove_core::thumbs::find_thumbnailer(grove_core::thumbs::thumbnailers(), mimes)?;
    let mime = mimes.first().map(String::as_str).unwrap_or("");
    grove_core::thumbs::run_thumbnailer(t, path, px, mime, cancel).map(|p| format!("thumb:{}", p.display()))
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
        Kind::Epub => image(previews::epub_cover(p, cancel)).or_else(|| system_thumbnailer(p, mimes, px, cancel)),
        Kind::Comic => image(previews::comic_cover(p, cancel)).or_else(|| system_thumbnailer(p, mimes, px, cancel)),
        Kind::Office => image(previews::office_thumbnail(p, cancel))
            .or_else(|| system_thumbnailer(p, mimes, px, cancel))
            .or_else(|| previews::office_text(p, 6000, cancel).map(|t| format!("page:{}", prose_page(&t))))
            .or_else(|| image(previews::office_picture(p, cancel))),
        Kind::Text => text_page(p).map(|j| format!("page:{j}")),
        Kind::Picture => system_thumbnailer(p, mimes, px, cancel).or_else(|| image(previews::picture(p, px, cancel))),
        Kind::Video => system_thumbnailer(p, mimes, px, cancel).or_else(|| image(previews::video_frame(p, px, cancel))),
        Kind::Audio => image(previews::audio_cover(p, px, cancel))
            .or_else(|| system_thumbnailer(p, mimes, px, cancel))
            .or_else(|| previews::audio_waveform(p, px, px / 2, "0x3b82f6", cancel).map(|w| format!("wave:{}", w.display()))),
        Kind::Font | Kind::Model => None, // drawn by the provider itself
        Kind::Other => system_thumbnailer(p, mimes, px, cancel),
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
    if buf.is_empty() || buf.contains(&0) {
        return None;
    }
    let text = String::from_utf8_lossy(&buf).into_owned();
    let shown: String = text.lines().take(48).map(|l| format!("{}\n", l.replace('\t', "    "))).collect();
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let lines = crate::preview::highlight_runs(&shown, &name, 48);
    Some(json!({"mono": true, "lines": lines}).to_string())
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
