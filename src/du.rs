//! Disk Usage window (KDirStat-like): scan sessions over grove_core::du,
//! the tree's visible rows, file-type totals, and the cushion treemap
//! (rendered here, handed to the C++ image provider as RGBA pixels).

use grove_core::du::{Node, Progress, Tree};
use grove_core::util::human_size;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering as AO};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

struct Scan {
    root: PathBuf,
    prog: Arc<Progress>,
    tree: Option<Arc<RwLock<Tree>>>,
    error: String,
    started: std::time::Instant,
    secs: f64,
    /// The last treemap drawn: rectangles (x0, y0, x1, y1, node) in draw order.
    layout: Vec<(f32, f32, f32, f32, u32)>,
}

fn scans() -> &'static Mutex<HashMap<i32, Scan>> {
    static S: OnceLock<Mutex<HashMap<i32, Scan>>> = OnceLock::new();
    S.get_or_init(Default::default)
}

static NEXT: AtomicI32 = AtomicI32::new(1);

pub fn start(path: &str) -> i32 {
    let id = NEXT.fetch_add(1, AO::Relaxed);
    let prog = Arc::new(Progress::default());
    let root = PathBuf::from(path);
    scans().lock().unwrap().insert(id, Scan { root: root.clone(), prog: prog.clone(), tree: None, error: String::new(), started: std::time::Instant::now(), secs: 0.0, layout: vec![] });
    std::thread::spawn(move || {
        let r = grove_core::du::scan(&root, &prog);
        prog.done.store(true, AO::SeqCst);
        if let Some(s) = scans().lock().unwrap().get_mut(&id) {
            s.secs = s.started.elapsed().as_secs_f64();
            match r {
                Ok(t) => s.tree = Some(Arc::new(RwLock::new(t))),
                Err(e) => s.error = e,
            }
        }
    });
    id
}

pub fn cancel(id: i32) {
    if let Some(s) = scans().lock().unwrap().get(&id) {
        s.prog.cancel.store(true, AO::SeqCst);
    }
}

pub fn forget(id: i32) {
    if let Some(s) = scans().lock().unwrap().remove(&id) {
        s.prog.cancel.store(true, AO::SeqCst);
    }
}

fn tree(id: i32) -> Option<Arc<RwLock<Tree>>> {
    scans().lock().unwrap().get(&id).and_then(|s| s.tree.clone())
}

pub fn progress(id: i32) -> Value {
    let g = scans().lock().unwrap();
    let Some(s) = g.get(&id) else { return json!({"error": "No such scan."}) };
    let p = &s.prog;
    let bytes = p.bytes.load(AO::Relaxed);
    json!({
        "root": s.root.display().to_string(),
        "ready": s.tree.is_some(),
        "error": s.error,
        "cancelled": p.cancel.load(AO::Relaxed),
        "files": p.files.load(AO::Relaxed),
        "dirs": p.dirs.load(AO::Relaxed),
        "unreadable": p.errors.load(AO::Relaxed),
        "bytes": bytes, "bytesText": human_size(bytes),
        "current": p.current.lock().unwrap().clone(),
        "secs": if s.tree.is_some() { s.secs } else { s.started.elapsed().as_secs_f64() },
    })
}

fn size_of(n: &Node, apparent: bool) -> u64 {
    if apparent { n.apparent } else { n.disk }
}

fn ext_of(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, e)) if !stem.is_empty() && e.len() <= 8 => e.to_lowercase(),
        _ => String::new(),
    }
}

/// File categories for the treemap's colours (KDirStat colours by type).
pub const CATS: &[(&str, [u8; 3])] = &[
    ("Pictures", [0x4f, 0x9c, 0xf0]),
    ("Videos", [0xe0, 0x4f, 0x5f]),
    ("Music", [0xb0, 0x6c, 0xe8]),
    ("Archives & disk images", [0xf0, 0xa0, 0x30]),
    ("Documents", [0x3c, 0xc0, 0x8c]),
    ("Code & text", [0xe8, 0xd8, 0x48]),
    ("Apps & libraries", [0xf0, 0x6c, 0xc0]),
    ("Other files", [0x9a, 0xa4, 0xb0]),
];

