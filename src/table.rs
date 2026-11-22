//! TableData: CSV, TSV and spreadsheets (xlsx, xls, xlsm, xlsb, ods) for
//! Quick Look's table view. The whole file is read on a worker thread;
//! sorting, searching, column filters and SQL queries run there too, and
//! the view gets the rows that pass as a list model (`cells`: the row's
//! cells joined by U+001F; column order and hiding are the view's).
//!
//! SQL: an in-memory SQLite database (made on the first query) with the
//! current sheet as table `t` and every sheet under its own name too.

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};

#[path = "tablepaged.rs"]
mod paged;
pub use paged::BIG_CSV;
use serde_json::{json, Value};
use std::cmp::Ordering as CmpOrdering;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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
        #[qproperty(bool, loading)]
        /// Sorting/filtering/querying in progress.
        #[qproperty(bool, busy)]
        #[qproperty(QString, error)]
        /// JSON: ["Sheet1", ...] (one "" for CSV/TSV).
        #[qproperty(QString, sheets)]
        #[qproperty(i32, sheet)]
        /// JSON: [{"name", "width" (characters), "numeric"}, ...]
        #[qproperty(QString, columns)]
        /// Rows shown / rows in the sheet (or query result).
        #[qproperty(i32, count)]
        #[qproperty(i32, total)]
        #[qproperty(i32, sort_column, cxx_name = "sortColumn")]
        #[qproperty(bool, descending)]
        /// A query's result is shown (instead of the sheet).
        #[qproperty(bool, querying)]
        #[qproperty(QString, query_error, cxx_name = "queryError")]
        /// Rows are read a page at a time (a big CSV, a SQLite database).
        #[qproperty(bool, paged)]
        /// A big CSV is still being indexed (the count grows).
        #[qproperty(bool, indexing)]
        /// Importing a big CSV for sorting/SQL: 0..1 (-1: not importing).
        #[qproperty(f64, import_progress, cxx_name = "importProgress")]
        /// A SQLite database: its tables ([{"name", "kind", "rows"}]), the
        /// one shown and its schema (CREATE statements).
        #[qproperty(bool, is_db, cxx_name = "isDb")]
        #[qproperty(QString, tables)]
        #[qproperty(QString, table)]
        #[qproperty(QString, schema)]
        /// The first row the model's rows start at: it holds at most WINDOW
        /// rows (deep rows of a huge table sat at positions too large for
        /// the scene graph's floats: uneven gaps). The view moves it.
        #[qproperty(i32, row_base, cxx_name = "rowBase")]
        type TableData = super::TableDataRust;
    }

    impl cxx_qt::Threading for TableData {}

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut TableData>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut TableData>);
        #[inherit]
        #[cxx_name = "beginInsertRows"]
        unsafe fn begin_insert_rows(self: Pin<&mut TableData>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut TableData>);
        #[inherit]
        fn index(self: &TableData, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(self: Pin<&mut TableData>, top_left: &QModelIndex, bottom_right: &QModelIndex, roles: &QList_i32);
        /// The rows asked for with requestTsv (paged tables).
        #[qsignal]
        #[cxx_name = "tsvReady"]
        fn tsv_ready(self: Pin<&mut TableData>, text: QString, rows: i32);
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &TableData, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &TableData) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &TableData, parent: &QModelIndex) -> i32;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        /// Read a CSV/TSV/spreadsheet ("" clears).
        #[qinvokable]
        fn load(self: Pin<&mut TableData>, path: &QString);
        #[qinvokable]
        fn show_sheet(self: Pin<&mut TableData>, index: i32);
        /// Sort by a column (-1: file order).
        #[qinvokable]
        fn sort_by(self: Pin<&mut TableData>, column: i32, descending: bool);
        /// Rows with `text` in any cell (case-insensitive).
        #[qinvokable]
        fn set_search(self: Pin<&mut TableData>, text: &QString);
        /// JSON: [{"col": i, "op": "contains"|"="|"!="|">"|"<"|">="|"<="|"empty"|"notempty", "value": "..."}].
        #[qinvokable]
        fn set_filters(self: Pin<&mut TableData>, json: &QString);
        /// Run SQL over the sheets (table `t`: the current one); the result
        /// replaces the view. "" goes back to the sheet.
        #[qinvokable]
        fn run_query(self: Pin<&mut TableData>, sql: &QString);
        /// A cell's whole text (row: shown row).
        #[qinvokable]
        fn cell(self: &TableData, row: i32, column: i32) -> QString;
        /// The shown rows (columns in `order`, JSON array of column
        /// indexes) as TSV, for copying.
        #[qinvokable]
        fn rows_tsv(self: &TableData, first: i32, last: i32, order: &QString) -> QString;
        /// Show rows from `base` on (at most WINDOW of them).
        #[qinvokable]
        fn move_window(self: Pin<&mut TableData>, base: i32);
        /// Paged tables: the shown rows (up to 100 000) as TSV, answered
        /// by tsvReady.
        #[qinvokable]
        fn request_tsv(self: &TableData, order: &QString);
        /// A SQLite database: show this table or view.
        #[qinvokable]
        fn show_table(self: Pin<&mut TableData>, name: &QString);
    }
}

