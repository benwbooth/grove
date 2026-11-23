//! libarchive, linked (no bsdtar/bsdcat): list archives, extract all or
//! some members, read one member, decompress a lone compressed file, and
//! write zip / tar.zst archives. Every format libarchive reads: zip, tar
//! with any compression, 7z, rar, cab, iso/udf, cpio, ar/deb, rpm, xar,
//! lha, warc...

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[repr(C)]
struct Archive {
    _p: [u8; 0],
}
#[repr(C)]
struct Entry {
    _p: [u8; 0],
}

const OK: c_int = 0;
const EOF: c_int = 1;
const WARN: c_int = -20;
const AE_IFMT: u32 = 0o170000;
const AE_IFDIR: u32 = 0o040000;
const AE_IFREG: u32 = 0o100000;
const EXTRACT_PERM: c_int = 0x0002;
const EXTRACT_TIME: c_int = 0x0004;
const EXTRACT_SECURE_SYMLINKS: c_int = 0x0100;
const EXTRACT_SECURE_NODOTDOT: c_int = 0x0200;

#[link(name = "archive")]
extern "C" {
    fn archive_read_new() -> *mut Archive;
    fn archive_read_support_filter_all(a: *mut Archive) -> c_int;
    fn archive_read_support_format_all(a: *mut Archive) -> c_int;
    fn archive_read_support_format_raw(a: *mut Archive) -> c_int;
    fn archive_read_open_filename(a: *mut Archive, name: *const c_char, block: usize) -> c_int;
    fn archive_read_next_header(a: *mut Archive, e: *mut *mut Entry) -> c_int;
    fn archive_read_data(a: *mut Archive, buf: *mut c_void, len: usize) -> isize;
    fn archive_read_data_skip(a: *mut Archive) -> c_int;
    fn archive_read_extract2(a: *mut Archive, e: *mut Entry, to: *mut Archive) -> c_int;
    fn archive_read_free(a: *mut Archive) -> c_int;
    fn archive_error_string(a: *mut Archive) -> *const c_char;

    fn archive_write_disk_new() -> *mut Archive;
    fn archive_write_disk_set_options(a: *mut Archive, flags: c_int) -> c_int;
    fn archive_write_disk_set_standard_lookup(a: *mut Archive) -> c_int;

    fn archive_write_new() -> *mut Archive;
    fn archive_write_set_format_zip(a: *mut Archive) -> c_int;
    fn archive_write_set_format_pax_restricted(a: *mut Archive) -> c_int;
    fn archive_write_add_filter_zstd(a: *mut Archive) -> c_int;
    fn archive_write_open_filename(a: *mut Archive, name: *const c_char) -> c_int;
    fn archive_write_header(a: *mut Archive, e: *mut Entry) -> c_int;
    fn archive_write_data(a: *mut Archive, buf: *const c_void, len: usize) -> isize;
    fn archive_write_close(a: *mut Archive) -> c_int;
    fn archive_write_free(a: *mut Archive) -> c_int;

    fn archive_read_disk_new() -> *mut Archive;
    fn archive_read_disk_set_standard_lookup(a: *mut Archive) -> c_int;
    fn archive_read_disk_open(a: *mut Archive, name: *const c_char) -> c_int;
    fn archive_read_next_header2(a: *mut Archive, e: *mut Entry) -> c_int;
    fn archive_read_disk_descend(a: *mut Archive) -> c_int;
    fn archive_read_close(a: *mut Archive) -> c_int;

    fn archive_entry_new() -> *mut Entry;
    fn archive_entry_clear(e: *mut Entry) -> *mut Entry;
    fn archive_entry_free(e: *mut Entry);
    fn archive_entry_pathname(e: *mut Entry) -> *const c_char;
    fn archive_entry_pathname_utf8(e: *mut Entry) -> *const c_char;
    fn archive_entry_set_pathname(e: *mut Entry, p: *const c_char);
    fn archive_entry_hardlink(e: *mut Entry) -> *const c_char;
    fn archive_entry_set_hardlink(e: *mut Entry, p: *const c_char);
    fn archive_entry_size(e: *mut Entry) -> i64;
    fn archive_entry_filetype(e: *mut Entry) -> u32;
    fn archive_entry_mtime(e: *mut Entry) -> i64;
}

/// One member: its path inside (no "./", no trailing "/"), size, folder or
/// not, and modification time (seconds).
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
    pub mtime: i64,
}

