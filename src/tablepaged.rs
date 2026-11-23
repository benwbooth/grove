//! Paged tables: big CSV/TSV files and SQLite databases, never read whole.
//!
//! The view asks for rows by page (PAGE rows); a worker thread answers
//! from the source and the model shows a page once it arrives.
//!
//! - A big CSV/TSV: a worker indexes where every PAGE-th row starts (one
//!   pass, quotes understood); pages are read from the file. Sorting,
//!   filtering, searching and SQL need a database: the first time one is
//!   asked for, the file is imported (in the background, with progress)
//!   into an on-disk SQLite file in the cache, deleted when the viewer
//!   closes.
//! - A SQLite database: opened read-only (`mode=ro`, never written); a
//!   list of its tables and views, each shown with paging by
//!   LIMIT/OFFSET; only reading statements run in the SQL box.
//!
//! A sorted/filtered view is materialised as a TEMP table of rowids (or of
//! the rows, for views and WITHOUT ROWID tables) so any page of it is one
//! quick lookup.

use super::{num, qobject, qs, quote_ident, View, SEP};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QList, QModelIndex, QString};
use rusqlite::types::{Value as SqlValue, ValueRef};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

/// Rows per page.
pub const PAGE: usize = 256;
/// CSV/TSV files from this size are paged (smaller ones are read whole).
pub const BIG_CSV: u64 = 100 << 20;
/// Pages kept in memory.
const CACHE_PAGES: usize = 48;
/// The most rows copied at once from a paged table.
const COPY_MAX: usize = 100_000;

pub struct Page {
    pub rows: Vec<Vec<String>>,
    pub nums: Vec<i64>,
}

pub enum Cmd {
    Page(u64, usize),
    View(u64, View),
    Query(u64, String),
    Table(u64, String),
    /// Columns in this order, shown rows first..=last, with the header line?
    Tsv(Vec<usize>, usize, usize, bool),
    Imported(Result<PathBuf, String>),
}

/// The UI's end of a paged table.
pub struct Paged {
    pub tx: Sender<Cmd>,
    pub interrupt: Arc<Mutex<Option<rusqlite::InterruptHandle>>>,
    pub cancel: Arc<AtomicBool>,
    pub vgen: u64,
    pub next_vgen: u64,
    pub count: usize,
    pub width: usize,
    /// Rows come from the file (the count grows while it's indexed).
    pub file_mode: bool,
    pub cache: RefCell<HashMap<usize, Arc<Page>>>,
    pub order: RefCell<VecDeque<usize>>,
    pub pending: RefCell<HashSet<usize>>,
    /// SQLite: (name, "table"/"view", rows or -1).
    pub tables: Vec<(String, String, i64)>,
}

impl Drop for Paged {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(h) = self.interrupt.lock().unwrap().as_ref() {
            h.interrupt();
        }
    }
}

impl Paged {
    pub fn new(tx: Sender<Cmd>, interrupt: Arc<Mutex<Option<rusqlite::InterruptHandle>>>, cancel: Arc<AtomicBool>) -> Self {
        Paged {
            tx,
            interrupt,
            cancel,
            vgen: 0,
            next_vgen: 0,
            count: 0,
            width: 0,
            file_mode: false,
            cache: RefCell::new(HashMap::new()),
            order: RefCell::new(VecDeque::new()),
            pending: RefCell::new(HashSet::new()),
            tables: vec![],
        }
    }

    /// The row's page, asking for it when it isn't here.
    pub fn row(&self, row: usize) -> Option<(Arc<Page>, usize)> {
        let p = row / PAGE;
        if let Some(pg) = self.cache.borrow().get(&p) {
            return Some((pg.clone(), row % PAGE));
        }
        if self.pending.borrow_mut().insert(p) {
            let _ = self.tx.send(Cmd::Page(self.vgen, p));
        }
        None
    }

    pub fn interrupt(&self) {
        if let Some(h) = self.interrupt.lock().unwrap().as_ref() {
            h.interrupt();
        }
    }

    pub fn clear(&mut self) {
        self.cache.borrow_mut().clear();
        self.order.borrow_mut().clear();
        self.pending.borrow_mut().clear();
    }

    pub fn tables_json(&self) -> String {
        json!(self.tables.iter().map(|(n, k, c)| json!({"name": n, "kind": k, "rows": c})).collect::<Vec<_>>()).to_string()
    }
}

// ---- CSV scanning ----

/// Finds where rows end in delimited text, as parse_delimited reads it
/// (quotes at a field's start, "" inside quotes, CR, LF or CRLF), fed a
/// chunk at a time.
pub struct Scanner {
    delim: u8,
    quoted: bool,
    maybe_end: bool,
    at_start: bool,
    prev_cr: bool,
}

impl Scanner {
    pub fn new(delim: u8) -> Self {
        Scanner { delim, quoted: false, maybe_end: false, at_start: true, prev_cr: false }
    }

    /// Calls `row_end(offset just past the row's line break)` for each row
    /// that ends in `chunk` (which starts at file offset `base`).
    pub fn feed(&mut self, chunk: &[u8], base: u64, mut row_end: impl FnMut(u64)) {
        for (i, &b) in chunk.iter().enumerate() {
            let at = base + i as u64;
            if self.prev_cr {
                self.prev_cr = false;
                if b == b'\n' {
                    row_end(at + 1);
                    continue;
                }
                row_end(at);
            }
            if self.quoted {
                if self.maybe_end {
                    self.maybe_end = false;
                    if b == b'"' {
                        continue;
                    }
                    self.quoted = false;
                } else {
                    if b == b'"' {
                        self.maybe_end = true;
                    }
                    continue;
                }
            }
            match b {
                b'"' if self.at_start => {
                    self.quoted = true;
                    self.at_start = false;
                }
                b'\n' => {
                    self.at_start = true;
                    row_end(at + 1);
                }
                b'\r' => {
                    self.at_start = true;
                    self.prev_cr = true;
                }
                _ if b == self.delim => self.at_start = true,
                _ => self.at_start = false,
            }
        }
    }

    /// End of the file: a pending CR ends its row there.
    pub fn finish(&mut self, end: u64, mut row_end: impl FnMut(u64)) {
        if self.prev_cr {
            self.prev_cr = false;
            row_end(end);
        }
    }
}

/// Where the pages of a CSV start.
#[derive(Default)]
pub struct CsvIndex {
    /// offsets[p]: the first byte of page p's first row; one more entry
    /// than complete pages (the start of the next).
    pub offsets: Vec<u64>,
    pub rows: usize,
    pub done: bool,
    pub size: u64,
}

impl CsvIndex {
    /// Rows the view may show now (complete pages while indexing).
    pub fn shown(&self) -> usize {
        if self.done { self.rows } else { self.offsets.len().saturating_sub(1) * PAGE }
    }
    /// Page p's bytes, when known.
    pub fn span(&self, p: usize) -> Option<(u64, u64)> {
        let start = *self.offsets.get(p)?;
        match self.offsets.get(p + 1) {
            Some(&e) => Some((start, e)),
            None if self.done => Some((start, self.size)),
            None => None,
        }
    }
}

struct Csv {
    path: String,
    delim: char,
    header: Vec<String>,
    nums: Vec<bool>,
    /// The columns as first measured (kept through sorting and filtering).
    cols_json: String,
    idx: Arc<Mutex<CsvIndex>>,
    file: std::fs::File,
}