fn category(name: &str) -> usize {
    let e = ext_of(name);
    let e = e.as_str();
    const PIC: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "avif", "tif", "tiff", "bmp", "svg", "ico", "raw", "cr2", "cr3", "nef", "arw", "dng", "orf", "rw2", "psd", "xcf", "kra", "exr", "jxl"];
    const VID: &[&str] = &["mp4", "mkv", "mov", "avi", "webm", "m4v", "wmv", "flv", "mpg", "mpeg", "ts", "m2ts", "3gp", "vob", "ogv"];
    const AUD: &[&str] = &["mp3", "flac", "ogg", "opus", "m4a", "aac", "wav", "wma", "aiff", "ape", "alac", "mid", "midi", "spc", "nsf", "vgm", "vgz"];
    const ARC: &[&str] = &["zip", "rar", "7z", "tar", "gz", "xz", "bz2", "zst", "lz", "lz4", "lzma", "tgz", "txz", "iso", "img", "dmg", "qcow2", "vdi", "vmdk", "vhd", "vhdx", "deb", "rpm", "cab", "squashfs", "nsp", "xci", "sfc", "smc", "nds", "3ds", "cia", "gba", "n64", "z64", "wbfs", "rvz", "chd", "cue", "bin"];
    const DOC: &[&str] = &["pdf", "doc", "docx", "odt", "rtf", "xls", "xlsx", "ods", "ppt", "pptx", "odp", "epub", "mobi", "azw3", "djvu", "cbz", "cbr", "pages", "numbers", "key"];
    const CODE: &[&str] = &["txt", "md", "rs", "c", "h", "cpp", "hpp", "cc", "py", "js", "ts", "tsx", "jsx", "java", "kt", "go", "rb", "php", "cs", "swift", "sh", "qml", "html", "css", "json", "toml", "yaml", "yml", "xml", "nix", "lua", "log", "csv", "sql", "ini", "conf", "asm", "s", "rlib", "rmeta", "d", "o"];
    const APP: &[&str] = &["so", "a", "dll", "exe", "msi", "appimage", "apk", "jar", "pyc", "wasm", "node", "dylib", "flatpak", "snap", "elf", "sys", "ko"];
    let in_ = |l: &[&str]| l.contains(&e);
    if in_(PIC) { 0 } else if in_(VID) { 1 } else if in_(AUD) { 2 } else if in_(ARC) { 3 } else if in_(DOC) { 4 } else if in_(CODE) { 5 } else if in_(APP) { 6 } else if name.contains(".so.") { 6 } else { 7 }
}

/// Sorted children of a node: biggest first (or by name / items / date).
fn sorted(t: &Tree, id: u32, apparent: bool, sort: &str) -> Vec<u32> {
    let mut v: Vec<u32> = t.children(id).map(|(i, _)| i).collect();
    let n = |i: &u32| &t.nodes[*i as usize];
    match sort {
        "name" => v.sort_by(|a, b| n(a).name.to_lowercase().cmp(&n(b).name.to_lowercase())),
        "items" => v.sort_by(|a, b| n(b).items.cmp(&n(a).items)),
        "date" => v.sort_by(|a, b| n(b).mtime.cmp(&n(a).mtime)),
        _ => v.sort_by(|a, b| size_of(n(b), apparent).cmp(&size_of(n(a), apparent))),
    }
    v
}

fn subtree_mtime(t: &Tree, id: u32, cache: &mut HashMap<u32, i64>) -> i64 {
    if let Some(m) = cache.get(&id) {
        return *m;
    }
    let mut m = t.nodes[id as usize].mtime;
    let kids: Vec<u32> = t.children(id).map(|(i, _)| i).collect();
    for k in kids {
        m = m.max(subtree_mtime(t, k, cache));
    }
    cache.insert(id, m);
    m
}