fn cstr(p: &Path) -> Result<CString, String> {
    CString::new(p.as_os_str().as_bytes()).map_err(|_| "A path with a NUL byte".to_string())
}

fn err(a: *mut Archive, what: &str) -> String {
    let s = unsafe { archive_error_string(a) };
    if s.is_null() {
        what.to_string()
    } else {
        format!("{what}: {}", unsafe { CStr::from_ptr(s) }.to_string_lossy())
    }
}

fn entry_path(e: *mut Entry) -> String {
    let p = unsafe { archive_entry_pathname_utf8(e) };
    let p = if p.is_null() { unsafe { archive_entry_pathname(e) } } else { p };
    if p.is_null() {
        return String::new();
    }
    let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
    clean(&s)
}

fn clean(s: &str) -> String {
    let mut s = s;
    while let Some(r) = s.strip_prefix("./") {
        s = r;
    }
    s.trim_start_matches('/').trim_end_matches('/').to_string()
}

/// An archive open for reading (freed on drop).
struct Reader(*mut Archive);
impl Drop for Reader {
    fn drop(&mut self) {
        unsafe { archive_read_free(self.0) };
    }
}
unsafe impl Send for Reader {}

fn open(path: &Path, raw: bool) -> Result<Reader, String> {
    let c = cstr(path)?;
    let a = unsafe { archive_read_new() };
    if a.is_null() {
        return Err("Out of memory".into());
    }
    let r = Reader(a);
    unsafe {
        archive_read_support_filter_all(a);
        if raw {
            archive_read_support_format_raw(a);
        } else {
            archive_read_support_format_all(a);
        }
        if archive_read_open_filename(a, c.as_ptr(), 1 << 16) != OK {
            return Err(err(a, "This archive can't be read"));
        }
    }
    Ok(r)
}

/// Walk the headers; `f` gets each entry and says whether to keep going.
fn each(r: &Reader, cancel: &AtomicBool, mut f: impl FnMut(*mut Entry) -> Result<bool, String>) -> Result<(), String> {
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        let mut e: *mut Entry = std::ptr::null_mut();
        let rc = unsafe { archive_read_next_header(r.0, &mut e) };
        if rc == EOF {
            return Ok(());
        }
        if rc < WARN {
            return Err(err(r.0, "This archive can't be read"));
        }
        if !f(e)? {
            return Ok(());
        }
    }
}

/// Every member (folders only implied by paths aren't added here).
pub fn list(path: &Path, cancel: &AtomicBool) -> Result<Vec<Member>, String> {
    let r = open(path, false)?;
    let mut v = Vec::new();
    each(&r, cancel, |e| {
        let p = entry_path(e);
        if !p.is_empty() && p != "." {
            let t = unsafe { archive_entry_filetype(e) } & AE_IFMT;
            v.push(Member { path: p, size: unsafe { archive_entry_size(e) }.max(0) as u64, is_dir: t == AE_IFDIR, mtime: unsafe { archive_entry_mtime(e) } });
        }
        unsafe { archive_read_data_skip(r.0) };
        Ok(true)
    })?;
    Ok(v)
}

/// The member names (files only).
pub fn names(path: &Path, cancel: &AtomicBool) -> Vec<String> {
    list(path, cancel).map(|v| v.into_iter().filter(|m| !m.is_dir).map(|m| m.path).collect()).unwrap_or_default()
}

/// One member's bytes (up to `limit`; None if it isn't there or is bigger).
pub fn read_member(path: &Path, name: &str, limit: usize, cancel: &AtomicBool) -> Option<Vec<u8>> {
    let r = open(path, false).ok()?;
    let want = clean(name);
    let mut out: Option<Vec<u8>> = None;
    each(&r, cancel, |e| {
        if entry_path(e) != want || unsafe { archive_entry_filetype(e) } & AE_IFMT != AE_IFREG {
            unsafe { archive_read_data_skip(r.0) };
            return Ok(true);
        }
        let mut buf = Vec::new();
        let mut block = vec![0u8; 1 << 16];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            let n = unsafe { archive_read_data(r.0, block.as_mut_ptr() as *mut c_void, block.len()) };
            if n < 0 {
                return Err(err(r.0, "Couldn't read"));
            }
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&block[..n as usize]);
            if buf.len() > limit {
                return Ok(false);
            }
        }
        out = Some(buf);
        Ok(false)
    })
    .ok()?;
    out
}