/// A sheet: header and rows of display text.
#[derive(Default, Clone)]
pub struct Sheet {
    pub name: String,
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Default, Clone)]
struct View {
    search: String,
    filters: Vec<Filter>,
    sort: i32,
    desc: bool,
}

#[derive(Clone)]
struct Filter {
    col: usize,
    op: String,
    value: String,
    num: Option<f64>,
}

struct Shown(Vec<u32>);

#[derive(Default)]
pub struct TableDataRust {
    row_base: i32,
    path: QString,
    loading: bool,
    busy: bool,
    error: QString,
    sheets: QString,
    sheet: i32,
    columns: QString,
    count: i32,
    total: i32,
    sort_column: i32,
    descending: bool,
    querying: bool,
    query_error: QString,

    all: Arc<Vec<Arc<Sheet>>>,
    /// What's shown: a sheet or a query's result.
    cur: Arc<Sheet>,
    vis: Vec<u32>,
    view: View,
    gen: u64,
    view_gen: u64,
    cancel: Option<Arc<AtomicBool>>,
    db: Arc<Mutex<Option<rusqlite::Connection>>>,
    paged: bool,
    indexing: bool,
    import_progress: f64,
    is_db: bool,
    tables: QString,
    table: QString,
    schema: QString,
    /// A paged table's worker (set once the worker says it's paged).
    paged_state: Option<paged::Paged>,
    link: Option<Link>,
}

/// The way to a paged worker, until it says the file is paged.
struct Link(std::sync::mpsc::Sender<paged::Cmd>, Arc<Mutex<Option<rusqlite::InterruptHandle>>>, Arc<AtomicBool>);

/// A SQLite database (by its first 16 bytes; reads the disk).
pub fn is_sqlite(path: &str) -> bool {
    use std::io::Read;
    let mut b = [0u8; 16];
    std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut b)).is_ok() && &b == b"SQLite format 3\0"
}

const ROLES: [&str; 2] = ["cells", "row"];
pub const SEP: char = '\u{1f}';

fn qs(s: &str) -> QString {
    QString::from(s)
}

/// CSV/TSV/spreadsheet by name (lower case).
pub fn is_table_name(lower: &str) -> bool {
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    matches!(ext, "csv" | "tsv" | "tab" | "xlsx" | "xlsm" | "xlsb" | "xls" | "ods")
}

/// The separator of a delimited text file by name (lower case).
pub fn delimiter_for(lower: &str) -> Option<char> {
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match ext {
        "csv" => Some(','),
        "tsv" | "tab" => Some('\t'),
        _ => None,
    }
}

/// Delimited text into rows (RFC 4180 quoting: "" for a quote, separators
/// and line breaks inside quotes). A ',' file whose first line has more
/// ';' than ',' is read with ';' (European CSV). At most `max_rows`.
pub fn parse_delimited(text: &str, delim: char, max_rows: usize) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let delim = if delim == ',' {
        let first = text.lines().next().unwrap_or("");
        if first.matches(';').count() > first.matches(',').count() { ';' } else { ',' }
    } else {
        delim
    };
    parse_exact(text, delim, max_rows)
}