fn read_span(f: &mut std::fs::File, start: u64, end: u64) -> Vec<u8> {
    let mut buf = vec![0u8; end.saturating_sub(start) as usize];
    if f.seek(SeekFrom::Start(start)).and_then(|_| f.read_exact(&mut buf)).is_err() {
        buf.clear();
    }
    buf
}

fn pad(mut rows: Vec<Vec<String>>, width: usize) -> Vec<Vec<String>> {
    for r in &mut rows {
        r.resize(width, String::new());
    }
    rows
}

// ---- SQL helpers ----

fn cell_string(v: ValueRef) -> String {
    match v {
        ValueRef::Null => String::new(),
        ValueRef::Integer(v) => v.to_string(),
        ValueRef::Real(v) => format!("{v}"),
        ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
        ValueRef::Blob(b) => format!("<{} bytes>", b.len()),
    }
}

fn like_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('%');
    for c in s.chars() {
        if c == '%' || c == '_' || c == '\\' {
            o.push('\\');
        }
        o.push(c);
    }
    o.push('%');
    o
}

/// WHERE and ORDER BY for a view over columns `cols` (no keywords when
/// empty), with their parameters.
pub fn view_sql(v: &View, cols: &[String], rowid: bool) -> (String, String, Vec<SqlValue>) {
    let mut conds = vec![];
    let mut params: Vec<SqlValue> = vec![];
    let text = |c: &str| format!("CAST({} AS TEXT)", quote_ident(c));
    if !v.search.is_empty() && !cols.is_empty() {
        params.push(SqlValue::Text(like_escape(&v.search)));
        let k = params.len();
        conds.push(format!("({})", cols.iter().map(|c| format!("{} LIKE ?{k} ESCAPE '\\'", text(c))).collect::<Vec<_>>().join(" OR ")));
    }
    for f in &v.filters {
        let Some(c) = cols.get(f.col) else { continue };
        let q = quote_ident(c);
        let empty = format!("({q} IS NULL OR trim({}) = '')", text(c));
        let val = f.value.trim();
        match f.op.as_str() {
            "empty" => conds.push(empty),
            "notempty" => conds.push(format!("NOT {empty}")),
            "contains" | "!contains" if !val.is_empty() => {
                params.push(SqlValue::Text(like_escape(val)));
                let k = params.len();
                if f.op == "contains" {
                    conds.push(format!("{} LIKE ?{k} ESCAPE '\\'", text(c)));
                } else {
                    conds.push(format!("({q} IS NULL OR {} NOT LIKE ?{k} ESCAPE '\\')", text(c)));
                }
            }
            op @ ("=" | "!=" | ">" | "<" | ">=" | "<=") if !val.is_empty() => {
                let op = if op == "!=" { "<>" } else { op };
                params.push(SqlValue::Text(val.to_string()));
                let kt = params.len();
                match f.num {
                    Some(n) => {
                        params.push(SqlValue::Real(n));
                        let kn = params.len();
                        conds.push(format!("(CASE WHEN typeof({q}) IN ('integer','real') THEN {q} {op} ?{kn} ELSE lower({}) {op} lower(?{kt}) END)", text(c)));
                    }
                    None => conds.push(format!("lower({}) {op} lower(?{kt})", text(c))),
                }
            }
            _ => {}
        }
    }
    let wh = if conds.is_empty() { String::new() } else { format!("WHERE {}", conds.join(" AND ")) };
    let ord = match cols.get(v.sort.max(0) as usize) {
        Some(c) if v.sort >= 0 => {
            let q = quote_ident(c);
            format!("ORDER BY ({q} IS NULL OR {} = ''), {q} COLLATE NOCASE {}{}", text(c), if v.desc { "DESC" } else { "ASC" }, if rowid { ", rowid" } else { "" })
        }
        _ => String::new(),
    };
    (wh, ord, params)
}

fn view_empty(v: &View) -> bool {
    v.search.is_empty() && v.sort < 0 && v.filters.iter().all(|f| !matches!(f.op.as_str(), "empty" | "notempty") && f.value.trim().is_empty())
}

/// A cell as a database value (numbers in numeric columns).
fn sql_value(c: &str, numeric: bool) -> SqlValue {
    if c.is_empty() {
        SqlValue::Null
    } else if numeric {
        match num(c) {
            Some(v) if v.fract() == 0.0 && v.abs() < 9e15 => SqlValue::Integer(v as i64),
            Some(v) => SqlValue::Real(v),
            None => SqlValue::Text(c.to_string()),
        }
    } else {
        SqlValue::Text(c.to_string())
    }
}

/// A file in the cache for an imported table; removed when dropped.
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(self.0.with_extension("sqlite-journal"));
    }
}

/// Copies left by a grove that's gone (quit or crashed with a big CSV
/// open): their names start with its process id.
fn clean_scratch(dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let pid = name.split('-').next().unwrap_or("");
        let alive = !pid.is_empty() && Path::new("/proc").join(pid).exists();
        let old = e.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok()).map(|d| d.as_secs() > 7 * 86400).unwrap_or(false);
        if !alive || old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

fn scratch_path() -> PathBuf {
    let dir = grove_core::util::cache_home().join("grove/tables");
    let _ = std::fs::create_dir_all(&dir);
    clean_scratch(&dir);
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    dir.join(format!("{}-{}.sqlite", std::process::id(), N.fetch_add(1, Ordering::Relaxed)))
}

/// Import a CSV into table `t` of a new database at `dest` (pages as the
/// index finds them). Progress 0..1.
fn import_csv(csv: &CsvShared, dest: &Path, cancel: &AtomicBool, progress: &dyn Fn(f64)) -> Result<(), String> {
    let db = rusqlite::Connection::open(dest).map_err(|e| e.to_string())?;
    db.execute_batch("PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF; PRAGMA cache_size=-65536;").map_err(|e| e.to_string())?;
    let cols: Vec<String> = csv.header.iter().enumerate().map(|(i, h)| format!("{} {}", quote_ident(h), if csv.nums[i] { "NUMERIC" } else { "TEXT" })).collect();
    db.execute_batch(&format!("CREATE TABLE t ({}); BEGIN;", cols.join(", "))).map_err(|e| e.to_string())?;
    let width = csv.header.len();
    let marks = vec!["?"; width].join(", ");
    let mut st = db.prepare(&format!("INSERT INTO t VALUES ({marks})")).map_err(|e| e.to_string())?;
    let mut f = std::fs::File::open(&csv.path).map_err(|e| e.to_string())?;
    let mut p = 0usize;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        // Up to 64 pages at a time, waiting for the index when it's behind.
        let (span, size, done) = {
            let ix = csv.idx.lock().unwrap();
            let last = (p + 64).min(ix.offsets.len().saturating_sub(1));
            let span = if last > p {
                Some((ix.offsets[p], ix.offsets[last], last))
            } else if ix.done {
                ix.span(p).map(|(s, e)| (s, e, p + 1))
            } else {
                None
            };
            (span, ix.size, ix.done)
        };
        let Some((start, end, next)) = span else {
            if done {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
            continue;
        };
        let bytes = read_span(&mut f, start, end);
        let text = String::from_utf8_lossy(&bytes);
        for r in super::parse_exact(&text, csv.delim, usize::MAX) {
            let vals: Vec<SqlValue> = (0..width).map(|i| sql_value(r.get(i).map(String::as_str).unwrap_or(""), csv.nums[i])).collect();
            st.execute(rusqlite::params_from_iter(vals)).map_err(|e| e.to_string())?;
        }
        progress(if size > 0 { end as f64 / size as f64 } else { 1.0 });
        p = next;
        if done && end >= size {
            break;
        }
    }
    drop(st);
    db.execute_batch("COMMIT;").map_err(|e| e.to_string())?;
    Ok(())
}

