//! Show All Subvolumes: the top of a btrfs disk (subvolid=5), where `@`,
//! `@home`, `@snapshots` and the like live. It's normally mounted nowhere
//! (the system mounts `@` on / and `@home` on /home), so grove mounts it,
//! read-only unless asked otherwise, and opens it:
//! - a mount of the top that's already there (e.g. /mnt/stuff) is just opened;
//! - a disk not mounted at all goes through udisks (`subvolid=5`, no password
//!   for your own removable disks);
//! - a disk in use (udisks won't mount a device twice) needs root: grove's
//!   root helper (`pkexec grove --root-helper btrfs-top`) mounts it on
//!   `$XDG_RUNTIME_DIR/grove/NAME (All Subvolumes)` and stays running, so
//!   Eject unmounts it without asking again, and quitting grove (or grove
//!   crashing) lets it go.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Stdio};
use std::sync::Mutex;

enum Held {
    /// The root helper holding the mount: its stdin takes "unmount".
    Helper { child: Child, out: BufReader<ChildStdout> },
    /// Mounted through udisks (a disk that wasn't mounted).
    Udisks,
}

struct Mounted {
    name: String,
    held: Held,
}

/// Grove's own mounts of disks' tops, by mount point.
static HELD: Mutex<Option<HashMap<String, Mounted>>> = Mutex::new(None);

fn with_held<R>(f: impl FnOnce(&mut HashMap<String, Mounted>) -> R) -> R {
    let mut g = HELD.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(HashMap::new))
}

/// The sidebar's name for one of grove's top mounts ("Data (All Subvolumes)").
pub fn name_of_mount(mount_point: &str) -> Option<String> {
    with_held(|h| h.get(mount_point).map(|m| m.name.clone()))
}

fn same_device(a: &str, b: &str) -> bool {
    a == b || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

/// The disk's name from /dev/disk/by-label, if it has one.
fn label_of(device: &str) -> Option<String> {
    for e in std::fs::read_dir("/dev/disk/by-label").ok()?.flatten() {
        if e.path().to_str().map(|p| same_device(p, device)).unwrap_or(false) {
            // udev escapes characters as \xNN.
            let raw = e.file_name().to_string_lossy().into_owned();
            let mut out = Vec::new();
            let b = raw.as_bytes();
            let mut i = 0;
            while i < b.len() {
                if b[i] == b'\\' && i + 3 < b.len() && b[i + 1] == b'x' {
                    if let Ok(v) = u8::from_str_radix(&raw[i + 2..i + 4], 16) {
                        out.push(v);
                        i += 4;
                        continue;
                    }
                }
                out.push(b[i]);
                i += 1;
            }
            return Some(String::from_utf8_lossy(&out).into_owned());
        }
    }
    None
}

/// The btrfs disk `spec` means: a folder on it, or "mount:/dev/X" / "/dev/X"
/// for a disk not mounted. {device, name, mounted, existing, needsRoot} or
/// {error}.
struct Disk {
    device: String,
    name: String,
    mounted: bool,
    /// A mount of its top that's already there.
    existing: Option<String>,
}

fn resolve(spec: &str) -> Result<Disk, String> {
    let mounts = grove_core::mounts::mounts();
    let device = if let Some(d) = spec.strip_prefix("mount:").or_else(|| spec.starts_with("/dev/").then_some(spec)) {
        let d = d.to_string();
        let mounted_fs = mounts.iter().find(|m| same_device(&m.device, &d)).map(|m| m.fs_type.clone());
        let fs = match mounted_fs {
            Some(t) => t,
            None => crate::places::fs_type_of(&d),
        };
        if fs != "btrfs" {
            return Err("That disk isn't btrfs, so it has no subvolumes.".into());
        }
        d
    } else {
        // The mount holding the folder (the longest mount point above it).
        let p = std::fs::canonicalize(spec).unwrap_or_else(|_| PathBuf::from(spec));
        let m = mounts.iter().filter(|m| p.starts_with(&m.mount_point)).max_by_key(|m| m.mount_point.len()).ok_or("Grove can't tell which disk that is.")?;
        if m.fs_type != "btrfs" {
            return Err("That folder isn't on a btrfs disk, so there are no subvolumes to show.".into());
        }
        m.device.clone()
    };
    let name = label_of(&device).filter(|l| !l.is_empty()).unwrap_or_else(|| {
        if same_device(&device, mounts.iter().find(|m| m.mount_point == "/").map(|m| m.device.as_str()).unwrap_or("")) {
            crate::controller::os_name().to_string()
        } else {
            Path::new(&device).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| device.clone())
        }
    });
    let ours: Vec<String> = mounts.iter().filter(|m| same_device(&m.device, &device)).map(|m| m.mount_point.clone()).collect();
    let existing = mounts.iter().find(|m| m.fs_type == "btrfs" && m.root == "/" && same_device(&m.device, &device)).map(|m| m.mount_point.clone());
    Ok(Disk { device, name, mounted: !ours.is_empty(), existing })
}

/// What the Show All Subvolumes sheet says before anything is mounted.
pub fn describe(spec: &str) -> Value {
    match resolve(spec) {
        Ok(d) => json!({"device": d.device, "name": d.name, "mounted": d.mounted, "existing": d.existing,
                        "needsRoot": d.existing.is_none() && d.mounted}),
        Err(e) => json!({"error": e}),
    }
}

