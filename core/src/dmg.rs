//! Apple disk images (.dmg) in Rust (no 7-Zip): the HFS+ or APFS partition
//! is decompressed once (dmgwiz) to grove's cache, then read as a volume
//! (hfsplus / apfs crates).

use crate::libarchive::Member;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// The decompressed volume, cached by the image's path, size and time.
fn volume_image(path: &Path) -> Result<PathBuf, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let key = format!("{}|{}|{}", path.display(), meta.len(), std::os::unix::fs::MetadataExt::mtime(&meta));
    let dir = crate::util::cache_home().join("grove/dmg");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let out = dir.join(format!("{}.img", crate::thumbs::md5_hex(&key)));
    if out.is_file() {
        return Ok(out);
    }
    // One image at a time (browsing and a preview can ask together).
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if out.is_file() {
        return Ok(out);
    }
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut wiz = dmgwiz::DmgWiz::from_reader(f, dmgwiz::Verbosity::None).map_err(|e| format!("This disk image can't be read: {e}"))?;
    // The volume: HFS+/HFSX or APFS (not the partition map, boot records...).
    let pick = wiz
        .partitions
        .iter()
        .position(|p| p.name.contains("Apple_HFS") || p.name.contains("Apple_APFS"))
        .or_else(|| (wiz.partitions.len() == 1).then_some(0))
        .ok_or_else(|| "This disk image has no Mac volume grove can read.".to_string())?;
    let part = out.with_extension("part");
    let w = BufWriter::new(std::fs::File::create(&part).map_err(|e| e.to_string())?);
    if let Err(e) = wiz.extract_partition(w, pick) {
        let _ = std::fs::remove_file(&part);
        return Err(format!("This disk image can't be read: {e}"));
    }
    std::fs::rename(&part, &out).map_err(|e| e.to_string())?;
    Ok(out)
}

enum Vol {
    Hfs(hfsplus::HfsVolume<std::fs::File>),
    Apfs(apfs::ApfsVolume<std::fs::File>),
}

fn open(path: &Path) -> Result<Vol, String> {
    let img = volume_image(path)?;
    let f = std::fs::File::open(&img).map_err(|e| e.to_string())?;
    if let Ok(v) = hfsplus::HfsVolume::open(f) {
        return Ok(Vol::Hfs(v));
    }
    let f = std::fs::File::open(&img).map_err(|e| e.to_string())?;
    apfs::ApfsVolume::open(f).map(Vol::Apfs).map_err(|e| format!("The volume in this disk image can't be read: {e}"))
}

fn clean(p: &str) -> String {
    p.trim_matches('/').to_string()
}

/// Seconds since 1904 (HFS+) to Unix time.
const HFS_EPOCH: i64 = 2_082_844_800;

/// Every file and folder (no symlinks).
pub fn list(path: &Path) -> Result<Vec<Member>, String> {
    let mut v = Vec::new();
    match open(path)? {
        Vol::Hfs(mut h) => {
            for w in h.walk().map_err(|e| e.to_string())? {
                let p = clean(&w.path);
                if p.is_empty() || w.entry.kind == hfsplus::EntryKind::Symlink {
                    continue;
                }
                let t = w.entry.modify_date as i64 - HFS_EPOCH;
                v.push(Member { path: p, size: w.entry.size, is_dir: w.entry.kind == hfsplus::EntryKind::Directory, mtime: t.max(0) });
            }
        }
        Vol::Apfs(mut a) => {
            for w in a.walk().map_err(|e| e.to_string())? {
                let p = clean(&w.path);
                if p.is_empty() || w.entry.kind == apfs::EntryKind::Symlink {
                    continue;
                }
                // APFS times are nanoseconds.
                let t = w.entry.modify_time / 1_000_000_000;
                v.push(Member { path: p, size: w.entry.size, is_dir: w.entry.kind == apfs::EntryKind::Directory, mtime: t.max(0) });
            }
        }
    }
    Ok(v)
}

fn copy_out(vol: &mut Vol, inner: &str, out: &mut dyn Write) -> Result<u64, String> {
    let p = format!("/{inner}");
    match vol {
        Vol::Hfs(h) => h.read_file_to(&p, out).map_err(|e| e.to_string()),
        Vol::Apfs(a) => {
            let mut out = out;
            a.read_file_to(&p, &mut out).map_err(|e| e.to_string())
        }
    }
}

/// Extract into `dest`: everything, or the files and folders named.
pub fn extract(path: &Path, dest: &Path, only: &[&str], cancel: &AtomicBool) -> Result<(), String> {
    let only: Vec<String> = only.iter().map(|s| clean(s)).collect();
    let members = list(path)?;
    let mut vol = open(path)?;
    for m in members.iter().filter(|m| only.is_empty() || only.iter().any(|o| m.path == *o || m.path.starts_with(&format!("{o}/")))) {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        if m.path.split('/').any(|c| c == "..") {
            continue;
        }
        let out = dest.join(&m.path);
        if m.is_dir {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(d) = out.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let mut w = BufWriter::new(std::fs::File::create(&out).map_err(|e| e.to_string())?);
        copy_out(&mut vol, &m.path, &mut w).map_err(|e| format!("Couldn't extract “{}”: {e}", m.path))?;
        w.flush().ok();
    }
    Ok(())
}

/// One file's bytes (up to `limit`).
pub fn read_member(path: &Path, name: &str, limit: usize, _cancel: &AtomicBool) -> Option<Vec<u8>> {
    let mut vol = open(path).ok()?;
    let mut b = Vec::new();
    copy_out(&mut vol, &clean(name), &mut b).ok()?;
    (b.len() <= limit).then_some(b)
}
