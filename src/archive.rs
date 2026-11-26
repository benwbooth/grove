//! Archives, read in-process (grove_core::archives: libarchive for zip,
//! tar.*, 7z, rar, iso...; Rust readers for .msi and .dmg): list them
//! (browse as folders, Quick Look), extract, compress.

use crate::bridge::ffi::{self, QString};
use cxx_qt::CxxQtThread;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ArchEntry {
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
    pub mtime_text: String,
}

/// A lone compressed file (not a tar): "notes.txt.gz" -> "notes.txt". It
/// browses as a folder holding that one file, decompressed by libarchive
/// (libarchive: gzip, xz, bzip2, zstd, lzip, lz4, lzma, compress).
pub fn single_compressed(name: &str) -> Option<String> {
    let n = name.to_lowercase();
    const SUFFIXES: &[&str] = &[".gz", ".xz", ".bz2", ".bz", ".zst", ".zstd", ".lz", ".lzma", ".lz4", ".z"];
    let suf = SUFFIXES.iter().find(|s| n.ends_with(*s))?;
    let stem = &name[..name.len() - suf.len()];
    if stem.is_empty() || stem.to_lowercase().ends_with(".tar") {
        return None;
    }
    Some(stem.to_string())
}

fn single_of(path: &str) -> Option<String> {
    Path::new(path).file_name().and_then(|n| single_compressed(&n.to_string_lossy()))
}

/// Decompress a lone compressed file to `out` (libarchive).
fn decompress_to(archive: &str, out: &Path) -> Result<(), String> {
    grove_core::libarchive::decompress(Path::new(archive), out, &std::sync::atomic::AtomicBool::new(false)).map_err(|e| format!("Couldn't decompress: {e}"))
}

/// Every container libarchive can list and extract: zip and its
/// relatives, tar with any compression, 7z, rar, cab, iso/udf, cpio, ar/deb,
/// rpm, xar/pkg, lha, warc, comic book archives. Documents that happen to be
/// zips (EPUB, ODF, OOXML) aren't on the list: they open in their apps.
/// A path typed or pasted that goes through an archive
/// ("/home/me/x.zip/docs/a.txt"): grove's address for it
/// ("archive:///home/me/x.zip#docs/a.txt"). Anything else comes back as it was.
pub fn address_for(typed: &str) -> String {
    let t = typed.trim_end_matches('/');
    if t.is_empty() || !t.starts_with('/') || std::path::Path::new(t).exists() {
        return typed.to_string();
    }
    // The nearest part that exists: an archive file holding the rest.
    let mut cut = t.len();
    while let Some(i) = t[..cut].rfind('/') {
        if i == 0 {
            break;
        }
        let head = &t[..i];
        if let Ok(m) = std::fs::metadata(head) {
            let name = head.rsplit('/').next().unwrap_or(head);
            if m.is_file() && is_archive("", name) {
                return format!("archive://{head}#{}", &t[i + 1..]);
            }
            break;
        }
        cut = i;
    }
    typed.to_string()
}

pub fn is_archive(mime: &str, name: &str) -> bool {
    let n = name.to_lowercase();
    const MIMES: &[&str] = &[
        "application/zip", "application/x-zip-compressed", "application/x-tar", "application/x-compressed-tar",
        "application/x-xz-compressed-tar", "application/x-zstd-compressed-tar", "application/x-bzip2-compressed-tar",
        "application/x-lzma-compressed-tar", "application/x-lzip-compressed-tar", "application/x-lz4-compressed-tar",
        "application/x-tarz", "application/x-7z-compressed", "application/vnd.rar", "application/x-rar",
        "application/x-rar-compressed", "application/x-iso9660-image", "application/x-cd-image", "application/x-udf-image",
        "application/x-cpio", "application/x-cpio-compressed", "application/vnd.debian.binary-package",
        "application/x-deb", "application/x-rpm", "application/x-source-rpm", "application/java-archive",
        "application/x-archive", "application/vnd.ms-cab-compressed", "application/x-cabinet", "application/x-xar",
        "application/x-lha", "application/x-lzh-compressed", "application/x-cbz", "application/vnd.comicbook+zip",
        "application/x-cbr", "application/vnd.comicbook-rar", "application/x-cb7", "application/x-cbt",
        "application/x-apple-diskimage", "application/x-ms-wim", "application/x-msi", "application/x-java-archive",
    ];
    const EXTS: &[&str] = &[
        ".zip", ".zipx", ".tar", ".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.zst", ".tzst", ".tar.bz2", ".tbz", ".tbz2",
        ".tar.lz", ".tar.lzma", ".tlz", ".tar.lz4", ".tar.z", ".taz", ".7z", ".rar", ".iso", ".udf", ".cpio", ".jar",
        ".war", ".ear", ".apk", ".aar", ".xpi", ".crx", ".whl", ".nupkg", ".vsix", ".deb", ".udeb", ".ipk", ".rpm",
        ".ar", ".a", ".cab", ".xar", ".pkg", ".lha", ".lzh", ".cbz", ".cbr", ".cb7", ".cbt", ".warc",
    ];
    MIMES.contains(&mime) || EXTS.iter().any(|e| n.ends_with(e)) || grove_core::archives::EXTRA_EXTS.iter().any(|e| n.ends_with(e)) || single_compressed(name).is_some()
}

