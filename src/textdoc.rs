//! TextDoc: a whole text file for Quick Look, in blocks a ListView shows
//! lazily (only the blocks on screen are laid out, so a 100 MB log scrolls
//! like a short one). Read, highlighted and rendered on a worker thread;
//! blocks arrive in batches, the first at once.
//!
//! Modes: "code" (syntax-coloured HTML for the first lines, plain text
//! after), "markdown" (rendered with QTextDocument's reader, styled by
//! grove_markdown_html), "office" (an office document's text), "plain".

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
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
    max_cols: i32,
    rich_all: bool,
    family: QString,
    pixel: f64,
    page_width: f64,
    blocks: Vec<Block>,
    gen: u64,
    cancel: Option<Arc<AtomicBool>>,
}

const ROLES: [&str; 2] = ["text", "rich"];
/// Lines per block.
const BLOCK_LINES: usize = 200;
/// Syntax colouring stops here (it costs ~1 s per 100 000 lines); the
/// rest is plain text.
const HIGHLIGHT_LINES: usize = 20_000;
/// The most read of one file.
const READ_LIMIT: u64 = 1 << 30;

fn qs(s: &str) -> QString {
    QString::from(s)
}

impl qobject::TextDoc {
    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
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
        self.rust().blocks.len() as i32
    }

    fn load(mut self: Pin<&mut Self>, path: &QString, mode: &QString, dark: bool) {
        if let Some(c) = self.as_mut().rust_mut().cancel.take() {
            c.store(true, Ordering::Relaxed);
        }
        unsafe {
            self.as_mut().begin_reset_model();
        }
        self.as_mut().rust_mut().blocks.clear();
        self.as_mut().rust_mut().gen += 1;
        unsafe {
            self.as_mut().end_reset_model();
        }
        self.as_mut().set_path(path.clone());
        self.as_mut().set_error(QString::default());
        self.as_mut().set_lines(0);
        self.as_mut().set_max_cols(0);
        let p = path.to_string();
        let mode = mode.to_string();
        self.as_mut().set_rich_all(mode == "markdown");
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
        std::thread::Builder::new()
            .name("grove-textdoc".into())
            .spawn(move || {
                let send = |blocks: Vec<Block>, lines: usize, cols: usize, done: bool, err: Option<String>| {
                    let b = Batch(blocks);
                    let _ = qt.queue(move |m| m.add_blocks(gen, b, lines, cols, done, err));
                };
                let text = match read_text(&p, &mode, &cancel) {
                    Ok(t) => t,
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
                    "code" => {
                        let name = std::path::Path::new(&p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        let text = if text.contains('\t') { text.replace('\t', "    ") } else { text };
                        let cols = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
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
            })
            .ok();
    }

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

/// The file's text (an office document's text for "office").
fn read_text(p: &str, mode: &str, cancel: &AtomicBool) -> Result<String, String> {
    use std::io::Read;
    let path = std::path::Path::new(p);
    if mode == "office" {
        return grove_core::previews::office_text(path, usize::MAX, cancel).ok_or_else(|| "No text in this document".to_string());
    }
    let f = std::fs::File::open(path).map_err(|e| grove_core::util::plain_error(&e, path))?;
    let mut buf = vec![];
    f.take(READ_LIMIT).read_to_end(&mut buf).map_err(|e| grove_core::util::plain_error(&e, path))?;
    Ok(match String::from_utf8(buf) {
        Ok(s) => s,
        Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
    })
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
