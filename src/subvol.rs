//! btrfs subvolumes from the window: File › New Subvolume, Take Snapshot,
//! Delete Subvolume (grove-core's btrfs ioctls; deleting goes through the
//! root helper when the disk doesn't let users delete subvolumes).

use grove_core::btrfs;
use serde_json::{json, Value};
use std::path::Path;

/// What the menus and the snapshot/delete sheets need to know about
/// `path` (a folder): {btrfs, subvol, kind, info, snapper, place,
/// snapshotName, needsRoot}.
pub fn describe(path: &str) -> Value {
    let p = Path::new(path);
    let on = btrfs::is_btrfs(p);
    let Some(info) = btrfs::info(p) else { return json!({"btrfs": on, "subvol": false}) };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    json!({
        "btrfs": on,
        "subvol": true,
        "kind": btrfs::kind(p),
        "info": info,
        "snapper": info.snapper,
        "place": btrfs::snapshot_place(p).to_string_lossy(),
        "snapshotName": btrfs::snapshot_name(p, now),
        "needsRoot": btrfs::delete_needs_root(p),
        // A snapshot: the subvolume it was taken of, if grove can see it
        // (Restore puts that back the way it was here).
        "source": btrfs::source_of(p).map(|s| s.to_string_lossy().into_owned()),
        "asideName": btrfs::source_of(p).map(|s| btrfs::aside_name(&s, now)),
    })
}

/// A new subvolume in `dir` ("" name: "untitled subvolume", numbered when
/// taken).
pub fn create(dir: &str, name: &str) -> Value {
    let d = Path::new(dir);
    if !btrfs::is_btrfs(d) {
        return json!({"error": "Subvolumes can only be made on btrfs disks, and this folder isn't on one."});
    }
    let base = d.join(if name.is_empty() { "untitled subvolume" } else { name });
    let p = if base.symlink_metadata().is_ok() { grove_core::ops::unique_name(&base) } else { base };
    let n = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match btrfs::create(d, &n) {
        Ok(p) => json!({"path": p.to_string_lossy()}),
        Err(e) => json!({"error": e}),
    }
}

/// Restores `target` from the snapshot `snap` (what's there now is kept
/// aside: {aside}).
pub fn restore(snap: &str, target: &str, aside: &str) -> Value {
    match btrfs::restore(Path::new(snap), Path::new(target), aside) {
        Ok(a) => json!({"ok": true, "aside": a.to_string_lossy()}),
        Err(e) => json!({"error": e}),
    }
}

pub fn snapshot(src: &str, dest: &str, name: &str, read_only: bool) -> Value {
    let dest = if dest.is_empty() { btrfs::snapshot_place(Path::new(src)) } else { dest.into() };
    match btrfs::snapshot(Path::new(src), &dest, name, read_only) {
        Ok(p) => json!({"path": p.to_string_lossy()}),
        Err(e) => json!({"error": e}),
    }
}

/// Deletes as the user when the disk allows it, otherwise as root through
/// `pkexec grove --root-helper subvol-delete` (polkit asks for the
/// password). Snapper's snapshots are left to snapper.
pub fn delete(path: &str) -> Value {
    let p = Path::new(path);
    if btrfs::is_snapper(p) {
        return json!({"error": "This is one of snapper's snapshots. Snapper keeps and removes those itself, so grove doesn't delete them."});
    }
    match btrfs::delete(p) {
        Ok(()) => json!({"ok": true}),
        Err(btrfs::DeleteError::Other(e)) => json!({"error": e}),
        Err(btrfs::DeleteError::NeedsRoot) => {
            let exe = crate::roothelper::exe();
            match std::process::Command::new("pkexec").arg(&exe).arg("--root-helper").arg("subvol-delete").arg(path).output() {
                Ok(o) if o.status.success() => json!({"ok": true, "elevated": true}),
                Ok(o) if o.status.code() == Some(126) || o.status.code() == Some(127) => {
                    json!({"error": "Not deleted: the administrator password wasn't given."})
                }
                Ok(o) => json!({"error": format!("Couldn't delete it: {}", String::from_utf8_lossy(&o.stderr).trim())}),
                Err(_) => json!({"error": "Deleting this needs administrator rights, and pkexec (polkit) isn't available."}),
            }
        }
    }
}
