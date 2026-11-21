//! Reading folders: entries with the facts the views show, read in the
//! background and handed over in batches so a 100 000-item folder starts
//! showing at once; sorting (natural order, folders first).

use crate::util::natural_cmp;
use serde::Serialize;
use std::cmp::Ordering;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering as AO};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Entry {
    pub name: String,
    /// Full path.
    pub path: String,
    pub is_dir: bool,
    pub is_link: bool,
    /// Bytes (files); item count is filled in later for folders (-1 unknown).
    pub size: u64,
    pub mtime: i64,
    pub atime: i64,
    pub btime: i64,
    /// Status change time (Finder's "Date Added" stand-in: when it was put here).
    pub ctime: i64,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub hidden: bool,
    /// Link target for symlinks.
    pub target: String,
    /// The link points nowhere.
    pub broken: bool,
    /// Indentation level in an expanded list (0 = the folder itself).
    pub depth: u16,
    /// Extra text a special listing shows (trash: original location;
    /// search: the matching line).
    pub note: String,
    /// Original-path or other key used by special folders (trash item id).
    pub key: String,
}

impl Entry {
    pub fn from_path(path: &Path) -> std::io::Result<Entry> {
        let lmeta = std::fs::symlink_metadata(path)?;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
        Ok(Self::from_meta(name, path.to_string_lossy().into_owned(), path, lmeta))
    }

    fn from_meta(name: String, full: String, path: &Path, lmeta: std::fs::Metadata) -> Entry {
        let is_link = lmeta.file_type().is_symlink();
        let (meta, broken) = if is_link {
            match std::fs::metadata(path) {
                Ok(m) => (m, false),
                Err(_) => (lmeta.clone(), true),
            }
        } else {
            (lmeta.clone(), false)
        };
        let target = if is_link { std::fs::read_link(path).map(|t| t.to_string_lossy().into_owned()).unwrap_or_default() } else { String::new() };
        let btime = meta.created().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
        Entry {
            hidden: name.starts_with('.'),
            name,
            path: full,
            is_dir: meta.is_dir(),
            is_link,
            size: if meta.is_dir() { 0 } else { meta.len() },
            mtime: meta.mtime(),
            atime: meta.atime(),
            btime,
            ctime: lmeta.ctime(),
            mode: meta.mode(),
            uid: meta.uid(),
            gid: meta.gid(),
            target,
            broken,
            depth: 0,
            note: String::new(),
            key: String::new(),
        }
    }