fn runtime_grove() -> PathBuf {
    std::env::var("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() }))).join("grove")
}

/// A new, empty folder for the mount: "NAME (All Subvolumes)", numbered
/// when that's taken.
fn new_mount_dir(name: &str) -> Result<PathBuf, String> {
    let base = runtime_grove();
    std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let safe: String = name.chars().map(|c| if c == '/' { '_' } else { c }).collect();
    for n in 1..100 {
        let p = base.join(if n == 1 { format!("{safe} (All Subvolumes)") } else { format!("{safe} (All Subvolumes) {n}") });
        match std::fs::create_dir(&p) {
            Ok(()) => return Ok(p),
            // An empty leftover from before: reuse it.
            Err(_) if std::fs::read_dir(&p).map(|mut d| d.next().is_none()).unwrap_or(false) && !grove_core::mounts::mounts().iter().any(|m| Path::new(&m.mount_point) == p) => return Ok(p),
            Err(_) => continue,
        }
    }
    Err("Couldn't make a folder to open it in.".into())
}

/// Mounts the top of the disk `spec` names and answers {path} (or {error}).
pub fn open(spec: &str, writable: bool) -> Value {
    let d = match resolve(spec) {
        Ok(d) => d,
        Err(e) => return json!({"error": e}),
    };
    if let Some(p) = d.existing {
        return json!({"path": p, "existing": true});
    }
    let shown = format!("{} (All Subvolumes)", d.name);
    if !d.mounted {
        return match crate::places::mount_with_options(&d.device, if writable { "subvolid=5" } else { "ro,subvolid=5" }) {
            Ok(p) => {
                with_held(|h| h.insert(p.clone(), Mounted { name: shown, held: Held::Udisks }));
                json!({"path": p})
            }
            Err(e) => json!({"error": format!("Couldn't open the disk's subvolumes: {e}")}),
        };
    }
    let dir = match new_mount_dir(&d.name) {
        Ok(p) => p,
        Err(e) => return json!({"error": e}),
    };
    let mp = dir.to_string_lossy().into_owned();
    let args = ["btrfs-top", d.device.as_str(), mp.as_str(), if writable { "rw" } else { "ro" }];
    // Through pkexec, like every root-helper call (GROVE_PKEXEC: tests run it
    // through something else, e.g. `env`, so the mount is refused).
    let mut cmd = crate::roothelper::command(&args);
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => {
            let _ = std::fs::remove_dir(&dir);
            return json!({"error": "This disk is in use, so opening its top needs administrator rights, and pkexec (polkit) isn't available."});
        }
    };
    let mut out = BufReader::new(child.stdout.take().expect("piped"));
    let mut line = String::new();
    let _ = out.read_line(&mut line);
    if line.trim() == "mounted" {
        with_held(|h| h.insert(mp.clone(), Mounted { name: shown, held: Held::Helper { child, out } }));
        return json!({"path": mp, "elevated": true});
    }
    drop(child.stdin.take());
    let o = child.wait_with_output();
    let _ = std::fs::remove_dir(&dir);
    match o.map(|o| crate::roothelper::read_exit(o.status.code(), &o.stdout, &o.stderr)) {
        Ok(Err(crate::roothelper::AdminError::Cancelled)) => json!({"error": "Not opened: the administrator password wasn't given."}),
        Ok(Err(crate::roothelper::AdminError::Failed(m))) => json!({"error": format!("Couldn't open the disk's subvolumes: {m}")}),
        Ok(_) => json!({"error": "Couldn't open the disk's subvolumes: the helper stopped."}),
        Err(e) => json!({"error": e.to_string()}),
    }
}

/// Eject for one of grove's top mounts held by the root helper (None:
/// not one of those; the usual eject handles it).
pub fn eject(mount_point: &str) -> Option<Value> {
    let mut m = with_held(|h| h.remove(mount_point))?;
    let Held::Helper { child, out } = &mut m.held else { return None };
    let sent = child.stdin.as_mut().map(|i| i.write_all(b"unmount\n").and_then(|_| i.flush()).is_ok()).unwrap_or(false);
    let mut line = String::new();
    if sent {
        let _ = out.read_line(&mut line);
    }
    let line = line.trim().to_string();
    if line == "ok" || !sent || line.is_empty() {
        // Done (or the helper is gone, and its mount with it).
        drop(child.stdin.take());
        let _ = child.wait();
        return Some(json!({"ok": true}));
    }
    with_held(|h| h.insert(mount_point.to_string(), m));
    Some(json!({"error": "Couldn't eject: it's still in use by a program. Close files on it and try again."}))
}

/// Grove is quitting: let go of every top it mounted.
pub fn close_all() {
    let all: Vec<(String, Mounted)> = with_held(|h| h.drain().collect());
    for (mp, mut m) in all {
        match &mut m.held {
            Held::Helper { child, .. } => {
                // Its stdin closing tells it to detach the mount.
                drop(child.stdin.take());
                for _ in 0..20 {
                    if matches!(child.try_wait(), Ok(Some(_))) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
            Held::Udisks => {
                let _ = crate::places::eject(&mp);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_btrfs() {
        // /proc is never btrfs.
        assert!(describe("/proc").get("error").is_some());
    }
}
