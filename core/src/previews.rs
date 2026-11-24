//! Pictures of files that Qt can't decode itself, made with small standard
//! tools: PDF pages (pdftoppm), pictures (vipsthumbnail, ImageMagick),
//! video frames and audio cover art / waveforms (ffmpeg), covers inside
//! EPUBs and comic archives, office documents' embedded thumbnails and
//! text (libarchive), and office documents as PDF (LibreOffice, when it's
//! installed). Every call blocks: run them on a worker. Each one gives up
//! when `cancel` is set (the item scrolled away) or after a timeout.

use quick_xml::events::Event;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A fresh file name in grove's private temporary folder.
pub fn temp_file(ext: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!("grove-preview-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(0o700));
    dir.join(format!("{}.{}", N.fetch_add(1, Ordering::Relaxed), ext))
}

/// Run a command (stdout to `stdout` when given), polling for `cancel`.
/// True when it exited successfully.
pub fn run(argv: &[String], stdout: Option<&Path>, cancel: &AtomicBool, timeout: Duration) -> bool {
    let Some((cmd, args)) = argv.split_first() else { return false };
    let out = match stdout {
        Some(p) => match std::fs::File::create(p) {
            Ok(f) => Stdio::from(f),
            Err(_) => return false,
        },
        None => Stdio::null(),
    };
    let Ok(mut child) = Command::new(cmd).args(args).stdin(Stdio::null()).stdout(out).stderr(Stdio::null()).spawn() else { return false };
    let start = Instant::now();
    let mut nap = 2;
    loop {
        match child.try_wait() {
            Ok(Some(st)) => return st.success(),
            Ok(None) => {
                if cancel.load(Ordering::Relaxed) || start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return false;
                }
                std::thread::sleep(Duration::from_millis(nap));
                nap = (nap * 2).min(20);
            }
            Err(_) => return false,
        }
    }
}

fn s(x: impl AsRef<std::ffi::OsStr>) -> String {
    x.as_ref().to_string_lossy().into_owned()
}

fn nonempty(p: &Path) -> bool {
    std::fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false)
}


// ---------- pictures ----------

/// A PNG of a picture Qt can't read (HEIC, AVIF, RAW, JPEG XL, PSD...),
/// at most `px` on its longer side (libvips, linked).
pub fn picture(path: &Path, px: u32, cancel: &AtomicBool) -> Option<PathBuf> {
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    crate::vips::thumbnail(path, px)
}

// ---------- PDF (MuPDF linked: crate::docs) ----------

/// Page `page` (1-based) of a PDF (or EPUB, XPS...) as a PNG. `px`: the
/// longer side; with `width_only`, the width.
pub fn pdf_page(path: &Path, page: u32, px: u32, width_only: bool, cancel: &AtomicBool) -> Option<PathBuf> {
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    let out = temp_file("png");
    match crate::docs::render(path, page.max(1) - 1, px, width_only, &out) {
        Ok(()) if nonempty(&out) => Some(out),
        _ => {
            let _ = std::fs::remove_file(&out);
            None
        }
    }
}

