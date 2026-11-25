//! Disk usage analysis (like gdu/ncdu): scan a folder tree once, in
//! parallel, into memory, then browse it instantly. Stays on the starting
//! disk (other mounts are left out), counts hard-linked files once, and
//! records both the space used on disk and the files' apparent sizes.

use std::collections::{HashMap, HashSet, VecDeque};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering as AO};
use std::sync::{Condvar, Mutex};

/// Live counters for a running scan (read from the UI while it runs).
#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
    pub errors: AtomicU64,
    pub cancel: AtomicBool,
    pub done: AtomicBool,
    pub current: Mutex<String>,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub name: Box<str>,
    pub parent: u32,
    pub first_child: u32,
    pub n_children: u32,
    /// Space used on disk (blocks), apparent size (file lengths).
    pub disk: u64,
    /// Of `disk`, the part shared with other files: hard links, reflinked
    /// copies, snapshots (btrfs extents marked shared; see [`shared_bytes`]).
    /// Only measured when the scan asks for it.
    pub shared: u64,
    pub apparent: u64,
    /// Files and folders inside (a file counts itself).
    pub items: u64,
    pub is_dir: bool,
    /// A folder that couldn't be read (its size is a lower bound).
    pub error: bool,
    pub mtime: i64,
    pub removed: bool,
}

pub struct Tree {
    pub root: PathBuf,
    pub nodes: Vec<Node>,
}

const NONE: u32 = u32::MAX;

struct Raw {
    name: String,
    is_dir: bool,
    disk: u64,
    shared: u64,
    apparent: u64,
    mtime: i64,
    dir_id: usize,
}

struct DirScan {
    entries: Vec<Raw>,
    error: bool,
}

struct Shared<'a> {
    queue: Mutex<VecDeque<(usize, PathBuf)>>,
    cv: Condvar,
    pending: AtomicUsize,
    next_id: AtomicUsize,
    results: Mutex<HashMap<usize, DirScan>>,
    links: Mutex<HashSet<(u64, u64)>>,
    /// Shared extents already counted (reflinked copies inside the scan
    /// count once, like hard links).
    extents: Mutex<HashSet<(u64, u64)>>,
    dev: u64,
    prog: &'a Progress,
    /// Measure shared data (btrfs: extents in use elsewhere too).
    fiemap: bool,
}

fn read_one(sh: &Shared, path: &Path) -> DirScan {
    let rd = match std::fs::read_dir(path) {
        Ok(r) => r,
        Err(_) => {
            sh.prog.errors.fetch_add(1, AO::Relaxed);
            return DirScan { entries: vec![], error: true };
        }
    };
    let mut entries = Vec::new();
    let mut bytes = 0u64;
    let mut files = 0u64;
    for e in rd.flatten() {
        let Ok(m) = e.metadata() else { continue };
        let name = e.file_name().to_string_lossy().into_owned();
        if m.is_dir() {
            if m.dev() != sh.dev {
                continue; // another disk mounted here
            }
            let id = sh.next_id.fetch_add(1, AO::Relaxed);
            sh.pending.fetch_add(1, AO::SeqCst);
            sh.queue.lock().unwrap().push_back((id, e.path()));
            sh.cv.notify_one();
            entries.push(Raw { name, is_dir: true, disk: m.blocks() * 512, shared: 0, apparent: m.len(), mtime: m.mtime(), dir_id: id });
        } else {
            let counted = m.nlink() <= 1 || sh.links.lock().unwrap().insert((m.dev(), m.ino()));
            let (mut disk, apparent) = if counted { (m.blocks() * 512, m.len()) } else { (0, 0) };
            // Shared: a hard-linked file as a whole; else (btrfs) the
            // extents other files or snapshots use too, each counted once
            // in the scan (a reflinked copy next to its original adds nothing).
            let shared = if disk == 0 {
                0
            } else if m.nlink() > 1 {
                disk
            } else if sh.fiemap && m.is_file() {
                let ext = shared_extents(&e.path());
                let (mut all, mut seen) = (0u64, 0u64);
                {
                    let mut known = sh.extents.lock().unwrap();
                    for x in ext {
                        all += x.1;
                        if !known.insert(x) {
                            seen += x.1;
                        }
                    }
                }
                let all = all.min(disk);
                disk = disk.saturating_sub(seen.min(all));
                all.saturating_sub(seen).min(disk)
            } else {
                0
            };
            bytes += disk;
            files += 1;
            entries.push(Raw { name, is_dir: false, disk, shared, apparent, mtime: m.mtime(), dir_id: 0 });
        }
    }
    sh.prog.files.fetch_add(files, AO::Relaxed);
    sh.prog.bytes.fetch_add(bytes, AO::Relaxed);
    sh.prog.dirs.fetch_add(1, AO::Relaxed);
    DirScan { entries, error: false }
}

