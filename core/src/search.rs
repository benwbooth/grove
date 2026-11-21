//! Searching below a folder: by name (an fd-style parallel walk) and by
//! content (ripgrep's searcher), streaming results as they're found.
//! Saved searches are "Smart Folders" (~/.config/grove/smart-folders.json).

use crate::listing::Entry;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::sinks::UTF8;
use grep_searcher::{BinaryDetection, SearcherBuilder};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Query {
    /// Words that must all appear in the name (case-insensitive), or a
    /// glob when it has * or ?.
    #[serde(default)]
    pub name: String,
    /// Text to look for inside files (empty: names only).
    #[serde(default)]
    pub content: String,
    /// Treat `content` as a regular expression.
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub include_hidden: bool,
    /// Skip what .gitignore files say to skip.
    #[serde(default)]
    pub respect_ignore: bool,
    /// Only items with this tag.
    #[serde(default)]
    pub tag: String,
    /// "", "folder", "image", "video", "audio", "document", "archive".
    #[serde(default)]
    pub kind: String,
    /// Modified within this many days (0: any time).
    #[serde(default)]
    pub days: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SmartFolder {
    pub name: String,
    pub root: String,
    pub query: Query,
}

fn glob_to_regex(g: &str) -> String {
    let mut r = String::from("(?i)^");
    for c in g.chars() {
        match c {
            '*' => r.push_str(".*"),
            '?' => r.push('.'),
            c => r.push_str(&regex::escape(&c.to_string())),
        }
    }
    r.push('$');
    r
}

pub struct NameMatcher {
    words: Vec<String>,
    glob: Option<regex::Regex>,
}

impl NameMatcher {
    pub fn new(q: &str) -> NameMatcher {
        let q = q.trim();
        if q.contains('*') || q.contains('?') {
            NameMatcher { words: vec![], glob: regex::Regex::new(&glob_to_regex(q)).ok() }
        } else {
            NameMatcher { words: q.split_whitespace().map(|w| w.to_lowercase()).collect(), glob: None }
        }
    }
    pub fn matches(&self, name: &str) -> bool {
        if let Some(g) = &self.glob {
            return g.is_match(name);
        }
        if self.words.is_empty() {
            return true;
        }
        let lower = name.to_lowercase();
        self.words.iter().all(|w| lower.contains(w.as_str()))
    }
}

pub fn kind_matches(kind: &str, e: &Entry) -> bool {
    if kind.is_empty() {
        return true;
    }
    if kind == "folder" {
        return e.is_dir;
    }
    if e.is_dir {
        return false;
    }
    let ext = e.ext().to_ascii_lowercase();
    let list: &[&str] = match kind {
        "image" => &["png", "jpg", "jpeg", "gif", "webp", "avif", "heic", "heif", "bmp", "tif", "tiff", "svg", "jxl", "raw", "cr2", "nef", "arw", "dng", "ico"],
        "video" => &["mp4", "mkv", "webm", "mov", "avi", "m4v", "wmv", "flv", "mpg", "mpeg", "ts"],
        "audio" => &["mp3", "flac", "ogg", "opus", "wav", "m4a", "aac", "wma", "aiff", "mka"],
        "document" => &["pdf", "doc", "docx", "odt", "rtf", "txt", "md", "xls", "xlsx", "ods", "ppt", "pptx", "odp", "epub", "tex"],
        "archive" => &["zip", "tar", "gz", "xz", "zst", "bz2", "7z", "rar", "tgz", "iso"],
        _ => return true,
    };
    list.contains(&ext.as_str())
}

/// Search below `root`, sending batches of results to `out` (at most every
/// 80 ms). Returns how many items were found. Stops when `cancel` is set.
pub fn run(root: &Path, q: &Query, cancel: Arc<AtomicBool>, out: impl Fn(Vec<Entry>) + Send + Sync) -> usize {
    let name = Arc::new(NameMatcher::new(&q.name));
    let content = if q.content.trim().is_empty() {
        None
    } else {
        let pat = if q.regex { q.content.clone() } else { regex::escape(&q.content) };
        match RegexMatcherBuilder::new().case_smart(true).build(&pat) {
            Ok(m) => Some(m),
            Err(_) => return 0,
        }
    };
    let since = if q.days > 0 { crate::util::now_secs() - q.days as i64 * 86400 } else { i64::MIN };
    let found = AtomicUsize::new(0);
    let pending: Mutex<(Vec<Entry>, Instant)> = Mutex::new((Vec::new(), Instant::now()));
    let walker = ignore::WalkBuilder::new(root)
        .hidden(!q.include_hidden)
        .git_ignore(q.respect_ignore)
        .git_global(q.respect_ignore)
        .git_exclude(q.respect_ignore)
        .ignore(q.respect_ignore)
        .parents(q.respect_ignore)
        .follow_links(false)
        .threads(std::thread::available_parallelism().map(|n| n.get().min(8)).unwrap_or(4))
        .build_parallel();
    let flush = |force: bool| {
        let mut p = pending.lock().unwrap();
        if !p.0.is_empty() && (force || p.1.elapsed() >= Duration::from_millis(80)) {
            let batch = std::mem::take(&mut p.0);
            p.1 = Instant::now();
            drop(p);
            out(batch);
        }
    };
    walker.run(|| {
        let name = name.clone();
        let content = content.clone();
        let cancel = cancel.clone();
        let found = &found;
        let pending = &pending;
        let flush = &flush;
        let tag = q.tag.clone();
        let kind = q.kind.clone();
        let root = root.to_path_buf();
        let mut searcher = SearcherBuilder::new().binary_detection(BinaryDetection::quit(0)).line_number(true).build();
        Box::new(move |res| {
            if cancel.load(Ordering::Relaxed) {
                return ignore::WalkState::Quit;
            }
            let Ok(de) = res else { return ignore::WalkState::Continue };
            if de.depth() == 0 || de.path() == root {
                return ignore::WalkState::Continue;
            }
            let fname = de.file_name().to_string_lossy();
            if !name.matches(&fname) {
                return ignore::WalkState::Continue;
            }
            let Ok(mut e) = Entry::from_path(de.path()) else { return ignore::WalkState::Continue };
            if e.mtime < since || !kind_matches(&kind, &e) {
                return ignore::WalkState::Continue;
            }
            if !tag.is_empty() && !crate::tags::get(de.path()).iter().any(|t| t == &tag) {
                return ignore::WalkState::Continue;
            }
            if let Some(m) = &content {
                if e.is_dir {
                    return ignore::WalkState::Continue;
                }
                let mut hit: Option<String> = None;
                let _ = searcher.search_path(
                    m,
                    de.path(),
                    UTF8(|ln, line| {
                        hit = Some(format!("{ln}: {}", line.trim().chars().take(160).collect::<String>()));
                        Ok(false)
                    }),
                );
                match hit {
                    Some(h) => e.note = h,
                    None => return ignore::WalkState::Continue,
                }
            } else if let Ok(rel) = de.path().parent().unwrap_or(Path::new("")).strip_prefix(&root) {
                e.note = rel.to_string_lossy().into_owned();
            }
            found.fetch_add(1, Ordering::Relaxed);
            pending.lock().unwrap().0.push(e);
            flush(false);
            ignore::WalkState::Continue
        })
    });
    flush(true);
    found.load(Ordering::Relaxed)
}

fn smart_path() -> PathBuf {
    crate::util::grove_config().join("smart-folders.json")
}

pub fn smart_folders() -> Vec<SmartFolder> {
    std::fs::read_to_string(smart_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save_smart_folder(f: SmartFolder) -> Result<(), String> {
    let mut v = smart_folders();
    v.retain(|x| x.name != f.name);
    v.push(f);
    crate::util::write_atomic(&smart_path(), serde_json::to_string_pretty(&v).unwrap().as_bytes()).map_err(|e| e.to_string())
}

pub fn remove_smart_folder(name: &str) -> Result<(), String> {
    let mut v = smart_folders();
    v.retain(|x| x.name != name);
    crate::util::write_atomic(&smart_path(), serde_json::to_string_pretty(&v).unwrap().as_bytes()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("a/b")).unwrap();
        std::fs::create_dir_all(t.path().join(".secret")).unwrap();
        std::fs::write(t.path().join("a/Report 2024.txt"), "quarterly numbers\nTotal: 42\n").unwrap();
        std::fs::write(t.path().join("a/b/notes.md"), "nothing here\n").unwrap();
        std::fs::write(t.path().join(".secret/report.txt"), "Total: 7\n").unwrap();
        std::fs::write(t.path().join("bin.dat"), [0u8, 1, 2, 0, b'T', b'o', b't', b'a', b'l']).unwrap();
        t
    }

    fn collect(root: &Path, q: Query) -> Vec<Entry> {
        let v = Mutex::new(Vec::new());
        run(root, &q, Arc::new(AtomicBool::new(false)), |b| v.lock().unwrap().extend(b));
        let mut v = v.into_inner().unwrap();
        v.sort_by(|a, b| a.path.cmp(&b.path));
        v
    }

    #[test]
    fn by_name() {
        let t = tree();
        let r = collect(t.path(), Query { name: "report".into(), ..Default::default() });
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].name, "Report 2024.txt");
        assert_eq!(r[0].note, "a");
        let r = collect(t.path(), Query { name: "report".into(), include_hidden: true, ..Default::default() });
        assert_eq!(r.len(), 2);
        let r = collect(t.path(), Query { name: "*.md".into(), ..Default::default() });
        assert_eq!(r.len(), 1);
        let r = collect(t.path(), Query { name: "".into(), kind: "folder".into(), ..Default::default() });
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn by_content() {
        let t = tree();
        let r = collect(t.path(), Query { content: "total:".into(), ..Default::default() });
        assert_eq!(r.len(), 1, "{r:?}");
        assert_eq!(r[0].note, "2: Total: 42");
        let r = collect(t.path(), Query { content: r"Total: \d+".into(), regex: true, include_hidden: true, ..Default::default() });
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn cancel_stops() {
        let t = tree();
        let c = Arc::new(AtomicBool::new(true));
        assert_eq!(run(t.path(), &Query::default(), c, |_| {}), 0);
    }

    #[test]
    fn smart_folders_roundtrip() {
        let _g = crate::test_env_lock();
        let t = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CONFIG_HOME", t.path());
        save_smart_folder(SmartFolder { name: "PDFs".into(), root: "/home".into(), query: Query { name: "*.pdf".into(), ..Default::default() } }).unwrap();
        assert_eq!(smart_folders().len(), 1);
        remove_smart_folder("PDFs").unwrap();
        assert!(smart_folders().is_empty());
        std::env::remove_var("XDG_CONFIG_HOME");
    }
}
