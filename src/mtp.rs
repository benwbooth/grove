//! Phones, e-readers and cameras that share files over MTP (Media Transfer
//! Protocol: Android phones, newer Kindles, Boox readers...), found by
//! looking at the USB devices in sysfs: an interface that is MTP (still
//! image class 6/1, or one that names itself "MTP"). Named by their maker
//! and product ("Amazon Kindle"). They show in the sidebar by themselves;
//! opening one mounts it with simple-mtpfs (remote.rs).

use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    /// "Amazon Kindle" (maker and product), or the product alone.
    pub name: String,
    pub busnum: u32,
    pub devnum: u32,
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).map(|s| s.trim().to_string()).unwrap_or_default()
}

/// A USB device folder in sysfs is an MTP device.
fn is_mtp(dev: &Path) -> bool {
    let Some(base) = dev.file_name().map(|n| n.to_string_lossy().into_owned()) else { return false };
    let Ok(rd) = std::fs::read_dir(dev) else { return false };
    for e in rd.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        // Its interfaces: "<device>:<config>.<interface>".
        if !n.starts_with(&format!("{base}:")) {
            continue;
        }
        let p = e.path();
        let (class, sub) = (read(&p.join("bInterfaceClass")), read(&p.join("bInterfaceSubClass")));
        if class == "06" && sub == "01" {
            return true;
        }
        if read(&p.join("interface")).to_ascii_uppercase().contains("MTP") {
            return true;
        }
    }
    false
}

/// MTP devices plugged in now, in USB order.
pub fn devices() -> Vec<Device> {
    // (Tests: GROVE_TEST_USB_SYSFS, a made-up copy of the sysfs folder.)
    match std::env::var_os("GROVE_TEST_USB_SYSFS") {
        Some(p) => devices_in(Path::new(&p)),
        None => devices_in(Path::new("/sys/bus/usb/devices")),
    }
}

pub fn devices_in(sys: &Path) -> Vec<Device> {
    let Ok(rd) = std::fs::read_dir(sys) else { return vec![] };
    let mut out = vec![];
    for e in rd.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        // Devices ("1-2", "3-1.4"), not interfaces ("1-2:1.0") or hubs ("usb1").
        if n.contains(':') || n.starts_with("usb") {
            continue;
        }
        let p = e.path();
        if !is_mtp(&p) {
            continue;
        }
        let (maker, product) = (read(&p.join("manufacturer")), read(&p.join("product")));
        let name = if product.is_empty() {
            if maker.is_empty() { "Phone".to_string() } else { maker }
        } else if maker.is_empty() || product.to_lowercase().starts_with(&maker.to_lowercase()) {
            // "Amazon" + "Kindle", but not "Google" + "Google Pixel 8".
            product
        } else {
            format!("{maker} {product}")
        };
        let num = |f: &str| read(&p.join(f)).parse::<u32>().unwrap_or(0);
        out.push(Device { name, busnum: num("busnum"), devnum: num("devnum") });
    }
    out.sort_by_key(|d| (d.busnum, d.devnum));
    // Two of the same kind: number them, so each has its own name.
    let names: Vec<String> = out.iter().map(|d| d.name.clone()).collect();
    for (i, d) in out.iter_mut().enumerate() {
        let same = names.iter().filter(|n| **n == d.name).count();
        if same > 1 {
            let k = names[..i].iter().filter(|n| **n == d.name).count() + 1;
            d.name = format!("{} {k}", d.name);
        }
    }
    out
}

/// The number simple-mtpfs gives the device called `name` (its own list:
/// "1: Amazon Kindle"), else its place in USB order.
pub fn device_number(name: &str, list_output: &str) -> Option<String> {
    // (Compared without spaces or case: simple-mtpfs prints "AmazonKindle".)
    let squash = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    let want = squash(name);
    for line in list_output.lines() {
        let Some((n, rest)) = line.split_once(':') else { continue };
        let n = n.trim();
        if n.parse::<u32>().is_err() {
            continue;
        }
        let rest = squash(rest);
        if !rest.is_empty() && (want.contains(&rest) || rest.contains(&want)) {
            return Some(n.to_string());
        }
    }
    let pos = devices().iter().position(|d| d.name == name)?;
    Some((pos + 1).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dev(sys: &Path, name: &str, maker: &str, product: &str, ifaces: &[(&str, &str, &str)], bus: u32, num: u32) {
        let d = sys.join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("manufacturer"), maker).unwrap();
        std::fs::write(d.join("product"), product).unwrap();
        std::fs::write(d.join("busnum"), bus.to_string()).unwrap();
        std::fs::write(d.join("devnum"), num.to_string()).unwrap();
        for (i, (class, sub, label)) in ifaces.iter().enumerate() {
            let f = d.join(format!("{name}:1.{i}"));
            std::fs::create_dir_all(&f).unwrap();
            std::fs::write(f.join("bInterfaceClass"), class).unwrap();
            std::fs::write(f.join("bInterfaceSubClass"), sub).unwrap();
            std::fs::write(f.join("interface"), label).unwrap();
        }
    }
    #[test]
    fn finds_mtp_devices() {
        let dir = std::env::temp_dir().join(format!("grove-mtp-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = dir.as_path();
        dev(s, "3-2", "Amazon", "Kindle", &[("ff", "ff", "MTP")], 3, 7);
        dev(s, "1-4", "Google", "Pixel 8", &[("06", "01", "")], 1, 5);
        dev(s, "1-5", "Logitech", "USB Receiver", &[("03", "01", "")], 1, 6);
        dev(s, "2-1", "SanDisk", "Cruzer", &[("08", "06", "")], 2, 2);
        let d = devices_in(s);
        let names: Vec<&str> = d.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["Google Pixel 8", "Amazon Kindle"]);
        assert_eq!(device_number("Amazon Kindle", "Available devices:\n1: GooglePixel 8\n2: AmazonKindle"), Some("2".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
