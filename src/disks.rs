//! Grove Disk Utility's link to the system: disks and partitions from
//! UDisks2 (D-Bus), and the pending operations carried out through it.
//! UDisks2 runs libblockdev (libparted/libfdisk plus each filesystem's own
//! tools: mkfs.*, resize2fs, ntfsresize, btrfs, e2fsck...), the same
//! backend GParted drives itself, and polkit decides who may change what,
//! so no root process of grove's runs for most steps. Moving a partition
//! copies its data through a device UDisks2 opens for grove
//! (grove_core::diskops); FAT is resized by libparted as root through
//! `pkexec grove --root-helper fat-resize`.
//!
//! Safety: the disks holding the running system (/, /home, /boot, /nix,
//! active swap) are marked `system` and refused unless the user unlocked
//! them; GROVE_DISK_SANDBOX=DIR limits everything to loop devices backed
//! by image files in DIR (tests), and screenshot runs without it see no
//! disks at all.

use grove_core::partmap::{self, Caps, Disk, FsCaps, Kind, Op, Part, Queue, MIB};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::OnceLock;
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value as ZValue};

const UD: &str = "org.freedesktop.UDisks2";
const MGR: &str = "/org/freedesktop/UDisks2/Manager";

type Managed = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;
type Opts = HashMap<String, ZValue<'static>>;

fn conn() -> Result<Connection, String> {
    Connection::system().map_err(|_| "Can't reach the disk service (udisks).".to_string())
}

fn objects(c: &Connection) -> Result<Managed, String> {
    let r = c
        .call_method(Some(UD), "/org/freedesktop/UDisks2", Some("org.freedesktop.DBus.ObjectManager"), "GetManagedObjects", &())
        .map_err(|e| plain(&e))?;
    r.body().deserialize::<Managed>().map_err(|e| e.to_string())
}

fn v<T: TryFrom<OwnedValue>>(m: &HashMap<String, OwnedValue>, k: &str) -> Option<T> {
    m.get(k).and_then(|x| x.try_clone().ok()).and_then(|x| T::try_from(x).ok())
}
fn s(m: &HashMap<String, OwnedValue>, k: &str) -> String {
    v::<String>(m, k).unwrap_or_default()
}
fn bytes(m: &HashMap<String, OwnedValue>, k: &str) -> String {
    let b: Vec<u8> = v(m, k).unwrap_or_default();
    String::from_utf8_lossy(&b).trim_end_matches('\0').to_string()
}
fn u(m: &HashMap<String, OwnedValue>, k: &str) -> u64 {
    v::<u64>(m, k).unwrap_or(0)
}
fn b(m: &HashMap<String, OwnedValue>, k: &str) -> bool {
    v::<bool>(m, k).unwrap_or(false)
}
fn obj(m: &HashMap<String, OwnedValue>, k: &str) -> String {
    v::<OwnedObjectPath>(m, k).map(|p| p.to_string()).unwrap_or_default()
}
fn mounts(m: &HashMap<String, OwnedValue>) -> Vec<String> {
    let l: Vec<Vec<u8>> = v(m, "MountPoints").unwrap_or_default();
    l.into_iter().map(|x| String::from_utf8_lossy(&x).trim_end_matches('\0').to_string()).collect()
}

/// D-Bus errors in plain words.
pub fn plain(e: &zbus::Error) -> String {
    let t = e.to_string();
    let t = t.split_once(": ").map(|(a, b)| if a.contains("org.freedesktop") || a.contains("MethodError") { b.to_string() } else { t.clone() }).unwrap_or(t);
    let t = t.trim_start_matches("Error ").to_string();
    if t.contains("NotAuthorized") || t.contains("not authorized") {
        "You don't have permission (the password was refused or cancelled).".into()
    } else if t.contains("target is busy") || t.contains("is busy") {
        "It's still in use by a program. Close files on it and try again.".into()
    } else {
        t
    }
}

