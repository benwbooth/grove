//! Grove Archiver's engine: 7-Zip's features without 7-Zip's program.
//!
//! - 7z: sevenz-rust2 (pure Rust): list (packed size, CRC, method, solid,
//!   encrypted names), extract, create (LZMA2/LZMA/PPMd/BZip2/Copy, levels,
//!   threads, solid blocks, AES-256 with or without encrypted file names),
//!   test.
//! - zip: libarchive writes it (Deflate/Store/BZip2/LZMA/XZ/Zstandard,
//!   levels, AES-256 or ZipCrypto) and reads it with the password; the zip
//!   crate adds packed size, CRC, method and the comment to the listing.
//! - tar (.gz .xz .bz2 .zst .lz4), lone compressed files (.gz .xz .bz2
//!   .zst): libarchive both ways; rar, iso, cab, cpio, deb, rpm, xar...:
//!   libarchive, read only.
//! - Split volumes (name.7z.001, .002...; name.part1.rar...): read as one
//!   file; new archives are split after writing.
//! - Changing an archive (add, delete, rename, new folder): taken out to a
//!   folder next to it, changed there, packed again with the same format
//!   and password, then swapped in (as 7-Zip writes a new archive).
//!
//! Every long call takes a `Progress`: cancel, pause, bytes done, the
//! current file, and questions (an existing file) answered by the UI.

use serde::Serialize;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};

// ------------------------------------------------------------------ errors

#[derive(Clone, Debug, PartialEq)]
pub enum ArcError {
    /// Encrypted and no password given.
    NeedPassword,
    /// The password given doesn't open it.
    BadPassword,
    Cancelled,
    /// This kind of archive can only be read.
    ReadOnly(String),
    Failed(String),
}

impl std::fmt::Display for ArcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArcError::NeedPassword => write!(f, "This archive is protected by a password."),
            ArcError::BadPassword => write!(f, "That password doesn't open this archive."),
            ArcError::Cancelled => write!(f, "Stopped."),
            ArcError::ReadOnly(k) => write!(f, "{k} archives can be read but not changed. Extract it, then make a new archive (7z or zip)."),
            ArcError::Failed(s) => write!(f, "{s}"),
        }
    }
}

impl ArcError {
    pub fn code(&self) -> &'static str {
        match self {
            ArcError::NeedPassword => "needPassword",
            ArcError::BadPassword => "badPassword",
            ArcError::Cancelled => "cancelled",
            ArcError::ReadOnly(_) => "readOnly",
            ArcError::Failed(_) => "failed",
        }
    }
}

type Res<T> = Result<T, ArcError>;

fn failed(s: impl Into<String>) -> ArcError {
    ArcError::Failed(s.into())
}

// ---------------------------------------------------------------- progress

/// What an existing file at the destination should do.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Overwrite {
    Ask,
    Replace,
    Skip,
    /// Extract under a new name ("file 2.txt").
    Rename,
    /// Rename the file already there, then extract.
    RenameExisting,
}

impl Overwrite {
    pub fn parse(s: &str) -> Overwrite {
        match s {
            "replace" => Overwrite::Replace,
            "skip" => Overwrite::Skip,
            "rename" => Overwrite::Rename,
            "renameExisting" => Overwrite::RenameExisting,
            _ => Overwrite::Ask,
        }
    }
}

/// A running job's shared state: the UI reads it, the job updates it.
#[derive(Default)]
pub struct Progress {
    pub cancel: AtomicBool,
    pub paused: AtomicBool,
    /// Progress fraction = done / total (bytes of whatever is measured).
    pub done: AtomicU64,
    pub total: AtomicU64,
    /// Bytes written out (extract) or read in (compress, test).
    pub bytes: AtomicU64,
    /// Bytes of the archive read or written (for the ratio).
    pub packed: AtomicU64,
    pub files: AtomicU64,
    pub errors: AtomicU64,
    pub current: Mutex<String>,
    /// A question for the UI (JSON) while the job waits for `answer`.
    question: Mutex<Option<String>>,
    answer: Mutex<Option<String>>,
    cv: Condvar,
}

impl Progress {
    pub fn new() -> Progress {
        Progress::default()
    }
    /// Stops here while paused; Err once cancelled.
    pub fn check(&self) -> Res<()> {
        while self.paused.load(Ordering::Relaxed) && !self.cancel.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
        if self.cancel.load(Ordering::Relaxed) {
            return Err(ArcError::Cancelled);
        }
        Ok(())
    }
    pub fn set_current(&self, s: &str) {
        *self.current.lock().unwrap() = s.to_string();
    }
    pub fn add(&self, n: u64) {
        self.done.fetch_add(n, Ordering::Relaxed);
        self.bytes.fetch_add(n, Ordering::Relaxed);
    }
    pub fn question(&self) -> Option<String> {
        self.question.lock().unwrap().clone()
    }
    /// The UI's answer to the open question.
    pub fn reply(&self, a: &str) {
        *self.answer.lock().unwrap() = Some(a.to_string());
        self.cv.notify_all();
    }
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.cv.notify_all();
    }
    /// Ask the UI (blocks the job thread, not the UI); "cancel" if stopped.
    pub fn ask(&self, q: serde_json::Value) -> String {
        *self.answer.lock().unwrap() = None;
        *self.question.lock().unwrap() = Some(q.to_string());
        let mut a = self.answer.lock().unwrap();
        loop {
            if let Some(s) = a.take() {
                drop(a);
                *self.question.lock().unwrap() = None;
                return s;
            }
            if self.cancel.load(Ordering::Relaxed) {
                drop(a);
                *self.question.lock().unwrap() = None;
                return "cancel".into();
            }
            a = self.cv.wait_timeout(a, std::time::Duration::from_millis(200)).unwrap().0;
        }
    }
}

// ------------------------------------------------------------------ kinds

/// How a file is read and written.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    SevenZ,
    Zip,
    /// tar with a compression filter ("" = none).
    Tar(&'static str),
    /// One compressed file (filter).
    Single(&'static str),
    /// Anything else libarchive reads (rar, iso, cab, deb...): read only.
    Other,
}

/// The name without a volume number: "x.7z.001" -> "x.7z", "x.part2.rar" -> "x.rar".
pub fn base_name(name: &str) -> String {
    if let Some((stem, num)) = name.rsplit_once('.') {
        if num.len() == 3 && num.bytes().all(|b| b.is_ascii_digit()) {
            return stem.to_string();
        }
    }
    let l = name.to_lowercase();
    if l.ends_with(".rar") {
        if let Some(i) = l.rfind(".part") {
            let mid = &l[i + 5..l.len() - 4];
            if !mid.is_empty() && mid.bytes().all(|b| b.is_ascii_digit()) {
                return format!("{}.rar", &name[..i]);
            }
        }
    }
    name.to_string()
}

pub fn kind_of(path: &Path) -> Kind {
    let n = base_name(&path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).to_lowercase();
    let ends = |s: &[&str]| s.iter().any(|e| n.ends_with(e));
    if ends(&[".7z", ".cb7"]) {
        return Kind::SevenZ;
    }
    if ends(&[".zip", ".zipx", ".jar", ".cbz", ".apk", ".xpi", ".whl", ".war", ".ear", ".aar", ".nupkg", ".vsix", ".crx"]) {
        return Kind::Zip;
    }
    if ends(&[".tar.gz", ".tgz", ".taz"]) {
        return Kind::Tar("gzip");
    }
    if ends(&[".tar.xz", ".txz"]) {
        return Kind::Tar("xz");
    }
    if ends(&[".tar.bz2", ".tbz", ".tbz2", ".tb2"]) {
        return Kind::Tar("bzip2");
    }
    if ends(&[".tar.zst", ".tzst", ".tar.zstd"]) {
        return Kind::Tar("zstd");
    }
    if ends(&[".tar.lz4"]) {
        return Kind::Tar("lz4");
    }
    if ends(&[".tar.lz", ".tlz"]) {
        return Kind::Tar("lzip");
    }
    if ends(&[".tar", ".cbt"]) {
        return Kind::Tar("");
    }
    if ends(&[".gz"]) {
        return Kind::Single("gzip");
    }
    if ends(&[".xz"]) {
        return Kind::Single("xz");
    }
    if ends(&[".bz2", ".bz"]) {
        return Kind::Single("bzip2");
    }
    if ends(&[".zst", ".zstd"]) {
        return Kind::Single("zstd");
    }
    if ends(&[".lz4"]) {
        return Kind::Single("lz4");
    }
    if ends(&[".lz"]) {
        return Kind::Single("lzip");
    }
    if ends(&[".lzma"]) {
        return Kind::Single("lzma");
    }
    // By content: a zip or 7z with another name.
    let mut head = [0u8; 6];
    if File::open(path).and_then(|mut f| f.read_exact(&mut head)).is_ok() {
        if head == [b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C] {
            return Kind::SevenZ;
        }
    }
    Kind::Other
}

/// A friendly name for the kind ("7z", "ZIP", "TAR.GZ"...).
pub fn kind_label(path: &Path) -> String {
    match kind_of(path) {
        Kind::SevenZ => "7z".into(),
        Kind::Zip => "ZIP".into(),
        Kind::Tar(f) => match f {
            "" => "TAR".into(),
            "gzip" => "TAR.GZ".into(),
            "bzip2" => "TAR.BZ2".into(),
            "zstd" => "TAR.ZST".into(),
            f => format!("TAR.{}", f.to_uppercase()),
        },
        Kind::Single(f) => f.to_uppercase(),
        Kind::Other => {
            let n = base_name(&path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
            n.rsplit_once('.').map(|(_, e)| e.to_uppercase()).unwrap_or_else(|| "Archive".into())
        }
    }
}

pub fn writable(path: &Path) -> bool {
    matches!(kind_of(path), Kind::SevenZ | Kind::Zip | Kind::Tar(_))
}

// ---------------------------------------------------------------- volumes

/// Every volume of a split archive, in order (just the file when it isn't
/// split): x.7z.001 + .002..., x.part1.rar + part2...
pub fn volumes(path: &Path) -> Vec<PathBuf> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let dir = path.parent().unwrap_or(Path::new("."));
    if let Some((stem, num)) = name.rsplit_once('.') {
        if num.len() == 3 && num.bytes().all(|b| b.is_ascii_digit()) {
            let mut v = Vec::new();
            for i in 1..1000 {
                let p = dir.join(format!("{stem}.{i:03}"));
                if !p.exists() {
                    break;
                }
                v.push(p);
            }
            if !v.is_empty() {
                return v;
            }
        }
    }
    let l = name.to_lowercase();
    if l.ends_with(".rar") {
        if let Some(i) = l.rfind(".part") {
            let digits = &name[i + 5..name.len() - 4];
            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                let w = digits.len();
                let mut v = Vec::new();
                for k in 1..1000 {
                    let p = dir.join(format!("{}.part{:0w$}.rar", &name[..i], k, w = w));
                    if !p.exists() {
                        break;
                    }
                    v.push(p);
                }
                if !v.is_empty() {
                    return v;
                }
            }
        }
    }
    vec![path.to_path_buf()]
}