fn worker(sh: &Shared) {
    loop {
        let job = {
            let mut q = sh.queue.lock().unwrap();
            loop {
                if let Some(j) = q.pop_front() {
                    break Some(j);
                }
                if sh.pending.load(AO::SeqCst) == 0 {
                    break None;
                }
                q = sh.cv.wait_timeout(q, std::time::Duration::from_millis(50)).unwrap().0;
            }
        };
        let Some((id, path)) = job else {
            sh.cv.notify_all();
            return;
        };
        let scan = if sh.prog.cancel.load(AO::Relaxed) {
            DirScan { entries: vec![], error: true }
        } else {
            if id % 64 == 0 {
                *sh.prog.current.lock().unwrap() = path.display().to_string();
            }
            read_one(sh, &path)
        };
        sh.results.lock().unwrap().insert(id, scan);
        if sh.pending.fetch_sub(1, AO::SeqCst) == 1 {
            sh.cv.notify_all();
        }
    }
}

/// Scans `root` (a folder) into a tree. Cancel through `prog.cancel`.
pub fn scan(root: &Path, prog: &Progress) -> Result<Tree, String> {
    scan_opts(root, prog, false, false)
}

/// The calling thread runs only when nothing else wants the CPU or the
/// disk (nice 19, idle I/O class): background scans stay out of the way.
pub fn idle_priority() {
    unsafe {
        let tid = libc::syscall(libc::SYS_gettid) as libc::id_t;
        libc::setpriority(libc::PRIO_PROCESS, tid, 19);
        // ioprio_set(IOPRIO_WHO_PROCESS, tid, IOPRIO_CLASS_IDLE << 13)
        libc::syscall(libc::SYS_ioprio_set, 1, tid, 3 << 13);
    }
}

/// btrfs (the only filesystem whose shared extents are worth measuring here).
pub fn is_btrfs(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else { return false };
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    unsafe { libc::statfs(c.as_ptr(), &mut st) == 0 && st.f_type as i64 == 0x9123683E }
}

/// The bytes of a file's data that other files share (reflinked copies,
/// snapshots, deduplicated data): its extents the filesystem marks shared
/// (FIEMAP, no root needed). Capped at `disk`.
pub fn shared_bytes(path: &Path, disk: u64) -> u64 {
    shared_extents(path).iter().map(|e| e.1).sum::<u64>().min(disk)
}