/// Delimited text into rows with exactly this separator.
pub fn parse_exact(text: &str, delim: char, max_rows: usize) -> Vec<Vec<String>> {
    let mut rows = vec![];
    let mut row: Vec<String> = vec![];
    let mut field = String::new();
    let mut quoted = false;
    let mut at_start = true;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        if c == '"' && at_start {
            quoted = true;
            at_start = false;
        } else if c == delim {
            row.push(std::mem::take(&mut field));
            at_start = true;
        } else if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut field));
            rows.push(std::mem::take(&mut row));
            at_start = true;
            if rows.len() >= max_rows {
                return rows;
            }
        } else {
            field.push(c);
            at_start = false;
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// A header (first row) and the rest; header names made unique and
/// never empty ("Column 3").
fn sheet_from(name: &str, mut rows: Vec<Vec<String>>) -> Sheet {
    let width = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    // Trailing empty rows (a final newline, spreadsheet padding).
    while rows.last().map(|r| r.iter().all(|c| c.is_empty())).unwrap_or(false) {
        rows.pop();
    }
    let mut header = if rows.is_empty() { vec![] } else { rows.remove(0) };
    header.resize(width, String::new());
    let mut seen = std::collections::HashSet::new();
    for (i, h) in header.iter_mut().enumerate() {
        let mut n = h.trim().replace('\n', " ");
        if n.is_empty() {
            n = format!("Column {}", i + 1);
        }
        let base = n.clone();
        let mut k = 2;
        while !seen.insert(n.to_lowercase()) {
            n = format!("{base} {k}");
            k += 1;
        }
        *h = n;
    }
    for r in &mut rows {
        r.resize(width, String::new());
    }
    Sheet { name: name.to_string(), header, rows }
}

/// Days since 1899-12-30 (Excel's serial dates) as y-m-d.
fn excel_date(serial: f64) -> String {
    let days = serial.floor() as i64;
    let secs = ((serial - serial.floor()) * 86400.0).round() as i64;
    // Civil from days (Howard Hinnant), epoch 1970-01-01 = serial 25569.
    let z = days - 25569 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    if secs == 0 {
        format!("{y:04}-{m:02}-{d:02}")
    } else {
        format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
    }
}

fn cell_text(c: &calamine::Data) -> String {
    use calamine::Data;
    match c {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                format!("{f}")
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() },
        Data::DateTime(d) => {
            let v = d.as_f64();
            if d.is_duration() {
                format!("{v}")
            } else if v < 1.0 {
                let s = (v * 86400.0).round() as i64;
                format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
            } else {
                excel_date(v)
            }
        }
        other => other.to_string(),
    }
}

/// Read every sheet of a table file.
pub fn read_sheets(path: &str) -> Result<Vec<Sheet>, String> {
    let lower = path.to_lowercase();
    let p = std::path::Path::new(path);
    if let Some(delim) = delimiter_for(&lower) {
        let bytes = std::fs::read(p).map_err(|e| grove_core::util::plain_error(&e, p))?;
        let text = String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
        return Ok(vec![sheet_from("", parse_delimited(&text, delim, usize::MAX))]);
    }
    use calamine::Reader;
    let mut wb = calamine::open_workbook_auto(p).map_err(|e| format!("Can't read this spreadsheet: {e}"))?;
    let names = wb.sheet_names().to_vec();
    let mut out = vec![];
    for n in names {
        let Ok(range) = wb.worksheet_range(&n) else { continue };
        let rows: Vec<Vec<String>> = range.rows().map(|r| r.iter().map(cell_text).collect()).collect();
        out.push(sheet_from(&n, rows));
    }
    if out.is_empty() {
        return Err("No sheets in this spreadsheet".into());
    }
    Ok(out)
}

fn num(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    t.replace([',', '_'], "").trim_end_matches('%').parse::<f64>().ok().filter(|v| v.is_finite())
}

fn columns_json(s: &Sheet) -> String {
    let cols: Vec<Value> = s
        .header
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let mut width = h.chars().count();
            let mut nums = 0;
            let mut filled = 0;
            for r in s.rows.iter().take(2000) {
                let c = &r[i];
                let first_line = c.lines().next().unwrap_or("");
                width = width.max(first_line.chars().count());
                if !c.trim().is_empty() {
                    filled += 1;
                    if num(c).is_some() {
                        nums += 1;
                    }
                }
            }
            json!({"name": h, "width": width.clamp(3, 48), "numeric": filled > 0 && nums * 10 >= filled * 8})
        })
        .collect();
    json!(cols).to_string()
}