/// A disk writer: no ".." in paths, no writing through symlinks; times and
/// modes as stored.
struct Writer(*mut Archive);
impl Drop for Writer {
    fn drop(&mut self) {
        unsafe { archive_write_free(self.0) };
    }
}
fn disk_writer() -> Writer {
    let w = unsafe { archive_write_disk_new() };
    unsafe {
        // (Not SECURE_NOABSOLUTEPATHS: every path is rewritten to an absolute
        // one under the destination, from a member name with its leading "/"
        // stripped; ".." is still refused.)
        archive_write_disk_set_options(w, EXTRACT_TIME | EXTRACT_PERM | EXTRACT_SECURE_SYMLINKS | EXTRACT_SECURE_NODOTDOT);
        archive_write_disk_set_standard_lookup(w);
    }
    Writer(w)
}

fn set_under(e: *mut Entry, dest: &Path, inner: &str) -> Result<(), String> {
    let to = cstr(&dest.join(inner))?;
    unsafe { archive_entry_set_pathname(e, to.as_ptr()) };
    let h = unsafe { archive_entry_hardlink(e) };
    if !h.is_null() {
        let target = clean(&unsafe { CStr::from_ptr(h) }.to_string_lossy());
        let t = cstr(&dest.join(target))?;
        unsafe { archive_entry_set_hardlink(e, t.as_ptr()) };
    }
    Ok(())
}

/// Extract members into `dest`: everything (`only` empty), or the members
/// named and everything under the folders named.
pub fn extract(path: &Path, dest: &Path, only: &[&str], cancel: &AtomicBool) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let r = open(path, false)?;
    let w = disk_writer();
    let only: Vec<String> = only.iter().map(|s| clean(s)).collect();
    let mut first_err: Option<String> = None;
    each(&r, cancel, |e| {
        let p = entry_path(e);
        let wanted = !p.is_empty() && p != "." && (only.is_empty() || only.iter().any(|o| p == *o || p.starts_with(&format!("{o}/"))));
        if !wanted {
            unsafe { archive_read_data_skip(r.0) };
            return Ok(true);
        }
        set_under(e, dest, &p)?;
        let rc = unsafe { archive_read_extract2(r.0, e, w.0) };
        if rc < WARN && first_err.is_none() {
            first_err = Some(err(r.0, &format!("Couldn't extract “{p}”")));
        }
        Ok(true)
    })?;
    match first_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Decompress a lone compressed file (.gz, .xz, .zst, .bz2, .lz4...) to `out`.
pub fn decompress(path: &Path, out: &Path, cancel: &AtomicBool) -> Result<(), String> {
    use std::io::Write;
    let r = open(path, true)?;
    let mut f = std::fs::File::create(out).map_err(|e| e.to_string())?;
    let mut done = false;
    let res = each(&r, cancel, |_e| {
        let mut block = vec![0u8; 1 << 16];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            let n = unsafe { archive_read_data(r.0, block.as_mut_ptr() as *mut c_void, block.len()) };
            if n < 0 {
                return Err(err(r.0, "Couldn't decompress"));
            }
            if n == 0 {
                break;
            }
            f.write_all(&block[..n as usize]).map_err(|e| e.to_string())?;
        }
        done = true;
        Ok(false)
    });
    if res.is_err() || !done {
        let _ = std::fs::remove_file(out);
        return Err(res.err().unwrap_or_else(|| "Nothing to decompress".into()));
    }
    Ok(())
}

