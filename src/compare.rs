//! Compare two files or folders (no diff command): unified diffs of text
//! files (similar crate), "Only in" lines for what one side lacks, and
//! "Binary files ... differ" for the rest, as `diff -u -r` prints them.

use std::path::Path;

fn is_binary(b: &[u8]) -> bool {
    b.iter().take(8192).any(|&c| c == 0)
}

fn file_diff(a: &Path, b: &Path, out: &mut String) -> std::io::Result<bool> {
    let (x, y) = (std::fs::read(a)?, std::fs::read(b)?);
    if x == y {
        return Ok(true);
    }
    if is_binary(&x) || is_binary(&y) {
        out.push_str(&format!("Binary files {} and {} differ\n", a.display(), b.display()));
        return Ok(false);
    }
    let (xs, ys) = (String::from_utf8_lossy(&x), String::from_utf8_lossy(&y));
    let d = similar::TextDiff::from_lines(xs.as_ref(), ys.as_ref());
    out.push_str(&format!("diff -u {} {}\n", a.display(), b.display()));
    out.push_str(&d.unified_diff().context_radius(3).header(&a.display().to_string(), &b.display().to_string()).to_string());
    Ok(false)
}

fn dir_diff(a: &Path, b: &Path, out: &mut String) -> std::io::Result<bool> {
    let names = |d: &Path| -> std::io::Result<std::collections::BTreeSet<std::ffi::OsString>> { Ok(std::fs::read_dir(d)?.flatten().map(|e| e.file_name()).collect()) };
    let (na, nb) = (names(a)?, names(b)?);
    let mut same = true;
    for n in na.union(&nb) {
        let (pa, pb) = (a.join(n), b.join(n));
        match (na.contains(n), nb.contains(n)) {
            (true, false) => {
                out.push_str(&format!("Only in {}: {}\n", a.display(), n.to_string_lossy()));
                same = false;
            }
            (false, true) => {
                out.push_str(&format!("Only in {}: {}\n", b.display(), n.to_string_lossy()));
                same = false;
            }
            _ => {
                let (ma, mb) = (std::fs::symlink_metadata(&pa)?, std::fs::symlink_metadata(&pb)?);
                let r = if ma.is_dir() && mb.is_dir() {
                    dir_diff(&pa, &pb, out)?
                } else if ma.is_file() && mb.is_file() {
                    file_diff(&pa, &pb, out)?
                } else if ma.file_type().is_symlink() && mb.file_type().is_symlink() {
                    std::fs::read_link(&pa)? == std::fs::read_link(&pb)?
                } else {
                    out.push_str(&format!("File {} is a {} while file {} is a {}\n", pa.display(), kind(&ma), pb.display(), kind(&mb)));
                    false
                };
                same &= r;
            }
        }
    }
    Ok(same)
}

fn kind(m: &std::fs::Metadata) -> &'static str {
    if m.is_dir() {
        "directory"
    } else if m.file_type().is_symlink() {
        "symbolic link"
    } else {
        "regular file"
    }
}

/// (same, the differences as `diff -u -r` text).
pub fn compare(a: &Path, b: &Path) -> std::io::Result<(bool, String)> {
    let mut out = String::new();
    let same = if a.is_dir() && b.is_dir() { dir_diff(a, b, &mut out)? } else { file_diff(a, b, &mut out)? };
    Ok((same, out))
}
