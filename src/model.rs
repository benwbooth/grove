//! FolderModel: the list behind every view (icons, list, columns, gallery).
//!
//! Folders are read on a worker thread and handed over in batches; the
//! model keeps everything it read (`all`, sorted and flattened when list
//! view folders are expanded) and what the view sees (`vis`: hidden files
//! and the filter bar applied). Changes reach the view as row inserts and
//! removes (a diff by path), so scrolling and selection survive a folder
//! changing under it. Selection lives here too (100 000 items stay cheap).

use crate::mime::{self, MimeRec};
use cxx_qt::casting::Upcast;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use grove_core::listing::{self, Batch, Entry, Sort, SortKey};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

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
        fn grove_rows_changed(model: Pin<&mut QAbstractListModel>, first: i32, last: i32);
        fn grove_layout_changing(model: Pin<&mut QAbstractListModel>, done: bool);
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(QString, path)]
        #[qproperty(bool, loading)]
        #[qproperty(QString, error)]
        #[qproperty(QString, sort_key, cxx_name = "sortKey")]
        #[qproperty(bool, descending)]
        #[qproperty(bool, folders_first, cxx_name = "foldersFirst")]
        #[qproperty(bool, show_hidden, cxx_name = "showHidden")]
        #[qproperty(QString, filter)]
        #[qproperty(i32, count)]
        #[qproperty(i32, total)]
        #[qproperty(i32, selection_count, cxx_name = "selectionCount")]
        #[qproperty(i32, current_row, cxx_name = "currentRow")]
        #[qproperty(QString, status)]
        type FolderModel = super::FolderModelRust;
    }

    impl cxx_qt::Threading for FolderModel {}

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginInsertRows"]
        unsafe fn begin_insert_rows(self: Pin<&mut FolderModel>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut FolderModel>);
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(self: Pin<&mut FolderModel>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut FolderModel>);
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut FolderModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut FolderModel>);
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &FolderModel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &FolderModel) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &FolderModel, parent: &QModelIndex) -> i32;
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        /// Show a folder (or trash:/// recent:///); reads it in the background.
        #[qinvokable]
        fn load_path(self: Pin<&mut FolderModel>, path: &QString);
        /// Read the folder again.
        #[qinvokable]
        fn reload(self: Pin<&mut FolderModel>);
        /// Search below `root` (query JSON: grove_core::search::Query).
        #[qinvokable]
        fn start_search(self: Pin<&mut FolderModel>, root: &QString, query: &QString);
        #[qinvokable]
        fn stop(self: Pin<&mut FolderModel>);
        #[qinvokable]
        fn apply_sort(self: Pin<&mut FolderModel>, key: &QString, descending: bool);
        /// Disk usage mode's sizes ({path: bytes}; "{}" when it's off). They
        /// add to what's known; sorting by Size follows them.
        #[qinvokable]
        #[cxx_name = "setUsageSizes"]
        fn set_usage_sizes(self: Pin<&mut FolderModel>, json: &QString);
        #[qinvokable]
        fn apply_filter(self: Pin<&mut FolderModel>, text: &QString);
        #[qinvokable]
        fn apply_show_hidden(self: Pin<&mut FolderModel>, on: bool);
        #[qinvokable]
        fn apply_folders_first(self: Pin<&mut FolderModel>, on: bool);
        /// List view: show/hide a folder's contents under it.
        #[qinvokable]
        fn toggle_expanded(self: Pin<&mut FolderModel>, row: i32);
        #[qinvokable]
        fn collapse_all(self: Pin<&mut FolderModel>);

        /// mode: "set" (only this), "toggle" (Ctrl), "range" (Shift),
        /// "extend" (Ctrl+Shift), "add".
        #[qinvokable]
        fn select(self: Pin<&mut FolderModel>, row: i32, mode: &QString);
        #[qinvokable]
        fn select_all(self: Pin<&mut FolderModel>);
        #[qinvokable]
        fn clear_selection(self: Pin<&mut FolderModel>);
        #[qinvokable]
        fn invert_selection(self: Pin<&mut FolderModel>);
        /// Select the items with these paths (JSON array), e.g. after a paste.
        #[qinvokable]
        fn select_paths(self: Pin<&mut FolderModel>, paths: &QString);
        /// Rubber band: select rows first..=last (adding when `add`).
        #[qinvokable]
        fn select_rows(self: Pin<&mut FolderModel>, rows: &QString, add: bool);
        /// The selected paths, JSON array (in view order).
        #[qinvokable]
        fn selected_paths(self: &FolderModel) -> QString;
        #[qinvokable]
        fn is_selected(self: &FolderModel, row: i32) -> bool;
        /// First row whose name starts with `prefix` (type-to-select), after `from`.
        #[qinvokable]
        fn find_prefix(self: &FolderModel, prefix: &QString, from: i32) -> i32;
        #[qinvokable]
        fn row_of(self: &FolderModel, path: &QString) -> i32;
        /// One row's facts as JSON (path, name, isDir, mime...).
        #[qinvokable]
        fn get(self: &FolderModel, row: i32) -> QString;
        #[qinvokable]
        fn path_at(self: &FolderModel, row: i32) -> QString;
        /// Items cut to the clipboard (dimmed until pasted).
        #[qinvokable]
        fn set_cut_paths(self: Pin<&mut FolderModel>, paths: &QString);
        /// Fill in git status for this folder (JSON {path: "M"|"A"|"?"...}).
        #[qinvokable]
        fn set_git_status(self: Pin<&mut FolderModel>, json: &QString);
        /// Tags changed on these paths (read again).
        #[qinvokable]
        fn refresh_tags(self: Pin<&mut FolderModel>, paths: &QString);
        /// File chooser filters: JSON [{"mime": bool, "pattern": "*.png"}]
        /// (folders always show; [] shows everything).
        #[qinvokable]
        fn apply_patterns(self: Pin<&mut FolderModel>, json: &QString);
        /// Group items (Finder's "Use Groups", Dolphin's "Show in Groups"):
        /// "", "kind", "date", "size", "name", "tags".
        #[qinvokable]
        fn apply_group_by(self: Pin<&mut FolderModel>, key: &QString);
        #[qinvokable]
        fn grouping(self: &FolderModel) -> QString;
        /// Runs of rows under one group heading: [{label, start, count}].
        #[qinvokable]
        fn group_ranges(self: &FolderModel) -> QString;
        /// A video's or song's length ("3:25"), asking ffprobe the first
        /// time (the row changes when it's known).
        #[qinvokable]
        fn duration_at(self: &FolderModel, row: i32) -> QString;
        /// Folders panel: show folders only.
        #[qinvokable]
        fn apply_dirs_only(self: Pin<&mut FolderModel>, on: bool);
        /// Ratings changed on these paths (read again).
        #[qinvokable]
        fn refresh_ratings(self: Pin<&mut FolderModel>, paths: &QString);
        /// For performance checks: milliseconds the last listing took.
        #[qinvokable]
        fn last_load_ms(self: &FolderModel) -> i32;
    }

    unsafe extern "RustQt" {
        /// A listing finished (to restore scroll/selection).
        #[qsignal]
        fn loaded(self: Pin<&mut FolderModel>);
    }
}

pub struct Item {
    pub e: Entry,
    pub mime: Arc<MimeRec>,
    pub tags: Vec<String>,
    pub rating: u8,
    pub icon: String,
    pub selected: bool,
    pub expanded: bool,
    pub git: String,
    /// Folder it belongs to (for expanded list views).
    pub parent: String,
}

type ItemRef = Rc<RefCell<Item>>;

/// What the listing thread sends (Send: no Rc).
pub struct RawItem {
    pub e: Entry,
    pub mime: Arc<MimeRec>,
    pub tags: Vec<String>,
    pub rating: u8,
    /// Custom icon (a folder's .directory Icon=, as Dolphin sets it).
    pub icon: String,
}