/// PDFs are always drawn here (MuPDF is linked).
pub fn can_draw_pdf() -> bool {
    true
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PdfInfo {
    pub pages: u32,
    /// Each page's size in points (as many as were asked for).
    pub sizes: Vec<(f64, f64)>,
    pub title: String,
}

/// Page count and page sizes (up to `max_pages` sizes).
pub fn pdf_info(path: &Path, max_pages: u32, _cancel: &AtomicBool) -> Option<PdfInfo> {
    let i = crate::docs::info(path, max_pages).ok()?;
    (i.pages > 0).then_some(PdfInfo { pages: i.pages, sizes: i.sizes, title: i.title })
}

// ---------- video and audio (FFmpeg linked: crate::av) ----------

/// A frame from a video.
pub fn video_frame(path: &Path, px: u32, cancel: &AtomicBool) -> Option<PathBuf> {
    crate::av::video_frame(path, px, cancel)
}

/// The cover art embedded in an audio file.
pub fn audio_cover(path: &Path, px: u32, cancel: &AtomicBool) -> Option<PathBuf> {
    crate::av::audio_cover(path, px, cancel)
}

/// A waveform of (the first ten minutes of) an audio file, `w`×`h`, in
/// `color` on transparent.
pub fn audio_waveform(path: &Path, w: u32, h: u32, color: &str, cancel: &AtomicBool) -> Option<PathBuf> {
    let (peaks, _) = crate::av::peaks(path, 600.0, cancel)?;
    let out = temp_file("png");
    crate::av::waveform_png(&peaks, w, h, color, &out)?;
    Some(out)
}

/// A track's waveform for the player, the whole track (up to 3 hours), in
/// two colours from one decode: [base, played]. Cached by path, size and
/// time (~/.cache/grove/waves).
pub fn player_waveform(path: &Path, w: u32, h: u32, base: &str, played: &str, cancel: &AtomicBool) -> Option<(PathBuf, PathBuf)> {
    let m = std::fs::metadata(path).ok()?;
    let key = format!("{}|{}|{}|{w}x{h}|{base}|{played}", path.display(), m.len(), std::os::unix::fs::MetadataExt::mtime(&m));
    let dir = crate::util::cache_home().join("grove/waves");
    std::fs::create_dir_all(&dir).ok()?;
    let id = crate::thumbs::md5_hex(&key);
    let (a, b) = (dir.join(format!("{id}-a.png")), dir.join(format!("{id}-b.png")));
    if a.is_file() && b.is_file() {
        return Some((a, b));
    }
    let (peaks, _) = crate::av::peaks(path, 10800.0, cancel)?;
    if crate::av::waveform_png(&peaks, w, h, base, &a).is_some() && crate::av::waveform_png(&peaks, w, h, played, &b).is_some() {
        Some((a, b))
    } else {
        let _ = std::fs::remove_file(&a);
        let _ = std::fs::remove_file(&b);
        None
    }
}

// ---------- archives: EPUB, comics, office documents ----------

/// The names inside an archive (files only; libarchive, linked).
pub fn archive_names(path: &Path, cancel: &AtomicBool) -> Vec<String> {
    crate::archives::names(path, cancel)
}

/// One member of an archive, written to a temporary file (with `ext`).
pub fn archive_member(path: &Path, name: &str, ext: &str, cancel: &AtomicBool) -> Option<PathBuf> {
    let bytes = crate::archives::read_member(path, name, 512 << 20, cancel)?;
    let out = temp_file(ext);
    std::fs::write(&out, bytes).ok()?;
    Some(out)
}

fn archive_text(path: &Path, name: &str, cancel: &AtomicBool) -> Option<String> {
    crate::archives::read_member(path, name, 64 << 20, cancel).map(|b| String::from_utf8_lossy(&b).into_owned())
}

pub fn is_picture_name(n: &str) -> bool {
    let l = n.to_lowercase();
    [".jpg", ".jpeg", ".png", ".webp", ".gif", ".bmp", ".avif", ".jxl"].iter().any(|e| l.ends_with(e))
}

fn ext_of(n: &str) -> String {
    Path::new(n).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| "img".into())
}

/// A comic book archive's cover: its first picture in natural order.
pub fn comic_cover_name(names: &[String]) -> Option<String> {
    let mut pics: Vec<&String> = names.iter().filter(|n| is_picture_name(n) && !n.split('/').any(|c| c.starts_with('.') || c == "__MACOSX")).collect();
    pics.sort_by(|a, b| crate::util::natural_cmp(a, b));
    pics.first().map(|s| s.to_string())
}

fn attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.local_name().as_ref() == key).and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
}