/// Listing, remembered while the archive file is unchanged (browsing a big
/// archive lists it once, not on every folder inside it).
pub fn list(path: &str) -> Result<Vec<ArchEntry>, String> {
    type Cache = std::collections::HashMap<String, (u64, i64, std::sync::Arc<Vec<ArchEntry>>)>;
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Cache>> = std::sync::OnceLock::new();
    let key = std::fs::metadata(path).map(|m| (m.len(), std::os::unix::fs::MetadataExt::mtime(&m))).unwrap_or((0, 0));
    let cache = CACHE.get_or_init(Default::default);
    if let Some((l, t, v)) = cache.lock().unwrap().get(path) {
        if (*l, *t) == key {
            return Ok(v.as_ref().clone());
        }
    }
    let v = list_uncached(path)?;
    let mut c = cache.lock().unwrap();
    if c.len() > 24 {
        c.clear();
    }
    c.insert(path.to_string(), (key.0, key.1, std::sync::Arc::new(v.clone())));
    Ok(v)
}

/// Archives inside archives: the deepest container holding `inner` (each
/// nested archive copied once to grove's cache, keyed by its outer file,
/// as Kog does), the consumed prefix ("docs/inner.tar.gz/"), and the rest
/// of the path inside that container. Up to 4 levels.
pub fn resolve(archive: &str, inner: &str) -> Result<(String, String, String), String> {
    let mut container = archive.to_string();
    let mut prefix = String::new();
    let mut rest = inner.trim_matches('/').to_string();
    for _ in 0..4 {
        if rest.is_empty() {
            break;
        }
        let entries = list(&container)?;
        let parts: Vec<&str> = rest.split('/').collect();
        let mut hit = None;
        for len in (1..=parts.len()).rev() {
            let cand = parts[..len].join("/");
            let name = parts[len - 1];
            if entries.iter().any(|e| !e.is_dir && e.path == cand) && is_archive("", name) {
                hit = Some((cand, parts[len..].join("/")));
                break;
            }
        }
        let Some((link, remainder)) = hit else { break };
        container = extract_member(&container, &link)?.display().to_string();
        prefix = format!("{prefix}{link}/");
        rest = remainder;
    }
    Ok((container, prefix, rest))
}

fn list_uncached(path: &str) -> Result<Vec<ArchEntry>, String> {
    if let Some(inner) = single_of(path) {
        let size = single_uncompressed(path).unwrap_or(0);
        return Ok(vec![ArchEntry { path: inner, size, is_dir: false, mtime_text: String::new() }]);
    }
    let never = std::sync::atomic::AtomicBool::new(false);
    let v = grove_core::archives::list(Path::new(path), &never)?;
    Ok(v.into_iter().map(|m| ArchEntry { path: m.path, size: m.size, is_dir: m.is_dir, mtime_text: when(m.mtime) }).collect())
}

/// A member's time as shown in lists ("2026-10-15 19:43").
fn when(secs: i64) -> String {
    if secs <= 0 {
        return String::new();
    }
    let t = grove_core::util::local_timestamp(secs).replace('T', " ");
    t[..t.len().min(16)].to_string()
}