/// What the importer needs of the CSV.
struct CsvShared {
    path: String,
    delim: char,
    header: Vec<String>,
    nums: Vec<bool>,
    idx: Arc<Mutex<CsvIndex>>,
}

// ---- The worker ----

/// What the view shows rows of: a table (or the imported CSV `t`, or a
/// query's result `_grove_q`).
#[derive(Clone)]
struct Base {
    sql: String,
    rowid: bool,
    cols: Vec<String>,
    count: usize,
}

struct Mat {
    rowid: bool,
    count: usize,
}

struct Worker {
    qt: cxx_qt::CxxQtThread<qobject::TableData>,
    gen: u64,
    me: Sender<Cmd>,
    cancel: Arc<AtomicBool>,
    interrupt: Arc<Mutex<Option<rusqlite::InterruptHandle>>>,
    csv: Option<Csv>,
    conn: Option<rusqlite::Connection>,
    scratch: Option<Scratch>,
    importing: bool,
    waiting: Option<Cmd>,
    base: Option<Base>,
    mat: Option<Mat>,
    vgen: u64,
    view: View,
    querying: bool,
    is_db: bool,
    table: String,
    /// Parsed file pages, for showing the imported CSV's rows as written.
    file_pages: HashMap<usize, Arc<Vec<Vec<String>>>>,
    /// The unsorted base's rowid at the start of each page, found as far as
    /// pages were asked for: a deep page is then a key lookup, not an
    /// OFFSET that steps over every row before it (slower the deeper).
    keys: Option<(String, Vec<i64>)>,
}

pub struct Rows(Page);
unsafe impl Send for Rows {}

/// Open `path` as a paged table on a worker: a SQLite database (`db`) or a
/// big CSV/TSV. Runs on the caller's (worker) thread until the view closes.
pub fn run(qt: cxx_qt::CxxQtThread<qobject::TableData>, gen: u64, path: String, db: bool, delim: char, me: Sender<Cmd>, rx: Receiver<Cmd>, cancel: Arc<AtomicBool>, interrupt: Arc<Mutex<Option<rusqlite::InterruptHandle>>>) {
    let mut w = Worker {
        qt,
        gen,
        me,
        cancel,
        interrupt,
        csv: None,
        conn: None,
        scratch: None,
        importing: false,
        waiting: None,
        base: None,
        mat: None,
        vgen: 0,
        view: View { sort: -1, ..View::default() },
        querying: false,
        is_db: db,
        table: String::new(),
        file_pages: HashMap::new(),
        keys: None,
    };
    if !db {
        clean_scratch(&grove_core::util::cache_home().join("grove/tables"));
    }
    let ok = if db { w.open_db(&path) } else { w.open_csv(&path, delim) };
    if let Err(e) = ok {
        let _ = w.qt.queue(move |m| m.p_error(gen, e, false));
        return;
    }
    loop {
        if w.cancel.load(Ordering::Relaxed) {
            break;
        }
        let cmd = match rx.recv_timeout(std::time::Duration::from_millis(300)) {
            Ok(c) => c,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => break,
        };
        // Only the newest of a run of page requests matters most: answer
        // them newest first.
        let mut cmds = vec![cmd];
        while let Ok(c) = rx.try_recv() {
            cmds.push(c);
        }
        let mut pages = vec![];
        for c in cmds {
            match c {
                Cmd::Page(v, p) => pages.push((v, p)),
                other => {
                    for (v, p) in pages.drain(..).rev() {
                        w.page(v, p);
                    }
                    w.handle(other);
                }
            }
        }
        for (v, p) in pages.into_iter().rev() {
            w.page(v, p);
        }
    }
}

