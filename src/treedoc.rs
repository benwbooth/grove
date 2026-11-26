//! TreeDoc: a JSON, YAML, TOML, XML or INI file as rows of a tree for the
//! viewer's tree view (DocView's Tree / Source switch). The file is read and
//! parsed on a worker (grove_core::structured: a compact arena); the model
//! holds only the rows that are open, so a 50 MB JSON opens at its top
//! level at once and each container's children are made when it's opened.
//! Search runs on a worker too and opens the containers its matches are in.

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QList, QModelIndex, QString, QVariant};
use grove_core::structured::{self, Format, Kind, Tree, NONE};
use std::cell::RefCell;
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

    unsafe extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(QString, path)]
        /// "json", "yaml", "toml", "xml", "ini".
        #[qproperty(QString, format)]
        #[qproperty(bool, loading)]
        /// Why the file isn't a tree ("" when it is), and where (1-based; 0 = nowhere).
        #[qproperty(QString, error)]
        #[qproperty(i32, error_line, cxx_name = "errorLine")]
        #[qproperty(i32, error_col, cxx_name = "errorCol")]
        /// How many values the file has.
        #[qproperty(i32, values)]
        /// A word about what's shown ("Opened 3 levels: all of it would be 2 million rows").
        #[qproperty(QString, note)]
        #[qproperty(bool, searching)]
        /// Matches of the last search: how many are marked, how many there are.
        #[qproperty(i32, hit_count, cxx_name = "hitCount")]
        #[qproperty(i32, hit_total, cxx_name = "hitTotal")]
        type TreeDoc = super::TreeDocRust;
    }

    impl cxx_qt::Threading for TreeDoc {}

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginInsertRows"]
        unsafe fn begin_insert_rows(self: Pin<&mut TreeDoc>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut TreeDoc>);
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(self: Pin<&mut TreeDoc>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut TreeDoc>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut TreeDoc>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut TreeDoc>);
        #[inherit]
        fn index(self: &TreeDoc, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(self: Pin<&mut TreeDoc>, top_left: &QModelIndex, bottom_right: &QModelIndex, roles: &QList_i32);
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &TreeDoc, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &TreeDoc) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &TreeDoc, parent: &QModelIndex) -> i32;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        /// Read `path` as `format` ("" clears).
        #[qinvokable]
        fn load(self: Pin<&mut TreeDoc>, path: &QString, format: &QString);
        /// The tree format of a file by its name ("" = none).
        #[qinvokable]
        fn format_for(self: &TreeDoc, name: &QString) -> QString;
        /// Open or close the row's container (`deep`: everything inside too).
        #[qinvokable]
        fn set_open(self: Pin<&mut TreeDoc>, row: i32, open: bool, deep: bool);
        #[qinvokable]
        fn expand_all(self: Pin<&mut TreeDoc>);
        #[qinvokable]
        fn collapse_all(self: Pin<&mut TreeDoc>);
        /// The row of the row's container (-1 at the top).
        #[qinvokable]
        fn parent_row(self: &TreeDoc, row: i32) -> i32;
        /// Find `query` in keys and values (on a worker); "" clears.
        #[qinvokable]
        fn search(self: Pin<&mut TreeDoc>, query: &QString);
        /// The next (`dir` 1) or previous (-1) match's row from `row`; -1: none.
        #[qinvokable]
        fn next_hit(self: &TreeDoc, row: i32, dir: i32) -> i32;
        /// Which match the row is (1-based; 0: not one).
        #[qinvokable]
        fn hit_number(self: &TreeDoc, row: i32) -> i32;
        /// Put the row's value, path or key on the clipboard (`what`:
        /// "value", "path", "key"); `copied` says when it's there.
        #[qinvokable]
        fn copy(self: Pin<&mut TreeDoc>, row: i32, what: &QString);
        /// The row's whole value (up to a few thousand characters), for its tooltip.
        #[qinvokable]
        fn full_value(self: &TreeDoc, row: i32) -> QString;
        /// The row's path (`servers[2].host`, `/a/b[3]/@id`).
        #[qinvokable]
        fn path_of(self: &TreeDoc, row: i32) -> QString;
        #[qsignal]
        fn copied(self: Pin<&mut TreeDoc>, what: QString);
        #[qsignal]
        #[cxx_name = "searchDone"]
        fn search_done(self: Pin<&mut TreeDoc>);
    }
}

