//! The sidebar's places: favourites, locations (home, the computer,
//! mounted disks with capacity and eject, disks not mounted yet via
//! udisks2), network places, tags, smart folders.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

fn name_of(p: &str) -> String {
    let home = grove_core::util::home();
    if Path::new(p) == home {
        return std::env::var("USER").unwrap_or_else(|_| "Home".into());
    }
    if p == "/" {
        return "Computer".into();
    }
    Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.to_string())
}

fn icon_of(p: &str) -> String {
    let e = grove_core::listing::Entry::from_path(Path::new(p));
    match e {
        Ok(e) if e.is_dir => crate::mime::icon_for(&e, &crate::mime::folder()),
        Ok(e) => crate::mime::icon_for(&e, &crate::mime::for_entry(&e)),
        Err(_) => "folder".into(),
    }
}

type Managed = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

fn udisks_objects() -> Option<Managed> {
    let c = zbus::blocking::Connection::system().ok()?;
    let reply = c
        .call_method(Some("org.freedesktop.UDisks2"), "/org/freedesktop/UDisks2", Some("org.freedesktop.DBus.ObjectManager"), "GetManagedObjects", &())
        .ok()?;
    reply.body().deserialize::<Managed>().ok()
}

fn bytes_str(v: Option<&OwnedValue>) -> String {
    let Some(v) = v else { return String::new() };
    let b: Vec<u8> = v.try_clone().ok().and_then(|x| Vec::<u8>::try_from(x).ok()).unwrap_or_default();
    String::from_utf8_lossy(&b).trim_end_matches('\0').to_string()
}

fn str_of(v: Option<&OwnedValue>) -> String {
    v.and_then(|x| x.try_clone().ok()).and_then(|x| String::try_from(x).ok()).unwrap_or_default()
}

fn bool_of(v: Option<&OwnedValue>) -> bool {
    v.and_then(|x| x.try_clone().ok()).and_then(|x| bool::try_from(x).ok()).unwrap_or(false)
}

fn u64_of(v: Option<&OwnedValue>) -> u64 {
    v.and_then(|x| x.try_clone().ok()).and_then(|x| u64::try_from(x).ok()).unwrap_or(0)
}

fn path_of(v: Option<&OwnedValue>) -> String {
    v.and_then(|x| x.try_clone().ok()).and_then(|x| OwnedObjectPath::try_from(x).ok()).map(|p| p.to_string()).unwrap_or_default()
}

/// Filesystems udisks knows that the user could mount (not system ones).
pub fn devices() -> Vec<Value> {
    let Some(objs) = udisks_objects() else { return vec![] };
    let mut out = vec![];
    for (_path, ifaces) in &objs {
        let (Some(block), Some(fs)) = (ifaces.get("org.freedesktop.UDisks2.Block"), ifaces.get("org.freedesktop.UDisks2.Filesystem")) else { continue };
        if bool_of(block.get("HintIgnore")) || bool_of(block.get("HintSystem")) {
            continue;
        }
        let device = bytes_str(block.get("PreferredDevice"));
        let device = if device.is_empty() { bytes_str(block.get("Device")) } else { device };
        let label = str_of(block.get("IdLabel"));
        let size = u64_of(block.get("Size"));
        let drive_path = path_of(block.get("Drive"));
        let drive = objs.iter().find(|(p, _)| p.to_string() == drive_path).and_then(|(_, i)| i.get("org.freedesktop.UDisks2.Drive"));
        let removable = drive.map(|d| bool_of(d.get("Removable")) || bool_of(d.get("Ejectable"))).unwrap_or(false);
        let optical = drive.map(|d| bool_of(d.get("Optical"))).unwrap_or(false);
        let mounts: Vec<Vec<u8>> = fs.get("MountPoints").and_then(|v| v.try_clone().ok()).and_then(|v| Vec::<Vec<u8>>::try_from(v).ok()).unwrap_or_default();
        let mount = mounts.first().map(|m| String::from_utf8_lossy(m).trim_end_matches('\0').to_string()).unwrap_or_default();
        let name = if !label.is_empty() { label } else { format!("{} Volume", grove_core::util::human_size(size)) };
        let fs_type = str_of(block.get("IdType"));
        out.push(json!({"name": name, "device": device, "mount": mount, "size": size, "removable": removable, "fs": fs_type,
            "icon": if optical {"drive-optical"} else if removable {"drive-removable-media-usb|drive-removable-media"} else {"drive-harddisk"}}));
    }
    out
}

