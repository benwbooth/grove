//! Archives through libarchive's bsdtar (zip, tar.*, 7z, rar, iso...):
//! list them (browse as folders, Quick Look), extract, compress.

use crate::bridge::ffi::{self, QString};
use cxx_qt::CxxQtThread;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ArchEntry {
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
    pub mtime_text: String,
}

pub fn is_archive(mime: &str, name: &str) -> bool {
    let n = name.to_lowercase();
    matches!(
        mime,
        "application/zip" | "application/x-tar" | "application/x-compressed-tar" | "application/x-xz-compressed-tar" | "application/x-zstd-compressed-tar"
            | "application/x-bzip2-compressed-tar" | "application/x-7z-compressed" | "application/vnd.rar" | "application/x-rar" | "application/x-iso9660-image"
            | "application/x-cpio" | "application/vnd.debian.binary-package" | "application/x-rpm" | "application/java-archive" | "application/x-archive"
    ) || [".zip", ".tar", ".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.zst", ".tar.bz2", ".7z", ".rar", ".iso", ".cpio", ".jar"].iter().any(|e| n.ends_with(e))
}

/// Parse `bsdtar -tvf` (ls -l style) lines.
pub fn parse_listing(text: &str) -> Vec<ArchEntry> {
    let mut v = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 9 {
            continue;
        }
        // perms links owner group size month day time/year name...
        let Some(name_start) = nth_field_start(line, 8) else { continue };
        let mut name = line[name_start..].to_string();
        if let Some(i) = name.find(" -> ") {
            name.truncate(i);
        }
        if let Some(i) = name.find(" link to ") {
            name.truncate(i);
        }
        let is_dir = f[0].starts_with('d') || name.ends_with('/');
        v.push(ArchEntry { path: name.trim_end_matches('/').trim_start_matches("./").to_string(), size: f[4].parse().unwrap_or(0), is_dir, mtime_text: format!("{} {} {}", f[5], f[6], f[7]) });
    }
    v.retain(|e| !e.path.is_empty() && e.path != ".");
    v
}

fn nth_field_start(line: &str, n: usize) -> Option<usize> {
    let mut count = 0;
    let mut in_field = false;
    for (i, c) in line.char_indices() {
        if c.is_whitespace() {
            in_field = false;
        } else if !in_field {
            if count == n {
                return Some(i);
            }
            count += 1;
            in_field = true;
        }
    }
    None
}

