//! DiffDoc: a patch or diff as rows for the viewer's diff view (DocView's
//! Unified / Side by Side / Source switch), as delta shows them: each file
//! under a header with its +/− counts, each hunk under the function it's in,
//! lines on red and green with their old and new line numbers, the words
//! that changed within a changed line marked stronger, and the code
//! syntax-coloured for its file's language. Read and laid out on a worker
//! (grove_core::patch parses); both layouts are made at once, so switching
//! between them is instant.

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use grove_core::patch::{self, LineKind, Part, Status};
use std::ops::Range;
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

    unsafe extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(QString, path)]
        /// Side by side (old on the left, new on the right), else unified.
        #[qproperty(bool, split)]
        #[qproperty(bool, loading)]
        #[qproperty(QString, error)]
        /// How many files it changes, and the lines added and removed.
        #[qproperty(i32, files)]
        #[qproperty(i32, added)]
        #[qproperty(i32, removed)]
        /// The biggest line number (the gutter's width).
        #[qproperty(i32, max_number, cxx_name = "maxNumber")]
        type DiffDoc = super::DiffDocRust;
    }

    impl cxx_qt::Threading for DiffDoc {}

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut DiffDoc>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut DiffDoc>);
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &DiffDoc, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &DiffDoc) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &DiffDoc, parent: &QModelIndex) -> i32;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        /// Read `path` ("" clears), coloured for a dark or light window.
        #[qinvokable]
        fn load(self: Pin<&mut DiffDoc>, path: &QString, dark: bool);
        /// Whether a file is a patch by its name (.patch, .diff, .debdiff, .rej).
        #[qinvokable]
        fn is_patch_name(self: &DiffDoc, name: &QString) -> bool;
        /// The row's text as it is in the files (for copying): `side` "old",
        /// "new" or "" (unified: the line itself).
        #[qinvokable]
        fn plain(self: &DiffDoc, row: i32, side: &QString) -> QString;
        /// Put that text on the clipboard.
        #[qinvokable]
        fn copy(self: &DiffDoc, row: i32, side: &QString);
    }

    impl cxx_qt::Initialize for DiffDoc {}
}

/// What a row is.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RowKind {
    /// Text outside the files (a mail's headers and message, a diffstat).
    Text,
    File,
    Hunk,
    Line,
}

/// A side of a line: nothing there (split view's gap), unchanged, removed, added.
pub const NONE: i32 = 0;
pub const CONTEXT: i32 = 1;
pub const REMOVED: i32 = 2;
pub const ADDED: i32 = 3;

#[derive(Clone, Debug)]
pub struct Row {
    pub kind: RowKind,
    pub old_no: i32,
    pub new_no: i32,
    /// Line rows: coloured HTML of the line (unified), or of each side.
    /// Text rows: the text. File rows: the name. Hunk rows: the function.
    pub left: String,
    pub right: String,
    pub left_type: i32,
    pub right_type: i32,
    /// File rows: what happened ("Renamed from a.rs · +3 −1"). Hunk rows: "line 12".
    pub sub: String,
    /// The plain text of each side (copying).
    pub left_plain: String,
    pub right_plain: String,
}

impl Row {
    fn new(kind: RowKind) -> Row {
        Row { kind, old_no: 0, new_no: 0, left: String::new(), right: String::new(), left_type: NONE, right_type: NONE, sub: String::new(), left_plain: String::new(), right_plain: String::new() }
    }
}

#[derive(Default)]
pub struct DiffDocRust {
    path: QString,
    split: bool,
    loading: bool,
    error: QString,
    files: i32,
    added: i32,
    removed: i32,
    max_number: i32,
    unified: Vec<Row>,
    side: Vec<Row>,
    gen: u64,
    cancel: Option<Arc<AtomicBool>>,
}

impl cxx_qt::Initialize for qobject::DiffDoc {
    fn initialize(self: Pin<&mut Self>) {
        // Switching layouts: the other set of rows.
        self.on_split_changed(|mut m| {
            unsafe { m.as_mut().begin_reset_model() };
            unsafe { m.as_mut().end_reset_model() };
        })
        .release();
    }
}

