//! TreeDoc: a JSON, YAML, TOML, XML or INI file as rows of a tree for the
//! viewer's tree view (DocView's Tree / Source switch). The file is read and
//! parsed on a worker (grove_core::structured: a compact arena); the model
//! holds only the rows that are open, so a 50 MB JSON opens at its top
//! level at once and each container's children are made when it's opened.
//! Search runs on a worker too and opens the containers its matches are in.
//! A container with more than PAGE children shows them in pages ("0 … 999"),
//! each opened on its own, so an array of millions of items is a few
//! thousand rows. Long values are cut for their rows (never unescaped whole
//! on the UI thread). GROVE_PERF=1 prints the UI-thread time of each step.

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QList, QModelIndex, QString, QVariant};
use grove_core::structured::{self, Format, Kind, Tree, NONE};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Instant;
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
        /// How far reading and parsing are (0–1; JSON and XML say, others stay at 0).
        #[qproperty(f64, progress)]
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
        /// The row's value, at most `max` characters (the tooltip, wrapping, Show Whole Value).
        #[qinvokable]
        fn full_value(self: &TreeDoc, row: i32, max: i32) -> QString;
        /// Whether the row's value is longer than `max` characters.
        #[qinvokable]
        fn value_longer(self: &TreeDoc, row: i32, max: i32) -> bool;
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

/// A row: a node, or (page != NONE) a page of a big container's children.
#[derive(Clone, Copy)]
struct Row {
    node: u32,
    depth: u16,
    page: u32,
}

#[derive(Default)]
pub struct TreeDocRust {
    path: QString,
    format: QString,
    loading: bool,
    progress: f64,
    error: QString,
    error_line: i32,
    error_col: i32,
    values: i32,
    note: QString,
    searching: bool,
    hit_count: i32,
    hit_total: i32,
    tree: Option<Arc<Tree>>,
    rows: Vec<Row>,
    open: Bits,
    /// Open pages: node << 32 | page.
    open_pages: HashSet<u64>,
    hits: Bits,
    /// Where each page of a big container starts (made when first needed).
    pages: RefCell<HashMap<u32, Rc<Vec<u32>>>>,
    /// Rows of the matches, in order (made when asked for, after a change).
    hit_rows: RefCell<Option<Vec<u32>>>,
    gen: u64,
    search_gen: u64,
    cancel: Option<Arc<AtomicBool>>,
    search_cancel: Option<Arc<AtomicBool>>,
}

const ROLES: [&str; 12] = ["depth", "key", "keyKind", "kind", "vkind", "value", "summary", "expandable", "expanded", "hit", "node", "page"];
/// Children per page of a big container.
const PAGE: u32 = 1000;
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
/// Workers get a deep stack: YAML's and TOML's parsers recurse per level.
const WORKER_STACK: usize = 512 << 20;

fn qs(s: &str) -> QString {
    QString::from(s)
}

fn perf_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("GROVE_PERF").is_some())
}

fn perf(what: &str, t0: Instant, rows: usize) {
    if perf_on() {
        eprintln!("grove perf: tree {what} ({rows} rows): {:.1} ms on the UI thread", t0.elapsed().as_secs_f64() * 1000.0);
    }
}

/// The tree on the worker → UI thread.
struct Parsed(Result<Arc<Tree>, structured::ParseError>);
unsafe impl Send for Parsed {}