pub fn list(path: &str) -> Result<Vec<ArchEntry>, String> {
    let o = std::process::Command::new("bsdtar").args(["-tvf", path]).output().map_err(|_| "Archives need bsdtar (libarchive), which isn't installed.".to_string())?;
    if !o.status.success() {
        return Err(format!("This archive can't be read: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    Ok(parse_listing(&String::from_utf8_lossy(&o.stdout)))
}

pub fn list_json(path: &str) -> Value {
    match list(path) {
        Ok(v) => json!({"entries": v}),
        Err(e) => json!({"error": e}),
    }
}

/// The items directly inside `inner` ("" = top) of an archive listing,
/// with folders that only appear as path prefixes filled in.
pub fn children(entries: &[ArchEntry], inner: &str) -> Vec<ArchEntry> {
    let prefix = if inner.is_empty() { String::new() } else { format!("{}/", inner.trim_end_matches('/')) };
    let mut out: BTreeMap<String, ArchEntry> = BTreeMap::new();
    for e in entries {
        let Some(rest) = e.path.strip_prefix(&prefix) else { continue };
        if rest.is_empty() {
            continue;
        }
        match rest.split_once('/') {
            Some((dir, _)) => {
                out.entry(dir.to_string()).or_insert(ArchEntry { path: format!("{prefix}{dir}"), size: 0, is_dir: true, mtime_text: String::new() });
            }
            None => {
                out.insert(rest.to_string(), e.clone());
            }
        }
    }
    out.into_values().collect()
}

fn job_msg(qt: &CxxQtThread<ffi::Grove>, id: i32, v: Value) {
    let s = v.to_string();
    let _ = qt.queue(move |g| g.job_changed(id, QString::from(s.as_str())));
}

fn stem(p: &Path) -> String {
    let n = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    grove_core::ops::split_name(&n, false).0
}

/// Extract next to the archive (or into `dest`): into a folder named after
/// it unless everything is already in one top folder (as Finder does).
pub fn extract_job(qt: CxxQtThread<ffi::Grove>, id: i32, archive: String, dest: String) {
    std::thread::spawn(move || {
        let a = PathBuf::from(&archive);
        let title = format!("Extracting “{}”", a.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        job_msg(&qt, id, json!({"id": id, "title": title, "state": "running", "indeterminate": true}));
        let dest_dir = if dest.is_empty() { a.parent().unwrap_or(Path::new("/")).to_path_buf() } else { PathBuf::from(&dest) };
        let entries = match list(&archive) {
            Ok(e) => e,
            Err(e) => return job_msg(&qt, id, json!({"id": id, "title": title, "state": "failed", "errors": [e]})),
        };
        let tops: HashSet<String> = entries.iter().map(|e| e.path.split('/').next().unwrap_or("").to_string()).collect();
        let single_top = tops.len() == 1;
        let (target, created) = if single_top {
            let t = dest_dir.join(tops.iter().next().unwrap());
            if t.exists() {
                // Clash: extract into "<archive name> 2/".
                let d = grove_core::ops::unique_name(&dest_dir.join(stem(&a)));
                (d.clone(), d)
            } else {
                (dest_dir.clone(), t)
            }
        } else {
            let base = dest_dir.join(stem(&a));
            let d = if base.exists() { grove_core::ops::unique_name(&base) } else { base };
            (d.clone(), d)
        };
        let _ = std::fs::create_dir_all(&target);
        let o = std::process::Command::new("bsdtar").arg("-xf").arg(&archive).arg("-C").arg(&target).output();
        match o {
            Ok(o) if o.status.success() => {
                crate::controller::push_history_any("Extract", grove_core::ops::Action::Exists { paths: vec![created.clone()] });
                job_msg(&qt, id, json!({"id": id, "title": title, "state": "done", "results": [created], "dest": dest_dir}));
            }
            Ok(o) => job_msg(&qt, id, json!({"id": id, "title": title, "state": "failed", "errors": [format!("Couldn't extract: {}", String::from_utf8_lossy(&o.stderr).trim())]})),
            Err(_) => job_msg(&qt, id, json!({"id": id, "title": title, "state": "failed", "errors": ["Archives need bsdtar (libarchive), which isn't installed."]})),
        }
        let s = serde_json::to_string(&vec![dest_dir.to_string_lossy()]).unwrap();
        let _ = qt.queue(move |g| g.folders_touched(QString::from(s.as_str())));
    });
}

/// Compress items into one archive next to them: "Archive.zip" (several)
/// or "<name>.zip"; format "zip" or "tar.zst".
pub fn compress_job(qt: CxxQtThread<ffi::Grove>, id: i32, paths: Vec<PathBuf>, format: String) {
    std::thread::spawn(move || {
        let Some(first) = paths.first() else { return };
        let dir = first.parent().unwrap_or(Path::new("/")).to_path_buf();
        let ext = if format == "tar.zst" { "tar.zst" } else { "zip" };
        let base = if paths.len() == 1 { first.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() } else { "Archive".into() };
        let mut out = dir.join(format!("{base}.{ext}"));
        if out.exists() {
            out = grove_core::ops::unique_name(&out);
        }
        let title = format!("Compressing into “{}”", out.file_name().unwrap().to_string_lossy());
        job_msg(&qt, id, json!({"id": id, "title": title, "state": "running", "indeterminate": true}));
        let mut cmd = std::process::Command::new("bsdtar");
        cmd.arg("-a").arg("-cf").arg(&out).arg("-C").arg(&dir);
        for p in &paths {
            if let Some(n) = p.file_name() {
                cmd.arg(n);
            }
        }
        match cmd.output() {
            Ok(o) if o.status.success() => {
                crate::controller::push_history_any("Compress", grove_core::ops::Action::Exists { paths: vec![out.clone()] });
                job_msg(&qt, id, json!({"id": id, "title": title, "state": "done", "results": [out], "dest": dir}));
            }
            Ok(o) => {
                let _ = std::fs::remove_file(&out);
                job_msg(&qt, id, json!({"id": id, "title": title, "state": "failed", "errors": [format!("Couldn't compress: {}", String::from_utf8_lossy(&o.stderr).trim())]}))
            }
            Err(_) => job_msg(&qt, id, json!({"id": id, "title": title, "state": "failed", "errors": ["Compressing needs bsdtar (libarchive), which isn't installed."]})),
        }
        let s = serde_json::to_string(&vec![dir.to_string_lossy()]).unwrap();
        let _ = qt.queue(move |g| g.folders_touched(QString::from(s.as_str())));
    });
}

/// Extract one member to grove's cache (to open or preview it).
pub fn extract_member(archive: &str, inner: &str) -> Result<PathBuf, String> {
    let dir = grove_core::util::cache_home().join("grove/archives").join(grove_core::thumbs::md5_hex(archive));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let o = std::process::Command::new("bsdtar").arg("-xf").arg(archive).arg("-C").arg(&dir).arg(inner).output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(format!("Couldn't take “{inner}” out of the archive: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    Ok(dir.join(inner))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_bsdtar() {
        let t = "drwxr-xr-x  0 ben    users       0 Jan  5 12:00 dir/\n-rw-r--r--  0 ben    users    1234 Jan  5  2020 dir/a file.txt\n-rw-r--r--  0 0      0          5 Mar  1 09:00 top.md\n";
        let e = parse_listing(t);
        assert_eq!(e.len(), 3);
        assert_eq!(e[1].path, "dir/a file.txt");
        assert_eq!(e[1].size, 1234);
        assert!(e[0].is_dir);
        let top = children(&e, "");
        assert_eq!(top.iter().map(|x| x.path.as_str()).collect::<Vec<_>>(), vec!["dir", "top.md"]);
        let inner = children(&e, "dir");
        assert_eq!(inner.len(), 1);
        // Folders implied only by paths.
        let implied = parse_listing("-rw-r--r--  0 a b 1 Jan 1 2020 x/y/z.txt\n");
        assert_eq!(children(&implied, "")[0].path, "x");
        assert!(children(&implied, "x")[0].is_dir);
    }
}