pub fn raw_from_entry(e: Entry) -> RawItem {
    let mut mime = mime::for_entry(&e);
    // Unknown by name (LICENSE, flake.lock...): look inside small files.
    if mime.name == "application/octet-stream" && !e.is_dir && e.size > 0 && e.size < 8 << 20 && e.path.starts_with('/') {
        mime = std::sync::Arc::new(mime::deep(&e.path));
    }
    let local = e.path.starts_with('/');
    let tags = if local { grove_core::tags::get(Path::new(&e.path)) } else { vec![] };
    let rating = if local { grove_core::tags::rating(Path::new(&e.path)) } else { 0 };
    let icon = if local && e.is_dir { crate::mime::custom_folder_icon(Path::new(&e.path)) } else { String::new() };
    let icon = if icon.is_empty() { mime::icon_for(&e, &mime) } else { icon };
    RawItem { e, mime, tags, rating, icon }
}

#[derive(Default)]
pub struct FolderModelRust {
    path: QString,
    loading: bool,
    error: QString,
    sort_key: QString,
    descending: bool,
    folders_first: bool,
    show_hidden: bool,
    /// Disk usage mode's sizes, for sorting by Size (empty: the mode is off).
    usage: HashMap<String, u64>,
    filter: QString,
    count: i32,
    total: i32,
    selection_count: i32,
    current_row: i32,
    status: QString,

    gen: u64,
    all: Vec<ItemRef>,
    vis: Vec<ItemRef>,
    cancel: Option<Arc<AtomicBool>>,
    anchor: i32,
    expanded: HashSet<String>,
    cut: HashSet<String>,
    watch_stop: Option<Arc<AtomicBool>>,
    load_started: Option<std::time::Instant>,
    load_ms: i32,
    searching: bool,
    patterns: Vec<(bool, String)>,
    dirs_only: bool,
    group_by: String,
    /// Identifies this model to the media worker (dimensions, durations).
    media_id: std::cell::Cell<u64>,
}

const ROLES: &[&str] = &[
    "name", "path", "isDir", "isLink", "size", "sizeText", "mtime", "dateText", "kind", "mime", "iconName", "hidden", "selected", "depth",
    "expanded", "tags", "tagColors", "note", "git", "cut", "thumb", "row", "broken", "ext", "created", "createdText",
    "rating", "addedText", "openedText", "permissions", "owner", "group", "fullPath", "section", "dims", "duration",
    "badge", "isArchive", "subvol",
];

fn qs(s: &str) -> QString {
    QString::from(s)
}

impl FolderModelRust {
    fn sort(&self) -> Sort {
        Sort { key: SortKey::from_str(&self.sort_key.to_string()), descending: self.descending, folders_first: self.folders_first }
    }

    fn visible(&self, it: &Item) -> bool {
        if !self.show_hidden && it.e.hidden && !self.searching {
            return false;
        }
        if self.dirs_only && !it.e.is_dir {
            return false;
        }
        if !self.patterns.is_empty() && !it.e.is_dir {
            let name = it.e.name.to_lowercase();
            let ok = self.patterns.iter().any(|(is_mime, p)| if *is_mime { wild_match(p, &it.mime.name) } else { wild_match(p, &name) });
            if !ok {
                return false;
            }
        }
        let f = self.filter.to_string();
        if !f.is_empty() {
            // Filter bar: words in any order, case-insensitive; items under
            // an expanded folder stay when the folder matches.
            let lower = it.e.name.to_lowercase();
            if !f.to_lowercase().split_whitespace().all(|w| lower.contains(w)) {
                return false;
            }
        }
        true
    }
}

/// Sort `all` and keep expanded folders' contents right under them.
/// `usage`: disk usage mode's sizes (path → bytes on disk). Sorting by Size
/// then goes by them, folders and files mixed (a folder's whole content).
fn flatten(all: &[ItemRef], s: Sort, group: &str, usage: &HashMap<String, u64>) -> Vec<ItemRef> {
    let by_usage = s.key == listing::SortKey::Size && !usage.is_empty();
    let mut groups: HashMap<String, Vec<ItemRef>> = HashMap::new();
    let mut roots: Vec<ItemRef> = Vec::new();
    for it in all {
        if it.borrow().e.depth == 0 {
            roots.push(it.clone());
        } else {
            groups.entry(it.borrow().parent.clone()).or_default().push(it.clone());
        }
    }
    let cmp = |a: &ItemRef, b: &ItemRef| {
        let (x, y) = (a.borrow(), b.borrow());
        if !group.is_empty() && x.e.depth == 0 {
            let o = group_rank(group, &x).cmp(&group_rank(group, &y));
            if o != std::cmp::Ordering::Equal {
                return o;
            }
        }
        if by_usage {
            let size = |e: &Entry| usage.get(&e.path).copied().unwrap_or(if e.is_dir { 0 } else { e.size });
            let o = size(&x.e).cmp(&size(&y.e)).then_with(|| grove_core::util::natural_cmp(&x.e.name, &y.e.name));
            return if s.descending { o.reverse() } else { o };
        }
        listing::compare(&x.e, &y.e, s)
    };
    roots.sort_by(cmp);
    for g in groups.values_mut() {
        g.sort_by(cmp);
    }
    let mut out = Vec::with_capacity(all.len());
    fn walk(it: &ItemRef, groups: &HashMap<String, Vec<ItemRef>>, out: &mut Vec<ItemRef>) {
        out.push(it.clone());
        let (exp, p) = {
            let b = it.borrow();
            (b.expanded, b.e.path.clone())
        };
        if exp {
            if let Some(kids) = groups.get(&p) {
                for k in kids {
                    walk(k, groups, out);
                }
            }
        }
    }
    for r in &roots {
        walk(r, &groups, &mut out);
    }
    out
}

fn date_text(secs: i64) -> String {
    if secs <= 0 {
        return "--".into();
    }
    unsafe {
        let now = grove_core::util::now_secs();
        let mut tm: libc::tm = std::mem::zeroed();
        let mut tn: libc::tm = std::mem::zeroed();
        let t = secs as libc::time_t;
        let n = now as libc::time_t;
        libc::localtime_r(&t, &mut tm);
        libc::localtime_r(&n, &mut tn);
        let h12 = if tm.tm_hour % 12 == 0 { 12 } else { tm.tm_hour % 12 };
        let ampm = if tm.tm_hour < 12 { "AM" } else { "PM" };
        let time = format!("{h12}:{:02} {ampm}", tm.tm_min);
        let day_n = |tm: &libc::tm| (tm.tm_year as i64) * 400 + tm.tm_yday as i64;
        let d = day_n(&tn) - day_n(&tm);
        if tm.tm_year == tn.tm_year && d == 0 {
            return format!("Today at {time}");
        }
        if tm.tm_year == tn.tm_year && d == 1 {
            return format!("Yesterday at {time}");
        }
        const M: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        format!("{} {}, {} at {time}", M[tm.tm_mon.clamp(0, 11) as usize], tm.tm_mday, tm.tm_year + 1900)
    }
}

