//! Small shared pieces: XDG directories, file URIs, natural sort order,
//! local time stamps, and plain-language error messages.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    match std::env::var_os(var) {
        Some(v) if Path::new(&v).is_absolute() => PathBuf::from(v),
        _ => home().join(fallback),
    }
}

pub fn data_home() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share")
}
pub fn config_home() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config")
}
pub fn cache_home() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache")
}
pub fn state_home() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state")
}

/// $XDG_DATA_DIRS (system data directories), most important first.
pub fn data_dirs() -> Vec<PathBuf> {
    let v = std::env::var("XDG_DATA_DIRS").unwrap_or_default();
    let mut out: Vec<PathBuf> = v.split(':').filter(|s| !s.is_empty()).map(PathBuf::from).collect();
    if out.is_empty() {
        out = vec!["/usr/local/share".into(), "/usr/share".into()];
    }
    out
}

/// $XDG_CONFIG_DIRS, most important first.
pub fn config_dirs() -> Vec<PathBuf> {
    let v = std::env::var("XDG_CONFIG_DIRS").unwrap_or_default();
    let mut out: Vec<PathBuf> = v.split(':').filter(|s| !s.is_empty()).map(PathBuf::from).collect();
    if out.is_empty() {
        out = vec!["/etc/xdg".into()];
    }
    out
}

/// grove's own config directory (~/.config/grove), created on demand.
pub fn grove_config() -> PathBuf {
    let d = config_home().join("grove");
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn grove_state() -> PathBuf {
    let d = state_home().join("grove");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Characters GLib's g_filename_to_uri leaves alone in a path (so the MD5
/// of a URI matches what GNOME apps and thumbnailers compute).
const PATH_SET: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'!')
    .remove(b'$')
    .remove(b'&')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')')
    .remove(b'*')
    .remove(b'+')
    .remove(b',')
    .remove(b'-')
    .remove(b'.')
    .remove(b'/')
    .remove(b':')
    .remove(b'=')
    .remove(b'@')
    .remove(b'_')
    .remove(b'~');

/// file:// URI for an absolute path, escaped like GLib does.
pub fn file_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    format!("file://{}", percent_encoding::percent_encode(path.as_os_str().as_bytes(), PATH_SET))
}

/// Percent-encode a path for a .trashinfo Path= line (same escaping).
pub fn escape_path(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    percent_encoding::percent_encode(path.as_os_str().as_bytes(), PATH_SET).to_string()
}

pub fn unescape_path(s: &str) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    let bytes: Vec<u8> = percent_encoding::percent_decode_str(s).collect();
    PathBuf::from(std::ffi::OsString::from_vec(bytes))
}

/// A path from a file:// URI (or a plain path). None for other schemes.
pub fn path_from_uri(uri: &str) -> Option<PathBuf> {
    if let Some(rest) = uri.strip_prefix("file://") {
        // file://host/path: only localhost or empty host.
        let rest = if rest.starts_with('/') {
            rest
        } else {
            let slash = rest.find('/')?;
            let host = &rest[..slash];
            if host != "localhost" {
                return None;
            }
            &rest[slash..]
        };
        let rest = rest.split(['?', '#']).next().unwrap_or(rest);
        Some(unescape_path(rest))
    } else if uri.starts_with('/') {
        Some(PathBuf::from(uri))
    } else {
        None
    }
}

/// Finder/Dolphin-style ordering: case-insensitive, runs of digits compared
/// by value ("file2" before "file10").
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = ai.peek().copied().filter(|c| c.is_ascii_digit()) {
                    na.push(c);
                    ai.next();
                }
                let mut nb = String::new();
                while let Some(c) = bi.peek().copied().filter(|c| c.is_ascii_digit()) {
                    nb.push(c);
                    bi.next();
                }
                let ta = na.trim_start_matches('0');
                let tb = nb.trim_start_matches('0');
                let o = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = if x.is_ascii() && y.is_ascii() {
                    x.to_ascii_lowercase().cmp(&y.to_ascii_lowercase())
                } else {
                    x.to_lowercase().cmp(y.to_lowercase())
                };
                if o != Ordering::Equal {
                    return o;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

/// "YYYY-MM-DDThh:mm:ss" in local time (the trash spec's DeletionDate).
pub fn local_timestamp(secs: i64) -> String {
    unsafe {
        let t: libc::time_t = secs as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    }
}

/// Parse "YYYY-MM-DDThh:mm:ss" (local time) back to seconds.
pub fn parse_local_timestamp(s: &str) -> Option<i64> {
    let s = s.trim();
    let (d, t) = s.split_once('T')?;
    let dp: Vec<i32> = d.split('-').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    let tp: Vec<i32> = t.split(':').map(|x| x.split('.').next().unwrap_or("0").parse().ok()).collect::<Option<_>>()?;
    if dp.len() != 3 || tp.len() < 2 {
        return None;
    }
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        tm.tm_year = dp[0] - 1900;
        tm.tm_mon = dp[1] - 1;
        tm.tm_mday = dp[2];
        tm.tm_hour = tp[0];
        tm.tm_min = tp[1];
        tm.tm_sec = *tp.get(2).unwrap_or(&0);
        tm.tm_isdst = -1;
        Some(libc::mktime(&mut tm) as i64)
    }
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// EACCES/EPERM: refused for lack of rights (administrator rights may
/// get past it).
pub fn is_denied(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::PermissionDenied || matches!(e.raw_os_error(), Some(libc::EACCES) | Some(libc::EPERM))
}

fn may_write_dir(dir: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else { return false };
    unsafe { libc::access(c.as_ptr(), libc::W_OK | libc::X_OK) == 0 }
}

/// Making something in `dir` is refused to this user (its folder isn't
/// writable by them; a read-only disk doesn't count: root can't either).
pub fn create_denied(dir: &Path) -> bool {
    if may_write_dir(dir) {
        return false;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EACCES)
}

/// Deleting, renaming or moving `path` away is refused to this user: its
/// folder isn't writable by them, or the folder is sticky (like /tmp) and
/// neither it nor the item is theirs.
pub fn change_denied(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("/"),
    };
    if create_denied(dir) {
        return true;
    }
    let me = unsafe { libc::getuid() };
    match (std::fs::metadata(dir), std::fs::symlink_metadata(path)) {
        (Ok(d), Ok(f)) => d.mode() & 0o1000 != 0 && me != 0 && d.uid() != me && f.uid() != me,
        _ => false,
    }
}

