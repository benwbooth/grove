//! Pictures of files that Qt can't decode itself, made with small standard
//! tools: PDF pages (pdftoppm), pictures (vipsthumbnail, ImageMagick),
//! video frames and audio cover art / waveforms (ffmpeg), covers inside
//! EPUBs and comic archives, office documents' embedded thumbnails and
//! text (libarchive), and office documents as PDF (LibreOffice, when it's
//! installed). Every call blocks: run them on a worker. Each one gives up
//! when `cancel` is set (the item scrolled away) or after a timeout.

use crate::thumbs::which;
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

/// The output file if the tool made one, else clean up.
fn made(ok: bool, out: PathBuf) -> Option<PathBuf> {
    if ok && nonempty(&out) {
        Some(out)
    } else {
        let _ = std::fs::remove_file(&out);
        None
    }
}

// ---------- pictures ----------

/// A PNG of a picture Qt can't read (HEIC, AVIF, RAW, JPEG XL, PSD...),
/// at most `px` on its longer side. vipsthumbnail first (it decodes only
/// what it needs), ImageMagick second.
pub fn picture(path: &Path, px: u32, cancel: &AtomicBool) -> Option<PathBuf> {
    let out = temp_file("png");
    if which("vipsthumbnail").is_some() {
        let argv = vec!["vipsthumbnail".into(), s(path), "--size".into(), format!("{px}x{px}>"), "-o".into(), s(&out)];
        if let Some(p) = made(run(&argv, None, cancel, Duration::from_secs(30)), out.clone()) {
            return Some(p);
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    if which("magick").is_some() {
        let argv = vec!["magick".into(), format!("{}[0]", s(path)), "-auto-orient".into(), "-thumbnail".into(), format!("{px}x{px}>"), s(&out)];
        return made(run(&argv, None, cancel, Duration::from_secs(30)), out);
    }
    None
}

// ---------- PDF ----------

/// Page `page` (1-based) of a PDF as a PNG. `px`: the longer side; with
/// `width_only`, the width (for reading at a given column width).
/// pdftoppm, or mutool when that's what's installed.
pub fn pdf_page(path: &Path, page: u32, px: u32, width_only: bool, cancel: &AtomicBool) -> Option<PathBuf> {
    let page = page.max(1);
    if which("pdftoppm").is_some() {
        let out = temp_file("png");
        let prefix = out.with_extension("");
        let mut argv = vec!["pdftoppm".into(), "-f".into(), page.to_string(), "-l".into(), page.to_string()];
        if width_only {
            argv.extend(["-scale-to-x".into(), px.to_string(), "-scale-to-y".into(), "-1".into()]);
        } else {
            argv.extend(["-scale-to".into(), px.to_string()]);
        }
        argv.extend(["-png".into(), "-singlefile".into(), "-aa".into(), "yes".into(), "-aaVector".into(), "yes".into(), s(path), s(&prefix)]);
        return made(run(&argv, None, cancel, Duration::from_secs(40)), out);
    }
    if which("mutool").is_some() {
        let out = temp_file("png");
        let mut argv = vec!["mutool".into(), "draw".into(), "-q".into(), "-w".into(), px.to_string()];
        if !width_only {
            argv.extend(["-h".into(), px.to_string()]);
        }
        argv.extend(["-o".into(), s(&out), s(path), page.to_string()]);
        return made(run(&argv, None, cancel, Duration::from_secs(40)), out);
    }
    None
}

/// Whether PDFs can be drawn with a tool (else the caller falls back to
/// QtPdf, whose first use reads every system font and can stall).
pub fn can_draw_pdf() -> bool {
    which("pdftoppm").is_some() || which("mutool").is_some()
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PdfInfo {
    pub pages: u32,
    /// Each page's size in points (as many as were reported).
    pub sizes: Vec<(f64, f64)>,
    pub title: String,
}

/// Parse `pdfinfo -f 1 -l N` output.
pub fn parse_pdfinfo(text: &str) -> PdfInfo {
    let mut info = PdfInfo::default();
    let mut single: Option<(f64, f64)> = None;
    for line in text.lines() {
        let Some((k, v)) = line.split_once(':') else { continue };
        let v = v.trim();
        let size = || -> Option<(f64, f64)> {
            let mut it = v.split_whitespace();
            let w = it.next()?.parse().ok()?;
            let _x = it.next()?;
            let h = it.next()?.parse().ok()?;
            Some((w, h))
        };
        if k == "Pages" {
            info.pages = v.parse().unwrap_or(0);
        } else if k == "Title" {
            info.title = v.to_string();
        } else if k == "Page size" {
            single = size();
        } else if k.starts_with("Page ") && k.ends_with(" size") {
            if let Some(sz) = size() {
                info.sizes.push(sz);
            }
        } else if k.starts_with("Page ") && k.ends_with(" rot") {
            // Rotated by 90/270: swap the last page's size.
            if matches!(v.parse::<i32>().unwrap_or(0).rem_euclid(360), 90 | 270) {
                if let Some(last) = info.sizes.last_mut() {
                    *last = (last.1, last.0);
                }
            }
        }
    }
    if info.sizes.is_empty() {
        if let Some(sz) = single {
            info.sizes.push(sz);
        }
    }
    info
}

/// Page count and page sizes (pdfinfo; up to `max_pages` sizes).
pub fn pdf_info(path: &Path, max_pages: u32, cancel: &AtomicBool) -> Option<PdfInfo> {
    which("pdfinfo")?;
    let out = temp_file("txt");
    let argv = vec!["pdfinfo".into(), "-f".into(), "1".into(), "-l".into(), max_pages.to_string(), s(path)];
    let ok = run(&argv, Some(&out), cancel, Duration::from_secs(20));
    let text = std::fs::read_to_string(&out).unwrap_or_default();
    let _ = std::fs::remove_file(&out);
    let info = parse_pdfinfo(&text);
    (ok && info.pages > 0).then_some(info)
}

// ---------- video and audio ----------

/// A frame from a video (when no system thumbnailer did it).
pub fn video_frame(path: &Path, px: u32, cancel: &AtomicBool) -> Option<PathBuf> {
    which("ffmpeg")?;
    let out = temp_file("png");
    for at in ["4", "0"] {
        let argv: Vec<String> = [
            "ffmpeg", "-v", "error", "-nostdin", "-y", "-ss", at, "-i", &s(path), "-frames:v", "1", "-an",
            "-vf", &format!("thumbnail=30,scale={px}:{px}:force_original_aspect_ratio=decrease"), &s(&out),
        ]
        .iter()
        .map(|x| x.to_string())
        .collect();
        if let Some(p) = made(run(&argv, None, cancel, Duration::from_secs(30)), out.clone()) {
            return Some(p);
        }
        if cancel.load(Ordering::Relaxed) {
            break;
        }
    }
    None
}

/// The cover art embedded in an audio file.
pub fn audio_cover(path: &Path, px: u32, cancel: &AtomicBool) -> Option<PathBuf> {
    which("ffmpeg")?;
    let out = temp_file("png");
    let argv: Vec<String> = [
        "ffmpeg", "-v", "error", "-nostdin", "-y", "-i", &s(path), "-map", "0:v:0", "-frames:v", "1", "-an",
        "-vf", &format!("scale={px}:{px}:force_original_aspect_ratio=decrease"), &s(&out),
    ]
    .iter()
    .map(|x| x.to_string())
    .collect();
    made(run(&argv, None, cancel, Duration::from_secs(20)), out)
}

/// A waveform of (the first ten minutes of) an audio file, `w`×`h`, in
/// `color` on transparent.
pub fn audio_waveform(path: &Path, w: u32, h: u32, color: &str, cancel: &AtomicBool) -> Option<PathBuf> {
    which("ffmpeg")?;
    let out = temp_file("png");
    let argv: Vec<String> = [
        "ffmpeg", "-v", "error", "-nostdin", "-y", "-t", "600", "-i", &s(path),
        "-filter_complex", &format!("aformat=channel_layouts=mono,showwavespic=s={w}x{h}:colors={color}:scale=sqrt"),
        "-frames:v", "1", &s(&out),
    ]
    .iter()
    .map(|x| x.to_string())
    .collect();
    made(run(&argv, None, cancel, Duration::from_secs(30)), out)
}

/// A track's waveform for the player, the whole track (up to 3 hours), in
/// two colours from one decode: [base, played]. Cached by path, size and
/// time (~/.cache/grove/waves).
pub fn player_waveform(path: &Path, w: u32, h: u32, base: &str, played: &str, cancel: &AtomicBool) -> Option<(PathBuf, PathBuf)> {
    which("ffmpeg")?;
    let m = std::fs::metadata(path).ok()?;
    let key = format!("{}|{}|{}|{w}x{h}|{base}|{played}", path.display(), m.len(), std::os::unix::fs::MetadataExt::mtime(&m));
    let dir = crate::util::cache_home().join("grove/waves");
    std::fs::create_dir_all(&dir).ok()?;
    let id = crate::thumbs::md5_hex(&key);
    let (a, b) = (dir.join(format!("{id}-a.png")), dir.join(format!("{id}-b.png")));
    if a.is_file() && b.is_file() {
        return Some((a, b));
    }
    let graph = format!(
        "[0:a]aformat=channel_layouts=mono,asplit=2[x][y];[x]showwavespic=s={w}x{h}:colors={base}:scale=sqrt[a];[y]showwavespic=s={w}x{h}:colors={played}:scale=sqrt[b]"
    );
    let argv: Vec<String> = [
        "ffmpeg", "-v", "error", "-nostdin", "-y", "-t", "10800", "-i", &s(path), "-filter_complex", &graph,
        "-map", "[a]", "-frames:v", "1", &s(&a), "-map", "[b]", "-frames:v", "1", &s(&b),
    ]
    .iter()
    .map(|x| x.to_string())
    .collect();
    if run(&argv, None, cancel, Duration::from_secs(120)) && a.is_file() && b.is_file() {
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
/// picture" stores an all-white page). Needs ImageMagick; without it
/// every picture counts as having something on it.
pub fn is_blank_picture(p: &Path, cancel: &AtomicBool) -> bool {
    if which("magick").is_none() {
        return false;
    }
    let out = temp_file("txt");
    let argv = vec!["magick".into(), format!("{}[0]", s(p)), "-format".into(), "%[fx:standard_deviation]".into(), "info:".into()];
    let ok = run(&argv, Some(&out), cancel, Duration::from_secs(10));
    let sd = std::fs::read_to_string(&out).ok().and_then(|t| t.trim().parse::<f64>().ok());
    let _ = std::fs::remove_file(&out);
    ok && sd.is_some_and(|sd| sd < 0.004)
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
    let names = archive_names(path, cancel);
    let part = office_text_part(&names)?;
    let xml = archive_text(path, &part, cancel)?;
    let t = office_xml_text(&xml, limit);
    (!t.is_empty()).then_some(t)
}

// ---------- LibreOffice ----------

pub fn has_libreoffice() -> bool {
    which("soffice").is_some() || which("libreoffice").is_some()
}

/// An office document converted to PDF by LibreOffice (headless, with its
/// own profile so it doesn't touch a running LibreOffice), cached in
/// ~/.cache/grove/office by path and modification time. Slow (seconds):
/// one at a time.
pub fn office_pdf(path: &Path, cancel: &AtomicBool) -> Option<PathBuf> {
    static ONE: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
    let exe = if which("soffice").is_some() { "soffice" } else { "libreoffice" };
    which(exe)?;
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
    let work = temp_file("lo");
    std::fs::create_dir_all(&work).ok()?;
    let profile = std::env::temp_dir().join(format!("grove-lo-profile-{}", unsafe { libc::getuid() }));
    let argv = vec![
        exe.to_string(),
        format!("-env:UserInstallation={}", crate::util::file_uri(&profile)),
        "--headless".into(),
        "--norestore".into(),
        "--convert-to".into(),
        "pdf".into(),
        "--outdir".into(),
        s(&work),
        s(path),
    ];
    let ok = run(&argv, None, cancel, Duration::from_secs(120));
    let made = std::fs::read_dir(&work).ok().and_then(|rd| rd.flatten().map(|e| e.path()).find(|p| p.extension().map(|e| e == "pdf").unwrap_or(false)));
    let result = match made {
        Some(p) if ok => std::fs::rename(&p, &target).or_else(|_| std::fs::copy(&p, &target).map(|_| ())).ok().map(|_| target),
        _ => None,
    };
    let _ = std::fs::remove_dir_all(&work);
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
    if m.starts_with("application/vnd.oasis.opendocument") || m.starts_with("application/vnd.openxmlformats-officedocument")
        || matches!(ext, "odt" | "ods" | "odp" | "odg" | "docx" | "xlsx" | "pptx" | "ott" | "ots" | "otp")
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
    fn pdfinfo_parsing() {
        let t = "Title:           A Book\nPages:           3\nPage    1 size: 612 x 792 pts (letter)\nPage    1 rot:  0\nPage    2 size: 612 x 792 pts (letter)\nPage    2 rot:  90\n";
        let i = parse_pdfinfo(t);
        assert_eq!(i.pages, 3);
        assert_eq!(i.title, "A Book");
        assert_eq!(i.sizes, vec![(612.0, 792.0), (792.0, 612.0)]);
        let j = parse_pdfinfo("Pages: 1\nPage size: 100 x 200 pts\n");
        assert_eq!(j.sizes, vec![(100.0, 200.0)]);
    }

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