/// Bits per node: which are open, which match.
#[derive(Default)]
struct Bits(Vec<u64>);
impl Bits {
    fn sized(n: usize) -> Bits {
        Bits(vec![0; n.div_ceil(64)])
    }
    fn get(&self, i: u32) -> bool {
        self.0.get(i as usize / 64).is_some_and(|w| w & (1 << (i % 64)) != 0)
    }
    fn set(&mut self, i: u32, on: bool) {
        if let Some(w) = self.0.get_mut(i as usize / 64) {
            if on {
                *w |= 1 << (i % 64);
            } else {
                *w &= !(1 << (i % 64));
            }
        }
    }
    fn clear(&mut self) {
        self.0.iter_mut().for_each(|w| *w = 0);
    }
}

#[derive(Default)]
pub struct TreeDocRust {
    path: QString,
    format: QString,
    loading: bool,
    error: QString,
    error_line: i32,
    error_col: i32,
    values: i32,
    note: QString,
    searching: bool,
    hit_count: i32,
    hit_total: i32,
    tree: Option<Arc<Tree>>,
    /// The rows shown: (node, depth).
    rows: Vec<(u32, u16)>,
    open: Bits,
    hits: Bits,
    /// Rows of the matches, in order (made when asked for, after a change).
    hit_rows: RefCell<Option<Vec<u32>>>,
    gen: u64,
    search_gen: u64,
    cancel: Option<Arc<AtomicBool>>,
    search_cancel: Option<Arc<AtomicBool>>,
}

const ROLES: [&str; 11] = ["depth", "key", "keyKind", "kind", "vkind", "value", "summary", "expandable", "expanded", "hit", "node"];
/// Rows opened at most by Expand All (whole levels, as many as fit).
const EXPAND_ALL_ROWS: usize = 200_000;
/// A file opens showing levels while they fit in this many rows.
const FIRST_ROWS: usize = 60;
/// Opening one container with everything inside: at most this many rows.
const DEEP_ROWS: usize = 100_000;
/// Rows shown at most (search opening many containers).
const MAX_ROWS: usize = 4_000_000;
/// A value shown in a row is cut here.
const SHOWN_CHARS: usize = 300;

fn qs(s: &str) -> QString {
    QString::from(s)
}

/// The tree on the worker → UI thread.
struct Parsed(Result<Tree, structured::ParseError>);
unsafe impl Send for Parsed {}

fn shown_value(t: &Tree, n: u32) -> String {
    let v = t.value(n);
    let mut s: String = v.chars().take(SHOWN_CHARS).map(|c| if c == '\n' { '⏎' } else if c == '\t' || c == '\r' { ' ' } else { c }).collect();
    if v.chars().nth(SHOWN_CHARS).is_some() {
        s.push('…');
    }
    let quoted = t.kind(n) == Kind::Str && matches!(t.format, Format::Json | Format::Yaml | Format::Toml);
    if quoted {
        format!("\"{s}\"")
    } else {
        s
    }
}

impl TreeDocRust {
    /// Whether the root has a row of its own (XML's root element, a lone
    /// value, an empty container), or only its children show.
    fn root_shown(t: &Tree) -> bool {
        t.format == Format::Xml || !t.kind(t.root()).is_container() || t.count(t.root()) == 0
    }

