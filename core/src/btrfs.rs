//! btrfs subvolumes and snapshots, through the kernel's own ioctls (no
//! `btrfs` command): which folders are subvolumes, making one, taking a
//! snapshot, deleting one.
//!
//! What a user may do without root: make subvolumes in folders they can
//! write to, snapshot subvolumes they own, and delete an empty, writable
//! subvolume with rmdir. Deleting a subvolume that still holds files (or a
//! read-only snapshot) needs the disk mounted with `user_subvol_rm_allowed`,
//! or root: grove's root helper does it then (`subvol-delete`).

use serde::Serialize;
use std::ffi::CString;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

const BTRFS_SUPER_MAGIC: i64 = 0x9123683E;
/// The inode number every subvolume's top folder has.
const FIRST_FREE_OBJECTID: u64 = 256;
const SUBVOL_RDONLY: u64 = 1 << 1;
/// GET_SUBVOL_INFO hands back the root item's flags, where read-only is
/// bit 0 (BTRFS_ROOT_SUBVOL_RDONLY), not the ioctl flag above.
const ROOT_SUBVOL_RDONLY: u64 = 1 << 0;
const VOL_NAME_MAX: usize = 255;
const PATH_NAME_MAX: usize = 4087;
const SUBVOL_NAME_MAX: usize = 4039;

#[repr(C)]
struct VolArgs {
    fd: i64,
    name: [u8; PATH_NAME_MAX + 1],
}

#[repr(C)]
struct VolArgsV2 {
    fd: i64,
    transid: u64,
    flags: u64,
    unused: [u64; 4],
    name: [u8; SUBVOL_NAME_MAX + 1],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Timespec {
    sec: u64,
    nsec: u32,
}

#[repr(C)]
struct GetSubvolInfo {
    treeid: u64,
    name: [u8; VOL_NAME_MAX + 1],
    parent_id: u64,
    dirid: u64,
    generation: u64,
    flags: u64,
    uuid: [u8; 16],
    parent_uuid: [u8; 16],
    received_uuid: [u8; 16],
    ctransid: u64,
    otransid: u64,
    stransid: u64,
    rtransid: u64,
    ctime: Timespec,
    otime: Timespec,
    stime: Timespec,
    rtime: Timespec,
    reserved: [u64; 8],
}

// _IOW/_IOR/_IOWR as the kernel spells them (asm-generic).
const fn ioc(dir: u64, nr: u64, size: usize) -> u64 {
    (dir << 30) | ((size as u64) << 16) | (0x94 << 8) | nr
}
const W: u64 = 1;
const R: u64 = 2;
const IOC_SUBVOL_CREATE: u64 = ioc(W, 14, std::mem::size_of::<VolArgs>());
const IOC_SNAP_DESTROY: u64 = ioc(W, 15, std::mem::size_of::<VolArgs>());
const IOC_SNAP_CREATE_V2: u64 = ioc(W, 23, std::mem::size_of::<VolArgsV2>());
const IOC_SUBVOL_GETFLAGS: u64 = ioc(R, 25, 8);
const IOC_GET_SUBVOL_INFO: u64 = ioc(R, 60, std::mem::size_of::<GetSubvolInfo>());
const IOC_SNAP_DESTROY_V2: u64 = ioc(W, 63, std::mem::size_of::<VolArgsV2>());

fn open_dir(p: &Path) -> std::io::Result<OwnedFd> {
    use std::os::unix::fs::OpenOptionsExt;
    let f = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC).open(p)?;
    Ok(f.into())
}

fn ioctl<T>(fd: &OwnedFd, req: u64, arg: *mut T) -> std::io::Result<()> {
    if unsafe { libc::ioctl(fd.as_raw_fd(), req as _, arg) } < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// The folder is on a btrfs disk.
pub fn is_btrfs(path: &Path) -> bool {
    let Ok(c) = CString::new(path.as_os_str().as_bytes()) else { return false };
    let mut s: libc::statfs = unsafe { std::mem::zeroed() };
    unsafe { libc::statfs(c.as_ptr(), &mut s) == 0 && s.f_type as i64 == BTRFS_SUPER_MAGIC }
}

/// The folder is the top of a btrfs subvolume (or snapshot). Symlinks
/// aren't followed.
pub fn is_subvolume(path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(m) => m.is_dir() && m.ino() == FIRST_FREE_OBJECTID && is_btrfs(path),
        Err(_) => false,
    }
}