/// The OPF file an EPUB's container.xml points to.
pub fn epub_rootfile(container: &str) -> Option<String> {
    let mut r = quick_xml::Reader::from_str(container);
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.local_name().as_ref() == b"rootfile" => return attr(&e, b"full-path"),
            Ok(Event::Eof) | Err(_) => return None,
            _ => {}
        }
    }
}

/// The cover picture an EPUB's package (OPF) names, relative to the OPF.
/// EPUB 3's cover-image property, EPUB 2's <meta name="cover">, then an
/// item called cover, then the first picture.
pub fn epub_cover_href(opf: &str) -> Option<String> {
    let mut r = quick_xml::Reader::from_str(opf);
    let mut meta_cover: Option<String> = None;
    let mut items: Vec<(String, String, String, String)> = vec![]; // id, href, media-type, properties
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"meta" if attr(&e, b"name").as_deref() == Some("cover") => meta_cover = attr(&e, b"content"),
                b"item" => items.push((
                    attr(&e, b"id").unwrap_or_default(),
                    attr(&e, b"href").unwrap_or_default(),
                    attr(&e, b"media-type").unwrap_or_default(),
                    attr(&e, b"properties").unwrap_or_default(),
                )),
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    let img = |i: &&(String, String, String, String)| i.2.starts_with("image/");
    items
        .iter()
        .find(|i| i.3.split_whitespace().any(|p| p == "cover-image"))
        .or_else(|| meta_cover.as_ref().and_then(|c| items.iter().find(|i| &i.0 == c && img(i))))
        .or_else(|| items.iter().filter(img).find(|i| i.0.to_lowercase().contains("cover") || i.1.to_lowercase().contains("cover")))
        .or_else(|| items.iter().find(img))
        .map(|i| i.1.clone())
}

/// `href` relative to the folder of `base` inside an archive.
pub fn join_member(base: &str, href: &str) -> String {
    let href = percent_encoding::percent_decode_str(href).decode_utf8_lossy().into_owned();
    let mut parts: Vec<&str> = base.split('/').collect();
    parts.pop();
    for c in href.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            c => parts.push(c),
        }
    }
    parts.join("/")
}

/// An EPUB's cover picture, extracted to a temporary file.
pub fn epub_cover(path: &Path, cancel: &AtomicBool) -> Option<PathBuf> {
    let names = archive_names(path, cancel);
    let found = archive_text(path, "META-INF/container.xml", cancel)
        .and_then(|c| epub_rootfile(&c))
        .and_then(|opf_name| archive_text(path, &opf_name, cancel).and_then(|opf| epub_cover_href(&opf)).map(|h| join_member(&opf_name, &h)))
        .filter(|n| names.iter().any(|x| x == n))
        .or_else(|| comic_cover_name(&names.iter().filter(|n| n.to_lowercase().contains("cover")).cloned().collect::<Vec<_>>()))
        .or_else(|| comic_cover_name(&names))?;
    archive_member(path, &found, &ext_of(&found), cancel)
}

/// A comic book's cover (CBZ, CBR, CB7, CBT).
pub fn comic_cover(path: &Path, cancel: &AtomicBool) -> Option<PathBuf> {
    let names = archive_names(path, cancel);
    let n = comic_cover_name(&names)?;
    archive_member(path, &n, &ext_of(&n), cancel)
}

/// True when a picture is one flat colour (Word for Mac's "Save preview
/// picture" stores an all-white page). PNG and JPEG are checked (image
/// crate); anything else counts as having something on it.
pub fn is_blank_picture(p: &Path, _cancel: &AtomicBool) -> bool {
    let Ok(img) = image::open(p) else { return false };
    let g = img.to_luma8();
    let n = g.len().max(1) as f64;
    let mean = g.iter().map(|&v| v as f64).sum::<f64>() / n;
    let var = g.iter().map(|&v| (v as f64 - mean).powi(2)).sum::<f64>() / n;
    var.sqrt() / 255.0 < 0.004
}

