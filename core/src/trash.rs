//! The freedesktop.org Trash specification (1.0): the home trash in
//! $XDG_DATA_HOME/Trash and a trash per disk ($topdir/.Trash/$uid or
//! $topdir/.Trash-$uid), shared with every other desktop file manager.

use crate::util::{self, escape_path, plain_error, unescape_path};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TrashItem {
    /// The trash directory holding it (…/Trash or …/.Trash-1000).
    pub trash_dir: String,
    /// Its name inside files/ (and info/<name>.trashinfo).
    pub id: String,
    /// Where it was before.
    pub original: String,
    pub deleted: i64,
}

impl TrashItem {
    pub fn file_path(&self) -> PathBuf {
        Path::new(&self.trash_dir).join("files").join(&self.id)
    }
    pub fn info_path(&self) -> PathBuf {
        Path::new(&self.trash_dir).join("info").join(format!("{}.trashinfo", self.id))
    }
}

pub fn home_trash() -> PathBuf {
    util::data_home().join("Trash")
}

fn uid() -> u32 {
    unsafe { libc::getuid() }
}

/// The top directory of the mount holding `path` (walk up while the device
/// stays the same).
pub fn mount_top(path: &Path) -> PathBuf {
    let dev = match std::fs::symlink_metadata(path) {
        Ok(m) => m.dev(),
        Err(_) => return PathBuf::from("/"),
    };
    let mut cur = path.to_path_buf();
    while let Some(parent) = cur.parent() {
        match std::fs::metadata(parent) {
            Ok(m) if m.dev() == dev => cur = parent.to_path_buf(),
            _ => break,
        }
    }
    cur
}

/// Which trash directory a file goes to, creating it if need be, and
/// whether Path= is relative to the disk's top (true for disk trashes).
fn trash_for(path: &Path) -> std::io::Result<(PathBuf, Option<PathBuf>)> {
    let home = home_trash();
    let _ = std::fs::create_dir_all(home.join("files"));
    let _ = std::fs::create_dir_all(home.join("info"));
    let home_dev = std::fs::metadata(&home).map(|m| m.dev()).ok();
    let file_dev = std::fs::symlink_metadata(path)?.dev();
    if home_dev == Some(file_dev) {
        return Ok((home, None));
    }
    let top = mount_top(path);
    // $topdir/.Trash: must be a real directory with the sticky bit.
    let shared = top.join(".Trash");
    if let Ok(m) = std::fs::symlink_metadata(&shared) {
        if m.is_dir() && !m.file_type().is_symlink() && m.mode() & 0o1000 != 0 {
            let d = shared.join(uid().to_string());
            if std::fs::create_dir_all(d.join("files")).is_ok() && std::fs::create_dir_all(d.join("info")).is_ok() {
                let _ = std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700));
                return Ok((d, Some(top)));
            }
        }
    }
    let own = top.join(format!(".Trash-{}", uid()));
    std::fs::create_dir_all(own.join("files"))?;
    std::fs::create_dir_all(own.join("info"))?;
    let _ = std::fs::set_permissions(&own, std::fs::Permissions::from_mode(0o700));
    Ok((own, Some(top)))
}

/// Move `path` to the trash. Returns where it went.
pub fn trash(path: &Path) -> Result<TrashItem, String> {
    let path = if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir().unwrap_or_default().join(path) };
    let (dir, top) = trash_for(&path).map_err(|e| {
        format!("{} It can't be moved to the Trash; you can delete it permanently instead.", plain_error(&e, &path))
    })?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "item".into());
    let rel = match &top {
        Some(t) => path.strip_prefix(t).map(|p| p.to_path_buf()).unwrap_or_else(|_| path.clone()),
        None => path.clone(),
    };
    let now = util::now_secs();
    let info = format!("[Trash Info]\nPath={}\nDeletionDate={}\n", escape_path(&rel), util::local_timestamp(now));
    // A name not taken yet: "name", "name.2", "name.3"... (the .trashinfo
    // is created first, exclusively, which reserves the name).
    let (stem, ext) = split_ext(&name);
    for n in 1..10000 {
        let id = if n == 1 { name.clone() } else { format!("{stem}.{n}{ext}") };
        let info_path = dir.join("info").join(format!("{id}.trashinfo"));
        let f = OpenOptions::new().write(true).create_new(true).open(&info_path);
        let mut f = match f {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(plain_error(&e, &path)),
        };
        let target = dir.join("files").join(&id);
        if target.symlink_metadata().is_ok() {
            let _ = std::fs::remove_file(&info_path);
            continue;
        }
        if let Err(e) = f.write_all(info.as_bytes()) {
            let _ = std::fs::remove_file(&info_path);
            return Err(plain_error(&e, &path));
        }
        drop(f);
        if let Err(e) = std::fs::rename(&path, &target) {
            let _ = std::fs::remove_file(&info_path);
            return Err(plain_error(&e, &path));
        }
        invalidate_dirsizes(&dir);
        return Ok(TrashItem { trash_dir: dir.to_string_lossy().into_owned(), id, original: path.to_string_lossy().into_owned(), deleted: now });
    }
    Err(format!("Couldn't find a free name in the Trash for “{name}”."))
}

