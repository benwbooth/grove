//! Burn to Disc (Finder's File › Burn): files and folders onto a CD, DVD or
//! Blu-ray, or into an ISO image file, through libisofs + libburn
//! (grove_core::burnlib; no xorriso). Drives come from sysfs (no device is
//! opened to list them).

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Drive {
    pub dev: String,
    pub name: String,
    pub can_write: bool,
}

/// Optical drives (/sys/class/block/sr*).
pub fn drives() -> Vec<Drive> {
    drives_in(Path::new("/sys/class/block"))
}

pub fn drives_in(sys: &Path) -> Vec<Drive> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(sys) else { return out };
    for e in rd.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        if !n.starts_with("sr") {
            continue;
        }
        let read = |f: &str| std::fs::read_to_string(e.path().join("device").join(f)).map(|s| s.trim().to_string()).unwrap_or_default();
        let name = format!("{} {}", read("vendor"), read("model")).trim().to_string();
        // "ro" is 1 for read-only drives (CD-ROM only).
        let ro = std::fs::read_to_string(e.path().join("ro")).map(|s| s.trim() == "1").unwrap_or(false);
        out.push(Drive { dev: format!("/dev/{n}"), name: if name.is_empty() { n.clone() } else { name }, can_write: !ro });
    }
    out.sort_by(|a, b| a.dev.cmp(&b.dev));
    out
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Request {
    pub paths: Vec<String>,
    /// A drive (/dev/sr0), or "" to write `iso`.
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub iso: String,
    #[serde(default)]
    pub volid: String,
    #[serde(default)]
    pub eject: bool,
    /// The drive goes through the motions without writing.
    #[serde(default)]
    pub simulate: bool,
    /// Don't burn: answer with what would go on the disc (tests).
    #[serde(default)]
    pub dry_run: bool,
}

/// ISO 9660 volume names: up to 32 of A-Z 0-9 _.
pub fn volume_id(name: &str) -> String {
    let v: String = name.chars().map(|c| c.to_ascii_uppercase()).map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).take(32).collect();
    if v.trim_matches('_').is_empty() { "DISC".into() } else { v }
}

/// What goes on the disc: each item by name, or one folder's contents at
/// the top (Finder burns a folder that way).
pub fn layout(r: &Request) -> Result<Vec<(String, String)>, String> {
    if r.paths.is_empty() {
        return Err("There's nothing to burn.".into());
    }
    if r.device.is_empty() && r.iso.is_empty() {
        return Err("Choose a disc burner or a file to save the disc image in.".into());
    }
    let single_dir = r.paths.len() == 1 && Path::new(&r.paths[0]).is_dir();
    Ok(r.paths
        .iter()
        .map(|p| {
            let t = if single_dir { "/".to_string() } else { format!("/{}", Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()) };
            (p.clone(), t)
        })
        .collect())
}

/// Run a burn, calling `progress(fraction, phase)` (fraction -1: phase only).
pub fn run(r: &Request, cancel: &std::sync::atomic::AtomicBool, mut progress: impl FnMut(f64, &str)) -> Result<(), String> {
    let items = layout(r)?;
    if !r.iso.is_empty() {
        if let Some(d) = PathBuf::from(&r.iso).parent() {
            let _ = std::fs::create_dir_all(d);
        }
    }
    let vol = if r.volid.is_empty() { volume_id(Path::new(&r.paths[0]).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().as_str()) } else { volume_id(&r.volid) };
    let job = grove_core::burnlib::Job { items, volid: &vol, device: &r.device, iso: &r.iso, simulate: r.simulate, eject: r.eject };
    grove_core::burnlib::burn(&job, cancel, |f, phase| progress(f, phase)).map_err(|e| if e == "Cancelled" { e } else { explain(&e) })
}

/// libburn's words, in plainer ones where it's a common case.
fn explain(p: &str) -> String {
    let l = p.to_lowercase();
    if l.contains("no media") || l.contains("no medium") {
        "There's no disc in the drive. Insert a blank (or rewritable) disc and try again.".into()
    } else if l.contains("not writable") || l.contains("closed media") || l.contains("is not blank") {
        "The disc in the drive can't be written: it's full or closed. Use a blank or rewritable disc.".into()
    } else if l.contains("permission denied") || l.contains("cannot open") {
        "grove isn't allowed to use the drive. Your user needs to be in the cdrom (or optical) group.".into()
    } else if l.contains("too large") || l.contains("not enough") {
        "These items don't fit on the disc.".into()
    } else {
        p.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn volume_ids() {
        assert_eq!(volume_id("My Photos 2024"), "MY_PHOTOS_2024");
        assert_eq!(volume_id(""), "DISC");
        assert_eq!(volume_id(&"x".repeat(40)).len(), 32);
    }
    #[test]
    fn layouts() {
        let r = Request { paths: vec!["/tmp".into()], iso: "/x/out.iso".into(), ..Default::default() };
        assert_eq!(layout(&r).unwrap(), vec![("/tmp".to_string(), "/".to_string())]);
        let r = Request { paths: vec!["/a/b.txt".into(), "/a/c".into()], device: "/dev/sr0".into(), ..Default::default() };
        assert_eq!(layout(&r).unwrap()[0].1, "/b.txt");
        assert!(layout(&Request::default()).is_err());
    }
    #[test]
    fn no_drives_in_empty_sysfs() {
        let t = std::env::temp_dir().join("grove-burn-test-sys");
        let _ = std::fs::create_dir_all(t.join("sda/device"));
        assert!(drives_in(&t).is_empty());
        let _ = std::fs::create_dir_all(t.join("sr0/device"));
        std::fs::write(t.join("sr0/device/vendor"), "HL-DT-ST\n").unwrap();
        std::fs::write(t.join("sr0/device/model"), "BD-RE BH16NS40\n").unwrap();
        let d = drives_in(&t);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].dev, "/dev/sr0");
        assert_eq!(d[0].name, "HL-DT-ST BD-RE BH16NS40");
        let _ = std::fs::remove_dir_all(&t);
    }
}