/// The decompressed size of a lone compressed file. gzip records it (mod
/// 4 GiB) in its last four bytes; anything else is decompressed and
/// counted, up to 512 MB compressed (bigger: unknown).
pub fn single_uncompressed(path: &str) -> Option<u64> {
    use std::io::{Read, Seek, SeekFrom};
    let len = std::fs::metadata(path).ok()?.len();
    if path.to_lowercase().ends_with(".gz") && len >= 18 && len < (1u64 << 32) {
        let mut f = std::fs::File::open(path).ok()?;
        let mut magic = [0u8; 2];
        f.read_exact(&mut magic).ok()?;
        if magic == [0x1f, 0x8b] {
            f.seek(SeekFrom::End(-4)).ok()?;
            let mut b = [0u8; 4];
            f.read_exact(&mut b).ok()?;
            let n = u32::from_le_bytes(b) as u64;
            // A multi-member or odd file can lie; only trust a plausible size.
            if n >= len / 4 || len < 1024 {
                return Some(n);
            }
        }
    }
    if len > 512 << 20 {
        return None;
    }
    grove_core::libarchive::decompressed_size(Path::new(path), &std::sync::atomic::AtomicBool::new(false))
}

/// Compressed (on disk) and uncompressed sizes of an archive or lone
/// compressed file, plus how many files it holds.
pub fn sizes_json(path: &str) -> Value {
    let compressed = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let entries = match list(path) {
        Ok(v) => v,
        Err(e) => return json!({"error": e}),
    };
    let files = entries.iter().filter(|e| !e.is_dir).count();
    let known = single_of(path).is_none() || entries.iter().any(|e| e.size > 0);
    let uncompressed: u64 = entries.iter().map(|e| e.size).sum();
    let hs = grove_core::util::human_size;
    json!({
        "compressed": compressed, "compressedText": hs(compressed),
        "uncompressed": if known { json!(uncompressed) } else { Value::Null },
        "uncompressedText": if known { hs(uncompressed) } else { "unknown".to_string() },
        "ratio": if known && uncompressed > 0 { json!(compressed as f64 / uncompressed as f64) } else { Value::Null },
        "files": files,
    })
}

pub fn list_json(path: &str) -> Value {
    match list(path) {
        Ok(v) => json!({"entries": v}),
        Err(e) => json!({"error": e}),
    }
}

/// The items directly inside `inner` ("" = top) of an archive listing,
/// with folders that only appear as path prefixes filled in.
pub fn children(entries: &[ArchEntry], inner: &str) -> Vec<ArchEntry> {
    let prefix = if inner.is_empty() { String::new() } else { format!("{}/", inner.trim_end_matches('/')) };
    let mut out: BTreeMap<String, ArchEntry> = BTreeMap::new();
    for e in entries {
        let Some(rest) = e.path.strip_prefix(&prefix) else { continue };
        if rest.is_empty() {
            continue;
        }
        match rest.split_once('/') {
            Some((dir, _)) => {
                out.entry(dir.to_string()).or_insert(ArchEntry { path: format!("{prefix}{dir}"), size: 0, is_dir: true, mtime_text: String::new() });
            }
            None => {
                out.insert(rest.to_string(), e.clone());
            }
        }
    }
    out.into_values().collect()
}

fn job_msg(qt: &CxxQtThread<ffi::Grove>, id: i32, v: Value) {
    let s = v.to_string();
    let _ = qt.queue(move |g| g.job_changed(id, QString::from(s.as_str())));
}

fn stem(p: &Path) -> String {
    let n = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    grove_core::ops::split_name(&n, false).0
}

