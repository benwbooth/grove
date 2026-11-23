//! Documents through linked libraries (csrc/docs.c): MuPDF for PDF, EPUB,
//! XPS, CBZ, FB2, MOBI (info, pages, conversion to PDF), djvulibre for DjVu
//! and Ghostscript for PostScript. No mutool, pdftoppm, pdfinfo, ddjvu or
//! ps2pdf commands.

use std::ffi::{c_char, c_float, c_int, CStr, CString};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

extern "C" {
    fn grove_doc_info(path: *const c_char, max_sizes: c_int, pages: *mut c_int, sizes: *mut c_float, title: *mut c_char, title_len: c_int, err: *mut c_char, errlen: c_int) -> c_int;
    fn grove_doc_render(path: *const c_char, page: c_int, px: c_int, width_only: c_int, out_png: *const c_char, err: *mut c_char, errlen: c_int) -> c_int;
    fn grove_doc_to_pdf(path: *const c_char, out_pdf: *const c_char, err: *mut c_char, errlen: c_int) -> c_int;
    fn grove_djvu_to_pdf(path: *const c_char, out_pdf: *const c_char, max_dpi: c_int, err: *mut c_char, errlen: c_int) -> c_int;
    fn grove_office_to_pdf(program_dir: *const c_char, profile_url: *const c_char, path: *const c_char, out_pdf: *const c_char, err: *mut c_char, errlen: c_int) -> c_int;
    fn grove_ps_to_pdf(path: *const c_char, out_pdf: *const c_char, err: *mut c_char, errlen: c_int) -> c_int;
}

fn c(p: &Path) -> Result<CString, String> {
    CString::new(p.as_os_str().as_bytes()).map_err(|_| "A path with a NUL byte".to_string())
}

fn result(rc: c_int, err: &[u8]) -> Result<(), String> {
    if rc == 0 {
        Ok(())
    } else {
        Err(CStr::from_bytes_until_nul(err).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|_| "unknown error".into()))
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Info {
    pub pages: u32,
    /// Each page's size in points (up to the number asked for).
    pub sizes: Vec<(f64, f64)>,
    pub title: String,
}

/// Page count, the first `max_sizes` page sizes, and the title.
pub fn info(path: &Path, max_sizes: u32) -> Result<Info, String> {
    let p = c(path)?;
    let mut pages: c_int = 0;
    let mut sizes = vec![0f32; max_sizes as usize * 2];
    let mut title = vec![0u8; 512];
    let mut err = vec![0u8; 512];
    let rc = unsafe { grove_doc_info(p.as_ptr(), max_sizes as c_int, &mut pages, sizes.as_mut_ptr(), title.as_mut_ptr() as *mut c_char, title.len() as c_int, err.as_mut_ptr() as *mut c_char, err.len() as c_int) };
    result(rc, &err)?;
    let n = (pages.max(0) as u32).min(max_sizes) as usize;
    Ok(Info {
        pages: pages.max(0) as u32,
        sizes: (0..n).map(|i| (sizes[2 * i] as f64, sizes[2 * i + 1] as f64)).collect(),
        title: CStr::from_bytes_until_nul(&title).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
    })
}

/// Page `page` (0-based) as a PNG at `out`: `px` on the longer side, or the
/// width with `width_only`.
pub fn render(path: &Path, page: u32, px: u32, width_only: bool, out: &Path) -> Result<(), String> {
    let (p, o) = (c(path)?, c(out)?);
    let mut err = vec![0u8; 512];
    let rc = unsafe { grove_doc_render(p.as_ptr(), page as c_int, px as c_int, width_only as c_int, o.as_ptr(), err.as_mut_ptr() as *mut c_char, err.len() as c_int) };
    result(rc, &err)
}

/// Any document MuPDF reads (EPUB, XPS, CBZ, FB2, MOBI...) as a PDF.
pub fn to_pdf(path: &Path, out: &Path) -> Result<(), String> {
    let (p, o) = (c(path)?, c(out)?);
    let mut err = vec![0u8; 512];
    let rc = unsafe { grove_doc_to_pdf(p.as_ptr(), o.as_ptr(), err.as_mut_ptr() as *mut c_char, err.len() as c_int) };
    result(rc, &err)
}

/// A DjVu document as a PDF (pages drawn at up to 300 dpi).
pub fn djvu_to_pdf(path: &Path, out: &Path) -> Result<(), String> {
    let (p, o) = (c(path)?, c(out)?);
    let mut err = vec![0u8; 512];
    let rc = unsafe { grove_djvu_to_pdf(p.as_ptr(), o.as_ptr(), 300, err.as_mut_ptr() as *mut c_char, err.len() as c_int) };
    result(rc, &err)
}

/// PostScript / EPS as a PDF. Ghostscript runs one conversion at a time
/// in a process.
pub fn ps_to_pdf(path: &Path, out: &Path) -> Result<(), String> {
    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let (p, o) = (c(path)?, c(out)?);
    let mut err = vec![0u8; 512];
    let rc = unsafe { grove_ps_to_pdf(p.as_ptr(), o.as_ptr(), err.as_mut_ptr() as *mut c_char, err.len() as c_int) };
    result(rc, &err)
}

/// LibreOffice's program folder (where its libraries are), found without
/// running anything: GROVE_LIBREOFFICE, then the usual install places.
pub fn libreoffice_dir() -> Option<std::path::PathBuf> {
    let user = std::env::var("USER").unwrap_or_default();
    let home = crate::util::home();
    let mut c: Vec<std::path::PathBuf> = Vec::new();
    if let Some(d) = std::env::var_os("GROVE_LIBREOFFICE") {
        c.push(d.into());
    }
    for base in [format!("/etc/profiles/per-user/{user}/lib"), format!("{}/.nix-profile/lib", home.display()), "/run/current-system/sw/lib".into(), "/usr/lib".into(), "/usr/lib64".into(), "/usr/local/lib".into()] {
        c.push(std::path::Path::new(&base).join("libreoffice/program"));
        c.push(std::path::Path::new(&base).join("collaboraoffice/program"));
    }
    if let Ok(rd) = std::fs::read_dir("/opt") {
        for e in rd.flatten() {
            c.push(e.path().join("program"));
        }
    }
    c.into_iter().find_map(|d| {
        let lib = d.join("libsofficeapp.so");
        let lib = if lib.exists() { lib } else { d.join("libmergedlo.so") };
        // The real install (on Nix the profile entry is a symlink farm).
        lib.exists().then(|| std::fs::canonicalize(&lib).ok().and_then(|l| l.parent().map(Path::to_path_buf)).unwrap_or(d))
    })
}

/// In `grove --office-pdf` only (its own process): LibreOfficeKit saves
/// `path` as a PDF at `out`, with its own profile in `profile`.
pub fn office_to_pdf_here(path: &Path, out: &Path, profile: &Path) -> Result<(), String> {
    let dir = libreoffice_dir().ok_or("LibreOffice isn't installed.")?;
    let (d, pr, p, o) = (c(&dir)?, CString::new(crate::util::file_uri(profile)).map_err(|e| e.to_string())?, c(path)?, c(out)?);
    let mut err = vec![0u8; 512];
    let rc = unsafe { grove_office_to_pdf(d.as_ptr(), pr.as_ptr(), p.as_ptr(), o.as_ptr(), err.as_mut_ptr() as *mut c_char, err.len() as c_int) };
    result(rc, &err)
}
