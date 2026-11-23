//! Whole-document viewing for formats Quick Look has no viewer of its own
//! for: each is turned into a PDF once (cached by path, size and time) with
//! linked libraries (grove_core::docs), then paged through like any PDF.
//!   DjVu                          -> djvulibre pages into a PDF (MuPDF)
//!   EPUB, MOBI, FB2, XPS, CBZ     -> MuPDF's PDF writer (books reflowed)
//!   PostScript, EPS               -> Ghostscript (pdfwrite)
//!   CBR, CB7, CBT (comics)        -> repacked as CBZ (libarchive), then MuPDF
//! Office documents go through LibreOffice (previews::office_pdf).

use std::path::{Path, PathBuf};

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
    let r = match conv {
        Conv::Djvu => grove_core::docs::djvu_to_pdf(p, &part),
        Conv::Mupdf => grove_core::docs::to_pdf(p, &part),
        Conv::Ps => grove_core::docs::ps_to_pdf(p, &part),
        Conv::ComicRepack => {
            // CBR/CB7/CBT: the pages into a zip (libarchive), which MuPDF reads.
            let cbz = out.with_extension("cbz");
            let tmp = out.with_extension("pages");
            let never = std::sync::atomic::AtomicBool::new(false);
            let r = grove_core::libarchive::extract(p, &tmp, &[], &never)
                .and_then(|_| {
                    let mut pages: Vec<PathBuf> = std::fs::read_dir(&tmp).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).collect();
                    pages.sort();
                    grove_core::libarchive::create(&cbz, &pages, grove_core::libarchive::Format::Zip, &never)
                })
                .map_err(|e| format!("Couldn't read the comic: {e}"))
                .and_then(|_| grove_core::docs::to_pdf(&cbz, &part));
            let _ = std::fs::remove_file(&cbz);
            let _ = std::fs::remove_dir_all(&tmp);
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