/// The rows of `s` that pass `v`, sorted.
fn compute(s: &Sheet, v: &View, cancel: &AtomicBool) -> Option<Vec<u32>> {
    let needle = v.search.to_lowercase();
    let mut out: Vec<u32> = Vec::with_capacity(s.rows.len());
    for (i, r) in s.rows.iter().enumerate() {
        if i % 65536 == 0 && cancel.load(Ordering::Relaxed) {
            return None;
        }
        if !needle.is_empty() && !r.iter().any(|c| c.to_lowercase().contains(&needle)) {
            continue;
        }
        if !v.filters.iter().all(|f| passes(r.get(f.col).map(String::as_str).unwrap_or(""), f)) {
            continue;
        }
        out.push(i as u32);
    }
    if v.sort >= 0 {
        let c = v.sort as usize;
        let key = |i: &u32| {
            let t = s.rows[*i as usize].get(c).map(String::as_str).unwrap_or("");
            (num(t), t)
        };
        let cmp = |a: &u32, b: &u32| {
            let (na, ta) = key(a);
            let (nb, tb) = key(b);
            // Empty cells last either way; numbers before text.
            match (ta.trim().is_empty(), tb.trim().is_empty()) {
                (true, false) => return CmpOrdering::Greater,
                (false, true) => return CmpOrdering::Less,
                _ => {}
            }
            let o = match (na, nb) {
                (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(CmpOrdering::Equal),
                (Some(_), None) => CmpOrdering::Less,
                (None, Some(_)) => CmpOrdering::Greater,
                (None, None) => grove_core::util::natural_cmp(ta, tb),
            };
            if v.desc { o.reverse() } else { o }
        };
        out.sort_by(cmp);
    }
    Some(out)
}

fn passes(cell: &str, f: &Filter) -> bool {
    let c = cell.trim();
    let val = f.value.trim();
    match f.op.as_str() {
        "empty" => c.is_empty(),
        "notempty" => !c.is_empty(),
        "contains" => val.is_empty() || c.to_lowercase().contains(&val.to_lowercase()),
        "!contains" => val.is_empty() || !c.to_lowercase().contains(&val.to_lowercase()),
        op => {
            if val.is_empty() {
                return true;
            }
            let o = match (num(c), f.num) {
                (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(CmpOrdering::Equal),
                _ => grove_core::util::natural_cmp(&c.to_lowercase(), &val.to_lowercase()),
            };
            match op {
                "=" => o == CmpOrdering::Equal,
                "!=" => o != CmpOrdering::Equal,
                ">" => o == CmpOrdering::Greater,
                "<" => o == CmpOrdering::Less,
                ">=" => o != CmpOrdering::Less,
                "<=" => o != CmpOrdering::Greater,
                _ => true,
            }
        }
    }
}

fn quote_ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// A database with every sheet (and the current one as `t`).
fn make_db(all: &[Arc<Sheet>], current: usize) -> rusqlite::Result<rusqlite::Connection> {
    let db = rusqlite::Connection::open_in_memory()?;
    let mut names = std::collections::HashSet::new();
    let mut add = |db: &rusqlite::Connection, table: &str, s: &Sheet| -> rusqlite::Result<()> {
        if !names.insert(table.to_lowercase()) {
            return Ok(());
        }
        let nums: Vec<bool> = serde_json::from_str::<Vec<Value>>(&columns_json(s))
            .unwrap_or_default()
            .iter()
            .map(|c| c["numeric"].as_bool().unwrap_or(false))
            .collect();
        let cols: Vec<String> = s.header.iter().enumerate().map(|(i, h)| format!("{} {}", quote_ident(h), if nums.get(i) == Some(&true) { "NUMERIC" } else { "TEXT" })).collect();
        if cols.is_empty() {
            return Ok(());
        }
        db.execute_batch(&format!("CREATE TABLE {} ({});", quote_ident(table), cols.join(", ")))?;
        let marks = vec!["?"; s.header.len()].join(", ");
        db.execute_batch("BEGIN")?;
        {
            let mut st = db.prepare(&format!("INSERT INTO {} VALUES ({marks})", quote_ident(table)))?;
            for r in &s.rows {
                let vals: Vec<rusqlite::types::Value> = r
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        if c.is_empty() {
                            rusqlite::types::Value::Null
                        } else if nums.get(i) == Some(&true) {
                            match num(c) {
                                Some(v) if v.fract() == 0.0 && v.abs() < 9e15 => rusqlite::types::Value::Integer(v as i64),
                                Some(v) => rusqlite::types::Value::Real(v),
                                None => rusqlite::types::Value::Text(c.clone()),
                            }
                        } else {
                            rusqlite::types::Value::Text(c.clone())
                        }
                    })
                    .collect();
                st.execute(rusqlite::params_from_iter(vals))?;
            }
        }
        db.execute_batch("COMMIT")?;
        Ok(())
    };
    if let Some(s) = all.get(current) {
        add(&db, "t", s)?;
    }
    for s in all {
        if !s.name.is_empty() {
            add(&db, &s.name, s)?;
        }
    }
    Ok(db)
}