/// Extract next to the archive (or into `dest`): into a folder named after
/// it unless everything is already in one top folder (as Finder does).
pub fn extract_job(qt: CxxQtThread<ffi::Grove>, id: i32, archive: String, dest: String) {
    std::thread::spawn(move || {
        let a = PathBuf::from(&archive);
        let title = format!("Extracting “{}”", a.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        job_msg(&qt, id, json!({"id": id, "title": title, "state": "running", "indeterminate": true}));
        let dest_dir = if dest.is_empty() { a.parent().unwrap_or(Path::new("/")).to_path_buf() } else { PathBuf::from(&dest) };
        // A lone compressed file: decompress it next to itself.
        if let Some(inner) = single_of(&archive) {
            let mut out = dest_dir.join(&inner);
            if out.exists() {
                out = grove_core::ops::unique_name(&out);
            }
            match decompress_to(&archive, &out) {
                Ok(()) => {
                    crate::controller::push_history_any("Extract", grove_core::ops::Action::Exists { paths: vec![out.clone()] });
                    job_msg(&qt, id, json!({"id": id, "title": title, "state": "done", "results": [out], "dest": dest_dir}));
                }
                Err(e) => job_msg(&qt, id, json!({"id": id, "title": title, "state": "failed", "errors": [e]})),
            }
            let s = serde_json::to_string(&vec![dest_dir.to_string_lossy()]).unwrap();
            let _ = qt.queue(move |g| g.folders_touched(QString::from(s.as_str())));
            return;
        }
        let entries = match list(&archive) {
            Ok(e) => e,
            Err(e) => return job_msg(&qt, id, json!({"id": id, "title": title, "state": "failed", "errors": [e]})),
        };
        let tops: HashSet<String> = entries.iter().map(|e| e.path.split('/').next().unwrap_or("").to_string()).collect();
        let single_top = tops.len() == 1;
        let (target, created) = if single_top {
            let t = dest_dir.join(tops.iter().next().unwrap());
            if t.exists() {
                // Clash: extract into "<archive name> 2/".
                let d = grove_core::ops::unique_name(&dest_dir.join(stem(&a)));
                (d.clone(), d)
            } else {
                (dest_dir.clone(), t)
            }
        } else {
            let base = dest_dir.join(stem(&a));
            let d = if base.exists() { grove_core::ops::unique_name(&base) } else { base };
            (d.clone(), d)
        };
        let _ = std::fs::create_dir_all(&target);
        match grove_core::archives::extract(Path::new(&archive), &target, &[], &std::sync::atomic::AtomicBool::new(false)) {
            Ok(()) => {
                crate::controller::push_history_any("Extract", grove_core::ops::Action::Exists { paths: vec![created.clone()] });
                job_msg(&qt, id, json!({"id": id, "title": title, "state": "done", "results": [created], "dest": dest_dir}));
            }
            Err(e) => job_msg(&qt, id, json!({"id": id, "title": title, "state": "failed", "errors": [e]})),
        }
        let s = serde_json::to_string(&vec![dest_dir.to_string_lossy()]).unwrap();
        let _ = qt.queue(move |g| g.folders_touched(QString::from(s.as_str())));
    });
}

/// Compress items into one archive next to them: "Archive.zip" (several)
/// or "<name>.zip"; format "zip" or "tar.zst".
pub fn compress_job(qt: CxxQtThread<ffi::Grove>, id: i32, paths: Vec<PathBuf>, format: String) {
    std::thread::spawn(move || {
        let Some(first) = paths.first() else { return };
        let dir = first.parent().unwrap_or(Path::new("/")).to_path_buf();
        let ext = if format == "tar.zst" { "tar.zst" } else { "zip" };
        let base = if paths.len() == 1 { first.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() } else { "Archive".into() };
        let mut out = dir.join(format!("{base}.{ext}"));
        if out.exists() {
            out = grove_core::ops::unique_name(&out);
        }
        let title = format!("Compressing into “{}”", out.file_name().unwrap().to_string_lossy());
        job_msg(&qt, id, json!({"id": id, "title": title, "state": "running", "indeterminate": true}));
        let fmt = if format == "tar.zst" { grove_core::libarchive::Format::TarZst } else { grove_core::libarchive::Format::Zip };
        match grove_core::libarchive::create(&out, &paths, fmt, &std::sync::atomic::AtomicBool::new(false)) {
            Ok(()) => {
                crate::controller::push_history_any("Compress", grove_core::ops::Action::Exists { paths: vec![out.clone()] });
                job_msg(&qt, id, json!({"id": id, "title": title, "state": "done", "results": [out], "dest": dir}));
            }
            Err(e) => job_msg(&qt, id, json!({"id": id, "title": title, "state": "failed", "errors": [format!("Couldn't compress: {e}")]})),
        }
        let s = serde_json::to_string(&vec![dir.to_string_lossy()]).unwrap();
        let _ = qt.queue(move |g| g.folders_touched(QString::from(s.as_str())));
    });
}

fn member_dir(archive: &str) -> PathBuf {
    grove_core::util::cache_home().join("grove/archives").join(grove_core::thumbs::md5_hex(archive))
}

/// A member already taken out (and not older than its archive).
fn cached_member(archive: &str, inner: &str) -> Option<PathBuf> {
    let out = member_dir(archive).join(inner);
    let m = std::fs::metadata(&out).ok()?;
    let a = std::fs::metadata(archive).ok()?;
    (m.is_file() && m.modified().ok()? >= a.modified().ok()?).then_some(out)
}

/// The local copy of "archive://A#inner" for previews: already taken out,
/// or (with `extract`) taken out now. None for anything else.
pub fn local_copy(path: &str, extract: bool) -> Option<PathBuf> {
    let rest = path.strip_prefix("archive://")?;
    let (a, inner) = rest.split_once('#')?;
    if inner.is_empty() {
        return None;
    }
    if let Some(p) = cached_member(a, inner) {
        return Some(p);
    }
    if !extract {
        return None;
    }
    // Through any archives on the way (nested archives).
    let (container, _, leaf) = resolve(a, inner).ok()?;
    if leaf.is_empty() {
        // The path names a nested archive itself: its cached copy.
        return Some(PathBuf::from(container));
    }
    extract_member(&container, &leaf).ok()
}

/// Extract one member to grove's cache (to open or preview it); reused
/// while the archive doesn't change. One extraction at a time (previews
/// of many members at once would otherwise race on the same files).
pub fn extract_member(archive: &str, inner: &str) -> Result<PathBuf, String> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(p) = cached_member(archive, inner) {
        return Ok(p);
    }
    let dir = member_dir(archive);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if single_of(archive).is_some() {
        let out = dir.join(inner);
        decompress_to(archive, &out)?;
        return Ok(out);
    }
    grove_core::archives::extract(Path::new(archive), &dir, &[inner], &std::sync::atomic::AtomicBool::new(false))
        .map_err(|e| format!("Couldn't take “{inner}” out of the archive: {e}"))?;
    Ok(dir.join(inner))
}