/// Several files read as one (split archives).
pub struct MultiFile {
    parts: Vec<(File, u64)>,
    pos: u64,
    len: u64,
}

impl MultiFile {
    pub fn open(paths: &[PathBuf]) -> std::io::Result<MultiFile> {
        let mut parts = Vec::new();
        let mut len = 0;
        for p in paths {
            let f = File::open(p)?;
            let l = f.metadata()?.len();
            len += l;
            parts.push((f, l));
        }
        Ok(MultiFile { parts, pos: 0, len })
    }
}

impl Read for MultiFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut off = self.pos;
        for (f, l) in self.parts.iter_mut() {
            if off < *l {
                f.seek(SeekFrom::Start(off))?;
                let want = buf.len().min((*l - off) as usize);
                let n = f.read(&mut buf[..want])?;
                self.pos += n as u64;
                return Ok(n);
            }
            off -= *l;
        }
        Ok(0)
    }
}

impl Seek for MultiFile {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let p = match to {
            SeekFrom::Start(p) => p as i128,
            SeekFrom::End(d) => self.len as i128 + d as i128,
            SeekFrom::Current(d) => self.pos as i128 + d as i128,
        };
        if p < 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "seek before start"));
        }
        self.pos = p as u64;
        Ok(self.pos)
    }
}

/// Split a finished archive into numbered volumes of `size` bytes
/// ("x.7z" -> "x.7z.001", ".002"...). Returns the files written.
pub fn split(path: &Path, size: u64, p: &Progress) -> Res<Vec<PathBuf>> {
    let len = std::fs::metadata(path).map_err(|e| failed(e.to_string()))?.len();
    if size == 0 || len <= size {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut f = File::open(path).map_err(|e| failed(e.to_string()))?;
    let mut out = Vec::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut i = 1;
    let mut left = len;
    while left > 0 {
        p.check()?;
        let name = format!("{}.{i:03}", path.file_name().unwrap().to_string_lossy());
        let vp = path.with_file_name(name);
        let mut w = File::create(&vp).map_err(|e| failed(e.to_string()))?;
        let mut part = size.min(left);
        while part > 0 {
            let want = part.min(buf.len() as u64) as usize;
            let n = f.read(&mut buf[..want]).map_err(|e| failed(e.to_string()))?;
            if n == 0 {
                break;
            }
            w.write_all(&buf[..n]).map_err(|e| failed(e.to_string()))?;
            part -= n as u64;
            left -= n as u64;
        }
        out.push(vp);
        i += 1;
    }
    drop(f);
    let _ = std::fs::remove_file(path);
    Ok(out)
}

// ----------------------------------------------------------------- listing

#[derive(Clone, Debug, Default, Serialize)]
pub struct Item {
    pub path: String,
    pub size: u64,
    /// Packed size where the format records it (7z: on a block's first file).
    pub packed: Option<u64>,
    pub mtime: i64,
    #[serde(rename = "isDir")]
    pub is_dir: bool,
    pub encrypted: bool,
    /// CRC-32, hex.
    pub crc: String,
    pub method: String,
    /// Unix mode (0 = unknown).
    pub mode: u32,
    /// Symlink target.
    pub link: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Listing {
    pub format: String,
    pub items: Vec<Item>,
    pub solid: bool,
    /// File names are encrypted too (7z).
    #[serde(rename = "headerEncrypted")]
    pub header_encrypted: bool,
    /// Something in it is encrypted.
    pub encrypted: bool,
    pub comment: String,
    /// Size on disk (all volumes).
    pub physical: u64,
    pub volumes: Vec<String>,
    pub writable: bool,
    pub blocks: u64,
    /// The main method ("LZMA2:24 AES", "Deflate"...).
    pub method: String,
}

pub fn list(path: &Path, password: Option<&str>, p: &Progress) -> Res<Listing> {
    let vols = volumes(path);
    let physical = vols.iter().filter_map(|v| std::fs::metadata(v).ok()).map(|m| m.len()).sum();
    let mut l = match kind_of(path) {
        Kind::SevenZ => sz_list(&vols, password)?,
        Kind::Single(_) => single_list(path, &vols, p)?,
        k => {
            let mut l = la_list(&vols, password, p)?;
            if k == Kind::Zip {
                zip_enrich(&vols, &mut l);
            }
            l
        }
    };
    l.format = kind_label(path);
    l.physical = physical;
    l.volumes = vols.iter().map(|v| v.to_string_lossy().into_owned()).collect();
    l.writable = writable(path) && vols.len() == 1;
    l.encrypted = l.header_encrypted || l.items.iter().any(|i| i.encrypted);
    Ok(l)
}

fn single_list(path: &Path, vols: &[PathBuf], p: &Progress) -> Res<Listing> {
    let name = base_name(&path.file_name().unwrap().to_string_lossy());
    let inner = name.rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or(name.clone());
    let r = la_open(vols, None, true)?;
    let mut size = 0u64;
    let mut buf = vec![0u8; 1 << 16];
    let mut first = true;
    loop {
        p.check()?;
        let mut e = std::ptr::null_mut();
        let rc = unsafe { archive_read_next_header(r.0, &mut e) };
        if rc == EOF || !first {
            break;
        }
        first = false;
        if rc < WARN {
            return Err(la_err(r.0, "This file can't be read", None));
        }
        loop {
            let n = unsafe { archive_read_data(r.0, buf.as_mut_ptr() as *mut c_void, buf.len()) };
            if n <= 0 {
                break;
            }
            size += n as u64;
        }
    }
    let mtime = std::fs::metadata(path).ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
    let packed = std::fs::metadata(path).map(|m| m.len()).ok();
    Ok(Listing { items: vec![Item { path: inner, size, packed, mtime, method: kind_label(path), ..Default::default() }], ..Default::default() })
}

// -------------------------------------------------------------------- 7z

fn sz_err(e: sevenz_rust2::Error, password: Option<&str>) -> ArcError {
    use sevenz_rust2::Error as E;
    match e {
        E::PasswordRequired => ArcError::NeedPassword,
        E::MaybeBadPassword(_) => {
            if password.is_some() {
                ArcError::BadPassword
            } else {
                ArcError::NeedPassword
            }
        }
        E::ChecksumVerificationFailed if password.is_some() => ArcError::BadPassword,
        E::Io(ref io, _) if io.kind() == std::io::ErrorKind::Interrupted => ArcError::Cancelled,
        e => failed(format!("This 7z archive can't be read: {e}")),
    }
}

fn sz_password(p: Option<&str>) -> sevenz_rust2::Password {
    match p {
        Some(s) if !s.is_empty() => sevenz_rust2::Password::from(s),
        _ => sevenz_rust2::Password::empty(),
    }
}

fn sz_open(vols: &[PathBuf], password: Option<&str>) -> Res<sevenz_rust2::ArchiveReader<MultiFile>> {
    let f = MultiFile::open(vols).map_err(|e| failed(e.to_string()))?;
    sevenz_rust2::ArchiveReader::new(f, sz_password(password)).map_err(|e| sz_err(e, password))
}

fn nt_to_unix(t: sevenz_rust2::NtTime) -> i64 {
    let v: u64 = t.into();
    if v == 0 {
        return 0;
    }
    (v / 10_000_000) as i64 - 11_644_473_600
}

fn method_name(id: &[u8]) -> String {
    use sevenz_rust2::EncoderMethod as M;
    match M::by_id(id) {
        Some(m) if m.id() == M::ID_AES256_SHA256 => "AES".into(),
        Some(m) => m.name().replace("_FILTER", ""),
        None => format!("{id:02X?}"),
    }
}

fn sz_list(vols: &[PathBuf], password: Option<&str>) -> Res<Listing> {
    // Encrypted names: the header itself needs the password.
    let r = sz_open(vols, password)?;
    let a = r.archive();
    let header_encrypted = {
        // Opening without a password worked only if the header is plain.
        password.is_some() && sz_open(vols, None).is_err()
    };
    let block_methods: Vec<String> = a
        .blocks
        .iter()
        .map(|b| b.coders.iter().rev().map(|c| method_name(c.encoder_method_id())).collect::<Vec<_>>().join(" "))
        .collect();
    let mut items = Vec::new();
    for (i, f) in a.files.iter().enumerate() {
        let block = a.stream_map.file_block_index.get(i).copied().flatten();
        let first_in_block = block.map(|b| a.stream_map.block_first_file_index.get(b) == Some(&i)).unwrap_or(false);
        let attrs = if f.has_windows_attributes { f.windows_attributes } else { 0 };
        let mode = if attrs & 0x8000 != 0 { attrs >> 16 } else { 0 };
        let is_link = mode & 0o170000 == 0o120000;
        items.push(Item {
            path: clean(&f.name.replace('\\', "/")),
            size: f.size,
            packed: if f.is_directory { None } else if first_in_block { Some(f.compressed_size) } else { Some(0) },
            mtime: if f.has_last_modified_date { nt_to_unix(f.last_modified_date) } else { 0 },
            is_dir: f.is_directory,
            encrypted: a.is_file_encrypted(i),
            crc: if f.has_crc { format!("{:08X}", f.crc as u32) } else { String::new() },
            method: block.and_then(|b| block_methods.get(b).cloned()).unwrap_or_default(),
            mode,
            link: if is_link { "→".into() } else { String::new() },
        });
    }
    let mut method = block_methods.first().cloned().unwrap_or_default();
    if header_encrypted && !method.contains("AES") {
        method.push_str(" AES");
    }
    Ok(Listing { items, solid: a.is_solid, header_encrypted, blocks: a.blocks.len() as u64, method, ..Default::default() })
}

/// Read every entry in order; `f` gets the item and its data.
fn sz_each(vols: &[PathBuf], password: Option<&str>, p: &Progress, mut f: impl FnMut(&Item, &mut dyn Read) -> Res<()>) -> Res<()> {
    let mut r = sz_open(vols, password)?;
    let mut inner_err: Option<ArcError> = None;
    let res = r.for_each_entries(|e, rd| {
        if let Err(x) = p.check() {
            inner_err = Some(x);
            return Ok(false);
        }
        let attrs = if e.has_windows_attributes { e.windows_attributes } else { 0 };
        let it = Item {
            path: clean(&e.name.replace('\\', "/")),
            size: e.size,
            mtime: if e.has_last_modified_date { nt_to_unix(e.last_modified_date) } else { 0 },
            is_dir: e.is_directory,
            mode: if attrs & 0x8000 != 0 { attrs >> 16 } else { 0 },
            ..Default::default()
        };
        match f(&it, rd) {
            Ok(()) => Ok(true),
            Err(x) => {
                inner_err = Some(x);
                Ok(false)
            }
        }
    });
    if let Some(e) = inner_err {
        return Err(e);
    }
    res.map_err(|e| sz_err(e, password))
}

// ---------------------------------------------------------------- libarchive

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
const AE_IFLNK: u32 = 0o120000;
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
    fn archive_read_add_passphrase(a: *mut Archive, p: *const c_char) -> c_int;
    fn archive_read_open_filenames(a: *mut Archive, names: *const *const c_char, block: usize) -> c_int;
    fn archive_read_next_header(a: *mut Archive, e: *mut *mut Entry) -> c_int;
    fn archive_read_data(a: *mut Archive, buf: *mut c_void, len: usize) -> isize;
    fn archive_read_data_block(a: *mut Archive, buf: *mut *const c_void, size: *mut usize, offset: *mut i64) -> c_int;
    fn archive_read_data_skip(a: *mut Archive) -> c_int;
    fn archive_read_free(a: *mut Archive) -> c_int;
    fn archive_read_close(a: *mut Archive) -> c_int;
    fn archive_error_string(a: *mut Archive) -> *const c_char;
    fn archive_filter_bytes(a: *mut Archive, n: c_int) -> i64;
    fn archive_read_has_encrypted_entries(a: *mut Archive) -> c_int;

    fn archive_write_disk_new() -> *mut Archive;
    fn archive_write_disk_set_options(a: *mut Archive, flags: c_int) -> c_int;
    fn archive_write_disk_set_standard_lookup(a: *mut Archive) -> c_int;
    fn archive_write_data_block(a: *mut Archive, buf: *const c_void, size: usize, offset: i64) -> isize;
    fn archive_write_finish_entry(a: *mut Archive) -> c_int;

    fn archive_write_new() -> *mut Archive;
    fn archive_write_set_format_zip(a: *mut Archive) -> c_int;
    fn archive_write_set_format_pax_restricted(a: *mut Archive) -> c_int;
    fn archive_write_set_format_raw(a: *mut Archive) -> c_int;
    fn archive_write_add_filter_by_name(a: *mut Archive, name: *const c_char) -> c_int;
    fn archive_write_set_options(a: *mut Archive, opts: *const c_char) -> c_int;
    fn archive_write_set_passphrase(a: *mut Archive, p: *const c_char) -> c_int;
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

    fn archive_entry_new() -> *mut Entry;
    fn archive_entry_clear(e: *mut Entry) -> *mut Entry;
    fn archive_entry_free(e: *mut Entry);
    fn archive_entry_pathname(e: *mut Entry) -> *const c_char;
    fn archive_entry_pathname_utf8(e: *mut Entry) -> *const c_char;
    fn archive_entry_set_pathname(e: *mut Entry, p: *const c_char);
    fn archive_entry_hardlink(e: *mut Entry) -> *const c_char;
    fn archive_entry_set_hardlink(e: *mut Entry, p: *const c_char);
    fn archive_entry_symlink(e: *mut Entry) -> *const c_char;
    fn archive_entry_size(e: *mut Entry) -> i64;
    fn archive_entry_filetype(e: *mut Entry) -> u32;
    fn archive_entry_mode(e: *mut Entry) -> u32;
    fn archive_entry_mtime(e: *mut Entry) -> i64;
    fn archive_entry_is_encrypted(e: *mut Entry) -> c_int;
}

struct LaRead(*mut Archive);
impl Drop for LaRead {
    fn drop(&mut self) {
        unsafe { archive_read_free(self.0) };
    }
}
struct LaWrite(*mut Archive);
impl Drop for LaWrite {
    fn drop(&mut self) {
        unsafe { archive_write_free(self.0) };
    }
}
struct LaEntry(*mut Entry);
impl Drop for LaEntry {
    fn drop(&mut self) {
        unsafe { archive_entry_free(self.0) };
    }
}

fn cstr(p: &Path) -> Res<CString> {
    CString::new(p.as_os_str().as_bytes()).map_err(|_| failed("A path with a NUL byte"))
}

fn la_msg(a: *mut Archive) -> String {
    let s = unsafe { archive_error_string(a) };
    if s.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned()
    }
}