/// Trees in use, shared: the preview pane and Quick Look showing the same
/// file hold one tree, not two (a 300 MB JSON's is over a GB). Keyed by
/// path, size, mtime and format; a second load waits for the first's parse.
type CacheKey = (String, u64, i64, &'static str);
fn shared(key: CacheKey) -> Arc<parking_lot::Mutex<std::sync::Weak<Tree>>> {
    static CACHE: std::sync::OnceLock<parking_lot::Mutex<HashMap<CacheKey, Arc<parking_lot::Mutex<std::sync::Weak<Tree>>>>>> = std::sync::OnceLock::new();
    let mut c = CACHE.get_or_init(Default::default).lock();
    // Forget entries nobody uses.
    c.retain(|_, v| Arc::strong_count(v) > 1 || v.lock().strong_count() > 0);
    c.entry(key).or_default().clone()
}

fn shown_value(t: &Tree, n: u32) -> String {
    let (v, more) = t.value_head(n, SHOWN_CHARS);
    let mut s: String = v.chars().map(|c| if c == '\n' { '⏎' } else if c == '\t' || c == '\r' { ' ' } else { c }).collect();
    if more {
        s.push('…');
    }
    let quoted = t.kind(n) == Kind::Str && matches!(t.format, Format::Json | Format::Yaml | Format::Toml);
    if quoted {
        format!("\"{s}\"")
    } else {
        s
    }
}

fn pages_of(t: &Tree, n: u32) -> u32 {
    let c = t.count(n);
    if c > PAGE {
        c.div_ceil(PAGE)
    } else {
        0
    }
}

/// Rows a container shows when it's open (its pages, or its children).
fn shown_count(t: &Tree, n: u32) -> usize {
    match pages_of(t, n) {
        0 => t.count(n) as usize,
        p => p as usize,
    }
}

fn group(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

impl TreeDocRust {
    /// Whether the root has a row of its own (XML's root element, a lone
    /// value, an empty container), or only its children show.
    fn root_shown(t: &Tree) -> bool {
        t.format == Format::Xml || !t.kind(t.root()).is_container() || t.count(t.root()) == 0
    }

    fn page_starts(&self, t: &Tree, n: u32) -> Rc<Vec<u32>> {
        self.pages.borrow_mut().entry(n).or_insert_with(|| Rc::new(t.page_starts(n, PAGE))).clone()
    }

    fn page_open(&self, n: u32, k: u32) -> bool {
        self.open_pages.contains(&((n as u64) << 32 | k as u64))
    }

    /// The page of its parent's children that `c` is on (ids rise along siblings).
    fn page_of(&self, t: &Tree, parent: u32, c: u32) -> u32 {
        let st = self.page_starts(t, parent);
        (st.partition_point(|&s| s <= c).max(1) - 1) as u32
    }

    /// Rows inside `node` (open, or a page of it when `page` isn't NONE) at
    /// `depth`, at most `cap`: an iterative walk through what's open.
    fn rows_under(&self, t: &Tree, node: u32, page: u32, depth: u16, cap: usize, out: &mut Vec<Row>) {
        enum F {
            Kids { next: u32, left: u32, depth: u16 },
            Pages { node: u32, k: u32, n: u32, depth: u16 },
        }
        let container = |n: u32, d: u16| -> F {
            match pages_of(t, n) {
                0 => F::Kids { next: t.first_child(n), left: u32::MAX, depth: d },
                p => F::Pages { node: n, k: 0, n: p, depth: d },
            }
        };
        let mut stack = vec![if page == NONE { container(node, depth) } else { F::Kids { next: self.page_starts(t, node)[page as usize], left: PAGE, depth } }];
        while let Some(f) = stack.last_mut() {
            if out.len() >= cap {
                return;
            }
            match f {
                F::Kids { next, left, depth } => {
                    if *next == NONE || *left == 0 {
                        stack.pop();
                        continue;
                    }
                    let c = *next;
                    let d = *depth;
                    *next = t.next_sibling(c);
                    *left -= 1;
                    out.push(Row { node: c, depth: d, page: NONE });
                    if t.expandable(c) && self.open.get(c) {
                        stack.push(container(c, d.saturating_add(1)));
                    }
                }
                F::Pages { node, k, n, depth } => {
                    if *k >= *n {
                        stack.pop();
                        continue;
                    }
                    let (nd, kk, d) = (*node, *k, *depth);
                    *k += 1;
                    out.push(Row { node: nd, depth: d, page: kk });
                    if self.page_open(nd, kk) {
                        let start = self.page_starts(t, nd)[kk as usize];
                        stack.push(F::Kids { next: start, left: PAGE, depth: d.saturating_add(1) });
                    }
                }
            }
        }
    }

    fn all_rows(&self, t: &Tree) -> (Vec<Row>, bool) {
        let mut rows = Vec::new();
        let r = t.root();
        if Self::root_shown(t) {
            rows.push(Row { node: r, depth: 0, page: NONE });
            if self.open.get(r) && t.expandable(r) {
                self.rows_under(t, r, NONE, 1, MAX_ROWS, &mut rows);
            }
        } else {
            self.rows_under(t, r, NONE, 0, MAX_ROWS, &mut rows);
        }
        let capped = rows.len() >= MAX_ROWS;
        (rows, capped)
    }

    /// Open `from` and whole levels below it while they fit in `budget`
    /// rows (big containers' pages too); true when everything is open.
    fn open_levels(&mut self, t: &Tree, from: u32, budget: usize) -> (bool, usize) {
        self.open.set(from, true);
        let mut total = shown_count(t, from);
        self.open_all_pages(t, from, &mut total, budget);
        // Its own rows already fill the budget (or its children are in closed
        // pages): nothing below opens. Checked before walking millions of children.
        if total > budget || (pages_of(t, from) > 0 && !self.page_open(from, 0)) {
            return (false, 1);
        }
        let mut level: Vec<u32> = t.children(from).filter(|&c| t.expandable(c)).collect();
        let mut levels = 1;
        loop {
            if level.is_empty() {
                return (true, levels);
            }
            let extra: usize = level.iter().map(|&c| t.count(c) as usize + pages_of(t, c) as usize).sum();
            if total + extra > budget {
                return (false, levels);
            }
            total += extra;
            let mut next = Vec::new();
            for &c in &level {
                self.open.set(c, true);
                for k in 0..pages_of(t, c) {
                    self.open_pages.insert((c as u64) << 32 | k as u64);
                }
                next.extend(t.children(c).filter(|&g| t.expandable(g)));
            }
            level = next;
            levels += 1;
        }
    }

    /// A big container opened whole: its pages too, when they fit.
    fn open_all_pages(&mut self, t: &Tree, n: u32, total: &mut usize, budget: usize) {
        let p = pages_of(t, n);
        if p > 0 && *total + t.count(n) as usize <= budget {
            for k in 0..p {
                self.open_pages.insert((n as u64) << 32 | k as u64);
            }
            *total += t.count(n) as usize;
        }
    }

    /// The node a row stands for, when it's a node (not a page).
    fn node_at(&self, row: i32) -> Option<(Arc<Tree>, u32)> {
        let t = self.tree.clone()?;
        let r = *self.rows.get(usize::try_from(row).ok()?)?;
        Some((t, r.node))
    }
}

impl qobject::TreeDoc {
    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let r = self.rust();
        let (Some(t), Some(&row)) = (r.tree.as_ref(), r.rows.get(index.row() as usize)) else { return QVariant::default() };
        let n = row.node;
        let role = ROLES.get((role - 0x0100) as usize).copied().unwrap_or("");
        if row.page != NONE {
            // A page of a big container's children.
            let first = row.page as u64 * PAGE as u64;
            let last = (first + PAGE as u64).min(t.count(n) as u64) - 1;
            return match role {
                "depth" => QVariant::from(&(row.depth as i32)),
                "key" => QVariant::from(&qs(&format!("{} … {}", group(first), group(last)))),
                "keyKind" => QVariant::from(&qs("page")),
                "kind" => QVariant::from(&qs(&format!("{} of {}", group(last - first + 1), group(t.count(n) as u64)))),
                "vkind" => QVariant::from(&qs("container")),
                "value" => QVariant::from(&QString::default()),
                "summary" => QVariant::from(&qs(&format!("{} items", group(last - first + 1)))),
                "expandable" => QVariant::from(&true),
                "expanded" => QVariant::from(&r.page_open(n, row.page)),
                "hit" => QVariant::from(&false),
                "node" => QVariant::from(&(n as i32)),
                "page" => QVariant::from(&(row.page as i32)),
                _ => QVariant::default(),
            };
        }
        let kind = t.kind(n);
        match role {
            "depth" => QVariant::from(&(row.depth as i32)),
            "key" => {
                let k = match kind {
                    Kind::Text => "#text".into(),
                    Kind::Comment => "#comment".into(),
                    _ if t.parent(n) == NONE && t.format != Format::Xml => "(the whole file)".into(),
                    _ => {
                        let k = t.key(n);
                        if k.len() > 400 {
                            k.chars().take(200).collect::<String>() + "…"
                        } else {
                            k.into_owned()
                        }
                    }
                };
                QVariant::from(&qs(&k))
            }
            "keyKind" => {
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
            "kind" => QVariant::from(&qs(t.type_name(n))),
            "vkind" => {
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
            "value" => QVariant::from(&qs(&if kind.is_container() && t.count(n) > 0 { String::new() } else { shown_value(t, n) })),
            "summary" => QVariant::from(&qs(&t.summary(n))),
            "expandable" => QVariant::from(&t.expandable(n)),
            "expanded" => QVariant::from(&(t.expandable(n) && r.open.get(n))),
            "hit" => QVariant::from(&r.hits.get(n)),
            "node" => QVariant::from(&(n as i32)),
            "page" => QVariant::from(&-1i32),
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

    fn reset_rows(mut self: Pin<&mut Self>, rows: Vec<Row>) {
        unsafe { self.as_mut().begin_reset_model() };
        let old = std::mem::replace(&mut self.as_mut().rust_mut().rows, rows);
        *self.rust().hit_rows.borrow_mut() = None;
        unsafe { self.as_mut().end_reset_model() };
        if old.len() > 100_000 {
            std::thread::spawn(move || drop(old));
        }
    }

    fn load(mut self: Pin<&mut Self>, path: &QString, format: &QString) {
        let t0 = Instant::now();
        for c in [self.as_mut().rust_mut().cancel.take(), self.as_mut().rust_mut().search_cancel.take()].into_iter().flatten() {
            c.store(true, Ordering::Relaxed);
        }
        self.as_mut().rust_mut().gen += 1;
        self.as_mut().rust_mut().search_gen += 1;
        // A big tree is freed on a worker (hundreds of MB).
        let old = {
            let mut rm = self.as_mut().rust_mut();
            let me: &mut TreeDocRust = &mut rm;
            (me.tree.take(), std::mem::take(&mut me.open), std::mem::take(&mut me.hits), std::mem::take(&mut me.open_pages))
        };
        self.rust().pages.borrow_mut().clear();
        self.as_mut().reset_rows(Vec::new());
        std::thread::spawn(move || drop(old));
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
        self.as_mut().set_progress(0.0);
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
        let progress = Arc::new(std::sync::atomic::AtomicU32::new(0));
        // Progress: a light ticker while the worker reads and parses.
        {
            let (qt, cancel, progress) = (qt.clone(), cancel.clone(), progress.clone());
            std::thread::Builder::new()
                .name("grove-treeprog".into())
                .spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_millis(150));
                    let v = progress.load(Ordering::Relaxed);
                    if cancel.load(Ordering::Relaxed) || v >= 1000 {
                        return;
                    }
                    if qt.queue(move |mut m| if m.rust().gen == gen && m.rust().loading { m.as_mut().set_progress(v as f64 / 1000.0) }).is_err() {
                        return;
                    }
                })
                .ok();
        }
        std::thread::Builder::new()
            .name("grove-treedoc".into())
            .stack_size(WORKER_STACK)
            .spawn(move || {
                let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                let max = structured::max_bytes(fmt);
                let budget = structured::memory_budget();
                let mtime = std::fs::metadata(&p).ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos() as i64).unwrap_or(0);
                let slot = shared((p.clone(), size, mtime, fmt.name()));
                let mut slot = slot.lock();
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(t) = slot.upgrade() {
                    progress.store(1000, Ordering::Relaxed);
                    let parsed = Parsed(Ok(t));
                    let _ = qt.queue(move |mut m| {
                        if m.rust().gen == gen {
                            m.as_mut().loaded(parsed.0);
                        }
                    });
                    return;
                }
                let r = if size > max {
                    Err(structured::ParseError {
                        message: format!("It's too big to show as a tree ({} MB; {} files up to {} MB are); the Source view shows all of it", size >> 20, fmt.title(), max >> 20),
                        line: 0,
                        col: 0,
                    })
                } else if size as usize > budget {
                    Err(structured::ParseError {
                        message: format!("It's too big to show as a tree here ({} MB, more than half the free memory); the Source view shows all of it", size >> 20),
                        line: 0,
                        col: 0,
                    })
                } else {
                    // Read in pieces: reading is the first fifth of the progress.
                    let read = (|| -> std::io::Result<Vec<u8>> {
                        use std::io::Read;
                        let mut f = std::fs::File::open(&p)?;
                        let mut buf = Vec::with_capacity(size as usize + 1);
                        let mut chunk = vec![0u8; 8 << 20];
                        loop {
                            if cancel.load(Ordering::Relaxed) {
                                return Err(std::io::Error::other("cancelled"));
                            }
                            let n = f.read(&mut chunk)?;
                            if n == 0 {
                                return Ok(buf);
                            }
                            buf.extend_from_slice(&chunk[..n]);
                            progress.store((buf.len() as u64 * 200 / size.max(1)) as u32, Ordering::Relaxed);
                        }
                    })();
                    match read {
                        Ok(bytes) => {
                            let parse_prog = std::sync::atomic::AtomicU32::new(0);
                            // Parse progress maps onto 20–100 %.
                            let (pp, prog2, done) = (&parse_prog, progress.clone(), AtomicBool::new(false));
                            std::thread::scope(|sc| {
                                sc.spawn(|| {
                                    while !done.load(Ordering::Relaxed) {
                                        std::thread::sleep(std::time::Duration::from_millis(100));
                                        prog2.store(200 + pp.load(Ordering::Relaxed) * 8 / 10, Ordering::Relaxed);
                                    }
                                });
                                let r = structured::parse_with(bytes, fmt, &cancel, pp, budget);
                                done.store(true, Ordering::Relaxed);
                                r
                            })
                        }
                        Err(e) => Err(structured::ParseError { message: format!("It couldn't be read: {e}"), line: 0, col: 0 }),
                    }
                };
                progress.store(1000, Ordering::Relaxed);
                if cancel.load(Ordering::Relaxed) {
                    // Freed here, not on the UI thread.
                    drop(r);
                    return;
                }
                let r = r.map(Arc::new);
                if let Ok(t) = &r {
                    *slot = Arc::downgrade(t);
                }
                drop(slot);
                let parsed = Parsed(r);
                let _ = qt.queue(move |mut m| {
                    if m.rust().gen != gen {
                        std::thread::spawn(move || drop(parsed));
                        return;
                    }
                    m.as_mut().loaded(parsed.0);
                });
            })
            .ok();
        perf("load", t0, 0);
    }

    fn loaded(mut self: Pin<&mut Self>, r: Result<Arc<Tree>, structured::ParseError>) {
        let t0 = Instant::now();
        self.as_mut().set_loading(false);
        self.as_mut().set_progress(1.0);
        let t = match r {
            Ok(t) => t,
            Err(e) => {
                self.as_mut().set_error_line(e.line as i32);
                self.as_mut().set_error_col(e.col as i32);
                self.as_mut().set_error(qs(&e.message));
                return;
            }
        };
        self.as_mut().set_values(t.len().min(i32::MAX as usize) as i32);
        {
            let mut rm = self.as_mut().rust_mut();
            let me: &mut TreeDocRust = &mut rm;
            me.open = Bits::sized(t.len());
            me.hits = Bits::sized(t.len());
            me.open_levels(&t, t.root(), FIRST_ROWS);
            me.tree = Some(t.clone());
        }
        let (rows, _) = self.rust().all_rows(&t);
        let n = rows.len();
        self.as_mut().reset_rows(rows);
        perf("loaded", t0, n);
    }

    fn set_open(mut self: Pin<&mut Self>, row: i32, open: bool, deep: bool) {
        let t0 = Instant::now();
        let Some(t) = self.rust().tree.clone() else { return };
        let Some(&Row { node: n, depth, page }) = self.rust().rows.get(row.max(0) as usize) else { return };
        if page == NONE && !t.expandable(n) {
            return;
        }
        let row = row as usize;
        // Take away the rows under it.
        let end = self.rust().rows[row + 1..].iter().position(|r| r.depth <= depth).map(|i| row + 1 + i).unwrap_or(self.rust().rows.len());
        if end > row + 1 {
            unsafe { self.as_mut().begin_remove_rows(&QModelIndex::default(), row as i32 + 1, end as i32 - 1) };
            let gone: Vec<Row> = self.as_mut().rust_mut().rows.drain(row + 1..end).collect();
            unsafe { self.as_mut().end_remove_rows() };
            if gone.len() > 100_000 {
                std::thread::spawn(move || drop(gone));
            }
        }
        {
            let mut rm = self.as_mut().rust_mut();
            let me: &mut TreeDocRust = &mut rm;
            if page != NONE {
                let key = (n as u64) << 32 | page as u64;
                if open {
                    me.open_pages.insert(key);
                } else {
                    me.open_pages.remove(&key);
                }
            } else if open && deep {
                me.open_levels(&t, n, DEEP_ROWS);
            } else if open {
                me.open.set(n, true);
            } else {
                me.open.set(n, false);
                if deep {
                    // Close everything open inside (walking only into what's open).
                    let mut level: Vec<u32> = t.children(n).filter(|&c| t.expandable(c)).collect();
                    while !level.is_empty() {
                        let mut next = Vec::new();
                        for &c in &level {
                            if !me.open.get(c) {
                                continue;
                            }
                            me.open.set(c, false);
                            next.extend(t.children(c).filter(|&g| t.expandable(g)));
                        }
                        level = next;
                    }
                    me.open_pages.retain(|k| {
                        let pn = (k >> 32) as u32;
                        // Pages of `n` itself or of anything inside it.
                        let mut a = pn;
                        while a != NONE {
                            if a == n {
                                return false;
                            }
                            a = t.parent(a);
                        }
                        true
                    });
                }
            }
        }
        if open {
            let mut add = Vec::new();
            let cap = MAX_ROWS.saturating_sub(self.rust().rows.len());
            self.rust().rows_under(&t, n, page, depth.saturating_add(1), cap, &mut add);
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
        perf(if open { "open" } else { "close" }, t0, self.rust().rows.len());
    }

    fn expand_all(mut self: Pin<&mut Self>) {
        let t0 = Instant::now();
        let Some(t) = self.rust().tree.clone() else { return };
        let (all, levels) = {
            let mut rm = self.as_mut().rust_mut();
            let me: &mut TreeDocRust = &mut rm;
            me.open.clear();
            me.open_pages.clear();
            me.open_levels(&t, t.root(), EXPAND_ALL_ROWS)
        };
        let (rows, _) = self.rust().all_rows(&t);
        let note = if all {
            String::new()
        } else {
            format!("Opened the first {levels} levels ({} rows); open deeper ones one at a time", group(rows.len() as u64))
        };
        self.as_mut().set_note(qs(&note));
        let n = rows.len();
        self.as_mut().reset_rows(rows);
        perf("expand all", t0, n);
    }

    fn collapse_all(mut self: Pin<&mut Self>) {
        let t0 = Instant::now();
        let Some(t) = self.rust().tree.clone() else { return };
        self.as_mut().rust_mut().open.clear();
        self.as_mut().rust_mut().open_pages.clear();
        let r = t.root();
        // The root stays open when it has no row of its own (its children are the top rows).
        if !TreeDocRust::root_shown(&t) {
            self.as_mut().rust_mut().open.set(r, true);
        }
        let (rows, _) = self.rust().all_rows(&t);
        self.as_mut().set_note(QString::default());
        let n = rows.len();
        self.as_mut().reset_rows(rows);
        perf("collapse all", t0, n);
    }

    fn parent_row(&self, row: i32) -> i32 {
        let rows = &self.rust().rows;
        let Some(&Row { depth: d, .. }) = rows.get(row.max(0) as usize) else { return -1 };
        if d == 0 {
            return -1;
        }
        (0..row as usize).rev().find(|&i| rows[i].depth < d).map(|i| i as i32).unwrap_or(-1)
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
        let t0 = Instant::now();
        let Some(t) = self.rust().tree.clone() else { return };
        {
            let mut rm = self.as_mut().rust_mut();
            let me: &mut TreeDocRust = &mut rm;
            me.hits.clear();
            for &h in &hits {
                me.hits.set(h, true);
                // Open every container it's in, and the page it's on.
                let mut c = h;
                let mut p = t.parent(h);
                while p != NONE {
                    me.open.set(p, true);
                    if pages_of(&t, p) > 0 {
                        let k = me.page_of(&t, p, c);
                        me.open_pages.insert((p as u64) << 32 | k as u64);
                    }
                    c = p;
                    p = t.parent(p);
                }
            }
        }
        let (rows, capped) = self.rust().all_rows(&t);
        let n = rows.len();
        self.as_mut().reset_rows(rows);
        self.as_mut().set_note(qs(if capped { "Too many rows to show every match" } else { "" }));
        self.as_mut().set_hit_count(hits.len() as i32);
        self.as_mut().set_hit_total(total.min(i32::MAX as usize) as i32);
        self.as_mut().set_searching(false);
        self.as_mut().search_done();
        perf("search results", t0, n);
    }

    fn with_hit_rows<R>(&self, f: impl FnOnce(&[u32]) -> R) -> R {
        let r = self.rust();
        let mut cache = r.hit_rows.borrow_mut();
        if cache.is_none() {
            *cache = Some(r.rows.iter().enumerate().filter(|(_, row)| row.page == NONE && r.hits.get(row.node)).map(|(i, _)| i as u32).collect());
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
        let Some((t, n)) = self.rust().node_at(row) else { return };
        let what = what.to_string();
        let qt = self.qt_thread();
        // A big container's text is made on a worker (deep stack: nesting).
        std::thread::Builder::new()
            .name("grove-treecopy".into())
            .stack_size(WORKER_STACK)
            .spawn(move || {
                let text = match what.as_str() {
                    "path" => t.path(n),
                    "key" => t.key(n).into_owned(),
                    _ => t.copy_text(n),
                };
                let _ = qt.queue(move |m| {
                    crate::bridge::ffi::grove_set_clipboard_text(&qs(&text));
                    m.copied(qs(&what));
                });
            })
            .ok();
    }

    fn full_value(&self, row: i32, max: i32) -> QString {
        let Some((t, n)) = self.rust().node_at(row) else { return QString::default() };
        if self.rust().rows[row as usize].page != NONE || (t.kind(n).is_container() && t.count(n) > 0) {
            return QString::default();
        }
        let (v, more) = t.value_head(n, max.max(1) as usize);
        qs(&if more { format!("{v}…") } else { v.into_owned() })
    }

    fn value_longer(&self, row: i32, max: i32) -> bool {
        let Some((t, n)) = self.rust().node_at(row) else { return false };
        self.rust().rows[row as usize].page == NONE && !(t.kind(n).is_container() && t.count(n) > 0) && t.value_head(n, max.max(1) as usize).1
    }

    fn path_of(&self, row: i32) -> QString {
        let Some((t, n)) = self.rust().node_at(row) else { return QString::default() };
        let r = self.rust().rows[row as usize];
        if r.page != NONE {
            let first = r.page as u64 * PAGE as u64;
            return qs(&format!("{} (items {} … {})", t.path(n), group(first), group((first + PAGE as u64).min(t.count(n) as u64) - 1)));
        }
        qs(&t.path(n))
    }
}