/// The preview picture an office document carries (OpenDocument's
/// Thumbnails/thumbnail.png, Office Open XML's docProps/thumbnail.*),
/// unless it's blank.
pub fn office_thumbnail(path: &Path, cancel: &AtomicBool) -> Option<PathBuf> {
    let names = archive_names(path, cancel);
    let n = names.iter().find(|n| {
        let l = n.to_lowercase();
        l == "thumbnails/thumbnail.png" || (l.starts_with("docprops/thumbnail.") && is_picture_name(&l))
    })?;
    let f = archive_member(path, n, &ext_of(n), cancel)?;
    if is_blank_picture(&f, cancel) {
        let _ = std::fs::remove_file(&f);
        return None;
    }
    Some(f)
}

/// The first picture inside an office document (word/media, ppt/media,
/// xl/media, OpenDocument's Pictures/), for documents with no text.
pub fn office_media_name(names: &[String]) -> Option<String> {
    let mut pics: Vec<&String> = names
        .iter()
        .filter(|n| ["word/media/", "ppt/media/", "xl/media/", "Pictures/"].iter().any(|d| n.starts_with(d)) && is_picture_name(n))
        .collect();
    pics.sort_by(|a, b| crate::util::natural_cmp(a, b));
    pics.first().map(|s| s.to_string())
}

pub fn office_picture(path: &Path, cancel: &AtomicBool) -> Option<PathBuf> {
    let names = archive_names(path, cancel);
    let n = office_media_name(&names)?;
    archive_member(path, &n, &ext_of(&n), cancel)
}

