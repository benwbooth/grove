//! Permission and owner changes done by grove itself (no chmod/chown
//! commands), recursively without following symlinks. When the files
//! aren't yours, `pkexec grove --root-helper ...` runs this same code as
//! root (polkit asks for the password); so does deleting a btrfs
//! subvolume when the disk doesn't let users do that, and file operations
//! the user was refused and then asked to redo as administrator (delete,
//! rename, new folder/file, copy/move/alias into a place that isn't
//! theirs). Nothing else runs as root. `as_root` is the one way grove asks
//! polkit (pkexec) to run this helper.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Each path under `root` (itself first), symlinks not followed.
fn walk(root: &Path, recursive: bool, f: &mut dyn FnMut(&Path) -> std::io::Result<()>) -> std::io::Result<()> {
    f(root)?;
    if !recursive {
        return Ok(());
    }
    let meta = std::fs::symlink_metadata(root)?;
    if meta.is_dir() {
        for e in std::fs::read_dir(root)? {
            walk(&e?.path(), true, f)?;
        }
    }
    Ok(())
}

pub fn chmod(path: &Path, mode: u32, recursive: bool) -> std::io::Result<()> {
    walk(path, recursive, &mut |p| {
        if std::fs::symlink_metadata(p)?.file_type().is_symlink() {
            return Ok(());
        }
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode))
    })
}

fn uid_of(name: &str) -> Option<u32> {
    if let Ok(n) = name.parse() {
        return Some(n);
    }
    let c = std::ffi::CString::new(name).ok()?;
    let pw = unsafe { libc::getpwnam(c.as_ptr()) };
    (!pw.is_null()).then(|| unsafe { (*pw).pw_uid })
}

fn gid_of(name: &str) -> Option<u32> {
    if let Ok(n) = name.parse() {
        return Some(n);
    }
    let c = std::ffi::CString::new(name).ok()?;
    let gr = unsafe { libc::getgrnam(c.as_ptr()) };
    (!gr.is_null()).then(|| unsafe { (*gr).gr_gid })
}

/// Owner and/or group by name or number ("" leaves it as it is).
pub fn chown(path: &Path, owner: &str, group: &str, recursive: bool) -> std::io::Result<()> {
    let bad = |what: &str, n: &str| std::io::Error::new(std::io::ErrorKind::NotFound, format!("No {what} named “{n}”"));
    let uid = if owner.is_empty() { None } else { Some(uid_of(owner).ok_or_else(|| bad("user", owner))?) };
    let gid = if group.is_empty() { None } else { Some(gid_of(group).ok_or_else(|| bad("group", group))?) };
    walk(path, recursive, &mut |p| std::os::unix::fs::lchown(p, uid, gid))
}

/// Grove Disk Utility: FAT resized by libparted (as GParted does). Checked
/// again here, as root: a whole disk, not the running system's unless the
/// user unlocked it, and the partition not mounted.
fn fat_resize(dev: &str, num: &str, start: &str, end: &str, unlocked: bool) -> std::io::Result<()> {
    let bad = |m: &str| std::io::Error::new(std::io::ErrorKind::InvalidInput, m.to_string());
    let (Ok(num), Ok(start), Ok(end)) = (num.parse::<u32>(), start.parse::<i64>(), end.parse::<i64>()) else { return Err(bad("bad numbers")) };
    use std::os::unix::fs::FileTypeExt;
    if !std::fs::metadata(dev)?.file_type().is_block_device() {
        return Err(bad("not a disk"));
    }
    let name = std::fs::canonicalize(dev)?.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !unlocked && grove_core::partmap::system_disks().contains(&name) {
        return Err(bad("This disk holds the running system."));
    }
    let mounts = std::fs::read_to_string("/proc/self/mounts").unwrap_or_default();
    let part = |p: &str| mounts.lines().any(|l| l.split(' ').next().map(|d| std::fs::canonicalize(d).ok().and_then(|x| x.file_name().map(|n| n.to_string_lossy().into_owned())) == Some(p.to_string())).unwrap_or(false));
    for p in [format!("{name}{num}"), format!("{name}p{num}")] {
        if part(&p) {
            return Err(bad("The partition is mounted; unmount it first."));
        }
    }
    grove_core::diskops::fat_resize(dev, num, start, end).map_err(|e| std::io::Error::other(e))
}

