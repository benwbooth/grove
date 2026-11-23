//! CD images grove reads itself when libarchive can't (old discs):
//! ISO 9660 with Joliet (long Windows names) and Rock Ridge names read
//! leniently (a broken SUSP/CE record is skipped, not fatal); raw images
//! with 2352-byte sectors (Mode 1 or Mode 2 Form 1, as BIN dumps); and
//! audio CDs dumped raw (16-bit stereo 44.1 kHz, no file system), shown as
//! WAV tracks split at the silent gaps between songs.

use crate::libarchive::Member;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

const RAW: u64 = 2352;
const SYNC: [u8; 12] = [0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0];

/// How 2048-byte data sectors sit in the file.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Layout {
    sector: u64,
    offset: u64,
}

fn layout(f: &mut File) -> Option<Layout> {
    let mut b = [0u8; 8];
    let mut at = |pos: u64, b: &mut [u8]| f.seek(SeekFrom::Start(pos)).ok().and_then(|_| f.read_exact(b).ok()).is_some();
    if at(16 * 2048, &mut b) && &b[1..6] == b"CD001" {
        return Some(Layout { sector: 2048, offset: 0 });
    }
    let mut s = [0u8; 16];
    if at(16 * RAW, &mut s) && s[..12] == SYNC {
        // Mode 1: data after the 16-byte header; Mode 2: after 8 more (subheader).
        let off = if s[15] == 2 { 24 } else { 16 };
        if at(16 * RAW + off, &mut b) && &b[1..6] == b"CD001" {
            return Some(Layout { sector: RAW, offset: off });
        }
    }
    None
}

struct Disc {
    f: File,
    l: Layout,
}

impl Disc {
    fn read(&mut self, lba: u64, len: u64) -> std::io::Result<Vec<u8>> {
        let mut out = Vec::with_capacity(len as usize);
        let mut left = len;
        let mut s = lba;
        let mut buf = vec![0u8; 2048];
        while left > 0 {
            self.f.seek(SeekFrom::Start(s * self.l.sector + self.l.offset))?;
            self.f.read_exact(&mut buf)?;
            let n = left.min(2048) as usize;
            out.extend_from_slice(&buf[..n]);
            left -= n as u64;
            s += 1;
        }
        Ok(out)
    }
    /// Copy `len` bytes from `lba` to `w` without holding them all.
    fn copy(&mut self, lba: u64, len: u64, w: &mut dyn Write, cancel: &AtomicBool) -> std::io::Result<()> {
        let mut left = len;
        let mut s = lba;
        if self.l.sector == 2048 {
            self.f.seek(SeekFrom::Start(lba * 2048))?;
            let mut buf = vec![0u8; 1 << 20];
            while left > 0 {
                if cancel.load(Ordering::Relaxed) {
                    return Err(std::io::Error::other("Cancelled"));
                }
                let n = left.min(buf.len() as u64) as usize;
                self.f.read_exact(&mut buf[..n])?;
                w.write_all(&buf[..n])?;
                left -= n as u64;
            }
            return Ok(());
        }
        let mut buf = vec![0u8; 2048];
        while left > 0 {
            if cancel.load(Ordering::Relaxed) {
                return Err(std::io::Error::other("Cancelled"));
            }
            self.f.seek(SeekFrom::Start(s * self.l.sector + self.l.offset))?;
            self.f.read_exact(&mut buf)?;
            let n = left.min(2048) as usize;
            w.write_all(&buf[..n])?;
            left -= n as u64;
            s += 1;
        }
        Ok(())
    }
}

/// A file's pieces on the disc (multi-extent files have several).
#[derive(Clone, Debug)]
struct Entry {
    path: String,
    is_dir: bool,
    size: u64,
    mtime: i64,
    extents: Vec<(u64, u64)>,
}

fn le32(b: &[u8]) -> u64 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as u64
}

/// A directory record's 7-byte date as Unix time (UTC offset applied).
fn record_time(d: &[u8]) -> i64 {
    if d.len() < 7 || d[0] == 0 {
        return 0;
    }
    let (y, mo, da, h, mi, s) = (1900 + d[0] as i64, d[1] as i64, d[2] as i64, d[3] as i64, d[4] as i64, d[5] as i64);
    let tz = (d[6] as i8) as i64 * 15 * 60;
    // Days from 1970-01-01 (civil calendar).
    let (yy, mm) = if mo <= 2 { (y - 1, mo + 9) } else { (y, mo - 3) };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doy = (153 * mm + 2) / 5 + da - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    days * 86400 + h * 3600 + mi * 60 + s - tz
}