fn split_ext(name: &str) -> (String, String) {
    match name.rfind('.') {
        Some(i) if i > 0 => (name[..i].to_string(), name[i..].to_string()),
        _ => (name.to_string(), String::new()),
    }
}

fn invalidate_dirsizes(dir: &Path) {
    // The optional directorysizes cache: simplest valid update is to drop it.
    let _ = std::fs::remove_file(dir.join("directorysizes"));
}

/// Every trash directory: home plus each mounted disk's.
pub fn all_trash_dirs() -> Vec<PathBuf> {
    let mut v = vec![home_trash()];
    let u = uid();
    for m in crate::mounts::mounts() {
        let top = PathBuf::from(&m.mount_point);
        for d in [top.join(".Trash").join(u.to_string()), top.join(format!(".Trash-{u}"))] {
            if d.join("info").is_dir() && !v.contains(&d) {
                v.push(d);
            }
        }
    }
    v
}

/// What's in the trash, newest first.
pub fn list() -> Vec<TrashItem> {
    let mut out = Vec::new();
    for dir in all_trash_dirs() {
        out.extend(list_dir(&dir));
    }
    out.sort_by(|a, b| b.deleted.cmp(&a.deleted));
    out
}

pub fn list_dir(dir: &Path) -> Vec<TrashItem> {
    let mut out = Vec::new();
    let top = dir_top(dir);
    let Ok(rd) = std::fs::read_dir(dir.join("info")) else { return out };
    for e in rd.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        let Some(id) = n.strip_suffix(".trashinfo") else { continue };
        if dir.join("files").join(id).symlink_metadata().is_err() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
        let (mut orig, mut date) = (None, 0);
        for line in text.lines() {
            if let Some(p) = line.strip_prefix("Path=") {
                let p = unescape_path(p.trim());
                orig = Some(if p.is_absolute() { p } else { top.join(p) });
            } else if let Some(d) = line.strip_prefix("DeletionDate=") {
                date = util::parse_local_timestamp(d).unwrap_or(0);
            }
        }
        if let Some(o) = orig {
            out.push(TrashItem { trash_dir: dir.to_string_lossy().into_owned(), id: id.to_string(), original: o.to_string_lossy().into_owned(), deleted: date });
        }
    }
    out
}

/// The disk top a trash directory belongs to (for relative Path= lines).
fn dir_top(dir: &Path) -> PathBuf {
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if name.starts_with(".Trash-") {
        return dir.parent().map(Path::to_path_buf).unwrap_or_default();
    }
    if dir.parent().and_then(|p| p.file_name()).map(|n| n == ".Trash").unwrap_or(false) {
        return dir.parent().and_then(Path::parent).map(Path::to_path_buf).unwrap_or_default();
    }
    PathBuf::from("/")
}

/// Put an item back where it was. Fails (without changing anything) when
/// something else has that name now, unless `to` gives another place.
pub fn restore(item: &TrashItem, to: Option<&Path>) -> Result<PathBuf, String> {
    let dest = to.map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(&item.original));
    if dest.symlink_metadata().is_ok() {
        return Err(format!("Can't put back “{}”: an item with that name is already at {}.", item.id, dest.parent().map(|p| p.display().to_string()).unwrap_or_default()));
    }
    if let Some(p) = dest.parent() {
        std::fs::create_dir_all(p).map_err(|e| plain_error(&e, p))?;
    }
    let src = item.file_path();
    match std::fs::rename(&src, &dest) {
        Ok(()) => {}
        Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
            crate::ops::copy_tree_simple(&src, &dest).map_err(|e| plain_error(&e, &src))?;
            crate::ops::remove_tree(&src).map_err(|e| plain_error(&e, &src))?;
        }
        Err(e) => return Err(plain_error(&e, &src)),
    }
    let _ = std::fs::remove_file(item.info_path());
    invalidate_dirsizes(Path::new(&item.trash_dir));
    Ok(dest)
}

