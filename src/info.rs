//! Get Info: facts about a file, and changing its permissions/owner
//! (through pkexec when it isn't yours).

use serde_json::{json, Value};
use std::ffi::CStr;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

pub fn user_name(uid: u32) -> String {
    unsafe {
        let mut pw: libc::passwd = std::mem::zeroed();
        let mut buf = vec![0 as libc::c_char; 4096];
        let mut out: *mut libc::passwd = std::ptr::null_mut();
        if libc::getpwuid_r(uid, &mut pw, buf.as_mut_ptr(), buf.len(), &mut out) == 0 && !out.is_null() {
            return CStr::from_ptr(pw.pw_name).to_string_lossy().into_owned();
        }
    }
    uid.to_string()
}

pub fn group_name(gid: u32) -> String {
    unsafe {
        let mut gr: libc::group = std::mem::zeroed();
        let mut buf = vec![0 as libc::c_char; 8192];
        let mut out: *mut libc::group = std::ptr::null_mut();
        if libc::getgrgid_r(gid, &mut gr, buf.as_mut_ptr(), buf.len(), &mut out) == 0 && !out.is_null() {
            return CStr::from_ptr(gr.gr_name).to_string_lossy().into_owned();
        }
    }
    gid.to_string()
}

pub fn rwx(mode: u32) -> String {
    let mut s = String::new();
    for shift in [6, 3, 0] {
        let b = (mode >> shift) & 7;
        s.push(if b & 4 != 0 { 'r' } else { '-' });
        s.push(if b & 2 != 0 { 'w' } else { '-' });
        s.push(if b & 1 != 0 { 'x' } else { '-' });
    }
    s
}

fn writable(p: &Path) -> bool {
    let Ok(c) = std::ffi::CString::new(p.to_string_lossy().as_bytes()) else { return false };
    unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

pub fn info(path: &str) -> Value {
    let p = Path::new(path);
    let lm = match std::fs::symlink_metadata(p) {
        Ok(m) => m,
        Err(e) => return json!({"error": grove_core::util::plain_error(&e, p)}),
    };
    let m = std::fs::metadata(p).unwrap_or(lm.clone());
    let mime = if m.is_dir() { crate::mime::folder().as_ref().clone() } else { crate::mime::deep(path) };
    let mimes = crate::mime::with_parents(&mime.name);
    let all = grove_core::apps::all();
    let def = if m.is_dir() { None } else { grove_core::apps::default_for(&mimes, &all) };
    let btime = m.created().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (total, free) = grove_core::mounts::space(path).unwrap_or((0, 0));
    let mount = grove_core::trash::mount_top(p);
    let items = if m.is_dir() { grove_core::listing::count_children(p, true).unwrap_or(0) } else { 0 };
    let entry = grove_core::listing::Entry::from_path(p).ok();
    let icon = entry.as_ref().map(|e| crate::mime::icon_for(e, &mime)).unwrap_or_default();
    json!({
        "path": path,
        "name": p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string()),
        "parent": p.parent().map(|d| d.display().to_string()).unwrap_or_default(),
        "isDir": m.is_dir(),
        "isLink": lm.file_type().is_symlink(),
        "target": std::fs::read_link(p).map(|t| t.display().to_string()).unwrap_or_default(),
        "mime": mime.name, "kind": mime.comment, "icon": icon,
        "isArchive": !m.is_dir() && crate::archive::is_archive(&mime.name, &p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
        "size": m.len(), "sizeText": grove_core::util::human_size(m.len()),
        "blocks": m.blocks() * 512,
        "items": items,
        "mtime": m.mtime(), "atime": m.atime(), "ctime": m.ctime(), "btime": btime,
        "mode": m.permissions().mode() & 0o7777,
        "rwx": rwx(m.permissions().mode()),
        "uid": m.uid(), "gid": m.gid(), "owner": user_name(m.uid()), "group": group_name(m.gid()),
        "mine": m.uid() == unsafe { libc::getuid() },
        "writable": writable(p),
        "tags": grove_core::tags::get(p),
        "rating": grove_core::tags::rating(p),
        "customIcon": if m.is_dir() { crate::mime::custom_folder_icon(p) } else { String::new() },
        "comment": grove_core::tags::comment(p),
        "defaultApp": def.map(|a| json!({"id": a.id, "name": a.name, "icon": a.icon})),
        "mount": mount.display().to_string(),
        "diskTotal": total, "diskFree": free,
    })
}

fn pkexec(args: &[&str]) -> Value {
    match std::process::Command::new("pkexec").args(args).output() {
        Ok(o) if o.status.success() => json!({"ok": true, "elevated": true}),
        Ok(o) if o.status.code() == Some(126) => json!({"error": "Cancelled: the administrator password wasn't given."}),
        Ok(o) => json!({"error": format!("The change didn't work: {}", String::from_utf8_lossy(&o.stderr).trim())}),
        Err(_) => json!({"error": "Changing this needs administrator rights, and pkexec (polkit) isn't available."}),
    }
}

pub fn set_mode(path: &str, mode: u32, recursive: bool) -> Value {
    let p = Path::new(path);
    if !recursive {
        match std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)) {
            Ok(()) => return json!({"ok": true}),
            Err(e) if e.kind() != std::io::ErrorKind::PermissionDenied => return json!({"error": grove_core::util::plain_error(&e, p)}),
            Err(_) => {}
        }
    } else {
        let ok = std::process::Command::new("chmod").args(["-R", &format!("{mode:o}"), path]).status().map(|s| s.success()).unwrap_or(false);
        if ok {
            return json!({"ok": true});
        }
    }
    let m = format!("{mode:o}");
    let mut args = vec!["chmod"];
    if recursive {
        args.push("-R");
    }
    args.push(&m);
    args.push(path);
    pkexec(&args)
}

pub fn set_owner(path: &str, owner: &str, group: &str, recursive: bool) -> Value {
    let spec = if group.is_empty() { owner.to_string() } else { format!("{owner}:{group}") };
    let mut args = vec!["chown"];
    if recursive {
        args.push("-R");
    }
    args.push(&spec);
    args.push(path);
    // Changing just the group to one of yours works without root.
    if owner == user_name(unsafe { libc::getuid() }) {
        let ok = std::process::Command::new(args[0]).args(&args[1..]).status().map(|s| s.success()).unwrap_or(false);
        if ok {
            return json!({"ok": true});
        }
    }
    pkexec(&args)
}

/// User/group names for list columns (cached: looked up once per id).
pub fn user_name_cached(uid: u32) -> String {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static C: OnceLock<Mutex<HashMap<u32, String>>> = OnceLock::new();
    let c = C.get_or_init(Default::default);
    if let Some(n) = c.lock().unwrap().get(&uid) {
        return n.clone();
    }
    let n = user_name(uid);
    c.lock().unwrap().insert(uid, n.clone());
    n
}

pub fn group_name_cached(gid: u32) -> String {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static C: OnceLock<Mutex<HashMap<u32, String>>> = OnceLock::new();
    let c = C.get_or_init(Default::default);
    if let Some(n) = c.lock().unwrap().get(&gid) {
        return n.clone();
    }
    let n = group_name(gid);
    c.lock().unwrap().insert(gid, n.clone());
    n
}