const ROLES: [&str; 9] = ["kind", "oldNo", "newNo", "left", "right", "leftType", "rightType", "sub", "hasPlain"];

fn qs(s: &str) -> QString {
    QString::from(s)
}

/// Lines beyond this many aren't syntax-coloured (only red and green):
/// colouring is the slow part.
const COLOUR_LIMIT: usize = 200_000;

/// Delta's colours: the line's own background is drawn by the view; these
/// mark the words that changed within it.
fn emph_colour(dark: bool, added: bool) -> &'static str {
    match (dark, added) {
        (true, false) => "#901011",
        (true, true) => "#006000",
        (false, false) => "#ffb3b3",
        (false, true) => "#9ae99a",
    }
}

fn esc_into(out: &mut String, s: &str, col: &mut usize) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            // Tabs to the next stop of 4 (rich text has none of its own).
            '\t' => {
                let n = 4 - *col % 4;
                for _ in 0..n {
                    out.push(' ');
                }
                *col += n;
                continue;
            }
            c => out.push(c),
        }
        *col += 1;
    }
}

/// Words, runs of spaces and single other characters, as byte ranges.
fn tokens(s: &str) -> Vec<Range<usize>> {
    let mut out = vec![];
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        let word = c.is_alphanumeric() || c == '_';
        let space = c.is_whitespace();
        let mut end = i + c.len_utf8();
        if word || space {
            while let Some(&(j, d)) = it.peek() {
                if (word && (d.is_alphanumeric() || d == '_')) || (space && d.is_whitespace()) {
                    end = j + d.len_utf8();
                    it.next();
                } else {
                    break;
                }
            }
        }
        out.push(i..end);
    }
    out
}

/// The parts of a removed line and the added line that replaced it that
/// differ, when the two are alike enough for that to help (else None: the
/// whole line changed).
pub fn word_changes(old: &str, new: &str) -> Option<(Vec<Range<usize>>, Vec<Range<usize>>)> {
    if old.len() > 4000 || new.len() > 4000 || old == new {
        return None;
    }
    let (ta, tb) = (tokens(old), tokens(new));
    let wa: Vec<&str> = ta.iter().map(|r| &old[r.clone()]).collect();
    let wb: Vec<&str> = tb.iter().map(|r| &new[r.clone()]).collect();
    let ops = similar::capture_diff_slices(similar::Algorithm::Myers, &wa, &wb);
    let (mut same, mut ra, mut rb) = (0usize, vec![], vec![]);
    let span = |t: &[Range<usize>], r: Range<usize>| -> Option<Range<usize>> {
        if r.is_empty() { None } else { Some(t[r.start].start..t[r.end - 1].end) }
    };
    for op in ops {
        let (tag, a, b) = op.as_tag_tuple();
        if tag == similar::DiffTag::Equal {
            same += a.clone().map(|k| wa[k].len()).sum::<usize>();
            continue;
        }
        ra.extend(span(&ta, a));
        rb.extend(span(&tb, b));
    }
    // Mostly different (delta's "max line distance" 0.6): no marks.
    let total = old.trim().len() + new.trim().len();
    if total == 0 || (2 * same) as f64 / ((old.len() + new.len()) as f64) < 0.4 {
        return None;
    }
    Some((ra, rb))
}

/// One side's syntax colouring through a file, line after line.
struct Side {
    h: Option<syntect::easy::HighlightLines<'static>>,
    fg: syntect::highlighting::Color,
}

impl Side {
    fn new(name: &str, sample: &str, dark: bool, colour: bool) -> Side {
        let theme = &crate::preview::themes().themes[if dark { "base16-ocean.dark" } else { "InspiredGitHub" }];
        let fg = theme.settings.foreground.unwrap_or(syntect::highlighting::Color::BLACK);
        let h = colour.then(|| syntect::easy::HighlightLines::new(crate::preview::syntax_for(crate::preview::syntaxes(), sample, name), theme));
        Side { h, fg }
    }

