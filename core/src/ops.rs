//! File operations: copy, move, link and duplicate with progress, pause and
//! cancel, conflict questions (replace / skip / keep both / merge, "apply
//! to all"), and the undo/redo history.
//!
//! A replaced item goes to the Trash rather than being deleted, so an undo
//! can bring it back.

use crate::trash::{self, TrashItem};
use crate::util::{self, plain_error};
use serde::Serialize;
use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Kind {
    Copy,
    Move,
    Link,
    Duplicate,
}

impl Kind {
    pub fn from_str(s: &str) -> Kind {
        match s {
            "move" => Kind::Move,
            "link" => Kind::Link,
            "duplicate" => Kind::Duplicate,
            _ => Kind::Copy,
        }
    }
    pub fn verb(&self) -> &'static str {
        match self {
            Kind::Copy => "Copying",
            Kind::Move => "Moving",
            Kind::Link => "Making aliases of",
            Kind::Duplicate => "Duplicating",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Spec {
    pub kind: Kind,
    pub sources: Vec<PathBuf>,
    /// The folder to copy/move/link into (ignored for Duplicate).
    pub dest: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    Replace,
    Skip,
    KeepBoth,
    /// Folders only: put the contents together (asking again for files
    /// that exist in both).
    Merge,
    Cancel,
}

impl Resolution {
    pub fn from_str(s: &str) -> Resolution {
        match s {
            "replace" => Resolution::Replace,
            "skip" => Resolution::Skip,
            "keepboth" | "keep" => Resolution::KeepBoth,
            "merge" => Resolution::Merge,
            _ => Resolution::Cancel,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ConflictInfo {
    pub source: String,
    pub target: String,
    pub source_is_dir: bool,
    pub target_is_dir: bool,
    pub source_size: u64,
    pub target_size: u64,
    pub source_mtime: i64,
    pub target_mtime: i64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Progress {
    pub total_bytes: u64,
    pub done_bytes: u64,
    pub total_items: u64,
    pub done_items: u64,
    pub current: String,
}

/// What the job asks the user and tells them while it runs.
pub trait Host: Send + Sync {
    fn progress(&self, p: &Progress);
    /// Called from the job's thread; may block until the user answers.
    /// Returns the choice and whether it applies to all later conflicts.
    fn conflict(&self, c: &ConflictInfo) -> (Resolution, bool);
}

#[derive(Default)]
pub struct Control {
    cancelled: AtomicBool,
    paused: Mutex<bool>,
    cv: Condvar,
}

impl Control {
    pub fn pause(&self) {
        *self.paused.lock().unwrap() = true;
    }
    pub fn resume(&self) {
        *self.paused.lock().unwrap() = false;
        self.cv.notify_all();
    }
    pub fn is_paused(&self) -> bool {
        *self.paused.lock().unwrap()
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.resume();
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
    /// Blocks while paused; false when cancelled.
    fn checkpoint(&self) -> bool {
        let mut p = self.paused.lock().unwrap();
        while *p && !self.is_cancelled() {
            p = self.cv.wait(p).unwrap();
        }
        !self.is_cancelled()
    }
}

#[derive(Debug, Default)]
pub struct Outcome {
    /// What it did, for Undo (None when nothing changed).
    pub undo: Option<Action>,
    /// Where the items ended up (to select them afterwards).
    pub results: Vec<PathBuf>,
    pub errors: Vec<String>,
    pub cancelled: bool,
    /// Some items were refused for lack of rights (administrator rights
    /// may do them).
    pub denied: bool,
}

struct Run<'a> {
    ctl: &'a Control,
    host: &'a dyn Host,
    prog: Progress,
    last_report: Instant,
    apply_all: Option<Resolution>,
    created: Vec<PathBuf>,
    moved: Vec<(PathBuf, PathBuf)>,
    replaced: Vec<TrashItem>,
    errors: Vec<String>,
    denied: bool,
    buf: Vec<u8>,
}

/// Finder's "keep both" name: "name 2.ext", "name 3.ext"...
pub fn unique_name(target: &Path) -> PathBuf {
    let dir = target.parent().unwrap_or(Path::new("/"));
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (stem, ext) = split_name(&name, target.is_dir());
    for n in 2.. {
        let c = dir.join(format!("{stem} {n}{ext}"));
        if c.symlink_metadata().is_err() {
            return c;
        }
    }
    unreachable!()
}

/// Finder's duplicate name: "name copy.ext", "name copy 2.ext"...
pub fn duplicate_name(src: &Path) -> PathBuf {
    let dir = src.parent().unwrap_or(Path::new("/"));
    let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (stem, ext) = split_name(&name, src.is_dir());
    let first = dir.join(format!("{stem} copy{ext}"));
    if first.symlink_metadata().is_err() {
        return first;
    }
    for n in 2.. {
        let c = dir.join(format!("{stem} copy {n}{ext}"));
        if c.symlink_metadata().is_err() {
            return c;
        }
    }
    unreachable!()
}

/// "archive.tar.gz" keeps ".tar.gz" together; folders have no extension.
pub fn split_name(name: &str, is_dir: bool) -> (String, String) {
    if is_dir {
        return (name.to_string(), String::new());
    }
    for multi in [".tar.gz", ".tar.xz", ".tar.zst", ".tar.zstd", ".tar.bz2", ".tar.bz", ".tar.lz", ".tar.lzma", ".tar.lz4", ".tar.z"] {
        if name.len() > multi.len() && name.to_ascii_lowercase().ends_with(multi) {
            let i = name.len() - multi.len();
            return (name[..i].to_string(), name[i..].to_string());
        }
    }
    match name.rfind('.') {
        Some(i) if i > 0 => (name[..i].to_string(), name[i..].to_string()),
        _ => (name.to_string(), String::new()),
    }
}

fn scan(p: &Path, prog: &mut Progress) {
    let Ok(m) = std::fs::symlink_metadata(p) else { return };
    prog.total_items += 1;
    if m.is_dir() {
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                scan(&e.path(), prog);
            }
        }
    } else if m.is_file() {
        prog.total_bytes += m.len();
    }
}

fn same_device(a: &Path, b: &Path) -> bool {
    match (std::fs::symlink_metadata(a), std::fs::metadata(b)) {
        (Ok(x), Ok(y)) => x.dev() == y.dev(),
        _ => false,
    }
}

impl<'a> Run<'a> {
    fn fail(&mut self, e: &std::io::Error, p: &Path) {
        self.denied |= util::is_denied(e);
        self.errors.push(plain_error(e, p));
    }

    fn report(&mut self, force: bool) {
        if force || self.last_report.elapsed() >= Duration::from_millis(100) {
            self.last_report = Instant::now();
            self.host.progress(&self.prog);
        }
    }

    fn ask(&mut self, src: &Path, dst: &Path) -> Resolution {
        let sm = std::fs::symlink_metadata(src).ok();
        let dm = std::fs::symlink_metadata(dst).ok();
        let both_dirs = sm.as_ref().map(|m| m.is_dir()).unwrap_or(false) && dm.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        if let Some(r) = self.apply_all {
            // "Merge" for all means "replace" for files.
            return if r == Resolution::Merge && !both_dirs { Resolution::Replace } else { r };
        }
        let info = ConflictInfo {
            source: src.to_string_lossy().into_owned(),
            target: dst.to_string_lossy().into_owned(),
            source_is_dir: sm.as_ref().map(|m| m.is_dir()).unwrap_or(false),
            target_is_dir: dm.as_ref().map(|m| m.is_dir()).unwrap_or(false),
            source_size: sm.as_ref().map(|m| m.len()).unwrap_or(0),
            target_size: dm.as_ref().map(|m| m.len()).unwrap_or(0),
            source_mtime: sm.as_ref().map(|m| m.mtime()).unwrap_or(0),
            target_mtime: dm.as_ref().map(|m| m.mtime()).unwrap_or(0),
        };
        self.report(true);
        let (r, all) = self.host.conflict(&info);
        let r = if r == Resolution::Merge && !both_dirs { Resolution::Replace } else { r };
        if all {
            self.apply_all = Some(r);
        }
        r
    }

    /// Put an existing target out of the way (to the Trash).
    fn replace(&mut self, dst: &Path) -> bool {
        match trash::trash(dst) {
            Ok(it) => {
                self.replaced.push(it);
                true
            }
            Err(_) => match remove_tree(dst) {
                Ok(()) => true,
                Err(e) => {
                    self.fail(&e, dst);
                    false
                }
            },
        }
    }

    /// Resolve where `src` should go when `dst` may exist. None: skip it.
    /// Some((target, merge)).
    fn resolve(&mut self, src: &Path, dst: PathBuf) -> Option<(PathBuf, bool)> {
        if dst.symlink_metadata().is_err() {
            return Some((dst, false));
        }
        match self.ask(src, &dst) {
            Resolution::Skip => None,
            Resolution::Cancel => {
                self.ctl.cancel();
                None
            }
            Resolution::KeepBoth => Some((unique_name(&dst), false)),
            Resolution::Merge => Some((dst, true)),
            Resolution::Replace => {
                if self.replace(&dst) {
                    Some((dst, false))
                } else {
                    None
                }
            }
        }
    }

    fn copy_file(&mut self, src: &Path, dst: &Path, meta: &std::fs::Metadata) -> std::io::Result<bool> {
        let mut r = File::open(src)?;
        let mut w = std::fs::OpenOptions::new().write(true).create_new(true).open(dst)?;
        loop {
            if !self.ctl.checkpoint() {
                drop(w);
                let _ = std::fs::remove_file(dst);
                return Ok(false);
            }
            let n = r.read(&mut self.buf)?;
            if n == 0 {
                break;
            }
            w.write_all(&self.buf[..n])?;
            self.prog.done_bytes += n as u64;
            self.report(false);
        }
        let _ = w.set_permissions(std::fs::Permissions::from_mode(meta.mode() & 0o7777));
        if let Ok(t) = meta.modified() {
            let _ = w.set_times(std::fs::FileTimes::new().set_modified(t).set_accessed(meta.accessed().unwrap_or(t)));
        }
        Ok(true)
    }

    /// Copy `src` to `dst` (dst may exist only when merging folders).
    /// Records created top-level paths when `top`.
    fn copy_item(&mut self, src: &Path, dst: &Path, merge: bool, top: bool) {
        if !self.ctl.checkpoint() {
            return;
        }
        let meta = match std::fs::symlink_metadata(src) {
            Ok(m) => m,
            Err(e) => {
                self.fail(&e, src);
                return;
            }
        };
        self.prog.current = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let ft = meta.file_type();
        if ft.is_dir() {
            if !merge {
                if let Err(e) = std::fs::create_dir(dst) {
                    self.fail(&e, dst);
                    return;
                }
                if top {
                    self.created.push(dst.to_path_buf());
                }
            }
            self.prog.done_items += 1;
            let rd = match std::fs::read_dir(src) {
                Ok(r) => r,
                Err(e) => {
                    self.fail(&e, src);
                    return;
                }
            };
            for e in rd.flatten() {
                if self.ctl.is_cancelled() {
                    return;
                }
                let child = e.path();
                let target = dst.join(e.file_name());
                if merge {
                    // Inside a merged folder: ask again for clashes, and
                    // remember new items for Undo.
                    if let Some((t, m)) = self.resolve(&child, target) {
                        let fresh = t.symlink_metadata().is_err();
                        self.copy_item(&child, &t, m, false);
                        if fresh && t.symlink_metadata().is_ok() {
                            self.created.push(t);
                        }
                    }
                } else {
                    self.copy_item(&child, &target, false, false);
                }
            }
            let _ = std::fs::set_permissions(dst, std::fs::Permissions::from_mode(meta.mode() & 0o7777));
            if let Ok(t) = meta.modified() {
                if let Ok(f) = File::open(dst) {
                    let _ = f.set_times(std::fs::FileTimes::new().set_modified(t));
                }
            }
        } else if ft.is_symlink() {
            match std::fs::read_link(src).and_then(|t| std::os::unix::fs::symlink(t, dst)) {
                Ok(()) => {
                    if top {
                        self.created.push(dst.to_path_buf());
                    }
                }
                Err(e) => self.fail(&e, dst),
            }
            self.prog.done_items += 1;
        } else {
            match self.copy_file(src, dst, &meta) {
                Ok(true) => {
                    if top {
                        self.created.push(dst.to_path_buf());
                    }
                }
                Ok(false) => {}
                Err(e) => {
                    let _ = std::fs::remove_file(dst);
                    self.fail(&e, src);
                }
            }
            self.prog.done_items += 1;
        }
        self.report(false);
    }

    /// Move `src` to `dst`: a rename on the same disk, else copy + delete.
    fn move_item(&mut self, src: &Path, dst: &Path, merge: bool) {
        if !self.ctl.checkpoint() {
            return;
        }
        self.prog.current = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if merge {
            // Move the children one by one into the existing folder.
            let rd = match std::fs::read_dir(src) {
                Ok(r) => r,
                Err(e) => {
                    self.fail(&e, src);
                    return;
                }
            };
            for e in rd.flatten() {
                if self.ctl.is_cancelled() {
                    return;
                }
                let child = e.path();
                if let Some((t, m)) = self.resolve(&child, dst.join(e.file_name())) {
                    self.move_item(&child, &t, m);
                }
            }
            // Remove the source folder if it's empty now.
            let _ = std::fs::remove_dir(src);
            return;
        }
        match std::fs::rename(src, dst) {
            Ok(()) => {
                let mut sub = Progress::default();
                scan(dst, &mut sub);
                self.prog.done_items += sub.total_items;
                self.prog.done_bytes += sub.total_bytes;
                self.moved.push((src.to_path_buf(), dst.to_path_buf()));
            }
            Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
                let before = self.errors.len();
                let created_before = self.created.len();
                self.copy_item(src, dst, false, true);
                self.created.truncate(created_before);
                if self.errors.len() == before && !self.ctl.is_cancelled() {
                    if let Err(e) = remove_tree(src) {
                        self.fail(&e, src);
                    }
                    self.moved.push((src.to_path_buf(), dst.to_path_buf()));
                } else if dst.symlink_metadata().is_ok() && !src.exists() {
                    self.moved.push((src.to_path_buf(), dst.to_path_buf()));
                } else {
                    // Half-copied: leave the original alone, drop the copy.
                    let _ = remove_tree(dst);
                }
            }
            Err(e) => self.fail(&e, src),
        }
        self.report(false);
    }
}

/// Run a job on the current thread.
pub fn run(spec: &Spec, ctl: &Control, host: &dyn Host) -> Outcome {
    let mut r = Run {
        ctl,
        host,
        prog: Progress::default(),
        last_report: Instant::now() - Duration::from_secs(1),
        apply_all: None,
        created: vec![],
        moved: vec![],
        replaced: vec![],
        errors: vec![],
        denied: false,
        buf: vec![0u8; 1 << 20],
    };
    // Totals first (a rename on one disk is counted as done at once).
    for s in &spec.sources {
        if spec.kind == Kind::Link {
            r.prog.total_items += 1;
        } else {
            scan(s, &mut r.prog);
        }
    }
    r.report(true);
    let mut results = Vec::new();
    for src in &spec.sources {
        if ctl.is_cancelled() {
            break;
        }
        let Some(name) = src.file_name() else { continue };
        let (dest_dir, mut target) = match spec.kind {
            Kind::Duplicate => {
                let d = src.parent().unwrap_or(Path::new("/")).to_path_buf();
                (d, duplicate_name(src))
            }
            _ => (spec.dest.clone(), spec.dest.join(name)),
        };
        if src.symlink_metadata().is_err() {
            r.errors.push(format!("“{}” isn't there any more.", name.to_string_lossy()));
            continue;
        }
        if spec.kind != Kind::Link && src.is_dir() && dest_dir.starts_with(src) {
            r.errors.push(format!("“{}” can't be put inside itself.", name.to_string_lossy()));
            continue;
        }
        if target == *src {
            match spec.kind {
                // Moving onto itself: nothing to do.
                Kind::Move => continue,
                // Pasting a copy where it already is: make "name copy".
                Kind::Copy => target = duplicate_name(src),
                _ => {}
            }
        }
        let Some((target, merge)) = r.resolve(src, target) else { continue };
        match spec.kind {
            Kind::Copy | Kind::Duplicate => r.copy_item(src, &target, merge, true),
            Kind::Move => {
                if !merge && !same_device(src, &dest_dir) {
                    // (Copy + delete: counted by bytes.)
                }
                r.move_item(src, &target, merge)
            }
            Kind::Link => {
                match std::os::unix::fs::symlink(src, &target) {
                    Ok(()) => r.created.push(target.clone()),
                    Err(e) => r.fail(&e, &target),
                }
                r.prog.done_items += 1;
            }
        }
        if target.symlink_metadata().is_ok() {
            results.push(target);
        }
    }
    r.report(true);
    let mut seq = Vec::new();
    if !r.replaced.is_empty() {
        seq.push(Action::Trashed { items: std::mem::take(&mut r.replaced) });
    }
    if !r.moved.is_empty() {
        seq.push(Action::Moved { pairs: std::mem::take(&mut r.moved) });
    }
    if !r.created.is_empty() {
        seq.push(Action::Exists { paths: std::mem::take(&mut r.created) });
    }
    let undo = match seq.len() {
        0 => None,
        1 => seq.pop(),
        _ => Some(Action::Seq(seq)),
    };
    Outcome { undo, results, errors: r.errors, cancelled: ctl.is_cancelled(), denied: r.denied }
}

/// Delete a file or a whole folder for good.
pub fn remove_tree(p: &Path) -> std::io::Result<()> {
    let m = std::fs::symlink_metadata(p)?;
    if m.is_dir() {
        // Make read-only folders removable.
        if m.mode() & 0o200 == 0 {
            let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(m.mode() | 0o700));
        }
        std::fs::remove_dir_all(p)
    } else {
        std::fs::remove_file(p)
    }
}

/// Copy without progress (restoring from a trash on another disk).
pub fn copy_tree_simple(src: &Path, dst: &Path) -> std::io::Result<()> {
    let m = std::fs::symlink_metadata(src)?;
    if m.is_dir() {
        std::fs::create_dir(dst)?;
        for e in std::fs::read_dir(src)? {
            let e = e?;
            copy_tree_simple(&e.path(), &dst.join(e.file_name()))?;
        }
        std::fs::set_permissions(dst, m.permissions())?;
    } else if m.file_type().is_symlink() {
        std::os::unix::fs::symlink(std::fs::read_link(src)?, dst)?;
    } else {
        std::fs::copy(src, dst)?;
    }
    Ok(())
}

/// Move one path to another (rename, or copy + delete across disks);
/// refuses to overwrite.
pub fn move_path(from: &Path, to: &Path) -> Result<(), String> {
    if to.symlink_metadata().is_ok() {
        return Err(format!("An item named “{}” is already there.", to.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
    }
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
            copy_tree_simple(from, to).map_err(|e| plain_error(&e, from))?;
            remove_tree(from).map_err(|e| plain_error(&e, from))
        }
        Err(e) => Err(plain_error(&e, from)),
    }
}

/// Rename in place (inline rename). Plain-language checks first.
pub fn rename(path: &Path, new_name: &str) -> Result<Action, String> {
    let new_name = new_name.trim_end_matches('/');
    if new_name.is_empty() {
        return Err("A name can't be empty.".into());
    }
    if new_name.contains('/') {
        return Err("A name can't contain “/”.".into());
    }
    if new_name == "." || new_name == ".." {
        return Err("“.” and “..” are reserved names.".into());
    }
    let to = path.parent().unwrap_or(Path::new("/")).join(new_name);
    if to == path {
        return Ok(Action::Moved { pairs: vec![] });
    }
    // A case-only change on a case-insensitive disk is the same file.
    let same_file = match (std::fs::symlink_metadata(path), std::fs::symlink_metadata(&to)) {
        (Ok(a), Ok(b)) => a.ino() == b.ino() && a.dev() == b.dev(),
        _ => false,
    };
    if !same_file && to.symlink_metadata().is_ok() {
        return Err(format!("The name “{new_name}” is already taken. Please choose a different name."));
    }
    std::fs::rename(path, &to).map_err(|e| plain_error(&e, path))?;
    Ok(Action::Moved { pairs: vec![(path.to_path_buf(), to)] })
}

/// New folder: "untitled folder", "untitled folder 2"... Returns its path.
pub fn new_folder(dir: &Path, name: Option<&str>) -> Result<(PathBuf, Action), String> {
    let base = dir.join(name.unwrap_or("untitled folder"));
    let p = if base.symlink_metadata().is_ok() { unique_name(&base) } else { base };
    std::fs::create_dir(&p).map_err(|e| plain_error(&e, &p))?;
    Ok((p.clone(), Action::Exists { paths: vec![p] }))
}

/// New empty file (or a copy of a template).
pub fn new_file(dir: &Path, name: &str, template: Option<&Path>) -> Result<(PathBuf, Action), String> {
    let base = dir.join(name);
    let p = if base.symlink_metadata().is_ok() { unique_name(&base) } else { base };
    match template {
        Some(t) => std::fs::copy(t, &p).map(|_| ()),
        None => std::fs::OpenOptions::new().write(true).create_new(true).open(&p).map(|_| ()),
    }
    .map_err(|e| plain_error(&e, &p))?;
    Ok((p.clone(), Action::Exists { paths: vec![p] }))
}

/// Something done that can be undone. Each `invert` does the opposite and
/// returns the action that would undo *that* (so redo is undo of undo).
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Items were moved/renamed from .0 to .1.
    Moved { pairs: Vec<(PathBuf, PathBuf)> },
    /// Items were put in the Trash.
    Trashed { items: Vec<TrashItem> },
    /// Items were created (copies, new folders, links, restored items).
    Exists { paths: Vec<PathBuf> },
    /// Several, in the order they happened.
    Seq(Vec<Action>),
}

impl Action {
    pub fn is_empty(&self) -> bool {
        match self {
            Action::Moved { pairs } => pairs.is_empty(),
            Action::Trashed { items } => items.is_empty(),
            Action::Exists { paths } => paths.is_empty(),
            Action::Seq(v) => v.iter().all(|a| a.is_empty()),
        }
    }

