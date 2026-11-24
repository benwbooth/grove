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

/// Get Info for something inside an archive: from the archive's listing,
/// with "Where" as a path through the archives (they're folders here).
fn archive_info(path: &str) -> Value {
    let rest = &path["archive://".len()..];
    let (archive, inner) = rest.split_once('#').unwrap_or((rest, ""));
    let inner = inner.trim_matches('/');
    if inner.is_empty() {
        return info(archive);
    }
    let (container, _, leaf) = crate::archive::resolve(archive, inner).unwrap_or((archive.to_string(), String::new(), inner.to_string()));
    let leaf = leaf.trim_matches('/').to_string();
    // The path names a nested archive itself: it's listed in its parent.
    let (entries, leaf) = if leaf.is_empty() {
        let parent_inner = inner.rsplit_once('/').map(|(a, _)| a).unwrap_or("");
        let parent = if parent_inner.is_empty() { archive.to_string() } else { crate::archive::resolve(archive, parent_inner).map(|(c, _, _)| c).unwrap_or(archive.to_string()) };
        let last = inner.rsplit('/').next().unwrap_or(inner).to_string();
        let prefix = parent_inner.to_string();
        (crate::archive::list(&parent).unwrap_or_default(), if prefix.is_empty() || parent != archive { last } else { inner.to_string() })
    } else {
        (crate::archive::list(&container).unwrap_or_default(), leaf)
    };
    let found = entries.iter().find(|e| e.path == leaf).cloned();
    let under = format!("{leaf}/");
    let is_dir = found.as_ref().map(|e| e.is_dir).unwrap_or(false) || entries.iter().any(|e| e.path.starts_with(&under));
    if found.is_none() && !is_dir {
        return json!({"error": "It isn't in the archive any more."});
    }
    let name = inner.rsplit('/').next().unwrap_or(inner).to_string();
    let parent_inner = inner.rsplit_once('/').map(|(a, _)| a).unwrap_or("");
    let where_ = if parent_inner.is_empty() { archive.to_string() } else { format!("{archive}/{parent_inner}") };
    let (size, items) = if is_dir {
        let inside: Vec<_> = entries.iter().filter(|e| e.path.starts_with(&under)).collect();
        (inside.iter().filter(|e| !e.is_dir).map(|e| e.size).sum::<u64>(), crate::archive::children(&entries, &leaf).len())
    } else {
        (found.as_ref().map(|e| e.size).unwrap_or(0), 0)
    };
    let e = grove_core::listing::Entry {
        hidden: name.starts_with('.'),
        name: name.clone(),
        path: path.to_string(),
        is_dir,
        is_link: false,
        size,
        mtime: 0,
        atime: 0,
        ctime: 0,
        btime: 0,
        mode: 0o644,
        uid: 0,
        gid: 0,
        target: String::new(),
        broken: false,
        depth: 0,
        note: String::new(),
        key: String::new(),
        subvol: "",
    };
    let mime = crate::mime::for_entry(&e);
    let mimes = crate::mime::with_parents(&mime.name);
    let def = if is_dir { None } else { grove_core::apps::default_for(&mimes, &grove_core::apps::all()) };
    json!({
        "path": path,
        "name": name,
        "parent": where_,
        "inArchive": true,
        "archive": archive,
        "isDir": is_dir,
        "isLink": false,
        "mime": mime.name, "kind": mime.comment, "icon": crate::mime::icon_for(&e, &mime),
        "isArchive": false,
        "size": size, "sizeText": grove_core::util::human_size(size),
        "items": items,
        "modifiedText": found.map(|e| e.mtime_text).unwrap_or_default(),
        "defaultApp": def.map(|a| json!({"id": a.id, "name": a.name, "icon": a.icon})),
    })
}

pub fn info(path: &str) -> Value {
    if path.starts_with("archive://") {
        return archive_info(path);
    }
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
        "mime": mime.name, "kind": entry.as_ref().filter(|e| !e.subvol.is_empty()).map_or(mime.comment.as_str(), |e| e.subvol), "icon": icon,
        "subvol": entry.as_ref().filter(|e| !e.subvol.is_empty()).and_then(|_| grove_core::btrfs::info(p)),
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

/// The same change as root: grove's own helper through pkexec (polkit asks).
fn pkexec(args: &[&str]) -> Value {
    let exe = crate::roothelper::exe();
    match std::process::Command::new("pkexec").arg(&exe).arg("--root-helper").args(args).output() {
        Ok(o) if o.status.success() => json!({"ok": true, "elevated": true}),
        Ok(o) if o.status.code() == Some(126) || o.status.code() == Some(127) => json!({"error": "Cancelled: the administrator password wasn't given."}),
        Ok(o) => json!({"error": format!("The change didn't work: {}", String::from_utf8_lossy(&o.stderr).trim())}),
        Err(_) => json!({"error": "Changing this needs administrator rights, and pkexec (polkit) isn't available."}),
    }
}

pub fn set_mode(path: &str, mode: u32, recursive: bool) -> Value {
    match crate::roothelper::chmod(Path::new(path), mode, recursive) {
        Ok(()) => return json!({"ok": true}),
        Err(e) if e.kind() != std::io::ErrorKind::PermissionDenied => return json!({"error": grove_core::util::plain_error(&e, Path::new(path))}),
        Err(_) => {}
    }
    let m = format!("{mode:o}");
    pkexec(&["chmod", &m, if recursive { "1" } else { "0" }, path])
}

pub fn set_owner(path: &str, owner: &str, group: &str, recursive: bool) -> Value {
    // Changing just the group to one of yours works without root.
    let mine = owner == user_name(unsafe { libc::getuid() });
    let (o, g) = (if mine { "" } else { owner }, group);
    match crate::roothelper::chown(Path::new(path), o, g, recursive) {
        Ok(()) => return json!({"ok": true}),
        Err(e) if e.kind() != std::io::ErrorKind::PermissionDenied && e.raw_os_error() != Some(libc::EPERM) => {
            return json!({"error": grove_core::util::plain_error(&e, Path::new(path))})
        }
        Err(_) => {}
    }
    pkexec(&["chown", owner, group, if recursive { "1" } else { "0" }, path])
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