#[cfg(test)]
mod tests {
    #[test]
    fn typed_paths_through_archives() {
        let dir = std::env::temp_dir().join(format!("grove-addr-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zip = dir.join("x.zip");
        std::fs::write(&zip, b"PK").unwrap();
        let z = zip.to_string_lossy();
        assert_eq!(address_for(&format!("{z}/docs/a.txt")), format!("archive://{z}#docs/a.txt"));
        assert_eq!(address_for(&format!("{z}/docs/")), format!("archive://{z}#docs"));
        // Real folders and files, and paths through things that aren't archives, stay.
        let d = dir.to_string_lossy();
        assert_eq!(address_for(&d), d.to_string());
        assert_eq!(address_for(&format!("{d}/nope/x")), format!("{d}/nope/x"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;
    #[test]
    fn children_of() {
        let ent = |p: &str, d: bool| ArchEntry { path: p.into(), size: 1, is_dir: d, mtime_text: String::new() };
        let e = vec![ent("dir", true), ent("dir/a file.txt", false), ent("top.md", false)];
        let top = children(&e, "");
        assert_eq!(top.iter().map(|x| x.path.as_str()).collect::<Vec<_>>(), vec!["dir", "top.md"]);
        assert_eq!(children(&e, "dir").len(), 1);
        // Folders implied only by paths.
        let implied = vec![ent("x/y/z.txt", false)];
        assert_eq!(children(&implied, "")[0].path, "x");
        assert!(children(&implied, "x")[0].is_dir);
    }
    #[test]
    fn lone_compressed_files() {
        assert_eq!(single_compressed("notes.txt.gz").as_deref(), Some("notes.txt"));
        assert_eq!(single_compressed("dump.sql.zst").as_deref(), Some("dump.sql"));
        assert_eq!(single_compressed("x.tar.gz"), None);
        assert_eq!(single_compressed("readme.md"), None);
        assert!(is_archive("application/gzip", "log.1.gz"));
        assert!(is_archive("application/x-7z-compressed", "a.7z"));
        assert!(!is_archive("application/epub+zip", "book.epub"));
    }
}