impl Worker {
    fn send<F: FnOnce(Pin<&mut qobject::TableData>) + Send + 'static>(&self, f: F) {
        let _ = self.qt.queue(f);
    }

    fn set_conn(&mut self, c: rusqlite::Connection) {
        *self.interrupt.lock().unwrap() = Some(c.get_interrupt_handle());
        self.conn = Some(c);
    }

    fn open_db(&mut self, path: &str) -> Result<(), String> {
        use rusqlite::OpenFlags;
        let enc = path.replace('%', "%25").replace('?', "%3f").replace('#', "%23");
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let probe = |c: &rusqlite::Connection| c.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0));
        let conn = match rusqlite::Connection::open_with_flags(format!("file:{enc}?mode=ro"), flags) {
            Ok(c) if probe(&c).is_ok() => c,
            // A WAL database in a folder we can't write: read it as it is.
            _ => {
                let c = rusqlite::Connection::open_with_flags(format!("file:{enc}?mode=ro&immutable=1"), flags).map_err(|e| format!("Can't open this database: {e}"))?;
                probe(&c).map_err(|e| format!("Can't read this database: {e}"))?;
                c
            }
        };
        let _ = conn.execute_batch("PRAGMA temp_store=FILE;");
        let mut tables = vec![];
        {
            let mut st = conn
                .prepare("SELECT name, type FROM sqlite_master WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY type, name COLLATE NOCASE")
                .map_err(|e| e.to_string())?;
            let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).map_err(|e| e.to_string())?;
            for r in rows.flatten() {
                tables.push(r);
            }
        }
        self.set_conn(conn);
        let gen = self.gen;
        let list: Vec<(String, String, i64)> = tables.iter().map(|(n, k)| (n.clone(), k.clone(), -1)).collect();
        self.send(move |m| m.p_tables(gen, list));
        // Row counts on a connection of their own (a big table's count
        // takes a moment).
        {
            let qt = self.qt.clone();
            let cancel = self.cancel.clone();
            let enc2 = enc.clone();
            let names: Vec<String> = tables.iter().filter(|(_, k)| k == "table").map(|(n, _)| n.clone()).collect();
            std::thread::Builder::new()
                .name("grove-sqlite-count".into())
                .spawn(move || {
                    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX;
                    let c = rusqlite::Connection::open_with_flags(format!("file:{enc2}?mode=ro"), flags)
                        .or_else(|_| rusqlite::Connection::open_with_flags(format!("file:{enc2}?mode=ro&immutable=1"), flags));
                    let Ok(c) = c else { return };
                    for n in names {
                        if cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        if let Ok(k) = c.query_row(&format!("SELECT count(*) FROM main.{}", quote_ident(&n)), [], |r| r.get::<_, i64>(0)) {
                            let _ = qt.queue(move |m| m.p_table_count(gen, n, k));
                        }
                    }
                })
                .ok();
        }
        match tables.first() {
            Some((n, _)) => {
                let n = n.clone();
                self.handle(Cmd::Table(0, n));
            }
            None => self.send(move |m| m.p_base(gen, 0, "[]".into(), 0, false, false)),
        }
        Ok(())
    }

    fn open_csv(&mut self, path: &str, delim: char) -> Result<(), String> {
        let p = Path::new(path);
        let mut file = std::fs::File::open(p).map_err(|e| grove_core::util::plain_error(&e, p))?;
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        // The first 4 MB: the header, the separator and the columns' widths.
        let mut head = vec![0u8; (4 << 20).min(size as usize)];
        let n = file.read(&mut head).map_err(|e| grove_core::util::plain_error(&e, p))?;
        head.truncate(n);
        let bom = if head.starts_with(b"\xef\xbb\xbf") { 3 } else { 0 };
        let text = String::from_utf8_lossy(&head[bom..]).into_owned();
        let delim = if delim == ',' {
            let first = text.lines().next().unwrap_or("");
            if first.matches(';').count() > first.matches(',').count() { ';' } else { ',' }
        } else {
            delim
        };
        // Header row end: the scanner's first row.
        let mut sc = Scanner::new(delim as u8);
        let mut header_end = None;
        sc.feed(&head[bom..], bom as u64, |e| {
            if header_end.is_none() {
                header_end = Some(e)
            }
        });
        let header_end = header_end.unwrap_or(size);
        let mut rows = super::parse_exact(&text, delim, 2001);
        if (header_end as usize) < head.len() || rows.len() <= 2000 {
            // A last partial row of the sample isn't whole: drop it.
            if rows.len() > 2000 || (n as u64) < size {
                rows.pop();
            }
        }
        let sheet = super::sheet_from("", rows);
        // Numbers further down are longer (ids): room for as many digits
        // as the file's rows (estimated from the sample).
        let sample_bytes = text.len().max(1);
        let est = (size as f64 * sheet.rows.len() as f64 / sample_bytes as f64) as usize;
        let cols_json = widen_numeric(&super::columns_json(&sheet), est);
        let nums: Vec<bool> = serde_json::from_str::<Vec<Value>>(&cols_json).unwrap_or_default().iter().map(|c| c["numeric"].as_bool().unwrap_or(false)).collect();
        let idx = Arc::new(Mutex::new(CsvIndex { offsets: vec![header_end], size, ..CsvIndex::default() }));
        self.csv = Some(Csv { path: path.to_string(), delim, header: sheet.header.clone(), nums, cols_json: cols_json.clone(), idx: idx.clone(), file });
        let gen = self.gen;
        self.send(move |m| m.p_base(gen, 0, cols_json, 0, false, true));
        // The index, on its own thread.
        let qt = self.qt.clone();
        let cancel = self.cancel.clone();
        let path = path.to_string();
        std::thread::Builder::new()
            .name("grove-csv-index".into())
            .spawn(move || index_csv(qt, gen, path, delim, header_end, idx, cancel))
            .ok();
        Ok(())
    }

    fn page(&mut self, v: u64, p: usize) {
        if v != self.vgen {
            return;
        }
        let page = if self.base.is_none() { self.file_page(p) } else { self.sql_page(p) };
        if let Some(pg) = page {
            let gen = self.gen;
            let r = Rows(pg);
            self.send(move |m| m.p_page(gen, v, p, r));
        }
    }

    fn file_page(&mut self, p: usize) -> Option<Page> {
        let csv = self.csv.as_mut()?;
        let span = csv.idx.lock().unwrap().span(p)?;
        let bytes = read_span(&mut csv.file, span.0, span.1);
        let text = String::from_utf8_lossy(&bytes);
        let rows = pad(super::parse_exact(&text, csv.delim, PAGE), csv.header.len());
        let nums = (0..rows.len()).map(|i| (p * PAGE + i + 1) as i64).collect();
        Some(Page { rows, nums })
    }

    /// The rowid that page `p` of the unsorted base starts at (extends the
    /// list of page starts from where it stopped, up to `p`).
    fn page_key(&mut self, p: usize) -> Option<i64> {
        let base = self.base.as_ref()?;
        if !base.rowid {
            return None;
        }
        let sql = base.sql.clone();
        if self.keys.as_ref().map(|(b, _)| b != &sql).unwrap_or(true) {
            self.keys = Some((sql.clone(), vec![]));
        }
        let have = self.keys.as_ref()?.1.len();
        if p < have {
            return self.keys.as_ref()?.1.get(p).copied();
        }
        let conn = self.conn.as_ref()?;
        // Continue after the last page start found (rows from there on).
        let (after, mut since) = match self.keys.as_ref()?.1.last() {
            Some(&k) => (k - 1, 0usize),
            None => (i64::MIN, 0usize),
        };
        let mut st = conn.prepare(&format!("SELECT rowid FROM {sql} WHERE rowid > ?1 ORDER BY rowid")).ok()?;
        let mut q = st.query([after]).ok()?;
        let mut found = vec![];
        let mut skip_first = have > 0; // that row starts a page we have
        while let Ok(Some(r)) = q.next() {
            let k: i64 = r.get(0).ok()?;
            if skip_first {
                skip_first = false;
                since = 1;
                continue;
            }
            if since % PAGE == 0 {
                found.push(k);
                if have + found.len() > p {
                    break;
                }
            }
            since += 1;
            if self.cancel.load(Ordering::Relaxed) {
                return None;
            }
        }
        drop(q);
        drop(st);
        let keys = &mut self.keys.as_mut()?.1;
        keys.extend(found);
        keys.get(p).copied()
    }

    fn sql_page(&mut self, p: usize) -> Option<Page> {
        // Unsorted rowid tables: by key (constant time at any depth).
        let key = if self.mat.is_none() && self.base.as_ref().map(|b| b.rowid).unwrap_or(false) { self.page_key(p) } else { None };
        let base = self.base.as_ref()?;
        let conn = self.conn.as_ref()?;
        let lo = (p * PAGE) as i64;
        let hi = lo + PAGE as i64;
        let width = base.cols.len();
        // ?1 the first row (0-based), ?2 the row after the last.
        let (sql, numbered) = match &self.mat {
            Some(m) if m.rowid => (format!("SELECT v.r, b.* FROM temp._grove_v v JOIN {} b ON b.rowid = v.r WHERE v.rowid > ?1 AND v.rowid <= ?2 ORDER BY v.rowid", base.sql), true),
            Some(_) => ("SELECT rowid, * FROM temp._grove_v WHERE rowid > ?1 AND rowid <= ?2 ORDER BY rowid".to_string(), true),
            None if base.rowid => (format!("SELECT rowid, * FROM {} LIMIT ?2 - ?1 OFFSET ?1", base.sql), true),
            None => (format!("SELECT * FROM {} LIMIT ?2 - ?1 OFFSET ?1", base.sql), false),
        };
        let (sql, args) = match key {
            Some(k) => (format!("SELECT rowid, * FROM {} WHERE rowid >= ?1 ORDER BY rowid LIMIT ?2", base.sql), [k, PAGE as i64]),
            None => (sql, [lo, hi]),
        };
        let mut st = conn.prepare_cached(&sql).ok()?;
        let n = st.column_count();
        let mut out = Page { rows: vec![], nums: vec![] };
        let mut q = st.query(args).ok()?;
        let mut i = 0;
        while let Ok(Some(r)) = q.next() {
            let first = if numbered { 1 } else { 0 };
            let num = if numbered { r.get::<_, i64>(0).unwrap_or(lo + i + 1) } else { lo + i + 1 };
            let mut row = Vec::with_capacity(width);
            for k in first..n {
                row.push(r.get_ref(k).map(cell_string).unwrap_or_default());
            }
            row.resize(width, String::new());
            out.rows.push(row);
            out.nums.push(num);
            i += 1;
        }
        drop(q);
        drop(st);
        // The imported CSV: its rows as the file has them (the database
        // keeps numbers as numbers: 4449.80 would show as 4449.8).
        if self.csv.is_some() && !self.querying && self.base.as_ref().map(|b| b.sql == "main.t").unwrap_or(false) {
            for (k, &n) in out.nums.iter().enumerate() {
                if n < 1 {
                    continue;
                }
                let row = (n - 1) as usize;
                let fp = row / PAGE;
                if !self.file_pages.contains_key(&fp) {
                    if self.file_pages.len() >= 96 {
                        self.file_pages.clear();
                    }
                    let Some(pg) = self.file_page(fp) else { continue };
                    self.file_pages.insert(fp, Arc::new(pg.rows));
                }
                if let Some(r) = self.file_pages.get(&fp).and_then(|p| p.get(row % PAGE)) {
                    out.rows[k] = r.clone();
                }
            }
        }
        Some(out)
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Page(..) => {}
            Cmd::View(v, view) => {
                self.vgen = v;
                self.view = view;
                if self.csv.is_some() && !self.querying && view_empty(&self.view) {
                    // The file itself, in its own order.
                    return self.file_base(v);
                }
                if self.conn.is_none() {
                    return self.need_db(Cmd::View(v, self.view.clone()));
                }
                if self.base.is_none() {
                    self.base = self.t_base();
                }
                self.apply_view(v);
            }
            Cmd::Query(v, sql) => {
                self.vgen = v;
                if sql.trim().is_empty() {
                    self.querying = false;
                    self.view = View { sort: -1, ..View::default() };
                    let _ = self.conn.as_ref().map(|c| c.execute_batch("DROP TABLE IF EXISTS temp._grove_q; DROP TABLE IF EXISTS temp._grove_v;"));
                    self.mat = None;
                    if self.is_db {
                        let t = self.table.clone();
                        return self.handle(Cmd::Table(v, t));
                    }
                    self.base = None;
                    return self.file_base(v);
                }
                if self.conn.is_none() {
                    return self.need_db(Cmd::Query(v, sql));
                }
                self.query(v, &sql);
            }
            Cmd::Table(v, name) => {
                self.vgen = v;
                self.querying = false;
                self.table = name.clone();
                self.view = View { sort: -1, ..View::default() };
                self.mat = None;
                let Some(conn) = self.conn.as_ref() else { return };
                let _ = conn.execute_batch("DROP TABLE IF EXISTS temp._grove_v; DROP TABLE IF EXISTS temp._grove_q; DROP TABLE IF EXISTS temp._grove_m;");
                let is_view = conn.query_row("SELECT type = 'view' FROM sqlite_master WHERE name = ?1", [&name], |r| r.get::<_, bool>(0)).unwrap_or(false);
                // Schema: the CREATE statement and the table's indexes.
                let mut schema = String::new();
                if let Ok(mut st) = conn.prepare("SELECT sql FROM sqlite_master WHERE (name = ?1 OR tbl_name = ?1) AND sql IS NOT NULL ORDER BY type = 'index', name") {
                    if let Ok(rows) = st.query_map([&name], |r| r.get::<_, String>(0)) {
                        for s in rows.flatten() {
                            schema.push_str(s.trim());
                            schema.push_str(";\n\n");
                        }
                    }
                }
                let mut sql = format!("main.{}", quote_ident(&name));
                let gen = self.gen;
                let schema = schema.trim_end().to_string();
                self.send(move |m| m.p_schema(gen, name, schema));
                // A view runs once (into a temporary table), not once a page.
                if is_view {
                    let Some(conn) = self.conn.as_ref() else { return };
                    if let Err(e) = conn.execute_batch(&format!("CREATE TEMP TABLE _grove_m AS SELECT * FROM {sql}")) {
                        if !is_interrupt(&e) {
                            let e = e.to_string();
                            self.send(move |m| m.p_error(gen, e, true));
                        }
                        return;
                    }
                    sql = "temp._grove_m".into();
                }
                match self.make_base(sql, true) {
                    Ok(b) => {
                        self.base = Some(b);
                        self.send_base(v);
                    }
                    Err(e) => self.send(move |m| m.p_error(gen, e, false)),
                }
            }
            Cmd::Tsv(order, first, last, header) => self.tsv(order, first, last, header),
            Cmd::Imported(r) => {
                self.importing = false;
                let gen = self.gen;
                match r.and_then(|p| rusqlite::Connection::open(p).map_err(|e| e.to_string())) {
                    Ok(c) => {
                        let _ = c.execute_batch("PRAGMA temp_store=FILE; PRAGMA cache_size=-16384;");
                        self.set_conn(c);
                        self.send(move |m| m.p_progress(gen, -1.0));
                        if let Some(w) = self.waiting.take() {
                            self.handle(w);
                        }
                    }
                    Err(e) => {
                        self.scratch = None;
                        self.send(move |m| m.p_progress(gen, -1.0));
                        self.send(move |m| m.p_error(gen, format!("Couldn't prepare the table for sorting and SQL: {e}"), true));
                    }
                }
            }
        }
    }

    fn t_base(&self) -> Option<Base> {
        let csv = self.csv.as_ref()?;
        let count = csv.idx.lock().unwrap().rows;
        Some(Base { sql: "main.t".into(), rowid: true, cols: csv.header.clone(), count })
    }

    fn file_base(&mut self, v: u64) {
        self.mat = None;
        self.base = None;
        let Some(csv) = self.csv.as_ref() else { return };
        let count = csv.idx.lock().unwrap().shown();
        let cols = csv.cols_json.clone();
        let gen = self.gen;
        self.send(move |m| m.p_base(gen, v, cols, count, false, true));
    }

    /// Sorting, filtering or SQL on a CSV: import it first (once).
    fn need_db(&mut self, cmd: Cmd) {
        self.waiting = Some(cmd);
        if self.importing || self.is_db {
            return;
        }
        let Some(csv) = self.csv.as_ref() else { return };
        self.importing = true;
        let shared = CsvShared { path: csv.path.clone(), delim: csv.delim, header: csv.header.clone(), nums: csv.nums.clone(), idx: csv.idx.clone() };
        let dest = scratch_path();
        self.scratch = Some(Scratch(dest.clone()));
        let me = self.me.clone();
        let cancel = self.cancel.clone();
        let qt = self.qt.clone();
        let gen = self.gen;
        self.send(move |m| m.p_progress(gen, 0.0));
        std::thread::Builder::new()
            .name("grove-csv-import".into())
            .spawn(move || {
                let last = std::cell::Cell::new(std::time::Instant::now());
                let progress = |f: f64| {
                    if last.get().elapsed().as_millis() >= 150 {
                        last.set(std::time::Instant::now());
                        let _ = qt.queue(move |m| m.p_progress(gen, f));
                    }
                };
                let r = import_csv(&shared, &dest, &cancel, &progress).map(|_| dest);
                if !cancel.load(Ordering::Relaxed) {
                    let _ = me.send(Cmd::Imported(r));
                }
            })
            .ok();
    }

    /// A base relation: its columns and count (and whether it has rowids).
    fn make_base(&self, sql: String, rowid_maybe: bool) -> Result<Base, String> {
        let conn = self.conn.as_ref().ok_or("No database")?;
        let rowid = rowid_maybe && conn.prepare(&format!("SELECT rowid FROM {sql} LIMIT 0")).is_ok();
        let st = conn.prepare(&format!("SELECT * FROM {sql} LIMIT 0")).map_err(|e| e.to_string())?;
        let cols: Vec<String> = st.column_names().map(|s| s.to_string()).collect();
        drop(st);
        let count = conn.query_row(&format!("SELECT count(*) FROM {sql}"), [], |r| r.get::<_, i64>(0)).map_err(|e| e.to_string())? as usize;
        Ok(Base { sql, rowid, cols, count })
    }

    /// Columns JSON from a sample of the shown rows.
    fn columns_now(&self) -> String {
        if let Some(csv) = self.csv.as_ref().filter(|_| !self.querying) {
            return csv.cols_json.clone();
        }
        let Some(base) = self.base.as_ref() else { return "[]".into() };
        let mut sheet = super::Sheet { name: String::new(), header: base.cols.clone(), rows: vec![] };
        // Same unique, non-empty names as a sheet's.
        sheet = super::sheet_from("", std::iter::once(sheet.header.clone()).collect());
        if let Some(conn) = self.conn.as_ref() {
            if let Ok(mut st) = conn.prepare(&format!("SELECT * FROM {} LIMIT 300", base.sql)) {
                let n = st.column_count();
                if let Ok(mut q) = st.query([]) {
                    while let Ok(Some(r)) = q.next() {
                        let mut row: Vec<String> = (0..n).map(|k| r.get_ref(k).map(cell_string).unwrap_or_default()).collect();
                        row.resize(sheet.header.len(), String::new());
                        sheet.rows.push(row);
                    }
                }
            }
        }
        widen_numeric(&super::columns_json(&sheet), base.count)
    }

    fn send_base(&mut self, v: u64) {
        let cols = self.columns_now();
        let count = self.mat.as_ref().map(|m| m.count).or(self.base.as_ref().map(|b| b.count)).unwrap_or(0);
        let gen = self.gen;
        let q = self.querying;
        self.send(move |m| m.p_base(gen, v, cols, count, q, false));
    }

    fn apply_view(&mut self, v: u64) {
        let Some(base) = self.base.clone() else { return };
        let Some(conn) = self.conn.as_ref() else { return };
        let _ = conn.execute_batch("DROP TABLE IF EXISTS temp._grove_v;");
        self.mat = None;
        if view_empty(&self.view) {
            return self.send_base(v);
        }
        let gen = self.gen;
        let (wh, ord, params) = view_sql(&self.view, &base.cols, base.rowid);
        let what = if base.rowid { "rowid AS r" } else { "*" };
        let sql = format!("CREATE TEMP TABLE _grove_v AS SELECT {what} FROM {} {wh} {ord}", base.sql);
        match conn.execute(&sql, rusqlite::params_from_iter(params)) {
            Ok(_) => {
                let count = conn.query_row("SELECT count(*) FROM temp._grove_v", [], |r| r.get::<_, i64>(0)).unwrap_or(0) as usize;
                self.mat = Some(Mat { rowid: base.rowid, count });
                self.send_base(v);
            }
            Err(e) => {
                if !is_interrupt(&e) {
                    let e = e.to_string();
                    self.send(move |m| m.p_error(gen, e, true));
                }
            }
        }
    }

    fn query(&mut self, v: u64, sql: &str) {
        let gen = self.gen;
        let Some(conn) = self.conn.as_ref() else { return };
        let sql = sql.trim().trim_end_matches(';').trim().to_string();
        let r: Result<(), String> = (|| {
            let st = conn.prepare(&sql).map_err(|e| e.to_string())?;
            if !st.readonly() {
                return Err(if self.is_db { "Only queries that read run here (the database is opened read-only and never changed)".into() } else { "Only queries that read run here".into() });
            }
            let lower = sql.to_lowercase();
            conn.execute_batch("DROP TABLE IF EXISTS temp._grove_v; DROP TABLE IF EXISTS temp._grove_q;").map_err(|e| e.to_string())?;
            if lower.starts_with("select") || lower.starts_with("with") || lower.starts_with("values") {
                drop(st);
                conn.execute_batch(&format!("CREATE TEMP TABLE _grove_q AS {sql}")).map_err(|e| e.to_string())?;
            } else {
                // PRAGMA, EXPLAIN...: their rows into a table.
                let mut st = st;
                let names: Vec<String> = st.column_names().map(|s| s.to_string()).collect();
                let n = names.len().max(1);
                let mut rows: Vec<Vec<SqlValue>> = vec![];
                {
                    let mut q = st.query([]).map_err(|e| e.to_string())?;
                    while let Some(r) = q.next().map_err(|e| e.to_string())? {
                        rows.push((0..names.len()).map(|k| r.get::<_, SqlValue>(k).unwrap_or(SqlValue::Null)).collect());
                        if rows.len() >= 200_000 {
                            break;
                        }
                    }
                }
                let cols: Vec<String> = if names.is_empty() {
                    vec![quote_ident("result")]
                } else {
                    names.iter().enumerate().map(|(i, c)| if c.is_empty() || names[..i].contains(c) { quote_ident(&format!("{c}_{i}")) } else { quote_ident(c) }).collect()
                };
                conn.execute_batch(&format!("CREATE TEMP TABLE _grove_q ({})", cols.join(", "))).map_err(|e| e.to_string())?;
                let marks = vec!["?"; n].join(", ");
                let mut ins = conn.prepare(&format!("INSERT INTO temp._grove_q VALUES ({marks})")).map_err(|e| e.to_string())?;
                for r in rows {
                    let mut r = r;
                    r.resize(n, SqlValue::Null);
                    ins.execute(rusqlite::params_from_iter(r)).map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        })();
        match r {
            Err(e) => {
                if e.contains("interrupted") {
                    return;
                }
                self.send(move |m| m.p_error(gen, e, true));
            }
            Ok(()) => {
                self.querying = true;
                self.view = View { sort: -1, ..View::default() };
                self.mat = None;
                match self.make_base("temp._grove_q".into(), true) {
                    Ok(b) => {
                        self.base = Some(b);
                        self.send_base(v);
                    }
                    Err(e) => self.send(move |m| m.p_error(gen, e, true)),
                }
            }
        }
    }

    /// The shown rows (up to COPY_MAX) as TSV, columns in `order`.
    fn tsv(&mut self, order: Vec<usize>, first: usize, last: usize, header_line: bool) {
        let header: Vec<String> = match (&self.base, &self.csv) {
            (Some(b), _) => b.cols.clone(),
            (None, Some(c)) => c.header.clone(),
            _ => vec![],
        };
        let clean = |c: &str| c.replace(['\t', '\n', '\r'], " ");
        let line = |cells: &Vec<String>| order.iter().map(|&c| clean(cells.get(c).map(String::as_str).unwrap_or(""))).collect::<Vec<_>>().join("\t") + "\n";
        let mut out = if header_line { line(&header) } else { String::new() };
        let mut p = first / PAGE;
        let mut n = 0;
        let v = self.vgen;
        while n < COPY_MAX {
            if self.cancel.load(Ordering::Relaxed) {
                return;
            }
            let pg = if self.base.is_none() { self.file_page(p) } else { self.sql_page(p) };
            let Some(pg) = pg else { break };
            if pg.rows.is_empty() {
                break;
            }
            for (k, r) in pg.rows.iter().enumerate() {
                let at = p * PAGE + k;
                if at < first {
                    continue;
                }
                if at > last || n >= COPY_MAX {
                    break;
                }
                out.push_str(&line(r));
                n += 1;
            }
            if (p + 1) * PAGE > last {
                break;
            }
            p += 1;
        }
        let gen = self.gen;
        let _ = v;
        self.send(move |m| m.p_tsv(gen, out, n));
    }
}

fn is_interrupt(e: &rusqlite::Error) -> bool {
    matches!(e, rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::OperationInterrupted)
}

/// Numeric columns at least as wide as `rows` written out (ids).
fn widen_numeric(cols: &str, rows: usize) -> String {
    let digits = rows.max(1).to_string().len();
    let mut v: Vec<Value> = serde_json::from_str(cols).unwrap_or_default();
    for c in v.iter_mut() {
        if c["numeric"].as_bool() == Some(true) {
            let w = c["width"].as_u64().unwrap_or(3) as usize;
            c["width"] = json!(w.max(digits + 1).min(48));
        }
    }
    json!(v).to_string()
}

/// Index a CSV's pages (where every PAGE-th row starts), telling the view
/// the row count as it grows.
fn index_csv(qt: cxx_qt::CxxQtThread<qobject::TableData>, gen: u64, path: String, delim: char, header_end: u64, idx: Arc<Mutex<CsvIndex>>, cancel: Arc<AtomicBool>) {
    let Ok(mut f) = std::fs::File::open(&path) else { return };
    if f.seek(SeekFrom::Start(header_end)).is_err() {
        return;
    }
    let mut sc = Scanner::new(delim as u8);
    let mut buf = vec![0u8; 4 << 20];
    let mut pos = header_end;
    let mut rows = 0usize;
    let mut last_end = header_end;
    let mut new_offsets = vec![];
    let mut last_send = std::time::Instant::now();
    let mut first = true;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let n = match f.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        sc.feed(&buf[..n], pos, |e| {
            rows += 1;
            last_end = e;
            if rows % PAGE == 0 {
                new_offsets.push(e);
            }
        });
        pos += n as u64;
        if !new_offsets.is_empty() || last_send.elapsed().as_millis() >= 200 {
            let shown = {
                let mut ix = idx.lock().unwrap();
                ix.offsets.append(&mut new_offsets);
                ix.rows = rows;
                ix.shown()
            };
            if first || last_send.elapsed().as_millis() >= 200 {
                first = false;
                last_send = std::time::Instant::now();
                let _ = qt.queue(move |m| m.p_rows(gen, shown, false));
            }
        }
    }
    sc.finish(pos, |e| {
        rows += 1;
        last_end = e;
    });
    // A last row without a line break.
    if pos > last_end {
        rows += 1;
    }
    let shown = {
        let mut ix = idx.lock().unwrap();
        ix.offsets.append(&mut new_offsets);
        // A final page boundary exactly at the end isn't a page.
        if rows % PAGE == 0 && ix.offsets.len() > 1 && *ix.offsets.last().unwrap() >= pos {
            ix.offsets.pop();
        }
        ix.rows = rows;
        ix.size = pos;
        ix.done = true;
        ix.shown()
    };
    let _ = qt.queue(move |m| m.p_rows(gen, shown, true));
}