/// The tree's visible rows: the root, then every expanded folder's
/// children (at most `per_dir` each; the rest summed in one row).
pub fn rows(id: i32, expanded: &str, apparent: bool, sort: &str) -> Value {
    let Some(t) = tree(id) else { return json!({"rows": []}) };
    let t = t.read().unwrap();
    let open: HashSet<u32> = serde_json::from_str::<Vec<u32>>(expanded).unwrap_or_default().into_iter().collect();
    let mut out = Vec::new();
    let mut mcache = HashMap::new();
    const PER_DIR: usize = 400;
    fn row(t: &Tree, i: u32, depth: u32, parent_size: u64, apparent: bool, open: bool, mcache: &mut HashMap<u32, i64>) -> Value {
        let n = &t.nodes[i as usize];
        let s = size_of(n, apparent);
        let kids = n.is_dir && t.children(i).next().is_some();
        json!({
            "node": i, "depth": depth, "name": &*n.name, "isDir": n.is_dir,
            "size": s, "sizeText": human_size(s),
            "pct": if parent_size > 0 { s as f64 / parent_size as f64 } else { 1.0 },
            "items": if n.is_dir { n.items } else { 0 },
            "mtime": if n.is_dir { subtree_mtime(t, i, mcache) } else { n.mtime },
            "hasKids": kids, "expanded": open && kids, "error": n.error,
            "cat": if n.is_dir { -1 } else { category(&n.name) as i64 },
        })
    }
    fn walk(t: &Tree, i: u32, depth: u32, apparent: bool, sort: &str, open: &HashSet<u32>, out: &mut Vec<Value>, mcache: &mut HashMap<u32, i64>) {
        let ps = size_of(&t.nodes[i as usize], apparent);
        let kids = sorted(t, i, apparent, sort);
        let shown = kids.len().min(PER_DIR);
        for &k in &kids[..shown] {
            let o = open.contains(&k);
            out.push(row(t, k, depth, ps, apparent, o, mcache));
            if o {
                walk(t, k, depth + 1, apparent, sort, open, out, mcache);
            }
        }
        if kids.len() > shown {
            let rest: u64 = kids[shown..].iter().map(|k| size_of(&t.nodes[*k as usize], apparent)).sum();
            out.push(json!({"node": -1, "depth": depth, "name": format!("{} more small items", kids.len() - shown), "isDir": false,
                "size": rest, "sizeText": human_size(rest), "pct": if ps > 0 { rest as f64 / ps as f64 } else { 0.0 },
                "items": 0, "mtime": 0, "hasKids": false, "expanded": false, "error": false, "cat": -2}));
        }
    }
    out.push(row(&t, 0, 0, size_of(&t.nodes[0], apparent), apparent, open.contains(&0), &mut mcache));
    if open.contains(&0) {
        walk(&t, 0, 1, apparent, sort, &open, &mut out, &mut mcache);
    }
    json!({"rows": out})
}

/// Totals per file category and the biggest extensions under a node.
pub fn types(id: i32, node: u32, apparent: bool) -> Value {
    let Some(t) = tree(id) else { return json!({"cats": [], "exts": []}) };
    let t = t.read().unwrap();
    let mut cats = vec![(0u64, 0u64); CATS.len()];
    let mut exts: HashMap<String, (u64, u64, usize)> = HashMap::new();
    let mut stack = vec![node];
    while let Some(i) = stack.pop() {
        for (k, n) in t.children(i) {
            if n.is_dir {
                stack.push(k);
            } else {
                let c = category(&n.name);
                let s = size_of(n, apparent);
                cats[c].0 += s;
                cats[c].1 += 1;
                let e = exts.entry(ext_of(&n.name)).or_insert((0, 0, c));
                e.0 += s;
                e.1 += 1;
            }
        }
    }
    let mut ev: Vec<_> = exts.into_iter().collect();
    ev.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
    json!({
        "cats": CATS.iter().enumerate().filter(|(i, _)| cats[*i].1 > 0).map(|(i, (name, rgb))| json!({
            "cat": i, "name": name, "color": format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]),
            "size": cats[i].0, "sizeText": human_size(cats[i].0), "files": cats[i].1})).collect::<Vec<_>>(),
        "exts": ev.into_iter().take(40).map(|(e, (s, n, c))| {
            let rgb = CATS[c].1;
            let color = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
            let ext = if e.is_empty() { "(none)".to_string() } else { format!(".{e}") };
            json!({"ext": ext, "size": s, "sizeText": human_size(s), "files": n, "color": color})
        }).collect::<Vec<_>>(),
    })
}

pub fn path_of(id: i32, node: u32) -> String {
    tree(id).map(|t| t.read().unwrap().path_of(node).display().to_string()).unwrap_or_default()
}