pub fn places(favs: &[String]) -> Value {
    let favourites: Vec<Value> = favs.iter().map(|p| json!({"name": name_of(p), "path": p, "icon": icon_of(p), "exists": Path::new(p).exists()})).collect();
    let home = grove_core::util::home().to_string_lossy().into_owned();
    let mut locations = vec![json!({"name": "Computer", "path": "computer:///", "icon": "computer", "kind": "root"})];
    let devs = devices();
    let mut network = vec![];
    for m in grove_core::mounts::user_mounts() {
        let kind = grove_core::mounts::kind(&m);
        let (total, free) = grove_core::mounts::space(&m.mount_point).unwrap_or((0, 0));
        let dev = devs.iter().find(|d| d["mount"] == m.mount_point.as_str());
        let name = dev.and_then(|d| d["name"].as_str().map(String::from)).unwrap_or_else(|| name_of(&m.mount_point));
        // Show All Subvolumes' mount of a disk's top (subvolid=5).
        let name = crate::subvoltop::name_of_mount(&m.mount_point).unwrap_or(name);
        let icon = match kind {
            "sftp" | "smb" | "nfs" => "folder-network|network-server",
            "cloud" => "folder-cloud|network-server",
            "phone" => "phone|smartphone",
            "optical" => "drive-optical",
            _ => dev.and_then(|d| d["icon"].as_str()).unwrap_or("drive-harddisk"),
        };
        let v = json!({"name": name, "path": m.mount_point, "icon": icon, "kind": kind, "total": total, "free": free,
            "ejectable": true, "device": m.device, "fs": m.fs_type});
        if matches!(kind, "sftp" | "smb" | "nfs" | "cloud") {
            network.push(v);
        } else {
            locations.push(v);
        }
    }
    // Phones and e-readers (MTP) not opened yet: click to open.
    for d in crate::mtp::devices() {
        let mp = crate::remote::mount_point(&d.name).to_string_lossy().into_owned();
        if locations.iter().any(|l| l["path"] == mp.as_str()) {
            continue;
        }
        let lower = d.name.to_lowercase();
        let icon = if ["kindle", "kobo", "boox", "pocketbook", "remarkable", "nook"].iter().any(|k| lower.contains(k)) { "ebook-reader|phone|smartphone" } else { "phone|smartphone" };
        locations.push(json!({"name": d.name, "path": "", "device": d.name, "icon": icon, "kind": "mtp", "ejectable": false}));
    }
    // Disks not mounted yet: click to mount.
    for d in &devs {
        if d["mount"].as_str().map(|s| s.is_empty()).unwrap_or(true) {
            locations.push(json!({"name": d["name"], "path": "", "device": d["device"], "icon": d["icon"], "kind": "unmounted", "ejectable": false, "fs": d["fs"]}));
        }
    }
    let tags: Vec<Value> = grove_core::tags::known().into_iter().map(|t| json!({"name": t, "color": grove_core::tags::color_of(&t).unwrap_or("")})).collect();
    let smart: Vec<Value> = grove_core::search::smart_folders().into_iter().map(|s| json!({"name": s.name, "root": s.root, "query": s.query})).collect();
    let servers = crate::remote::saved();
    json!({
        "favourites": favourites,
        "locations": locations,
        "network": network,
        "servers": servers,
        "tags": tags,
        "smart": smart,
        "home": home,
        "trashCount": grove_core::trash::count(),
    })
}

fn find_fs_object(device: &str) -> Option<(String, String)> {
    let objs = udisks_objects()?;
    for (path, ifaces) in &objs {
        let (Some(block), Some(_)) = (ifaces.get("org.freedesktop.UDisks2.Block"), ifaces.get("org.freedesktop.UDisks2.Filesystem")) else { continue };
        let d = bytes_str(block.get("Device"));
        let pd = bytes_str(block.get("PreferredDevice"));
        if d == device || pd == device {
            return Some((path.to_string(), path_of(block.get("Drive"))));
        }
    }
    None
}

/// The filesystem udisks found on `device` ("btrfs"...), system disks too.
pub fn fs_type_of(device: &str) -> String {
    let Some(objs) = udisks_objects() else { return String::new() };
    for (_, ifaces) in &objs {
        let Some(block) = ifaces.get("org.freedesktop.UDisks2.Block") else { continue };
        if bytes_str(block.get("Device")) == device || bytes_str(block.get("PreferredDevice")) == device {
            return str_of(block.get("IdType"));
        }
    }
    String::new()
}

fn opts() -> HashMap<String, zbus::zvariant::Value<'static>> {
    HashMap::new()
}

/// Mounts `device` through udisks with mount options (e.g. "ro,subvolid=5"):
/// the mount point, or udisks' error.
pub fn mount_with_options(device: &str, options: &str) -> Result<String, String> {
    let (obj, _) = find_fs_object(device).ok_or("That disk isn't available any more.")?;
    let c = zbus::blocking::Connection::system().map_err(|_| "Can't reach the disk service (udisks).".to_string())?;
    let mut o = opts();
    o.insert("options".into(), zbus::zvariant::Value::from(options.to_string()));
    let r = c
        .call_method(Some("org.freedesktop.UDisks2"), obj.as_str(), Some("org.freedesktop.UDisks2.Filesystem"), "Mount", &(o,))
        .map_err(|e| plain_dbus(&e))?;
    Ok(r.body().deserialize::<String>().unwrap_or_default())
}