impl qobject::FolderModel {
    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let r = self.rust();
        let row = index.row();
        let Some(it) = r.vis.get(row as usize) else { return QVariant::default() };
        let it = it.borrow();
        let base = 0x0100; // Qt::UserRole
        let Some(name) = ROLES.get((role - base) as usize) else { return QVariant::default() };
        match *name {
            "name" => QVariant::from(&qs(&it.e.name)),
            "path" => QVariant::from(&qs(&it.e.path)),
            "isDir" => QVariant::from(&it.e.is_dir),
            "isArchive" => QVariant::from(&(!it.e.is_dir && crate::archive::is_archive(&it.mime.name, &it.e.name))),
            "isLink" => QVariant::from(&it.e.is_link),
            "broken" => QVariant::from(&it.e.broken),
            "size" => QVariant::from(&(it.e.size as f64)),
            "sizeText" => QVariant::from(&qs(&if it.e.is_dir { "--".to_string() } else { grove_core::util::human_size(it.e.size) })),
            "mtime" => QVariant::from(&(it.e.mtime as f64)),
            "dateText" => QVariant::from(&qs(&date_text(it.e.mtime))),
            "created" => QVariant::from(&(it.e.btime as f64)),
            "createdText" => QVariant::from(&qs(&date_text(it.e.btime))),
            "kind" => QVariant::from(&qs(if it.e.subvol.is_empty() { &it.mime.comment } else { it.e.subvol })),
            "subvol" => QVariant::from(&qs(it.e.subvol)),
            "mime" => QVariant::from(&qs(&it.mime.name)),
            "iconName" => QVariant::from(&qs(&it.icon)),
            "rating" => QVariant::from(&(it.rating as i32)),
            "addedText" => QVariant::from(&qs(&date_text(it.e.ctime))),
            "openedText" => QVariant::from(&qs(&date_text(it.e.atime))),
            "permissions" => QVariant::from(&qs(&format!("{}{}", if it.e.is_dir { "d" } else if it.e.is_link { "l" } else { "-" }, crate::info::rwx(it.e.mode)))),
            "owner" => QVariant::from(&qs(&crate::info::user_name_cached(it.e.uid))),
            "group" => QVariant::from(&qs(&crate::info::group_name_cached(it.e.gid))),
            "section" => QVariant::from(&qs(&group_label(&r.group_by, &it))),
            "fullPath" => QVariant::from(&qs(it.e.path.rsplit_once('/').map(|x| x.0).unwrap_or(""))),
            "hidden" => QVariant::from(&it.e.hidden),
            "selected" => QVariant::from(&it.selected),
            "depth" => QVariant::from(&(it.e.depth as i32)),
            "expanded" => QVariant::from(&it.expanded),
            "tags" => QVariant::from(&qs(&it.tags.join(", "))),
            "tagColors" => {
                let c: Vec<&str> = it.tags.iter().map(|t| grove_core::tags::color_of(t).unwrap_or("")).collect();
                QVariant::from(&qs(&c.join(",")))
            }
            "note" => QVariant::from(&qs(&it.e.note)),
            "git" => QVariant::from(&qs(&it.git)),
            "cut" => QVariant::from(&r.cut.contains(&it.e.path)),
            "thumb" => QVariant::from(&mime::thumbable(&it.e, &it.mime)),
            "row" => QVariant::from(&row),
            "ext" => QVariant::from(&qs(it.e.ext())),
            "dims" => QVariant::from(&qs(&self.media_of(&it, false).dims)),
            "duration" => QVariant::from(&qs(&self.media_of(&it, true).duration)),
            // On the icon: a video's or recording's length, an archive's item count.
            // A btrfs subvolume's or snapshot's kind, in short.
            "badge" if !it.e.subvol.is_empty() => {
                QVariant::from(&qs(if it.e.subvol.contains("Snapshot") { "Snapshot" } else { "Subvolume" }))
            }
            "badge" => {
                let m = self.media_of(&it, true);
                QVariant::from(&qs(if m.items.is_empty() { &m.duration } else { &m.items }))
            }
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
        self.rust().vis.len() as i32
    }

    fn update_counts(mut self: Pin<&mut Self>) {
        let n = self.rust().vis.len() as i32;
        if *self.count() != n {
            self.as_mut().set_count(n);
        }
        let t = self.rust().all.len() as i32;
        if *self.total() != t {
            self.as_mut().set_total(t);
        }
        let sel = self.rust().vis.iter().filter(|i| i.borrow().selected).count() as i32;
        if *self.selection_count() != sel {
            self.as_mut().set_selection_count(sel);
        }
        let status = {
            let r = self.rust();
            let folders = r.vis.iter().filter(|i| i.borrow().e.is_dir).count();
            let files = r.vis.len() - folders;
            let mut s = String::new();
            if sel > 0 {
                s = format!("{sel} of {} selected", r.vis.len());
                let bytes: u64 = r.vis.iter().filter(|i| i.borrow().selected && !i.borrow().e.is_dir).map(|i| i.borrow().e.size).sum();
                if bytes > 0 {
                    s.push_str(&format!(", {}", grove_core::util::human_size(bytes)));
                }
            } else {
                let item = |n: usize, one: &str, many: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {many}") };
                s = match (folders, files) {
                    (0, 0) => if r.loading { "Loading…".into() } else { "Empty".into() },
                    (0, f) => item(f, "item", "items"),
                    (d, 0) => item(d, "folder", "folders"),
                    (d, f) => format!("{}, {}", item(d, "folder", "folders"), item(f, "file", "files")),
                };
                let hidden = r.all.len() - r.vis.len();
                if hidden > 0 && !r.filter.to_string().is_empty() {
                    s.push_str(&format!(" (filtered from {})", r.all.len()));
                }
            }
            s
        };
        if self.status().to_string() != status {
            self.as_mut().set_status(qs(&status));
        }
    }

    /// Make the view show `target` (by path identity): removes, inserts and
    /// in-place updates, or a layout change when only the order differs.
    fn apply_target(mut self: Pin<&mut Self>, target: Vec<ItemRef>) {
        let same_set = {
            let r = self.rust();
            r.vis.len() == target.len() && {
                let a: HashSet<String> = r.vis.iter().map(|i| i.borrow().e.path.clone()).collect();
                target.iter().all(|i| a.contains(&i.borrow().e.path))
            }
        };
        if same_set {
            let order_same = self.rust().vis.iter().zip(target.iter()).all(|(a, b)| a.borrow().e.path == b.borrow().e.path);
            if order_same {
                // Maybe new objects for the same paths (metadata changed).
                let changed: Vec<usize> = self.rust().vis.iter().zip(target.iter()).enumerate().filter(|(_, (a, b))| !Rc::ptr_eq(a, b)).map(|(i, _)| i).collect();
                self.as_mut().rust_mut().vis = target;
                if !changed.is_empty() {
                    let (lo, hi) = (*changed.first().unwrap() as i32, *changed.last().unwrap() as i32);
                    qobject::grove_rows_changed(self.as_mut().upcast_pin(), lo, hi);
                }
            } else {
                qobject::grove_layout_changing(self.as_mut().upcast_pin(), false);
                self.as_mut().rust_mut().vis = target;
                qobject::grove_layout_changing(self.as_mut().upcast_pin(), true);
            }
            self.update_counts();
            return;
        }
        // Big changes: a reset is cheaper than thousands of signals.
        let old_len = self.rust().vis.len();
        if old_len == 0 || target.is_empty() || (old_len > 5000 && target.len() > 5000 && old_len.abs_diff(target.len()) > 2000) {
            unsafe { self.as_mut().begin_reset_model() };
            self.as_mut().rust_mut().vis = target;
            unsafe { self.as_mut().end_reset_model() };
            self.update_counts();
            return;
        }
        let keep: HashSet<String> = target.iter().map(|i| i.borrow().e.path.clone()).collect();
        // 1. Removals, from the end, in runs.
        let mut i = self.rust().vis.len();
        while i > 0 {
            i -= 1;
            if keep.contains(&self.rust().vis[i].borrow().e.path) {
                continue;
            }
            let end = i;
            while i > 0 && !keep.contains(&self.rust().vis[i - 1].borrow().e.path) {
                i -= 1;
            }
            unsafe { self.as_mut().begin_remove_rows(&QModelIndex::default(), i as i32, end as i32) };
            self.as_mut().rust_mut().vis.drain(i..=end);
            unsafe { self.as_mut().end_remove_rows() };
        }
        // 2. Insertions (what's left is in target's order).
        let mut j = 0;
        let mut changed = Vec::new();
        while j < target.len() {
            let matches = self.rust().vis.get(j).map(|v| v.borrow().e.path == target[j].borrow().e.path).unwrap_or(false);
            if matches {
                if !Rc::ptr_eq(&self.rust().vis[j], &target[j]) {
                    self.as_mut().rust_mut().vis[j] = target[j].clone();
                    changed.push(j);
                }
                j += 1;
                continue;
            }
            let start = j;
            let mut end = j;
            let cur: HashSet<String> = HashSet::new();
            let _ = cur;
            while end + 1 < target.len() && self.rust().vis.get(start).map(|v| v.borrow().e.path != target[end + 1].borrow().e.path).unwrap_or(true) {
                end += 1;
            }
            unsafe { self.as_mut().begin_insert_rows(&QModelIndex::default(), start as i32, end as i32) };
            {
                let items: Vec<ItemRef> = target[start..=end].to_vec();
                let mut r = self.as_mut().rust_mut();
                let tail = r.vis.split_off(start);
                r.vis.extend(items);
                r.vis.extend(tail);
            }
            unsafe { self.as_mut().end_insert_rows() };
            j = end + 1;
        }
        for c in changed {
            qobject::grove_rows_changed(self.as_mut().upcast_pin(), c as i32, c as i32);
        }
        self.update_counts();
    }