/// Snapper's snapshot folders: `…/.snapshots/N/snapshot`. Those belong to
/// snapper (it numbers them, cleans them up, keeps info.xml beside them).
pub fn is_snapper(path: &Path) -> bool {
    let s = path.to_string_lossy();
    let Some(rest) = s.strip_suffix("/snapshot") else { return false };
    let Some((parent, num)) = rest.rsplit_once('/') else { return false };
    parent.ends_with("/.snapshots") && !num.is_empty() && num.bytes().all(|b| b.is_ascii_digit())
}

#[derive(Clone, Debug, Serialize, PartialEq, Default)]
pub struct Info {
    /// The subvolume's id on its disk.
    pub id: u64,
    pub parent_id: u64,
    pub read_only: bool,
    /// Made as a snapshot of another subvolume.
    pub snapshot: bool,
    pub snapper: bool,
    pub uuid: String,
    /// The subvolume it's a snapshot of ("" when it isn't one).
    pub parent_uuid: String,
    /// When it was made (Unix seconds; 0 unknown).
    pub created: i64,
    pub generation: u64,
}

fn uuid_text(u: &[u8; 16]) -> String {
    if u.iter().all(|&b| b == 0) {
        return String::new();
    }
    let h: String = u.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

/// Facts about a subvolume (None when `path` isn't one). Works without
/// root (GET_SUBVOL_INFO); on old kernels only read-only is known.
pub fn info(path: &Path) -> Option<Info> {
    if !is_subvolume(path) {
        return None;
    }
    let fd = open_dir(path).ok()?;
    let snapper = is_snapper(path);
    let mut a: GetSubvolInfo = unsafe { std::mem::zeroed() };
    if ioctl(&fd, IOC_GET_SUBVOL_INFO, &mut a).is_ok() {
        let parent_uuid = uuid_text(&a.parent_uuid);
        return Some(Info {
            id: a.treeid,
            parent_id: a.parent_id,
            read_only: a.flags & ROOT_SUBVOL_RDONLY != 0,
            snapshot: !parent_uuid.is_empty(),
            snapper,
            uuid: uuid_text(&a.uuid),
            parent_uuid,
            created: a.otime.sec as i64,
            generation: a.generation,
        });
    }
    let mut flags: u64 = 0;
    let _ = ioctl(&fd, IOC_SUBVOL_GETFLAGS, &mut flags);
    Some(Info { read_only: flags & SUBVOL_RDONLY != 0, snapshot: snapper, snapper, ..Default::default() })
}

/// What the Kind column says for a subvolume ("" when it isn't one).
pub fn kind(path: &Path) -> &'static str {
    match info(path) {
        None => "",
        Some(i) if i.snapper => "Snapper Snapshot",
        Some(i) if i.snapshot && i.read_only => "Read-only Snapshot",
        Some(i) if i.snapshot => "Snapshot",
        Some(i) if i.read_only => "Read-only Subvolume",
        Some(_) => "Subvolume",
    }
}

fn check_name(name: &str, max: usize) -> Result<(), String> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') {
        return Err(format!("“{name}” can't be used as a name."));
    }
    if name.len() > max.min(VOL_NAME_MAX) {
        return Err(format!("The name “{name}” is too long."));
    }
    Ok(())
}

fn plain(e: &std::io::Error, p: &Path, what: &str) -> String {
    match e.raw_os_error() {
        Some(libc::ENOTTY) | Some(libc::EOPNOTSUPP) | Some(libc::EINVAL) if !is_btrfs(p.parent().unwrap_or(p)) => {
            format!("{what} only works on btrfs disks, and this folder isn't on one.")
        }
        Some(libc::EXDEV) => format!("{what}: a snapshot has to be on the same btrfs disk as the subvolume."),
        Some(libc::EPERM) | Some(libc::EACCES) => format!("{what}: you don't have permission here."),
        _ => crate::util::plain_error(e, p),
    }
}

