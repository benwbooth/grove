//! TextDoc: a whole text file for Quick Look, in blocks a ListView shows
//! lazily (only the blocks on screen are laid out, so a 100 MB log scrolls
//! like a short one). Read, highlighted and rendered on a worker thread;
//! blocks arrive in batches, the first at once.
//!
//! Modes: "code" (syntax-coloured HTML for the first lines, plain text
//! after), "markdown" (rendered with QTextDocument's reader, styled by
//! grove_markdown_html), "html" (a web page's body as rich text), "office"
//! (an office document's text), "plain".

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QList, QModelIndex, QString, QVariant};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("cxx-qt-lib/qlist.h");
        type QList_i32 = cxx_qt_lib::QList<i32>;
    }

    unsafe extern "C++" {
        include!("native_helpers.h");
        fn grove_markdown_html(md: &QString, dark: bool, family: &QString, px: f64, base_dir: &QString, max_width: f64) -> QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(QString, path)]
        #[qproperty(bool, loading)]
        #[qproperty(QString, error)]
        #[qproperty(i32, lines)]
        /// The text's encoding when it isn't UTF-8 ("DOS (CP437)", "windows-1252"...).
        #[qproperty(QString, encoding)]
        #[qproperty(i32, max_cols, cxx_name = "maxCols")]
        #[qproperty(bool, rich_all, cxx_name = "richAll")]
        /// Markdown: the body font and the widest a picture may be.
        #[qproperty(QString, family)]
        #[qproperty(f64, pixel)]
        #[qproperty(f64, page_width, cxx_name = "pageWidth")]
        type TextDoc = super::TextDocRust;
    }

    impl cxx_qt::Threading for TextDoc {}

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginInsertRows"]
        unsafe fn begin_insert_rows(self: Pin<&mut TextDoc>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut TextDoc>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut TextDoc>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut TextDoc>);
        #[inherit]
        fn index(self: &TextDoc, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(self: Pin<&mut TextDoc>, top_left: &QModelIndex, bottom_right: &QModelIndex, roles: &QList_i32);
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &TextDoc, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &TextDoc) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &TextDoc, parent: &QModelIndex) -> i32;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        /// Read `path` as `mode` ("code", "markdown", "office", "plain");
        /// "" clears.
        #[qinvokable]
        fn load(self: Pin<&mut TextDoc>, path: &QString, mode: &QString, dark: bool);
    }
}

/// One block of the document: HTML (rich) or plain text.
pub struct Block {
    text: QString,
    rich: bool,
}

// QString is reference counted with atomics (implicit sharing): fine to
// hand over from the worker.
struct Batch(Vec<Block>);
unsafe impl Send for Batch {}

#[derive(Default)]
pub struct TextDocRust {
    path: QString,
    loading: bool,
    error: QString,
    lines: i32,
    encoding: QString,
    max_cols: i32,
    rich_all: bool,
    family: QString,
    pixel: f64,
    page_width: f64,
    blocks: Vec<Block>,
    gen: u64,
    cancel: Option<Arc<AtomicBool>>,
    /// A big file: only an index of where its blocks start; their text is
    /// read from disk when the list shows them.
    stream: Option<Stream>,
}

/// A streamed file: block starts (byte offsets) and line counts, the
/// coloured blocks at the top, and a small cache of blocks read on demand.
pub struct Stream {
    starts: Vec<u64>,
    lines: Vec<u32>,
    /// Where the last indexed block ends.
    end: u64,
    /// The coloured blocks at the top.
    rich: HashMap<usize, Block>,
    cache: RefCell<HashMap<usize, Block>>,
    order: RefCell<VecDeque<usize>>,
    pending: RefCell<HashSet<usize>>,
    tx: std::sync::mpsc::Sender<(usize, u64, u64)>,
}

