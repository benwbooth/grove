//! The freedesktop.org thumbnail specification: thumbnails cached in
//! ~/.cache/thumbnails/{normal,large,x-large,xx-large}/<md5 of URI>.png
//! (shared with every other app), checked against the file's mtime, and
//! made by the system's thumbnailers (*.thumbnailer files, e.g.
//! ffmpegthumbnailer) when grove can't decode the file itself.

use crate::util::{self, file_uri};
use md5::{Digest, Md5};
use std::collections::HashMap;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub const FLAVORS: [(&str, u32); 4] = [("normal", 128), ("large", 256), ("x-large", 512), ("xx-large", 1024)];
const FAIL_DIR: &str = "fail/grove-1.0";

pub fn root() -> PathBuf {
    util::cache_home().join("thumbnails")
}

pub fn md5_hex(s: &str) -> String {
    let d = Md5::digest(s.as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// The smallest flavor at least `px` big (xx-large for anything larger).
pub fn flavor_for(px: u32) -> (&'static str, u32) {
    for f in FLAVORS {
        if px <= f.1 {
            return f;
        }
    }
    FLAVORS[3]
}

/// Where the thumbnail of `path` at `px` lives (whether or not it exists).
pub fn target(path: &Path, px: u32) -> PathBuf {
    let (dir, _) = flavor_for(px);
    root().join(dir).join(format!("{}.png", md5_hex(&file_uri(path))))
}

fn fail_path(path: &Path) -> PathBuf {
    root().join(FAIL_DIR).join(format!("{}.png", md5_hex(&file_uri(path))))
}

/// tEXt chunks of a PNG (keyword -> value).
pub fn png_text(bytes: &[u8]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if bytes.len() < 8 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return out;
    }
    let mut i = 8;
    while i + 8 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;
        let ty = &bytes[i + 4..i + 8];
        let data_end = i + 8 + len;
        if data_end + 4 > bytes.len() {
            break;
        }
        let data = &bytes[i + 8..data_end];
        if ty == b"tEXt" {
            if let Some(z) = data.iter().position(|&b| b == 0) {
                out.insert(String::from_utf8_lossy(&data[..z]).into_owned(), String::from_utf8_lossy(&data[z + 1..]).into_owned());
            }
        } else if ty == b"iTXt" {
            // keyword\0 flag method lang\0 translated\0 text
            let parts: Vec<&[u8]> = data.splitn(2, |&b| b == 0).collect();
            if parts.len() == 2 && parts[1].len() >= 2 && parts[1][0] == 0 {
                let rest = &parts[1][2..];
                let mut it = rest.splitn(3, |&b| b == 0);
                let (_lang, _tr, text) = (it.next(), it.next(), it.next());
                if let Some(t) = text {
                    out.insert(String::from_utf8_lossy(parts[0]).into_owned(), String::from_utf8_lossy(t).into_owned());
                }
            }
        } else if ty == b"IDAT" || ty == b"IEND" {
            // Text after the image data is allowed but rare; keep reading
            // only until IEND.
            if ty == b"IEND" {
                break;
            }
        }
        i = data_end + 4;
    }
    out
}

/// Only the PNG's header part (thumbnails put their text before IDAT).
fn read_head(p: &Path) -> Option<Vec<u8>> {
    let mut f = std::fs::File::open(p).ok()?;
    let mut buf = vec![0u8; 4096];
    let n = f.read(&mut buf).ok()?;
    buf.truncate(n);
    Some(buf)
}

fn read_all(p: &Path) -> Option<Vec<u8>> {
    std::fs::read(p).ok()
}

fn fresh(thumb: &Path, mtime: i64) -> bool {
    let Some(head) = read_head(thumb) else { return false };
    let mut t = png_text(&head);
    if !t.contains_key("Thumb::MTime") {
        if let Some(all) = read_all(thumb) {
            t = png_text(&all);
        }
    }
    t.get("Thumb::MTime").and_then(|v| v.trim().parse::<i64>().ok()) == Some(mtime)
}

fn mtime_of(path: &Path) -> Option<(i64, u64)> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.mtime(), m.len()))
}

