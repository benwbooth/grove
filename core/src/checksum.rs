//! File checksums (Dolphin's Properties › Checksums): MD5, SHA-1, SHA-256,
//! SHA-512, read in 1 MB pieces so a big file can be cancelled.

use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub fn compute(path: &Path, algo: &str, cancel: &AtomicBool) -> Result<String, String> {
    use md5::Digest;
    let mut f = std::fs::File::open(path).map_err(|e| crate::util::plain_error(&e, path))?;
    let mut buf = vec![0u8; 1 << 20];
    macro_rules! run {
        ($h:expr) => {{
            let mut h = $h;
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Cancelled".into());
                }
                let n = f.read(&mut buf).map_err(|e| crate::util::plain_error(&e, path))?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
            }
            h.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>()
        }};
    }
    Ok(match algo {
        "md5" => run!(md5::Md5::new()),
        "sha1" => run!(sha1::Sha1::new()),
        "sha256" => run!(sha2::Sha256::new()),
        "sha512" => run!(sha2::Sha512::new()),
        _ => return Err(format!("Unknown checksum “{algo}”.")),
    })
}

/// Which algorithm a pasted checksum looks like (by length).
pub fn guess(sum: &str) -> Option<&'static str> {
    let s = sum.trim();
    if !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match s.len() {
        32 => Some("md5"),
        40 => Some("sha1"),
        64 => Some("sha256"),
        128 => Some("sha512"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sums() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("a");
        std::fs::write(&f, "abc").unwrap();
        let c = AtomicBool::new(false);
        assert_eq!(compute(&f, "md5", &c).unwrap(), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(compute(&f, "sha1", &c).unwrap(), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(compute(&f, "sha256", &c).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(guess("900150983cd24fb0d6963f7d28e17f72"), Some("md5"));
        assert_eq!(guess("xyz"), None);
    }
}
