//! Windows Installer packages (.msi, .msp) in Rust (msi + cab crates, no
//! 7-Zip): the files laid out as they'd be installed (Directory, Component
//! and File tables), read from the package's cabinets (embedded streams
//! "#name", or .cab files next to it).

use crate::libarchive::Member;
use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

struct FileRow {
    key: String,
    path: String,
    size: u64,
    seq: i32,
}

/// The long name in an MSI name field ("SHORT~1.TXT|Long name.txt").
fn long_name(s: &str) -> &str {
    s.rsplit('|').next().unwrap_or(s)
}

/// Every file's install path ("PFiles/OpenSSH/ssh.exe"), key, size, sequence.
fn files(pkg: &mut msi::Package<std::fs::File>) -> Result<Vec<FileRow>, String> {
    let e = |x: std::io::Error| x.to_string();
    // Directory: id -> (parent, name).
    let mut dirs: HashMap<String, (String, String)> = HashMap::new();
    for r in pkg.select_rows(msi::Select::table("Directory")).map_err(e)? {
        let id = r["Directory"].as_str().unwrap_or("").to_string();
        let parent = r["Directory_Parent"].as_str().unwrap_or("").to_string();
        // "target:source"; the target, long name; "." is the parent itself;
        // the root ("SourceDir", "TARGETDIR") isn't a folder of its own.
        let def = r["DefaultDir"].as_str().unwrap_or("");
        let target = def.split(':').next().unwrap_or(def);
        let mut name = long_name(target).to_string();
        if name == "." || id == "TARGETDIR" || name == "SourceDir" || name == "SOURCEDIR" {
            name.clear();
        }
        dirs.insert(id, (parent, name));
    }
    fn dir_path(id: &str, dirs: &HashMap<String, (String, String)>, depth: usize) -> String {
        if depth > 64 {
            return String::new();
        }
        let Some((parent, name)) = dirs.get(id) else { return String::new() };
        let up = if parent.is_empty() || parent == id { String::new() } else { dir_path(parent, dirs, depth + 1) };
        match (up.is_empty(), name.is_empty()) {
            (_, true) => up,
            (true, false) => name.clone(),
            (false, false) => format!("{up}/{name}"),
        }
    }
    let mut comp_dir: HashMap<String, String> = HashMap::new();
    for r in pkg.select_rows(msi::Select::table("Component")).map_err(e)? {
        comp_dir.insert(r["Component"].as_str().unwrap_or("").to_string(), r["Directory_"].as_str().unwrap_or("").to_string());
    }
    let mut out = Vec::new();
    if !pkg.has_table("File") {
        return Ok(out);
    }
    for r in pkg.select_rows(msi::Select::table("File")).map_err(e)? {
        let key = r["File"].as_str().unwrap_or("").to_string();
        let comp = r["Component_"].as_str().unwrap_or("");
        let name = long_name(r["FileName"].as_str().unwrap_or(&key)).to_string();
        let dir = comp_dir.get(comp).map(|d| dir_path(d, &dirs, 0)).unwrap_or_default();
        let path = if dir.is_empty() { name } else { format!("{dir}/{name}") };
        out.push(FileRow { key, path, size: r["FileSize"].as_int().unwrap_or(0).max(0) as u64, seq: r["Sequence"].as_int().unwrap_or(0) });
    }
    Ok(out)
}

fn open(path: &Path) -> Result<msi::Package<std::fs::File>, String> {
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    msi::Package::open(f).map_err(|e| format!("This installer can't be read: {e}"))
}

/// Every file and the folders holding them.
pub fn list(path: &Path) -> Result<Vec<Member>, String> {
    let mut pkg = open(path)?;
    let mut v: Vec<Member> = Vec::new();
    let mut seen_dirs = std::collections::HashSet::new();
    for f in files(&mut pkg)? {
        let mut acc = String::new();
        let parts: Vec<&str> = f.path.split('/').collect();
        for d in &parts[..parts.len() - 1] {
            acc = if acc.is_empty() { d.to_string() } else { format!("{acc}/{d}") };
            if seen_dirs.insert(acc.clone()) {
                v.push(Member { path: acc.clone(), size: 0, is_dir: true, mtime: 0 });
            }
        }
        v.push(Member { path: f.path, size: f.size, is_dir: false, mtime: 0 });
    }
    Ok(v)
}