fn run_sql(db: &rusqlite::Connection, sql: &str) -> Result<Sheet, String> {
    let mut st = db.prepare(sql).map_err(|e| e.to_string())?;
    let header: Vec<String> = st.column_names().map(|s| s.to_string()).collect();
    let n = header.len();
    let mut rows = vec![];
    let mut q = st.query([]).map_err(|e| e.to_string())?;
    while let Some(r) = q.next().map_err(|e| e.to_string())? {
        let mut row = Vec::with_capacity(n);
        for i in 0..n {
            use rusqlite::types::ValueRef;
            row.push(match r.get_ref(i).map_err(|e| e.to_string())? {
                ValueRef::Null => String::new(),
                ValueRef::Integer(v) => v.to_string(),
                ValueRef::Real(v) => format!("{v}"),
                ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
                ValueRef::Blob(b) => format!("<{} bytes>", b.len()),
            });
        }
        rows.push(row);
    }
    let mut s = Sheet { name: "Query".into(), header, rows: vec![] };
    let mut seen = std::collections::HashSet::new();
    for (i, h) in s.header.iter_mut().enumerate() {
        if h.is_empty() || !seen.insert(h.to_lowercase()) {
            *h = format!("{h} {}", i + 1);
        }
    }
    s.rows = rows;
    Ok(s)
}

struct Loaded(Result<Vec<Sheet>, String>);
unsafe impl Send for Loaded {}

/// Rows the model holds at once (200k × a row ≈ 5 M px: exact in floats).
pub const WINDOW: usize = 200_000;