const ROLES: [&str; 2] = ["text", "rich"];
/// Lines per block.
const BLOCK_LINES: usize = 200;
/// Syntax colouring stops here (it costs ~1 s per 100 000 lines); the
/// rest is plain text.
const HIGHLIGHT_LINES: usize = 20_000;
/// The most read of one file (smaller files only: bigger ones stream).
const READ_LIMIT: u64 = 1 << 30;
/// Code and plain text from this size are streamed: indexed on a worker,
/// each block read from disk when it comes on screen.
pub const STREAM_FROM: u64 = 16 << 20;
/// A streamed file colours this many lines at its top.
const STREAM_HIGHLIGHT_LINES: usize = 5_000;
/// Blocks kept in memory (besides the coloured ones).
const CACHE_BLOCKS: usize = 160;
/// A block ends at a line end once this big, or anywhere (at a character
/// boundary) once HARD_BLOCK_BYTES (one enormous line).
const SOFT_BLOCK_BYTES: u64 = 1 << 20;
const HARD_BLOCK_BYTES: u64 = 4 << 20;

fn qs(s: &str) -> QString {
    QString::from(s)
}

impl qobject::TextDoc {
    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        if let Some(st) = &self.rust().stream {
            return stream_data(st, index.row() as usize, role);
        }
        let Some(b) = self.rust().blocks.get(index.row() as usize) else { return QVariant::default() };
        match ROLES.get((role - 0x0100) as usize) {
            Some(&"text") => QVariant::from(&b.text),
            Some(&"rich") => QVariant::from(&b.rich),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut h = QHash::<QHashPair_i32_QByteArray>::default();
        for (i, n) in ROLES.iter().enumerate() {
            h.insert(0x0100 + i as i32, QByteArray::from(*n));
        }
        h
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        match &self.rust().stream {
            Some(st) => st.starts.len() as i32,
            None => self.rust().blocks.len() as i32,
        }
    }

    fn load(mut self: Pin<&mut Self>, path: &QString, mode: &QString, dark: bool) {
        if let Some(c) = self.as_mut().rust_mut().cancel.take() {
            c.store(true, Ordering::Relaxed);
        }
        unsafe {
            self.as_mut().begin_reset_model();
        }
        self.as_mut().rust_mut().blocks.clear();
        self.as_mut().rust_mut().stream = None;
        self.as_mut().rust_mut().gen += 1;
        unsafe {
            self.as_mut().end_reset_model();
        }
        self.as_mut().set_path(path.clone());
        self.as_mut().set_error(QString::default());
        self.as_mut().set_lines(0);
        self.as_mut().set_encoding(QString::default());
        self.as_mut().set_max_cols(0);
        let p = path.to_string();
        let mode = mode.to_string();
        self.as_mut().set_rich_all(mode == "markdown" || mode == "html");
        if p.is_empty() {
            self.as_mut().set_loading(false);
            return;
        }
        self.as_mut().set_loading(true);
        let cancel = Arc::new(AtomicBool::new(false));
        self.as_mut().rust_mut().cancel = Some(cancel.clone());
        let gen = self.rust().gen;
        let family = self.rust().family.clone().to_string();
        let px = if self.rust().pixel > 0.0 { self.rust().pixel } else { 14.0 };
        let width = self.rust().page_width;
        let qt = self.qt_thread();
        if mode == "code" || mode == "plain" {
            // Big files stream (the size is read on the worker; it hands
            // back to the whole-file path when the file is small).
            let qt2 = qt.clone();
            let p2 = p.clone();
            let cancel2 = cancel.clone();
            let mode2 = mode.clone();
            let big = std::thread::Builder::new().name("grove-textdoc".into()).spawn(move || {
                let size = std::fs::metadata(&p2).map(|m| m.len()).unwrap_or(0);
                if size < STREAM_FROM {
                    return false;
                }
                index_stream(qt2, gen, p2, mode2 == "code", dark, cancel2);
                true
            });
            // Small: the whole-file path below, on its own thread.
            let _ = big.map(move |h| {
                std::thread::spawn(move || {
                    if h.join().unwrap_or(true) {
                        return;
                    }
                    whole_file(qt, gen, p, mode, dark, family, px, width, cancel);
                })
            });
            return;
        }
        std::thread::Builder::new()
            .name("grove-textdoc".into())
            .spawn(move || whole_file(qt, gen, p, mode, dark, family, px, width, cancel))
            .ok();
    }
}

