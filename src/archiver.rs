//! Grove Archiver's jobs: list, extract, add to archive, test and change
//! archives on worker threads (grove_core::archiver). The windows poll
//! `progress(id)` (a snapshot, no waiting) and answer questions (a file
//! already there) with `answer(id, ...)`.

use crate::bridge::ffi::{self, QString};
use cxx_qt::CxxQtThread;
use grove_core::archiver::{self as arc, ArcError, Changes, CreateOpts, ExtractOpts, Format, Overwrite, Progress};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

struct Job {
    kind: String,
    title: String,
    p: Arc<Progress>,
    end: Arc<Mutex<Option<Value>>>,
    started: Instant,
}

fn jobs() -> &'static Mutex<HashMap<i32, Job>> {
    static J: OnceLock<Mutex<HashMap<i32, Job>>> = OnceLock::new();
    J.get_or_init(Default::default)
}

fn s(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or("").to_string()
}
fn strs(v: &Value, k: &str) -> Vec<String> {
    v[k].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}
fn pw(v: &Value) -> Option<String> {
    v["password"].as_str().filter(|s| !s.is_empty()).map(String::from)
}
fn name_of(p: &str) -> String {
    Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn err_json(e: &ArcError) -> Value {
    json!({"state": if *e == ArcError::Cancelled { "cancelled" } else { "failed" }, "code": e.code(), "error": e.to_string()})
}

/// The listing for the archive window (async).
pub fn list_json(path: &str, password: Option<&str>) -> Value {
    match arc::list(Path::new(path), password, &Progress::new()) {
        Ok(l) => serde_json::to_value(&l).unwrap_or(Value::Null),
        Err(e) => err_json(&e),
    }
}

/// Start a job. kind: extract, create, test, update. Returns its id.
pub fn start(qt: CxxQtThread<ffi::Grove>, kind: &str, args: &str) -> i32 {
    static NEXT: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(1);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let a: Value = serde_json::from_str(args).unwrap_or(Value::Null);
    let p = Arc::new(Progress::new());
    let end = Arc::new(Mutex::new(None));
    let title = match kind {
        "extract" => format!("Extracting “{}”", name_of(&s(&a, "archive"))),
        "create" => format!("Compressing into “{}”", name_of(&s(&a, "out"))),
        "test" => format!("Testing “{}”", name_of(&s(&a, "archive"))),
        _ => format!("Updating “{}”", name_of(&s(&a, "archive"))),
    };
    jobs().lock().unwrap().insert(id, Job { kind: kind.into(), title, p: p.clone(), end: end.clone(), started: Instant::now() });
    let kind = kind.to_string();
    std::thread::spawn(move || {
        let r = match kind.as_str() {
            "extract" => do_extract(&a, &p),
            "create" => do_create(&a, &p),
            "test" => arc::test(Path::new(&s(&a, "archive")), pw(&a).as_deref(), &p).map(|rep| json!({"report": rep})),
            _ => do_update(&a, &p),
        };
        let v = match r {
            Ok(mut v) => {
                v["state"] = json!("done");
                v
            }
            Err(e) => err_json(&e),
        };
        // Folders that changed: refresh grove's views of them.
        let mut dirs: Vec<String> = Vec::new();
        for k in ["dest", "dir"] {
            if let Some(d) = v[k].as_str() {
                dirs.push(d.to_string());
            }
        }
        if let Some(a) = a["archive"].as_str() {
            if let Some(d) = Path::new(a).parent() {
                dirs.push(d.to_string_lossy().into_owned());
            }
        }
        *end.lock().unwrap() = Some(v);
        if !dirs.is_empty() {
            let s = serde_json::to_string(&dirs).unwrap();
            let _ = qt.queue(move |g| g.folders_touched(QString::from(s.as_str())));
        }
    });
    id
}

fn do_extract(a: &Value, p: &Progress) -> Result<Value, ArcError> {
    let archive = PathBuf::from(s(a, "archive"));
    let password = pw(a);
    let base = match s(a, "dest") {
        d if d.is_empty() => archive.parent().unwrap_or(Path::new("/")).to_path_buf(),
        d => PathBuf::from(d),
    };
    let only = strs(a, "only");
    let mode = match s(a, "folder").as_str() {
        "always" => "subfolder",
        "never" => "here",
        _ => "auto",
    };
    // A folder named after the archive unless what's taken out is one
    // folder already (judged on what's being extracted).
    let listing = arc::list(&archive, password.as_deref(), p)?;
    let strip = s(a, "strip");
    let mut view = listing.clone();
    if !only.is_empty() || !strip.is_empty() {
        view.items.retain(|i| only.is_empty() || only.iter().any(|o| i.path == *o || i.path.starts_with(&format!("{o}/"))));
        for i in view.items.iter_mut() {
            i.path = i.path.strip_prefix(&strip).unwrap_or(&i.path).to_string();
        }
    }
    let (dest, shown) = arc::destination(&archive, &view, &base, mode);
    let o = ExtractOpts {
        password,
        only,
        strip,
        keep_paths: a["keepPaths"].as_bool().unwrap_or(true),
        overwrite: Overwrite::parse(&s(a, "overwrite")),
    };
    let made_dir = dest != base && !dest.exists();
    let rep = arc::extract(&archive, &dest, &o, p).inspect_err(|_| {
        if made_dir {
            let _ = std::fs::remove_dir_all(&dest);
        }
    })?;
    let created: Vec<PathBuf> = if dest != base { vec![dest.clone()] } else { rep.created.iter().map(PathBuf::from).collect() };
    if !created.is_empty() {
        crate::controller::push_history_any("Extract", grove_core::ops::Action::Exists { paths: created.clone() });
    }
    Ok(json!({"report": rep, "dest": base, "shown": shown, "created": created, "whole": o.only.is_empty()}))
}

fn do_create(a: &Value, p: &Progress) -> Result<Value, ArcError> {
    let items: Vec<PathBuf> = strs(a, "items").into_iter().map(PathBuf::from).collect();
    let out = PathBuf::from(s(a, "out"));
    let format = Format::parse(&s(a, "format")).ok_or_else(|| ArcError::Failed("Pick a format".into()))?;
    if out.exists() && !a["replace"].as_bool().unwrap_or(false) {
        return Err(ArcError::Failed(format!("“{}” already exists", name_of(&out.to_string_lossy()))));
    }
    let o = CreateOpts {
        format,
        level: a["level"].as_u64().unwrap_or(5) as u32,
        method: s(a, "method"),
        password: pw(a),
        encrypt_names: a["encryptNames"].as_bool().unwrap_or(false),
        zip_encryption: s(a, "zipEncryption"),
        solid: a["solid"].as_bool().unwrap_or(true),
        threads: a["threads"].as_u64().unwrap_or(0) as u32,
        volume_size: a["volumeSize"].as_u64().unwrap_or(0),
    };
    // Written beside, then moved into place (no half archive if it fails).
    let files = arc::create(&out, &items, &o, p)?;
    crate::controller::push_history_any("Compress", grove_core::ops::Action::Exists { paths: files.clone() });
    let dir = out.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(json!({"files": files, "dir": dir}))
}

fn do_update(a: &Value, p: &Progress) -> Result<Value, ArcError> {
    let archive = PathBuf::from(s(a, "archive"));
    let rename = a["rename"].as_array().map(|r| r.iter().filter_map(|x| Some((x[0].as_str()?.to_string(), x[1].as_str()?.to_string()))).collect()).unwrap_or_default();
    let c = Changes {
        add: strs(a, "add").into_iter().map(PathBuf::from).collect(),
        add_into: s(a, "addInto"),
        delete: strs(a, "delete"),
        rename,
        mkdir: strs(a, "mkdir"),
        password: pw(a),
        level: a["level"].as_u64().unwrap_or(5) as u32,
    };
    arc::update(&archive, &c, p)?;
    Ok(json!({}))
}

/// A snapshot for the progress window.
pub fn progress(id: i32) -> Value {
    let j = jobs().lock().unwrap();
    let Some(job) = j.get(&id) else { return json!({"state": "gone"}) };
    let p = &job.p;
    let mut v = json!({
        "kind": job.kind, "title": job.title,
        "state": if p.paused.load(Ordering::Relaxed) { "paused" } else { "running" },
        "done": p.done.load(Ordering::Relaxed), "total": p.total.load(Ordering::Relaxed),
        "bytes": p.bytes.load(Ordering::Relaxed), "packed": p.packed.load(Ordering::Relaxed),
        "files": p.files.load(Ordering::Relaxed), "errors": p.errors.load(Ordering::Relaxed),
        "current": p.current.lock().unwrap().clone(),
        "elapsed": job.started.elapsed().as_secs_f64(),
    });
    if let Some(q) = p.question() {
        v["question"] = serde_json::from_str(&q).unwrap_or(Value::Null);
        v["state"] = json!("asking");
    }
    if let Some(end) = job.end.lock().unwrap().as_ref() {
        if let Value::Object(m) = end {
            for (k, x) in m {
                v[k] = x.clone();
            }
        }
    }
    v
}

pub fn answer(id: i32, a: &str) {
    if let Some(j) = jobs().lock().unwrap().get(&id) {
        j.p.reply(a);
    }
}
pub fn cancel(id: i32) {
    if let Some(j) = jobs().lock().unwrap().get(&id) {
        j.p.stop();
    }
}
pub fn pause(id: i32, on: bool) {
    if let Some(j) = jobs().lock().unwrap().get(&id) {
        j.p.paused.store(on, Ordering::Relaxed);
    }
}
pub fn forget(id: i32) {
    let mut j = jobs().lock().unwrap();
    if let Some(job) = j.get(&id) {
        if job.end.lock().unwrap().is_none() {
            job.p.stop();
        }
    }
    j.remove(&id);
}

/// One member taken out to grove's cache (to open, preview or drag it):
/// {path} or {error, code}.
pub fn member_json(archive: &str, inner: &str, password: Option<&str>) -> Value {
    let dir = grove_core::util::cache_home().join("grove/archiver").join(grove_core::thumbs::md5_hex(&format!("{archive}\0{inner}")));
    let out = dir.join(inner.rsplit('/').next().unwrap_or(inner));
    // Reused while newer than the archive.
    if let (Ok(m), Ok(a)) = (std::fs::metadata(&out), std::fs::metadata(archive)) {
        if m.modified().ok() >= a.modified().ok() {
            return json!({"path": out});
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    let parent = inner.rsplit_once('/').map(|(d, _)| format!("{d}/")).unwrap_or_default();
    let o = ExtractOpts { password: password.map(String::from), only: vec![inner.to_string()], strip: parent, overwrite: Overwrite::Replace, ..Default::default() };
    match arc::extract(Path::new(archive), &dir, &o, &Progress::new()) {
        Ok(_) if out.exists() => {
            // Newer than the archive even if the member's own date is old.
            if let Ok(f) = std::fs::File::options().write(true).open(&out) {
                let _ = f.set_modified(std::time::SystemTime::now());
            }
            json!({"path": out})
        }
        Ok(_) => json!({"error": "Couldn't take it out of the archive."}),
        Err(e) => err_json(&e),
    }
}