/// A thumbnail that's up to date: the requested flavor or a larger one.
pub fn lookup(path: &Path, px: u32) -> Option<PathBuf> {
    let (mtime, _) = mtime_of(path)?;
    let uri_md5 = md5_hex(&file_uri(path));
    let (want, _) = flavor_for(px);
    let mut started = false;
    for (dir, _) in FLAVORS {
        if dir == want {
            started = true;
        }
        if !started {
            continue;
        }
        let p = root().join(dir).join(format!("{uri_md5}.png"));
        if p.exists() && fresh(&p, mtime) {
            return Some(p);
        }
    }
    None
}

/// Thumbnails grove (or anyone) shouldn't make: the cache itself.
pub fn should_skip(path: &Path) -> bool {
    path.starts_with(root())
}

pub fn failed(path: &Path) -> bool {
    match mtime_of(path) {
        Some((m, _)) => {
            let f = fail_path(path);
            f.exists() && fresh(&f, m)
        }
        None => true,
    }
}

/// Remember that no thumbnail can be made (until the file changes).
pub fn mark_failed(path: &Path) {
    let Some((m, size)) = mtime_of(path) else { return };
    let png = add_text(TINY_PNG, &meta_pairs(path, m, size, ""));
    let f = fail_path(path);
    let _ = std::fs::create_dir_all(f.parent().unwrap());
    let _ = write_private(&f, &png);
}

const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

/// The metadata keys the spec asks for.
pub fn meta_pairs(path: &Path, mtime: i64, size: u64, mime: &str) -> Vec<(String, String)> {
    let mut v = vec![
        ("Thumb::URI".to_string(), file_uri(path)),
        ("Thumb::MTime".to_string(), mtime.to_string()),
        ("Thumb::Size".to_string(), size.to_string()),
        ("Software".to_string(), "grove".to_string()),
    ];
    if !mime.is_empty() {
        v.push(("Thumb::Mimetype".to_string(), mime.to_string()));
    }
    v
}

/// The same keys for a path, from its current metadata (for the Qt side,
/// which writes built-in thumbnails itself).
pub fn meta_for(path: &Path, mime: &str) -> Vec<(String, String)> {
    let (m, s) = mtime_of(path).unwrap_or((0, 0));
    meta_pairs(path, m, s, mime)
}

/// Insert tEXt chunks right after IHDR (replacing ones with the same key).
pub fn add_text(png: &[u8], pairs: &[(String, String)]) -> Vec<u8> {
    if png.len() < 33 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        return png.to_vec();
    }
    let keys: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
    let mut out = Vec::with_capacity(png.len() + 256);
    out.extend_from_slice(&png[..8]);
    let mut i = 8;
    let mut inserted = false;
    while i + 8 <= png.len() {
        let len = u32::from_be_bytes([png[i], png[i + 1], png[i + 2], png[i + 3]]) as usize;
        let end = (i + 12 + len).min(png.len());
        let ty = &png[i + 4..i + 8];
        let skip = ty == b"tEXt" && {
            let data = &png[i + 8..i + 8 + len.min(png.len() - i - 8)];
            let k = data.split(|&b| b == 0).next().unwrap_or(&[]);
            keys.iter().any(|x| x.as_bytes() == k)
        };
        if !skip {
            out.extend_from_slice(&png[i..end]);
        }
        if ty == b"IHDR" && !inserted {
            inserted = true;
            for (k, v) in pairs {
                let mut data = Vec::new();
                data.extend_from_slice(k.as_bytes());
                data.push(0);
                data.extend_from_slice(v.as_bytes());
                out.extend_from_slice(&(data.len() as u32).to_be_bytes());
                let mut h = crc32fast::Hasher::new();
                h.update(b"tEXt");
                h.update(&data);
                out.extend_from_slice(b"tEXt");
                out.extend_from_slice(&data);
                out.extend_from_slice(&h.finalize().to_be_bytes());
            }
        }
        i = end;
    }
    out
}

fn write_private(p: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = p.with_extension(format!("png.grove-{}", std::process::id()));
    std::fs::write(&tmp, data)?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    std::fs::rename(&tmp, p)
}