    fn refilter(mut self: Pin<&mut Self>) {
        let target: Vec<ItemRef> = {
            let r = self.rust();
            // Items inside collapsed-away or filtered-away folders follow
            // their folder for hidden files only.
            r.all.iter().filter(|i| r.visible(&i.borrow())).cloned().collect()
        };
        self.as_mut().apply_target(target);
    }

    fn resort(mut self: Pin<&mut Self>) {
        let s = self.rust().sort();
        let all = flatten(&self.rust().all, s, &self.rust().group_by.clone(), &self.rust().usage);
        self.as_mut().rust_mut().all = all;
        self.refilter();
    }

    fn start_watch(mut self: Pin<&mut Self>, dirs: Vec<PathBuf>) {
        if let Some(s) = self.as_mut().rust_mut().watch_stop.take() {
            s.store(true, Ordering::SeqCst);
        }
        if dirs.is_empty() {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        self.as_mut().rust_mut().watch_stop = Some(stop.clone());
        let gen = self.rust().gen;
        let qt = self.qt_thread();
        crate::watch::watch(dirs, stop, move |changed_dirs| {
            let _ = qt.queue(move |m| m.on_dirs_changed(gen, changed_dirs));
        });
    }

    /// Something changed in a watched folder: read it again and merge.
    fn on_dirs_changed(self: Pin<&mut Self>, gen: u64, dirs: Vec<PathBuf>) {
        if gen != self.rust().gen {
            return;
        }
        let path = self.rust().path.to_string();
        if path.starts_with("trash:") || path.starts_with("recent:") || path.starts_with("computer:") {
            let qt = self.qt_thread();
            let p = path.clone();
            std::thread::spawn(move || {
                let items = special_list(&p);
                let _ = qt.queue(move |m| m.merge_dir(gen, None, items));
            });
            return;
        }
        for d in dirs {
            let qt = self.qt_thread();
            std::thread::spawn(move || {
                let Ok(entries) = listing::list(&d) else {
                    let _ = qt.queue(move |m| m.merge_dir(gen, Some(d), vec![]));
                    return;
                };
                let items: Vec<RawItem> = entries.into_iter().map(raw_from_entry).collect();
                let _ = qt.queue(move |m| m.merge_dir(gen, Some(d), items));
            });
        }
    }

    /// Replace the entries of one folder (None: the whole special list).
    fn merge_dir(mut self: Pin<&mut Self>, gen: u64, dir: Option<PathBuf>, items: Vec<RawItem>) {
        if gen != self.rust().gen {
            return;
        }
        let dir_s = dir.as_ref().map(|d| d.to_string_lossy().into_owned());
        let root = self.rust().path.to_string();
        let is_root = dir_s.as_deref().map(|d| d == root).unwrap_or(true);
        let (depth, parent) = if is_root {
            (0u16, String::new())
        } else {
            let d = dir_s.clone().unwrap();
            let depth = self.rust().all.iter().find(|i| i.borrow().e.path == d).map(|i| i.borrow().e.depth + 1).unwrap_or(1);
            (depth, d)
        };
        // Keep the old item objects (selection, expansion) where unchanged.
        let old: HashMap<String, ItemRef> = self
            .rust()
            .all
            .iter()
            .filter(|i| {
                let b = i.borrow();
                if is_root {
                    b.e.depth == 0
                } else {
                    b.parent == parent
                }
            })
            .map(|i| (i.borrow().e.path.clone(), i.clone()))
            .collect();
        let new_paths: HashSet<String> = items.iter().map(|r| r.e.path.clone()).collect();
        let mut fresh: Vec<ItemRef> = Vec::with_capacity(items.len());
        for mut raw in items {
            raw.e.depth = depth;
            match old.get(&raw.e.path) {
                Some(o) if o.borrow().e == raw.e && o.borrow().tags == raw.tags && o.borrow().rating == raw.rating => fresh.push(o.clone()),
                Some(o) => {
                    let (sel, exp, git) = {
                        let b = o.borrow();
                        (b.selected, b.expanded, b.git.clone())
                    };
                    fresh.push(Rc::new(RefCell::new(Item { e: raw.e, mime: raw.mime, tags: raw.tags, rating: raw.rating, icon: raw.icon, selected: sel, expanded: exp, git, parent: parent.clone() })));
                }
                None => fresh.push(Rc::new(RefCell::new(Item { e: raw.e, mime: raw.mime, tags: raw.tags, rating: raw.rating, icon: raw.icon, selected: false, expanded: false, git: String::new(), parent: parent.clone() }))),
            }
        }
        // Drop the folder's old entries (and contents of removed folders).
        let removed_dirs: Vec<String> = old.keys().filter(|p| !new_paths.contains(*p)).cloned().collect();
        let mut all: Vec<ItemRef> = self
            .rust()
            .all
            .iter()
            .filter(|i| {
                let b = i.borrow();
                let mine = if is_root { b.e.depth == 0 } else { b.parent == parent };
                !mine && !removed_dirs.iter().any(|d| b.parent.starts_with(d.as_str()) && (b.parent.len() == d.len() || b.parent.as_bytes().get(d.len()) == Some(&b'/')))
            })
            .cloned()
            .collect();
        all.extend(fresh);
        self.as_mut().rust_mut().all = all;
        self.as_mut().resort();
    }

    fn load_path(mut self: Pin<&mut Self>, path: &QString) {
        let p = path.to_string();
        let p = if p.len() > 1 && p.starts_with('/') { p.trim_end_matches('/').to_string() } else { p };
        self.as_mut().begin_load(&p, false);
        let gen = self.rust().gen;
        let cancel = self.rust().cancel.clone().unwrap();
        let qt = self.qt_thread();
        if p.contains(":///") || p.starts_with("trash:") || p.starts_with("recent:") || p.starts_with("archive://") {
            std::thread::spawn(move || {
                let items = special_list(&p);
                let _ = qt.queue(move |m| {
                    m.add_batch(gen, items, true, None);
                });
            });
            let dirs: Vec<PathBuf> = if path.to_string().starts_with("trash:") { grove_core::trash::all_trash_dirs().into_iter().map(|d| d.join("files")).collect() } else { vec![] };
            self.start_watch(dirs);
            return;
        }
        let dir = PathBuf::from(&p);
        std::thread::Builder::new()
            .name("grove-list".into())
            .spawn(move || {
                listing::list_streaming(&dir, &cancel, Duration::from_millis(120), Duration::from_millis(100), |b| match b {
                    Batch::Entries { entries } => {
                        let items: Vec<RawItem> = entries.into_iter().map(raw_from_entry).collect();
                        let _ = qt.queue(move |m| m.add_batch(gen, items, false, None));
                    }
                    Batch::Done { error } => {
                        let _ = qt.queue(move |m| m.add_batch(gen, vec![], true, error));
                    }
                });
            })
            .ok();
        self.start_watch(vec![PathBuf::from(path.to_string())]);
    }

    fn begin_load(mut self: Pin<&mut Self>, p: &str, searching: bool) {
        if let Some(c) = self.as_mut().rust_mut().cancel.take() {
            c.store(true, Ordering::SeqCst);
        }
        {
            let mut r = self.as_mut().rust_mut();
            r.gen += 1;
            r.cancel = Some(Arc::new(AtomicBool::new(false)));
            r.expanded.clear();
            r.anchor = -1;
            r.searching = searching;
            r.load_started = Some(std::time::Instant::now());
        }
        if self.sort_key().is_empty() {
            self.as_mut().set_sort_key(qs("name"));
        }
        unsafe { self.as_mut().begin_reset_model() };
        {
            let mut r = self.as_mut().rust_mut();
            r.all.clear();
            r.vis.clear();
        }
        unsafe { self.as_mut().end_reset_model() };
        self.as_mut().set_path(qs(p));
        self.as_mut().set_error(QString::default());
        self.as_mut().set_loading(true);
        self.as_mut().set_current_row(-1);
        self.update_counts();
    }

    fn add_batch(mut self: Pin<&mut Self>, gen: u64, items: Vec<RawItem>, done: bool, error: Option<String>) {
        if gen != self.rust().gen {
            return;
        }
        let _perf = Perf::new(if done { "final batch (sort + show)" } else { "batch" }, items.len());
        if !items.is_empty() {
            let new: Vec<ItemRef> = items
                .into_iter()
                .map(|r| Rc::new(RefCell::new(Item { e: r.e, mime: r.mime, tags: r.tags, rating: r.rating, icon: r.icon, selected: false, expanded: false, git: String::new(), parent: String::new() })))
                .collect();
            // Streaming: append (sorted at the end); a single batch is sorted now.
            let visible: Vec<ItemRef> = new.iter().filter(|i| self.rust().visible(&i.borrow())).cloned().collect();
            self.as_mut().rust_mut().all.extend(new);
            if !visible.is_empty() && !done {
                let start = self.rust().vis.len() as i32;
                unsafe { self.as_mut().begin_insert_rows(&QModelIndex::default(), start, start + visible.len() as i32 - 1) };
                self.as_mut().rust_mut().vis.extend(visible);
                unsafe { self.as_mut().end_insert_rows() };
                self.as_mut().update_counts();
            }
        }
        if done {
            if !self.rust().searching {
                self.as_mut().resort();
            } else {
                self.as_mut().refilter();
            }
            if let Some(e) = error {
                self.as_mut().set_error(qs(&e));
            }
            let ms = self.rust().load_started.map(|t| t.elapsed().as_millis() as i32).unwrap_or(0);
            self.as_mut().rust_mut().load_ms = ms;
            self.as_mut().set_loading(false);
            self.as_mut().update_counts();
            self.as_mut().loaded();
        }
    }

    fn reload(mut self: Pin<&mut Self>) {
        let p = self.path().clone();
        let selected = self.selected_paths();
        self.as_mut().load_path(&p);
        // Selection comes back once loaded (select_paths after `loaded`).
        let _ = selected;
    }

    fn start_search(mut self: Pin<&mut Self>, root: &QString, query: &QString) {
        let q: grove_core::search::Query = serde_json::from_str(&query.to_string()).unwrap_or_default();
        let label = format!("search:///{}?{}", root, query);
        self.as_mut().begin_load(&label, true);
        let gen = self.rust().gen;
        let cancel = self.rust().cancel.clone().unwrap();
        let qt = self.qt_thread();
        let root = PathBuf::from(root.to_string());
        std::thread::spawn(move || {
            // Two searches at once, results merged: the desktop search's
            // index (contents and names, instant in indexed folders) and
            // grove's own walk (every folder, names and plain text).
            let seen = std::sync::Mutex::new(std::collections::HashSet::<String>::new());
            let send = |batch: Vec<grove_core::listing::Entry>| {
                let fresh: Vec<RawItem> = {
                    let mut s = seen.lock().unwrap();
                    batch.into_iter().filter(|e| s.insert(e.path.clone())).map(raw_from_entry).collect()
                };
                if !fresh.is_empty() {
                    let _ = qt.queue(move |m| m.add_batch(gen, fresh, false, None));
                }
            };
            std::thread::scope(|sc| {
                sc.spawn(|| {
                    let found = grove_core::search::indexed(&root, &q, &cancel);
                    if !cancel.load(Ordering::Relaxed) {
                        send(found);
                    }
                });
                grove_core::search::run(&root, &q, cancel.clone(), &send);
            });
            let _ = qt.queue(move |m| m.add_batch(gen, vec![], true, None));
        });
    }

    fn stop(mut self: Pin<&mut Self>) {
        if let Some(c) = self.as_mut().rust_mut().cancel.take() {
            c.store(true, Ordering::SeqCst);
        }
        if let Some(s) = self.as_mut().rust_mut().watch_stop.take() {
            s.store(true, Ordering::SeqCst);
        }
        self.as_mut().set_loading(false);
    }

    fn set_usage_sizes(mut self: Pin<&mut Self>, json: &QString) {
        let m: HashMap<String, u64> = serde_json::from_str(&json.to_string()).unwrap_or_default();
        if m.is_empty() {
            if self.rust().usage.is_empty() {
                return;
            }
            self.as_mut().rust_mut().usage.clear();
        } else {
            self.as_mut().rust_mut().usage.extend(m);
        }
        if listing::SortKey::from_str(&self.rust().sort_key.to_string()) == listing::SortKey::Size {
            self.resort();
        }
    }

    fn apply_sort(mut self: Pin<&mut Self>, key: &QString, descending: bool) {
        let _perf = Perf::new("sort", self.rust().all.len());
        self.as_mut().set_sort_key(key.clone());
        self.as_mut().set_descending(descending);
        self.resort();
    }

    fn apply_filter(mut self: Pin<&mut Self>, text: &QString) {
        let _perf = Perf::new("filter", self.rust().all.len());
        self.as_mut().set_filter(text.clone());
        self.refilter();
    }

    fn apply_show_hidden(mut self: Pin<&mut Self>, on: bool) {
        self.as_mut().set_show_hidden(on);
        self.refilter();
    }

    fn apply_folders_first(mut self: Pin<&mut Self>, on: bool) {
        self.as_mut().set_folders_first(on);
        self.resort();
    }

    fn toggle_expanded(mut self: Pin<&mut Self>, row: i32) {
        let Some(it) = self.rust().vis.get(row as usize).cloned() else { return };
        // Folders, folders inside archives, and archives themselves (their
        // members listed under them, like a folder's).
        let (is_dir, is_archive) = {
            let b = it.borrow();
            (b.e.is_dir, !b.e.is_dir && crate::archive::is_archive(&b.mime.name, &b.e.name))
        };
        if !is_dir && !is_archive {
            return;
        }
        let path = it.borrow().e.path.clone();
        if it.borrow().expanded {
            it.borrow_mut().expanded = false;
            self.as_mut().rust_mut().expanded.remove(&path);
            // Drop everything below it.
            let all: Vec<ItemRef> = self
                .rust()
                .all
                .iter()
                .filter(|i| {
                    let b = i.borrow();
                    !(b.parent == path || b.parent.starts_with(&format!("{path}/")) || b.parent.starts_with(&format!("archive://{path}#")))
                })
                .cloned()
                .collect();
            self.as_mut().rust_mut().all = all;
            self.as_mut().resort();
            qobject::grove_rows_changed(self.as_mut().upcast_pin(), row, row);
            return;
        }
        it.borrow_mut().expanded = true;
        self.as_mut().rust_mut().expanded.insert(path.clone());
        qobject::grove_rows_changed(self.as_mut().upcast_pin(), row, row);
        let gen = self.rust().gen;
        let qt = self.qt_thread();
        let dir = PathBuf::from(&path);
        std::thread::spawn(move || {
            let items: Vec<RawItem> = if is_archive {
                special_list(&format!("archive://{path}"))
            } else if path.starts_with("archive://") {
                special_list(&path)
            } else {
                listing::list(&dir).unwrap_or_default().into_iter().map(raw_from_entry).collect()
            };
            let _ = qt.queue(move |m| m.merge_dir(gen, Some(dir), items));
        });
        // Watch real folders too (not archives or what's inside them).
        let root = self.rust().path.to_string();
        if root.starts_with('/') {
            let mut dirs: Vec<PathBuf> = vec![PathBuf::from(&root)];
            let real: Vec<PathBuf> = self
                .rust()
                .all
                .iter()
                .filter(|i| {
                    let b = i.borrow();
                    b.expanded && b.e.is_dir && b.e.path.starts_with('/')
                })
                .map(|i| PathBuf::from(&i.borrow().e.path))
                .collect();
            dirs.extend(real);
            self.start_watch(dirs);
        }
    }

    fn collapse_all(mut self: Pin<&mut Self>) {
        for i in self.rust().all.iter() {
            i.borrow_mut().expanded = false;
        }
        let all: Vec<ItemRef> = self.rust().all.iter().filter(|i| i.borrow().e.depth == 0).cloned().collect();
        self.as_mut().rust_mut().all = all;
        self.as_mut().rust_mut().expanded.clear();
        self.resort();
    }

    fn mark(mut self: Pin<&mut Self>, row: usize, on: bool) {
        let changed = match self.rust().vis.get(row) {
            Some(it) => {
                let mut b = it.borrow_mut();
                let c = b.selected != on;
                b.selected = on;
                c
            }
            None => false,
        };
        if changed {
            qobject::grove_rows_changed(self.as_mut().upcast_pin(), row as i32, row as i32);
        }
    }

    fn set_all(mut self: Pin<&mut Self>, f: impl Fn(usize, bool) -> bool) {
        let n = self.rust().vis.len();
        let mut lo = usize::MAX;
        let mut hi = 0;
        for i in 0..n {
            let it = self.rust().vis[i].clone();
            let mut b = it.borrow_mut();
            let want = f(i, b.selected);
            if b.selected != want {
                b.selected = want;
                lo = lo.min(i);
                hi = hi.max(i);
            }
        }
        if lo != usize::MAX {
            qobject::grove_rows_changed(self.as_mut().upcast_pin(), lo as i32, hi as i32);
        }
    }

    fn select(mut self: Pin<&mut Self>, row: i32, mode: &QString) {
        let n = self.rust().vis.len() as i32;
        if row < 0 || row >= n {
            if mode.to_string() == "set" {
                self.as_mut().clear_selection();
            }
            return;
        }
        let mode = mode.to_string();
        let anchor = if self.rust().anchor >= 0 && self.rust().anchor < n { self.rust().anchor } else { row };
        match mode.as_str() {
            "toggle" => {
                let cur = self.rust().vis[row as usize].borrow().selected;
                self.as_mut().mark(row as usize, !cur);
                self.as_mut().rust_mut().anchor = row;
            }
            "add" => {
                self.as_mut().mark(row as usize, true);
            }
            "range" | "extend" => {
                let (a, b) = (anchor.min(row) as usize, anchor.max(row) as usize);
                let extend = mode == "extend";
                self.as_mut().set_all(move |i, cur| if (a..=b).contains(&i) { true } else if extend { cur } else { false });
            }
            _ => {
                let r = row as usize;
                self.as_mut().set_all(move |i, _| i == r);
                self.as_mut().rust_mut().anchor = row;
            }
        }
        self.as_mut().set_current_row(row);
        self.update_counts();
    }

    fn select_all(mut self: Pin<&mut Self>) {
        self.as_mut().set_all(|_, _| true);
        self.update_counts();
    }

    fn clear_selection(mut self: Pin<&mut Self>) {
        self.as_mut().set_all(|_, _| false);
        self.update_counts();
    }

    fn invert_selection(mut self: Pin<&mut Self>) {
        self.as_mut().set_all(|_, c| !c);
        self.update_counts();
    }

    fn select_paths(mut self: Pin<&mut Self>, paths: &QString) {
        let list: Vec<String> = serde_json::from_str(&paths.to_string()).unwrap_or_default();
        let set: HashSet<String> = list.into_iter().collect();
        let rows: Vec<bool> = self.rust().vis.iter().map(|i| set.contains(&i.borrow().e.path)).collect();
        self.as_mut().set_all(|i, _| rows[i]);
        if let Some(first) = rows.iter().position(|x| *x) {
            self.as_mut().set_current_row(first as i32);
            self.as_mut().rust_mut().anchor = first as i32;
        }
        self.update_counts();
    }

    fn select_rows(mut self: Pin<&mut Self>, rows: &QString, add: bool) {
        let list: Vec<i32> = serde_json::from_str(&rows.to_string()).unwrap_or_default();
        let set: HashSet<usize> = list.into_iter().map(|x| x as usize).collect();
        self.as_mut().set_all(|i, cur| set.contains(&i) || (add && cur));
        self.update_counts();
    }

    fn selected_paths(&self) -> QString {
        let v: Vec<String> = self.rust().vis.iter().filter(|i| i.borrow().selected).map(|i| i.borrow().e.path.clone()).collect();
        qs(&serde_json::to_string(&v).unwrap())
    }

    fn is_selected(&self, row: i32) -> bool {
        self.rust().vis.get(row as usize).map(|i| i.borrow().selected).unwrap_or(false)
    }

    fn find_prefix(&self, prefix: &QString, from: i32) -> i32 {
        let p = prefix.to_string().to_lowercase();
        let v = &self.rust().vis;
        let n = v.len();
        if n == 0 || p.is_empty() {
            return -1;
        }
        let start = (from.max(0) as usize).min(n);
        for k in 0..n {
            let i = (start + k) % n;
            if v[i].borrow().e.name.to_lowercase().starts_with(&p) {
                return i as i32;
            }
        }
        -1
    }

    fn row_of(&self, path: &QString) -> i32 {
        let p = path.to_string();
        self.rust().vis.iter().position(|i| i.borrow().e.path == p).map(|x| x as i32).unwrap_or(-1)
    }

    fn get(&self, row: i32) -> QString {
        let Some(it) = self.rust().vis.get(row as usize) else { return qs("{}") };
        let b = it.borrow();
        let v = serde_json::json!({
            "path": b.e.path, "name": b.e.name, "isDir": b.e.is_dir, "isLink": b.e.is_link, "size": b.e.size,
            "mtime": b.e.mtime, "mime": b.mime.name, "kind": if b.e.subvol.is_empty() { b.mime.comment.as_str() } else { b.e.subvol }, "subvol": b.e.subvol, "icon": b.icon,
            "note": b.e.note, "key": b.e.key, "tags": b.tags, "depth": b.e.depth, "expanded": b.expanded,
            "thumb": mime::thumbable(&b.e, &b.mime), "target": b.e.target, "selected": b.selected, "rating": b.rating,
            "hidden": b.e.hidden, "git": b.git, "cut": self.rust().cut.contains(&b.e.path),
            "tagColors": b.tags.iter().map(|t| grove_core::tags::color_of(t).unwrap_or("")).collect::<Vec<_>>().join(","),
            "tagText": b.tags.join(", "), "sizeText": if b.e.is_dir { "--".to_string() } else { grove_core::util::human_size(b.e.size) },
            "row": row, "dims": self.media_of(&b, false).dims,
            "isArchive": !b.e.is_dir && crate::archive::is_archive(&b.mime.name, &b.e.name),
            "duration": crate::media::cached(&b.e.path, b.e.mtime).map(|m| m.duration).unwrap_or_default(),
        });
        qs(&v.to_string())
    }

    fn path_at(&self, row: i32) -> QString {
        self.rust().vis.get(row as usize).map(|i| qs(&i.borrow().e.path)).unwrap_or_default()
    }

    fn set_cut_paths(mut self: Pin<&mut Self>, paths: &QString) {
        let list: Vec<String> = serde_json::from_str(&paths.to_string()).unwrap_or_default();
        self.as_mut().rust_mut().cut = list.into_iter().collect();
        let n = self.rust().vis.len() as i32;
        if n > 0 {
            qobject::grove_rows_changed(self.as_mut().upcast_pin(), 0, n - 1);
        }
    }

    fn set_git_status(mut self: Pin<&mut Self>, json: &QString) {
        let map: HashMap<String, String> = serde_json::from_str(&json.to_string()).unwrap_or_default();
        let mut any = false;
        for it in self.rust().all.iter() {
            let mut b = it.borrow_mut();
            let s = map.get(&b.e.path).cloned().unwrap_or_default();
            if b.git != s {
                b.git = s;
                any = true;
            }
        }
        let n = self.rust().vis.len() as i32;
        if any && n > 0 {
            qobject::grove_rows_changed(self.as_mut().upcast_pin(), 0, n - 1);
        }
    }

    fn refresh_tags(mut self: Pin<&mut Self>, paths: &QString) {
        let list: Vec<String> = serde_json::from_str(&paths.to_string()).unwrap_or_default();
        let set: HashSet<String> = list.into_iter().collect();
        let mut rows = Vec::new();
        for (i, it) in self.rust().vis.iter().enumerate() {
            let p = it.borrow().e.path.clone();
            if set.contains(&p) {
                it.borrow_mut().tags = grove_core::tags::get(Path::new(&p));
                rows.push(i as i32);
            }
        }
        for r in rows {
            qobject::grove_rows_changed(self.as_mut().upcast_pin(), r, r);
        }
    }

    fn apply_group_by(mut self: Pin<&mut Self>, key: &QString) {
        self.as_mut().rust_mut().group_by = key.to_string();
        self.resort();
    }

    fn grouping(&self) -> QString {
        qs(&self.rust().group_by)
    }

    fn duration_at(&self, row: i32) -> QString {
        match self.rust().vis.get(row as usize) {
            Some(it) => qs(&self.media_of(&it.borrow(), true).duration),
            None => QString::default(),
        }
    }

    fn group_ranges(&self) -> QString {
        let r = self.rust();
        let mut out: Vec<serde_json::Value> = Vec::new();
        let mut cur: Option<(String, usize)> = None;
        for (i, it) in r.vis.iter().enumerate() {
            let l = group_label(&r.group_by, &it.borrow());
            match &cur {
                Some((c, _)) if *c == l => {}
                _ => {
                    if let Some((c, st)) = cur.take() {
                        out.push(serde_json::json!({"label": c, "start": st, "count": i - st}));
                    }
                    cur = Some((l, i));
                }
            }
        }
        if let Some((c, st)) = cur {
            out.push(serde_json::json!({"label": c, "start": st, "count": r.vis.len() - st}));
        }
        qs(&serde_json::Value::Array(out).to_string())
    }

    fn apply_dirs_only(mut self: Pin<&mut Self>, on: bool) {
        self.as_mut().rust_mut().dirs_only = on;
        self.refilter();
    }

    fn refresh_ratings(mut self: Pin<&mut Self>, paths: &QString) {
        let list: Vec<String> = serde_json::from_str(&paths.to_string()).unwrap_or_default();
        let set: HashSet<String> = list.into_iter().collect();
        let mut rows = Vec::new();
        for (i, it) in self.rust().vis.iter().enumerate() {
            let p = it.borrow().e.path.clone();
            if set.contains(&p) {
                it.borrow_mut().rating = grove_core::tags::rating(Path::new(&p));
                rows.push(i as i32);
            }
        }
        for r in rows {
            qobject::grove_rows_changed(self.as_mut().upcast_pin(), r, r);
        }
    }

    fn apply_patterns(mut self: Pin<&mut Self>, json: &QString) {
        let v: Vec<serde_json::Value> = serde_json::from_str(&json.to_string()).unwrap_or_default();
        self.as_mut().rust_mut().patterns = v
            .into_iter()
            .map(|x| (x["mime"].as_bool().unwrap_or(false), x["pattern"].as_str().unwrap_or("*").to_lowercase()))
            .collect();
        self.refilter();
    }

    fn last_load_ms(&self) -> i32 {
        self.rust().load_ms
    }

    /// Cached picture size / duration, asking the worker when unknown.
    /// Durations (ffprobe) only when a view shows them.
    fn media_of(&self, it: &Item, duration: bool) -> crate::media::Media {
        let m = &it.mime.name;
        if !crate::media::wants(m, &it.e.path) || it.e.is_dir {
            return Default::default();
        }
        if let Some(x) = crate::media::cached(&it.e.path, it.e.mtime) {
            return x;
        }
        if !duration && !m.starts_with("image/") {
            return Default::default();
        }
        let r = self.rust();
        if r.media_id.get() == 0 {
            static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
            r.media_id.set(N.fetch_add(1, Ordering::Relaxed));
        }
        let qt = self.qt_thread();
        crate::media::request(&it.e.path, it.e.mtime, m, r.media_id.get(), move |paths| {
            let _ = qt.queue(move |model| model.media_ready(paths));
        });
        Default::default()
    }

    fn media_ready(mut self: Pin<&mut Self>, paths: Vec<String>) {
        let set: HashSet<String> = paths.into_iter().collect();
        let rows: Vec<i32> = self.rust().vis.iter().enumerate().filter(|(_, i)| set.contains(&i.borrow().e.path)).map(|(i, _)| i as i32).collect();
        for r in rows {
            qobject::grove_rows_changed(self.as_mut().upcast_pin(), r, r);
        }
    }
}

fn date_bucket(t: i64) -> (u8, &'static str) {
    let age = grove_core::util::now_secs() - t;
    match age {
        a if a < 86400 => (0, "Today"),
        a if a < 2 * 86400 => (1, "Yesterday"),
        a if a < 7 * 86400 => (2, "Previous 7 Days"),
        a if a < 30 * 86400 => (3, "Previous 30 Days"),
        a if a < 365 * 86400 => (4, "This Year"),
        _ => (5, "Earlier"),
    }
}

fn size_bucket(it: &Item) -> (u8, &'static str) {
    if it.e.is_dir {
        return (0, "Folders");
    }
    match it.e.size {
        n if n < 100_000 => (1, "Tiny (under 100 KB)"),
        n if n < 1_000_000 => (2, "Small (under 1 MB)"),
        n if n < 100_000_000 => (3, "Medium (under 100 MB)"),
        n if n < 1_000_000_000 => (4, "Large (under 1 GB)"),
        _ => (5, "Huge (1 GB or more)"),
    }
}

/// Sort key of an item's group.
fn group_rank(g: &str, it: &Item) -> String {
    match g {
        // Stacks (icon view): folders stay loose, first; files stack.
        "stackkind" | "stackdate" if it.e.is_dir => "0".into(),
        "stackkind" => match stack_kind(&it.mime.name) {
            "Other" => "2".into(),
            k => format!("1{k}"),
        },
        "stackdate" => format!("1{}", date_bucket(it.e.mtime).0),
        "date" => date_bucket(it.e.mtime).0.to_string(),
        "size" => size_bucket(it).0.to_string(),
        _ => group_label(g, it).to_lowercase(),
    }
}

/// Finder's stack kinds: broad families rather than exact types.
pub fn stack_kind(mime: &str) -> &'static str {
    let (top, sub) = mime.split_once('/').unwrap_or((mime, ""));
    match top {
        "image" => "Images",
        "video" => "Movies",
        "audio" => "Music",
        _ if sub == "pdf" => "PDF Documents",
        _ if sub.contains("spreadsheet") || sub.contains("excel") || sub == "csv" => "Spreadsheets",
        _ if sub.contains("presentation") || sub.contains("powerpoint") => "Presentations",
        _ if ["zip", "x-tar", "gzip", "x-7z-compressed", "vnd.rar", "x-xz", "zstd", "x-bzip2", "x-compressed-tar", "x-xz-compressed-tar", "x-zstd-compressed-tar", "x-bzip2-compressed-tar", "x-iso9660-image", "x-cd-image"].contains(&sub) => "Archives",
        _ if sub.contains("x-rust") || sub.contains("x-python") || sub.contains("javascript") || sub.contains("x-csrc") || sub.contains("x-c++") || sub.contains("shellscript") || sub.contains("x-java") || sub == "json" || sub.contains("x-go") || sub == "xml" || sub.contains("toml") || sub.contains("yaml") || sub.contains("x-nix") => "Developer",
        "text" => "Documents",
        _ if sub.contains("document") || sub.contains("msword") || sub.contains("rtf") || sub.contains("epub") || sub.contains("opendocument.text") => "Documents",
        _ if sub.contains("executable") || sub.contains("x-sharedlib") || sub.contains("appimage") => "Applications",
        _ => "Other",
    }
}