    pub fn invert(&self) -> Result<Action, String> {
        match self {
            Action::Moved { pairs } => {
                let mut back = Vec::new();
                for (from, to) in pairs.iter().rev() {
                    move_path(to, from)?;
                    back.push((to.clone(), from.clone()));
                }
                back.reverse();
                Ok(Action::Moved { pairs: back })
            }
            Action::Trashed { items } => {
                let mut paths = Vec::new();
                for it in items {
                    paths.push(trash::restore(it, None)?);
                }
                Ok(Action::Exists { paths })
            }
            Action::Exists { paths } => {
                let mut items = Vec::new();
                for p in paths.iter().rev() {
                    if p.symlink_metadata().is_err() {
                        continue;
                    }
                    items.push(trash::trash(p)?);
                }
                items.reverse();
                Ok(Action::Trashed { items })
            }
            Action::Seq(v) => {
                let mut out = Vec::new();
                for a in v.iter().rev() {
                    out.push(a.invert()?);
                }
                Ok(Action::Seq(out))
            }
        }
    }
}

/// Undo and Redo, with a label for the Edit menu ("Undo Move").
#[derive(Default)]
pub struct History {
    undo: Vec<(String, Action)>,
    redo: Vec<(String, Action)>,
}

impl History {
    pub fn push(&mut self, label: &str, a: Action) {
        if a.is_empty() {
            return;
        }
        self.undo.push((label.to_string(), a));
        self.redo.clear();
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
    }
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|(l, _)| l.as_str())
    }
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|(l, _)| l.as_str())
    }
    pub fn take_undo(&mut self) -> Option<(String, Action)> {
        self.undo.pop()
    }
    pub fn take_redo(&mut self) -> Option<(String, Action)> {
        self.redo.pop()
    }
    pub fn pushed_undone(&mut self, label: String, inverse: Action) {
        self.redo.push((label, inverse));
    }
    pub fn pushed_redone(&mut self, label: String, inverse: Action) {
        self.undo.push((label, inverse));
    }
    /// Undo the last action. Ok(label) or a plain error.
    pub fn undo(&mut self) -> Result<Option<String>, String> {
        let Some((label, a)) = self.undo.pop() else { return Ok(None) };
        match a.invert() {
            Ok(inv) => {
                self.redo.push((label.clone(), inv));
                Ok(Some(label))
            }
            Err(e) => Err(format!("Couldn't undo {label}: {e}")),
        }
    }
    pub fn redo(&mut self) -> Result<Option<String>, String> {
        let Some((label, a)) = self.redo.pop() else { return Ok(None) };
        match a.invert() {
            Ok(inv) => {
                self.undo.push((label.clone(), inv));
                Ok(Some(label))
            }
            Err(e) => Err(format!("Couldn't redo {label}: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as M;

    struct TestHost {
        answers: M<Vec<(Resolution, bool)>>,
        asked: M<Vec<ConflictInfo>>,
    }
    impl TestHost {
        fn new(a: Vec<(Resolution, bool)>) -> Self {
            TestHost { answers: M::new(a), asked: M::new(vec![]) }
        }
    }
    impl Host for TestHost {
        fn progress(&self, _: &Progress) {}
        fn conflict(&self, c: &ConflictInfo) -> (Resolution, bool) {
            self.asked.lock().unwrap().push(c.clone());
            let mut a = self.answers.lock().unwrap();
            if a.is_empty() {
                (Resolution::Cancel, false)
            } else {
                a.remove(0)
            }
        }
    }

    fn setup() -> (std::sync::MutexGuard<'static, ()>, tempfile::TempDir) {
        let g = crate::test_env_lock();
        let t = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_DATA_HOME", t.path().join("data"));
        std::fs::create_dir_all(t.path().join("src/dir/sub")).unwrap();
        std::fs::write(t.path().join("src/a.txt"), "A").unwrap();
        std::fs::write(t.path().join("src/dir/sub/deep.txt"), "deep").unwrap();
        std::fs::create_dir_all(t.path().join("dst")).unwrap();
        (g, t)
    }

    #[test]
    fn copy_then_undo_redo() {
        let (_g, t) = setup();
        let s = t.path().join("src");
        let d = t.path().join("dst");
        let host = TestHost::new(vec![]);
        let out = run(&Spec { kind: Kind::Copy, sources: vec![s.join("a.txt"), s.join("dir")], dest: d.clone() }, &Control::default(), &host);
        assert!(out.errors.is_empty(), "{:?}", out.errors);
        assert_eq!(std::fs::read_to_string(d.join("dir/sub/deep.txt")).unwrap(), "deep");
        assert_eq!(out.results.len(), 2);
        let mut h = History::default();
        h.push("Copy", out.undo.unwrap());
        h.undo().unwrap();
        assert!(!d.join("a.txt").exists() && !d.join("dir").exists());
        h.redo().unwrap();
        assert!(d.join("a.txt").exists() && d.join("dir/sub/deep.txt").exists());
    }

    #[test]
    fn conflicts_keep_both_skip_replace() {
        let (_g, t) = setup();
        let s = t.path().join("src");
        let d = t.path().join("dst");
        std::fs::write(d.join("a.txt"), "old").unwrap();
        // Keep both.
        let host = TestHost::new(vec![(Resolution::KeepBoth, false)]);
        let out = run(&Spec { kind: Kind::Copy, sources: vec![s.join("a.txt")], dest: d.clone() }, &Control::default(), &host);
        assert!(out.errors.is_empty());
        assert_eq!(std::fs::read_to_string(d.join("a 2.txt")).unwrap(), "A");
        assert_eq!(std::fs::read_to_string(d.join("a.txt")).unwrap(), "old");
        // Skip.
        let host = TestHost::new(vec![(Resolution::Skip, false)]);
        let out = run(&Spec { kind: Kind::Copy, sources: vec![s.join("a.txt")], dest: d.clone() }, &Control::default(), &host);
        assert!(out.undo.is_none());
        // Replace: the old file goes to the Trash, undo brings it back.
        let host = TestHost::new(vec![(Resolution::Replace, false)]);
        let out = run(&Spec { kind: Kind::Copy, sources: vec![s.join("a.txt")], dest: d.clone() }, &Control::default(), &host);
        assert_eq!(std::fs::read_to_string(d.join("a.txt")).unwrap(), "A");
        let mut h = History::default();
        h.push("Copy", out.undo.unwrap());
        h.undo().unwrap();
        assert_eq!(std::fs::read_to_string(d.join("a.txt")).unwrap(), "old");
        h.redo().unwrap();
        assert_eq!(std::fs::read_to_string(d.join("a.txt")).unwrap(), "A");
    }

    #[test]
    fn merge_folders_and_apply_all() {
        let (_g, t) = setup();
        let s = t.path().join("src");
        let d = t.path().join("dst");
        std::fs::create_dir_all(d.join("dir/sub")).unwrap();
        std::fs::write(d.join("dir/sub/deep.txt"), "mine").unwrap();
        std::fs::write(d.join("dir/keep.txt"), "keep").unwrap();
        // Merge the folder; then "skip, apply to all" for the clashing file.
        let host = TestHost::new(vec![(Resolution::Merge, false), (Resolution::Merge, false), (Resolution::Skip, true)]);
        let out = run(&Spec { kind: Kind::Move, sources: vec![s.join("dir")], dest: d.clone() }, &Control::default(), &host);
        assert!(out.errors.is_empty(), "{:?}", out.errors);
        assert_eq!(std::fs::read_to_string(d.join("dir/sub/deep.txt")).unwrap(), "mine");
        assert!(d.join("dir/keep.txt").exists());
        // The skipped file stays in the source.
        assert!(s.join("dir/sub/deep.txt").exists());
        assert_eq!(host.asked.lock().unwrap().len(), 3);
    }

    #[test]
    fn move_and_undo() {
        let (_g, t) = setup();
        let s = t.path().join("src");
        let d = t.path().join("dst");
        let host = TestHost::new(vec![]);
        let out = run(&Spec { kind: Kind::Move, sources: vec![s.join("a.txt"), s.join("dir")], dest: d.clone() }, &Control::default(), &host);
        assert!(out.errors.is_empty());
        assert!(!s.join("a.txt").exists() && d.join("a.txt").exists());
        let mut h = History::default();
        h.push("Move", out.undo.unwrap());
        assert_eq!(h.undo_label(), Some("Move"));
        h.undo().unwrap();
        assert!(s.join("a.txt").exists() && s.join("dir/sub/deep.txt").exists());
        assert!(!d.join("dir").exists());
        assert_eq!(h.redo_label(), Some("Move"));
        h.redo().unwrap();
        assert!(d.join("dir/sub/deep.txt").exists());
    }

    #[test]
    fn into_itself_refused() {
        let (_g, t) = setup();
        let s = t.path().join("src");
        let host = TestHost::new(vec![]);
        let out = run(&Spec { kind: Kind::Move, sources: vec![s.join("dir")], dest: s.join("dir/sub") }, &Control::default(), &host);
        assert_eq!(out.errors.len(), 1);
        assert!(out.errors[0].contains("inside itself"));
    }

    #[test]
    fn duplicate_and_names() {
        let (_g, t) = setup();
        let s = t.path().join("src");
        let host = TestHost::new(vec![]);
        run(&Spec { kind: Kind::Duplicate, sources: vec![s.join("a.txt")], dest: PathBuf::new() }, &Control::default(), &host);
        run(&Spec { kind: Kind::Duplicate, sources: vec![s.join("a.txt")], dest: PathBuf::new() }, &Control::default(), &host);
        assert!(s.join("a copy.txt").exists() && s.join("a copy 2.txt").exists());
        // Copy into the same folder also duplicates.
        run(&Spec { kind: Kind::Copy, sources: vec![s.join("dir")], dest: s.clone() }, &Control::default(), &host);
        assert!(s.join("dir copy/sub/deep.txt").exists());
        assert_eq!(split_name("x.tar.gz", false), ("x".into(), ".tar.gz".into()));
    }

    #[test]
    fn cancel_and_pause() {
        let (_g, t) = setup();
        let s = t.path().join("src");
        let d = t.path().join("dst");
        let ctl = Control::default();
        ctl.cancel();
        let out = run(&Spec { kind: Kind::Copy, sources: vec![s.join("a.txt")], dest: d.clone() }, &ctl, &TestHost::new(vec![]));
        assert!(out.cancelled);
        assert!(!d.join("a.txt").exists());
        // Paused job waits until resumed.
        let ctl = std::sync::Arc::new(Control::default());
        ctl.pause();
        let c2 = ctl.clone();
        let (s2, d2) = (s.clone(), d.clone());
        let h = std::thread::spawn(move || run(&Spec { kind: Kind::Copy, sources: vec![s2.join("a.txt")], dest: d2 }, &c2, &TestHost::new(vec![])));
        std::thread::sleep(Duration::from_millis(100));
        assert!(!d.join("a.txt").exists() || std::fs::read_to_string(d.join("a.txt")).unwrap().is_empty());
        ctl.resume();
        let out = h.join().unwrap();
        assert!(out.errors.is_empty());
        assert_eq!(std::fs::read_to_string(d.join("a.txt")).unwrap(), "A");
    }

    #[test]
    fn rename_and_new() {
        let (_g, t) = setup();
        let s = t.path().join("src");
        assert!(rename(&s.join("a.txt"), "dir").is_err());
        assert!(rename(&s.join("a.txt"), "x/y").is_err());
        let a = rename(&s.join("a.txt"), "b.txt").unwrap();
        assert!(s.join("b.txt").exists());
        let mut h = History::default();
        h.push("Rename", a);
        h.undo().unwrap();
        assert!(s.join("a.txt").exists());
        let (p1, _) = new_folder(&s, None).unwrap();
        let (p2, a2) = new_folder(&s, None).unwrap();
        assert_eq!(p1.file_name().unwrap(), "untitled folder");
        assert_eq!(p2.file_name().unwrap(), "untitled folder 2");
        h.push("New Folder", a2);
        h.undo().unwrap();
        assert!(!p2.exists());
    }
}