/// An I/O error in words a person can act on.
pub fn plain_error(e: &std::io::Error, path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
    use std::io::ErrorKind::*;
    match e.kind() {
        NotFound => format!("“{name}” isn't there any more. It may have been moved or deleted."),
        PermissionDenied => format!("You don't have permission to change “{name}”."),
        AlreadyExists => format!("An item named “{name}” already exists here."),
        _ => match e.raw_os_error() {
            Some(libc::ENOSPC) => format!("There isn't enough free space for “{name}”."),
            Some(libc::EROFS) => format!("“{name}” is on a disk that can't be written to (read-only)."),
            Some(libc::EXDEV) => format!("“{name}” is on another disk."),
            Some(libc::ENOTEMPTY) => format!("The folder “{name}” isn't empty."),
            Some(libc::ENAMETOOLONG) => format!("The name “{name}” is too long."),
            Some(libc::EBUSY) => format!("“{name}” is in use by another program."),
            Some(libc::EIO) => format!("The disk couldn't read or write “{name}” (input/output error)."),
            _ => format!("Couldn't work with “{name}”: {e}"),
        },
    }
}

/// Human file size, Finder style (decimal units: 1 KB = 1000 bytes).
pub fn human_size(n: u64) -> String {
    const U: [&str; 6] = ["bytes", "KB", "MB", "GB", "TB", "PB"];
    if n < 1000 {
        return if n == 1 { "1 byte".into() } else { format!("{n} bytes") };
    }
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1000.0 && i < U.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    if v < 10.0 {
        format!("{v:.1} {}", U[i])
    } else {
        format!("{v:.0} {}", U[i])
    }
}

/// Write a file atomically (temp file in the same directory, then rename).
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{}.grove-tmp-{}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), std::process::id()));
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refused_changes_are_told_apart() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::getuid() } == 0 {
            return;
        }
        let d = std::env::temp_dir().join(format!("grove-denied-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("locked")).unwrap();
        std::fs::write(d.join("locked/f"), "x").unwrap();
        std::fs::write(d.join("mine"), "x").unwrap();
        std::fs::set_permissions(d.join("locked"), std::fs::Permissions::from_mode(0o555)).unwrap();
        assert!(change_denied(&d.join("locked/f")));
        assert!(create_denied(&d.join("locked")));
        assert!(!change_denied(&d.join("mine")));
        assert!(!create_denied(&d));
        let e = std::fs::remove_file(d.join("locked/f")).unwrap_err();
        assert!(is_denied(&e));
        std::fs::set_permissions(d.join("locked"), std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["file10", "File2", "file1", "a", "B", "file02b"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["a", "B", "file1", "File2", "file02b", "file10"]);
    }

    #[test]
    fn uri_roundtrip() {
        let p = Path::new("/home/x/My File#1 [ü].txt");
        let u = file_uri(p);
        assert_eq!(u, "file:///home/x/My%20File%231%20%5B%C3%BC%5D.txt");
        assert_eq!(path_from_uri(&u).unwrap(), p);
        assert_eq!(path_from_uri("file://localhost/tmp/a").unwrap(), Path::new("/tmp/a"));
        assert!(path_from_uri("sftp://host/tmp").is_none());
    }

    #[test]
    fn timestamps() {
        let t = 1_700_000_000;
        assert_eq!(parse_local_timestamp(&local_timestamp(t)), Some(t));
    }

    #[test]
    fn sizes() {
        assert_eq!(human_size(1), "1 byte");
        assert_eq!(human_size(999), "999 bytes");
        assert_eq!(human_size(1500), "1.5 KB");
        assert_eq!(human_size(25_000_000), "25 MB");
    }
}
