//! Previous versions from filesystem snapshots (snapper's hourly btrfs
//! snapshots: <subvolume>/.snapshots/<n>/snapshot/...), Time Machine
//! style: the distinct earlier copies of a file or folder, newest first.

use serde::Serialize;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Version {
    pub snapshot: u64,
    /// Seconds (from the snapshot's info.xml, UTC), or the folder's mtime.
    pub date: i64,
    pub description: String,
    /// The copy inside the snapshot (read-only).
    pub path: String,
    pub size: u64,
    pub mtime: i64,
    pub is_dir: bool,
}

/// The folder holding snapshots for `path`, and `path` relative to its subvolume.
pub fn snapshot_root(path: &Path) -> Option<(PathBuf, PathBuf)> {
    let mut cur = path.to_path_buf();
    loop {
        let snaps = cur.join(".snapshots");
        if snaps.is_dir() && std::fs::read_dir(&snaps).is_ok() {
            let rel = path.strip_prefix(&cur).ok()?.to_path_buf();
            return Some((snaps, rel));
        }
        if !cur.pop() {
            return None;
        }
    }
}

fn info_date(dir: &Path) -> (i64, String) {
    let t = std::fs::read_to_string(dir.join("info.xml")).unwrap_or_default();
    let tag = |name: &str| -> String {
        let open = format!("<{name}>");
        let close = format!("</{name}>");
        t.find(&open).and_then(|a| t[a + open.len()..].find(&close).map(|b| t[a + open.len()..a + open.len() + b].to_string())).unwrap_or_default()
    };
    let date = tag("date");
    // "2026-11-21 07:00:00" UTC
    let secs = crate::recents::parse_utc(&date.replace(' ', "T")).unwrap_or_else(|| std::fs::metadata(dir).map(|m| m.mtime()).unwrap_or(0));
    (secs, tag("description"))
}

/// Distinct earlier versions of `path`, newest first.
pub fn versions(path: &Path) -> Vec<Version> {
    let Some((snaps, rel)) = snapshot_root(path) else { return vec![] };
    let mut nums: Vec<(u64, PathBuf)> = std::fs::read_dir(&snaps)
        .map(|r| r.flatten().filter_map(|e| e.file_name().to_string_lossy().parse::<u64>().ok().map(|n| (n, e.path()))).collect())
        .unwrap_or_default();
    nums.sort_by(|a, b| b.0.cmp(&a.0));
    let current = std::fs::metadata(path).ok().map(|m| (m.mtime(), m.len()));
    let mut out: Vec<Version> = Vec::new();
    let mut last: Option<(i64, u64)> = current;
    for (n, dir) in nums {
        let copy = dir.join("snapshot").join(&rel);
        let Ok(m) = std::fs::symlink_metadata(&copy) else { continue };
        let key = (m.mtime(), m.len());
        // Only when it differs from the newer one (folders: their mtime).
        if Some(key) == last {
            continue;
        }
        last = Some(key);
        let (date, description) = info_date(&dir);
        out.push(Version { snapshot: n, date, description, path: copy.to_string_lossy().into_owned(), size: m.len(), mtime: m.mtime(), is_dir: m.is_dir() });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finds_distinct_versions() {
        let t = tempfile::tempdir().unwrap();
        let home = t.path().join("home");
        std::fs::create_dir_all(home.join("me")).unwrap();
        std::fs::write(home.join("me/doc.txt"), "now").unwrap();
        for (n, text, date) in [(1, "old", "2026-11-01 10:00:00"), (2, "old", "2026-11-02 10:00:00"), (3, "newer", "2026-11-03 10:00:00")] {
            let d = home.join(format!(".snapshots/{n}"));
            std::fs::create_dir_all(d.join("snapshot/me")).unwrap();
            std::fs::write(d.join("snapshot/me/doc.txt"), text).unwrap();
            let ft = std::fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1000 + if text == "old" { 0 } else { 500 }));
            std::fs::File::options().write(true).open(d.join("snapshot/me/doc.txt")).unwrap().set_times(ft).unwrap();
            std::fs::write(d.join("info.xml"), format!("<snapshot><num>{n}</num><date>{date}</date><description>timeline</description></snapshot>")).unwrap();
        }
        let v = versions(&home.join("me/doc.txt"));
        assert_eq!(v.len(), 2, "{v:?}");
        assert_eq!(v[0].snapshot, 3);
        assert_eq!(v[1].snapshot, 2);
        assert_eq!(v[0].date, crate::recents::parse_utc("2026-11-03T10:00:00").unwrap());
    }
}