    /// The line as HTML, `marks` (byte ranges) on `mark`'s background.
    fn html(&mut self, text: &str, marks: &[Range<usize>], mark: &str) -> String {
        let with_nl = format!("{text}\n");
        let regions: Vec<(syntect::highlighting::Color, &str)> = match self.h.as_mut().and_then(|h| h.highlight_line(&with_nl, crate::preview::syntaxes()).ok()) {
            Some(r) => r.into_iter().map(|(st, p)| (st.foreground, p)).collect(),
            None => vec![(self.fg, with_nl.as_str())],
        };
        let mut out = String::with_capacity(text.len() * 2 + 32);
        let (mut at, mut col) = (0usize, 0usize);
        for (c, piece) in regions {
            let piece = piece.strip_suffix('\n').unwrap_or(piece);
            let end = at + piece.len();
            // Cut where a mark starts or ends.
            let mut cuts = vec![at, end];
            for m in marks {
                for b in [m.start, m.end] {
                    if b > at && b < end && piece.is_char_boundary(b - at) {
                        cuts.push(b);
                    }
                }
            }
            cuts.sort_unstable();
            cuts.dedup();
            for w in cuts.windows(2) {
                let (s, e) = (w[0], w[1]);
                if s == e {
                    continue;
                }
                let marked = marks.iter().any(|m| m.start <= s && e <= m.end);
                out.push_str(&format!("<span style=\"color:#{:02x}{:02x}{:02x}", c.r, c.g, c.b));
                if marked {
                    out.push_str(";background-color:");
                    out.push_str(mark);
                }
                out.push_str("\">");
                esc_into(&mut out, &piece[s - at..e - at], &mut col);
                out.push_str("</span>");
            }
            at = end;
        }
        out
    }
}

fn no_newline(html: &mut String, dark: bool) {
    html.push_str(&format!("<span style=\"color:{}\">  ⊘ no newline at end</span>", if dark { "#8e8e93" } else { "#6e6e73" }));
}

/// What a file's header says under its name.
fn file_sub(f: &patch::FilePatch) -> String {
    let mut bits: Vec<String> = vec![];
    match f.status {
        Status::Added => bits.push("New file".into()),
        Status::Deleted => bits.push("Deleted".into()),
        Status::Renamed if !f.old.is_empty() && f.old != f.new => bits.push(format!("Renamed from {}", f.old)),
        Status::Copied => bits.push(format!("Copied from {}", f.old)),
        _ => {}
    }
    if f.binary {
        bits.push("Binary".into());
    }
    for n in &f.notes {
        if n.starts_with("old mode") || n.starts_with("new mode") {
            bits.push(n.clone());
        }
    }
    let (a, r) = f.counts();
    if a + r > 0 {
        bits.push(format!("+{a} −{r}"));
    }
    bits.join(" · ")
}

pub struct Built {
    pub unified: Vec<Row>,
    pub side: Vec<Row>,
    pub files: i32,
    pub added: i32,
    pub removed: i32,
    pub max_number: i32,
}