// ---- The model's side ----

impl qobject::TableData {
    pub(super) fn paged_cells(&self, row: usize) -> (String, i32) {
        let Some(pg) = self.rust().paged_state.as_ref() else { return (String::new(), 0) };
        match pg.row(row) {
            Some((page, i)) => match page.rows.get(i) {
                Some(cells) => {
                    let mut s = String::new();
                    for (k, c) in cells.iter().enumerate() {
                        if k > 0 {
                            s.push(SEP);
                        }
                        let first: String = c.chars().take(400).map(|ch| if ch == '\n' || ch == '\r' || ch == SEP { ' ' } else { ch }).collect();
                        s.push_str(&first);
                    }
                    (s, page.nums.get(i).copied().unwrap_or(row as i64 + 1) as i32)
                }
                None => (String::new(), row as i32 + 1),
            },
            None => (SEP.to_string().repeat(pg.width.saturating_sub(1)), row as i32 + 1),
        }
    }

    pub(super) fn paged_cell(&self, row: usize, col: usize) -> QString {
        let Some(pg) = self.rust().paged_state.as_ref() else { return QString::default() };
        let p = row / PAGE;
        let cache = pg.cache.borrow();
        cache.get(&p).and_then(|page| page.rows.get(row % PAGE)).and_then(|r| r.get(col)).map(|c| qs(c)).unwrap_or_default()
    }