/// Rock Ridge's name (NM) in a record's system use area, if any. Broken
/// entries end the search quietly.
fn rr_name(su: &[u8]) -> Option<String> {
    let mut i = 0;
    let mut name = String::new();
    while i + 4 <= su.len() {
        let (sig, len) = (&su[i..i + 2], su[i + 2] as usize);
        if len < 4 || i + len > su.len() {
            break;
        }
        if sig == b"NM" && len >= 5 {
            let flags = su[i + 4];
            if flags & 0x06 == 0 {
                name.push_str(&String::from_utf8_lossy(&su[i + 5..i + len]));
            }
        }
        if sig == b"ST" {
            break;
        }
        i += len;
    }
    (!name.is_empty()).then_some(name)
}

fn iso_name(raw: &[u8], joliet: bool) -> String {
    let n = if joliet {
        let u: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        String::from_utf8_lossy(raw).into_owned()
    };
    let n = n.split(';').next().unwrap_or(&n).to_string();
    n.strip_suffix('.').map(str::to_string).unwrap_or(n)
}

fn walk(d: &mut Disc, lba: u64, size: u64, prefix: &str, joliet: bool, depth: u32, out: &mut Vec<Entry>, cancel: &AtomicBool) {
    if depth > 32 || size > 64 << 20 || cancel.load(Ordering::Relaxed) {
        return;
    }
    let Ok(data) = d.read(lba, size) else { return };
    let mut i = 0usize;
    let mut pending: Option<Entry> = None;
    while i < data.len() {
        let len = data[i] as usize;
        if len == 0 {
            // Records don't cross sectors: on to the next one.
            i = (i / 2048 + 1) * 2048;
            continue;
        }
        if i + len > data.len() || len < 34 {
            break;
        }
        let r = &data[i..i + len];
        i += len;
        let nl = r[32] as usize;
        if 33 + nl > r.len() {
            continue;
        }
        let raw = &r[33..33 + nl];
        if nl == 1 && (raw[0] == 0 || raw[0] == 1) {
            continue; // "." and ".."
        }
        let flags = r[25];
        let is_dir = flags & 2 != 0;
        let su_start = 33 + nl + (if nl % 2 == 0 { 1 } else { 0 });
        let name = (!joliet).then(|| r.get(su_start..).and_then(rr_name)).flatten().unwrap_or_else(|| iso_name(raw, joliet));
        if name.is_empty() || name == "." || name == ".." || name.contains('/') && name.split('/').any(|c| c == "..") {
            continue;
        }
        let path = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        let (ext, dsize) = (le32(&r[2..6]), le32(&r[10..14]));
        let mtime = record_time(&r[18..25]);
        if is_dir {
            out.push(Entry { path: path.clone(), is_dir: true, size: 0, mtime, extents: vec![] });
            walk(d, ext, dsize, &path, joliet, depth + 1, out, cancel);
            continue;
        }
        // Multi-extent files: records with the "more" flag continue.
        match pending.as_mut() {
            Some(p) if p.path == path => {
                p.extents.push((ext, dsize));
                p.size += dsize;
            }
            _ => {
                if let Some(p) = pending.take() {
                    out.push(p);
                }
                pending = Some(Entry { path, is_dir: false, size: dsize, mtime, extents: vec![(ext, dsize)] });
            }
        }
        if flags & 0x80 == 0 {
            if let Some(p) = pending.take() {
                out.push(p);
            }
        }
    }
    if let Some(p) = pending.take() {
        out.push(p);
    }
}

fn open(path: &Path) -> Option<Disc> {
    let mut f = File::open(path).ok()?;
    let l = layout(&mut f)?;
    Some(Disc { f, l })
}

