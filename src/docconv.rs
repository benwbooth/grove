//! Whole-document viewing for formats Quick Look has no viewer of its own
//! for: each is turned into a PDF once (cached by path, size and time) with
//! the standard tool for it, then paged through like any PDF.
//!   DjVu                          -> ddjvu (djvulibre)
//!   EPUB, MOBI, FB2, XPS, CBZ     -> mutool convert (MuPDF; books reflowed)
//!   PostScript, EPS               -> ps2pdf (Ghostscript)
//!   CBR, CB7, CBT (comics)        -> repacked as CBZ (bsdtar), then mutool
//! Office documents go through LibreOffice (previews::office_pdf).

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Conv {
    Djvu,
    Mupdf,
    Ps,
    ComicRepack,
}

pub fn kind(name: &str) -> Option<Conv> {
    let l = name.to_lowercase();
    let ext = l.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match ext {
        "djvu" | "djv" => Some(Conv::Djvu),
        "epub" | "mobi" | "azw" | "azw3" | "fb2" | "fbz" | "xps" | "oxps" | "cbz" => Some(Conv::Mupdf),
        "ps" | "eps" | "epsf" => Some(Conv::Ps),
        "cbr" | "cb7" | "cbt" => Some(Conv::ComicRepack),
        _ => None,
    }
}

fn cache_for(path: &Path) -> Option<PathBuf> {
    let m = std::fs::metadata(path).ok()?;
    let key = format!("{}|{}|{}", path.display(), m.len(), std::os::unix::fs::MetadataExt::mtime(&m));
    let dir = grove_core::util::cache_home().join("grove/docs");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join(format!("{}.pdf", grove_core::thumbs::md5_hex(&key))))
}

fn run(cmd: &str, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let o = Command::new(cmd).args(args).output().map_err(|_| format!("Viewing this needs {cmd}, which isn't installed."))?;
    if o.status.success() {
        Ok(())
    } else {
        Err(format!("{cmd} couldn't read it: {}", String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("").trim()))
    }
}

/// The document as a PDF (made now, or the cached one).
pub fn to_pdf(path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path);
    let conv = kind(path).ok_or("Grove can't show this kind of document.")?;
    let out = cache_for(p).ok_or("Couldn't read the file.")?;
    if out.is_file() {
        return Ok(out);
    }
    // One conversion per file at a time (Quick Look and the preview column
    // can ask together).
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if out.is_file() {
        return Ok(out);
    }
    let part = out.with_extension("part.pdf");
    let os = |s: &Path| s.as_os_str().to_owned();
    let r = match conv {
        Conv::Djvu => run("ddjvu", &[std::ffi::OsStr::new("-format=pdf"), &os(p), &os(&part)]),
        Conv::Mupdf => run("mutool", &[std::ffi::OsStr::new("convert"), std::ffi::OsStr::new("-o"), &os(&part), &os(p)]),
        Conv::Ps => run("ps2pdf", &[&os(p), &os(&part)]),
        Conv::ComicRepack => {
            let cbz = out.with_extension("cbz");
            let r = Command::new("bsdtar")
                .args(["--format", "zip", "-s", ",^\\./,,", "-cf"])
                .arg(&cbz)
                .arg(format!("@{}", p.display()))
                .output()
                .map_err(|_| "Viewing comics needs bsdtar (libarchive).".to_string())
                .and_then(|o| if o.status.success() { Ok(()) } else { Err(format!("Couldn't read the comic: {}", String::from_utf8_lossy(&o.stderr).trim())) })
                .and_then(|_| run("mutool", &[std::ffi::OsStr::new("convert"), std::ffi::OsStr::new("-o"), &os(&part), &os(&cbz)]));
            let _ = std::fs::remove_file(&cbz);
            r
        }
    };
    if let Err(e) = r {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, &out).map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kinds() {
        assert_eq!(kind("Book.EPUB"), Some(Conv::Mupdf));
        assert_eq!(kind("scan.djvu"), Some(Conv::Djvu));
        assert_eq!(kind("a.cbr"), Some(Conv::ComicRepack));
        assert_eq!(kind("fig.eps"), Some(Conv::Ps));
        assert_eq!(kind("a.pdf"), None);
    }
}