    pub fn ext(&self) -> &str {
        if self.is_dir {
            return "";
        }
        match self.name.rfind('.') {
            Some(0) | None => "",
            Some(i) => &self.name[i + 1..],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Name,
    Size,
    Modified,
    Created,
    Kind,
    Accessed,
    Added,
}

impl SortKey {
    pub fn from_str(s: &str) -> SortKey {
        match s {
            "size" => SortKey::Size,
            "modified" | "date" => SortKey::Modified,
            "created" => SortKey::Created,
            "kind" | "type" => SortKey::Kind,
            "accessed" | "opened" => SortKey::Accessed,
            "added" => SortKey::Added,
            _ => SortKey::Name,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            SortKey::Name => "name",
            SortKey::Size => "size",
            SortKey::Modified => "modified",
            SortKey::Created => "created",
            SortKey::Kind => "kind",
            SortKey::Accessed => "accessed",
            SortKey::Added => "added",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Sort {
    pub key: SortKey,
    pub descending: bool,
    pub folders_first: bool,
}

pub fn compare(a: &Entry, b: &Entry, s: Sort) -> Ordering {
    if s.folders_first && a.is_dir != b.is_dir {
        return if a.is_dir { Ordering::Less } else { Ordering::Greater };
    }
    let o = match s.key {
        SortKey::Name => Ordering::Equal,
        SortKey::Size => a.size.cmp(&b.size),
        SortKey::Modified => a.mtime.cmp(&b.mtime),
        SortKey::Created => a.btime.cmp(&b.btime),
        SortKey::Accessed => a.atime.cmp(&b.atime),
        SortKey::Added => a.ctime.cmp(&b.ctime),
        SortKey::Kind => natural_cmp(a.ext(), b.ext()),
    };
    let o = o.then_with(|| natural_cmp(&a.name, &b.name));
    if s.descending {
        o.reverse()
    } else {
        o
    }
}

pub fn sort(v: &mut [Entry], s: Sort) {
    v.sort_by(|a, b| compare(a, b, s));
}

/// What a listing pass reports.
pub enum Batch {
    /// More entries (unsorted while `sorted` is false).
    Entries { entries: Vec<Entry> },
    /// The listing is complete.
    Done { error: Option<String> },
}

/// Read `dir`, calling `out` with batches. Small folders arrive in one
/// batch; a folder that takes longer than `first_after` streams a batch
/// every `every`. Stops early when `cancel` is set.
pub fn list_streaming(dir: &Path, cancel: &AtomicBool, first_after: Duration, every: Duration, mut out: impl FnMut(Batch)) {
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) => {
            out(Batch::Done { error: Some(crate::util::plain_error(&e, dir)) });
            return;
        }
    };
    let start = Instant::now();
    let mut last = start;
    let mut streaming = false;
    let mut buf: Vec<Entry> = Vec::with_capacity(512);
    for (i, de) in rd.enumerate() {
        if i % 256 == 0 && cancel.load(AO::Relaxed) {
            return;
        }
        let Ok(de) = de else { continue };
        let name = de.file_name().to_string_lossy().into_owned();
        let path = de.path();
        let Ok(lmeta) = de.metadata() else { continue };
        let full = path.to_string_lossy().into_owned();
        buf.push(Entry::from_meta(name, full, &path, lmeta));
        if i % 128 == 0 {
            let now = Instant::now();
            let due = if streaming { now - last >= every } else { now - start >= first_after };
            if due && !buf.is_empty() {
                streaming = true;
                last = now;
                out(Batch::Entries { entries: std::mem::take(&mut buf) });
            }
        }
    }
    if cancel.load(AO::Relaxed) {
        return;
    }
    if !buf.is_empty() {
        out(Batch::Entries { entries: buf });
    }
    out(Batch::Done { error: None });
}

/// All entries at once (tests, small helpers).
pub fn list(dir: &Path) -> std::io::Result<Vec<Entry>> {
    let mut v = Vec::new();
    for de in std::fs::read_dir(dir)? {
        let de = de?;
        if let Ok(m) = de.metadata() {
            let p = de.path();
            v.push(Entry::from_meta(de.file_name().to_string_lossy().into_owned(), p.to_string_lossy().into_owned(), &p, m));
        }
    }
    Ok(v)
}

/// Number of items in a folder (shown as a folder's "size" in list views).
pub fn count_children(dir: &Path, show_hidden: bool) -> Option<u64> {
    let rd = std::fs::read_dir(dir).ok()?;
    Some(rd.filter_map(|e| e.ok()).filter(|e| show_hidden || !e.file_name().to_string_lossy().starts_with('.')).count() as u64)
}

/// Total size and item count of a folder tree (Get Info), stopping when
/// cancelled. `tick` gets running totals now and then.
pub fn tree_size(path: &Path, cancel: &AtomicBool, mut tick: impl FnMut(u64, u64)) -> (u64, u64) {
    let mut bytes = 0u64;
    let mut items = 0u64;
    let mut stack = vec![PathBuf::from(path)];
    let mut seen = std::collections::HashSet::new();
    let mut last = Instant::now();
    while let Some(d) = stack.pop() {
        if cancel.load(AO::Relaxed) {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let Ok(m) = e.metadata() else { continue };
            items += 1;
            if m.is_dir() {
                if seen.insert((m.dev(), m.ino())) {
                    stack.push(e.path());
                }
            } else if m.nlink() <= 1 || seen.insert((m.dev(), m.ino())) {
                bytes += m.blocks() * 512;
            }
        }
        if last.elapsed() > Duration::from_millis(150) {
            last = Instant::now();
            tick(bytes, items);
        }
    }
    (bytes, items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_and_sorts() {
        let t = tempfile::tempdir().unwrap();
        for n in ["b10.txt", "b2.txt", ".hidden", "Apple.png"] {
            std::fs::write(t.path().join(n), n).unwrap();
        }
        std::fs::create_dir(t.path().join("zdir")).unwrap();
        std::os::unix::fs::symlink("nowhere", t.path().join("dangling")).unwrap();
        let cancel = AtomicBool::new(false);
        let mut all = vec![];
        let mut done = false;
        list_streaming(t.path(), &cancel, Duration::ZERO, Duration::ZERO, |b| match b {
            Batch::Entries { entries } => all.extend(entries),
            Batch::Done { error } => {
                assert!(error.is_none());
                done = true
            }
        });
        assert!(done);
        assert_eq!(all.len(), 6);
        sort(&mut all, Sort { key: SortKey::Name, descending: false, folders_first: true });
        let names: Vec<_> = all.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["zdir", ".hidden", "Apple.png", "b2.txt", "b10.txt", "dangling"]);
        let d = all.iter().find(|e| e.name == "dangling").unwrap();
        assert!(d.is_link && d.broken);
        assert!(all.iter().find(|e| e.name == ".hidden").unwrap().hidden);
        sort(&mut all, Sort { key: SortKey::Size, descending: true, folders_first: false });
        assert_eq!(all[0].name, "Apple.png");
    }

    #[test]
    fn missing_folder_is_plain_error() {
        let cancel = AtomicBool::new(false);
        let mut err = None;
        list_streaming(Path::new("/nonexistent/grove"), &cancel, Duration::ZERO, Duration::ZERO, |b| {
            if let Batch::Done { error } = b {
                err = error
            }
        });
        assert!(err.unwrap().contains("isn't there"));
    }

    #[test]
    fn tree_sizes() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("a/b")).unwrap();
        std::fs::write(t.path().join("a/b/f"), vec![0u8; 10000]).unwrap();
        let c = AtomicBool::new(false);
        let (bytes, items) = tree_size(t.path(), &c, |_, _| {});
        assert_eq!(items, 3);
        assert!(bytes >= 10000);
    }
}