/// Store PNG bytes as the thumbnail of `path` (adding the spec's keys).
pub fn store(path: &Path, px: u32, png: &[u8], mime: &str) -> Option<PathBuf> {
    let (m, s) = mtime_of(path)?;
    let t = target(path, px);
    std::fs::create_dir_all(t.parent()?).ok()?;
    let _ = std::fs::set_permissions(root(), std::fs::Permissions::from_mode(0o700));
    write_private(&t, &add_text(png, &meta_pairs(path, m, s, mime))).ok()?;
    Some(t)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Thumbnailer {
    pub file: PathBuf,
    pub exec: String,
    pub try_exec: String,
    pub mime_types: Vec<String>,
}

pub fn parse_thumbnailer(file: &Path, text: &str) -> Option<Thumbnailer> {
    let mut in_entry = false;
    let mut t = Thumbnailer { file: file.to_path_buf(), exec: String::new(), try_exec: String::new(), mime_types: vec![] };
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Thumbnailer Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "Exec" => t.exec = v.trim().to_string(),
                "TryExec" => t.try_exec = v.trim().to_string(),
                "MimeType" => t.mime_types = v.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                _ => {}
            }
        }
    }
    (!t.exec.is_empty() && !t.mime_types.is_empty()).then_some(t)
}

pub fn which(cmd: &str) -> Option<PathBuf> {
    if cmd.contains('/') {
        let p = PathBuf::from(cmd);
        return p.is_file().then_some(p);
    }
    std::env::var_os("PATH").and_then(|path| std::env::split_paths(&path).map(|d| d.join(cmd)).find(|p| p.is_file()))
}

fn load_thumbnailers() -> Vec<Thumbnailer> {
    let mut dirs = vec![util::data_home()];
    dirs.extend(util::data_dirs());
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(d.join("thumbnailers")) else { continue };
        let mut files: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().map(|e| e == "thumbnailer").unwrap_or(false)).collect();
        files.sort();
        for f in files {
            let name = f.file_name().unwrap().to_owned();
            if !seen.insert(name) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&f) else { continue };
            if let Some(t) = parse_thumbnailer(&f, &text) {
                let exe = if t.try_exec.is_empty() { crate::apps::split_exec(&t.exec).first().cloned().unwrap_or_default() } else { t.try_exec.clone() };
                if which(&exe).is_some() {
                    out.push(t);
                }
            }
        }
    }
    out
}

pub fn thumbnailers() -> &'static [Thumbnailer] {
    static T: OnceLock<Vec<Thumbnailer>> = OnceLock::new();
    T.get_or_init(load_thumbnailers)
}

/// The thumbnailer for the first of `mimes` (a type and its parents) that
/// one handles.
pub fn find_thumbnailer<'a>(list: &'a [Thumbnailer], mimes: &[String]) -> Option<&'a Thumbnailer> {
    for m in mimes {
        if let Some(t) = list.iter().find(|t| t.mime_types.iter().any(|x| x == m)) {
            return Some(t);
        }
    }
    None
}

/// Expand a thumbnailer's Exec line: %i input path, %u input URI, %o
/// output path, %s size, %% a percent sign.
pub fn expand_exec(exec: &str, input: &Path, output: &Path, px: u32) -> Vec<String> {
    crate::apps::split_exec(exec)
        .into_iter()
        .map(|arg| {
            let mut s = String::new();
            let mut it = arg.chars().peekable();
            while let Some(c) = it.next() {
                if c == '%' {
                    match it.next() {
                        Some('i') => s.push_str(&input.to_string_lossy()),
                        Some('u') => s.push_str(&file_uri(input)),
                        Some('o') => s.push_str(&output.to_string_lossy()),
                        Some('s') => s.push_str(&px.to_string()),
                        Some('%') => s.push('%'),
                        Some(_) | None => {}
                    }
                } else {
                    s.push(c);
                }
            }
            s
        })
        .collect()
}