/// The chain of folders from the root down to `node` (to expand them).
pub fn ancestors(id: i32, node: u32) -> Value {
    let Some(t) = tree(id) else { return json!([]) };
    let t = t.read().unwrap();
    let mut v = Vec::new();
    let mut i = node;
    while (i as usize) < t.nodes.len() && i != u32::MAX {
        v.push(i);
        i = t.nodes[i as usize].parent;
    }
    v.reverse();
    json!(v)
}

pub fn parent_of(id: i32, node: u32) -> i64 {
    tree(id).map(|t| { let p = t.read().unwrap().nodes.get(node as usize).map(|n| n.parent).unwrap_or(u32::MAX); if p == u32::MAX { -1 } else { p as i64 } }).unwrap_or(-1)
}

pub fn find(id: i32, path: &str) -> i64 {
    tree(id).and_then(|t| t.read().unwrap().find(Path::new(path))).map(|n| n as i64).unwrap_or(-1)
}

pub fn removed(id: i32, node: u32) {
    if let Some(t) = tree(id) {
        t.write().unwrap().remove(node);
    }
}

/// The deepest treemap rectangle under a point (last drawn wins).
pub fn hit(id: i32, x: f64, y: f64) -> i64 {
    let g = scans().lock().unwrap();
    let Some(s) = g.get(&id) else { return -1 };
    let (x, y) = (x as f32, y as f32);
    s.layout.iter().rev().find(|r| x >= r.0 && x < r.2 && y >= r.1 && y < r.3).map(|r| r.4 as i64).unwrap_or(-1)
}

/// Where a node sits in the last treemap: [x, y, w, h], or [] if too small.
pub fn rect(id: i32, node: u32) -> Value {
    let g = scans().lock().unwrap();
    let Some(s) = g.get(&id) else { return json!([]) };
    match s.layout.iter().find(|r| r.4 == node) {
        Some(r) => json!([r.0, r.1, r.2 - r.0, r.3 - r.1]),
        None => json!([]),
    }
}

// ---------------------------------------------------------------- treemap

/// One cushion's surface: z = s2·x² + s1·x per axis (van Wijk & van de Wetering).
#[derive(Clone, Copy, Default)]
struct Surface {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

fn ridge(s: &mut Surface, h: f32, x0: f32, y0: f32, x1: f32, y1: f32) {
    if x1 - x0 > 0.0 {
        s.x1 += 4.0 * h * (x1 + x0) / (x1 - x0);
        s.x2 -= 4.0 * h / (x1 - x0);
    }
    if y1 - y0 > 0.0 {
        s.y1 += 4.0 * h * (y1 + y0) / (y1 - y0);
        s.y2 -= 4.0 * h / (y1 - y0);
    }
}

struct Painter<'a> {
    t: &'a Tree,
    apparent: bool,
    w: usize,
    h: usize,
    px: Vec<u8>,
    layout: Vec<(f32, f32, f32, f32, u32)>,
}

