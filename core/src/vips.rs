//! Pictures Qt can't read (HEIC, AVIF, RAW, JPEG XL, PSD, SVGZ...) through
//! libvips, linked (no vipsthumbnail/magick): a thumbnail PNG decoded at
//! the size it's needed.

use std::ffi::{c_char, c_int, c_void, CString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

#[link(name = "vips")]
extern "C" {
    fn vips_init(argv0: *const c_char) -> c_int;
    fn vips_thumbnail(filename: *const c_char, out: *mut *mut c_void, width: c_int, ...) -> c_int;
    fn vips_image_write_to_file(image: *mut c_void, name: *const c_char, ...) -> c_int;
    fn vips_error_clear();
}
#[link(name = "gobject-2.0")]
extern "C" {
    fn g_object_unref(obj: *mut c_void);
}

/// VIPS_SIZE_DOWN: only ever shrink.
const SIZE_DOWN: c_int = 2;

fn ready() -> bool {
    static OK: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *OK.get_or_init(|| {
        let name = CString::new("grove").unwrap();
        unsafe { vips_init(name.as_ptr()) == 0 }
    })
}

/// A picture Qt can't open itself (HEIC, AVIF, camera RAW, PSD...) as a
/// full-size JPEG grove can show and zoom (turned upright), kept in the
/// cache by the file's path, size and time: made once, then reused.
pub fn viewable_copy(path: &Path) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    if !ready() {
        return None;
    }
    let m = std::fs::metadata(path).ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (path, m.len(), std::os::unix::fs::MetadataExt::mtime(&m)).hash(&mut h);
    let dir = crate::util::cache_home().join("grove").join("converted");
    let out = dir.join(format!("{:016x}.jpg", h.finish()));
    if out.is_file() {
        return Some(out);
    }
    std::fs::create_dir_all(&dir).ok()?;
    let src = CString::new(path.as_os_str().as_bytes()).ok()?;
    // (Written beside it, then renamed: a half-written copy is never shown.)
    let part = dir.join(format!("{:016x}.part.jpg", h.finish()));
    let dst = CString::new(format!("{}[Q=92]", part.display())).ok()?;
    let (k_height, k_size) = (CString::new("height").unwrap(), CString::new("size").unwrap());
    unsafe {
        let mut img: *mut c_void = std::ptr::null_mut();
        // (A thumbnail as big as it gets: vips's thumbnail turns it upright.)
        let r = vips_thumbnail(src.as_ptr(), &mut img, 16384 as c_int, k_height.as_ptr(), 16384 as c_int, k_size.as_ptr(), SIZE_DOWN, std::ptr::null::<c_char>());
        if r != 0 || img.is_null() {
            vips_error_clear();
            return None;
        }
        let w = vips_image_write_to_file(img, dst.as_ptr(), std::ptr::null::<c_char>());
        g_object_unref(img);
        if w != 0 {
            vips_error_clear();
            let _ = std::fs::remove_file(&part);
            return None;
        }
    }
    std::fs::rename(&part, &out).ok()?;
    Some(out)
}

/// A PNG of `path` at most `px` on its longer side (auto-rotated).
pub fn thumbnail(path: &Path, px: u32) -> Option<PathBuf> {
    if !ready() {
        return None;
    }
    let src = CString::new(path.as_os_str().as_bytes()).ok()?;
    let out = crate::previews::temp_file("png");
    let dst = CString::new(out.as_os_str().as_bytes()).ok()?;
    let (k_height, k_size) = (CString::new("height").unwrap(), CString::new("size").unwrap());
    unsafe {
        let mut img: *mut c_void = std::ptr::null_mut();
        let r = vips_thumbnail(src.as_ptr(), &mut img, px as c_int, k_height.as_ptr(), px as c_int, k_size.as_ptr(), SIZE_DOWN, std::ptr::null::<c_char>());
        if r != 0 || img.is_null() {
            vips_error_clear();
            return None;
        }
        let w = vips_image_write_to_file(img, dst.as_ptr(), std::ptr::null::<c_char>());
        g_object_unref(img);
        if w != 0 {
            vips_error_clear();
            let _ = std::fs::remove_file(&out);
            return None;
        }
    }
    Some(out)
}
