//! Disk usage mode (View › Show Disk Usage): every item of a shown folder
//! with the space it takes, worked out in the background. Files are
//! measured at once; each subfolder is scanned (core's du, in parallel)
//! in turn and its row filled in as soon as it's done. Scans are kept, so
//! going into a subfolder is instant (and redone after a while).
//!
//! What counts: the space used on disk, hard-linked files once. On btrfs,
//! the part shared with other files (reflinked copies, snapshots) is
//! measured too, so a row can say how much would really free up. Other
//! disks mounted inside a folder, and btrfs snapshots (.snapshots), are
//! volumes of their own: shown, never added in. An archive is a volume of
//! its own too: inside one, sizes are its members' (uncompressed) sizes,
//! out of the archive's total.
//!
//! Answers arrive as `usageReady(folder, json)`: {volume: {total, used},
//! items: {path: {disk, shared, items, kind, error, estimate}}, max, pending:
//! [paths still being worked out], done}.

use crate::bridge::ffi::{self, QString};
use cxx_qt::CxxQtThread;
use grove_core::du;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, VecDeque};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Scans are redone when older than this.
const FRESH: Duration = Duration::from_secs(600);
/// Nodes of cached scans kept at most (older scans go first).
const MAX_NODES: usize = 4_000_000;

struct State {
    queue: VecDeque<String>,
    /// The folder being worked on, and the flag that stops its scan.
    current: Option<(String, Arc<du::Progress>)>,
    qt: Option<CxxQtThread<ffi::Grove>>,
}

fn state() -> &'static (Mutex<State>, Condvar) {
    static S: OnceLock<(Mutex<State>, Condvar)> = OnceLock::new();
    S.get_or_init(|| {
        let _ = std::thread::Builder::new().name("grove-usage".into()).spawn(worker);
        (Mutex::new(State { queue: VecDeque::new(), current: None, qt: None }), Condvar::new())
    })
}

fn trees() -> &'static Mutex<Vec<(Instant, Arc<du::Tree>)>> {
    static T: OnceLock<Mutex<Vec<(Instant, Arc<du::Tree>)>>> = OnceLock::new();
    T.get_or_init(Default::default)
}

/// Work out `folder`'s items, before any folder asked for earlier (one
/// being worked on goes back in line behind it).
pub fn request(qt: CxxQtThread<ffi::Grove>, folder: String) {
    let (m, cv) = state();
    let mut s = m.lock().unwrap();
    s.qt = Some(qt);
    s.queue.retain(|f| f != &folder);
    if let Some((cur, prog)) = &s.current {
        if cur == &folder {
            return;
        }
        prog.cancel.store(true, Ordering::Relaxed);
    }
    s.queue.push_front(folder);
    cv.notify_all();
}

/// Stop working on everything (the mode was turned off).
pub fn stop() {
    let (m, _) = state();
    let mut s = m.lock().unwrap();
    s.queue.clear();
    if let Some((_, prog)) = &s.current {
        prog.cancel.store(true, Ordering::Relaxed);
    }
}

/// Forget what's known about `folders` and what's in them (they changed).
pub fn forget(folders: &[String]) {
    let mut t = trees().lock().unwrap();
    t.retain(|(_, tree)| !folders.iter().any(|f| Path::new(f).starts_with(&tree.root)));
}

/// The "measure shared data" setting (off: names and sizes only, like
/// ncdu and gdu; on: each file is opened to ask btrfs what it shares).
static SHARED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static SHARED_INIT: std::sync::Once = std::sync::Once::new();
fn measure_shared() -> bool {
    SHARED_INIT.call_once(|| SHARED.store(grove_core::settings::load().usage_shared, Ordering::Relaxed));
    SHARED.load(Ordering::Relaxed)
}
pub fn set_measure_shared(on: bool) {
    SHARED_INIT.call_once(|| {});
    if SHARED.swap(on, Ordering::Relaxed) != on {
        // Kept scans were made the other way.
        trees().lock().unwrap().clear();
    }
}

fn worker() {
    // Out of the way of everything else: this works while you browse.
    du::idle_priority();
    loop {
        let (folder, prog) = {
            let (m, cv) = state();
            let mut s = m.lock().unwrap();
            let folder = loop {
                if let Some(f) = s.queue.pop_front() {
                    break f;
                }
                s = cv.wait(s).unwrap();
            };
            let prog = Arc::new(du::Progress::default());
            s.current = Some((folder.clone(), prog.clone()));
            (folder, prog)
        };
        let finished = work(&folder, &prog);
        let (m, _) = state();
        let mut s = m.lock().unwrap();
        s.current = None;
        // Stopped for another folder: back in line, behind it.
        if !finished && !s.queue.iter().any(|f| f == &folder) && !s.queue.is_empty() {
            let at = 1.min(s.queue.len());
            s.queue.insert(at, folder);
        }
    }
}