/// `grove --root-helper chmod MODE R PATH` / `chown OWNER GROUP R PATH`
/// (R is 1 for recursive). Returns the exit code.
pub fn run(args: &[String]) -> i32 {
    let r = match args {
        [op, mode, rec, path] if op == "chmod" => match u32::from_str_radix(mode, 8) {
            Ok(m) => chmod(Path::new(path), m, rec == "1"),
            Err(_) => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "bad mode")),
        },
        [op, owner, group, rec, path] if op == "chown" => chown(Path::new(path), owner, group, rec == "1"),
        // Deleting a btrfs subvolume or snapshot: grove-core refuses
        // anything that isn't one (inode 256 on btrfs).
        [op, path] if op == "subvol-delete" => match grove_core::btrfs::delete(Path::new(path)) {
            Ok(()) => Ok(()),
            Err(grove_core::btrfs::DeleteError::NeedsRoot) => Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "not allowed")),
            Err(grove_core::btrfs::DeleteError::Other(m)) => Err(std::io::Error::other(m)),
        },
        [op, dev, num, start, end, unlocked] if op == "fat-resize" => fat_resize(dev, num, start, end, unlocked == "1"),
        [op, rest @ ..] if op == "files" => match files(rest) {
            Ok(v) => {
                println!("{v}");
                Ok(())
            }
            Err(m) => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, m)),
        },
        _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "usage: --root-helper chmod MODE 0|1 PATH | chown OWNER GROUP 0|1 PATH | subvol-delete PATH | fat-resize DISK N START END 0|1 | files OP ARGS...")),
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

/// pkexec runs programs with a clean environment: `--env NAME=VALUE`
/// arguments put back what the window needs (Open as Root).
pub fn apply_env_args(args: &mut Vec<String>) {
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--env" && i + 1 < args.len() {
            if let Some((k, v)) = args[i + 1].split_once('=') {
                std::env::set_var(k, v);
            }
            args.drain(i..i + 2);
        } else {
            i += 1;
        }
    }
}

/// This program, for pkexec.
pub fn exe() -> String {
    std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "grove".into())
}

// ---------------- file operations as administrator ----------------

/// Places grove won't delete even as administrator: the root and the
/// system's own top folders (what's inside them is up to the user).
fn protected(p: &Path) -> bool {
    const TOP: [&str; 21] = ["/", "/bin", "/boot", "/dev", "/etc", "/home", "/lib", "/lib64", "/nix", "/nix/store", "/nix/var", "/opt", "/proc", "/root", "/run", "/sbin", "/srv", "/sys", "/tmp", "/usr", "/var"];
    let s = p.to_string_lossy();
    let s = s.trim_end_matches('/');
    let s = if s.is_empty() { "/" } else { s };
    TOP.contains(&s) || p.components().any(|c| matches!(c, std::path::Component::ParentDir))
}

fn absolute(p: &str) -> Result<std::path::PathBuf, String> {
    let pb = std::path::PathBuf::from(p);
    if !pb.is_absolute() {
        return Err(format!("not an absolute path: {p}"));
    }
    Ok(pb)
}

/// Conflicts while copying as administrator: the item already there is
/// kept (the user already chose for the items that weren't refused).
struct SkipHost;
impl grove_core::ops::Host for SkipHost {
    fn progress(&self, _: &grove_core::ops::Progress) {}
    fn conflict(&self, _: &grove_core::ops::ConflictInfo) -> (grove_core::ops::Resolution, bool) {
        (grove_core::ops::Resolution::Skip, true)
    }
}