    /// Rows under `node` (its open descendants) at `depth`, at most `cap`.
    fn rows_under(&self, t: &Tree, node: u32, depth: u16, cap: usize, out: &mut Vec<(u32, u16)>) {
        // Iterative depth-first walk through open containers.
        let mut stack: Vec<(structured::Children<'_>, u16)> = vec![(t.children(node), depth)];
        while let Some((it, d)) = stack.last_mut() {
            let d = *d;
            match it.next() {
                Some(c) => {
                    if out.len() >= cap {
                        return;
                    }
                    out.push((c, d));
                    if t.expandable(c) && self.open.get(c) {
                        stack.push((t.children(c), d.saturating_add(1)));
                    }
                }
                None => {
                    stack.pop();
                }
            }
        }
    }

    fn all_rows(&self, t: &Tree) -> (Vec<(u32, u16)>, bool) {
        let mut rows = Vec::new();
        let r = t.root();
        if Self::root_shown(t) {
            rows.push((r, 0));
            if self.open.get(r) {
                self.rows_under(t, r, 1, MAX_ROWS, &mut rows);
            }
        } else {
            self.rows_under(t, r, 0, MAX_ROWS, &mut rows);
        }
        let capped = rows.len() >= MAX_ROWS;
        (rows, capped)
    }

    /// Open whole levels from the top while they fit in `budget` rows;
    /// true when everything is open.
    fn open_levels(&mut self, t: &Tree, budget: usize) -> (bool, usize) {
        self.open.clear();
        let r = t.root();
        self.open.set(r, true);
        let mut total = t.count(r) as usize + usize::from(Self::root_shown(t));
        let mut level: Vec<u32> = t.children(r).filter(|&c| t.expandable(c)).collect();
        let mut levels = 1;
        loop {
            if level.is_empty() {
                return (true, levels);
            }
            let extra: usize = level.iter().map(|&c| t.count(c) as usize).sum();
            if total + extra > budget {
                return (false, levels);
            }
            total += extra;
            let mut next = Vec::new();
            for &c in &level {
                self.open.set(c, true);
                next.extend(t.children(c).filter(|&g| t.expandable(g)));
            }
            level = next;
            levels += 1;
        }
    }
}

impl qobject::TreeDoc {
    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let r = self.rust();
        let (Some(t), Some(&(n, depth))) = (r.tree.as_ref(), r.rows.get(index.row() as usize)) else { return QVariant::default() };
        let kind = t.kind(n);
        match ROLES.get((role - 0x0100) as usize) {
            Some(&"depth") => QVariant::from(&(depth as i32)),
            Some(&"key") => {
                let k = match kind {
                    Kind::Text => "#text".into(),
                    Kind::Comment => "#comment".into(),
                    _ if t.parent(n) == NONE && t.format != Format::Xml => "(the whole file)".into(),
                    _ => t.key(n).into_owned(),
                };
                QVariant::from(&qs(&k))
            }
            Some(&"keyKind") => {
                let k = if t.key_is_index(n) {
                    "index"
                } else {
                    match kind {
                        Kind::Attr => "attr",
                        Kind::Element => "element",
                        Kind::Text | Kind::Comment => "special",
                        _ if t.parent(n) == NONE => "special",
                        _ => "name",
                    }
                };
                QVariant::from(&qs(k))
            }
            Some(&"kind") => QVariant::from(&qs(t.type_name(n))),
            Some(&"vkind") => {
                let v = match kind {
                    Kind::Str | Kind::Attr | Kind::Text => "string",
                    Kind::Element if t.count(n) == 0 => "string",
                    Kind::Num => "number",
                    Kind::Bool => "boolean",
                    Kind::Null => "null",
                    Kind::Date => "date",
                    Kind::Comment => "comment",
                    Kind::Alias => "alias",
                    _ => "container",
                };
                QVariant::from(&qs(v))
            }
            Some(&"value") => QVariant::from(&qs(&if kind.is_container() && t.count(n) > 0 { String::new() } else { shown_value(t, n) })),
            Some(&"summary") => QVariant::from(&qs(&t.summary(n))),
            Some(&"expandable") => QVariant::from(&t.expandable(n)),
            Some(&"expanded") => QVariant::from(&(t.expandable(n) && r.open.get(n))),
            Some(&"hit") => QVariant::from(&r.hits.get(n)),
            Some(&"node") => QVariant::from(&(n as i32)),
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
        self.rust().rows.len() as i32
    }

    fn format_for(&self, name: &QString) -> QString {
        qs(structured::format_for(&name.to_string()).map(|f| f.name()).unwrap_or(""))
    }

    fn reset_rows(mut self: Pin<&mut Self>, rows: Vec<(u32, u16)>) {
        unsafe { self.as_mut().begin_reset_model() };
        self.as_mut().rust_mut().rows = rows;
        *self.rust().hit_rows.borrow_mut() = None;
        unsafe { self.as_mut().end_reset_model() };
    }