impl Painter<'_> {
    fn shade(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, s: &Surface, rgb: [u8; 3]) {
        let (ix0, iy0) = (x0.round().max(0.0) as usize, y0.round().max(0.0) as usize);
        let (ix1, iy1) = ((x1.round() as usize).min(self.w), (y1.round() as usize).min(self.h));
        // Light from the top left, like KDirStat.
        let (lx, ly, lz) = (-0.09759f32, -0.19518f32, 0.97590f32);
        let (ia, is) = (0.3f32, 0.75f32);
        for y in iy0..iy1 {
            let fy = y as f32 + 0.5;
            let ny = -(2.0 * s.y2 * fy + s.y1);
            let row = y * self.w * 4;
            for x in ix0..ix1 {
                let fx = x as f32 + 0.5;
                let nx = -(2.0 * s.x2 * fx + s.x1);
                let cos = (nx * lx + ny * ly + lz) / (nx * nx + ny * ny + 1.0).sqrt();
                let i = (ia + is * cos.max(0.0)).min(1.0);
                let o = row + x * 4;
                self.px[o] = (rgb[0] as f32 * i) as u8;
                self.px[o + 1] = (rgb[1] as f32 * i) as u8;
                self.px[o + 2] = (rgb[2] as f32 * i) as u8;
                self.px[o + 3] = 255;
            }
        }
    }

    fn node(&mut self, id: u32, x0: f32, y0: f32, x1: f32, y1: f32, mut s: Surface, h: f32) {
        let n = &self.t.nodes[id as usize];
        self.layout.push((x0, y0, x1, y1, id));
        ridge(&mut s, h, x0, y0, x1, y1);
        let kids: Vec<(u32, u64)> = if n.is_dir {
            let mut v: Vec<(u32, u64)> = self.t.children(id).map(|(i, c)| (i, size_of(c, self.apparent))).filter(|c| c.1 > 0).collect();
            v.sort_by(|a, b| b.1.cmp(&a.1));
            v
        } else {
            vec![]
        };
        let small = (x1 - x0) < 3.0 || (y1 - y0) < 3.0;
        if kids.is_empty() || small {
            let rgb = if n.is_dir { [0x80, 0x86, 0x90] } else { CATS[category(&n.name)].1 };
            self.shade(x0, y0, x1, y1, &s, rgb);
            return;
        }
        let total: u64 = kids.iter().map(|k| k.1).sum::<u64>().max(1);
        self.squarify(&kids, total, x0, y0, x1, y1, s, h * 0.72);
    }

    /// Squarified treemap: fill rows along the short side while the worst
    /// aspect ratio keeps improving.
    #[allow(clippy::too_many_arguments)]
    fn squarify(&mut self, kids: &[(u32, u64)], total: u64, mut x0: f32, mut y0: f32, x1: f32, y1: f32, s: Surface, h: f32) {
        let mut i = 0;
        let mut remaining = total as f64;
        while i < kids.len() {
            let (w, hh) = ((x1 - x0) as f64, (y1 - y0) as f64);
            if w < 1.0 || hh < 1.0 || remaining <= 0.0 {
                break;
            }
            let side = w.min(hh);
            let area = w * hh;
            let scale = area / remaining;
            let worst = |sum: f64, mx: f64, mn: f64| {
                let s2 = side * side;
                let a = sum * scale;
                ((s2 * mx * scale) / (a * a)).max((a * a) / (s2 * mn * scale))
            };
            let mut j = i;
            let mut sum = 0.0;
            let mut best = f64::MAX;
            while j < kids.len() {
                let v = kids[j].1 as f64;
                let ns = sum + v;
                let wr = worst(ns, kids[i].1 as f64, v);
                if wr > best && j > i {
                    break;
                }
                best = wr;
                sum = ns;
                j += 1;
            }
            // Lay kids[i..j] out in a strip of thickness sum*scale/side.
            let thick = (sum * scale / side) as f32;
            let mut pos = 0f32;
            for k in &kids[i..j] {
                let len = (k.1 as f64 / sum * side) as f32;
                if w >= hh {
                    // strip on the left, children stacked vertically
                    self.node(k.0, x0, y0 + pos, x0 + thick, y0 + pos + len, s, h);
                } else {
                    self.node(k.0, x0 + pos, y0, x0 + pos + len, y0 + thick, s, h);
                }
                pos += len;
            }
            if w >= hh { x0 += thick } else { y0 += thick }
            remaining -= sum;
            i = j;
        }
    }
}

/// RGBA pixels of the treemap of `node` at w×h (empty if not scanned).
pub fn treemap(id: i32, node: u32, w: u32, h: u32, apparent: bool) -> Vec<u8> {
    let (w, h) = (w.clamp(1, 8192) as usize, h.clamp(1, 8192) as usize);
    let Some(t) = tree(id) else { return vec![] };
    let t = t.read().unwrap();
    if node as usize >= t.nodes.len() {
        return vec![];
    }
    let mut p = Painter { t: &t, apparent, w, h, px: vec![0; w * h * 4], layout: Vec::new() };
    p.node(node, 0.0, 0.0, w as f32, h as f32, Surface::default(), 0.38);
    let layout = std::mem::take(&mut p.layout);
    let px = std::mem::take(&mut p.px);
    drop(t);
    if let Some(s) = scans().lock().unwrap().get_mut(&id) {
        s.layout = layout;
    }
    px
}
