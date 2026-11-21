//! Burn to Disc (Finder's File › Burn): files and folders onto a CD, DVD or
//! Blu-ray with xorriso, or into an ISO image file. Drives come from sysfs
//! (no device is opened to list them). Progress comes from xorriso's
//! UPDATE lines.

use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
    /// The drive goes through the motions without writing (xorriso -dummy).
    #[serde(default)]
    pub simulate: bool,
    /// Don't run anything: answer with the command (tests).
    #[serde(default)]
    pub dry_run: bool,
}

/// ISO 9660 volume names: up to 32 of A-Z 0-9 _.
pub fn volume_id(name: &str) -> String {
    let v: String = name.chars().map(|c| c.to_ascii_uppercase()).map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).take(32).collect();
    if v.trim_matches('_').is_empty() { "DISC".into() } else { v }
}

/// The xorriso command line for a request.
pub fn command(r: &Request) -> Result<Vec<String>, String> {
    if r.paths.is_empty() {
        return Err("There's nothing to burn.".into());
    }
    let mut a: Vec<String> = vec!["xorriso".into(), "-abort_on".into(), "FAILURE".into(), "-report_about".into(), "UPDATE".into()];
    if r.device.is_empty() {
        if r.iso.is_empty() {
            return Err("Choose a disc burner or a file to save the disc image in.".into());
        }
        a.extend(["-outdev".into(), format!("stdio:{}", r.iso)]);
    } else {
        a.extend(["-outdev".into(), r.device.clone(), "-blank".into(), "as_needed".into()]);
        if r.simulate {
            a.extend(["-dummy".into(), "on".into()]);
        }
    }
    let vol = if r.volid.is_empty() { volume_id(Path::new(&r.paths[0]).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().as_str()) } else { volume_id(&r.volid) };
    a.extend(["-volid".into(), vol, "-joliet".into(), "on".into(), "-rockridge".into(), "on".into()]);
    // One folder: its contents at the top of the disc (Finder burns a
    // folder that way). Otherwise each item by name.
    let single_dir = r.paths.len() == 1 && Path::new(&r.paths[0]).is_dir();
    for p in &r.paths {
        let target = if single_dir { "/".to_string() } else { format!("/{}", Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()) };
        a.extend(["-map".into(), p.clone(), target]);
    }
    a.push("-commit".into());
    if !r.device.is_empty() && r.eject {
        a.extend(["-eject".into(), "all".into()]);
    }
    Ok(a)
}

/// "xorriso : UPDATE :  52.38% done" -> 0.5238.
pub fn parse_progress(line: &str) -> Option<f64> {
    let i = line.find("% done")?;
    let head = &line[..i];
    let num: String = head.chars().rev().take_while(|c| c.is_ascii_digit() || *c == '.').collect::<Vec<_>>().into_iter().rev().collect();
    num.parse::<f64>().ok().map(|p| (p / 100.0).clamp(0.0, 1.0))
}

/// Run a burn, calling `progress(fraction, line)`; Err has xorriso's reason.
pub fn run(r: &Request, cancel: &std::sync::atomic::AtomicBool, mut progress: impl FnMut(f64, &str)) -> Result<(), String> {
    let argv = command(r)?;
    if !r.iso.is_empty() {
        if let Some(d) = PathBuf::from(&r.iso).parent() {
            let _ = std::fs::create_dir_all(d);
        }
        // xorriso adds to an existing image; start afresh.
        let _ = std::fs::remove_file(&r.iso);
    }
    let mut child = Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { "Burning needs xorriso, which isn't installed.".to_string() } else { e.to_string() })?;
    let err = child.stderr.take().unwrap();
    let mut last_problem = String::new();
    for line in BufReader::new(err).lines().map_while(Result::ok) {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Cancelled".into());
        }
        if let Some(f) = parse_progress(&line) {
            progress(f, "");
        } else if line.contains("FAILURE") || line.contains("SORRY") || line.contains("FATAL") {
            last_problem = line.split(" : ").last().unwrap_or(&line).trim().to_string();
        } else if line.contains("Blanking") || line.contains("Writing") || line.contains("Closing") {
            progress(-1.0, line.split(" : ").last().unwrap_or("").trim());
        }
    }
    let st = child.wait().map_err(|e| e.to_string())?;
    if st.success() {
        Ok(())
    } else if last_problem.is_empty() {
        Err("The burn didn't finish (xorriso stopped with an error).".into())
    } else {
        Err(explain(&last_problem))
    }
}

/// xorriso's words, in plainer ones where it's a common case.
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
    fn progress_lines() {
        assert_eq!(parse_progress("xorriso : UPDATE :  52.38% done"), Some(0.5238));
        assert!(parse_progress("xorriso : UPDATE : 1 MB written").is_none());
    }
    #[test]
    fn commands() {
        let r = Request { paths: vec!["/tmp".into()], iso: "/x/out.iso".into(), ..Default::default() };
        let c = command(&r).unwrap();
        assert!(c.windows(2).any(|w| w[0] == "-outdev" && w[1] == "stdio:/x/out.iso"));
        assert!(c.windows(3).any(|w| w[0] == "-map" && w[1] == "/tmp" && w[2] == "/"));
        let r = Request { paths: vec!["/a/b.txt".into(), "/a/c".into()], device: "/dev/sr0".into(), simulate: true, eject: true, ..Default::default() };
        let c = command(&r).unwrap();
        assert!(c.contains(&"-dummy".to_string()) && c.contains(&"-eject".to_string()));
        assert!(c.windows(3).any(|w| w[0] == "-map" && w[2] == "/b.txt"));
        assert!(command(&Request::default()).is_err());
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