pub fn mount(device: &str) -> Value {
    let Some((obj, _)) = find_fs_object(device) else { return json!({"error": "That disk isn't available any more."}) };
    let Ok(c) = zbus::blocking::Connection::system() else { return json!({"error": "Can't reach the disk service (udisks)."}) };
    match c.call_method(Some("org.freedesktop.UDisks2"), obj.as_str(), Some("org.freedesktop.UDisks2.Filesystem"), "Mount", &(opts(),)) {
        Ok(r) => json!({"ok": true, "path": r.body().deserialize::<String>().unwrap_or_default()}),
        Err(e) => json!({"error": format!("Couldn't open the disk: {}", plain_dbus(&e))}),
    }
}

/// Attach a disk image read-only (UDisks2 LoopSetup over D-Bus, no
/// udisksctl) and mount it, or its first partition: (mount path, device).
pub fn mount_image(path: &str) -> Result<(String, String), String> {
    let c = zbus::blocking::Connection::system().map_err(|_| "Can't reach the disk service (udisks).".to_string())?;
    let f = std::fs::File::open(path).map_err(|e| grove_core::util::plain_error(&e, Path::new(path)))?;
    let mut o: HashMap<String, zbus::zvariant::Value<'static>> = HashMap::new();
    o.insert("read-only".into(), zbus::zvariant::Value::from(true));
    let fd = zbus::zvariant::Fd::from(&f);
    let r = c
        .call_method(Some("org.freedesktop.UDisks2"), "/org/freedesktop/UDisks2/Manager", Some("org.freedesktop.UDisks2.Manager"), "LoopSetup", &(fd, o))
        .map_err(|e| format!("Couldn't open the disk image: {}", plain_dbus(&e)))?;
    let loop_obj: zbus::zvariant::OwnedObjectPath = r.body().deserialize().map_err(|e| e.to_string())?;
    let base = loop_obj.as_str().to_string();
    // udisks probes the image (and its partitions) just after attaching it.
    for _ in 0..20 {
        for obj in [base.clone(), format!("{base}p1")] {
            if let Ok(r) = c.call_method(Some("org.freedesktop.UDisks2"), obj.as_str(), Some("org.freedesktop.UDisks2.Filesystem"), "Mount", &(opts(),)) {
                let at = r.body().deserialize::<String>().unwrap_or_default();
                let dev = obj.rsplit('/').next().map(|d| format!("/dev/{d}")).unwrap_or_default();
                return Ok((at, dev));
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    Err("The disk image was attached but has nothing grove can open (no readable filesystem).".into())
}

fn plain_dbus(e: &zbus::Error) -> String {
    let s = e.to_string();
    if s.contains("target is busy") || s.contains("Busy") {
        "it's still in use by a program. Close files on it and try again.".into()
    } else if s.contains("NotAuthorized") {
        "you don't have permission.".into()
    } else {
        s
    }
}

pub fn eject(mount_point: &str) -> Value {
    // Show All Subvolumes' own mount (held by grove's root helper).
    if let Some(v) = crate::subvoltop::eject(mount_point) {
        return v;
    }
    let m = grove_core::mounts::mounts().into_iter().find(|m| m.mount_point == mount_point);
    let Some(m) = m else { return json!({"ok": true}) };
    if m.fs_type.starts_with("fuse") {
        let r = std::process::Command::new("fusermount3").args(["-u", mount_point]).output().or_else(|_| std::process::Command::new("fusermount").args(["-u", mount_point]).output());
        return match r {
            Ok(o) if o.status.success() => {
                let _ = std::fs::remove_dir(mount_point);
                json!({"ok": true})
            }
            Ok(o) => json!({"error": format!("Couldn't disconnect: {}", String::from_utf8_lossy(&o.stderr).trim())}),
            Err(_) => json!({"error": "fusermount isn't installed."}),
        };
    }
    let Some((obj, drive)) = find_fs_object(&m.device) else {
        // Not udisks' (e.g. a cifs mount from fstab): try umount.
        return match std::process::Command::new("umount").arg(mount_point).output() {
            Ok(o) if o.status.success() => json!({"ok": true}),
            Ok(o) => json!({"error": String::from_utf8_lossy(&o.stderr).trim().to_string()}),
            Err(e) => json!({"error": e.to_string()}),
        };
    };
    let Ok(c) = zbus::blocking::Connection::system() else { return json!({"error": "Can't reach the disk service (udisks)."}) };
    if let Err(e) = c.call_method(Some("org.freedesktop.UDisks2"), obj.as_str(), Some("org.freedesktop.UDisks2.Filesystem"), "Unmount", &(opts(),)) {
        return json!({"error": format!("Couldn't eject: {}", plain_dbus(&e))});
    }
    if !drive.is_empty() && drive != "/" {
        // Removable drives: eject the media / power the stick off.
        let _ = c.call_method(Some("org.freedesktop.UDisks2"), drive.as_str(), Some("org.freedesktop.UDisks2.Drive"), "Eject", &(opts(),));
    }
    json!({"ok": true})
}