    /// Start a paged view of `path` (on the thread the caller made).
    pub(super) fn p_open(mut self: Pin<&mut Self>, gen: u64, is_db: bool) {
        if gen != self.rust().gen {
            return;
        }
        let Some(link) = self.as_mut().rust_mut().link.take() else { return };
        self.as_mut().rust_mut().paged_state = Some(Paged::new(link.0, link.1, link.2));
        self.as_mut().set_paged(true);
        self.as_mut().set_is_db(is_db);
    }

    pub(super) fn p_error(mut self: Pin<&mut Self>, gen: u64, e: String, query: bool) {
        if gen != self.rust().gen {
            return;
        }
        self.as_mut().set_loading(false);
        self.as_mut().set_busy(false);
        if query {
            self.as_mut().set_query_error(qs(&e));
        } else {
            self.as_mut().set_error(qs(&e));
        }
    }

    pub(super) fn p_base(mut self: Pin<&mut Self>, gen: u64, v: u64, cols: String, count: usize, querying: bool, file_mode: bool) {
        if gen != self.rust().gen || self.rust().paged_state.is_none() {
            return;
        }
        if v < self.rust().paged_state.as_ref().unwrap().next_vgen {
            // A newer view is on its way.
            return;
        }
        let width = serde_json::from_str::<Vec<Value>>(&cols).map(|c| c.len()).unwrap_or(0);
        self.as_mut().window_home();
        unsafe {
            self.as_mut().begin_reset_model();
        }
        {
            let mut r = self.as_mut().rust_mut();
            let pg = r.paged_state.as_mut().unwrap();
            pg.clear();
            pg.vgen = v;
            pg.count = count;
            pg.width = width;
            pg.file_mode = file_mode;
        }
        unsafe {
            self.as_mut().end_reset_model();
        }
        self.as_mut().set_columns(qs(&cols));
        self.as_mut().set_querying(querying);
        self.as_mut().set_query_error(QString::default());
        self.as_mut().set_error(QString::default());
        self.as_mut().set_count(count.min(i32::MAX as usize) as i32);
        // The total: rows in the table (a search or filter shows "n of total").
        let unfiltered = self.rust().view.search.is_empty() && self.rust().view.filters.iter().all(|f| f.value.trim().is_empty() && f.op != "empty" && f.op != "notempty");
        if querying || (unfiltered && !self.rust().indexing) {
            self.as_mut().set_total(count.min(i32::MAX as usize) as i32);
        }
        self.as_mut().set_loading(false);
        self.as_mut().set_busy(false);
    }