    fn load(mut self: Pin<&mut Self>, path: &QString, format: &QString) {
        for c in [self.as_mut().rust_mut().cancel.take(), self.as_mut().rust_mut().search_cancel.take()].into_iter().flatten() {
            c.store(true, Ordering::Relaxed);
        }
        self.as_mut().rust_mut().gen += 1;
        self.as_mut().rust_mut().search_gen += 1;
        self.as_mut().rust_mut().tree = None;
        self.as_mut().rust_mut().open = Bits::default();
        self.as_mut().rust_mut().hits = Bits::default();
        self.as_mut().reset_rows(Vec::new());
        self.as_mut().set_path(path.clone());
        self.as_mut().set_format(format.clone());
        self.as_mut().set_error(QString::default());
        self.as_mut().set_error_line(0);
        self.as_mut().set_error_col(0);
        self.as_mut().set_values(0);
        self.as_mut().set_note(QString::default());
        self.as_mut().set_searching(false);
        self.as_mut().set_hit_count(0);
        self.as_mut().set_hit_total(0);
        let p = path.to_string();
        let Some(fmt) = Format::from_name(&format.to_string()) else {
            self.as_mut().set_loading(false);
            return;
        };
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
            .name("grove-treedoc".into())
            .spawn(move || {
                let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                let max = structured::max_bytes(fmt);
                let r = if size > max {
                    Err(structured::ParseError {
                        message: format!("It's too big to show as a tree ({} MB; {} files up to {} MB are)", size >> 20, fmt.title(), max >> 20),
                        line: 0,
                        col: 0,
                    })
                } else {
                    match std::fs::read(&p) {
                        Ok(bytes) => structured::parse(bytes, fmt, &cancel),
                        Err(e) => Err(structured::ParseError { message: format!("It couldn't be read: {e}"), line: 0, col: 0 }),
                    }
                };
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let parsed = Parsed(r);
                let _ = qt.queue(move |mut m| {
                    if m.rust().gen != gen {
                        return;
                    }
                    m.as_mut().loaded(parsed.0);
                });
            })
            .ok();
    }

    fn loaded(mut self: Pin<&mut Self>, r: Result<Tree, structured::ParseError>) {
        self.as_mut().set_loading(false);
        let t = match r {
            Ok(t) => t,
            Err(e) => {
                self.as_mut().set_error_line(e.line as i32);
                self.as_mut().set_error_col(e.col as i32);
                self.as_mut().set_error(qs(&e.message));
                return;
            }
        };
        let t = Arc::new(t);
        self.as_mut().set_values(t.len().min(i32::MAX as usize) as i32);
        {
            let mut rm = self.as_mut().rust_mut();
            let me: &mut TreeDocRust = &mut rm;
            me.open = Bits::sized(t.len());
            me.hits = Bits::sized(t.len());
            me.open_levels(&t, FIRST_ROWS);
            me.tree = Some(t.clone());
        }
        let (rows, _) = self.rust().all_rows(&t);
        self.as_mut().reset_rows(rows);
    }