/// How many bytes a lone compressed file decompresses to (reads it all).
pub fn decompressed_size(path: &Path, cancel: &AtomicBool) -> Option<u64> {
    let r = open(path, true).ok()?;
    let mut total = 0u64;
    each(&r, cancel, |_e| {
        let mut block = vec![0u8; 1 << 16];
        loop {
            let n = unsafe { archive_read_data(r.0, block.as_mut_ptr() as *mut c_void, block.len()) };
            if n <= 0 {
                return if n < 0 { Err("bad".into()) } else { Ok(false) };
            }
            total += n as u64;
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
        }
    })
    .ok()?;
    Some(total)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Format {
    Zip,
    TarZst,
}

/// Write `items` (files and folders, recursively) into a new archive at
/// `out`, each under its own name (paths relative to its parent folder).
pub fn create(out: &Path, items: &[PathBuf], format: Format, cancel: &AtomicBool) -> Result<(), String> {
    let w = unsafe { archive_write_new() };
    let w = Writer(w);
    unsafe {
        match format {
            Format::Zip => {
                archive_write_set_format_zip(w.0);
            }
            Format::TarZst => {
                archive_write_set_format_pax_restricted(w.0);
                archive_write_add_filter_zstd(w.0);
            }
        }
        let o = cstr(out)?;
        if archive_write_open_filename(w.0, o.as_ptr()) != OK {
            return Err(err(w.0, "Couldn't create the archive"));
        }
    }
    let res = (|| {
        for item in items {
            let base = item.parent().unwrap_or(Path::new("/")).to_path_buf();
            add_tree(&w, item, &base, cancel)?;
        }
        Ok(())
    })();
    unsafe { archive_write_close(w.0) };
    if res.is_err() {
        let _ = std::fs::remove_file(out);
    }
    res
}

fn add_tree(w: &Writer, item: &Path, base: &Path, cancel: &AtomicBool) -> Result<(), String> {
    let d = unsafe { archive_read_disk_new() };
    let d = Reader(d);
    let e = unsafe { archive_entry_new() };
    struct E(*mut Entry);
    impl Drop for E {
        fn drop(&mut self) {
            unsafe { archive_entry_free(self.0) };
        }
    }
    let e = E(e);
    unsafe {
        archive_read_disk_set_standard_lookup(d.0);
        let c = cstr(item)?;
        if archive_read_disk_open(d.0, c.as_ptr()) != OK {
            return Err(err(d.0, "Couldn't read"));
        }
    }
    let base_s = format!("{}/", base.to_string_lossy().trim_end_matches('/'));
    let mut block = vec![0u8; 1 << 16];
    loop {
        if cancel.load(Ordering::Relaxed) {
            unsafe { archive_read_close(d.0) };
            return Err("Cancelled".into());
        }
        unsafe { archive_entry_clear(e.0) };
        let rc = unsafe { archive_read_next_header2(d.0, e.0) };
        if rc == EOF {
            break;
        }
        if rc < WARN {
            let m = err(d.0, "Couldn't read");
            unsafe { archive_read_close(d.0) };
            return Err(m);
        }
        unsafe { archive_read_disk_descend(d.0) };
        let full = unsafe { CStr::from_ptr(archive_entry_pathname(e.0)) }.to_string_lossy().into_owned();
        let rel = full.strip_prefix(&base_s).unwrap_or(&full).to_string();
        let rel_c = CString::new(rel.clone()).map_err(|_| "A path with a NUL byte".to_string())?;
        unsafe { archive_entry_set_pathname(e.0, rel_c.as_ptr()) };
        if unsafe { archive_write_header(w.0, e.0) } < WARN {
            let m = err(w.0, &format!("Couldn't add “{rel}”"));
            unsafe { archive_read_close(d.0) };
            return Err(m);
        }
        if unsafe { archive_entry_filetype(e.0) } & AE_IFMT == AE_IFREG {
            loop {
                let n = unsafe { archive_read_data(d.0, block.as_mut_ptr() as *mut c_void, block.len()) };
                if n < 0 {
                    let m = err(d.0, &format!("Couldn't read “{rel}”"));
                    unsafe { archive_read_close(d.0) };
                    return Err(m);
                }
                if n == 0 {
                    break;
                }
                if unsafe { archive_write_data(w.0, block.as_ptr() as *const c_void, n as usize) } < 0 {
                    let m = err(w.0, &format!("Couldn't write “{rel}”"));
                    unsafe { archive_read_close(d.0) };
                    return Err(m);
                }
            }
        }
    }
    unsafe { archive_read_close(d.0) };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let never = AtomicBool::new(false);
        let t = tempfile::tempdir().unwrap();
        let src = t.path().join("src");
        std::fs::create_dir_all(src.join("docs")).unwrap();
        std::fs::write(src.join("readme.txt"), "hi").unwrap();
        std::fs::write(src.join("docs/a.md"), "a").unwrap();
        for (fmt, name) in [(Format::Zip, "x.zip"), (Format::TarZst, "x.tar.zst")] {
            let out = t.path().join(name);
            create(&out, &[src.join("readme.txt"), src.join("docs")], fmt, &never).unwrap();
            let mut l: Vec<String> = list(&out, &never).unwrap().into_iter().map(|m| m.path).collect();
            l.sort();
            assert_eq!(l, vec!["docs", "docs/a.md", "readme.txt"]);
            assert_eq!(read_member(&out, "docs/a.md", 100, &never).unwrap(), b"a");
            let dest = t.path().join(format!("{name}-out"));
            extract(&out, &dest, &["docs"], &never).unwrap();
            assert!(dest.join("docs/a.md").is_file());
            assert!(!dest.join("readme.txt").exists());
        }
    }
}