/// A new, empty subvolume named `name` in `dir`.
pub fn create(dir: &Path, name: &str) -> Result<PathBuf, String> {
    check_name(name, PATH_NAME_MAX)?;
    let target = dir.join(name);
    if std::fs::symlink_metadata(&target).is_ok() {
        return Err(format!("An item named “{name}” already exists here."));
    }
    let fd = open_dir(dir).map_err(|e| crate::util::plain_error(&e, dir))?;
    let mut a: VolArgs = unsafe { std::mem::zeroed() };
    a.name[..name.len()].copy_from_slice(name.as_bytes());
    ioctl(&fd, IOC_SUBVOL_CREATE, &mut a).map_err(|e| plain(&e, &target, "Couldn't make the subvolume"))?;
    Ok(target)
}

/// A snapshot of the subvolume `src` named `name` in `dest_dir` (same
/// btrfs disk). Read-only snapshots can't change by accident and are what
/// backups (btrfs send) need.
pub fn snapshot(src: &Path, dest_dir: &Path, name: &str, read_only: bool) -> Result<PathBuf, String> {
    check_name(name, SUBVOL_NAME_MAX)?;
    if !is_subvolume(src) {
        return Err(format!("“{}” isn't a btrfs subvolume, so it can't be snapshotted.", display_name(src)));
    }
    let target = dest_dir.join(name);
    if std::fs::symlink_metadata(&target).is_ok() {
        return Err(format!("An item named “{name}” already exists there."));
    }
    let sfd = open_dir(src).map_err(|e| crate::util::plain_error(&e, src))?;
    let dfd = open_dir(dest_dir).map_err(|e| crate::util::plain_error(&e, dest_dir))?;
    let mut a: VolArgsV2 = unsafe { std::mem::zeroed() };
    a.fd = sfd.as_raw_fd() as i64;
    a.flags = if read_only { SUBVOL_RDONLY } else { 0 };
    a.name[..name.len()].copy_from_slice(name.as_bytes());
    ioctl(&dfd, IOC_SNAP_CREATE_V2, &mut a).map_err(|e| match e.raw_os_error() {
        Some(libc::EPERM) => format!("Couldn't take the snapshot: only the owner of “{}” (or an administrator) can snapshot it.", display_name(src)),
        _ => plain(&e, &target, "Couldn't take the snapshot"),
    })?;
    Ok(target)
}

fn display_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

/// "Name 2026-11-24 09.15": a snapshot's default name.
pub fn snapshot_name(src: &Path, now: i64) -> String {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let t = now as libc::time_t;
    unsafe { libc::localtime_r(&t, &mut tm) };
    let base = match src.file_name() {
        Some(n) => n.to_string_lossy().into_owned(),
        None => "Snapshot".into(),
    };
    format!("{base} {:04}-{:02}-{:02} {:02}.{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min)
}

/// The mount point holding `path` and its options (filesystem ones
/// included, as /proc/self/mounts lists them).
fn mount_of(path: &Path) -> Option<(PathBuf, String)> {
    let text = std::fs::read_to_string("/proc/self/mounts").ok()?;
    let p = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut best: Option<(PathBuf, String)> = None;
    for line in text.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        if f.len() < 4 {
            continue;
        }
        let mp = PathBuf::from(f[1].replace("\\040", " ").replace("\\011", "\t").replace("\\134", "\\"));
        if p.starts_with(&mp) && best.as_ref().map_or(true, |b| mp.as_os_str().len() >= b.0.as_os_str().len()) {
            best = Some((mp, f[3].to_string()));
        }
    }
    best
}

/// Where a new snapshot of `src` goes by default: beside it when its
/// folder is on the same disk and writable, otherwise inside it.
pub fn snapshot_place(src: &Path) -> PathBuf {
    let at_mount_top = mount_of(src).map_or(false, |(mp, _)| std::fs::canonicalize(src).map_or(false, |c| c == mp));
    if let Some(parent) = src.parent() {
        if !at_mount_top && writable(parent) {
            return parent.to_path_buf();
        }
    }
    src.to_path_buf()
}