/// Which XML part holds an office document's text.
pub fn office_text_part(names: &[String]) -> Option<String> {
    let has = |n: &str| names.iter().any(|x| x == n);
    if has("word/document.xml") {
        return Some("word/document.xml".into());
    }
    if has("content.xml") {
        return Some("content.xml".into());
    }
    if has("xl/sharedStrings.xml") {
        return Some("xl/sharedStrings.xml".into());
    }
    let mut slides: Vec<&String> = names.iter().filter(|n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml")).collect();
    slides.sort_by(|a, b| crate::util::natural_cmp(a, b));
    slides.first().map(|s| s.to_string())
}

/// The text of an office XML part: paragraphs, headings, cells and rows
/// on lines of their own.
pub fn office_xml_text(xml: &str, limit: usize) -> String {
    let mut r = quick_xml::Reader::from_str(xml);
    let mut out = String::new();
    let mut in_text_run = 0usize;
    loop {
        match r.read_event() {
            Ok(Event::Text(t)) => {
                if let Ok(u) = t.unescape() {
                    let u = u.trim_matches(|c| c == '\n' || c == '\r');
                    if !u.trim().is_empty() || in_text_run > 0 {
                        out.push_str(u);
                    }
                }
            }
            Ok(Event::Start(e)) => {
                if e.local_name().as_ref() == b"t" {
                    in_text_run += 1;
                }
            }
            Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"tab" => out.push('\t'),
                b"s" => out.push(' '),
                b"br" | b"line-break" => out.push('\n'),
                _ => {}
            },
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"t" => in_text_run = in_text_run.saturating_sub(1),
                b"p" | b"h" | b"si" | b"tr" | b"table-row" => {
                    if !out.ends_with('\n') && !out.is_empty() {
                        out.push('\n');
                    }
                }
                b"tc" | b"table-cell" | b"c" => out.push('\t'),
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        if out.len() > limit {
            break;
        }
    }
    out.trim().to_string()
}

/// An office document's text (the start of it).
pub fn office_text(path: &Path, limit: usize, cancel: &AtomicBool) -> Option<String> {
    // Compiled HTML Help (.chm): its pages' text (chm.rs).
    if path.to_string_lossy().to_lowercase().ends_with(".chm") {
        return crate::chm::text(path, limit);
    }
    // WinHelp (.hlp): its topics' text (winhelp.rs).
    if path.to_string_lossy().to_lowercase().ends_with(".hlp") {
        let t = crate::winhelp::text(path)?;
        return Some(t.chars().take(limit).collect());
    }
    // Old Word (.doc/.dot): its own binary formats (worddoc.rs).
    let lower = path.to_string_lossy().to_lowercase();
    if lower.ends_with(".doc") || lower.ends_with(".dot") {
        let t = crate::worddoc::text(path).ok()?;
        let t: String = t.chars().take(limit).collect();
        return (!t.trim().is_empty()).then_some(t);
    }
    let names = archive_names(path, cancel);
    let part = office_text_part(&names)?;
    let xml = archive_text(path, &part, cancel)?;
    let t = office_xml_text(&xml, limit);
    (!t.is_empty()).then_some(t)
}

// ---------- LibreOffice ----------

pub fn has_libreoffice() -> bool {
    crate::docs::libreoffice_dir().is_some()
}

/// An office document converted to PDF by LibreOffice (LibreOfficeKit, in
/// `grove --office-pdf`: grove's own process, not soffice; with its own
/// profile so it doesn't touch a running LibreOffice), cached in
/// ~/.cache/grove/office by path and modification time. Slow (seconds):
/// one at a time.
pub fn office_pdf(path: &Path, cancel: &AtomicBool) -> Option<PathBuf> {
    static ONE: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
    crate::docs::libreoffice_dir()?;
    let exe = std::env::current_exe().ok()?;
    let meta = std::fs::metadata(path).ok()?;
    let key = crate::thumbs::md5_hex(&format!("{}:{}", crate::util::file_uri(path), std::os::unix::fs::MetadataExt::mtime(&meta)));
    let cache = crate::util::cache_home().join("grove/office");
    let target = cache.join(format!("{key}.pdf"));
    if nonempty(&target) {
        return Some(target);
    }
    let _one = ONE.lock();
    if nonempty(&target) {
        return Some(target);
    }
    std::fs::create_dir_all(&cache).ok()?;
    let profile = std::env::temp_dir().join(format!("grove-lo-profile-{}", unsafe { libc::getuid() }));
    let part = cache.join(format!("{key}.part.pdf"));
    let argv = vec![s(&exe), "--office-pdf".into(), s(path), s(&part), s(&profile)];
    let ok = run(&argv, None, cancel, Duration::from_secs(120));
    let result = if ok && nonempty(&part) { std::fs::rename(&part, &target).ok().map(|_| target) } else { None };
    let _ = std::fs::remove_file(&part);
    result
}

// ---------- kinds ----------

/// What kind of preview a file gets, from its type (and parents) and name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Picture,
    Video,
    Audio,
    Pdf,
    Epub,
    Comic,
    Office,
    Text,
    Font,
    Model,
    Other,
}