/// The data disc's entries: Joliet names when the disc has them, else
/// Rock Ridge / plain ISO 9660 names.
fn entries(path: &Path, cancel: &AtomicBool) -> Option<Vec<Entry>> {
    let mut d = open(path)?;
    let (mut pvd_root, mut joliet_root) = (None, None);
    for s in 16..64u64 {
        let Ok(v) = d.read(s, 2048) else { break };
        if &v[1..6] != b"CD001" {
            break;
        }
        let root = (le32(&v[156 + 2..156 + 6]), le32(&v[156 + 10..156 + 14]));
        match v[0] {
            1 if pvd_root.is_none() => pvd_root = Some(root),
            2 if matches!(&v[88..91], b"%/@" | b"%/C" | b"%/E") => joliet_root = Some(root),
            255 => break,
            _ => {}
        }
    }
    let mut out = Vec::new();
    if let Some((l, s)) = joliet_root {
        walk(&mut d, l, s, "", true, 0, &mut out, cancel);
    }
    if out.is_empty() {
        if let Some((l, s)) = pvd_root {
            walk(&mut d, l, s, "", false, 0, &mut out, cancel);
        }
    }
    Some(out)
}

// ---------------------------------------------------------------- audio CDs

/// Whether `path` looks like a raw audio CD dump: a whole number of 2352-
/// byte sectors, no data-sector sync patterns and no file system.
fn is_audio(path: &Path) -> bool {
    let Ok(m) = std::fs::metadata(path) else { return false };
    if m.len() < RAW * 75 || m.len() % RAW != 0 {
        return false;
    }
    let Ok(mut f) = File::open(path) else { return false };
    if layout(&mut f).is_some() {
        return false;
    }
    // A few sectors through the disc: none starts with a data sync.
    let n = m.len() / RAW;
    let mut b = [0u8; 12];
    for k in 1..8u64 {
        let s = n * k / 8;
        if f.seek(SeekFrom::Start(s * RAW)).is_err() || f.read_exact(&mut b).is_err() || b == SYNC {
            return false;
        }
    }
    true
}

/// Tracks of an audio dump as (first byte, length), split at runs of
/// digital silence of at least 1.5 s (the gaps between songs); cached.
fn tracks(path: &Path, cancel: &AtomicBool) -> Vec<(u64, u64)> {
    let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let key = format!("{}|{len}|{}", path.display(), std::fs::metadata(path).ok().map(|m| std::os::unix::fs::MetadataExt::mtime(&m)).unwrap_or(0));
    static CACHE: std::sync::Mutex<Option<(String, Vec<(u64, u64)>)>> = std::sync::Mutex::new(None);
    if let Some((k, v)) = CACHE.lock().unwrap().as_ref() {
        if *k == key {
            return v.clone();
        }
    }
    let Ok(mut f) = File::open(path) else { return vec![(0, len)] };
    // Per sector (1/75 s): silent or not (every sample within ±2).
    let mut silent: Vec<bool> = Vec::with_capacity((len / RAW) as usize);
    let mut buf = vec![0u8; (RAW * 75 * 4) as usize];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return vec![(0, len)];
        }
        let n = match f.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        for sec in buf[..n].chunks(RAW as usize) {
            silent.push(sec.chunks_exact(2).all(|s| i16::from_le_bytes([s[0], s[1]]).unsigned_abs() <= 2));
        }
    }
    let min_gap = 112; // 1.5 s of sectors
    let mut cuts = Vec::new(); // sector where each track starts
    let mut i = 0;
    // Leading silence belongs to nothing.
    while i < silent.len() && silent[i] {
        i += 1;
    }
    let start = i;
    cuts.push(start);
    while i < silent.len() {
        if silent[i] {
            let mut j = i;
            while j < silent.len() && silent[j] {
                j += 1;
            }
            if j - i >= min_gap && j < silent.len() {
                cuts.push(j);
            }
            i = j;
        } else {
            i += 1;
        }
    }
    let total = silent.len();
    let mut v: Vec<(u64, u64)> = cuts
        .iter()
        .enumerate()
        .map(|(k, &s)| {
            let e = cuts.get(k + 1).copied().unwrap_or(total);
            (s as u64 * RAW, (e - s) as u64 * RAW)
        })
        .filter(|(_, l)| *l > 0)
        .collect();
    if v.is_empty() {
        v.push((0, len));
    }
    *CACHE.lock().unwrap() = Some((key, v.clone()));
    v
}