/// libarchive's error, as a password problem when it is one.
fn la_err(a: *mut Archive, what: &str, password: Option<&str>) -> ArcError {
    let m = la_msg(a);
    let l = m.to_lowercase();
    if l.contains("passphrase") || l.contains("password") || l.contains("decrypt") || l.contains("encrypt") {
        return if password.map(|p| !p.is_empty()).unwrap_or(false) && !l.contains("required") && !l.contains("not supported") && !l.contains("unsupported") {
            ArcError::BadPassword
        } else if l.contains("not supported") || l.contains("unsupported") {
            failed(format!("{what}: this kind of encryption isn't supported ({m})"))
        } else {
            ArcError::NeedPassword
        };
    }
    if m.is_empty() {
        failed(what)
    } else {
        failed(format!("{what}: {m}"))
    }
}

fn la_open(vols: &[PathBuf], password: Option<&str>, raw: bool) -> Res<LaRead> {
    let a = unsafe { archive_read_new() };
    if a.is_null() {
        return Err(failed("Out of memory"));
    }
    let r = LaRead(a);
    let names: Vec<CString> = vols.iter().map(|v| cstr(v)).collect::<Res<_>>()?;
    let mut ptrs: Vec<*const c_char> = names.iter().map(|c| c.as_ptr()).collect();
    ptrs.push(std::ptr::null());
    unsafe {
        archive_read_support_filter_all(a);
        if raw {
            archive_read_support_format_raw(a);
        } else {
            archive_read_support_format_all(a);
        }
        if let Some(p) = password.filter(|p| !p.is_empty()) {
            let c = CString::new(p).map_err(|_| failed("A password with a NUL byte"))?;
            archive_read_add_passphrase(a, c.as_ptr());
        }
        if archive_read_open_filenames(a, ptrs.as_ptr(), 1 << 16) != OK {
            return Err(la_err(a, "This archive can't be read", password));
        }
    }
    Ok(r)
}

fn clean(s: &str) -> String {
    let mut s = s;
    while let Some(r) = s.strip_prefix("./") {
        s = r;
    }
    s.trim_start_matches('/').trim_end_matches('/').to_string()
}

fn la_item(e: *mut Entry) -> Item {
    let p = unsafe { archive_entry_pathname_utf8(e) };
    let p = if p.is_null() { unsafe { archive_entry_pathname(e) } } else { p };
    let path = if p.is_null() { String::new() } else { clean(&unsafe { CStr::from_ptr(p) }.to_string_lossy()) };
    let t = unsafe { archive_entry_filetype(e) } & AE_IFMT;
    let link = unsafe { archive_entry_symlink(e) };
    Item {
        path,
        size: unsafe { archive_entry_size(e) }.max(0) as u64,
        mtime: unsafe { archive_entry_mtime(e) },
        is_dir: t == AE_IFDIR,
        encrypted: unsafe { archive_entry_is_encrypted(e) } != 0,
        mode: unsafe { archive_entry_mode(e) },
        link: if link.is_null() { String::new() } else { unsafe { CStr::from_ptr(link) }.to_string_lossy().into_owned() },
        ..Default::default()
    }
}

fn la_list(vols: &[PathBuf], password: Option<&str>, p: &Progress) -> Res<Listing> {
    let r = la_open(vols, password, false)?;
    let mut items = Vec::new();
    loop {
        p.check()?;
        let mut e = std::ptr::null_mut();
        let rc = unsafe { archive_read_next_header(r.0, &mut e) };
        if rc == EOF {
            break;
        }
        if rc < WARN {
            return Err(la_err(r.0, "This archive can't be read", password));
        }
        let it = la_item(e);
        if !it.path.is_empty() && it.path != "." {
            items.push(it);
        }
        unsafe { archive_read_data_skip(r.0) };
    }
    let enc = unsafe { archive_read_has_encrypted_entries(r.0) } > 0;
    Ok(Listing { items, encrypted: enc, ..Default::default() })
}

/// Zip details libarchive doesn't give: packed size, CRC, method, comment.
fn zip_enrich(vols: &[PathBuf], l: &mut Listing) {
    let Ok(f) = MultiFile::open(vols) else { return };
    let Ok(mut z) = zip::ZipArchive::new(f) else { return };
    l.comment = String::from_utf8_lossy(z.comment()).into_owned();
    let mut by: std::collections::HashMap<String, (u64, u32, String, bool)> = Default::default();
    let mut methods = std::collections::BTreeSet::new();
    for i in 0..z.len() {
        if let Ok(f) = z.by_index_raw(i) {
            let m = format!("{:?}", f.compression());
            let m = match m.as_str() {
                "Stored" => "Store".to_string(),
                "Deflated" => "Deflate".to_string(),
                "Deflate64" => "Deflate64".to_string(),
                "Bzip2" => "BZip2".to_string(),
                "Lzma" => "LZMA".to_string(),
                "Zstd" => "Zstandard".to_string(),
                "Xz" => "XZ".to_string(),
                "Aes" => "AES".to_string(),
                o => o.to_string(),
            };
            if !f.is_dir() {
                methods.insert(m.clone());
            }
            by.insert(clean(&f.name().map(|c| c.into_owned()).unwrap_or_default()), (f.compressed_size(), f.crc32(), m, f.encrypted()));
        }
    }
    for it in l.items.iter_mut() {
        if let Some((packed, crc, m, enc)) = by.get(&it.path) {
            if !it.is_dir {
                it.packed = Some(*packed);
                it.crc = format!("{crc:08X}");
                it.method = m.clone();
            }
            it.encrypted |= *enc;
        }
    }
    l.method = methods.into_iter().collect::<Vec<_>>().join(" ");
}

// ---------------------------------------------------------------- extract

