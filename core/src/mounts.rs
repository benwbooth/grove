//! Mounted filesystems (/proc/self/mountinfo) and their free space, and
//! which of them the sidebar shows as devices.

use serde::Serialize;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Mount {
    pub device: String,
    pub mount_point: String,
    pub fs_type: String,
    pub options: String,
    /// The folder of the filesystem mounted here ("/" for its top; a
    /// btrfs subvolume's path, e.g. "/@home").
    pub root: String,
}

fn unescape_octal(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 4 <= b.len() {
            if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 4]).unwrap_or(""), 8) {
                out.push(v);
                i += 4;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn parse_mountinfo(text: &str) -> Vec<Mount> {
    let mut v = Vec::new();
    for line in text.lines() {
        let Some((pre, post)) = line.split_once(" - ") else { continue };
        let pre: Vec<&str> = pre.split(' ').collect();
        let post: Vec<&str> = post.split(' ').collect();
        if pre.len() < 6 || post.len() < 2 {
            continue;
        }
        v.push(Mount {
            mount_point: unescape_octal(pre[4]),
            options: pre[5].to_string(),
            root: unescape_octal(pre[3]),
            fs_type: post[0].to_string(),
            device: unescape_octal(post[1]),
        });
    }
    v
}

pub fn mounts() -> Vec<Mount> {
    parse_mountinfo(&std::fs::read_to_string("/proc/self/mountinfo").unwrap_or_default())
}

/// (total, free-for-user) bytes of the filesystem holding `path`.
pub fn space(path: &str) -> Option<(u64, u64)> {
    let c = std::ffi::CString::new(path).ok()?;
    unsafe {
        let mut s: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut s) != 0 {
            return None;
        }
        Some((s.f_blocks as u64 * s.f_frsize as u64, s.f_bavail as u64 * s.f_frsize as u64))
    }
}

/// The kind of place a mount is, for its sidebar icon and eject action.
pub fn kind(m: &Mount) -> &'static str {
    let t = m.fs_type.as_str();
    if t == "fuse.sshfs" {
        "sftp"
    } else if t == "cifs" || t == "smb3" || t == "fuse.smbnetfs" {
        "smb"
    } else if t.starts_with("fuse.rclone") || t == "rclone" {
        "cloud"
    } else if t == "fuse.jmtpfs" || t == "fuse.simple-mtpfs" || t.contains("mtp") {
        "phone"
    } else if t.starts_with("nfs") {
        "nfs"
    } else if t == "iso9660" || t == "udf" {
        "optical"
    } else {
        "disk"
    }
}

/// Mounts worth showing in the sidebar: removable media, network and
/// FUSE mounts the user made. System mounts (/nix/store, /boot, /proc...)
/// are left out.
pub fn user_mounts() -> Vec<Mount> {
    let home = crate::util::home().to_string_lossy().into_owned();
    let uid = unsafe { libc::getuid() };
    mounts()
        .into_iter()
        .filter(|m| {
            let p = m.mount_point.as_str();
            let t = m.fs_type.as_str();
            let user_place = p.starts_with("/run/media/") || p.starts_with("/media/") || p.starts_with("/mnt/") || p.starts_with(&format!("/run/user/{uid}/grove/")) || (p.starts_with(&home) && p != home);
            let net = matches!(t, "cifs" | "smb3" | "nfs" | "nfs4") || t.starts_with("fuse.");
            let skip_fuse = matches!(t, "fuse.portal" | "fuse.gvfsd-fuse" | "fuse.kio-fuse" | "fuse.doc") || p.starts_with(&format!("/run/user/{uid}/doc"));
            (user_place || net) && !skip_fuse && !p.starts_with("/nix") && !p.starts_with("/boot") && p != "/"
        })
        .fold(Vec::new(), |mut acc: Vec<Mount>, m| {
            if !acc.iter().any(|x| x.mount_point == m.mount_point) {
                acc.push(m);
            }
            acc
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses() {
        let t = "36 35 98:0 /mnt1 /mnt/my\\040disk rw,noatime master:1 - ext3 /dev/root rw,errors=continue\n\
                 40 1 0:40 / /run/media/ben/USB rw shared:3 - vfat /dev/sdb1 rw\n";
        let m = parse_mountinfo(t);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].mount_point, "/mnt/my disk");
        assert_eq!(m[0].fs_type, "ext3");
        assert_eq!(m[1].device, "/dev/sdb1");
        assert_eq!(m[0].root, "/mnt1");
        assert_eq!(kind(&m[1]), "disk");
    }
}