/// `files delete PATH...` | `files rename PATH NAME` | `files mkdir DIR NAME`
/// | `files newfile DIR NAME` | `files job KIND DEST SRC...`. Answers JSON
/// {"results": [...], "errors": [...]} (per item; exit code 0 when it ran).
pub fn files(args: &[String]) -> Result<serde_json::Value, String> {
    use grove_core::ops;
    use grove_core::util::plain_error;
    use serde_json::json;
    let (mut results, mut errors): (Vec<String>, Vec<String>) = (vec![], vec![]);
    let s = |p: &Path| p.to_string_lossy().into_owned();
    match args {
        [op, paths @ ..] if op == "delete" && !paths.is_empty() => {
            for p in paths {
                let p = absolute(p)?;
                if protected(&p) {
                    errors.push(format!("Grove doesn't delete “{}”: the system needs it.", p.display()));
                    continue;
                }
                match ops::remove_tree(&p) {
                    Ok(()) => results.push(s(&p)),
                    Err(e) => errors.push(plain_error(&e, &p)),
                }
            }
        }
        [op, path, name] if op == "rename" => {
            let p = absolute(path)?;
            if protected(&p) {
                return Err(format!("Grove doesn't rename “{}”: the system needs it.", p.display()));
            }
            match ops::rename(&p, name) {
                Ok(ops::Action::Moved { pairs }) => results.push(pairs.first().map(|x| s(&x.1)).unwrap_or_else(|| s(&p))),
                Ok(_) => results.push(s(&p)),
                Err(e) => errors.push(e),
            }
        }
        [op, dir, name] if op == "mkdir" || op == "newfile" => {
            let d = absolute(dir)?;
            if name.contains('/') || name == "." || name == ".." {
                return Err("bad name".into());
            }
            let r = if op == "mkdir" { ops::new_folder(&d, if name.is_empty() { None } else { Some(name) }) } else { ops::new_file(&d, if name.is_empty() { "untitled.txt" } else { name }, None) };
            match r {
                Ok((p, _)) => results.push(s(&p)),
                Err(e) => errors.push(e),
            }
        }
        [op, kind, dest, sources @ ..] if op == "job" && !sources.is_empty() => {
            let kind = ops::Kind::from_str(kind);
            let dest = absolute(dest)?;
            let mut srcs = vec![];
            for p in sources {
                let p = absolute(p)?;
                // Moved before the refusal (the user's own run did those).
                if p.symlink_metadata().is_err() {
                    continue;
                }
                if kind == ops::Kind::Move && protected(&p) {
                    errors.push(format!("Grove doesn't move “{}”: the system needs it.", p.display()));
                    continue;
                }
                srcs.push(p);
            }
            if !srcs.is_empty() {
                let out = ops::run(&ops::Spec { kind, sources: srcs, dest }, &ops::Control::default(), &SkipHost);
                results.extend(out.results.iter().map(|p| s(p)));
                errors.extend(out.errors);
            }
        }
        _ => return Err("usage: files delete PATH... | rename PATH NAME | mkdir DIR NAME | newfile DIR NAME | job KIND DEST SRC...".into()),
    }
    Ok(json!({"results": results, "errors": errors}))
}

/// Why running the helper as root didn't happen or didn't work.
#[derive(Debug, PartialEq)]
pub enum AdminError {
    /// The polkit prompt was dismissed, or the password refused.
    Cancelled,
    /// No pkexec here.
    Unavailable,
    /// The helper ran and failed (its message).
    Failed(String),
}

impl AdminError {
    /// In words, for an action like "Deleting this".
    pub fn words(&self, doing: &str) -> String {
        match self {
            AdminError::Cancelled => "Nothing was changed: the administrator password wasn't given.".into(),
            AdminError::Unavailable => format!("{doing} needs administrator rights, and pkexec (polkit) isn't available."),
            AdminError::Failed(m) => format!("{doing} didn't work: {m}"),
        }
    }
}

/// The command line that runs grove's helper as root: `pkexec GROVE
/// --root-helper ARGS...`. GROVE_PKEXEC replaces pkexec (tests run the
/// helper through `unshare -r` instead; no password, no real root).
pub fn helper_argv(pkexec: Option<&str>, exe: &str, args: &[&str]) -> Vec<String> {
    let mut v = vec![pkexec.filter(|p| !p.is_empty()).unwrap_or("pkexec").to_string(), exe.to_string(), "--root-helper".to_string()];
    v.extend(args.iter().map(|a| a.to_string()));
    v
}