fn wav_header(data_len: u64) -> Vec<u8> {
    let d = data_len.min(u32::MAX as u64 - 36) as u32;
    let mut h = Vec::with_capacity(44);
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&(36 + d).to_le_bytes());
    h.extend_from_slice(b"WAVEfmt ");
    h.extend_from_slice(&16u32.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes()); // PCM
    h.extend_from_slice(&2u16.to_le_bytes()); // stereo
    h.extend_from_slice(&44100u32.to_le_bytes());
    h.extend_from_slice(&(44100u32 * 4).to_le_bytes());
    h.extend_from_slice(&4u16.to_le_bytes());
    h.extend_from_slice(&16u16.to_le_bytes());
    h.extend_from_slice(b"data");
    h.extend_from_slice(&d.to_le_bytes());
    h
}

fn track_name(k: usize) -> String {
    format!("Track {:02}.wav", k + 1)
}

// ---------------------------------------------------------------- the API

/// Whether grove's own reader understands `path` (data or audio disc).
pub fn readable(path: &Path) -> bool {
    File::open(path).ok().and_then(|mut f| layout(&mut f)).is_some() || is_audio(path)
}

pub fn list(path: &Path, cancel: &AtomicBool) -> Result<Vec<Member>, String> {
    if is_audio(path) {
        return Ok(tracks(path, cancel).iter().enumerate().map(|(k, (_, l))| Member { path: track_name(k), size: 44 + l, is_dir: false, mtime: 0 }).collect());
    }
    let e = entries(path, cancel).ok_or("This disc image can't be read.")?;
    Ok(e.into_iter().map(|e| Member { path: e.path, size: e.size, is_dir: e.is_dir, mtime: e.mtime }).collect())
}

fn under(p: &str, only: &[String]) -> bool {
    only.is_empty() || only.iter().any(|o| p == o || p.starts_with(&format!("{o}/")))
}

pub fn extract(path: &Path, dest: &Path, only: &[&str], cancel: &AtomicBool) -> Result<(), String> {
    let only: Vec<String> = only.iter().map(|s| s.trim_matches('/').to_string()).collect();
    if is_audio(path) {
        let mut f = File::open(path).map_err(|e| e.to_string())?;
        for (k, (start, len)) in tracks(path, cancel).into_iter().enumerate() {
            let name = track_name(k);
            if !under(&name, &only) {
                continue;
            }
            std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
            let mut w = std::io::BufWriter::new(File::create(dest.join(&name)).map_err(|e| e.to_string())?);
            w.write_all(&wav_header(len)).map_err(|e| e.to_string())?;
            f.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
            std::io::copy(&mut (&mut f).take(len), &mut w).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    let list = entries(path, cancel).ok_or("This disc image can't be read.")?;
    let mut d = open(path).ok_or("This disc image can't be read.")?;
    for e in list.iter().filter(|e| under(&e.path, &only)) {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        let out = dest.join(&e.path);
        if e.is_dir {
            std::fs::create_dir_all(&out).map_err(|x| x.to_string())?;
            continue;
        }
        if let Some(p) = out.parent() {
            std::fs::create_dir_all(p).map_err(|x| x.to_string())?;
        }
        let mut w = std::io::BufWriter::new(File::create(&out).map_err(|x| x.to_string())?);
        for (lba, len) in &e.extents {
            d.copy(*lba, *len, &mut w, cancel).map_err(|x| format!("Couldn't extract “{}”: {x}", e.path))?;
        }
    }
    Ok(())
}

pub fn read_member(path: &Path, name: &str, limit: usize, cancel: &AtomicBool) -> Option<Vec<u8>> {
    let want = name.trim_matches('/');
    let tmp = std::env::temp_dir().join(format!("grove-iso-{}-{}", std::process::id(), crate::thumbs::md5_hex(want)));
    extract(path, &tmp, &[want], cancel).ok()?;
    let b = std::fs::read(tmp.join(want)).ok();
    let _ = std::fs::remove_dir_all(&tmp);
    b.filter(|b| b.len() <= limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dates() {
        // 1999-01-06 17:48:00 UTC
        assert_eq!(record_time(&[99, 1, 6, 17, 48, 0, 0]), 915644880);
    }
    #[test]
    fn rock_ridge_names() {
        let mut su = vec![b'N', b'M', 9, 1, 0];
        su.extend_from_slice(b"read");
        assert_eq!(rr_name(&su).as_deref(), Some("read"));
        // A broken entry (length 0) ends the search without a name.
        assert_eq!(rr_name(&[b'C', b'E', 0, 1]), None);
    }
}