/// Delete one trashed item for good.
pub fn delete(item: &TrashItem) -> Result<(), String> {
    let p = item.file_path();
    crate::ops::remove_tree(&p).map_err(|e| plain_error(&e, &p))?;
    let _ = std::fs::remove_file(item.info_path());
    invalidate_dirsizes(Path::new(&item.trash_dir));
    Ok(())
}

/// Empty every trash. Returns how many items were removed and the errors.
pub fn empty() -> (usize, Vec<String>) {
    empty_in(&all_trash_dirs())
}

/// Empty the given trash directories.
pub fn empty_in(dirs: &[PathBuf]) -> (usize, Vec<String>) {
    let mut n = 0;
    let mut errs = Vec::new();
    for dir in dirs.iter().cloned() {
        // Items with info files first, then strays in files/.
        for it in list_dir(&dir) {
            match delete(&it) {
                Ok(()) => n += 1,
                Err(e) => errs.push(e),
            }
        }
        if let Ok(rd) = std::fs::read_dir(dir.join("files")) {
            for e in rd.flatten() {
                if crate::ops::remove_tree(&e.path()).is_ok() {
                    n += 1;
                }
            }
        }
        if let Ok(rd) = std::fs::read_dir(dir.join("info")) {
            for e in rd.flatten() {
                let _ = std::fs::remove_file(e.path());
            }
        }
        invalidate_dirsizes(&dir);
    }
    (n, errs)
}

pub fn count() -> usize {
    all_trash_dirs().iter().map(|d| std::fs::read_dir(d.join("files")).map(|r| r.count()).unwrap_or(0)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_home<T>(f: impl FnOnce(&Path) -> T) -> T {
        let _g = crate::test_env_lock();
        let t = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_DATA_HOME", t.path().join("data"));
        let r = f(t.path());
        std::env::remove_var("XDG_DATA_HOME");
        r
    }

    #[test]
    fn trash_and_restore() {
        with_home(|root| {
            let f = root.join("note with space.txt");
            std::fs::write(&f, "hi").unwrap();
            let it = trash(&f).unwrap();
            assert!(!f.exists());
            assert!(it.file_path().exists());
            let info = std::fs::read_to_string(it.info_path()).unwrap();
            assert!(info.starts_with("[Trash Info]\nPath=/"));
            assert!(info.contains("note%20with%20space.txt"));
            let listed = list_dir(&home_trash());
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].original, f.to_string_lossy());
            // Same name again: gets a different id.
            std::fs::write(&f, "again").unwrap();
            let it2 = trash(&f).unwrap();
            assert_ne!(it.id, it2.id);
            assert_eq!(it2.id, "note with space.2.txt");
            // Restore blocked by an existing file, then allowed.
            std::fs::write(&f, "blocker").unwrap();
            assert!(restore(&it, None).is_err());
            std::fs::remove_file(&f).unwrap();
            restore(&it, None).unwrap();
            assert_eq!(std::fs::read_to_string(&f).unwrap(), "hi");
            assert!(!it.info_path().exists());
            // Empty (only the test's own trash: never the real disks').
            let (n, errs) = empty_in(&[home_trash()]);
            assert!(errs.is_empty());
            assert_eq!(n, 1);
            assert!(list_dir(&home_trash()).is_empty());
        });
    }

    #[test]
    fn trash_folder() {
        with_home(|root| {
            let d = root.join("dir");
            std::fs::create_dir_all(d.join("sub")).unwrap();
            std::fs::write(d.join("sub/x"), "x").unwrap();
            let it = trash(&d).unwrap();
            assert!(it.file_path().join("sub/x").exists());
            delete(&it).unwrap();
            assert!(!it.file_path().exists());
        });
    }

    #[test]
    fn relative_paths_for_disk_trash() {
        assert_eq!(dir_top(Path::new("/mnt/usb/.Trash-1000")), PathBuf::from("/mnt/usb"));
        assert_eq!(dir_top(Path::new("/mnt/usb/.Trash/1000")), PathBuf::from("/mnt/usb"));
    }
}