fn opts(pairs: &[(&str, ZValue<'static>)]) -> Opts {
    pairs.iter().map(|(k, v)| (k.to_string(), v.try_clone().unwrap())).collect()
}

fn call<B>(c: &Connection, path: &str, iface: &str, method: &str, body: &B) -> Result<zbus::message::Message, String>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    let iface = format!("org.freedesktop.UDisks2.{iface}");
    c.call_method(Some(UD), path, Some(iface.as_str()), method, body).map_err(|e| plain(&e))
}

// ---- safety ----

/// Image files under this folder are the only disks tests may see.
pub fn sandbox() -> Option<String> {
    std::env::var("GROVE_DISK_SANDBOX").ok().filter(|s| !s.is_empty())
}

fn in_sandbox(loop_file: &str) -> bool {
    match sandbox() {
        Some(dir) => !loop_file.is_empty() && Path::new(loop_file).starts_with(&dir),
        None => std::env::var_os("GROVE_SCREENSHOT").is_none(),
    }
}

// ---- what each filesystem allows ----

static CAPS: OnceLock<Caps> = OnceLock::new();

/// What's known so far without asking udisks (the UI thread's view).
pub fn caps_now() -> Caps {
    CAPS.get().cloned().unwrap_or_default()
}

pub fn caps() -> Caps {
    CAPS.get_or_init(|| {
        let mut out = Caps::new();
        let Ok(c) = conn() else { return out };
        for (fs, _, _) in partmap::FILESYSTEMS {
            let mut fc = FsCaps { label: true, ..Default::default() };
            if *fs == "empty" {
                fc.format = true;
                out.insert(fs.to_string(), fc);
                continue;
            }
            if let Ok(r) = call(&c, MGR, "Manager", "CanFormat", &(fs,)) {
                if let Ok((ok, need)) = r.body().deserialize::<(bool, String)>() {
                    fc.format = ok;
                    fc.missing = need;
                }
            }
            if let Ok(r) = call(&c, MGR, "Manager", "CanResize", &(fs,)) {
                if let Ok((ok, mode, _need)) = r.body().deserialize::<(bool, u64, String)>() {
                    // libblockdev's flags: 2 offline shrink, 4 offline grow,
                    // 8 online shrink, 16 online grow.
                    if ok {
                        fc.shrink = mode & (2 | 8) != 0;
                        fc.grow = mode & (4 | 16) != 0;
                        fc.online_only = mode & (2 | 4) == 0;
                    }
                }
            }
            for (m, f) in [("CanCheck", &mut fc.check), ("CanRepair", &mut fc.repair)] {
                if let Ok(r) = call(&c, MGR, "Manager", m, &(fs,)) {
                    *f = r.body().deserialize::<(bool, String)>().map(|x| x.0).unwrap_or(false);
                }
            }
            match *fs {
                // FAT: libparted's resizer (as root), as GParted does.
                "vfat" => {
                    fc.shrink = true;
                    fc.grow = true;
                    fc.online_only = false;
                }
                // Swap holds nothing to keep: resized by making it again.
                "swap" => {
                    fc.shrink = true;
                    fc.grow = true;
                }
                "udf" => fc.label = false,
                _ => {}
            }
            out.insert(fs.to_string(), fc);
        }
        out
    })
    .clone()
}

// ---- reading disks ----

fn kname(dev: &str) -> String {
    let real = std::fs::canonicalize(dev).unwrap_or_else(|_| dev.into());
    real.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn has_holders(dev: &str) -> bool {
    std::fs::read_dir(Path::new("/sys/class/block").join(kname(dev)).join("holders")).map(|mut r| r.next().is_some()).unwrap_or(false)
}

fn statvfs_used(path: &str) -> Option<u64> {
    let c = std::ffi::CString::new(path).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    Some((st.f_blocks as u64).saturating_sub(st.f_bfree as u64) * st.f_frsize as u64)
}

/// Every disk udisks knows (in the sandbox, only its image files).
pub fn disks() -> Result<Vec<Disk>, String> {
    let c = conn()?;
    let objs = objects(&c)?;
    let system = partmap::system_disks();
    let mut out = vec![];
    for (path, ifaces) in &objs {
        let Some(block) = ifaces.get("org.freedesktop.UDisks2.Block") else { continue };
        if ifaces.contains_key("org.freedesktop.UDisks2.Partition") {
            continue;
        }
        let size = u(block, "Size");
        let device = bytes(block, "Device");
        let name = kname(&device);
        // Whole disks only: no zram, device-mapper, RAID arrays or empty
        // loop devices.
        if size == 0 || name.starts_with("zram") || name.starts_with("dm-") || name.starts_with("md") || name.starts_with("sr") || !matches!(obj(block, "CryptoBackingDevice").as_str(), "" | "/") {
            continue;
        }
        let loop_file = ifaces.get("org.freedesktop.UDisks2.Loop").map(|l| bytes(l, "BackingFile")).unwrap_or_default();
        if name.starts_with("loop") && loop_file.is_empty() {
            continue;
        }
        if !in_sandbox(&loop_file) && (sandbox().is_some() || std::env::var_os("GROVE_SCREENSHOT").is_some()) {
            continue;
        }
        let drive = objs.iter().find(|(p, _)| p.to_string() == obj(block, "Drive")).and_then(|(_, i)| i.get("org.freedesktop.UDisks2.Drive"));
        let model = match drive {
            Some(d) => format!("{} {}", s(d, "Vendor"), s(d, "Model")).trim().to_string(),
            None if !loop_file.is_empty() => format!("Disk image {}", Path::new(&loop_file).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
            None => String::new(),
        };
        let table = ifaces.get("org.freedesktop.UDisks2.PartitionTable").map(|t| s(t, "Type")).unwrap_or_default();
        let mut d = Disk {
            id: path.to_string(),
            device: device.clone(),
            model,
            size,
            sector: std::fs::read_to_string(Path::new("/sys/class/block").join(&name).join("queue/logical_block_size")).ok().and_then(|x| x.trim().parse().ok()).unwrap_or(512),
            table: table.clone(),
            // Tests: GROVE_DISK_TEST_SYSTEM=loopN plays the system's disk
            // (only inside the sandbox).
            system: system.contains(&name) || (sandbox().is_some() && std::env::var("GROVE_DISK_TEST_SYSTEM").ok().as_deref() == Some(name.as_str())),
            removable: drive.map(|d| b(d, "Removable") || b(d, "MediaRemovable") || b(d, "Ejectable")).unwrap_or(false),
            read_only: b(block, "ReadOnly"),
            loop_file,
            connection: drive.map(|d| s(d, "ConnectionBus")).unwrap_or_default(),
            ..Default::default()
        };
        if table.is_empty() && !s(block, "IdType").is_empty() {
            d.whole_fs = s(block, "IdType");
            d.whole_label = s(block, "IdLabel");
        }
        for (pp, pi) in &objs {
            let (Some(part), Some(pb)) = (pi.get("org.freedesktop.UDisks2.Partition"), pi.get("org.freedesktop.UDisks2.Block")) else { continue };
            if obj(part, "Table") != path.as_str() {
                continue;
            }
            let pdev = bytes(pb, "Device");
            let fsm = pi.get("org.freedesktop.UDisks2.Filesystem").map(mounts).unwrap_or_default();
            let swap_on = pi.get("org.freedesktop.UDisks2.Swapspace").map(|sw| b(sw, "Active")).unwrap_or(false);
            let unlocked = objs.values().any(|i| i.get("org.freedesktop.UDisks2.Block").map(|x| obj(x, "CryptoBackingDevice") == pp.as_str()).unwrap_or(false));
            let kind = if b(part, "IsContainer") { Kind::Extended } else if b(part, "IsContained") { Kind::Logical } else { Kind::Primary };
            let mut p = Part {
                id: pp.to_string(),
                number: v::<u32>(part, "Number").unwrap_or(0),
                offset: u(part, "Offset"),
                size: u(part, "Size"),
                kind,
                fs: if kind == Kind::Extended { String::new() } else { s(pb, "IdType") },
                label: s(pb, "IdLabel"),
                name: s(part, "Name"),
                uuid: s(pb, "IdUUID"),
                ptype: s(part, "Type"),
                attrs: u(part, "Flags"),
                busy: !fsm.is_empty() || swap_on || unlocked || has_holders(&pdev),
                mounts: fsm.clone(),
                device: pdev,
                ..Default::default()
            };
            if let Some(m) = fsm.first() {
                p.used = statvfs_used(m);
                if p.fs == "btrfs" {
                    if let Some((sv, sn)) = grove_core::diskops::btrfs_counts(&fsm) {
                        p.subvols = Some(sv);
                        p.snapshots = Some(sn);
                    }
                }
            }
            if p.fs == "swap" {
                p.used = Some(0);
            }
            d.parts.push(p);
        }
        d.parts.sort_by_key(|p| (p.offset, p.number));
        // How full unmounted filesystems are, where no password is needed.
        for i in 0..d.parts.len() {
            let p = d.parts[i].clone();
            if p.used.is_none() && !p.busy && !p.fs.is_empty() {
                d.parts[i].used = usage_of(&c, &d, &p, &p.fs, false);
            }
        }
        out.push(d);
    }
    out.sort_by(|a, b| (a.loop_file.is_empty() == false, a.device.clone()).cmp(&(b.loop_file.is_empty() == false, b.device.clone())));
    Ok(out)
}

pub fn disk(id: &str) -> Result<Disk, String> {
    disks()?.into_iter().find(|d| d.id == id).ok_or_else(|| "That disk isn't connected any more.".to_string())
}

fn fd_file(r: zbus::message::Message) -> Result<File, String> {
    let fd: zbus::zvariant::OwnedFd = r.body().deserialize().map_err(|e| e.to_string())?;
    let fd: std::os::fd::OwnedFd = fd.into();
    Ok(File::from(fd))
}

/// How much of an unmounted filesystem is in use (its superblock, read
/// through udisks: polkit may ask for the password on internal disks).
pub fn usage(part_id: &str, fs: &str) -> Option<u64> {
    usage_with(&conn().ok()?, part_id, fs, true)
}

/// `ask`: false never asks for a password (where polkit would, it's
/// simply not known).
fn usage_with(c: &Connection, part_id: &str, fs: &str, ask: bool) -> Option<u64> {
    if !matches!(fs, "ext2" | "ext3" | "ext4" | "btrfs" | "xfs" | "vfat") {
        return None;
    }
    let (d, p) = disks().ok()?.into_iter().find_map(|d| d.parts.iter().find(|p| p.id == part_id).cloned().map(|p| (d, p)))?;
    usage_of(c, &d, &p, fs, ask)
}

fn usage_of(c: &Connection, d: &Disk, p: &Part, fs: &str, ask: bool) -> Option<u64> {
    if !matches!(fs, "ext2" | "ext3" | "ext4" | "btrfs" | "xfs" | "vfat") {
        return None;
    }
    let (f, at) = open_part_read(c, d, p, ask).ok()?;
    let mut head = vec![0u8; if fs == "vfat" { 0x100000 } else { 0x20000 }];
    use std::os::unix::fs::FileExt;
    let n = f.read_at(&mut head, at).ok()?;
    head.truncate(n);
    partmap::used_from_superblock(fs, &head)
}

// ---- carrying out the queue ----

pub struct Progress<'a> {
    pub step: usize,
    pub steps: usize,
    pub report: &'a mut dyn FnMut(usize, usize, f64, &str),
}

impl Progress<'_> {
    fn say(&mut self, frac: f64, msg: &str) {
        (self.report)(self.step, self.steps, frac, msg);
    }
}

/// Wait until udisks shows a partition at `offset` (after a move or a new
/// table) and return its object.
fn wait_part(c: &Connection, disk_id: &str, offset: u64) -> Result<String, String> {
    for _ in 0..60 {
        if let Ok(objs) = objects(c) {
            for (p, i) in &objs {
                if let Some(part) = i.get("org.freedesktop.UDisks2.Partition") {
                    if obj(part, "Table") == disk_id && u(part, "Offset") == offset {
                        return Ok(p.to_string());
                    }
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    Err("The system didn't pick up the new partition table. Unplug the disk and plug it in again, or restart.".into())
}

fn rescan(c: &Connection, id: &str) {
    let _ = call(c, id, "Block", "Rescan", &(opts(&[]),));
}

fn is_linux_fs(fs: &str) -> bool {
    matches!(fs, "ext2" | "ext3" | "ext4" | "btrfs" | "xfs" | "f2fs")
}

fn format_opts(fs: &str, label: &str, update_type: bool) -> Opts {
    let mut o = opts(&[]);
    if !label.is_empty() && fs != "empty" {
        o.insert("label".into(), ZValue::from(label.to_string()));
    }
    if update_type {
        o.insert("update-partition-type".into(), ZValue::from(true));
    }
    // The new filesystem's top folder belongs to the user (Linux ones).
    if is_linux_fs(fs) {
        o.insert("take-ownership".into(), ZValue::from(true));
    }
    o
}

fn format(c: &Connection, id: &str, fs: &str, label: &str, update_type: bool) -> Result<(), String> {
    let fs = if fs.is_empty() { "empty" } else { fs };
    call(c, id, "Block", "Format", &(fs, format_opts(fs, label, update_type))).map(|_| ())
}

/// Resize a filesystem to `size` bytes (0: fill its partition).
fn fs_resize(c: &Connection, d: &Disk, p: &Part, size: u64, caps: &Caps) -> Result<(), String> {
    let cap = caps.get(&p.fs).cloned().unwrap_or_default();
    if p.fs == "swap" {
        return format(c, &p.id, "swap", &p.label, false);
    }
    if cap.online_only {
        // btrfs: mounted for the resize only, as GParted does.
        let r = call(c, &p.id, "Filesystem", "Mount", &(opts(&[]),));
        let r = r.and_then(|_| call(c, &p.id, "Filesystem", "Resize", &(size, opts(&[])))).map(|_| ());
        let _ = call(c, &p.id, "Filesystem", "Unmount", &(opts(&[]),));
        return r;
    }
    let _ = d;
    call(c, &p.id, "Filesystem", "Resize", &(size, opts(&[]))).map(|_| ())
}

/// FAT: libparted resizes the filesystem and its partition together, as
/// root (pkexec grove --root-helper); in the sandbox, on the image file.
fn fat_resize(c: &Connection, d: &Disk, p: &Part, size: u64, unlocked: bool) -> Result<(), String> {
    let ss = 512;
    let (start, end) = ((p.offset / ss) as i64, ((p.offset + size) / ss - 1) as i64);
    if sandbox().is_some() && !d.loop_file.is_empty() {
        grove_core::diskops::fat_resize(&d.loop_file, p.number, start, end)?;
        return reattach(c, d);
    }
    let (n, st, en) = (p.number.to_string(), start.to_string(), end.to_string());
    match crate::roothelper::as_root(&["fat-resize", &d.device, &n, &st, &en, if unlocked { "1" } else { "0" }]) {
        Ok(_) => {
            rescan(c, &d.id);
            Ok(())
        }
        Err(crate::roothelper::AdminError::Cancelled) => Err("Resizing FAT needs the administrator password; it was refused or cancelled.".into()),
        Err(crate::roothelper::AdminError::Unavailable) => Err("Couldn't ask for permission: pkexec (polkit) isn't available.".into()),
        Err(crate::roothelper::AdminError::Failed(m)) => Err(m),
    }
}

// ---- raw access: the device through udisks (polkit asks), or in the
// sandbox the image file itself (tests can't type a password; the same
// grove_core::diskops code runs on it) ----

fn sandbox_file(d: &Disk) -> Option<&str> {
    (sandbox().is_some() && !d.loop_file.is_empty() && in_sandbox(&d.loop_file)).then_some(d.loop_file.as_str())
}

/// The whole disk, open for reading and writing.
fn open_disk_rw(c: &Connection, d: &Disk) -> Result<File, String> {
    if let Some(f) = sandbox_file(d) {
        return std::fs::OpenOptions::new().read(true).write(true).open(f).map_err(|e| e.to_string());
    }
    let r = call(c, &d.id, "Block", "OpenDevice", &("rw", opts(&[("flags", ZValue::from(libc::O_EXCL))])))
        .or_else(|_| call(c, &d.id, "Block", "OpenDevice", &("rw", opts(&[]))))?;
    fd_file(r)
}

/// A partition to read (and where in the file it starts).
fn open_part_read(c: &Connection, d: &Disk, p: &Part, ask: bool) -> Result<(File, u64), String> {
    if let Some(f) = sandbox_file(d) {
        return Ok((File::open(f).map_err(|e| e.to_string())?, p.offset));
    }
    let o = if ask { opts(&[]) } else { opts(&[("auth.no_user_interaction", ZValue::from(true))]) };
    Ok((fd_file(call(c, &p.id, "Block", "OpenForBackup", &(o,))?)?, 0))
}

/// A partition to write over (paste).
fn open_part_write(c: &Connection, d: &Disk, part_id: &str, offset: u64) -> Result<(File, u64), String> {
    if let Some(f) = sandbox_file(d) {
        return Ok((std::fs::OpenOptions::new().write(true).open(f).map_err(|e| e.to_string())?, offset));
    }
    Ok((fd_file(call(c, part_id, "Block", "OpenForRestore", &(opts(&[]),))?)?, 0))
}

/// After the image file was written directly: detach the loop device and
/// attach it again, so nothing stale stays in the kernel's caches.
fn reattach(c: &Connection, d: &Disk) -> Result<(), String> {
    let Some(file) = sandbox_file(d) else {
        rescan(c, &d.id);
        return Ok(());
    };
    call(c, &d.id, "Loop", "Delete", &(opts(&[]),))?;
    let f = std::fs::OpenOptions::new().read(true).write(true).open(file).map_err(|e| e.to_string())?;
    let r = call(c, MGR, "Manager", "LoopSetup", &(zbus::zvariant::Fd::from(&f), opts(&[])))?;
    let new: OwnedObjectPath = r.body().deserialize().map_err(|e| e.to_string())?;
    if new.as_str() != d.id {
        return Err(format!("The disk image came back as {} instead of {}; look again (F5).", new.as_str(), d.id));
    }
    // udisks reads its partitions again.
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(250));
        if let Ok(objs) = objects(c) {
            let n = objs.values().filter(|i| i.get("org.freedesktop.UDisks2.Partition").map(|p| obj(p, "Table") == d.id).unwrap_or(false)).count();
            if n > 0 || d.table.is_empty() {
                std::thread::sleep(std::time::Duration::from_millis(500));
                return Ok(());
            }
        }
    }
    Ok(())
}

fn copy_progress<'a, 'b: 'a>(pr: &'a mut Progress<'b>, what: &'a str) -> Box<dyn FnMut(u64, u64) + 'a> {
    let mut last = std::time::Instant::now();
    Box::new(move |done, total| {
        if last.elapsed().as_millis() > 250 || done == total {
            last = std::time::Instant::now();
            pr.say(done as f64 / total.max(1) as f64, &format!("{what} {} of {}", grove_core::util::human_size(done), grove_core::util::human_size(total)));
        }
    })
}

/// Carry out one disk's queue. `base` is the disk as the user saw it when
/// the steps were queued: if it changed since, nothing is done.
pub fn apply(base: &Disk, queue: &Queue, unlocked: bool, cancel: &AtomicBool, report: &mut dyn FnMut(usize, usize, f64, &str)) -> Result<(), String> {
    let caps = caps();
    let c = conn()?;
    let now = disk(&base.id)?;
    // Safety first: the same disk, still as it was, allowed.
    if now.device != base.device || now.size != base.size || now.table != base.table || now.parts.len() != base.parts.len() || now.parts.iter().zip(&base.parts).any(|(a, b)| a.id != b.id || a.offset != b.offset || a.size != b.size) {
        return Err("The disk changed since these steps were planned (something else partitioned it, or it was swapped). Nothing was done; look again.".into());
    }
    if now.system && !unlocked {
        return Err(format!("{} holds the running system. Unlock it first if you really mean to change it.", now.device));
    }
    if (sandbox().is_some() || std::env::var_os("GROVE_SCREENSHOT").is_some()) && !in_sandbox(&now.loop_file) {
        return Err("Test runs only change disk images in GROVE_DISK_SANDBOX.".into());
    }
    // Every step checked again, on the disk as it is now.
    queue.result(&now, &caps)?;
    let mut ids: HashMap<String, String> = HashMap::new();
    let mut cur = now.clone();
    let steps = queue.ops.len();
    let mut pr = Progress { step: 0, steps, report };
    for (i, op) in queue.ops.iter().enumerate() {
        pr.step = i;
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(format!("Stopped after {i} of {steps} steps."));
        }
        let desc = partmap::describe(&cur, op);
        pr.say(0.0, &desc);
        // The disk as it should be after this step.
        let next = partmap::apply(&cur, op, &caps)?;
        let known = ids.clone();
        let real = |id: &str| known.get(id).cloned().unwrap_or_else(|| id.to_string());
        let part_now = |id: &str| -> Result<Part, String> {
            let mut p = cur.parts.iter().find(|p| p.id == id).cloned().ok_or("That partition isn't there any more.")?;
            p.id = real(id);
            Ok(p)
        };
        let r: Result<(), String> = (|| {
            match op {
                Op::NewTable { kind } => {
                    format(&c, &cur.id, kind, "", false)?;
                }
                Op::Create { new_id, offset, size, kind, fs, label, name } => {
                    let ptype = if *kind == Kind::Extended { String::new() } else { partmap::default_ptype(&cur.table, if fs == "empty" { "" } else { fs }, *kind) };
                    let mut o = opts(&[]);
                    if cur.table == "dos" {
                        o.insert("partition-type".into(), ZValue::from(match kind { Kind::Primary => "primary", Kind::Extended => "extended", Kind::Logical => "logical" }));
                    }
                    let name = if cur.table == "gpt" { name.clone() } else { String::new() };
                    let r = call(&c, &cur.id, "PartitionTable", "CreatePartition", &(*offset, *size, ptype.as_str(), name.as_str(), o))?;
                    let p: OwnedObjectPath = r.body().deserialize().map_err(|e| e.to_string())?;
                    ids.insert(new_id.clone(), p.to_string());
                    if *kind != Kind::Extended && !fs.is_empty() && fs != "empty" {
                        pr.say(0.5, &format!("Making the {} filesystem…", partmap::fs_name(fs)));
                        format(&c, p.as_str(), fs, label, false)?;
                    }
                }
                Op::Delete { id } => {
                    call(&c, &real(id), "Partition", "Delete", &(opts(&[]),))?;
                }
                Op::Format { id, fs, label } => {
                    format(&c, &real(id), fs, label, true)?;
                }
                Op::Label { id, label } => {
                    let p = part_now(id)?;
                    call(&c, &p.id, "Filesystem", "SetLabel", &(label.as_str(), opts(&[])))?;
                }
                Op::Name { id, name } => {
                    call(&c, &real(id), "Partition", "SetName", &(name.as_str(), opts(&[])))?;
                }
                Op::Flags { id, flags } => {
                    let p = part_now(id)?;
                    let (t, a) = partmap::apply_flags(&cur.table, &p, flags);
                    if t != p.ptype.to_ascii_lowercase() && t != p.ptype {
                        call(&c, &p.id, "Partition", "SetType", &(t.as_str(), opts(&[])))?;
                    }
                    if a != p.attrs {
                        call(&c, &p.id, "Partition", "SetFlags", &(a, opts(&[])))?;
                    }
                }
                Op::Check { id } => {
                    let p = part_now(id)?;
                    let cap = caps.get(&p.fs).cloned().unwrap_or_default();
                    pr.say(0.1, &format!("Checking {}…", p.device));
                    let ok = if cap.repair {
                        call(&c, &p.id, "Filesystem", "Repair", &(opts(&[]),))?.body().deserialize::<bool>().unwrap_or(false)
                    } else {
                        call(&c, &p.id, "Filesystem", "Check", &(opts(&[]),))?.body().deserialize::<bool>().unwrap_or(false)
                    };
                    if !ok {
                        return Err(format!("{} has problems that couldn't be repaired here.", p.device));
                    }
                    // Then fill the partition, as GParted's Check does.
                    if cap.grow && !cap.online_only && p.fs != "vfat" && p.fs != "swap" {
                        let _ = call(&c, &p.id, "Filesystem", "Resize", &(0u64, opts(&[])));
                    }
                }
                Op::Resize { id, offset, size } => {
                    let mut p = part_now(id)?;
                    let old = (p.offset, p.size);
                    let fat = p.fs == "vfat";
                    let has_fs = !p.fs.is_empty() && p.kind != Kind::Extended;
                    // 1. Shrink.
                    if *size < p.size {
                        pr.say(0.05, &format!("Shrinking {}…", p.device));
                        if fat {
                            fat_resize(&c, &cur, &p, *size, unlocked)?;
                        } else {
                            if has_fs && p.fs != "swap" {
                                fs_resize(&c, &cur, &p, *size, &caps)?;
                            }
                            call(&c, &p.id, "Partition", "Resize", &(*size, opts(&[])))?;
                            if p.fs == "swap" {
                                fs_resize(&c, &cur, &p, 0, &caps)?;
                            }
                        }
                        p.size = *size;
                    }
                    // 2. Move: the data, then the table entry.
                    if *offset != p.offset {
                        let mut f = open_disk_rw(&c, &cur)?;
                        let size_now = p.size;
                        let what = format!("Moving {}:", p.device);
                        {
                            let mut cb = copy_progress(&mut pr, &what);
                            grove_core::diskops::move_partition(&mut f, &cur.table, p.number, p.offset, size_now, *offset, cancel, &mut cb)?;
                        }
                        drop(f);
                        reattach(&c, &cur)?;
                        rescan(&c, &cur.id);
                        let np = wait_part(&c, &cur.id, *offset)?;
                        ids.insert(id.clone(), np.clone());
                        p.id = np;
                        p.offset = *offset;
                    }
                    // 3. Grow.
                    if *size > p.size {
                        pr.say(0.9, &format!("Growing {}…", p.device));
                        if fat {
                            fat_resize(&c, &cur, &p, *size, unlocked)?;
                        } else {
                            call(&c, &p.id, "Partition", "Resize", &(*size, opts(&[])))?;
                            if has_fs {
                                fs_resize(&c, &cur, &p, 0, &caps)?;
                            }
                        }
                    }
                    let _ = old;
                }
                Op::Paste { new_id, offset, size, kind, source, source_disk } => {
                    // The original, as it is now (unmounted, same size).
                    let src_disk = disk(source_disk)?;
                    let src = src_disk.parts.iter().find(|p| p.id == source.id && p.size == source.size).ok_or("The copied partition changed or isn't there any more.")?;
                    if src.busy {
                        return Err(format!("{} is in use; unmount it before pasting a copy.", src.device));
                    }
                    let ptype = partmap::default_ptype(&cur.table, &src.fs, *kind);
                    let mut o = opts(&[]);
                    if cur.table == "dos" {
                        o.insert("partition-type".into(), ZValue::from(if *kind == Kind::Logical { "logical" } else { "primary" }));
                    }
                    let name = if cur.table == "gpt" { src.name.clone() } else { String::new() };
                    let r = call(&c, &cur.id, "PartitionTable", "CreatePartition", &(*offset, *size, ptype.as_str(), name.as_str(), o))?;
                    let np: OwnedObjectPath = r.body().deserialize().map_err(|e| e.to_string())?;
                    ids.insert(new_id.clone(), np.to_string());
                    let (from, from_at) = open_part_read(&c, &src_disk, src, true)?;
                    let (to, to_at) = open_part_write(&c, &cur, np.as_str(), *offset)?;
                    let what = format!("Copying {}:", src.device);
                    {
                        let mut cb = copy_progress(&mut pr, &what);
                        grove_core::diskops::copy_device(&from, from_at, &to, to_at, src.size, cancel, &mut cb)?;
                    }
                    drop(to);
                    reattach(&c, &cur)?;
                    rescan(&c, np.as_str());
                    // Bigger than the original: the filesystem fills it.
                    if *size > src.size {
                        let cap = caps.get(&src.fs).cloned().unwrap_or_default();
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        let p = Part { id: np.to_string(), fs: src.fs.clone(), offset: *offset, size: src.size, number: 0, device: String::new(), ..Default::default() };
                        if src.fs == "vfat" {
                            let d2 = disk(&cur.id)?;
                            if let Some(q) = d2.parts.iter().find(|q| q.id == np.as_str()) {
                                fat_resize(&c, &d2, q, *size, unlocked)?;
                            }
                        } else if cap.grow {
                            fs_resize(&c, &cur, &p, 0, &caps)?;
                        }
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = r {
            return Err(format!("Step {} of {steps} ({desc}) failed: {e}{}", i + 1, if i > 0 { format!(". The {i} step(s) before it were done.") } else { ". Nothing was changed.".into() }));
        }
        // Udisks settles; the next step reads the new state.
        std::thread::sleep(std::time::Duration::from_millis(300));
        cur = next;
        pr.say(1.0, &desc);
    }
    let _ = MIB;
    Ok(())
}

// ---- mount and unmount (done at once, not queued) ----

pub fn unmount(part_id: &str) -> Result<(), String> {
    let c = conn()?;
    let objs = objects(&c)?;
    let Some(i) = objs.iter().find(|(p, _)| p.as_str() == part_id).map(|(_, i)| i) else { return Err("That partition isn't there any more.".into()) };
    if let Some(sw) = i.get("org.freedesktop.UDisks2.Swapspace") {
        if b(sw, "Active") {
            return call(&c, part_id, "Swapspace", "Stop", &(opts(&[]),)).map(|_| ());
        }
    }
    call(&c, part_id, "Filesystem", "Unmount", &(opts(&[]),)).map(|_| ())
}

pub fn mount(part_id: &str) -> Result<String, String> {
    let c = conn()?;
    let r = call(&c, part_id, "Filesystem", "Mount", &(opts(&[]),))?;
    r.body().deserialize::<String>().map_err(|e| e.to_string())
}

// ---- Format… (a removable drive, erased in one go) ----

/// The whole disk behind a mount point or device, and whether the simple
/// Format… may erase it.
pub fn drive_for(mount_or_dev: &str) -> Result<Disk, String> {
    let dev = if mount_or_dev.starts_with("/dev/") {
        mount_or_dev.to_string()
    } else {
        grove_core::mounts::mounts().into_iter().find(|m| m.mount_point == mount_or_dev).map(|m| m.device).ok_or("That disk isn't mounted any more.")?
    };
    let name = kname(&dev);
    let list = disks()?;
    list.into_iter()
        .find(|d| kname(&d.device) == name || d.parts.iter().any(|p| kname(&p.device) == name))
        .ok_or_else(|| "That isn't a disk Grove can format.".to_string())
}

pub fn can_quick_format(d: &Disk) -> Result<(), String> {
    if d.system {
        return Err(format!("{} holds the running system; Format… won't erase it.", d.device));
    }
    if d.read_only {
        return Err("This disk is read-only (check its lock switch).".into());
    }
    let usb = matches!(d.connection.as_str(), "usb" | "sdio" | "ieee1394");
    if !(d.removable || usb || (!d.loop_file.is_empty() && sandbox().is_some())) {
        return Err("Format… is for removable drives (USB sticks, SD cards). For this disk, use Disk Utility.".into());
    }
    Ok(())
}

/// Erase a removable drive: one partition table and one partition filling
/// it with `fs` named `name`, then mount it. Returns where it's mounted.
pub fn quick_format(disk_id: &str, name: &str, fs: &str, report: &mut dyn FnMut(f64, &str)) -> Result<String, String> {
    let d = disk(disk_id)?;
    can_quick_format(&d)?;
    if (sandbox().is_some() || std::env::var_os("GROVE_SCREENSHOT").is_some()) && !in_sandbox(&d.loop_file) {
        return Err("Test runs only change disk images in GROVE_DISK_SANDBOX.".into());
    }
    let c = conn()?;
    report(0.05, "Unmounting…");
    for p in &d.parts {
        if !p.mounts.is_empty() {
            call(&c, &p.id, "Filesystem", "Unmount", &(opts(&[]),)).map_err(|e| format!("Couldn't unmount {}: {e}", p.device))?;
        }
    }
    if !d.whole_fs.is_empty() {
        let _ = call(&c, &d.id, "Filesystem", "Unmount", &(opts(&[]),));
    }
    // MBR where other computers and devices expect it (FAT, exFAT, NTFS).
    let table = if matches!(fs, "vfat" | "exfat" | "ntfs") { "dos" } else { "gpt" };
    report(0.2, "Making a new partition table…");
    format(&c, &d.id, table, "", false)?;
    report(0.4, &format!("Making the {} filesystem…", partmap::fs_name(fs)));
    let ptype = partmap::default_ptype(table, fs, Kind::Primary);
    let mut po = opts(&[]);
    if table == "dos" {
        po.insert("partition-type".into(), ZValue::from("primary"));
    }
    let r = call(&c, &d.id, "PartitionTable", "CreatePartitionAndFormat", &(MIB, 0u64, ptype.as_str(), "", po, fs, format_opts(fs, name, false)))?;
    let p: OwnedObjectPath = r.body().deserialize().map_err(|e| e.to_string())?;
    report(0.9, "Opening it…");
    for _ in 0..20 {
        if let Ok(r) = call(&c, p.as_str(), "Filesystem", "Mount", &(opts(&[]),)) {
            return Ok(r.body().deserialize::<String>().unwrap_or_default());
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    Ok(String::new())
}

// ---- JSON for QML ----

pub fn disk_json(d: &Disk) -> Value {
    let mut v = serde_json::to_value(d).unwrap_or(Value::Null);
    let segs: Vec<Value> = partmap::segments(d)
        .into_iter()
        .map(|s| json!({"part": s.part.map(|i| i as i64).unwrap_or(-1), "offset": s.offset, "size": s.size, "inExtended": s.in_extended}))
        .collect();
    v["segments"] = json!(segs);
    let (lo, hi) = partmap::usable(d);
    v["usableStart"] = json!(lo);
    v["usableEnd"] = json!(hi);
    v["kname"] = d.device.rsplit('/').next().into();
    let caps = caps_now();
    for (i, p) in d.parts.iter().enumerate() {
        v["parts"][i]["flags"] = json!(partmap::flags_of(&d.table, p));
        v["parts"][i]["fsName"] = json!(partmap::fs_name(&p.fs));
        v["parts"][i]["title"] = json!(partmap::part_title(d, p));
        let (a, b) = partmap::room(d, i);
        v["parts"][i]["roomStart"] = json!(a.min(p.offset));
        v["parts"][i]["roomEnd"] = json!(b.max(p.offset + p.size));
        v["parts"][i]["minSize"] = json!(partmap::min_size(p, &caps));
    }
    v
}

/// Static facts for the window: filesystems, flags, what each can do.
pub fn info() -> Value {
    let caps = caps();
    json!({
        "filesystems": partmap::FILESYSTEMS.iter().map(|(id, name, help)| json!({"id": id, "name": name, "help": help, "min": partmap::fs_min(id), "labelMax": partmap::label_max(id), "caps": caps.get(*id)})).collect::<Vec<_>>(),
        "flags": {"gpt": partmap::flag_names("gpt").iter().map(|f| json!({"id": f, "help": partmap::flag_help(f)})).collect::<Vec<_>>(),
                  "dos": partmap::flag_names("dos").iter().map(|f| json!({"id": f, "help": partmap::flag_help(f)})).collect::<Vec<_>>()},
        "caps": caps,
        "sandbox": sandbox().unwrap_or_default(),
    })
}

/// The queue played out: the disk as it will be, each step in words, or
/// why the newest step can't be added.
pub fn plan(base: &Disk, queue: &Queue) -> Value {
    let caps = caps_now();
    let mut d = base.clone();
    let mut steps = vec![];
    for op in &queue.ops {
        steps.push(json!({"text": partmap::describe(&d, op), "op": op}));
        match partmap::apply(&d, op, &caps) {
            Ok(n) => d = n,
            Err(e) => return json!({"error": e, "steps": steps}),
        }
    }
    json!({"result": disk_json(&d), "steps": steps, "queue": queue})
}

pub fn push(base: &Disk, queue: &mut Queue, mut op: Op) -> Value {
    let caps = caps_now();
    // New partitions get their id here.
    match &mut op {
        Op::Create { new_id, .. } | Op::Paste { new_id, .. } if new_id.is_empty() => *new_id = queue.new_id(),
        _ => {}
    }
    match queue.push(base, op, &caps) {
        Ok(()) => plan(base, queue),
        Err(e) => json!({"error": e}),
    }
}

// ---- the controller's side ----

use crate::bridge::ffi::{self, QString};
use cxx_qt::Threading;

static CANCEL: AtomicBool = AtomicBool::new(false);

fn qs(s: &str) -> QString {
    QString::from(s)
}

impl ffi::Grove {
    pub fn disk_info(&self, key: &QString) {
        self.answer(key, info);
    }

    pub fn disk_list(&self, key: &QString) {
        self.answer(key, || {
            caps();
            match disks() {
                Ok(l) => json!({"disks": l.iter().map(disk_json).collect::<Vec<_>>()}),
                Err(e) => json!({"error": e}),
            }
        });
    }

    pub fn disk_plan(&self, disk: &QString, queue: &QString) -> QString {
        let (Ok(d), Ok(q)) = (serde_json::from_str::<Disk>(&disk.to_string()), serde_json::from_str::<Queue>(&queue.to_string())) else { return qs(&json!({"error": "bad request"}).to_string()) };
        qs(&plan(&d, &q).to_string())
    }

    pub fn disk_push(&self, disk: &QString, queue: &QString, op: &QString) -> QString {
        let d = serde_json::from_str::<Disk>(&disk.to_string());
        let q = serde_json::from_str::<Queue>(&queue.to_string());
        let o = serde_json::from_str::<Op>(&op.to_string());
        match (d, q, o) {
            (Ok(d), Ok(mut q), Ok(o)) => qs(&push(&d, &mut q, o).to_string()),
            (_, _, Err(e)) => qs(&json!({"error": format!("bad step: {e}")}).to_string()),
            _ => qs(&json!({"error": "bad request"}).to_string()),
        }
    }

    pub fn disk_usage(&self, key: &QString, part: &QString, fs: &QString) {
        let (p, f) = (part.to_string(), fs.to_string());
        self.answer(key, move || json!({"used": usage(&p, &f)}));
    }

    pub fn disk_apply(&self, key: &QString, disk: &QString, queue: &QString, unlocked: bool) {
        let d = serde_json::from_str::<Disk>(&disk.to_string());
        let q = serde_json::from_str::<Queue>(&queue.to_string());
        let qt = self.qt_thread();
        CANCEL.store(false, std::sync::atomic::Ordering::Relaxed);
        self.answer(key, move || {
            let (Ok(d), Ok(q)) = (d, q) else { return json!({"error": "bad request"}) };
            let mut report = |step: usize, steps: usize, frac: f64, text: &str| {
                let s = json!({"step": step, "steps": steps, "fraction": (step as f64 + frac) / steps.max(1) as f64, "text": text}).to_string();
                let _ = qt.queue(move |g| g.disk_progress(qs(&s)));
            };
            match apply(&d, &q, unlocked, &CANCEL, &mut report) {
                Ok(()) => json!({"ok": true}),
                Err(e) => json!({"error": e}),
            }
        });
    }

    pub fn disk_cancel(&self) {
        CANCEL.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn disk_unmount(&self, key: &QString, part: &QString) {
        let p = part.to_string();
        self.answer(key, move || match unmount(&p) {
            Ok(()) => json!({"ok": true}),
            Err(e) => json!({"error": e}),
        });
    }

    pub fn disk_mount(&self, key: &QString, part: &QString) {
        let p = part.to_string();
        self.answer(key, move || match mount(&p) {
            Ok(m) => json!({"ok": true, "mount": m}),
            Err(e) => json!({"error": e}),
        });
    }

    pub fn drive_lookup(&self, key: &QString, place: &QString) {
        let p = place.to_string();
        self.answer(key, move || {
            caps();
            match drive_for(&p) {
                Ok(d) => {
                    let why = can_quick_format(&d).err();
                    json!({"disk": disk_json(&d), "canFormat": why.is_none(), "reason": why.unwrap_or_default()})
                }
                Err(e) => json!({"error": e}),
            }
        });
    }

    pub fn drive_format(&self, key: &QString, disk: &QString, name: &QString, fs: &QString) {
        let (d, n, f) = (disk.to_string(), name.to_string(), fs.to_string());
        let qt = self.qt_thread();
        self.answer(key, move || {
            let mut report = |frac: f64, text: &str| {
                let s = json!({"step": 0, "steps": 1, "fraction": frac, "text": text}).to_string();
                let _ = qt.queue(move |g| g.disk_progress(qs(&s)));
            };
            match quick_format(&d, &n, &f, &mut report) {
                Ok(m) => json!({"ok": true, "mount": m}),
                Err(e) => json!({"error": e}),
            }
        });
    }
}