/// A file's shared extents: (where on the disk, length).
pub fn shared_extents(path: &Path) -> Vec<(u64, u64)> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    const FS_IOC_FIEMAP: libc::c_ulong = 0xC020_660B;
    const LAST: u32 = 0x1;
    const SHARED: u32 = 0x2000;
    const COUNT: usize = 64;
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Extent {
        logical: u64,
        physical: u64,
        length: u64,
        reserved64: [u64; 2],
        flags: u32,
        reserved: [u32; 3],
    }
    #[repr(C)]
    struct Map {
        start: u64,
        length: u64,
        flags: u32,
        mapped: u32,
        count: u32,
        reserved: u32,
        extents: [Extent; COUNT],
    }
    let Ok(f) = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_NOATIME).open(path).or_else(|_| std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(path)) else {
        return vec![];
    };
    let mut shared = vec![];
    let mut start = 0u64;
    for _ in 0..100_000 {
        let mut m = Map { start, length: u64::MAX - start, flags: 0, mapped: 0, count: COUNT as u32, reserved: 0, extents: [Extent::default(); COUNT] };
        if unsafe { libc::ioctl(f.as_raw_fd(), FS_IOC_FIEMAP as _, &mut m as *mut Map) } != 0 || m.mapped == 0 {
            break;
        }
        let n = (m.mapped as usize).min(COUNT);
        let mut last = false;
        for e in &m.extents[..n] {
            if e.flags & SHARED != 0 {
                shared.push((e.physical, e.length));
            }
            start = e.logical + e.length;
            last |= e.flags & LAST != 0;
        }
        if last {
            break;
        }
    }
    shared
}

/// [`scan`], also measuring shared data when `fiemap` (and on btrfs; each
/// file is opened for it, about 30 times the work of reading its size).
/// `background`: half the cores at most, at idle priority (disk usage mode
/// works while you browse); otherwise every core (the Disk Usage window).
pub fn scan_opts(root: &Path, prog: &Progress, fiemap: bool, background: bool) -> Result<Tree, String> {
    let meta = std::fs::metadata(root).map_err(|e| crate::util::plain_error(&e, root))?;
    if !meta.is_dir() {
        return Err(format!("{} isn't a folder.", root.display()));
    }
    let sh = Shared {
        queue: Mutex::new(VecDeque::from([(0usize, root.to_path_buf())])),
        cv: Condvar::new(),
        pending: AtomicUsize::new(1),
        next_id: AtomicUsize::new(1),
        results: Mutex::new(HashMap::new()),
        links: Mutex::new(HashSet::new()),
        extents: Mutex::new(HashSet::new()),
        dev: meta.dev(),
        prog,
        fiemap: fiemap && is_btrfs(root),
    };
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let n = if background { (cores / 2).clamp(1, 4) } else { cores.clamp(2, 16) };
    std::thread::scope(|s| {
        for _ in 0..n {
            s.spawn(|| {
                if background {
                    idle_priority();
                }
                worker(&sh)
            });
        }
    });
    let mut results = sh.results.into_inner().unwrap();

    // Lay the tree out breadth first so each folder's children sit together.
    let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| root.display().to_string());
    let mut nodes = vec![Node {
        name: name.into(),
        parent: NONE,
        first_child: NONE,
        n_children: 0,
        disk: meta.blocks() * 512,
        shared: 0,
        apparent: meta.len(),
        items: 0,
        is_dir: true,
        error: false,
        mtime: meta.mtime(),
        removed: false,
    }];
    let mut todo = VecDeque::from([(0u32, 0usize)]);
    while let Some((idx, dir_id)) = todo.pop_front() {
        let Some(scan) = results.remove(&dir_id) else { continue };
        let first = nodes.len() as u32;
        nodes[idx as usize].first_child = first;
        nodes[idx as usize].n_children = scan.entries.len() as u32;
        nodes[idx as usize].error = scan.error;
        for r in scan.entries {
            let i = nodes.len() as u32;
            if r.is_dir {
                todo.push_back((i, r.dir_id));
            }
            nodes.push(Node {
                name: r.name.into(),
                parent: idx,
                first_child: NONE,
                n_children: 0,
                disk: r.disk,
                shared: r.shared,
                apparent: r.apparent,
                items: if r.is_dir { 0 } else { 1 },
                is_dir: r.is_dir,
                error: false,
                mtime: r.mtime,
                removed: false,
            });
        }
    }
    // Children come after parents: add sizes up from the end.
    for i in (1..nodes.len()).rev() {
        let (d, a, it, sh) = (nodes[i].disk, nodes[i].apparent, nodes[i].items + if nodes[i].is_dir { 1 } else { 0 }, nodes[i].shared);
        let p = nodes[i].parent as usize;
        nodes[p].shared += sh;
        nodes[p].disk += d;
        nodes[p].apparent += a;
        nodes[p].items += it;
    }
    prog.done.store(true, AO::SeqCst);
    Ok(Tree { root: root.to_path_buf(), nodes })
}

