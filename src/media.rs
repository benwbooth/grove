//! Picture dimensions and audio/video durations for the list columns, the
//! icon view's info line and tooltips. Read lazily on one worker thread
//! (picture headers with `imagesize`, media with ffprobe) the first time a
//! view asks, cached by path + mtime, and handed back to the model in
//! batches. The UI thread never waits on it.

use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::sync::OnceLock;

#[derive(Clone, Default)]
pub struct Media {
    pub dims: String,
    pub duration: String,
    pub seconds: f64,
    /// Archives: "12 items".
    pub items: String,
}

struct Req {
    path: String,
    mtime: i64,
    mime: String,
    model: u64,
    done: Box<dyn FnOnce(Vec<String>) + Send>,
}

fn cache() -> &'static Mutex<HashMap<String, (i64, Media)>> {
    static C: OnceLock<Mutex<HashMap<String, (i64, Media)>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

fn pending() -> &'static Mutex<HashSet<String>> {
    static P: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    P.get_or_init(Default::default)
}

/// Which files have something to read.
pub fn wants(mime: &str, path: &str) -> bool {
    path.starts_with('/') && (mime.starts_with("image/") || mime.starts_with("video/") || mime.starts_with("audio/") || is_archive(mime, path))
}

fn is_archive(mime: &str, path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or("");
    crate::archive::is_archive(mime, name)
}

pub fn cached(path: &str, mtime: i64) -> Option<Media> {
    cache().lock().get(path).filter(|(m, _)| *m == mtime).map(|(_, x)| x.clone())
}

/// Ask for a file's facts; `done` gets the paths that became ready (on the
/// worker thread, batched per model).
pub fn request(path: &str, mtime: i64, mime: &str, model: u64, done: impl FnOnce(Vec<String>) + Send + 'static) {
    if !pending().lock().insert(path.to_string()) {
        return;
    }
    static TX: OnceLock<Mutex<mpsc::Sender<Req>>> = OnceLock::new();
    let tx = TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Req>();
        std::thread::Builder::new().name("grove-media".into()).spawn(move || worker(rx)).unwrap();
        Mutex::new(tx)
    });
    let _ = tx.lock().send(Req { path: path.into(), mtime, mime: mime.into(), model, done: Box::new(done) });
}

fn worker(rx: mpsc::Receiver<Req>) {
    while let Ok(first) = rx.recv() {
        // Work through what's queued, then report per model.
        let mut batch: HashMap<u64, (Vec<String>, Option<Box<dyn FnOnce(Vec<String>) + Send>>)> = HashMap::new();
        let mut next = Some(first);
        let started = std::time::Instant::now();
        while let Some(r) = next {
            let m = read(&r.path, &r.mime);
            cache().lock().insert(r.path.clone(), (r.mtime, m));
            pending().lock().remove(&r.path);
            let e = batch.entry(r.model).or_insert_with(|| (Vec::new(), None));
            e.0.push(r.path);
            e.1 = Some(r.done);
            if started.elapsed().as_millis() > 150 {
                break;
            }
            next = rx.try_recv().ok();
        }
        for (_, (paths, done)) in batch {
            if let Some(d) = done {
                d(paths);
            }
        }
    }
}

pub fn read(path: &str, mime: &str) -> Media {
    let mut m = Media::default();
    if is_archive(mime, path) {
        // Compressed tarballs have no index: only count the smaller ones.
        let size = std::fs::metadata(path).map(|x| x.len()).unwrap_or(0);
        let indexed = mime == "application/zip" || mime == "application/x-7z-compressed" || mime == "application/java-archive" || path.to_lowercase().ends_with(".zip");
        if indexed || size < 256 * 1024 * 1024 {
            let names = grove_core::previews::archive_names(std::path::Path::new(path), &std::sync::atomic::AtomicBool::new(false));
            if !names.is_empty() {
                m.items = if names.len() == 1 { "1 item".into() } else { format!("{} items", names.len()) };
            }
        }
        return m;
    }
    if mime.starts_with("image/") {
        if let Ok(s) = imagesize::size(path) {
            m.dims = format!("{} × {}", s.width, s.height);
            return m;
        }
    }
    if mime.starts_with("video/") || mime.starts_with("audio/") {
        if let Some((w, h, secs)) = probe(path) {
            if w > 0 && h > 0 && !mime.starts_with("audio/") {
                m.dims = format!("{w} × {h}");
            }
            if secs > 0.0 {
                m.seconds = secs;
                m.duration = duration_text(secs);
            }
        }
    }
    m
}

/// (width, height, seconds), FFmpeg linked (no ffprobe).
pub fn probe(path: &str) -> Option<(u32, u32, f64)> {
    grove_core::av::probe(std::path::Path::new(path)).map(|p| (p.width, p.height, p.seconds))
}

pub fn duration_text(secs: f64) -> String {
    let t = secs.round() as u64;
    let (h, m, s) = (t / 3600, (t / 60) % 60, t % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn durations() {
        assert_eq!(duration_text(5.4), "0:05");
        assert_eq!(duration_text(125.0), "2:05");
        assert_eq!(duration_text(3725.0), "1:02:05");
    }
}