fn send(folder: &str, v: Value) {
    let qt = state().0.lock().unwrap().qt.clone();
    if let Some(qt) = qt {
        let (f, j) = (folder.to_string(), v.to_string());
        let _ = qt.queue(move |g| g.usage_ready(QString::from(&f), QString::from(&j)));
    }
}

fn statvfs(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let f = st.f_frsize as u64;
    Some((st.f_blocks as u64 * f, (st.f_blocks as u64 - st.f_bfree as u64) * f))
}

fn item(disk: u64, shared: u64, items: u64, kind: &str, error: bool) -> Value {
    json!({"disk": disk, "shared": shared, "items": items, "kind": kind, "error": error})
}

/// A folder's numbers from a kept scan, if one covers it and is fresh.
fn cached(path: &Path) -> Option<(u64, u64, u64, bool)> {
    let t = trees().lock().unwrap();
    for (at, tree) in t.iter().rev() {
        if at.elapsed() > FRESH || !path.starts_with(&tree.root) {
            continue;
        }
        if let Some(id) = tree.find(path) {
            let n = &tree.nodes[id as usize];
            return Some((n.disk, n.shared, n.items, n.error));
        }
    }
    None
}

fn keep(tree: du::Tree) {
    let mut t = trees().lock().unwrap();
    // A newer scan of the same folder (or one above it) replaces it.
    t.retain(|(_, old)| !old.root.starts_with(&tree.root));
    t.push((Instant::now(), Arc::new(tree)));
    let mut total: usize = t.iter().map(|(_, x)| x.nodes.len()).sum();
    while total > MAX_NODES && t.len() > 1 {
        total -= t[0].1.nodes.len();
        t.remove(0);
    }
}

fn mount_points() -> Vec<String> {
    grove_core::mounts::mounts().into_iter().map(|m| m.mount_point).collect()
}

