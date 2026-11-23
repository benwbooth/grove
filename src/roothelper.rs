//! Permission and owner changes done by grove itself (no chmod/chown
//! commands), recursively without following symlinks. When the files
//! aren't yours, `pkexec grove --root-helper ...` runs this same code as
//! root (polkit asks for the password); nothing else runs as root.

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

/// `grove --root-helper chmod MODE R PATH` / `chown OWNER GROUP R PATH`
/// (R is 1 for recursive). Returns the exit code.
pub fn run(args: &[String]) -> i32 {
    let r = match args {
        [op, mode, rec, path] if op == "chmod" => match u32::from_str_radix(mode, 8) {
            Ok(m) => chmod(Path::new(path), m, rec == "1"),
            Err(_) => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "bad mode")),
        },
        [op, owner, group, rec, path] if op == "chown" => chown(Path::new(path), owner, group, rec == "1"),
        _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "usage: --root-helper chmod MODE 0|1 PATH | chown OWNER GROUP 0|1 PATH")),
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