impl Tree {
    pub fn children(&self, id: u32) -> impl Iterator<Item = (u32, &Node)> {
        let n = &self.nodes[id as usize];
        let start = if n.first_child == NONE { 0 } else { n.first_child };
        (start..start + n.n_children).map(move |i| (i, &self.nodes[i as usize])).filter(|(_, n)| !n.removed)
    }

    /// The node for a path inside the scanned folder.
    pub fn find(&self, path: &Path) -> Option<u32> {
        let rel = path.strip_prefix(&self.root).ok()?;
        let mut id = 0u32;
        for c in rel.components() {
            let c = c.as_os_str().to_string_lossy();
            id = self.children(id).find(|(_, n)| *n.name == *c)?.0;
        }
        Some(id)
    }

    pub fn path_of(&self, mut id: u32) -> PathBuf {
        let mut parts = Vec::new();
        while id != 0 && id != NONE {
            parts.push(&*self.nodes[id as usize].name);
            id = self.nodes[id as usize].parent;
        }
        let mut p = self.root.clone();
        for s in parts.into_iter().rev() {
            p.push(s);
        }
        p
    }

    /// Forget a removed (trashed/deleted) item: its size leaves every folder above it.
    pub fn remove(&mut self, id: u32) {
        if id == 0 || self.nodes[id as usize].removed {
            return;
        }
        let n = &self.nodes[id as usize];
        let (d, a, it) = (n.disk, n.apparent, n.items + if n.is_dir { 1 } else { 0 });
        self.nodes[id as usize].removed = true;
        let mut p = self.nodes[id as usize].parent;
        while p != NONE {
            let pn = &mut self.nodes[p as usize];
            pn.disk = pn.disk.saturating_sub(d);
            pn.apparent = pn.apparent.saturating_sub(a);
            pn.items = pn.items.saturating_sub(it);
            p = pn.parent;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_add_up_and_links_count_once() {
        let t = tempfile::tempdir().unwrap();
        let r = t.path();
        std::fs::create_dir_all(r.join("a/b")).unwrap();
        std::fs::write(r.join("a/b/big"), vec![1u8; 100_000]).unwrap();
        std::fs::write(r.join("a/small"), b"hi").unwrap();
        std::fs::write(r.join("top"), vec![2u8; 50_000]).unwrap();
        std::fs::hard_link(r.join("top"), r.join("a/toplink")).unwrap();
        let prog = Progress::default();
        let tree = scan(r, &prog).unwrap();
        assert!(prog.done.load(AO::SeqCst));
        let root = &tree.nodes[0];
        assert_eq!(root.apparent - std::fs::metadata(r).unwrap().len()
                   - std::fs::metadata(r.join("a")).unwrap().len()
                   - std::fs::metadata(r.join("a/b")).unwrap().len(), 150_002);
        assert_eq!(root.items, 6);
        let a = tree.find(&r.join("a")).unwrap();
        assert_eq!(tree.path_of(a), r.join("a"));
        let b = tree.find(&r.join("a/b/big")).unwrap();
        let before = tree.nodes[0].apparent;
        let mut tree = tree;
        tree.remove(b);
        assert_eq!(tree.nodes[0].apparent, before - 100_000);
        assert!(tree.find(&r.join("a/b/big")).is_none());
    }
}