/// Works out `folder`; false when it was stopped for another one.
fn work(folder: &str, prog: &Arc<du::Progress>) -> bool {
    if let Some(rest) = folder.strip_prefix("archive://") {
        archive(folder, rest);
        return true;
    }
    let dir = Path::new(folder);
    let Ok(meta) = std::fs::metadata(dir) else {
        send(folder, json!({"done": true, "items": {}, "error": "This folder can't be read."}));
        return true;
    };
    let dev = meta.dev();
    let (total, used) = statvfs(dir).unwrap_or((0, 0));
    let btrfs = du::is_btrfs(dir);
    let mounts = mount_points();
    let mut items = Map::new();
    let mut todo: Vec<PathBuf> = vec![];
    let mut snapshots: Option<String> = None;
    let mut names: Vec<_> = std::fs::read_dir(dir).map(|r| r.flatten().collect()).unwrap_or_default();
    names.sort_by(|a: &std::fs::DirEntry, b| grove_core::util::natural_cmp(&a.file_name().to_string_lossy(), &b.file_name().to_string_lossy()));
    for e in names {
        let path = e.path();
        let key = path.to_string_lossy().into_owned();
        let Ok(m) = std::fs::symlink_metadata(&path) else { continue };
        if m.file_type().is_symlink() {
            items.insert(key, item(0, 0, 0, "link", false));
            continue;
        }
        if !m.is_dir() {
            let disk = m.blocks() * 512;
            let shared = if m.nlink() > 1 { disk } else if btrfs && measure_shared() && m.is_file() && disk > 0 { du::shared_bytes(&path, disk) } else { 0 };
            items.insert(key, item(disk, shared, 1, if m.nlink() > 1 { "hardlink" } else { "file" }, false));
            continue;
        }
        if m.dev() != dev {
            // Another volume inside this folder: a snapshot folder, or a mounted disk.
            let under_snapshots = folder.contains("/.snapshots") || e.file_name() == ".snapshots";
            if under_snapshots {
                snapshots = Some(key.clone());
                items.insert(key, json!({"disk": 0, "shared": 0, "items": 0, "kind": "snapshots", "error": false}));
                continue;
            }
            if mounts.iter().any(|mp| mp == &key) {
                let (t, u) = statvfs(&path).unwrap_or((0, 0));
                items.insert(key, json!({"disk": u, "shared": 0, "items": 0, "kind": "mount", "error": false, "volumeTotal": t}));
                continue;
            }
            // A btrfs subvolume (not mounted on its own): part of this folder.
        }
        match cached(&path) {
            Some((d, s, n, err)) => {
                items.insert(key, item(d, s, n, "dir", err));
            }
            None => todo.push(path),
        }
    }
    let pending: Vec<String> = todo.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let max = |items: &Map<String, Value>| items.values().map(|v| v["disk"].as_u64().unwrap_or(0)).max().unwrap_or(0);
    send(folder, json!({"volume": {"total": total, "used": used}, "items": items, "max": max(&items), "pending": pending, "done": todo.is_empty()}));
    let mut left: Vec<String> = pending;
    for path in todo {
        if prog.cancel.load(Ordering::Relaxed) {
            return false;
        }
        let key = path.to_string_lossy().into_owned();
        let one = du::Progress::default();
        // Stop the inner scan when this folder's is stopped.
        let tree = std::thread::scope(|s| {
            let watch = s.spawn(|| {
                while !one.done.load(Ordering::Relaxed) {
                    if prog.cancel.load(Ordering::Relaxed) {
                        one.cancel.store(true, Ordering::Relaxed);
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            });
            let r = du::scan_opts(&path, &one, measure_shared(), true);
            one.done.store(true, Ordering::Relaxed);
            let _ = watch.join();
            r
        });
        if prog.cancel.load(Ordering::Relaxed) {
            return false;
        }
        left.retain(|p| p != &key);
        let v = match tree {
            Ok(t) => {
                let n = &t.nodes[0];
                let v = item(n.disk, n.shared, n.items, "dir", n.error || one.errors.load(Ordering::Relaxed) > 0);
                keep(t);
                v
            }
            Err(_) => item(0, 0, 0, "dir", true),
        };
        items.insert(key.clone(), v.clone());
        let mut one_item = Map::new();
        one_item.insert(key, v);
        send(folder, json!({"items": one_item, "max": max(&items), "pending": left, "done": left.is_empty()}));
    }
    // The volume's top folder: what the snapshots hold only for themselves,
    // about: the space in use less what the folder's own items take.
    if let Some(snap) = snapshots {
        if mounts.iter().any(|mp| mp == folder) {
            let live: u64 = items.iter().filter(|(k, _)| **k != snap).filter(|(_, v)| v["kind"] != "mount").map(|(_, v)| v["disk"].as_u64().unwrap_or(0)).sum();
            let est = used.saturating_sub(live);
            let mut one_item = Map::new();
            one_item.insert(snap, json!({"disk": est, "shared": 0, "items": 0, "kind": "snapshots", "error": false, "estimate": true}));
            send(folder, json!({"items": one_item, "max": max(&items).max(est), "pending": [], "done": true}));
        }
    }
    true
}

/// Inside an archive: its members' sizes, out of the archive's total.
fn archive(folder: &str, rest: &str) {
    let (file, inner) = rest.split_once('#').unwrap_or((rest, ""));
    let inner = inner.trim_matches('/');
    let entries = match crate::archive::list(file) {
        Ok(e) => e,
        Err(e) => {
            send(folder, json!({"done": true, "items": {}, "error": e}));
            return;
        }
    };
    let total: u64 = entries.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();
    let prefix = if inner.is_empty() { String::new() } else { format!("{inner}/") };
    let mut sums: HashMap<String, (u64, u64, bool)> = HashMap::new();
    for e in entries.iter() {
        let p = e.path.trim_matches('/');
        let Some(r) = p.strip_prefix(&prefix) else { continue };
        if r.is_empty() {
            continue;
        }
        let (child, deeper) = match r.split_once('/') {
            Some((c, _)) => (c, true),
            None => (r, false),
        };
        let s = sums.entry(child.to_string()).or_insert((0, 0, false));
        if !e.is_dir {
            s.0 += e.size;
            s.1 += 1;
        }
        s.2 |= deeper || e.is_dir;
    }
    let mut items = Map::new();
    for (child, (size, n, is_dir)) in sums {
        let key = format!("archive://{file}#{}{child}", prefix);
        items.insert(key, item(size, 0, n, if is_dir { "dir" } else { "file" }, false));
    }
    let max = items.values().map(|v| v["disk"].as_u64().unwrap_or(0)).max().unwrap_or(0);
    send(folder, json!({"volume": {"total": total, "used": total, "archive": true}, "items": items, "max": max, "pending": [], "done": true}));
}