#[derive(Clone, Debug)]
pub struct ExtractOpts {
    pub password: Option<String>,
    /// Members to take out (and everything under folders named); empty = all.
    pub only: Vec<String>,
    /// Removed from the front of every path ("docs/" when extracting what's
    /// selected inside the docs folder).
    pub strip: String,
    /// false: every file straight into the destination (7-Zip's "No pathnames").
    pub keep_paths: bool,
    pub overwrite: Overwrite,
}

impl Default for ExtractOpts {
    fn default() -> Self {
        ExtractOpts { password: None, only: vec![], strip: String::new(), keep_paths: true, overwrite: Overwrite::Ask }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Report {
    pub files: u64,
    pub folders: u64,
    pub bytes: u64,
    pub skipped: u64,
    pub errors: Vec<String>,
    /// Top-level items written (to select them afterwards).
    pub created: Vec<String>,
}

fn wanted(path: &str, only: &[String]) -> bool {
    only.is_empty() || only.iter().any(|o| path == o || path.starts_with(&format!("{o}/")))
}

/// A path inside the archive made safe: no "..", no root, no empty parts.
fn safe_rel(p: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for c in Path::new(p).components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// Where an item goes (None: skip it), resolving a clash with what's there.
fn target_for(dest: &Path, it: &Item, o: &ExtractOpts, policy: &mut Overwrite, p: &Progress, rep: &mut Report) -> Res<Option<PathBuf>> {
    let rel = it.path.strip_prefix(&o.strip).unwrap_or(&it.path).trim_start_matches('/');
    let rel = if o.keep_paths { rel.to_string() } else { rel.rsplit('/').next().unwrap_or(rel).to_string() };
    if rel.is_empty() || (!o.keep_paths && it.is_dir) {
        return Ok(None);
    }
    let Some(rel) = safe_rel(&rel) else {
        rep.errors.push(format!("Skipped “{}”: it points outside the folder", it.path));
        return Ok(None);
    };
    let mut t = dest.join(&rel);
    if let Some(top) = rel.components().next() {
        let top = dest.join(top.as_os_str()).to_string_lossy().into_owned();
        if !rep.created.contains(&top) {
            rep.created.push(top);
        }
    }
    if it.is_dir {
        return Ok(Some(t));
    }
    if std::fs::symlink_metadata(&t).is_ok() {
        let mut choice = *policy;
        if choice == Overwrite::Ask {
            let m = std::fs::metadata(&t).ok();
            let q = serde_json::json!({
                "kind": "overwrite", "path": t, "name": t.file_name().map(|n| n.to_string_lossy().into_owned()),
                "existingSize": m.as_ref().map(|m| m.len()),
                "existingTime": m.as_ref().and_then(|m| m.modified().ok()).and_then(|x| x.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()),
                "newSize": it.size, "newTime": it.mtime,
            });
            let a = p.ask(q);
            let (c, all) = match a.as_str() {
                "replace" => (Overwrite::Replace, false),
                "replaceAll" => (Overwrite::Replace, true),
                "skip" => (Overwrite::Skip, false),
                "skipAll" => (Overwrite::Skip, true),
                "rename" => (Overwrite::Rename, false),
                "renameAll" => (Overwrite::Rename, true),
                _ => return Err(ArcError::Cancelled),
            };
            if all {
                *policy = c;
            }
            choice = c;
        }
        match choice {
            Overwrite::Skip => {
                rep.skipped += 1;
                return Ok(None);
            }
            Overwrite::Replace => {
                let _ = std::fs::remove_file(&t);
            }
            Overwrite::Rename => t = crate::ops::unique_name(&t),
            Overwrite::RenameExisting => {
                let to = crate::ops::unique_name(&t);
                std::fs::rename(&t, &to).map_err(|e| failed(format!("Couldn't rename “{}”: {e}", t.display())))?;
            }
            Overwrite::Ask => {}
        }
    }
    Ok(Some(t))
}

pub fn extract(path: &Path, dest: &Path, o: &ExtractOpts, p: &Progress) -> Res<Report> {
    std::fs::create_dir_all(dest).map_err(|e| failed(format!("Couldn't make “{}”: {e}", dest.display())))?;
    let vols = volumes(path);
    match kind_of(path) {
        Kind::SevenZ => sz_extract(&vols, dest, o, p),
        Kind::Single(_) => single_extract(path, &vols, dest, o, p),
        _ => la_extract(&vols, dest, o, p),
    }
}

fn set_mtime(path: &Path, t: i64) {
    if t <= 0 {
        return;
    }
    if let Ok(f) = File::options().write(true).open(path) {
        let _ = f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(t as u64));
    }
}

fn sz_extract(vols: &[PathBuf], dest: &Path, o: &ExtractOpts, p: &Progress) -> Res<Report> {
    let total: u64 = sz_list(vols, o.password.as_deref())?.items.iter().filter(|i| wanted(&i.path, &o.only)).map(|i| i.size).sum();
    p.total.store(total.max(1), Ordering::Relaxed);
    let mut rep = Report::default();
    let mut policy = o.overwrite;
    let mut dirs: Vec<(PathBuf, i64)> = Vec::new();
    let mut buf = vec![0u8; 1 << 16];
    sz_each(vols, o.password.as_deref(), p, |it, rd| {
        if !wanted(&it.path, &o.only) {
            return Ok(());
        }
        p.set_current(&it.path);
        let Some(t) = target_for(dest, it, o, &mut policy, p, &mut rep)? else { return Ok(()) };
        if it.is_dir {
            std::fs::create_dir_all(&t).map_err(|e| failed(format!("Couldn't make “{}”: {e}", t.display())))?;
            dirs.push((t, it.mtime));
            rep.folders += 1;
            return Ok(());
        }
        if let Some(d) = t.parent() {
            std::fs::create_dir_all(d).map_err(|e| failed(format!("Couldn't make “{}”: {e}", d.display())))?;
        }
        if it.mode & 0o170000 == 0o120000 {
            let mut s = String::new();
            rd.read_to_string(&mut s).map_err(|e| failed(e.to_string()))?;
            // Only links that stay inside.
            if !s.starts_with('/') && !s.split('/').any(|c| c == "..") {
                let _ = std::os::unix::fs::symlink(&s, &t);
            }
            rep.files += 1;
            return Ok(());
        }
        let mut f = File::create(&t).map_err(|e| failed(format!("Couldn't write “{}”: {e}", t.display())))?;
        loop {
            p.check()?;
            let n = match rd.read(&mut buf) {
                Ok(n) => n,
                Err(e) => {
                    drop(f);
                    let _ = std::fs::remove_file(&t);
                    return Err(if o.password.is_some() && e.to_string().to_lowercase().contains("password") { ArcError::BadPassword } else { failed(format!("Couldn't extract “{}”: {e}", it.path)) });
                }
            };
            if n == 0 {
                break;
            }
            f.write_all(&buf[..n]).map_err(|e| failed(format!("Couldn't write “{}”: {e}", t.display())))?;
            p.add(n as u64);
            rep.bytes += n as u64;
        }
        drop(f);
        if it.mode & 0o777 != 0 {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&t, std::fs::Permissions::from_mode(it.mode & 0o777));
        }
        set_mtime(&t, it.mtime);
        rep.files += 1;
        p.files.fetch_add(1, Ordering::Relaxed);
        Ok(())
    })?;
    for (d, t) in dirs.into_iter().rev() {
        set_mtime(&d, t);
    }
    Ok(rep)
}

fn single_extract(path: &Path, vols: &[PathBuf], dest: &Path, o: &ExtractOpts, p: &Progress) -> Res<Report> {
    let l = single_list(path, vols, p)?;
    let it = l.items[0].clone();
    p.total.store(it.size.max(1), Ordering::Relaxed);
    let mut rep = Report::default();
    let mut policy = o.overwrite;
    let o2 = ExtractOpts { only: vec![], strip: String::new(), ..o.clone() };
    let Some(t) = target_for(dest, &it, &o2, &mut policy, p, &mut rep)? else { return Ok(rep) };
    p.set_current(&it.path);
    let r = la_open(vols, None, true)?;
    let mut e = std::ptr::null_mut();
    if unsafe { archive_read_next_header(r.0, &mut e) } < WARN {
        return Err(la_err(r.0, "This file can't be read", None));
    }
    let mut f = File::create(&t).map_err(|e| failed(format!("Couldn't write “{}”: {e}", t.display())))?;
    let mut buf = vec![0u8; 1 << 16];
    loop {
        if let Err(x) = p.check() {
            drop(f);
            let _ = std::fs::remove_file(&t);
            return Err(x);
        }
        let n = unsafe { archive_read_data(r.0, buf.as_mut_ptr() as *mut c_void, buf.len()) };
        if n < 0 {
            drop(f);
            let _ = std::fs::remove_file(&t);
            return Err(la_err(r.0, "Couldn't decompress", None));
        }
        if n == 0 {
            break;
        }
        f.write_all(&buf[..n as usize]).map_err(|e| failed(e.to_string()))?;
        p.add(n as u64);
        rep.bytes += n as u64;
    }
    drop(f);
    set_mtime(&t, it.mtime);
    rep.files = 1;
    Ok(rep)
}

fn la_extract(vols: &[PathBuf], dest: &Path, o: &ExtractOpts, p: &Progress) -> Res<Report> {
    let physical: u64 = vols.iter().filter_map(|v| std::fs::metadata(v).ok()).map(|m| m.len()).sum();
    p.total.store(physical.max(1), Ordering::Relaxed);
    let pw = o.password.as_deref();
    let r = la_open(vols, pw, false)?;
    let w = LaWrite(unsafe { archive_write_disk_new() });
    unsafe {
        archive_write_disk_set_options(w.0, EXTRACT_TIME | EXTRACT_PERM | EXTRACT_SECURE_SYMLINKS | EXTRACT_SECURE_NODOTDOT);
        archive_write_disk_set_standard_lookup(w.0);
    }
    let mut rep = Report::default();
    let mut policy = o.overwrite;
    loop {
        p.check()?;
        let mut e = std::ptr::null_mut();
        let rc = unsafe { archive_read_next_header(r.0, &mut e) };
        if rc == EOF {
            break;
        }
        if rc < WARN {
            return Err(la_err(r.0, "This archive can't be read", pw));
        }
        p.done.store(unsafe { archive_filter_bytes(r.0, -1) }.max(0) as u64, Ordering::Relaxed);
        let it = la_item(e);
        if it.path.is_empty() || it.path == "." || !wanted(&it.path, &o.only) {
            unsafe { archive_read_data_skip(r.0) };
            continue;
        }
        p.set_current(&it.path);
        let Some(t) = target_for(dest, &it, o, &mut policy, p, &mut rep)? else {
            unsafe { archive_read_data_skip(r.0) };
            continue;
        };
        let tc = cstr(&t)?;
        unsafe { archive_entry_set_pathname(e, tc.as_ptr()) };
        let h = unsafe { archive_entry_hardlink(e) };
        if !h.is_null() {
            let target = clean(&unsafe { CStr::from_ptr(h) }.to_string_lossy());
            let target = target.strip_prefix(&o.strip).unwrap_or(&target).to_string();
            match safe_rel(&target) {
                Some(rel) => {
                    let hc = cstr(&dest.join(rel))?;
                    unsafe { archive_entry_set_hardlink(e, hc.as_ptr()) };
                }
                None => {
                    unsafe { archive_read_data_skip(r.0) };
                    continue;
                }
            }
        }
        if unsafe { archive_write_header(w.0, e) } < WARN {
            rep.errors.push(la_err(w.0, &format!("Couldn't extract “{}”", it.path), None).to_string());
            unsafe { archive_read_data_skip(r.0) };
            continue;
        }
        let ft = unsafe { archive_entry_filetype(e) } & AE_IFMT;
        if ft == AE_IFREG {
            loop {
                p.check()?;
                let mut buf: *const c_void = std::ptr::null();
                let mut size = 0usize;
                let mut off = 0i64;
                let rc = unsafe { archive_read_data_block(r.0, &mut buf, &mut size, &mut off) };
                if rc == EOF {
                    break;
                }
                if rc < WARN {
                    let err = la_err(r.0, &format!("Couldn't extract “{}”", it.path), pw);
                    if matches!(err, ArcError::NeedPassword | ArcError::BadPassword) {
                        unsafe { archive_write_finish_entry(w.0) };
                        let _ = std::fs::remove_file(&t);
                        return Err(err);
                    }
                    rep.errors.push(err.to_string());
                    p.errors.fetch_add(1, Ordering::Relaxed);
                    break;
                }
                if unsafe { archive_write_data_block(w.0, buf, size, off) } < 0 {
                    rep.errors.push(la_err(w.0, &format!("Couldn't write “{}”", t.display()), None).to_string());
                    break;
                }
                rep.bytes += size as u64;
                p.bytes.fetch_add(size as u64, Ordering::Relaxed);
                p.done.store(unsafe { archive_filter_bytes(r.0, -1) }.max(0) as u64, Ordering::Relaxed);
            }
            rep.files += 1;
            p.files.fetch_add(1, Ordering::Relaxed);
        } else if ft == AE_IFDIR {
            rep.folders += 1;
        } else {
            rep.files += 1;
        }
        unsafe { archive_write_finish_entry(w.0) };
    }
    p.done.store(physical, Ordering::Relaxed);
    Ok(rep)
}

// ------------------------------------------------------------------- test

/// Read everything and check it (CRCs): Ok(report) lists any damaged files.
pub fn test(path: &Path, password: Option<&str>, p: &Progress) -> Res<Report> {
    let vols = volumes(path);
    let mut rep = Report::default();
    match kind_of(path) {
        Kind::SevenZ => {
            let total: u64 = sz_list(&vols, password)?.items.iter().map(|i| i.size).sum();
            p.total.store(total.max(1), Ordering::Relaxed);
            let mut buf = vec![0u8; 1 << 16];
            sz_each(&vols, password, p, |it, rd| {
                p.set_current(&it.path);
                if it.is_dir {
                    rep.folders += 1;
                    return Ok(());
                }
                loop {
                    p.check()?;
                    match rd.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            p.add(n as u64);
                            rep.bytes += n as u64;
                        }
                        Err(e) => {
                            if password.is_some() && e.to_string().to_lowercase().contains("password") {
                                return Err(ArcError::BadPassword);
                            }
                            return Err(failed(format!("“{}” is damaged: {e}", it.path)));
                        }
                    }
                }
                rep.files += 1;
                p.files.fetch_add(1, Ordering::Relaxed);
                Ok(())
            })
            .map_err(|e| match e {
                ArcError::Failed(s) if s.contains("ChecksumVerificationFailed") || s.to_lowercase().contains("checksum") => failed("A file in it is damaged (its checksum doesn't match)."),
                e => e,
            })?;
        }
        k => {
            let raw = matches!(k, Kind::Single(_));
            let physical: u64 = vols.iter().filter_map(|v| std::fs::metadata(v).ok()).map(|m| m.len()).sum();
            p.total.store(physical.max(1), Ordering::Relaxed);
            let r = la_open(&vols, password, raw)?;
            let mut buf = vec![0u8; 1 << 16];
            loop {
                p.check()?;
                let mut e = std::ptr::null_mut();
                let rc = unsafe { archive_read_next_header(r.0, &mut e) };
                if rc == EOF {
                    break;
                }
                if rc < WARN {
                    rep.errors.push(la_err(r.0, "The archive is damaged", password).to_string());
                    break;
                }
                let it = la_item(e);
                p.set_current(&it.path);
                if it.is_dir {
                    rep.folders += 1;
                    continue;
                }
                loop {
                    p.check()?;
                    let n = unsafe { archive_read_data(r.0, buf.as_mut_ptr() as *mut c_void, buf.len()) };
                    if n < 0 {
                        let err = la_err(r.0, &format!("“{}” is damaged", it.path), password);
                        if matches!(err, ArcError::NeedPassword | ArcError::BadPassword) {
                            return Err(err);
                        }
                        rep.errors.push(err.to_string());
                        break;
                    }
                    if n == 0 {
                        break;
                    }
                    rep.bytes += n as u64;
                    p.bytes.fetch_add(n as u64, Ordering::Relaxed);
                    p.done.store(unsafe { archive_filter_bytes(r.0, -1) }.max(0) as u64, Ordering::Relaxed);
                }
                rep.files += 1;
                p.files.fetch_add(1, Ordering::Relaxed);
            }
            unsafe { archive_read_close(r.0) };
        }
    }
    Ok(rep)
}