/// The cabinets ("Media" table) in sequence order: (last sequence, bytes).
fn cabinets(pkg: &mut msi::Package<std::fs::File>, msi_path: &Path) -> Result<Vec<(i32, Vec<u8>)>, String> {
    let mut v = Vec::new();
    if !pkg.has_table("Media") {
        return Ok(v);
    }
    let rows: Vec<(i32, String)> = pkg
        .select_rows(msi::Select::table("Media"))
        .map_err(|e| e.to_string())?
        .map(|r| (r["LastSequence"].as_int().unwrap_or(0), r["Cabinet"].as_str().unwrap_or("").to_string()))
        .collect();
    for (last, cab) in rows {
        if cab.is_empty() {
            continue;
        }
        let mut bytes = Vec::new();
        if let Some(stream) = cab.strip_prefix('#') {
            let mut r = pkg.read_stream(stream).map_err(|e| e.to_string())?;
            r.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        } else if let Some(dir) = msi_path.parent() {
            match std::fs::read(dir.join(&cab)) {
                Ok(b) => bytes = b,
                Err(_) => continue,
            }
        }
        v.push((last, bytes));
    }
    v.sort_by_key(|c| c.0);
    Ok(v)
}

/// Call `f` with each wanted file's install path and a reader of its bytes.
fn each_file(path: &Path, wanted: impl Fn(&str) -> bool, cancel: &AtomicBool, mut f: impl FnMut(&FileRow, &mut dyn Read) -> Result<bool, String>) -> Result<(), String> {
    let mut pkg = open(path)?;
    let rows = files(&mut pkg)?;
    let cabs = cabinets(&mut pkg, path)?;
    for (i, (last, bytes)) in cabs.iter().enumerate() {
        let first = if i == 0 { i32::MIN } else { cabs[i - 1].0 };
        let mut cab = cab::Cabinet::new(Cursor::new(bytes.as_slice())).map_err(|e| format!("A cabinet in the installer can't be read: {e}"))?;
        for row in rows.iter().filter(|r| r.seq > first && r.seq <= *last && wanted(&r.path)) {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            let Ok(mut r) = cab.read_file(&row.key) else { continue };
            if !f(row, &mut r)? {
                return Ok(());
            }
        }
    }
    Ok(())
}

fn under(p: &str, only: &[String]) -> bool {
    only.is_empty() || only.iter().any(|o| p == o || p.starts_with(&format!("{o}/")))
}

/// Extract into `dest`: everything, or the files and folders named.
pub fn extract(path: &Path, dest: &Path, only: &[&str], cancel: &AtomicBool) -> Result<(), String> {
    let only: Vec<String> = only.iter().map(|s| s.trim_matches('/').to_string()).collect();
    each_file(path, |p| under(p, &only), cancel, |row, r| {
        if row.path.split('/').any(|c| c == "..") {
            return Ok(true);
        }
        let out = dest.join(&row.path);
        if let Some(d) = out.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let mut w = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(r, &mut w).map_err(|e| format!("Couldn't extract “{}”: {e}", row.path))?;
        w.flush().ok();
        Ok(true)
    })
}

/// One file's bytes (up to `limit`).
pub fn read_member(path: &Path, name: &str, limit: usize, cancel: &AtomicBool) -> Option<Vec<u8>> {
    let want = name.trim_matches('/').to_string();
    let mut out = None;
    each_file(path, |p| p == want, cancel, |_row, r| {
        let mut b = Vec::new();
        r.take(limit as u64 + 1).read_to_end(&mut b).map_err(|e| e.to_string())?;
        if b.len() <= limit {
            out = Some(b);
        }
        Ok(false)
    })
    .ok()?;
    out
}