/// What pkexec's exit says: 126 = the prompt was dismissed, 127 = not
/// authorized (wrong password, or polkit said no).
pub fn read_exit(code: Option<i32>, stdout: &[u8], stderr: &[u8]) -> Result<String, AdminError> {
    match code {
        Some(0) => Ok(String::from_utf8_lossy(stdout).into_owned()),
        Some(126) | Some(127) => Err(AdminError::Cancelled),
        _ => {
            let m = String::from_utf8_lossy(stderr).trim().to_string();
            Err(AdminError::Failed(if m.is_empty() { "the helper stopped".into() } else { m }))
        }
    }
}

/// Runs `grove --root-helper ARGS...` as root through polkit (which asks
/// for the password) and waits: call it off the UI thread. Returns what
/// the helper printed.
pub fn as_root(args: &[&str]) -> Result<String, AdminError> {
    let alt = std::env::var("GROVE_PKEXEC").ok();
    let argv = helper_argv(alt.as_deref(), &exe(), args);
    let o = std::process::Command::new(&argv[0]).args(&argv[1..]).stdin(std::process::Stdio::null()).output().map_err(|_| AdminError::Unavailable)?;
    read_exit(o.status.code(), &o.stdout, &o.stderr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_runs_this_helper_through_pkexec() {
        assert_eq!(helper_argv(None, "/bin/grove", &["files", "delete", "/OpenRGB.json"]), ["pkexec", "/bin/grove", "--root-helper", "files", "delete", "/OpenRGB.json"]);
        assert_eq!(helper_argv(Some(""), "g", &[])[0], "pkexec");
        assert_eq!(helper_argv(Some("/t/fake"), "g", &["x"]), ["/t/fake", "g", "--root-helper", "x"]);
    }

    #[test]
    fn exit_codes() {
        assert_eq!(read_exit(Some(0), b"{}", b""), Ok("{}".into()));
        assert_eq!(read_exit(Some(126), b"", b""), Err(AdminError::Cancelled));
        assert_eq!(read_exit(Some(127), b"", b""), Err(AdminError::Cancelled));
        assert_eq!(read_exit(Some(1), b"", b"bad\n"), Err(AdminError::Failed("bad".into())));
        assert!(AdminError::Cancelled.words("Deleting").contains("Nothing was changed"));
    }

    #[test]
    fn system_folders_are_protected() {
        for p in ["/", "/etc", "/etc/", "/nix/store", "/home", "/usr", "/a/../etc"] {
            assert!(protected(Path::new(p)), "{p}");
        }
        for p in ["/OpenRGB.json", "/etc/foo.conf", "/home/x/y", "/opt/app"] {
            assert!(!protected(Path::new(p)), "{p}");
        }
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("grove-roothelper-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn files_ops_in_a_scratch_dir() {
        let d = scratch("ops");
        let ds = d.to_string_lossy().into_owned();
        std::fs::write(d.join("f"), "x").unwrap();
        let r = files(&a(&["rename", &format!("{ds}/f"), "g"])).unwrap();
        assert_eq!(r["results"][0], format!("{ds}/g"));
        let r = files(&a(&["mkdir", &ds, ""])).unwrap();
        assert_eq!(r["results"][0], format!("{ds}/untitled folder"));
        std::fs::create_dir(d.join("to")).unwrap();
        std::fs::write(d.join("to/g"), "keep").unwrap();
        // Already there: kept; a source that's gone: left out.
        let r = files(&a(&["job", "copy", &format!("{ds}/to"), &format!("{ds}/g"), &format!("{ds}/gone")])).unwrap();
        assert_eq!(r["errors"].as_array().unwrap().len(), 0);
        assert_eq!(std::fs::read_to_string(d.join("to/g")).unwrap(), "keep");
        let r = files(&a(&["delete", &format!("{ds}/g"), &format!("{ds}/to"), "/"])).unwrap();
        assert_eq!(r["results"].as_array().unwrap().len(), 2);
        assert_eq!(r["errors"].as_array().unwrap().len(), 1);
        assert!(files(&a(&["delete", "relative"])).is_err());
        assert!(files(&a(&["mkdir", &ds, "../x"])).is_err());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