// ----------------------------------------------------------------- create

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub enum Format {
    SevenZ,
    Zip,
    Tar,
    TarGz,
    TarXz,
    TarBz2,
    TarZst,
    Gz,
    Xz,
    Bz2,
    Zst,
}

impl Format {
    pub fn parse(s: &str) -> Option<Format> {
        Some(match s {
            "7z" => Format::SevenZ,
            "zip" => Format::Zip,
            "tar" => Format::Tar,
            "tar.gz" => Format::TarGz,
            "tar.xz" => Format::TarXz,
            "tar.bz2" => Format::TarBz2,
            "tar.zst" => Format::TarZst,
            "gz" => Format::Gz,
            "xz" => Format::Xz,
            "bz2" => Format::Bz2,
            "zst" => Format::Zst,
            _ => return None,
        })
    }
    pub fn ext(self) -> &'static str {
        match self {
            Format::SevenZ => "7z",
            Format::Zip => "zip",
            Format::Tar => "tar",
            Format::TarGz => "tar.gz",
            Format::TarXz => "tar.xz",
            Format::TarBz2 => "tar.bz2",
            Format::TarZst => "tar.zst",
            Format::Gz => "gz",
            Format::Xz => "xz",
            Format::Bz2 => "bz2",
            Format::Zst => "zst",
        }
    }
    fn filter(self) -> &'static str {
        match self {
            Format::TarGz | Format::Gz => "gzip",
            Format::TarXz | Format::Xz => "xz",
            Format::TarBz2 | Format::Bz2 => "bzip2",
            Format::TarZst | Format::Zst => "zstd",
            _ => "",
        }
    }
    /// One file only (no folder structure).
    pub fn single(self) -> bool {
        matches!(self, Format::Gz | Format::Xz | Format::Bz2 | Format::Zst)
    }
    /// The format an existing archive is written back in.
    pub fn of(path: &Path) -> Option<Format> {
        Some(match kind_of(path) {
            Kind::SevenZ => Format::SevenZ,
            Kind::Zip => Format::Zip,
            Kind::Tar("") => Format::Tar,
            Kind::Tar("gzip") => Format::TarGz,
            Kind::Tar("xz") => Format::TarXz,
            Kind::Tar("bzip2") => Format::TarBz2,
            Kind::Tar("zstd") => Format::TarZst,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct CreateOpts {
    pub format: Format,
    /// 0 (store) to 9 (ultra), 7-Zip's scale.
    pub level: u32,
    /// "" = the format's default. 7z: lzma2, lzma, ppmd, bzip2, copy;
    /// zip: deflate, store, bzip2, lzma, xz, zstd.
    pub method: String,
    pub password: Option<String>,
    /// 7z: encrypt the file names too.
    pub encrypt_names: bool,
    /// zip: "aes256" (default) or "zipcrypto" (opens everywhere, weak).
    pub zip_encryption: String,
    /// 7z: solid blocks.
    pub solid: bool,
    pub threads: u32,
    /// Split into volumes of this many bytes (0 = one file).
    pub volume_size: u64,
}

impl Default for CreateOpts {
    fn default() -> Self {
        CreateOpts { format: Format::SevenZ, level: 5, method: String::new(), password: None, encrypt_names: false, zip_encryption: "aes256".into(), solid: true, threads: 0, volume_size: 0 }
    }
}

/// Every file and folder under the items, with its name in the archive
/// (relative to the item's own folder).
fn walk(items: &[PathBuf]) -> Res<Vec<(PathBuf, String)>> {
    let mut out = Vec::new();
    fn rec(p: &Path, name: String, out: &mut Vec<(PathBuf, String)>) -> Res<()> {
        let m = std::fs::symlink_metadata(p).map_err(|e| failed(format!("Couldn't read “{}”: {e}", p.display())))?;
        out.push((p.to_path_buf(), name.clone()));
        if m.is_dir() {
            let mut kids: Vec<_> = std::fs::read_dir(p).map_err(|e| failed(format!("Couldn't read “{}”: {e}", p.display())))?.filter_map(|e| e.ok()).collect();
            kids.sort_by_key(|e| e.file_name());
            for k in kids {
                rec(&k.path(), format!("{name}/{}", k.file_name().to_string_lossy()), out)?;
            }
        }
        Ok(())
    }
    for it in items {
        let name = it.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        rec(it, name, &mut out)?;
    }
    Ok(out)
}

/// Counts what goes through, checks cancel/pause.
struct Counting<'a, R> {
    inner: R,
    p: &'a Progress,
}
impl<R: Read> Read for Counting<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.p.check().is_err() {
            return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "Cancelled"));
        }
        let n = self.inner.read(buf)?;
        self.p.add(n as u64);
        Ok(n)
    }
}