#[allow(clippy::too_many_arguments)]
fn whole_file(qt: cxx_qt::CxxQtThread<qobject::TextDoc>, gen: u64, p: String, mode: String, dark: bool, family: String, px: f64, width: f64, cancel: Arc<AtomicBool>) {
    {
        {
            (move || {
                let send = |blocks: Vec<Block>, lines: usize, cols: usize, done: bool, err: Option<String>| {
                    let b = Batch(blocks);
                    let _ = qt.queue(move |m| m.add_blocks(gen, b, lines, cols, done, err));
                };
                // Word/OpenDocument/RTF/PowerPoint: formatted (headings, bold,
                // lists, tables, pictures) through pandoc, drawn as a page.
                if mode == "office" {
                    if let Some(blocks) = office_html(&p, &cancel) {
                        if cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        let n = blocks.len();
                        let mut first = true;
                        let mut batch = vec![];
                        for b in blocks {
                            batch.push(Block { text: qs(&b), rich: true });
                            if first || batch.len() >= 30 {
                                first = false;
                                send(std::mem::take(&mut batch), n, 0, false, None);
                            }
                        }
                        return send(batch, n, 0, true, None);
                    }
                }
                let text = match read_text(&p, &mode, &cancel) {
                    Ok((t, enc)) => {
                        let _ = qt.queue(move |m| m.set_encoding(qs(&enc)));
                        t
                    }
                    Err(e) => return send(vec![], 0, 0, true, Some(e)),
                };
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let total_lines = text.lines().count();
                match mode.as_str() {
                    "markdown" => {
                        let dir = std::path::Path::new(&p).parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
                        let sections = markdown_sections(&text);
                        let mut batch = vec![];
                        let mut first = true;
                        for s in sections {
                            if cancel.load(Ordering::Relaxed) {
                                return;
                            }
                            let html = qobject::grove_markdown_html(&qs(&s), dark, &qs(&family), px, &qs(&dir), width);
                            batch.push(Block { text: html, rich: true });
                            if first || batch.len() >= 20 {
                                first = false;
                                send(std::mem::take(&mut batch), total_lines, 0, false, None);
                            }
                        }
                        send(batch, total_lines, 0, true, None);
                    }
                    "html" => {
                        let body = web_page_body(&text);
                        // Qt's default link blue is unreadable on dark.
                        let body = if dark { body.replace("<a ", "<a style=\"color:#4ea1ff\" ").replace("<A ", "<A style=\"color:#4ea1ff\" ") } else { body };
                        let blocks = html_blocks(&body);
                        let mut batch = vec![];
                        for b in blocks {
                            if cancel.load(Ordering::Relaxed) {
                                return;
                            }
                            batch.push(Block { text: b.into(), rich: true });
                            if batch.len() >= 20 {
                                send(std::mem::take(&mut batch), total_lines, 0, false, None);
                            }
                        }
                        send(batch, total_lines, 0, true, None);
                    }
                    "code" => {
                        let name = std::path::Path::new(&p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        let text = if text.contains('\t') { text.replace('\t', "    ") } else { text };
                        let cols = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
                        // No language from the name: guessed from the content (rules, then Magika).
                        let name = crate::langguess::name_for(&p, &text, &name);
                        let mut hl = crate::preview::Highlighter::new(&text, &name, dark);
                        let mut batch = vec![];
                        let mut first = true;
                        let mut chunk = String::new();
                        let mut n = 0;
                        let mut done_lines = 0;
                        let mut flush = |chunk: &mut String, done_lines: usize, batch: &mut Vec<Block>| {
                            let rich = done_lines <= HIGHLIGHT_LINES;
                            let body = std::mem::take(chunk);
                            let body = body.strip_suffix('\n').unwrap_or(&body).to_string();
                            batch.push(if rich { Block { text: qs(&hl.html(&body)), rich: true } } else { Block { text: qs(&body), rich: false } });
                        };
                        for line in text.split_inclusive('\n') {
                            chunk.push_str(line);
                            n += 1;
                            done_lines += 1;
                            if n == BLOCK_LINES {
                                n = 0;
                                flush(&mut chunk, done_lines, &mut batch);
                                if first || batch.len() >= 25 {
                                    if cancel.load(Ordering::Relaxed) {
                                        return;
                                    }
                                    first = false;
                                    send(std::mem::take(&mut batch), total_lines, cols, false, None);
                                }
                            }
                        }
                        if !chunk.is_empty() {
                            flush(&mut chunk, done_lines, &mut batch);
                        }
                        send(batch, total_lines, cols, true, None);
                    }
                    _ => {
                        // Plain prose / text: blocks of lines, wrapped by the view.
                        let mut batch = vec![];
                        let mut chunk = String::new();
                        let mut n = 0;
                        let mut first = true;
                        for line in text.split_inclusive('\n') {
                            chunk.push_str(line);
                            n += 1;
                            if n == BLOCK_LINES {
                                n = 0;
                                let body = std::mem::take(&mut chunk);
                                batch.push(Block { text: qs(body.strip_suffix('\n').unwrap_or(&body)), rich: false });
                                if first || batch.len() >= 50 {
                                    first = false;
                                    send(std::mem::take(&mut batch), total_lines, 0, false, None);
                                }
                            }
                        }
                        if !chunk.is_empty() {
                            batch.push(Block { text: qs(chunk.strip_suffix('\n').unwrap_or(&chunk)), rich: false });
                        }
                        send(batch, total_lines, 0, true, None);
                    }
                }
            })();
        }
    }
}

impl qobject::TextDoc {
    fn add_blocks(mut self: Pin<&mut Self>, gen: u64, batch: Batch, lines: usize, cols: usize, done: bool, err: Option<String>) {
        if gen != self.rust().gen {
            return;
        }
        let blocks = batch.0;
        if !blocks.is_empty() {
            let first = self.rust().blocks.len() as i32;
            let last = first + blocks.len() as i32 - 1;
            unsafe {
                self.as_mut().begin_insert_rows(&QModelIndex::default(), first, last);
            }
            self.as_mut().rust_mut().blocks.extend(blocks);
            unsafe {
                self.as_mut().end_insert_rows();
            }
        }
        self.as_mut().set_lines(lines.min(i32::MAX as usize) as i32);
        self.as_mut().set_max_cols(cols.min(i32::MAX as usize) as i32);
        if let Some(e) = err {
            self.as_mut().set_error(qs(&e));
        }
        if done {
            self.as_mut().set_loading(false);
        }
    }
}

/// An office document as HTML blocks (grove_core::officehtml: Word, ODT,
/// PowerPoint, RTF; no pandoc), pictures taken out to the cache. None for
/// formats it doesn't read (.doc, .xls).
fn office_html(p: &str, cancel: &AtomicBool) -> Option<Vec<String>> {
    let meta = std::fs::metadata(p).ok()?;
    let key = format!("{p}|{}|{}", meta.len(), std::os::unix::fs::MetadataExt::mtime(&meta));
    let media = grove_core::util::cache_home().join("grove/office-media").join(grove_core::thumbs::md5_hex(&key));
    let html = grove_core::officehtml::convert(std::path::Path::new(p), &media, cancel)?;
    if cancel.load(Ordering::Relaxed) || html.trim().is_empty() {
        return None;
    }
    // Tables get thin rules (Qt's rich text draws none by default).
    let html = html.replace("<table>", "<table border=\"1\" cellspacing=\"0\" cellpadding=\"5\" style=\"border-color:#c8c8c8; margin-top:6px; margin-bottom:6px\">");
    Some(html_blocks(&html))
}

/// A web page's body for Qt's rich text: scripts, styles, comments and the
/// head dropped (they'd show as text or do nothing), and a line break
/// before each block-level tag so it can be cut into blocks.
pub fn web_page_body(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    // Only what's inside <body> when there is one.
    let (start, end) = match lower.find("<body") {
        Some(b) => (lower[b..].find('>').map(|g| b + g + 1).unwrap_or(b), lower.rfind("</body>").unwrap_or(html.len())),
        None => (0, html.len()),
    };
    let (lower, html) = (&lower[start..end.max(start)], &html[start..end.max(start)]);
    while i < html.len() {
        let rest = &lower[i..];
        let skip_to = |open: &str, close: &str| -> Option<usize> {
            if rest.starts_with(open) {
                Some(rest.find(close).map(|e| i + e + close.len()).unwrap_or(html.len()))
            } else {
                None
            }
        };
        if let Some(n) = skip_to("<script", "</script>").or_else(|| skip_to("<style", "</style>")).or_else(|| skip_to("<!--", "-->")).or_else(|| skip_to("<head", "</head>")) {
            i = n;
            continue;
        }
        let ch = html[i..].chars().next().unwrap();
        if ch == '<' && ["<p", "<h1", "<h2", "<h3", "<h4", "<h5", "<h6", "<div", "<table", "<ul", "<ol", "<pre", "<blockquote", "<section", "<article", "<figure", "<hr"].iter().any(|t| rest.starts_with(t)) {
            out.push('\n');
        }
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Pandoc's HTML cut into blocks between its top-level elements (never
/// inside a table or a list), so long documents lay out a part at a time.
fn html_blocks(html: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut depth: i32 = 0;
    let mut pieces = 0;
    for line in html.lines() {
        let l = line.trim_start();
        let opens = ["<table", "<ul", "<ol", "<dl", "<blockquote", "<div", "<figure"].iter().map(|t| l.matches(t).count() as i32).sum::<i32>();
        let closes = ["</table", "</ul", "</ol", "</dl", "</blockquote", "</div", "</figure"].iter().map(|t| l.matches(t).count() as i32).sum::<i32>();
        if depth == 0 && pieces >= 12 && (l.starts_with("<h") || l.starts_with("<p")) {
            out.push(std::mem::take(&mut cur));
            pieces = 0;
        }
        cur.push_str(line);
        cur.push('\n');
        depth = (depth + opens - closes).max(0);
        if depth == 0 {
            pieces += 1;
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// The file's text, decoded from whatever it's in (UTF-8, UTF-16, DOS
/// CP437, Windows-1252, Shift_JIS...: grove_core::textenc), and the
/// encoding's name ("" for UTF-8).
fn read_text(p: &str, mode: &str, cancel: &AtomicBool) -> Result<(String, String), String> {
    use std::io::Read;
    let path = std::path::Path::new(p);
    if mode == "office" {
        return grove_core::previews::office_text(path, usize::MAX, cancel).map(|t| (t, String::new())).ok_or_else(|| "No text in this document".to_string());
    }
    let f = std::fs::File::open(path).map_err(|e| grove_core::util::plain_error(&e, path))?;
    let mut buf = vec![];
    f.take(READ_LIMIT).read_to_end(&mut buf).map_err(|e| grove_core::util::plain_error(&e, path))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (text, enc) = grove_core::textenc::read(&buf, &name);
    Ok((text, enc.label()))
}

/// Markdown cut into sections at blank lines outside code fences (about
/// 80 lines each, so no one block is slow to lay out). Reference-style
/// link definitions go with every section so links still resolve.
pub fn markdown_sections(text: &str) -> Vec<String> {
    let defs: String = text
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            l.len() - t.len() <= 3 && t.starts_with('[') && t.contains("]:") && !t.starts_with("[^")
        })
        .map(|l| format!("{l}\n"))
        .collect();
    let mut out = vec![];
    let mut cur = String::new();
    let mut n = 0;
    let mut fence: Option<String> = None;
    for line in text.lines() {
        let t = line.trim_start();
        if let Some(f) = &fence {
            if t.starts_with(f.as_str()) {
                fence = None;
            }
        } else if t.starts_with("```") || t.starts_with("~~~") {
            fence = Some(t[..3].to_string());
        }
        cur.push_str(line);
        cur.push('\n');
        n += 1;
        if fence.is_none() && line.trim().is_empty() && n >= 80 {
            out.push(std::mem::take(&mut cur));
            n = 0;
        }
    }
    if !cur.trim().is_empty() || out.is_empty() {
        out.push(cur);
    }
    if out.len() > 1 && !defs.is_empty() {
        for s in &mut out {
            s.push('\n');
            s.push_str(&defs);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn sections_keep_fences_whole() {
        let mut md = String::new();
        for i in 0..100 {
            md.push_str(&format!("line {i}\n\n"));
        }
        md.push_str("```\na\n\nb\n```\n");
        let s = super::markdown_sections(&md);
        assert!(s.len() >= 2);
        assert!(s.iter().any(|x| x.contains("```\na\n\nb\n```")));
        assert_eq!(s.concat().matches("line ").count(), 100);
    }
}

// ---- Streaming big files ----

/// What the indexer found since its last message: new blocks (start,
/// lines), coloured top blocks, totals.
struct IndexBatch {
    starts: Vec<u64>,
    lines: Vec<u32>,
    end: u64,
    total_lines: usize,
    cols: usize,
    done: bool,
    err: Option<String>,
}
unsafe impl Send for IndexBatch {}

struct ReadyBlock(Block);
unsafe impl Send for ReadyBlock {}

fn placeholder(lines: u32) -> QString {
    // As many (empty) lines as the block has, so the list's height is
    // right before its text arrives.
    qs(&"\n".repeat(lines.saturating_sub(1) as usize))
}

fn stream_data(st: &Stream, row: usize, role: i32) -> QVariant {
    let Some(&start) = st.starts.get(row) else { return QVariant::default() };
    let name = ROLES.get((role - 0x0100) as usize).copied();
    if let Some(b) = st.rich.get(&row) {
        return match name {
            Some("text") => QVariant::from(&b.text),
            Some("rich") => QVariant::from(&b.rich),
            _ => QVariant::default(),
        };
    }
    if let Some(b) = st.cache.borrow().get(&row) {
        return match name {
            Some("text") => QVariant::from(&b.text),
            Some("rich") => QVariant::from(&false),
            _ => QVariant::default(),
        };
    }
    if name == Some("rich") {
        return QVariant::from(&false);
    }
    if name != Some("text") {
        return QVariant::default();
    }
    if st.pending.borrow_mut().insert(row) {
        let end = st.starts.get(row + 1).copied().unwrap_or(st.end);
        let _ = st.tx.send((row, start, end));
    }
    QVariant::from(&placeholder(st.lines[row]))
}

impl qobject::TextDoc {
    fn stream_begin(mut self: Pin<&mut Self>, gen: u64, tx: std::sync::mpsc::Sender<(usize, u64, u64)>) {
        if gen != self.rust().gen {
            return;
        }
        unsafe {
            self.as_mut().begin_reset_model();
        }
        self.as_mut().rust_mut().stream = Some(Stream {
            starts: vec![],
            lines: vec![],
            end: 0,
            rich: HashMap::new(),
            cache: RefCell::new(HashMap::new()),
            order: RefCell::new(VecDeque::new()),
            pending: RefCell::new(HashSet::new()),
            tx,
        });
        unsafe {
            self.as_mut().end_reset_model();
        }
    }

    fn stream_add(mut self: Pin<&mut Self>, gen: u64, b: IndexBatch) {
        if gen != self.rust().gen || self.rust().stream.is_none() {
            return;
        }
        let have = self.rust().stream.as_ref().map(|s| s.starts.len()).unwrap_or(0);
        if !b.starts.is_empty() {
            let first = have as i32;
            let last = (have + b.starts.len()) as i32 - 1;
            unsafe {
                self.as_mut().begin_insert_rows(&QModelIndex::default(), first, last);
            }
            {
                let mut r = self.as_mut().rust_mut();
                let st = r.stream.as_mut().unwrap();
                st.starts.extend(b.starts);
                st.lines.extend(b.lines);
                st.end = b.end;
            }
            unsafe {
                self.as_mut().end_insert_rows();
            }
        } else if let Some(st) = self.as_mut().rust_mut().stream.as_mut() {
            st.end = b.end;
        }
        self.as_mut().set_lines(b.total_lines.min(i32::MAX as usize) as i32);
        self.as_mut().set_max_cols(b.cols.min(i32::MAX as usize) as i32);
        if let Some(e) = b.err {
            self.as_mut().set_error(qs(&e));
        }
        if b.done {
            self.as_mut().set_loading(false);
        }
    }

    fn stream_rich(mut self: Pin<&mut Self>, gen: u64, row: usize, b: ReadyBlock) {
        if gen != self.rust().gen {
            return;
        }
        match self.as_mut().rust_mut().stream.as_mut() {
            Some(st) => {
                st.rich.insert(row, b.0);
            }
            None => return,
        }
        let idx = self.index(row as i32, 0, &QModelIndex::default());
        self.as_mut().data_changed(&idx, &idx, &QList::<i32>::default());
    }

    fn stream_block(mut self: Pin<&mut Self>, gen: u64, row: usize, b: ReadyBlock) {
        if gen != self.rust().gen {
            return;
        }
        {
            let r = self.rust();
            let Some(st) = r.stream.as_ref() else { return };
            st.pending.borrow_mut().remove(&row);
            let mut cache = st.cache.borrow_mut();
            let mut order = st.order.borrow_mut();
            cache.insert(row, b.0);
            order.push_back(row);
            while order.len() > CACHE_BLOCKS {
                if let Some(old) = order.pop_front() {
                    cache.remove(&old);
                }
            }
        }
        let idx = self.index(row as i32, 0, &QModelIndex::default());
        self.as_mut().data_changed(&idx, &idx, &QList::<i32>::default());
    }
}

/// Index a big file's blocks (byte offsets of every BLOCK_LINES lines) in
/// one pass of big reads, the first block at once; colour the top; answer
/// block reads from a second thread.
fn index_stream(qt: cxx_qt::CxxQtThread<qobject::TextDoc>, gen: u64, p: String, code: bool, dark: bool, cancel: Arc<AtomicBool>) {
    use std::io::{Read, Seek, SeekFrom};
    let path = std::path::Path::new(&p);
    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            let err = grove_core::util::plain_error(&e, path);
            let _ = qt.queue(move |m| m.add_blocks(gen, Batch(vec![]), 0, 0, true, Some(err)));
            return;
        }
    };
    // The encoding, from the start of the file (every block uses it).
    let enc = {
        let mut head = vec![0u8; 1 << 16];
        let n = f.read(&mut head).unwrap_or(0);
        head.truncate(n);
        let _ = f.seek(SeekFrom::Start(0));
        grove_core::textenc::detect(&head, &path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
    };
    {
        let label = enc.label();
        let _ = qt.queue(move |m| m.set_encoding(qs(&label)));
    }
    // The block reader: one request at a time, newest first.
    let (tx, rx) = std::sync::mpsc::channel::<(usize, u64, u64)>();
    {
        let qt = qt.clone();
        let p = p.clone();
        let cancel = cancel.clone();
        std::thread::Builder::new()
            .name("grove-textdoc-read".into())
            .spawn(move || {
                let Ok(mut f) = std::fs::File::open(&p) else { return };
                while let Ok(first) = rx.recv() {
                    let mut reqs = vec![first];
                    while let Ok(r) = rx.try_recv() {
                        reqs.push(r);
                    }
                    for (row, start, end) in reqs.into_iter().rev() {
                        if cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        let mut buf = vec![0u8; end.saturating_sub(start) as usize];
                        let ok = f.seek(SeekFrom::Start(start)).and_then(|_| f.read_exact(&mut buf)).is_ok();
                        if !ok {
                            buf.clear();
                        }
                        let text = block_text(&buf, code, enc);
                        let b = ReadyBlock(Block { text: qs(&text), rich: false });
                        let _ = qt.queue(move |m| m.stream_block(gen, row, b));
                    }
                }
            })
            .ok();
    }
    let _ = qt.queue(move |m| m.stream_begin(gen, tx));

    // Colouring the top: a thread of its own (syntect is slow on some
    // syntaxes), so the plain text shows at once and turns coloured.
    let (ctx, crx) = std::sync::mpsc::channel::<(usize, u64, u64)>();
    if code {
        let qt = qt.clone();
        let p = p.clone();
        let cancel = cancel.clone();
        std::thread::Builder::new()
            .name("grove-textdoc-colour".into())
            .spawn(move || {
                let Ok(mut f) = std::fs::File::open(&p) else { return };
                let mut head = vec![0u8; 65536];
                let n = f.read(&mut head).unwrap_or(0);
                head.truncate(n);
                let path = std::path::Path::new(&p);
                let head = grove_core::textenc::decode(&head, enc);
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let name = crate::langguess::name_for(&p, &head, &name);
                let mut hl = crate::preview::Highlighter::new(&head, &name, dark);
                while let Ok((row, start, end)) = crx.recv() {
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    let mut buf = vec![0u8; end.saturating_sub(start) as usize];
                    if f.seek(SeekFrom::Start(start)).and_then(|_| f.read_exact(&mut buf)).is_err() {
                        return;
                    }
                    let html = hl.html(&block_text(&buf, true, enc));
                    let b = ReadyBlock(Block { text: qs(&html), rich: true });
                    let _ = qt.queue(move |m| m.stream_rich(gen, row, b));
                }
            })
            .ok();
    }

    // The first read small (the first lines show at once), then big ones.
    let mut buf = vec![0u8; 4 << 20];
    let mut first_read = 256 << 10;
    let mut pos: u64 = 0; // file offset of buf[0]
    let mut block_start: u64 = 0;
    let mut block_lines: u32 = 0;
    let mut line_start: u64 = 0;
    let mut total_lines: usize = 0;
    let mut cols: usize = 0;
    let mut rows = 0usize;
    let mut coloured = 0usize;
    let mut batch = IndexBatch { starts: vec![], lines: vec![], end: 0, total_lines: 0, cols: 0, done: false, err: None };
    let mut last_send = std::time::Instant::now();
    let mut sent_first = false;

    // A block ends at `end` (exclusive) with `lines` lines.
    let mut finish = |end: u64, lines: u32, batch: &mut IndexBatch, block_start: &mut u64| {
        if end <= *block_start {
            return;
        }
        batch.starts.push(*block_start);
        batch.lines.push(lines.max(1));
        batch.end = end;
        if code && coloured < STREAM_HIGHLIGHT_LINES {
            let _ = ctx.send((rows, *block_start, end));
            coloured += lines as usize;
        }
        rows += 1;
        *block_start = end;
    };

    loop {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let want = if first_read > 0 { std::mem::take(&mut first_read) } else { buf.len() };
        let n = match f.read(&mut buf[..want]) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                batch.err = Some(grove_core::util::plain_error(&e, path));
                break;
            }
        };
        let chunk = &buf[..n];
        let mut i = 0usize;
        while i < n {
            match memchr::memchr(b'\n', &chunk[i..]) {
                Some(k) => {
                    let nl = i + k;
                    let abs_end = pos + nl as u64 + 1;
                    cols = cols.max((abs_end - 1 - line_start) as usize);
                    line_start = abs_end;
                    total_lines += 1;
                    block_lines += 1;
                    if block_lines as usize >= BLOCK_LINES || abs_end - block_start >= SOFT_BLOCK_BYTES {
                        finish(abs_end, block_lines, &mut batch, &mut block_start);
                        block_lines = 0;
                    }
                    i = nl + 1;
                }
                None => i = n,
            }
            // One enormous line: cut it (at a character boundary).
            let here = pos + i as u64;
            if here - block_start >= HARD_BLOCK_BYTES {
                let mut cut = i;
                while cut > 0 && cut < n && (chunk[cut] & 0xC0) == 0x80 {
                    cut -= 1;
                }
                let cut_abs = pos + cut as u64;
                if cut_abs > block_start {
                    finish(cut_abs, block_lines + 1, &mut batch, &mut block_start);
                    block_lines = 0;
                }
            }
        }
        pos += n as u64;
        if (!sent_first && !batch.starts.is_empty()) || last_send.elapsed().as_millis() >= 100 {
            sent_first = true;
            last_send = std::time::Instant::now();
            batch.total_lines = total_lines;
            batch.cols = cols;
            let b = std::mem::replace(&mut batch, IndexBatch { starts: vec![], lines: vec![], end: block_start, total_lines: 0, cols: 0, done: false, err: None });
            let _ = qt.queue(move |m| m.stream_add(gen, b));
        }
    }
    // The last block (a last line without a newline counts).
    if pos > line_start {
        total_lines += 1;
        cols = cols.max((pos - line_start) as usize);
        block_lines += 1;
    }
    finish(pos, block_lines, &mut batch, &mut block_start);
    batch.total_lines = total_lines;
    batch.cols = cols;
    batch.done = true;
    let _ = qt.queue(move |m| m.stream_add(gen, batch));
}

/// A block's bytes as display text (in the file's encoding): no final
/// newline, tabs as spaces (code).
fn block_text(bytes: &[u8], code: bool, enc: grove_core::textenc::Enc) -> String {
    let s = grove_core::textenc::decode(bytes, enc);
    let s = s.strip_suffix('\n').unwrap_or(&s);
    let s = s.strip_suffix('\r').unwrap_or(s);
    if code && s.contains('\t') {
        s.replace('\t', "    ")
    } else {
        s.to_string()
    }
}