    fn set_open(mut self: Pin<&mut Self>, row: i32, open: bool, deep: bool) {
        let Some(t) = self.rust().tree.clone() else { return };
        let Some(&(n, depth)) = self.rust().rows.get(row.max(0) as usize) else { return };
        if !t.expandable(n) {
            return;
        }
        let row = row as usize;
        // Take away the rows under it.
        let end = self.rust().rows[row + 1..].iter().position(|&(_, d)| d <= depth).map(|i| row + 1 + i).unwrap_or(self.rust().rows.len());
        if end > row + 1 {
            unsafe { self.as_mut().begin_remove_rows(&QModelIndex::default(), row as i32 + 1, end as i32 - 1) };
            self.as_mut().rust_mut().rows.drain(row + 1..end);
            unsafe { self.as_mut().end_remove_rows() };
        }
        {
            let mut rm = self.as_mut().rust_mut();
            let me: &mut TreeDocRust = &mut rm;
            me.open.set(n, open);
            if deep {
                // Everything inside: open (while it fits) or close.
                let mut level: Vec<u32> = t.children(n).filter(|&c| t.expandable(c)).collect();
                let mut total = t.count(n) as usize;
                while !level.is_empty() {
                    if open {
                        let extra: usize = level.iter().map(|&c| t.count(c) as usize).sum();
                        if total + extra > DEEP_ROWS {
                            break;
                        }
                        total += extra;
                    }
                    let mut next = Vec::new();
                    for &c in &level {
                        // Closing walks only into what's open.
                        if !open && !me.open.get(c) {
                            continue;
                        }
                        me.open.set(c, open);
                        next.extend(t.children(c).filter(|&g| t.expandable(g)));
                    }
                    level = next;
                }
            }
        }
        if open {
            let mut add = Vec::new();
            self.rust().rows_under(&t, n, depth.saturating_add(1), MAX_ROWS.saturating_sub(self.rust().rows.len()), &mut add);
            if !add.is_empty() {
                unsafe { self.as_mut().begin_insert_rows(&QModelIndex::default(), row as i32 + 1, (row + add.len()) as i32) };
                self.as_mut().rust_mut().rows.splice(row + 1..row + 1, add);
                unsafe { self.as_mut().end_insert_rows() };
            }
        }
        *self.rust().hit_rows.borrow_mut() = None;
        let ix = self.index(row as i32, 0, &QModelIndex::default());
        let mut roles = QList::<i32>::default();
        roles.append(0x0100 + 8);
        self.as_mut().data_changed(&ix, &ix, &roles);
    }

    fn expand_all(mut self: Pin<&mut Self>) {
        let Some(t) = self.rust().tree.clone() else { return };
        let (all, levels) = {
            let mut rm = self.as_mut().rust_mut();
            let me: &mut TreeDocRust = &mut rm;
            me.open_levels(&t, EXPAND_ALL_ROWS)
        };
        let (rows, _) = self.rust().all_rows(&t);
        let note = if all {
            String::new()
        } else {
            format!("Opened the first {levels} levels ({} rows); open deeper ones one at a time", rows.len())
        };
        self.as_mut().set_note(qs(&note));
        self.as_mut().reset_rows(rows);
    }

    fn collapse_all(mut self: Pin<&mut Self>) {
        let Some(t) = self.rust().tree.clone() else { return };
        self.as_mut().rust_mut().open.clear();
        let r = t.root();
        // The root stays open when it has no row of its own (its children are the top rows).
        if !TreeDocRust::root_shown(&t) {
            self.as_mut().rust_mut().open.set(r, true);
        }
        let (rows, _) = self.rust().all_rows(&t);
        self.as_mut().set_note(QString::default());
        self.as_mut().reset_rows(rows);
    }

    fn parent_row(&self, row: i32) -> i32 {
        let rows = &self.rust().rows;
        let Some(&(_, d)) = rows.get(row.max(0) as usize) else { return -1 };
        if d == 0 {
            return -1;
        }
        (0..row as usize).rev().find(|&i| rows[i].1 < d).map(|i| i as i32).unwrap_or(-1)
    }

    fn search(mut self: Pin<&mut Self>, query: &QString) {
        if let Some(c) = self.as_mut().rust_mut().search_cancel.take() {
            c.store(true, Ordering::Relaxed);
        }
        self.as_mut().rust_mut().search_gen += 1;
        let q = query.to_string();
        let Some(t) = self.rust().tree.clone() else { return };
        if q.trim().is_empty() {
            self.as_mut().rust_mut().hits.clear();
            *self.rust().hit_rows.borrow_mut() = None;
            self.as_mut().set_hit_count(0);
            self.as_mut().set_hit_total(0);
            self.as_mut().set_searching(false);
            let n = self.rust().rows.len() as i32;
            if n > 0 {
                let a = self.index(0, 0, &QModelIndex::default());
                let b = self.index(n - 1, 0, &QModelIndex::default());
                let mut roles = QList::<i32>::default();
                roles.append(0x0100 + 9);
                self.as_mut().data_changed(&a, &b, &roles);
            }
            self.as_mut().search_done();
            return;
        }
        self.as_mut().set_searching(true);
        let cancel = Arc::new(AtomicBool::new(false));
        self.as_mut().rust_mut().search_cancel = Some(cancel.clone());
        let sg = self.rust().search_gen;
        let qt = self.qt_thread();
        std::thread::Builder::new()
            .name("grove-treesearch".into())
            .spawn(move || {
                let (hits, total) = t.search(q.trim(), 10_000, &cancel);
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let _ = qt.queue(move |mut m| {
                    if m.rust().search_gen != sg {
                        return;
                    }
                    m.as_mut().searched(hits, total);
                });
            })
            .ok();
    }