/// Make a new archive at `out` from `items`; returns the files written
/// (several when split into volumes). Nothing is left behind on failure.
pub fn create(out: &Path, items: &[PathBuf], o: &CreateOpts, p: &Progress) -> Res<Vec<PathBuf>> {
    let all = walk(items)?;
    let total: u64 = all.iter().filter_map(|(f, _)| std::fs::symlink_metadata(f).ok()).filter(|m| m.is_file()).map(|m| m.len()).sum();
    p.total.store(total.max(1), Ordering::Relaxed);
    let res = match o.format {
        Format::SevenZ => sz_create(out, &all, o, p),
        f if f.single() => {
            if items.len() != 1 || !items[0].is_file() {
                return Err(failed(format!(".{} holds a single file: pick one file, or use 7z, zip or tar.{} for folders and several files", f.ext(), f.ext())));
            }
            la_create(out, &all, o, p)
        }
        _ => la_create(out, &all, o, p),
    };
    if let Err(e) = res {
        let _ = std::fs::remove_file(out);
        return Err(e);
    }
    p.packed.store(std::fs::metadata(out).map(|m| m.len()).unwrap_or(0), Ordering::Relaxed);
    if o.volume_size > 0 {
        return split(out, o.volume_size, p);
    }
    Ok(vec![out.to_path_buf()])
}

fn sz_create(out: &Path, all: &[(PathBuf, String)], o: &CreateOpts, p: &Progress) -> Res<()> {
    use sevenz_rust2::encoder_options::{AesEncoderOptions, Bzip2Options, EncoderOptions, LzmaOptions, Lzma2Options, PpmdOptions};
    use sevenz_rust2::{ArchiveEntry, ArchiveWriter, EncoderConfiguration, EncoderMethod};
    let wrap = |e: sevenz_rust2::Error| match e {
        sevenz_rust2::Error::Io(ref io, _) if io.kind() == std::io::ErrorKind::Interrupted => ArcError::Cancelled,
        e => failed(format!("Couldn't write the 7z archive: {e}")),
    };
    let mut w = ArchiveWriter::create(out).map_err(wrap)?;
    let level = o.level.min(9);
    let threads = if o.threads == 0 { std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1).min(16) } else { o.threads };
    let method: EncoderConfiguration = match (level, o.method.as_str()) {
        (0, _) | (_, "copy") => EncoderMethod::COPY.into(),
        (_, "lzma") => EncoderConfiguration::new(EncoderMethod::LZMA).with_options(EncoderOptions::Lzma(LzmaOptions::from_level(level))),
        (_, "ppmd") => EncoderConfiguration::new(EncoderMethod::PPMD).with_options(PpmdOptions::from_level(level).into()),
        (_, "bzip2") => EncoderConfiguration::new(EncoderMethod::BZIP2).with_options(Bzip2Options::from_level(level).into()),
        _ => {
            let opts = if threads > 1 { Lzma2Options::from_level_mt(level, threads, 4 << 20) } else { Lzma2Options::from_level(level) };
            EncoderConfiguration::new(EncoderMethod::LZMA2).with_options(opts.into())
        }
    };
    let pw = o.password.clone().filter(|s| !s.is_empty());
    let mut methods = Vec::new();
    if let Some(pw) = &pw {
        methods.push(AesEncoderOptions::new(sevenz_rust2::Password::from(pw.as_str())).into());
    }
    methods.push(method);
    w.set_content_methods(methods);
    w.set_encrypt_header(pw.is_some() && o.encrypt_names);
    let entry = |f: &Path, name: &str| {
        let mut e = ArchiveEntry::from_path(f, name.to_string());
        if let Ok(m) = std::fs::symlink_metadata(f) {
            use std::os::unix::fs::MetadataExt;
            e.has_windows_attributes = true;
            e.windows_attributes = 0x8000 | ((m.mode() & 0xffff) << 16) | if m.is_dir() { 0x10 } else { 0x20 };
            if m.file_type().is_symlink() {
                e.is_directory = false;
                e.has_stream = true;
            }
        }
        e
    };
    // Solid: files in blocks of up to 64 MB per thread-chunk group (as 7-Zip's
    // default solid block size grows with the level); else one file each.
    const BLOCK: u64 = 256 << 20;
    let mut batch: Vec<ArchiveEntry> = Vec::new();
    let mut readers: Vec<sevenz_rust2::SourceReader<Counting<Box<dyn Read>>>> = Vec::new();
    let mut batch_size = 0u64;
    let open = |f: &Path| -> Res<Box<dyn Read>> {
        let m = std::fs::symlink_metadata(f).map_err(|e| failed(e.to_string()))?;
        if m.file_type().is_symlink() {
            let t = std::fs::read_link(f).map_err(|e| failed(e.to_string()))?;
            return Ok(Box::new(std::io::Cursor::new(t.as_os_str().as_bytes().to_vec())));
        }
        Ok(Box::new(File::open(f).map_err(|e| failed(format!("Couldn't read “{}”: {e}", f.display())))?))
    };
    for (f, name) in all {
        p.check()?;
        let m = std::fs::symlink_metadata(f).map_err(|e| failed(e.to_string()))?;
        if m.is_dir() {
            w.push_archive_entry::<File>(entry(f, name), None).map_err(wrap)?;
            continue;
        }
        p.set_current(name);
        let size = m.len();
        if !o.solid || size == 0 {
            w.push_archive_entry(entry(f, name), Some(Counting { inner: open(f)?, p })).map_err(wrap)?;
            p.files.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        if batch_size + size > BLOCK && !batch.is_empty() {
            w.push_archive_entries(std::mem::take(&mut batch), std::mem::take(&mut readers)).map_err(wrap)?;
            batch_size = 0;
        }
        batch.push(entry(f, name));
        readers.push(Counting { inner: open(f)?, p }.into());
        batch_size += size;
        p.files.fetch_add(1, Ordering::Relaxed);
    }
    if !batch.is_empty() {
        w.push_archive_entries(batch, readers).map_err(wrap)?;
    }
    p.check()?;
    w.finish().map_err(|e| failed(format!("Couldn't write the 7z archive: {e}")))?;
    Ok(())
}

fn la_create(out: &Path, all: &[(PathBuf, String)], o: &CreateOpts, p: &Progress) -> Res<()> {
    let w = LaWrite(unsafe { archive_write_new() });
    let level = o.level.min(9);
    let set = |opt: &str| -> Res<()> {
        let c = CString::new(opt).unwrap();
        if unsafe { archive_write_set_options(w.0, c.as_ptr()) } < WARN {
            return Err(failed(format!("This option isn't available here ({opt}): {}", la_msg(w.0))));
        }
        Ok(())
    };
    unsafe {
        match o.format {
            Format::Zip => {
                archive_write_set_format_zip(w.0);
            }
            f if f.single() => {
                archive_write_set_format_raw(w.0);
            }
            _ => {
                archive_write_set_format_pax_restricted(w.0);
            }
        }
        let filter = o.format.filter();
        if !filter.is_empty() {
            let c = CString::new(filter).unwrap();
            if archive_write_add_filter_by_name(w.0, c.as_ptr()) < WARN {
                return Err(failed(format!("{filter} isn't available: {}", la_msg(w.0))));
            }
        }
    }
    match o.format {
        Format::Zip => {
            let m = match (level, o.method.as_str()) {
                (0, _) | (_, "store") => "store",
                (_, "bzip2") => "bzip2",
                (_, "lzma") => "lzma",
                (_, "xz") => "xz",
                (_, "zstd") => "zstd",
                _ => "deflate",
            };
            set(&format!("zip:compression={m}"))?;
            if m != "store" {
                set(&format!("zip:compression-level={level}"))?;
            }
            if let Some(pw) = o.password.as_deref().filter(|s| !s.is_empty()) {
                let enc = if o.zip_encryption == "zipcrypto" { "zipcrypt" } else { "aes256" };
                set(&format!("zip:encryption={enc}"))?;
                let c = CString::new(pw).map_err(|_| failed("A password with a NUL byte"))?;
                if unsafe { archive_write_set_passphrase(w.0, c.as_ptr()) } < WARN {
                    return Err(failed(format!("Couldn't set the password: {}", la_msg(w.0))));
                }
            }
        }
        f if !f.filter().is_empty() => {
            // 7-Zip's 0-9 onto each compressor's own scale.
            let l = match f.filter() {
                "zstd" => [1, 1, 2, 3, 5, 7, 9, 12, 15, 19][level as usize],
                _ => level.max(1),
            };
            set(&format!("{}:compression-level={l}", f.filter()))?;
            if f.filter() == "xz" || f.filter() == "zstd" {
                let t = if o.threads == 0 { std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(16) as u32 } else { o.threads };
                let _ = set(&format!("{}:threads={t}", f.filter()));
            }
        }
        _ => {}
    }
    let oc = cstr(out)?;
    if unsafe { archive_write_open_filename(w.0, oc.as_ptr()) } != OK {
        return Err(failed(format!("Couldn't create “{}”: {}", out.display(), la_msg(w.0))));
    }
    let res = la_add_all(&w, all, o.format.single(), p);
    unsafe { archive_write_close(w.0) };
    res
}