fn writable(p: &Path) -> bool {
    let Ok(c) = CString::new(p.as_os_str().as_bytes()) else { return false };
    unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

fn empty_dir(p: &Path) -> bool {
    std::fs::read_dir(p).map_or(false, |mut r| r.next().is_none())
}

/// Deleting `path` needs an administrator: grove can't do it as you (the
/// disk isn't mounted with user_subvol_rm_allowed and it isn't an empty,
/// writable subvolume, which rmdir can remove).
pub fn delete_needs_root(path: &Path) -> bool {
    if unsafe { libc::geteuid() } == 0 {
        return false;
    }
    let allowed = mount_of(path).map_or(false, |(_, o)| o.split(',').any(|x| x == "user_subvol_rm_allowed"));
    if allowed {
        return false;
    }
    let ro = info(path).map_or(false, |i| i.read_only);
    !(empty_dir(path) && !ro)
}

#[derive(Debug, PartialEq)]
pub enum DeleteError {
    /// Only an administrator can delete it (EPERM).
    NeedsRoot,
    Other(String),
}

/// Deletes the subvolume or snapshot `path` and everything in it. It
/// refuses anything that isn't a subvolume. The kernel refuses a
/// subvolume that holds other subvolumes (delete those first).
pub fn delete(path: &Path) -> Result<(), DeleteError> {
    if !is_subvolume(path) {
        return Err(DeleteError::Other(format!("“{}” isn't a btrfs subvolume.", display_name(path))));
    }
    let name = path.file_name().ok_or_else(|| DeleteError::Other("Can't delete the top of a disk.".into()))?;
    let parent = path.parent().ok_or_else(|| DeleteError::Other("Can't delete the top of a disk.".into()))?;
    let nb = name.as_bytes();
    if nb.len() > SUBVOL_NAME_MAX {
        return Err(DeleteError::Other("The name is too long.".into()));
    }
    let pfd = open_dir(parent).map_err(|e| DeleteError::Other(crate::util::plain_error(&e, parent)))?;
    let mut a: VolArgsV2 = unsafe { std::mem::zeroed() };
    a.name[..nb.len()].copy_from_slice(nb);
    let mut r = ioctl(&pfd, IOC_SNAP_DESTROY_V2, &mut a);
    if matches!(&r, Err(e) if e.raw_os_error() == Some(libc::ENOTTY)) {
        // Kernels before 5.7.
        let mut a1: VolArgs = unsafe { std::mem::zeroed() };
        a1.name[..nb.len()].copy_from_slice(nb);
        r = ioctl(&pfd, IOC_SNAP_DESTROY, &mut a1);
    }
    match r {
        Ok(()) => Ok(()),
        Err(e) if matches!(e.raw_os_error(), Some(libc::EPERM) | Some(libc::EACCES)) => {
            // An empty, writable subvolume: rmdir is allowed to anyone who
            // may change the folder it's in.
            if empty_dir(path) && std::fs::remove_dir(path).is_ok() {
                return Ok(());
            }
            Err(DeleteError::NeedsRoot)
        }
        Err(e) if e.raw_os_error() == Some(libc::ENOTEMPTY) => Err(DeleteError::Other(format!(
            "“{}” holds other subvolumes inside it. Delete those first.",
            display_name(path)
        ))),
        Err(e) if e.raw_os_error() == Some(libc::EBUSY) => Err(DeleteError::Other(format!(
            "“{}” is in use (mounted somewhere, or the disk's default subvolume).",
            display_name(path)
        ))),
        Err(e) => Err(DeleteError::Other(crate::util::plain_error(&e, path))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_match_the_kernel() {
        assert_eq!(std::mem::size_of::<VolArgs>(), 4096);
        assert_eq!(std::mem::size_of::<VolArgsV2>(), 4096);
        assert_eq!(std::mem::size_of::<GetSubvolInfo>(), 504);
        // Numbers from linux/btrfs.h, as the btrfs tool sends them.
        assert_eq!(IOC_SUBVOL_CREATE, 0x5000_940e);
        assert_eq!(IOC_SNAP_CREATE_V2, 0x5000_9417);
        assert_eq!(IOC_SNAP_DESTROY_V2, 0x5000_943f);
        assert_eq!(IOC_GET_SUBVOL_INFO, 0x81f8_943c);
    }

    #[test]
    fn snapper_paths() {
        assert!(is_snapper(Path::new("/home/.snapshots/12/snapshot")));
        assert!(!is_snapper(Path::new("/home/.snapshots/12")));
        assert!(!is_snapper(Path::new("/home/x/snapshot")));
        assert!(!is_snapper(Path::new("/home/.snapshots/a/snapshot")));
    }

    #[test]
    fn names() {
        assert!(check_name("a/b", 100).is_err());
        assert!(check_name("..", 100).is_err());
        assert!(check_name("Data", 100).is_ok());
        let n = snapshot_name(Path::new("/x/Data"), 0);
        assert!(n.starts_with("Data 19"), "{n}");
    }

    /// Runs only with GROVE_BTRFS_TEST_DIR set to a writable folder on a
    /// throwaway btrfs disk (a loop-mounted image): never real subvolumes.
    #[test]
    fn on_a_test_disk() {
        let Ok(dir) = std::env::var("GROVE_BTRFS_TEST_DIR") else {
            eprintln!("skipped: GROVE_BTRFS_TEST_DIR not set");
            return;
        };
        let dir = PathBuf::from(dir);
        assert!(is_btrfs(&dir), "GROVE_BTRFS_TEST_DIR must be on btrfs");
        let t = dir.join(format!("grove-test-{}", std::process::id()));
        std::fs::create_dir(&t).unwrap();
        assert!(!is_subvolume(&t));
        let sv = create(&t, "Data").unwrap();
        assert!(is_subvolume(&sv));
        assert_eq!(kind(&sv), "Subvolume");
        assert!(create(&t, "Data").is_err());
        std::fs::write(sv.join("f.txt"), "hi").unwrap();
        let i = info(&sv).unwrap();
        assert!(i.id >= 256 && !i.read_only && !i.snapshot && !i.uuid.is_empty());
        assert_eq!(snapshot_place(&sv), t);

        let snap = snapshot(&sv, &t, "Data snap", true).unwrap();
        assert_eq!(std::fs::read_to_string(snap.join("f.txt")).unwrap(), "hi");
        let si = info(&snap).unwrap();
        assert!(si.read_only && si.snapshot && si.parent_uuid == i.uuid, "{si:?} {i:?}");
        assert_eq!(kind(&snap), "Read-only Snapshot");
        assert!(std::fs::write(snap.join("g.txt"), "x").is_err());
        let rw = snapshot(&sv, &t, "Data rw", false).unwrap();
        assert_eq!(kind(&rw), "Snapshot");
        assert!(snapshot(&t, &t, "nope", true).is_err());

        // A plain folder is never deleted as a subvolume.
        assert!(matches!(delete(&t), Err(DeleteError::Other(_))));
        // Empty and writable: rmdir works as the user.
        let empty = create(&t, "Empty").unwrap();
        assert!(!delete_needs_root(&empty) || unsafe { libc::geteuid() } == 0);
        delete(&empty).unwrap();
        assert!(!empty.exists());

        for p in [&snap, &rw, &sv] {
            match delete(p) {
                Ok(()) => assert!(!p.exists()),
                // Not mounted with user_subvol_rm_allowed: an admin's job.
                Err(DeleteError::NeedsRoot) => assert!(delete_needs_root(p)),
                Err(e) => panic!("{e:?}"),
            }
        }
        // Leave the disk as found when we could (an rw subvolume can be
        // emptied and removed by its owner).
        for p in [&rw, &sv] {
            if p.exists() {
                let _ = std::fs::remove_file(p.join("f.txt"));
                let _ = delete(p);
            }
        }
        let _ = std::fs::remove_dir(&t);
    }
}