/// The group heading an item goes under.
pub fn group_label(g: &str, it: &Item) -> String {
    match g {
        "stackkind" | "stackdate" if it.e.is_dir => String::new(),
        "stackkind" => stack_kind(&it.mime.name).into(),
        "stackdate" => date_bucket(it.e.mtime).1.into(),
        "kind" => if it.e.is_dir { "Folders".into() } else { it.mime.comment.clone() },
        "date" => date_bucket(it.e.mtime).1.into(),
        "size" => size_bucket(it).1.into(),
        "name" => it.e.name.chars().next().map(|c| if c.is_alphabetic() { c.to_uppercase().to_string() } else { "#".into() }).unwrap_or_default(),
        "tags" => it.tags.first().cloned().unwrap_or_else(|| "No Tags".into()),
        _ => String::new(),
    }
}

/// GROVE_PERF=1: how long the UI thread spends on each step.
struct Perf(&'static str, usize, std::time::Instant);
impl Perf {
    fn new(what: &'static str, n: usize) -> Perf {
        Perf(what, n, std::time::Instant::now())
    }
}
impl Drop for Perf {
    fn drop(&mut self) {
        if std::env::var_os("GROVE_PERF").is_some() {
            eprintln!("grove perf: {} ({} items): {:.1} ms on the UI thread", self.0, self.1, self.2.elapsed().as_secs_f64() * 1000.0);
        }
    }
}

/// `*` and `?` wildcards (case already folded by the caller).
pub fn wild_match(p: &str, s: &str) -> bool {
    let (p, s): (Vec<char>, Vec<char>) = (p.chars().collect(), s.chars().collect());
    let (mut pi, mut si, mut star, mut mark) = (0usize, 0usize, usize::MAX, 0usize);
    while si < s.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = pi;
            mark = si;
            pi += 1;
        } else if star != usize::MAX {
            pi = star + 1;
            mark += 1;
            si = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// computer:///, like Finder's Computer: the system disk, every other
/// disk (mounted or not: opening one that isn't mounts it), and network
/// places, each with its disk icon and free space.
fn computer_list() -> Vec<RawItem> {
    let places = crate::places::places(&[]);
    let mut out = vec![];
    let disk = |path: &str, name: &str, icon: &str, total: u64, free: u64| -> Option<RawItem> {
        let mut e = Entry::from_path(Path::new(path)).ok()?;
        e.name = name.to_string();
        e.hidden = false;
        e.note = if total > 0 { format!("{} free of {}", grove_core::util::human_size(free), grove_core::util::human_size(total)) } else { String::new() };
        e.size = total.saturating_sub(free);
        let mut r = raw_from_entry(e);
        r.icon = icon.to_string();
        Some(r)
    };
    // The system disk ("Macintosh HD"): named after the system.
    let os = crate::controller::os_name().to_string();
    let (total, free) = grove_core::mounts::space("/").unwrap_or((0, 0));
    out.extend(disk("/", &os, "drive-harddisk-system|drive-harddisk", total, free));
    for group in ["locations", "network"] {
        for d in places[group].as_array().into_iter().flatten() {
            let path = d["path"].as_str().unwrap_or("");
            let name = d["name"].as_str().unwrap_or("");
            let icon = d["icon"].as_str().unwrap_or("drive-harddisk");
            match d["kind"].as_str() {
                Some("root") => continue,
                Some("unmounted") => {
                    // Not mounted: a placeholder that mounts it when opened.
                    let dev = d["device"].as_str().unwrap_or("");
                    let Ok(mut e) = Entry::from_path(Path::new("/")) else { continue };
                    e.name = name.to_string();
                    e.path = format!("mount:{dev}");
                    e.note = "Not opened yet: open it to mount it".into();
                    e.size = 0;
                    let mut r = raw_from_entry(e);
                    r.icon = icon.to_string();
                    out.push(r);
                }
                _ if !path.is_empty() => out.extend(disk(path, name, icon, d["total"].as_u64().unwrap_or(0), d["free"].as_u64().unwrap_or(0))),
                _ => {}
            }
        }
    }
    out
}

/// trash:/// and recent:/// listings (and computer:///).
pub fn special_list(p: &str) -> Vec<RawItem> {
    if p.starts_with("computer:") {
        computer_list()
    } else if p.starts_with("trash:") {
        grove_core::trash::list()
            .into_iter()
            .filter_map(|t| {
                let fp = t.file_path();
                let mut e = Entry::from_path(&fp).ok()?;
                e.name = Path::new(&t.original).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(t.id.clone());
                e.hidden = false;
                e.note = Path::new(&t.original).parent().map(|d| d.display().to_string()).unwrap_or_default();
                e.key = serde_json::to_string(&t).unwrap_or_default();
                e.btime = t.deleted;
                Some(raw_from_entry(e))
            })
            .collect()
    } else if p.starts_with("recent:") {
        grove_core::recents::list(200)
            .into_iter()
            .filter_map(|p| {
                let mut e = Entry::from_path(&p).ok()?;
                e.note = p.parent().map(|d| d.display().to_string()).unwrap_or_default();
                Some(raw_from_entry(e))
            })
            .collect()
    } else if let Some(rest) = p.strip_prefix("archive://") {
        let (archive, inner) = rest.split_once('#').unwrap_or((rest, ""));
        // Nested archives open like folders too: list the innermost one.
        let (container, prefix, inner) = crate::archive::resolve(archive, inner).unwrap_or((archive.to_string(), String::new(), inner.to_string()));
        let inner = inner.as_str();
        let entries = crate::archive::list(&container).unwrap_or_default();
        let meta = std::fs::metadata(archive).ok();
        let mtime = meta.as_ref().map(|m| std::os::unix::fs::MetadataExt::mtime(m)).unwrap_or(0);
        crate::archive::children(&entries, inner)
            .into_iter()
            .map(|a| {
                let name = a.path.rsplit('/').next().unwrap_or(&a.path).to_string();
                let e = Entry {
                    hidden: name.starts_with('.'),
                    name,
                    path: format!("archive://{archive}#{prefix}{}", a.path),
                    is_dir: a.is_dir,
                    is_link: false,
                    size: a.size,
                    mtime,
                    atime: 0,
                    ctime: 0,
                    btime: 0,
                    mode: 0o644,
                    uid: 0,
                    gid: 0,
                    target: String::new(),
                    broken: false,
                    depth: 0,
                    note: a.mtime_text,
                    key: String::new(),
                    subvol: "",
                };
                let mime = mime::for_entry(&e);
                RawItem { icon: mime::icon_for(&e, &mime), e, mime, tags: vec![], rating: 0 }
            })
            .collect()
    } else {
        vec![]
    }
}