fn la_add_all(w: &LaWrite, all: &[(PathBuf, String)], raw: bool, p: &Progress) -> Res<()> {
    let d = LaRead(unsafe { archive_read_disk_new() });
    unsafe { archive_read_disk_set_standard_lookup(d.0) };
    let e = LaEntry(unsafe { archive_entry_new() });
    let mut buf = vec![0u8; 1 << 16];
    for (f, name) in all {
        p.check()?;
        // One entry at a time (walk() already went through the folders).
        let fc = cstr(f)?;
        unsafe {
            archive_entry_clear(e.0);
            if archive_read_disk_open(d.0, fc.as_ptr()) != OK {
                return Err(failed(format!("Couldn't read “{}”: {}", f.display(), la_msg(d.0))));
            }
            if archive_read_next_header2(d.0, e.0) < WARN {
                let m = la_msg(d.0);
                archive_read_close(d.0);
                return Err(failed(format!("Couldn't read “{}”: {m}", f.display())));
            }
        }
        let nc = CString::new(name.as_bytes()).map_err(|_| failed("A name with a NUL byte"))?;
        unsafe { archive_entry_set_pathname(e.0, nc.as_ptr()) };
        p.set_current(name);
        if unsafe { archive_write_header(w.0, e.0) } < WARN {
            let m = la_msg(w.0);
            unsafe { archive_read_close(d.0) };
            return Err(failed(format!("Couldn't add “{name}”: {m}")));
        }
        let ft = unsafe { archive_entry_filetype(e.0) } & AE_IFMT;
        if ft == AE_IFREG {
            let mut src = File::open(f).map_err(|x| failed(format!("Couldn't read “{}”: {x}", f.display())))?;
            loop {
                if let Err(x) = p.check() {
                    unsafe { archive_read_close(d.0) };
                    return Err(x);
                }
                let n = src.read(&mut buf).map_err(|x| failed(format!("Couldn't read “{}”: {x}", f.display())))?;
                if n == 0 {
                    break;
                }
                if unsafe { archive_write_data(w.0, buf.as_ptr() as *const c_void, n) } < 0 {
                    let m = la_msg(w.0);
                    unsafe { archive_read_close(d.0) };
                    return Err(failed(format!("Couldn't write “{name}”: {m}")));
                }
                p.add(n as u64);
            }
            p.files.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { archive_read_close(d.0) };
        let _ = (raw, AE_IFLNK, archive_read_disk_descend as unsafe extern "C" fn(*mut Archive) -> c_int);
    }
    Ok(())
}

// ----------------------------------------------------------------- update

/// Changes to make inside an archive.
#[derive(Clone, Debug, Default)]
pub struct Changes {
    /// Files and folders to add, and the folder inside to put them in ("" = top).
    pub add: Vec<PathBuf>,
    pub add_into: String,
    /// Members to remove (folders with everything in them).
    pub delete: Vec<String>,
    /// (old path inside, new name).
    pub rename: Vec<(String, String)>,
    /// New empty folders (paths inside).
    pub mkdir: Vec<String>,
    pub password: Option<String>,
    /// Written back at this level (5 = 7-Zip's Normal).
    pub level: u32,
}

/// Apply `c` to the archive: taken out next to it, changed, packed again,
/// swapped in. The archive is untouched if anything fails.
pub fn update(path: &Path, c: &Changes, p: &Progress) -> Res<()> {
    let Some(fmt) = Format::of(path) else {
        return Err(ArcError::ReadOnly(kind_label(path)));
    };
    if volumes(path).len() > 1 {
        return Err(failed("Split archives can't be changed: extract it, then make a new one."));
    }
    let listing = list(path, c.password.as_deref(), p)?;
    let dir = path.parent().unwrap_or(Path::new("/"));
    let work = tempdir_in(dir, ".grove-archive-")?;
    let res = (|| -> Res<()> {
        let tree = work.0.join("t");
        std::fs::create_dir_all(&tree).map_err(|e| failed(e.to_string()))?;
        if !listing.items.is_empty() {
            let o = ExtractOpts { password: c.password.clone(), overwrite: Overwrite::Replace, ..Default::default() };
            let sub = Progress::new();
            let rep = extract_quiet(path, &tree, &o, &sub, p)?;
            if let Some(e) = rep.errors.first() {
                return Err(failed(e.clone()));
            }
        }
        for d in &c.delete {
            let rel = safe_rel(d).ok_or_else(|| failed("A bad path"))?;
            let t = tree.join(rel);
            let m = std::fs::symlink_metadata(&t);
            if let Ok(m) = m {
                if m.is_dir() {
                    std::fs::remove_dir_all(&t).map_err(|e| failed(e.to_string()))?;
                } else {
                    std::fs::remove_file(&t).map_err(|e| failed(e.to_string()))?;
                }
            }
        }
        for (from, to) in &c.rename {
            if to.contains('/') || to.is_empty() || to == "." || to == ".." {
                return Err(failed(format!("“{to}” can't be a name")));
            }
            let f = tree.join(safe_rel(from).ok_or_else(|| failed("A bad path"))?);
            let t = f.with_file_name(to);
            if t.exists() {
                return Err(failed(format!("“{to}” is already there")));
            }
            std::fs::rename(&f, &t).map_err(|e| failed(format!("Couldn't rename: {e}")))?;
        }
        for d in &c.mkdir {
            let rel = safe_rel(d).ok_or_else(|| failed("A bad path"))?;
            std::fs::create_dir_all(tree.join(rel)).map_err(|e| failed(e.to_string()))?;
        }
        if !c.add.is_empty() {
            let into = if c.add_into.is_empty() { tree.clone() } else { tree.join(safe_rel(&c.add_into).ok_or_else(|| failed("A bad path"))?) };
            std::fs::create_dir_all(&into).map_err(|e| failed(e.to_string()))?;
            for src in &c.add {
                p.check()?;
                let name = src.file_name().ok_or_else(|| failed("A bad path"))?;
                copy_tree(src, &into.join(name), p)?;
            }
        }
        let mut tops: Vec<PathBuf> = std::fs::read_dir(&tree).map_err(|e| failed(e.to_string()))?.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        tops.sort();
        let out = work.0.join(format!("new.{}", fmt.ext()));
        let o = CreateOpts {
            format: fmt,
            level: c.level,
            password: c.password.clone(),
            encrypt_names: listing.header_encrypted,
            solid: listing.solid || fmt != Format::SevenZ,
            zip_encryption: if listing.items.iter().any(|i| i.encrypted && i.method == "AES") || fmt != Format::Zip { "aes256".into() } else if listing.encrypted { "zipcrypto".into() } else { "aes256".into() },
            ..Default::default()
        };
        // Only encrypt again if it was encrypted.
        let o = CreateOpts { password: if listing.encrypted { o.password.clone() } else { None }, ..o };
        p.set_current("Writing the archive");
        create(&out, &tops, &o, p)?;
        // Keep the original's permissions.
        if let Ok(m) = std::fs::metadata(path) {
            let _ = std::fs::set_permissions(&out, m.permissions());
        }
        std::fs::rename(&out, path).map_err(|e| failed(format!("Couldn't replace the archive: {e}")))?;
        Ok(())
    })();
    drop(work);
    res
}

/// Extract with progress going to `sub` (and cancel coming from `outer`).
fn extract_quiet(path: &Path, dest: &Path, o: &ExtractOpts, sub: &Progress, outer: &Progress) -> Res<Report> {
    // The outer cancel reaches the inner job through a watcher thread.
    let done = AtomicBool::new(false);
    std::thread::scope(|s| {
        s.spawn(|| {
            while !done.load(Ordering::Relaxed) {
                if outer.cancel.load(Ordering::Relaxed) {
                    sub.stop();
                }
                sub.paused.store(outer.paused.load(Ordering::Relaxed), Ordering::Relaxed);
                std::thread::sleep(std::time::Duration::from_millis(60));
            }
        });
        let r = extract(path, dest, o, sub);
        done.store(true, Ordering::Relaxed);
        r
    })
}

fn copy_tree(src: &Path, dst: &Path, p: &Progress) -> Res<()> {
    let m = std::fs::symlink_metadata(src).map_err(|e| failed(format!("Couldn't read “{}”: {e}", src.display())))?;
    if m.is_dir() {
        std::fs::create_dir_all(dst).map_err(|e| failed(e.to_string()))?;
        for e in std::fs::read_dir(src).map_err(|e| failed(e.to_string()))?.flatten() {
            p.check()?;
            copy_tree(&e.path(), &dst.join(e.file_name()), p)?;
        }
        let _ = std::fs::set_permissions(dst, m.permissions());
    } else if m.file_type().is_symlink() {
        let t = std::fs::read_link(src).map_err(|e| failed(e.to_string()))?;
        let _ = std::fs::remove_file(dst);
        std::os::unix::fs::symlink(t, dst).map_err(|e| failed(e.to_string()))?;
    } else {
        if dst.is_dir() {
            std::fs::remove_dir_all(dst).map_err(|e| failed(e.to_string()))?;
        }
        std::fs::copy(src, dst).map_err(|e| failed(format!("Couldn't copy “{}”: {e}", src.display())))?;
        if let Ok(t) = m.modified() {
            if let Ok(f) = File::options().write(true).open(dst) {
                let _ = f.set_modified(t);
            }
        }
    }
    Ok(())
}

/// A hidden work folder, removed on drop.
struct TempDir(PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn tempdir_in(dir: &Path, prefix: &str) -> Res<TempDir> {
    for i in 0..1000u32 {
        let n = format!("{prefix}{}-{i}", std::process::id());
        let p = dir.join(n);
        if std::fs::create_dir(&p).is_ok() {
            return Ok(TempDir(p));
        }
    }
    Err(failed(format!("Couldn't make a work folder in “{}”", dir.display())))
}

// ---------------------------------------------------------------- helpers

/// Where "Extract" puts things: `base` itself when everything is already in
/// one top folder (and that name is free), else a folder named after the
/// archive (made unique). Returns (folder to extract into, the item the
/// user will see).
pub fn destination(archive: &Path, listing: &Listing, base: &Path, mode: &str) -> (PathBuf, PathBuf) {
    let name = base_name(&archive.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    let stem = strip_archive_ext(&name);
    let sub = || {
        let d = base.join(&stem);
        let d = if d.exists() { crate::ops::unique_name(&d) } else { d };
        (d.clone(), d)
    };
    match mode {
        "here" => (base.to_path_buf(), base.to_path_buf()),
        "subfolder" => sub(),
        _ => {
            let tops: std::collections::BTreeSet<&str> = listing.items.iter().map(|i| i.path.split('/').next().unwrap_or("")).filter(|s| !s.is_empty()).collect();
            if tops.len() == 1 {
                let t = base.join(tops.iter().next().unwrap());
                if !t.exists() {
                    return (base.to_path_buf(), t);
                }
            }
            sub()
        }
    }
}

/// "photos.tar.gz" -> "photos", "a.7z" -> "a".
pub fn strip_archive_ext(name: &str) -> String {
    let l = name.to_lowercase();
    for e in [".tar.gz", ".tar.xz", ".tar.bz2", ".tar.zst", ".tar.zstd", ".tar.lz4", ".tar.lz", ".tar.lzma", ".tar.z"] {
        if l.ends_with(e) && name.len() > e.len() {
            return name[..name.len() - e.len()].to_string();
        }
    }
    match name.rsplit_once('.') {
        Some((s, _)) if !s.is_empty() => s.to_string(),
        _ => format!("{name} contents"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(t: &Path) -> PathBuf {
        let src = t.join("src");
        std::fs::create_dir_all(src.join("docs/deep")).unwrap();
        std::fs::write(src.join("readme.txt"), "hello archive ".repeat(200)).unwrap();
        std::fs::write(src.join("docs/a.md"), "a").unwrap();
        std::fs::write(src.join("docs/deep/b.bin"), vec![7u8; 100_000]).unwrap();
        std::fs::write(src.join("empty"), "").unwrap();
        src
    }

    fn names(l: &Listing) -> Vec<String> {
        let mut v: Vec<String> = l.items.iter().filter(|i| !i.is_dir).map(|i| i.path.clone()).collect();
        v.sort();
        v
    }

    #[test]
    fn every_format_round_trips() {
        let t = tempfile::tempdir().unwrap();
        let src = tree(t.path());
        let items = vec![src.join("readme.txt"), src.join("docs"), src.join("empty")];
        for fmt in ["7z", "zip", "tar", "tar.gz", "tar.xz", "tar.bz2", "tar.zst"] {
            for pw in [None, Some("s3cret")] {
                if pw.is_some() && !(fmt == "7z" || fmt == "zip") {
                    continue;
                }
                let p = Progress::new();
                let out = t.path().join(format!("x-{}.{fmt}", pw.is_some()));
                let o = CreateOpts { format: Format::parse(fmt).unwrap(), level: 7, password: pw.map(String::from), encrypt_names: pw.is_some(), ..Default::default() };
                let files = create(&out, &items, &o, &p).unwrap_or_else(|e| panic!("{fmt}: {e}"));
                assert_eq!(files, vec![out.clone()]);
                if pw.is_some() {
                    let e = list(&out, None, &p);
                    if fmt == "7z" {
                        assert_eq!(e.unwrap_err(), ArcError::NeedPassword, "{fmt} names are hidden");
                    }
                    let d = t.path().join(format!("nopw-{fmt}"));
                    let r = extract(&out, &d, &ExtractOpts::default(), &p);
                    assert!(matches!(r, Err(ArcError::NeedPassword)), "{fmt}: {r:?}");
                    let r = extract(&out, &d, &ExtractOpts { password: Some("wrong".into()), ..Default::default() }, &p);
                    assert!(matches!(r, Err(ArcError::BadPassword) | Err(ArcError::Failed(_))), "{fmt} wrong: {r:?}");
                }
                let l = list(&out, pw, &p).unwrap_or_else(|e| panic!("{fmt}: {e}"));
                assert_eq!(names(&l), vec!["docs/a.md", "docs/deep/b.bin", "empty", "readme.txt"], "{fmt}");
                if pw.is_some() {
                    assert!(l.encrypted, "{fmt} encrypted");
                }
                if fmt == "7z" || fmt == "zip" {
                    let b = l.items.iter().find(|i| i.path == "docs/deep/b.bin").unwrap();
                    assert!(!b.crc.is_empty(), "{fmt} crc");
                }
                let rep = test(&out, pw, &p).unwrap();
                assert!(rep.errors.is_empty(), "{fmt}: {:?}", rep.errors);
                assert_eq!(rep.files, 4, "{fmt} tested");
                let dest = t.path().join(format!("out-{fmt}-{}", pw.is_some()));
                let rep = extract(&out, &dest, &ExtractOpts { password: pw.map(String::from), ..Default::default() }, &p).unwrap();
                assert!(rep.errors.is_empty(), "{fmt}: {:?}", rep.errors);
                assert_eq!(std::fs::read(dest.join("docs/deep/b.bin")).unwrap(), vec![7u8; 100_000], "{fmt}");
                assert_eq!(std::fs::read_to_string(dest.join("readme.txt")).unwrap(), "hello archive ".repeat(200));
            }
        }
    }

    #[test]
    fn overwrite_rules_and_paths() {
        let t = tempfile::tempdir().unwrap();
        let src = tree(t.path());
        let p = Progress::new();
        let out = t.path().join("a.zip");
        create(&out, &[src.join("docs")], &CreateOpts { format: Format::Zip, ..Default::default() }, &p).unwrap();
        let dest = t.path().join("d");
        std::fs::create_dir_all(dest.join("docs")).unwrap();
        std::fs::write(dest.join("docs/a.md"), "mine").unwrap();
        extract(&out, &dest, &ExtractOpts { overwrite: Overwrite::Skip, ..Default::default() }, &p).unwrap();
        assert_eq!(std::fs::read_to_string(dest.join("docs/a.md")).unwrap(), "mine");
        extract(&out, &dest, &ExtractOpts { overwrite: Overwrite::Rename, ..Default::default() }, &p).unwrap();
        assert_eq!(std::fs::read_to_string(dest.join("docs/a 2.md")).unwrap(), "a");
        extract(&out, &dest, &ExtractOpts { overwrite: Overwrite::Replace, ..Default::default() }, &p).unwrap();
        assert_eq!(std::fs::read_to_string(dest.join("docs/a.md")).unwrap(), "a");
        // Selected members, relative to their folder, without paths.
        let d2 = t.path().join("d2");
        extract(&out, &d2, &ExtractOpts { only: vec!["docs/deep".into()], strip: "docs/".into(), ..Default::default() }, &p).unwrap();
        assert!(d2.join("deep/b.bin").is_file());
        let d3 = t.path().join("d3");
        extract(&out, &d3, &ExtractOpts { keep_paths: false, ..Default::default() }, &p).unwrap();
        assert!(d3.join("b.bin").is_file() && d3.join("a.md").is_file());
        // Asking: the answer comes from another thread.
        let pa = std::sync::Arc::new(Progress::new());
        let pb = pa.clone();
        let h = std::thread::spawn(move || loop {
            if pb.question().is_some() {
                pb.reply("skipAll");
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        });
        std::fs::write(dest.join("docs/a.md"), "mine again").unwrap();
        extract(&out, &dest, &ExtractOpts::default(), &pa).unwrap();
        h.join().unwrap();
        assert_eq!(std::fs::read_to_string(dest.join("docs/a.md")).unwrap(), "mine again");
    }

    #[test]
    fn volumes_and_updates() {
        let t = tempfile::tempdir().unwrap();
        let src = tree(t.path());
        let p = Progress::new();
        let out = t.path().join("v.7z");
        // Incompressible data, so the archive needs several volumes.
        let mut noise = vec![0u8; 300_000];
        let mut x = 12345u32;
        for b in noise.iter_mut() {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *b = x as u8;
        }
        std::fs::write(src.join("noise.bin"), &noise).unwrap();
        let files = create(&out, &[src.join("noise.bin"), src.join("docs")], &CreateOpts { volume_size: 100_000, ..Default::default() }, &p).unwrap();
        assert!(files.len() >= 3, "{files:?}");
        assert!(!out.exists());
        let l = list(&files[0], None, &p).unwrap();
        assert_eq!(l.volumes.len(), files.len());
        let d = t.path().join("vd");
        extract(&files[1], &d, &ExtractOpts::default(), &p).unwrap();
        assert_eq!(std::fs::read(d.join("noise.bin")).unwrap(), noise);

        for (name, pw) in [("u.7z", Some("pw")), ("u.zip", None), ("u.tar.gz", None)] {
            let a = t.path().join(name);
            create(&a, &[src.join("docs"), src.join("readme.txt")], &CreateOpts { format: Format::of(&a).unwrap(), password: pw.map(String::from), ..Default::default() }, &p).unwrap();
            let c = Changes { delete: vec!["docs/deep".into()], rename: vec![("readme.txt".into(), "READ ME.txt".into())], add: vec![src.join("empty")], add_into: "docs".into(), mkdir: vec!["new folder".into()], password: pw.map(String::from), level: 5 };
            update(&a, &c, &p).unwrap_or_else(|e| panic!("{name}: {e}"));
            let l = list(&a, pw, &p).unwrap();
            assert_eq!(names(&l), vec!["READ ME.txt", "docs/a.md", "docs/empty"], "{name}");
            assert!(l.items.iter().any(|i| i.is_dir && i.path == "new folder"), "{name}");
            if pw.is_some() {
                assert!(l.encrypted);
            }
            // No work folder left.
            assert!(!std::fs::read_dir(t.path()).unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with(".grove-archive-")));
        }
        assert!(matches!(update(&files[0], &Changes::default(), &p), Err(ArcError::Failed(_))));
    }

    #[test]
    fn single_files_and_names() {
        let t = tempfile::tempdir().unwrap();
        let src = tree(t.path());
        let p = Progress::new();
        let out = t.path().join("readme.txt.zst");
        create(&out, &[src.join("readme.txt")], &CreateOpts { format: Format::Zst, ..Default::default() }, &p).unwrap();
        let l = list(&out, None, &p).unwrap();
        assert_eq!(l.items[0].path, "readme.txt");
        assert_eq!(l.items[0].size, 2800);
        let d = t.path().join("sd");
        extract(&out, &d, &ExtractOpts::default(), &p).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("readme.txt")).unwrap(), "hello archive ".repeat(200));
        assert!(create(&t.path().join("x.gz"), &[src.join("docs")], &CreateOpts { format: Format::Gz, ..Default::default() }, &p).is_err());
        assert_eq!(base_name("a.7z.003"), "a.7z");
        assert_eq!(base_name("movie.part02.rar"), "movie.rar");
        assert_eq!(strip_archive_ext("photos.tar.gz"), "photos");
        assert_eq!(kind_of(Path::new("x.tgz")), Kind::Tar("gzip"));
        // Cancel stops at once and leaves nothing.
        let pc = Progress::new();
        pc.stop();
        let o = t.path().join("c.7z");
        assert_eq!(create(&o, &[src.clone()], &CreateOpts::default(), &pc).unwrap_err(), ArcError::Cancelled);
        assert!(!o.exists());
    }

    #[test]
    fn destination_rules() {
        let t = tempfile::tempdir().unwrap();
        let a = t.path().join("pics.zip");
        let one = Listing { items: vec![Item { path: "Pics".into(), is_dir: true, ..Default::default() }, Item { path: "Pics/a.jpg".into(), ..Default::default() }], ..Default::default() };
        let many = Listing { items: vec![Item { path: "a.jpg".into(), ..Default::default() }, Item { path: "b.jpg".into(), ..Default::default() }], ..Default::default() };
        assert_eq!(destination(&a, &one, t.path(), "auto"), (t.path().to_path_buf(), t.path().join("Pics")));
        assert_eq!(destination(&a, &many, t.path(), "auto").0, t.path().join("pics"));
        std::fs::create_dir(t.path().join("Pics")).unwrap();
        assert_eq!(destination(&a, &one, t.path(), "auto").0, t.path().join("pics"));
        std::fs::create_dir(t.path().join("pics")).unwrap();
        assert_eq!(destination(&a, &many, t.path(), "subfolder").0, t.path().join("pics 2"));
    }
}