impl qobject::TableData {
    pub(crate) fn all_rows(&self) -> usize {
        match &self.rust().paged_state {
            Some(p) => p.count,
            None => self.rust().vis.len(),
        }
    }
    fn base(&self) -> usize {
        self.rust().row_base.max(0) as usize
    }
    fn move_window(mut self: Pin<&mut Self>, base: i32) {
        let total = self.all_rows();
        let b = (base.max(0) as usize).min(total.saturating_sub(WINDOW)) as i32;
        if b == self.rust().row_base {
            return;
        }
        unsafe {
            self.as_mut().begin_reset_model();
        }
        self.as_mut().rust_mut().row_base = b;
        unsafe {
            self.as_mut().end_reset_model();
        }
        self.as_mut().row_base_changed();
    }
    /// Back to the top window (before a reset that shows other rows).
    pub(crate) fn window_home(mut self: Pin<&mut Self>) {
        if self.rust().row_base != 0 {
            self.as_mut().rust_mut().row_base = 0;
            self.as_mut().row_base_changed();
        }
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let at = index.row().max(0) as usize + self.base();
        if self.rust().paged_state.is_some() {
            let (cells, row) = self.paged_cells(at);
            return match ROLES.get((role - 0x0100) as usize) {
                Some(&"cells") => QVariant::from(&qs(&cells)),
                Some(&"row") => QVariant::from(&row),
                _ => QVariant::default(),
            };
        }
        let r = self.rust();
        let Some(&i) = r.vis.get(at) else { return QVariant::default() };
        match ROLES.get((role - 0x0100) as usize) {
            Some(&"cells") => {
                let row = &r.cur.rows[i as usize];
                let mut s = String::new();
                for (k, c) in row.iter().enumerate() {
                    if k > 0 {
                        s.push(SEP);
                    }
                    // One line per cell, not too long (the cell's tip shows it all).
                    let first: String = c.chars().take(400).map(|ch| if ch == '\n' || ch == '\r' || ch == SEP { ' ' } else { ch }).collect();
                    s.push_str(&first);
                }
                QVariant::from(&qs(&s))
            }
            Some(&"row") => QVariant::from(&(i as i32 + 1)),
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
        self.all_rows().saturating_sub(self.base()).min(WINDOW) as i32
    }

    fn request_tsv(&self, order: &QString) {
        let order: Vec<usize> = serde_json::from_str(&order.to_string()).unwrap_or_default();
        self.p_tsv_request(order);
    }

    fn show_table(self: Pin<&mut Self>, name: &QString) {
        self.p_table(name.to_string());
    }

    fn cell(&self, row: i32, column: i32) -> QString {
        if self.rust().paged_state.is_some() {
            return self.paged_cell(row.max(0) as usize, column.max(0) as usize);
        }
        let r = self.rust();
        r.vis
            .get(row as usize)
            .and_then(|&i| r.cur.rows.get(i as usize))
            .and_then(|x| x.get(column as usize))
            .map(|c| qs(c))
            .unwrap_or_default()
    }

    fn rows_tsv(&self, first: i32, last: i32, order: &QString) -> QString {
        let r = self.rust();
        let order: Vec<usize> = serde_json::from_str(&order.to_string()).unwrap_or_else(|_| (0..r.cur.header.len()).collect());
        let clean = |c: &str| c.replace(['\t', '\n', '\r'], " ");
        let mut out = String::new();
        let line = |cells: Vec<String>| cells.join("\t") + "\n";
        out.push_str(&line(order.iter().map(|&c| clean(r.cur.header.get(c).map(String::as_str).unwrap_or(""))).collect()));
        let last = (last.max(first) as usize).min(r.vis.len().saturating_sub(1));
        for k in first.max(0) as usize..=last {
            let Some(&i) = r.vis.get(k) else { break };
            let row = &r.cur.rows[i as usize];
            out.push_str(&line(order.iter().map(|&c| clean(row.get(c).map(String::as_str).unwrap_or(""))).collect()));
        }
        qs(&out)
    }

    fn reset(mut self: Pin<&mut Self>) {
        if let Some(c) = self.as_mut().rust_mut().cancel.take() {
            c.store(true, Ordering::Relaxed);
        }
        self.as_mut().window_home();
        unsafe {
            self.as_mut().begin_reset_model();
        }
        self.as_mut().rust_mut().vis.clear();
        self.as_mut().rust_mut().cur = Arc::new(Sheet::default());
        unsafe {
            self.as_mut().end_reset_model();
        }
    }

    fn load(mut self: Pin<&mut Self>, path: &QString) {
        // The last file's worker stops (and its imported copy goes).
        if let Some(l) = self.as_mut().rust_mut().link.take() {
            l.2.store(true, Ordering::Relaxed);
        }
        if self.rust().paged_state.is_some() {
            self.as_mut().window_home();
            unsafe {
                self.as_mut().begin_reset_model();
            }
            self.as_mut().rust_mut().paged_state = None;
            unsafe {
                self.as_mut().end_reset_model();
            }
        }
        self.as_mut().set_paged(false);
        self.as_mut().set_is_db(false);
        self.as_mut().set_indexing(false);
        self.as_mut().set_import_progress(-1.0);
        self.as_mut().set_tables(qs("[]"));
        self.as_mut().set_table(QString::default());
        self.as_mut().set_schema(QString::default());
        self.as_mut().reset();
        self.as_mut().rust_mut().gen += 1;
        self.as_mut().rust_mut().all = Arc::new(vec![]);
        self.as_mut().rust_mut().db = Arc::new(Mutex::new(None));
        self.as_mut().rust_mut().view = View { sort: -1, ..View::default() };
        self.as_mut().set_path(path.clone());
        self.as_mut().set_error(QString::default());
        self.as_mut().set_query_error(QString::default());
        self.as_mut().set_querying(false);
        self.as_mut().set_sheets(qs("[]"));
        self.as_mut().set_columns(qs("[]"));
        self.as_mut().set_sheet(0);
        self.as_mut().set_sort_column(-1);
        self.as_mut().set_descending(false);
        self.as_mut().set_count(0);
        self.as_mut().set_total(0);
        let p = path.to_string();
        if p.is_empty() {
            self.as_mut().set_loading(false);
            return;
        }
        self.as_mut().set_loading(true);
        let gen = self.rust().gen;
        let qt = self.qt_thread();
        let (tx, rx) = std::sync::mpsc::channel();
        let interrupt = Arc::new(Mutex::new(None));
        let cancel = Arc::new(AtomicBool::new(false));
        self.as_mut().rust_mut().link = Some(Link(tx.clone(), interrupt.clone(), cancel.clone()));
        std::thread::Builder::new()
            .name("grove-table".into())
            .spawn(move || {
                // A SQLite database, or a CSV too big to read whole: paged.
                let lower = p.to_lowercase();
                let is_db = is_sqlite(&p);
                let delim = delimiter_for(&lower);
                let big = !is_db && delim.is_some() && std::fs::metadata(&p).map(|m| m.len() >= BIG_CSV).unwrap_or(false);
                if is_db || big {
                    let _ = qt.queue(move |m| m.p_open(gen, is_db));
                    paged::run(qt, gen, p, is_db, delim.unwrap_or(','), tx, rx, cancel, interrupt);
                    return;
                }
                drop((tx, rx));
                let r = Loaded(read_sheets(&p));
                let _ = qt.queue(move |m| m.loaded(gen, r));
            })
            .ok();
    }

    fn loaded(mut self: Pin<&mut Self>, gen: u64, r: Loaded) {
        if gen != self.rust().gen {
            return;
        }
        self.as_mut().set_loading(false);
        match r.0 {
            Err(e) => self.as_mut().set_error(qs(&e)),
            Ok(sheets) => {
                let names: Vec<&str> = sheets.iter().map(|s| s.name.as_str()).collect();
                self.as_mut().set_sheets(qs(&json!(names).to_string()));
                self.as_mut().rust_mut().all = Arc::new(sheets.into_iter().map(Arc::new).collect());
                self.show_sheet(0);
            }
        }
    }

    fn show_sheet(mut self: Pin<&mut Self>, index: i32) {
        let Some(s) = self.rust().all.get(index.max(0) as usize).cloned() else { return };
        self.as_mut().set_sheet(index.max(0));
        self.as_mut().set_querying(false);
        self.as_mut().set_query_error(QString::default());
        // A new `t` for queries.
        self.as_mut().rust_mut().db = Arc::new(Mutex::new(None));
        self.as_mut().rust_mut().view = View { sort: -1, ..View::default() };
        self.as_mut().set_sort_column(-1);
        self.as_mut().set_descending(false);
        self.as_mut().show(s);
    }

    /// Show a sheet (or a query's result) with the current view.
    fn show(mut self: Pin<&mut Self>, s: Arc<Sheet>) {
        self.as_mut().reset();
        self.as_mut().set_columns(qs(&columns_json(&s)));
        self.as_mut().set_total(s.rows.len() as i32);
        self.as_mut().rust_mut().cur = s;
        self.recompute();
    }

    fn recompute(mut self: Pin<&mut Self>) {
        if self.rust().paged_state.is_some() {
            return self.p_view();
        }
        if let Some(c) = self.as_mut().rust_mut().cancel.take() {
            c.store(true, Ordering::Relaxed);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.as_mut().rust_mut().cancel = Some(cancel.clone());
        self.as_mut().rust_mut().view_gen += 1;
        let vg = self.rust().view_gen;
        let s = self.rust().cur.clone();
        let v = self.rust().view.clone();
        // Small tables at once (no flicker); big ones on a worker.
        if s.rows.len() <= 5000 {
            if let Some(vis) = compute(&s, &v, &cancel) {
                self.apply(vg, Shown(vis));
            }
            return;
        }
        self.as_mut().set_busy(true);
        let qt = self.qt_thread();
        std::thread::Builder::new()
            .name("grove-table-view".into())
            .spawn(move || {
                if let Some(vis) = compute(&s, &v, &cancel) {
                    let _ = qt.queue(move |m| m.apply(vg, Shown(vis)));
                }
            })
            .ok();
    }

    fn apply(mut self: Pin<&mut Self>, vg: u64, vis: Shown) {
        if vg != self.rust().view_gen {
            return;
        }
        self.as_mut().window_home();
        unsafe {
            self.as_mut().begin_reset_model();
        }
        self.as_mut().rust_mut().vis = vis.0;
        unsafe {
            self.as_mut().end_reset_model();
        }
        let n = self.rust().vis.len() as i32;
        self.as_mut().set_count(n);
        self.as_mut().set_busy(false);
    }

    fn sort_by(mut self: Pin<&mut Self>, column: i32, descending: bool) {
        self.as_mut().rust_mut().view.sort = column;
        self.as_mut().rust_mut().view.desc = descending;
        self.as_mut().set_sort_column(column);
        self.as_mut().set_descending(descending);
        self.recompute();
    }

    fn set_search(mut self: Pin<&mut Self>, text: &QString) {
        let t = text.to_string();
        if t == self.rust().view.search {
            return;
        }
        self.as_mut().rust_mut().view.search = t;
        self.recompute();
    }

    fn set_filters(mut self: Pin<&mut Self>, json: &QString) {
        let v: Vec<Value> = serde_json::from_str(&json.to_string()).unwrap_or_default();
        let filters: Vec<Filter> = v
            .iter()
            .filter_map(|f| {
                let value = f["value"].as_str().unwrap_or("").to_string();
                Some(Filter { col: f["col"].as_u64()? as usize, op: f["op"].as_str().unwrap_or("contains").to_string(), num: num(&value), value })
            })
            .collect();
        self.as_mut().rust_mut().view.filters = filters;
        self.recompute();
    }

    fn run_query(mut self: Pin<&mut Self>, sql: &QString) {
        let sql = sql.to_string();
        self.as_mut().set_query_error(QString::default());
        if self.rust().paged_state.is_some() {
            self.as_mut().rust_mut().view = View { sort: -1, ..View::default() };
            self.as_mut().set_sort_column(-1);
            self.as_mut().set_descending(false);
            return self.p_query(sql);
        }
        if sql.trim().is_empty() {
            let idx = self.rust().sheet;
            let Some(s) = self.rust().all.get(idx.max(0) as usize).cloned() else { return };
            self.as_mut().set_querying(false);
            self.as_mut().rust_mut().view = View { sort: -1, ..View::default() };
            self.as_mut().set_sort_column(-1);
            self.as_mut().show(s);
            return;
        }
        self.as_mut().set_busy(true);
        let all = self.rust().all.clone();
        let current = self.rust().sheet.max(0) as usize;
        let db = self.rust().db.clone();
        let gen = self.rust().gen;
        let qt = self.qt_thread();
        std::thread::Builder::new()
            .name("grove-table-sql".into())
            .spawn(move || {
                let mut g = db.lock().unwrap();
                let r = (|| {
                    if g.is_none() {
                        *g = Some(make_db(&all, current).map_err(|e| format!("Couldn't load the table: {e}"))?);
                    }
                    run_sql(g.as_ref().unwrap(), &sql)
                })();
                let r = Loaded(r.map(|s| vec![s]));
                let _ = qt.queue(move |m| m.query_done(gen, r));
            })
            .ok();
    }

    fn query_done(mut self: Pin<&mut Self>, gen: u64, r: Loaded) {
        if gen != self.rust().gen {
            return;
        }
        self.as_mut().set_busy(false);
        match r.0 {
            Err(e) => self.as_mut().set_query_error(qs(&e)),
            Ok(mut v) => {
                let s = v.pop().unwrap_or_default();
                self.as_mut().set_querying(true);
                self.as_mut().rust_mut().view = View { sort: -1, ..View::default() };
                self.as_mut().set_sort_column(-1);
                self.as_mut().set_descending(false);
                self.show(Arc::new(s));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quotes_and_newlines() {
        let r = parse_delimited("a,b,c\n1,\"x, \"\"y\"\"\",\"two\nlines\"\r\n2,,z\n", ',', usize::MAX);
        assert_eq!(r.len(), 3);
        assert_eq!(r[1], ["1", "x, \"y\"", "two\nlines"]);
        assert_eq!(r[2], ["2", "", "z"]);
        let r = parse_delimited("a;b\n1,5;2\n", ',', usize::MAX);
        assert_eq!(r[1], ["1,5", "2"]);
    }

    #[test]
    fn sorts_filters_and_queries() {
        let s = sheet_from("", parse_delimited("name,n\nb,10\na,9\nc,\nd,100\n", ',', usize::MAX));
        let never = AtomicBool::new(false);
        let v = View { sort: 1, ..View::default() };
        assert_eq!(compute(&s, &v, &never).unwrap(), [1, 0, 3, 2]);
        let v = View { sort: 1, desc: true, ..View::default() };
        assert_eq!(compute(&s, &v, &never).unwrap(), [3, 0, 1, 2]);
        let v = View { sort: -1, filters: vec![Filter { col: 1, op: ">".into(), value: "9".into(), num: Some(9.0) }], ..View::default() };
        assert_eq!(compute(&s, &v, &never).unwrap(), [0, 3]);
        let v = View { sort: -1, search: "A".into(), ..View::default() };
        assert_eq!(compute(&s, &v, &never).unwrap(), [1]);
        let db = make_db(&[Arc::new(s)], 0).unwrap();
        let q = run_sql(&db, "select name, n * 2 as d from t where n > 9 order by n").unwrap();
        assert_eq!(q.header, ["name", "d"]);
        assert_eq!(q.rows, [["b", "20"], ["d", "200"]]);
        assert!(run_sql(&db, "selec nonsense").is_err());
    }

    #[test]
    fn excel_dates() {
        assert_eq!(excel_date(45658.0), "2025-01-01");
        assert_eq!(excel_date(45658.5), "2025-01-01 12:00:00");
    }
}