/// Make a thumbnail with an external thumbnailer. Blocks (call it from a
/// worker); gives up after 30 s or when `cancel` is set.
pub fn generate_external(t: &Thumbnailer, path: &Path, px: u32, mime: &str, cancel: &AtomicBool) -> Option<PathBuf> {
    let (_, size_px) = flavor_for(px);
    let tmpdir = std::env::temp_dir().join(format!("grove-thumb-{}", std::process::id()));
    std::fs::create_dir_all(&tmpdir).ok()?;
    let out = tmpdir.join(format!("{}.png", md5_hex(&format!("{}{}", file_uri(path), size_px))));
    let argv = expand_exec(&t.exec, path, &out, size_px);
    let (cmd, args) = argv.split_first()?;
    let mut child = std::process::Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let start = Instant::now();
    let ok = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st.success(),
            Ok(None) => {
                if cancel.load(Ordering::Relaxed) || start.elapsed() > Duration::from_secs(30) {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = std::fs::remove_file(&out);
                    return None;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(_) => break false,
        }
    };
    let bytes = std::fs::read(&out).ok();
    let _ = std::fs::remove_file(&out);
    match bytes {
        Some(b) if ok && b.starts_with(b"\x89PNG") => store(path, size_px, &b, mime),
        _ => {
            if !cancel.load(Ordering::Relaxed) {
                mark_failed(path);
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_matches_spec_example() {
        // The spec's example: file:///home/jens/photos/me.png
        assert_eq!(md5_hex("file:///home/jens/photos/me.png"), "c6ee772d9e49320e97ec29a7eb5b1697");
        assert_eq!(flavor_for(64), ("normal", 128));
        assert_eq!(flavor_for(200), ("large", 256));
        assert_eq!(flavor_for(4000), ("xx-large", 1024));
    }

    #[test]
    fn text_chunks_roundtrip() {
        let pairs = vec![("Thumb::MTime".to_string(), "123".to_string()), ("Thumb::URI".to_string(), "file:///x".to_string())];
        let png = add_text(TINY_PNG, &pairs);
        let t = png_text(&png);
        assert_eq!(t.get("Thumb::MTime").unwrap(), "123");
        // Replacing keeps one copy.
        let png2 = add_text(&png, &[("Thumb::MTime".to_string(), "456".to_string())]);
        let t2 = png_text(&png2);
        assert_eq!(t2.get("Thumb::MTime").unwrap(), "456");
        assert_eq!(t2.get("Thumb::URI").unwrap(), "file:///x");
        assert_eq!(png2.len(), png.len());
    }

    #[test]
    fn parse_and_expand() {
        let t = parse_thumbnailer(Path::new("/x.thumbnailer"), "[Thumbnailer Entry]\nTryExec=ffmpegthumbnailer\nExec=ffmpegthumbnailer -i %i -o %o -s %s -f\nMimeType=video/mp4;video/webm;\n").unwrap();
        assert_eq!(t.mime_types, vec!["video/mp4", "video/webm"]);
        let a = expand_exec(&t.exec, Path::new("/v/a b.mp4"), Path::new("/o.png"), 256);
        assert_eq!(a, vec!["ffmpegthumbnailer", "-i", "/v/a b.mp4", "-o", "/o.png", "-s", "256", "-f"]);
        let list = vec![t];
        assert!(find_thumbnailer(&list, &["video/x-matroska".into(), "video/webm".into()]).is_some());
        assert!(find_thumbnailer(&list, &["image/png".into()]).is_none());
    }

    #[test]
    fn store_lookup_fail_cycle_with_fake_thumbnailer() {
        let _g = crate::test_env_lock();
        let t = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CACHE_HOME", t.path().join("cache"));
        let f = t.path().join("video.mp4");
        std::fs::write(&f, "fake").unwrap();
        assert!(lookup(&f, 128).is_none());
        // A "thumbnailer" that copies a PNG to %o.
        let src_png = t.path().join("src.png");
        std::fs::write(&src_png, TINY_PNG).unwrap();
        let th = Thumbnailer { file: "/x".into(), exec: format!("cp {} %o", src_png.display()), try_exec: String::new(), mime_types: vec!["video/mp4".into()] };
        let c = AtomicBool::new(false);
        let out = generate_external(&th, &f, 100, "video/mp4", &c).unwrap();
        assert_eq!(out, target(&f, 128));
        assert_eq!(lookup(&f, 128).unwrap(), out);
        let meta = png_text(&std::fs::read(&out).unwrap());
        assert_eq!(meta.get("Thumb::URI").unwrap(), &file_uri(&f));
        assert_eq!(std::fs::metadata(&out).unwrap().permissions().mode() & 0o777, 0o600);
        // A larger request isn't satisfied by the normal one.
        assert!(lookup(&f, 256).is_none());
        // Changing the file makes it stale.
        let ft = std::fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1000));
        std::fs::File::options().write(true).open(&f).unwrap().set_times(ft).unwrap();
        assert!(lookup(&f, 128).is_none());
        // A thumbnailer that fails marks the file as failed.
        let bad = Thumbnailer { exec: "false %o".into(), ..th };
        assert!(generate_external(&bad, &f, 100, "", &c).is_none());
        assert!(failed(&f));
        std::env::remove_var("XDG_CACHE_HOME");
    }
}