    pub(super) fn p_rows(mut self: Pin<&mut Self>, gen: u64, shown: usize, done: bool) {
        if gen != self.rust().gen {
            return;
        }
        self.as_mut().set_indexing(!done);
        self.as_mut().set_total(shown.min(i32::MAX as usize) as i32);
        let Some(pg) = self.rust().paged_state.as_ref() else { return };
        if !pg.file_mode || shown <= pg.count {
            return;
        }
        // Rows added inside the model's window only (it holds WINDOW rows).
        let base = self.rust().row_base.max(0) as usize;
        let had = pg.count.saturating_sub(base).min(super::WINDOW);
        let now = shown.saturating_sub(base).min(super::WINDOW);
        if now > had {
            unsafe {
                self.as_mut().begin_insert_rows(&QModelIndex::default(), had as i32, now as i32 - 1);
            }
            self.as_mut().rust_mut().paged_state.as_mut().unwrap().count = shown;
            unsafe {
                self.as_mut().end_insert_rows();
            }
        } else {
            self.as_mut().rust_mut().paged_state.as_mut().unwrap().count = shown;
        }
        self.as_mut().set_count(shown.min(i32::MAX as usize) as i32);
    }

    pub(super) fn p_page(mut self: Pin<&mut Self>, gen: u64, v: u64, p: usize, rows: Rows) {
        if gen != self.rust().gen {
            return;
        }
        let (first, last) = {
            let r = self.rust();
            let Some(pg) = r.paged_state.as_ref() else { return };
            pg.pending.borrow_mut().remove(&p);
            if v != pg.vgen {
                return;
            }
            let n = rows.0.rows.len();
            pg.cache.borrow_mut().insert(p, Arc::new(rows.0));
            let mut order = pg.order.borrow_mut();
            order.push_back(p);
            while order.len() > CACHE_PAGES {
                if let Some(old) = order.pop_front() {
                    pg.cache.borrow_mut().remove(&old);
                }
            }
            if n == 0 {
                return;
            }
            let first = p * PAGE;
            // In the model's window (rows from row_base, at most WINDOW).
            let base = r.row_base.max(0) as usize;
            let last = (first + n).min(pg.count).max(first + 1) - 1;
            let end = (base + super::WINDOW).min(pg.count);
            if last < base || first >= end {
                return;
            }
            ((first.max(base) - base) as i32, (last.min(end - 1) - base) as i32)
        };
        let a = self.index(first, 0, &QModelIndex::default());
        let b = self.index(last, 0, &QModelIndex::default());
        self.as_mut().data_changed(&a, &b, &QList::<i32>::default());
    }