pub fn build(parts: &[Part], dark: bool, cancel: &AtomicBool) -> Built {
    let total: usize = parts.iter().map(|p| if let Part::File(f) = p { f.hunks.iter().map(|h| h.lines.len()).sum() } else { 0 }).sum();
    let colour = total <= COLOUR_LIMIT;
    let mut b = Built { unified: vec![], side: vec![], files: 0, added: 0, removed: 0, max_number: 1 };
    for part in parts {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let f = match part {
            Part::Text(lines) => {
                let mut r = Row::new(RowKind::Text);
                let text = lines.join("\n");
                let mut col = 0;
                esc_into(&mut r.left, text.trim_end(), &mut col);
                r.left_plain = text.trim_end().to_string();
                b.unified.push(r.clone());
                b.side.push(r);
                continue;
            }
            Part::File(f) => f,
        };
        b.files += 1;
        let (a, rm) = f.counts();
        b.added += a as i32;
        b.removed += rm as i32;
        let mut head = Row::new(RowKind::File);
        head.left = f.name().to_string();
        head.left_plain = head.left.clone();
        head.sub = file_sub(f);
        b.unified.push(head.clone());
        b.side.push(head);
        // Each side's colouring runs on through the file's hunks (as delta does).
        let sample: String = f.hunks.iter().flat_map(|h| &h.lines).take(40).map(|l| format!("{}\n", l.text)).collect();
        let old_name = if f.old.is_empty() { f.name() } else { &f.old };
        let mut old = Side::new(old_name, &sample, dark, colour);
        let mut new = Side::new(f.name(), &sample, dark, colour);
        let (mark_old, mark_new) = (emph_colour(dark, false), emph_colour(dark, true));
        for h in &f.hunks {
            let mut hr = Row::new(RowKind::Hunk);
            if !h.context.is_empty() {
                hr.left = Side::new(f.name(), &h.context, dark, colour).html(&h.context, &[], "");
                hr.left_plain = h.context.clone();
            }
            hr.sub = format!("line {}", h.new_start);
            hr.new_no = h.new_start as i32;
            hr.old_no = h.old_start as i32;
            b.unified.push(hr.clone());
            b.side.push(hr);
            let ls = &h.lines;
            // The changed words of each removed line and the added line in
            // its place (the first removed with the first added, ...).
            let mut marks: Vec<Vec<Range<usize>>> = vec![vec![]; ls.len()];
            let mut i = 0;
            while i < ls.len() {
                if ls[i].kind == LineKind::Context {
                    i += 1;
                    continue;
                }
                let mut j = i;
                while j < ls.len() && ls[j].kind != LineKind::Context {
                    j += 1;
                }
                let dels: Vec<usize> = (i..j).filter(|&k| ls[k].kind == LineKind::Removed).collect();
                let adds: Vec<usize> = (i..j).filter(|&k| ls[k].kind == LineKind::Added).collect();
                for (&d, &a) in dels.iter().zip(&adds) {
                    if let Some((md, ma)) = word_changes(&ls[d].text, &ls[a].text) {
                        marks[d] = md;
                        marks[a] = ma;
                    }
                }
                i = j;
            }
            // Each line coloured on its side(s), numbered.
            let (mut on, mut nn) = (h.old_start as i32, h.new_start as i32);
            let mut cells: Vec<(i32, i32, Option<String>, Option<String>)> = Vec::with_capacity(ls.len());
            for (k, l) in ls.iter().enumerate() {
                let (mut oh, mut nh) = (None, None);
                let (mut o, mut n) = (0, 0);
                match l.kind {
                    LineKind::Context => {
                        oh = Some(old.html(&l.text, &[], ""));
                        nh = Some(new.html(&l.text, &[], ""));
                        (o, n) = (on, nn);
                        on += 1;
                        nn += 1;
                    }
                    LineKind::Removed => {
                        oh = Some(old.html(&l.text, &marks[k], mark_old));
                        o = on;
                        on += 1;
                    }
                    LineKind::Added => {
                        nh = Some(new.html(&l.text, &marks[k], mark_new));
                        n = nn;
                        nn += 1;
                    }
                }
                if l.no_newline {
                    for x in [oh.as_mut(), nh.as_mut()].into_iter().flatten() {
                        no_newline(x, dark);
                    }
                }
                cells.push((o, n, oh, nh));
            }
            b.max_number = b.max_number.max(on).max(nn);
            // Unified: in the patch's order.
            for (k, (o, n, oh, nh)) in cells.iter().enumerate() {
                let mut r = Row::new(RowKind::Line);
                r.old_no = *o;
                r.new_no = *n;
                r.left_type = match ls[k].kind {
                    LineKind::Context => CONTEXT,
                    LineKind::Removed => REMOVED,
                    LineKind::Added => ADDED,
                };
                r.left = nh.clone().or_else(|| oh.clone()).unwrap_or_default();
                r.left_plain = ls[k].text.clone();
                b.unified.push(r);
            }
            // Side by side: unchanged lines on both sides; in a change, the
            // removed lines down the left beside the added ones on the right.
            let mut i = 0;
            while i < ls.len() {
                if ls[i].kind == LineKind::Context {
                    let (o, n, oh, nh) = &cells[i];
                    let mut r = Row::new(RowKind::Line);
                    (r.old_no, r.new_no) = (*o, *n);
                    (r.left_type, r.right_type) = (CONTEXT, CONTEXT);
                    r.left = oh.clone().unwrap_or_default();
                    r.right = nh.clone().unwrap_or_default();
                    r.left_plain = ls[i].text.clone();
                    r.right_plain = ls[i].text.clone();
                    b.side.push(r);
                    i += 1;
                    continue;
                }
                let mut j = i;
                while j < ls.len() && ls[j].kind != LineKind::Context {
                    j += 1;
                }
                let dels: Vec<usize> = (i..j).filter(|&k| ls[k].kind == LineKind::Removed).collect();
                let adds: Vec<usize> = (i..j).filter(|&k| ls[k].kind == LineKind::Added).collect();
                for x in 0..dels.len().max(adds.len()) {
                    let mut r = Row::new(RowKind::Line);
                    if let Some(&d) = dels.get(x) {
                        r.old_no = cells[d].0;
                        r.left_type = REMOVED;
                        r.left = cells[d].2.clone().unwrap_or_default();
                        r.left_plain = ls[d].text.clone();
                    }
                    if let Some(&a) = adds.get(x) {
                        r.new_no = cells[a].1;
                        r.right_type = ADDED;
                        r.right = cells[a].3.clone().unwrap_or_default();
                        r.right_plain = ls[a].text.clone();
                    }
                    b.side.push(r);
                }
                i = j;
            }
        }
    }
    b
}