pub fn kind_of(mimes: &[String], name: &str) -> Kind {
    let m = mimes.first().map(String::as_str).unwrap_or("");
    let has = |x: &str| mimes.iter().any(|y| y == x);
    let l = name.to_lowercase();
    let ext = l.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    if m == "application/pdf" || ext == "pdf" {
        return Kind::Pdf;
    }
    if m == "application/epub+zip" || ext == "epub" {
        return Kind::Epub;
    }
    if m.starts_with("application/vnd.comicbook") || matches!(ext, "cbz" | "cbr" | "cb7" | "cbt") {
        return Kind::Comic;
    }
    if m.starts_with("application/vnd.oasis.opendocument") || m.starts_with("application/vnd.openxmlformats-officedocument") || m == "application/msword" || m == "application/winhlp" || m == "application/vnd.ms-htmlhelp"
        || matches!(ext, "odt" | "ods" | "odp" | "odg" | "docx" | "xlsx" | "pptx" | "ott" | "ots" | "otp" | "doc" | "dot" | "hlp" | "chm")
    {
        return Kind::Office;
    }
    if m.starts_with("font/") || matches!(m, "application/x-font-ttf" | "application/x-font-otf" | "application/vnd.ms-opentype")
        || matches!(ext, "ttf" | "otf" | "ttc" | "otc" | "woff" | "woff2")
    {
        return Kind::Font;
    }
    if matches!(m, "model/stl" | "model/obj" | "application/sla" | "model/x.stl-ascii" | "model/x.stl-binary") || matches!(ext, "stl" | "obj") {
        return Kind::Model;
    }
    if m.starts_with("image/") {
        return Kind::Picture;
    }
    if m.starts_with("video/") {
        return Kind::Video;
    }
    if m.starts_with("audio/") {
        return Kind::Audio;
    }
    // Code and config files by name too (inside archives the type comes
    // from the name alone, and many have no text/* type).
    const CODE: &[&str] = &[
        "txt", "text", "log", "md", "markdown", "rst", "adoc", "org", "csv", "tsv", "ini", "cfg", "conf", "config", "properties", "env",
        "toml", "yaml", "yml", "json", "jsonc", "json5", "xml", "xsd", "xsl", "plist", "html", "htm", "xhtml", "css", "scss", "sass", "less",
        "js", "mjs", "cjs", "ts", "tsx", "jsx", "vue", "svelte", "c", "h", "cc", "cpp", "cxx", "hpp", "hh", "hxx", "inl", "m", "mm",
        "rs", "go", "java", "kt", "kts", "scala", "groovy", "gradle", "cs", "fs", "fsx", "vb", "vbs", "bas", "frm", "cls", "swift", "dart",
        "py", "pyw", "pyi", "rb", "pl", "pm", "php", "lua", "tcl", "r", "jl", "nim", "zig", "d", "v", "sv", "vhd", "vhdl", "hs", "lhs",
        "ml", "mli", "elm", "ex", "exs", "erl", "hrl", "clj", "cljs", "lisp", "lsp", "scm", "rkt", "el", "vim", "sql", "graphql", "proto",
        "sh", "bash", "zsh", "fish", "ksh", "csh", "tsh", "tessh", "bat", "cmd", "btm", "ps1", "psm1", "psd1", "ahk", "au3", "asm", "s", "inc", "a51",
        "nix", "cmake", "mk", "mak", "make", "ninja", "meson", "bazel", "bzl", "dockerfile", "qml", "qrc", "pro", "pri", "ui", "glsl",
        "hlsl", "vert", "frag", "shader", "cg", "diff", "patch", "srt", "ass", "ssa", "vtt", "tex", "sty", "bib", "nfo", "diz", "reg",
        "inf", "desktop", "service", "gitignore", "gitattributes", "editorconfig", "lock", "cue", "m3u", "pls",
    ];
    const CODE_NAMES: &[&str] = &["makefile", "gnumakefile", "dockerfile", "containerfile", "cmakelists.txt", "readme", "license", "copying",
        "changelog", "authors", "install", "news", "todo", "pkgbuild", "vagrantfile", "gemfile", "rakefile", "procfile", "justfile"];
    if !m.starts_with("image/") && !m.starts_with("video/") && !m.starts_with("audio/") && (CODE.contains(&ext) || CODE_NAMES.contains(&l.as_str())) {
        return Kind::Text;
    }
    if m.starts_with("text/") || has("text/plain")
        || matches!(m, "application/json" | "application/xml" | "application/javascript" | "application/x-shellscript" | "application/toml"
            | "application/x-yaml" | "application/sql" | "application/x-desktop" | "application/x-perl" | "application/x-ruby"
            | "application/x-php" | "application/x-python" | "application/x-wine-extension-ini" | "application/x-subrip")
    {
        return Kind::Text;
    }
    Kind::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epub_cover_lookup() {
        let c = r#"<container><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
        assert_eq!(epub_rootfile(c).unwrap(), "OEBPS/content.opf");
        let opf2 = r#"<package><metadata><meta name="cover" content="cov"/></metadata><manifest><item id="a" href="images/a.jpg" media-type="image/jpeg"/><item id="cov" href="images/front%20page.jpg" media-type="image/jpeg"/></manifest></package>"#;
        let h = epub_cover_href(opf2).unwrap();
        assert_eq!(join_member("OEBPS/content.opf", &h), "OEBPS/images/front page.jpg");
        let opf3 = r#"<package><manifest><item id="x" href="../c.png" media-type="image/png" properties="cover-image"/><item id="y" href="cover.jpg" media-type="image/jpeg"/></manifest></package>"#;
        assert_eq!(join_member("OPS/pkg.opf", &epub_cover_href(opf3).unwrap()), "c.png");
    }

    #[test]
    fn comic_and_office_names() {
        let names: Vec<String> = ["page10.jpg", "page2.jpg", "__MACOSX/._page1.jpg", "page1.jpg", "info.txt"].iter().map(|s| s.to_string()).collect();
        assert_eq!(comic_cover_name(&names).unwrap(), "page1.jpg");
        let docx: Vec<String> = ["[Content_Types].xml", "word/document.xml", "docProps/thumbnail.jpeg"].iter().map(|s| s.to_string()).collect();
        assert_eq!(office_text_part(&docx).unwrap(), "word/document.xml");
        let pptx: Vec<String> = ["ppt/slides/slide10.xml", "ppt/slides/slide2.xml", "ppt/slides/slide1.xml"].iter().map(|s| s.to_string()).collect();
        assert_eq!(office_text_part(&pptx).unwrap(), "ppt/slides/slide1.xml");
        let pics: Vec<String> = ["word/document.xml", "word/media/image10.png", "word/media/image2.jpeg", "word/media/image3.emf"].iter().map(|s| s.to_string()).collect();
        assert_eq!(office_media_name(&pics).unwrap(), "word/media/image2.jpeg");
        assert_eq!(office_media_name(&docx), None);
    }

    #[test]
    fn office_text_extraction() {
        let docx = r#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t>Hello</w:t></w:r><w:r><w:t xml:space="preserve"> world</w:t></w:r></w:p><w:p><w:r><w:t>Second</w:t><w:tab/><w:t>col</w:t></w:r></w:p></w:body></w:document>"#;
        assert_eq!(office_xml_text(docx, 1000), "Hello world\nSecond\tcol");
        let odt = r#"<office:document-content><office:body><office:text><text:h>Title</text:h><text:p>One<text:s/>two</text:p></office:text></office:body></office:document-content>"#;
        assert_eq!(office_xml_text(odt, 1000), "Title\nOne two");
    }

    #[test]
    fn kinds() {
        let k = |m: &str, n: &str| kind_of(&[m.to_string()], n);
        assert_eq!(k("application/pdf", "a.pdf"), Kind::Pdf);
        assert_eq!(k("application/zip", "a.cbz"), Kind::Comic);
        assert_eq!(k("application/vnd.openxmlformats-officedocument.wordprocessingml.document", "a.docx"), Kind::Office);
        assert_eq!(k("font/ttf", "a.ttf"), Kind::Font);
        assert_eq!(k("model/stl", "a.stl"), Kind::Model);
        assert_eq!(k("text/x-rust", "a.rs"), Kind::Text);
        assert_eq!(k("image/heif", "a.heic"), Kind::Picture);
        assert_eq!(k("application/octet-stream", "a.bin"), Kind::Other);
        assert_eq!(k("application/octet-stream", "a.asm"), Kind::Text);
        assert_eq!(k("application/x-bat", "RUN.BAT"), Kind::Text);
        assert_eq!(k("application/octet-stream", "Makefile"), Kind::Text);
    }
}