    pub(super) fn p_progress(mut self: Pin<&mut Self>, gen: u64, f: f64) {
        if gen == self.rust().gen {
            self.as_mut().set_import_progress(f);
        }
    }

    pub(super) fn p_tables(mut self: Pin<&mut Self>, gen: u64, list: Vec<(String, String, i64)>) {
        if gen != self.rust().gen {
            return;
        }
        let j = {
            let mut r = self.as_mut().rust_mut();
            let Some(pg) = r.paged_state.as_mut() else { return };
            pg.tables = list;
            pg.tables_json()
        };
        self.as_mut().set_tables(qs(&j));
    }

    pub(super) fn p_table_count(mut self: Pin<&mut Self>, gen: u64, name: String, n: i64) {
        if gen != self.rust().gen {
            return;
        }
        let j = {
            let mut r = self.as_mut().rust_mut();
            let Some(pg) = r.paged_state.as_mut() else { return };
            if let Some(t) = pg.tables.iter_mut().find(|t| t.0 == name) {
                t.2 = n;
            }
            pg.tables_json()
        };
        self.as_mut().set_tables(qs(&j));
    }

    pub(super) fn p_schema(mut self: Pin<&mut Self>, gen: u64, name: String, schema: String) {
        if gen != self.rust().gen {
            return;
        }
        self.as_mut().set_table(qs(&name));
        self.as_mut().set_schema(qs(&schema));
    }

    pub(super) fn p_tsv(mut self: Pin<&mut Self>, gen: u64, text: String, rows: usize) {
        if gen == self.rust().gen {
            self.as_mut().tsv_ready(qs(&text), rows as i32);
        }
    }

    /// A new view (sort/search/filters) for a paged table.
    pub(super) fn p_view(mut self: Pin<&mut Self>) {
        let view = self.rust().view.clone();
        {
            let mut r = self.as_mut().rust_mut();
            let Some(pg) = r.paged_state.as_mut() else { return };
            pg.next_vgen += 1;
            let v = pg.next_vgen;
            pg.interrupt();
            let _ = pg.tx.send(Cmd::View(v, view));
        }
        self.as_mut().set_busy(true);
    }

    pub(super) fn p_query(mut self: Pin<&mut Self>, sql: String) {
        {
            let mut r = self.as_mut().rust_mut();
            let Some(pg) = r.paged_state.as_mut() else { return };
            pg.next_vgen += 1;
            let v = pg.next_vgen;
            pg.interrupt();
            let _ = pg.tx.send(Cmd::Query(v, sql));
        }
        self.as_mut().set_busy(true);
    }

    pub(super) fn p_table(mut self: Pin<&mut Self>, name: String) {
        self.as_mut().rust_mut().view = View { sort: -1, ..View::default() };
        self.as_mut().set_sort_column(-1);
        self.as_mut().set_descending(false);
        {
            let mut r = self.as_mut().rust_mut();
            let Some(pg) = r.paged_state.as_mut() else { return };
            pg.next_vgen += 1;
            let v = pg.next_vgen;
            pg.interrupt();
            let _ = pg.tx.send(Cmd::Table(v, name));
        }
        self.as_mut().set_busy(true);
    }

    pub(super) fn p_tsv_request(&self, order: Vec<usize>, first: usize, last: usize, header: bool) {
        if let Some(pg) = self.rust().paged_state.as_ref() {
            let _ = pg.tx.send(Cmd::Tsv(order, first, last, header));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ends(text: &str, chunk: usize) -> Vec<u64> {
        let mut sc = Scanner::new(b',');
        let mut out = vec![];
        let b = text.as_bytes();
        let mut pos = 0;
        for c in b.chunks(chunk) {
            sc.feed(c, pos, |e| out.push(e));
            pos += c.len() as u64;
        }
        sc.finish(pos, |e| out.push(e));
        out
    }

    #[test]
    fn scanner_matches_parser() {
        let t = "a,b\n1,\"x,\"\"y\n\"\"z\"\r\n2,3\r4,5\n\"q\"\"\",6\n";
        for chunk in [1, 2, 3, 7, 100] {
            assert_eq!(ends(t, chunk), vec![4, 19, 23, 27, 35], "chunk {chunk}");
        }
        // Every row the scanner finds starts where the parser's does.
        let rows = super::super::parse_exact(t, ',', usize::MAX);
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[1], ["1", "x,\"y\n\"z"]);
    }

    #[test]
    fn view_sql_shapes() {
        let cols = vec!["name".to_string(), "n".to_string()];
        let v = View { search: "a%".into(), sort: 1, desc: true, filters: vec![super::super::Filter { col: 1, op: ">".into(), value: "9".into(), num: Some(9.0) }] };
        let (wh, ord, params) = view_sql(&v, &cols, true);
        assert!(wh.starts_with("WHERE ("));
        assert!(ord.contains("DESC, rowid"));
        assert_eq!(params.len(), 3);
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE t (name TEXT, n NUMERIC); INSERT INTO t VALUES ('a%x', 10), ('a%y', 100), ('b', 50), ('a%z', 5), ('a%w', NULL);").unwrap();
        let mut st = db.prepare(&format!("SELECT name FROM t {wh} {ord}")).unwrap();
        let got: Vec<String> = st.query_map(rusqlite::params_from_iter(params), |r| r.get(0)).unwrap().flatten().collect();
        assert_eq!(got, ["a%y", "a%x"]);
    }
}