impl qobject::DiffDoc {
    fn rows(&self) -> &Vec<Row> {
        let r = self.rust();
        if r.split { &r.side } else { &r.unified }
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(row) = self.rows().get(index.row() as usize) else { return QVariant::default() };
        match ROLES.get((role - 0x0100) as usize).copied().unwrap_or("") {
            "kind" => QVariant::from(&qs(match row.kind {
                RowKind::Text => "text",
                RowKind::File => "file",
                RowKind::Hunk => "hunk",
                RowKind::Line => "line",
            })),
            "oldNo" => QVariant::from(&row.old_no),
            "newNo" => QVariant::from(&row.new_no),
            "left" => QVariant::from(&qs(&row.left)),
            "right" => QVariant::from(&qs(&row.right)),
            "leftType" => QVariant::from(&row.left_type),
            "rightType" => QVariant::from(&row.right_type),
            "sub" => QVariant::from(&qs(&row.sub)),
            "hasPlain" => QVariant::from(&(!row.left_plain.is_empty() || !row.right_plain.is_empty())),
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
        self.rows().len() as i32
    }

    fn is_patch_name(&self, name: &QString) -> bool {
        let n = name.to_string().to_lowercase();
        [".patch", ".diff", ".debdiff", ".rej"].iter().any(|e| n.ends_with(e))
    }

    fn plain(&self, row: i32, side: &QString) -> QString {
        let Some(r) = self.rows().get(row.max(0) as usize) else { return QString::default() };
        qs(if side.to_string() == "new" { &r.right_plain } else { &r.left_plain })
    }

    fn copy(&self, row: i32, side: &QString) {
        crate::bridge::ffi::grove_set_clipboard_text(&self.plain(row, side));
    }

    fn load(mut self: Pin<&mut Self>, path: &QString, dark: bool) {
        if let Some(c) = self.as_mut().rust_mut().cancel.take() {
            c.store(true, Ordering::Relaxed);
        }
        self.as_mut().rust_mut().gen += 1;
        unsafe { self.as_mut().begin_reset_model() };
        let old = {
            let mut rm = self.as_mut().rust_mut();
            let me: &mut DiffDocRust = &mut rm;
            (std::mem::take(&mut me.unified), std::mem::take(&mut me.side))
        };
        unsafe { self.as_mut().end_reset_model() };
        std::thread::spawn(move || drop(old));
        self.as_mut().set_path(path.clone());
        self.as_mut().set_error(QString::default());
        self.as_mut().set_files(0);
        self.as_mut().set_added(0);
        self.as_mut().set_removed(0);
        let p = path.to_string();
        if p.is_empty() {
            self.as_mut().set_loading(false);
            return;
        }
        self.as_mut().set_loading(true);
        let cancel = Arc::new(AtomicBool::new(false));
        self.as_mut().rust_mut().cancel = Some(cancel.clone());
        let gen = self.rust().gen;
        let qt = self.qt_thread();
        std::thread::Builder::new()
            .name("grove-diffdoc".into())
            .spawn(move || {
                let result = match std::fs::read(&p) {
                    Ok(bytes) => {
                        let name = std::path::Path::new(&p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        let text = grove_core::textenc::decode(&bytes, grove_core::textenc::detect(&bytes, &name));
                        let parts = patch::parse(&text);
                        if cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        if !parts.iter().any(|x| matches!(x, Part::File(_))) {
                            Err("There are no changes in this file to show as a diff.".to_string())
                        } else {
                            Ok(build(&parts, dark, &cancel))
                        }
                    }
                    Err(e) => Err(grove_core::util::plain_error(&e, std::path::Path::new(&p))),
                };
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let _ = qt.queue(move |mut m| {
                    if m.rust().gen != gen {
                        return;
                    }
                    match result {
                        Ok(b) => {
                            unsafe { m.as_mut().begin_reset_model() };
                            {
                                let mut rm = m.as_mut().rust_mut();
                                rm.unified = b.unified;
                                rm.side = b.side;
                            }
                            unsafe { m.as_mut().end_reset_model() };
                            m.as_mut().set_files(b.files);
                            m.as_mut().set_added(b.added);
                            m.as_mut().set_removed(b.removed);
                            m.as_mut().set_max_number(b.max_number);
                        }
                        Err(e) => m.as_mut().set_error(qs(&e)),
                    }
                    m.as_mut().set_loading(false);
                });
            })
            .ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_changed_words() {
        let (a, b) = word_changes("let total = count + 1;", "let total = count + 2;").unwrap();
        assert_eq!(a, vec![20..21]);
        assert_eq!(b, vec![20..21]);
        // Wholly different lines: no marks.
        assert!(word_changes("fn main() {", "    return x * y;").is_none());
    }

    #[test]
    fn lays_out_both_ways() {
        let d = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,3 +1,3 @@ fn f() {\n let a = 1;\n-let b = 2;\n+let b = 3;\n let c = 4;\n";
        let b = build(&patch::parse(d), true, &AtomicBool::new(false));
        assert_eq!((b.files, b.added, b.removed), (1, 1, 1));
        let kinds: Vec<RowKind> = b.unified.iter().map(|r| r.kind).collect();
        assert_eq!(kinds, [RowKind::File, RowKind::Hunk, RowKind::Line, RowKind::Line, RowKind::Line, RowKind::Line]);
        // Unified: the removed line, then the added one.
        assert_eq!((b.unified[3].left_type, b.unified[3].old_no, b.unified[3].new_no), (REMOVED, 2, 0));
        assert_eq!((b.unified[4].left_type, b.unified[4].old_no, b.unified[4].new_no), (ADDED, 0, 2));
        // Side by side: one row for the change, old left, new right.
        assert_eq!(b.side.len(), 5);
        let r = &b.side[3];
        assert_eq!((r.left_type, r.right_type, r.old_no, r.new_no), (REMOVED, ADDED, 2, 2));
        assert!(r.left.contains("background-color:#901011") && r.right.contains("background-color:#006000"));
        assert_eq!((r.left_plain.as_str(), r.right_plain.as_str()), ("let b = 2;", "let b = 3;"));
    }
}