    fn searched(mut self: Pin<&mut Self>, hits: Vec<u32>, total: usize) {
        let Some(t) = self.rust().tree.clone() else { return };
        {
            let mut rm = self.as_mut().rust_mut();
            let me: &mut TreeDocRust = &mut rm;
            me.hits.clear();
            for &h in &hits {
                me.hits.set(h, true);
                // Open every container it's in.
                let mut p = t.parent(h);
                while p != NONE {
                    me.open.set(p, true);
                    p = t.parent(p);
                }
            }
        }
        let (rows, capped) = self.rust().all_rows(&t);
        self.as_mut().reset_rows(rows);
        self.as_mut().set_note(qs(if capped { "Too many rows to show every match" } else { "" }));
        self.as_mut().set_hit_count(hits.len() as i32);
        self.as_mut().set_hit_total(total.min(i32::MAX as usize) as i32);
        self.as_mut().set_searching(false);
        self.as_mut().search_done();
    }

    fn with_hit_rows<R>(&self, f: impl FnOnce(&[u32]) -> R) -> R {
        let r = self.rust();
        let mut cache = r.hit_rows.borrow_mut();
        if cache.is_none() {
            *cache = Some(r.rows.iter().enumerate().filter(|(_, &(n, _))| r.hits.get(n)).map(|(i, _)| i as u32).collect());
        }
        f(cache.as_ref().unwrap())
    }

    fn next_hit(&self, row: i32, dir: i32) -> i32 {
        self.with_hit_rows(|h| {
            if h.is_empty() {
                return -1;
            }
            if dir >= 0 {
                let i = h.partition_point(|&r| (r as i64) <= row as i64);
                h[if i < h.len() { i } else { 0 }] as i32
            } else {
                let i = h.partition_point(|&r| (r as i64) < row as i64);
                h[if i > 0 { i - 1 } else { h.len() - 1 }] as i32
            }
        })
    }

    fn hit_number(&self, row: i32) -> i32 {
        if row < 0 {
            return 0;
        }
        self.with_hit_rows(|h| h.binary_search(&(row as u32)).map(|i| i as i32 + 1).unwrap_or(0))
    }

    fn copy(self: Pin<&mut Self>, row: i32, what: &QString) {
        let Some(t) = self.rust().tree.clone() else { return };
        let Some(&(n, _)) = self.rust().rows.get(row.max(0) as usize) else { return };
        let what = what.to_string();
        let qt = self.qt_thread();
        // A big container's text is made on a worker.
        std::thread::spawn(move || {
            let text = match what.as_str() {
                "path" => t.path(n),
                "key" => t.key(n).into_owned(),
                _ => t.copy_text(n),
            };
            let _ = qt.queue(move |m| {
                crate::bridge::ffi::grove_set_clipboard_text(&qs(&text));
                m.copied(qs(&what));
            });
        });
    }

    fn full_value(&self, row: i32) -> QString {
        let r = self.rust();
        let (Some(t), Some(&(n, _))) = (r.tree.as_ref(), r.rows.get(row.max(0) as usize)) else { return QString::default() };
        if t.kind(n).is_container() && t.count(n) > 0 {
            return QString::default();
        }
        let v = t.value(n);
        let mut s: String = v.chars().take(4000).collect();
        if v.chars().nth(4000).is_some() {
            s.push('…');
        }
        qs(&s)
    }

    fn path_of(&self, row: i32) -> QString {
        let r = self.rust();
        let (Some(t), Some(&(n, _))) = (r.tree.as_ref(), r.rows.get(row.max(0) as usize)) else { return QString::default() };
        qs(&t.path(n))
    }
}
