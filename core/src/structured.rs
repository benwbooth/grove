//! Structured documents for the viewer's tree view: JSON and its relatives
//! (JSONC, JSON5, JSON Lines), YAML, TOML, XML and INI-style files, parsed
//! (on a worker) into one arena of small fixed-size nodes. JSON and XML keep
//! their strings as spans of the file's own text, so a 50 MB JSON costs the
//! file plus ~40 bytes a value; the Qt model walks the arena lazily, only
//! the rows that are open.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

pub const NONE: u32 = u32::MAX;
/// The most values one tree holds (more says so instead of using GBs).
pub const MAX_NODES: usize = 40_000_000;
/// The biggest file read as a tree, per format (YAML's parser is slow and
/// allocates per scalar).
pub fn max_bytes(f: Format) -> u64 {
    match f {
        Format::Json | Format::Xml => 1 << 30,
        Format::Yaml => 96 << 20,
        Format::Toml | Format::Ini => 128 << 20,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Json,
    Yaml,
    Toml,
    Xml,
    Ini,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Yaml => "yaml",
            Format::Toml => "toml",
            Format::Xml => "xml",
            Format::Ini => "ini",
        }
    }
    pub fn from_name(s: &str) -> Option<Format> {
        Some(match s.to_ascii_lowercase().as_str() {
            "json" => Format::Json,
            "yaml" => Format::Yaml,
            "toml" => Format::Toml,
            "xml" => Format::Xml,
            "ini" => Format::Ini,
            _ => return None,
        })
    }
    /// How people call it ("JSON", "YAML"...).
    pub fn title(self) -> &'static str {
        match self {
            Format::Json => "JSON",
            Format::Yaml => "YAML",
            Format::Toml => "TOML",
            Format::Xml => "XML",
            Format::Ini => "INI",
        }
    }
}

/// The format a file is in, by its name ("" when it isn't one of these).
pub fn format_for(name: &str) -> Option<Format> {
    let base = name.rsplit('/').next().unwrap_or(name);
    let lower = base.to_ascii_lowercase();
    match lower.as_str() {
        "flake.lock" | "composer.lock" | "pipfile.lock" | "deno.lock" | "bun.lock" | ".babelrc" | ".jshintrc" | ".swcrc" | ".jscsrc" | ".bowerrc" | ".watchmanconfig" | ".luaurc" => {
            return Some(Format::Json)
        }
        "cargo.lock" | "pipfile" | "poetry.lock" | "uv.lock" | "pdm.lock" | "gopkg.lock" => return Some(Format::Toml),
        ".clang-format" | ".clang-tidy" | ".yamllint" | ".prettierrc" | ".eslintrc" | ".stylelintrc" | ".markdownlint" | ".gemrc" => return Some(Format::Yaml),
        ".editorconfig" | ".gitconfig" | ".gitmodules" | ".pylintrc" | "pylintrc" | ".coveragerc" | "mimeapps.list" | "mimeinfo.cache" | ".npmrc" | ".flake8" => {
            return Some(Format::Ini)
        }
        _ => {}
    }
    let ext = match lower.rfind('.') {
        Some(i) if i > 0 => &lower[i + 1..],
        _ => return None,
    };
    Some(match ext {
        "json" | "jsonc" | "json5" | "jsonl" | "ndjson" | "jsonlines" | "geojson" | "topojson" | "ipynb" | "har" | "webmanifest" | "jsonld" | "avsc" | "gltf" | "mcmeta" | "code-workspace"
        | "code-snippets" | "sublime-settings" | "sublime-project" | "sublime-keymap" | "sublime-commands" | "sublime-menu" | "sublime-build" | "babelrc" | "arcconfig" | "tfstate" | "jsonnet-lock" => Format::Json,
        "yaml" | "yml" | "eyaml" | "sublime-syntax" | "cff" => Format::Yaml,
        "toml" => Format::Toml,
        "xml" | "xsd" | "xsl" | "xslt" | "plist" | "rss" | "atom" | "opml" | "kml" | "gpx" | "xaml" | "csproj" | "vbproj" | "fsproj" | "vcxproj" | "props" | "targets" | "nuspec" | "resx" | "xib"
        | "storyboard" | "wsdl" | "svg" | "fodt" | "fods" | "fodp" | "fodg" | "xliff" | "xlf" | "tmx" | "ui" | "qrc" | "glade" | "ttml" | "dae" | "x3d" | "mml" | "rdf" | "owl" | "xspf" | "iml"
        | "urdf" | "xul" | "jnlp" | "pom" | "smil" | "musicxml" | "mxl" | "xmp" | "graphml" | "gexf" | "nzb" | "osm" | "tcx" | "xbel" | "kdenlive" | "mlt" | "drawio" | "dita" | "ditamap" | "docbook"
        | "manifest" | "appxmanifest" | "config" => Format::Xml,
        "ini" | "desktop" | "directory" | "service" | "socket" | "timer" | "mount" | "automount" | "target" | "path" | "slice" | "scope" | "network" | "netdev" | "link" | "swap" | "inf"
        | "url" | "flatpakref" | "flatpakrepo" | "theme" | "nmconnection" | "container" | "volume" | "pod" | "kube" | "lnk-ini" => Format::Ini,
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Kind {
    Object,
    Array,
    Str,
    Num,
    Bool,
    Null,
    Date,
    Element,
    Attr,
    Text,
    Comment,
    Alias,
}

impl Kind {
    pub fn is_container(self) -> bool {
        matches!(self, Kind::Object | Kind::Array | Kind::Element)
    }
}

// Node flags: where the key and value live.
const K_SRC: u8 = 1; // key is a span of the source
const K_IDX: u8 = 2; // key is an index (key.0)
const K_RAW: u8 = 4; // key is a quoted JSON string (unescape it)
const V_SRC: u8 = 8; // value is a span of the source
const V_RAW: u8 = 16; // value is a quoted JSON string
const V_ALIAS: u8 = 32; // value is a node (val.0): a YAML alias

#[derive(Clone, Copy)]
struct Node {
    kind: Kind,
    flags: u8,
    parent: u32,
    first: u32,
    next: u32,
    count: u32,
    key: (u32, u32),
    val: (u32, u32),
}

pub struct Tree {
    pub format: Format,
    src: String,
    pool: String,
    nodes: Vec<Node>,
    root: u32,
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    /// 1-based; 0 when there's no place.
    pub line: usize,
    pub col: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.line > 0 {
            write!(f, "{} (line {}, column {})", self.message, self.line, self.col)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

fn err_at(src: &str, offset: usize, message: impl Into<String>) -> ParseError {
    let offset = offset.min(src.len());
    let mut o = offset;
    while o > 0 && !src.is_char_boundary(o) {
        o -= 1;
    }
    let before = &src[..o];
    let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let col = before[line_start..].chars().count() + 1;
    ParseError { message: message.into(), line, col }
}

fn too_many() -> String {
    if BUDGET_HIT.with(|h| h.get()) {
        let gb = BUDGET.with(|b| b.get()) as f64 / (1u64 << 30) as f64;
        return format!("It's too big to show as a tree here (it would take more than {gb:.1} GB of memory); the Source view shows all of it");
    }
    format!("It has more than {} million values, too many to show as a tree; the Source view shows all of it", MAX_NODES / 1_000_000)
}

thread_local! {
    /// The most memory one tree may take (set by parse_with from the RAM there is).
    static BUDGET: std::cell::Cell<usize> = const { std::cell::Cell::new(usize::MAX) };
    static BUDGET_HIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Memory a tree may use: half of what's available now, between 512 MB and 6 GB.
pub fn memory_budget() -> usize {
    let avail = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|m| m.lines().find(|l| l.starts_with("MemAvailable:")).and_then(|l| l.split_whitespace().nth(1)?.parse::<usize>().ok()))
        .map(|kb| kb * 1024)
        .unwrap_or(4 << 30);
    (avail / 2).clamp(512 << 20, 6 << 30)
}

/// Builds the arena: children linked first/next, counted.
struct B {
    nodes: Vec<Node>,
    last: Vec<u32>,
    pool: String,
}

impl B {
    fn new() -> B {
        B { nodes: Vec::new(), last: Vec::new(), pool: String::new() }
    }
    fn add(&mut self, parent: u32, kind: Kind, mut flags: u8, mut key: (u32, u32), val: (u32, u32)) -> u32 {
        let id = self.nodes.len() as u32;
        if flags & K_IDX != 0 {
            key = (if parent == NONE { 0 } else { self.nodes[parent as usize].count }, 0);
        }
        if parent == NONE {
            flags &= !K_IDX;
        }
        self.nodes.push(Node { kind, flags, parent, first: NONE, next: NONE, count: 0, key, val });
        self.last.push(NONE);
        if parent != NONE {
            let p = parent as usize;
            let l = self.last[p];
            if l == NONE {
                self.nodes[p].first = id;
            } else {
                self.nodes[l as usize].next = id;
            }
            self.last[p] = id;
            self.nodes[p].count += 1;
        }
        id
    }
    fn s(&mut self, s: &str) -> (u32, u32) {
        let o = self.pool.len() as u32;
        self.pool.push_str(s);
        (o, s.len() as u32)
    }
    fn full(&self) -> bool {
        if self.nodes.len() >= MAX_NODES {
            return true;
        }
        // Every 64k values: still within the memory budget?
        if self.nodes.len() & 0xFFFF == 0 && !self.nodes.is_empty() {
            let used = self.nodes.capacity() * std::mem::size_of::<Node>() + self.last.capacity() * 4 + self.pool.capacity() + SRC_LEN.with(|l| l.get());
            if used > BUDGET.with(|b| b.get()) {
                BUDGET_HIT.with(|h| h.set(true));
                return true;
            }
        }
        false
    }
    /// The finished tree; a synthetic root with one child is dropped.
    fn finish(mut self, format: Format, src: String, root: u32) -> Tree {
        let mut root = root;
        if root != NONE && self.nodes[root as usize].count == 1 && self.nodes[root as usize].kind == Kind::Array {
            let c = self.nodes[root as usize].first;
            self.nodes[c as usize].parent = NONE;
            self.nodes[c as usize].flags &= !K_IDX;
            root = c;
        }
        self.pool.shrink_to_fit();
        self.nodes.shrink_to_fit();
        Tree { format, src, pool: self.pool, nodes: self.nodes, root }
    }
}

thread_local! {
    static SRC_LEN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Parse `bytes` as `format`. `cancel` stops a long parse early.
pub fn parse(bytes: Vec<u8>, format: Format, cancel: &AtomicBool) -> Result<Tree, ParseError> {
    parse_with(bytes, format, cancel, &std::sync::atomic::AtomicU32::new(0), usize::MAX)
}

/// `progress` gets how far it is, in thousandths (JSON and XML); a tree
/// bigger than `budget` bytes stops with a message.
pub fn parse_with(bytes: Vec<u8>, format: Format, cancel: &AtomicBool, progress: &std::sync::atomic::AtomicU32, budget: usize) -> Result<Tree, ParseError> {
    BUDGET.with(|b| b.set(budget));
    BUDGET_HIT.with(|h| h.set(false));
    SRC_LEN.with(|l| l.set(bytes.len()));
    PROGRESS.with(|p| p.set(progress as *const _));
    let r = parse_inner(bytes, format, cancel);
    PROGRESS.with(|p| p.set(std::ptr::null()));
    r
}

thread_local! {
    static PROGRESS: std::cell::Cell<*const std::sync::atomic::AtomicU32> = const { std::cell::Cell::new(std::ptr::null()) };
}

fn report(done: usize, total: usize) {
    PROGRESS.with(|p| {
        let p = p.get();
        if !p.is_null() && total > 0 {
            // SAFETY: set by parse_with for the length of the parse.
            unsafe { (*p).store((done as u64 * 1000 / total as u64) as u32, Ordering::Relaxed) };
        }
    });
}

fn parse_inner(bytes: Vec<u8>, format: Format, cancel: &AtomicBool) -> Result<Tree, ParseError> {
    let mut src = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => {
            let b = e.into_bytes();
            if b.starts_with(b"bplist") {
                return Err(ParseError { message: "It's a binary property list, not text".into(), line: 0, col: 0 });
            }
            String::from_utf8_lossy(&b).into_owned()
        }
    };
    if src.starts_with('\u{feff}') {
        src.replace_range(..3, "");
    }
    match format {
        Format::Json => parse_json(src, cancel),
        Format::Yaml => parse_yaml(src),
        Format::Toml => parse_toml(src),
        Format::Xml => parse_xml(src, cancel),
        Format::Ini => parse_ini(src),
    }
}

// ---------- JSON (lenient: comments, trailing commas, single quotes,
// unquoted keys, NaN/Infinity, hex; several values = JSON Lines) ----------

fn parse_json(src: String, cancel: &AtomicBool) -> Result<Tree, ParseError> {
    let s = src.as_bytes();
    let n = s.len();
    let mut b = B::new();
    b.nodes.reserve((n / 16).min(1 << 22));
    b.last.reserve((n / 16).min(1 << 22));
    // A synthetic array holds the top-level values (one, usually).
    let top = b.add(NONE, Kind::Array, 0, (0, 0), (0, 0));
    struct F {
        node: u32,
        obj: bool,
        sep: bool,
    }
    let mut st: Vec<F> = vec![F { node: top, obj: false, sep: false }];
    let mut pos = 0usize;
    let mut steps = 0u32;

    // Skip spaces and comments.
    let skip = |pos: &mut usize| -> Result<(), usize> {
        loop {
            while *pos < n && matches!(s[*pos], b' ' | b'\t' | b'\n' | b'\r') {
                *pos += 1;
            }
            if *pos + 1 < n && s[*pos] == b'/' && s[*pos + 1] == b'/' {
                while *pos < n && s[*pos] != b'\n' {
                    *pos += 1;
                }
                continue;
            }
            if *pos + 1 < n && s[*pos] == b'/' && s[*pos + 1] == b'*' {
                let start = *pos;
                *pos += 2;
                loop {
                    if *pos + 1 >= n {
                        return Err(start);
                    }
                    if s[*pos] == b'*' && s[*pos + 1] == b'/' {
                        *pos += 2;
                        break;
                    }
                    *pos += 1;
                }
                continue;
            }
            // A UTF-8 no-break space or BOM in the middle: skip too.
            if *pos + 2 < n && s[*pos] == 0xEF && s[*pos + 1] == 0xBB && s[*pos + 2] == 0xBF {
                *pos += 3;
                continue;
            }
            return Ok(());
        }
    };
    // The end of a quoted string starting at `pos` (past the closing quote).
    let string_end = |pos: usize| -> Option<usize> {
        let q = s[pos];
        let mut i = pos + 1;
        loop {
            let off = memchr::memchr3(q, b'\\', b'\n', &s[i..])?;
            i += off;
            match s[i] {
                b'\\' => i += 2,
                b'\n' => return None,
                _ => return Some(i + 1),
            }
            if i >= n {
                return None;
            }
        }
    };
    let is_tok = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'$' | b'+' | b'-' | b'.') || c >= 0x80;

    loop {
        steps += 1;
        if steps & 0xFFFF == 0 {
            if cancel.load(Ordering::Relaxed) {
                return Err(ParseError { message: "Cancelled".into(), line: 0, col: 0 });
            }
            report(pos, n);
        }
        if b.full() {
            return Err(ParseError { message: too_many(), line: 0, col: 0 });
        }
        skip(&mut pos).map_err(|at| err_at(&src, at, "This comment is never closed (*/ is missing)"))?;
        let depth = st.len();
        let f = st.last_mut().unwrap();
        let mut key = (0u32, 0u32);
        let mut kflags = K_IDX;
        if depth == 1 {
            // Top level: values one after another (JSON Lines), commas allowed.
            if pos >= n {
                break;
            }
            if f.sep && s[pos] == b',' {
                pos += 1;
                f.sep = false;
                continue;
            }
        } else if pos >= n {
            let what = if f.obj { "}" } else { "]" };
            return Err(err_at(&src, n, format!("The file ends before this is closed: {what} is missing")));
        } else {
            let close = if f.obj { b'}' } else { b']' };
            if s[pos] == close {
                pos += 1;
                let node = f.node as usize;
                let start = b.nodes[node].val.0 as usize;
                b.nodes[node].val.1 = (pos - start) as u32;
                st.pop();
                if let Some(p) = st.last_mut() {
                    p.sep = true;
                }
                continue;
            }
            if f.sep {
                if s[pos] == b',' {
                    pos += 1;
                    f.sep = false;
                    continue;
                }
                let want = if f.obj { "a comma or }" } else { "a comma or ]" };
                return Err(err_at(&src, pos, format!("Expected {want} here")));
            }
            if f.obj {
                // A key: quoted, or a bare word (JSON5).
                let c = s[pos];
                if c == b'"' || c == b'\'' {
                    let e = string_end(pos).ok_or_else(|| err_at(&src, pos, "This string is never closed"))?;
                    key = (pos as u32, (e - pos) as u32);
                    kflags = K_SRC | K_RAW;
                    pos = e;
                } else if is_tok(c) {
                    let start = pos;
                    while pos < n && is_tok(s[pos]) {
                        pos += 1;
                    }
                    key = (start as u32, (pos - start) as u32);
                    kflags = K_SRC;
                } else {
                    return Err(err_at(&src, pos, format!("Expected a key (a name in quotes) here, not {}", show_char(&src, pos))));
                }
                skip(&mut pos).map_err(|at| err_at(&src, at, "This comment is never closed (*/ is missing)"))?;
                if pos >= n || (s[pos] != b':' && s[pos] != b'=') {
                    return Err(err_at(&src, pos, "Expected a colon after the key"));
                }
                pos += 1;
                skip(&mut pos).map_err(|at| err_at(&src, at, "This comment is never closed (*/ is missing)"))?;
                if pos >= n {
                    return Err(err_at(&src, n, "The file ends where a value should be"));
                }
            }
        }
        // A value.
        let parent = st.last().unwrap().node;
        let c = s[pos];
        match c {
            b'{' | b'[' => {
                let obj = c == b'{';
                let id = b.add(parent, if obj { Kind::Object } else { Kind::Array }, kflags | V_SRC, key, (pos as u32, 1));
                pos += 1;
                st.push(F { node: id, obj, sep: false });
            }
            b'"' | b'\'' => {
                let e = string_end(pos).ok_or_else(|| err_at(&src, pos, "This string is never closed"))?;
                b.add(parent, Kind::Str, kflags | V_SRC | V_RAW, key, (pos as u32, (e - pos) as u32));
                pos = e;
                st.last_mut().unwrap().sep = true;
            }
            _ if is_tok(c) => {
                let start = pos;
                while pos < n && is_tok(s[pos]) {
                    pos += 1;
                }
                let t = &src[start..pos];
                let kind = match t {
                    "true" | "false" => Kind::Bool,
                    "null" | "undefined" => Kind::Null,
                    _ if is_number(t) => Kind::Num,
                    _ => return Err(err_at(&src, start, format!("“{}” isn't a value (strings need quotes)", clip(t, 40)))),
                };
                b.add(parent, kind, kflags | V_SRC, key, (start as u32, (pos - start) as u32));
                st.last_mut().unwrap().sep = true;
            }
            _ => return Err(err_at(&src, pos, format!("Expected a value here, not {}", show_char(&src, pos)))),
        }
    }
    if b.nodes[top as usize].count == 0 {
        return Err(ParseError { message: "There's nothing in it".into(), line: 0, col: 0 });
    }
    Ok(b.finish(Format::Json, src, top))
}

fn show_char(src: &str, pos: usize) -> String {
    match src[pos..].chars().next() {
        Some(c) if c.is_whitespace() || c.is_control() => format!("U+{:04X}", c as u32),
        Some(c) => format!("“{c}”"),
        None => "the end".into(),
    }
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}

fn is_number(t: &str) -> bool {
    let t = t.strip_prefix(['-', '+']).unwrap_or(t);
    if matches!(t, "Infinity" | "NaN" | "inf" | "nan") {
        return true;
    }
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return !h.is_empty() && h.bytes().all(|c| c.is_ascii_hexdigit());
    }
    let mut seen_digit = false;
    let mut seen_e = false;
    let mut seen_dot = false;
    let b = t.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'0'..=b'9' => seen_digit = true,
            b'.' if !seen_dot && !seen_e => seen_dot = true,
            b'e' | b'E' if seen_digit && !seen_e => {
                seen_e = true;
                if i + 1 < b.len() && (b[i + 1] == b'+' || b[i + 1] == b'-') {
                    i += 1;
                }
                if i + 1 >= b.len() {
                    return false;
                }
            }
            _ => return false,
        }
        i += 1;
    }
    seen_digit
}

/// A quoted JSON (or JSON5) string's text.
fn json_unescape(raw: &str) -> Cow<'_, str> {
    if raw.len() < 2 {
        return Cow::Borrowed(raw);
    }
    let inner = &raw[1..raw.len() - 1];
    if !inner.contains('\\') {
        return Cow::Borrowed(inner);
    }
    let mut out = String::with_capacity(inner.len());
    let mut it = inner.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('v') => out.push('\u{b}'),
            Some('0') => out.push('\0'),
            Some('\n') => {}
            Some('x') => {
                let h: String = it.by_ref().take(2).collect();
                if let Some(c) = u32::from_str_radix(&h, 16).ok().and_then(char::from_u32) {
                    out.push(c);
                }
            }
            Some('u') => {
                let h: String = it.by_ref().take(4).collect();
                let mut cp = u32::from_str_radix(&h, 16).unwrap_or(0xFFFD);
                if (0xD800..0xDC00).contains(&cp) {
                    // A surrogate pair: the low half follows as \uXXXX.
                    let mut look = it.clone();
                    if look.next() == Some('\\') && look.next() == Some('u') {
                        let l: String = look.by_ref().take(4).collect();
                        if let Ok(lo) = u32::from_str_radix(&l, 16) {
                            if (0xDC00..0xE000).contains(&lo) {
                                cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                                it = look;
                            }
                        }
                    }
                }
                out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
            }
            Some(o) => out.push(o),
            None => {}
        }
    }
    Cow::Owned(out)
}

// ---------- YAML (yaml-rust's event parser) ----------

fn parse_yaml(src: String) -> Result<Tree, ParseError> {
    use yaml_rust::parser::{MarkedEventReceiver, Parser};
    use yaml_rust::scanner::{Marker, TScalarStyle, TokenType};
    use yaml_rust::Event;

    struct F {
        node: u32,
        map: bool,
        key: Option<(u32, u32)>,
        /// A mapping's key that is itself a mapping or list.
        complex: bool,
    }
    struct R {
        b: B,
        st: Vec<F>,
        anchors: HashMap<usize, u32>,
        top: u32,
        full: bool,
    }
    fn classify(v: &str, style: TScalarStyle, tag: &Option<TokenType>) -> Kind {
        if let Some(TokenType::Tag(h, suffix)) = tag {
            if h == "!!" {
                match suffix.as_str() {
                    "str" | "binary" => return Kind::Str,
                    "int" | "float" => return Kind::Num,
                    "bool" => return Kind::Bool,
                    "null" => return Kind::Null,
                    "timestamp" => return Kind::Date,
                    _ => {}
                }
            }
        }
        if style != TScalarStyle::Plain {
            return Kind::Str;
        }
        match v {
            "" | "~" | "null" | "Null" | "NULL" => return Kind::Null,
            "true" | "True" | "TRUE" | "false" | "False" | "FALSE" => return Kind::Bool,
            ".inf" | "-.inf" | "+.inf" | ".Inf" | "-.Inf" | ".INF" | "-.INF" | ".nan" | ".NaN" | ".NAN" => return Kind::Num,
            _ => {}
        }
        let t = v.strip_prefix(['-', '+']).unwrap_or(v);
        if let Some(o) = t.strip_prefix("0o") {
            if !o.is_empty() && o.bytes().all(|c| (b'0'..=b'7').contains(&c)) {
                return Kind::Num;
            }
        }
        if !t.starts_with('.') && is_number(&t.replace('_', "")) {
            return Kind::Num;
        }
        // 2001-12-14 or 2001-12-14t21:59:43.10-05:00
        let b = v.as_bytes();
        if b.len() >= 10 && b[4] == b'-' && b[7] == b'-' && b[..4].iter().all(u8::is_ascii_digit) && b[5..7].iter().all(u8::is_ascii_digit) && b[8..10].iter().all(u8::is_ascii_digit) {
            return Kind::Date;
        }
        Kind::Str
    }
    impl R {
        /// The key for the next value in the open container, and whether
        /// the value is itself a key (a mapping waiting for one).
        fn slot(&mut self) -> (u32, u8, (u32, u32), bool) {
            let f = self.st.last_mut().unwrap();
            if f.map {
                match f.key.take() {
                    Some(k) => (f.node, 0, k, false),
                    None => (f.node, 0, (0, 0), true),
                }
            } else {
                (f.node, K_IDX, (0, 0), false)
            }
        }
    }
    impl MarkedEventReceiver for R {
        fn on_event(&mut self, ev: Event, _m: Marker) {
            if self.full || self.st.is_empty() {
                return;
            }
            if self.b.full() {
                self.full = true;
                return;
            }
            match ev {
                Event::Scalar(v, style, anchor, tag) => {
                    let (parent, flags, key, as_key) = self.slot();
                    if as_key {
                        let k = self.b.s(&v);
                        self.st.last_mut().unwrap().key = Some(k);
                        return;
                    }
                    let kind = classify(&v, style, &tag);
                    let val = self.b.s(&v);
                    let id = self.b.add(parent, kind, flags, key, val);
                    if anchor > 0 {
                        self.anchors.insert(anchor, id);
                    }
                }
                Event::Alias(a) => {
                    let (parent, flags, key, as_key) = self.slot();
                    let target = self.anchors.get(&a).copied().unwrap_or(NONE);
                    if as_key {
                        let k = self.b.s("*alias");
                        self.st.last_mut().unwrap().key = Some(k);
                        return;
                    }
                    self.b.add(parent, Kind::Alias, flags | V_ALIAS, key, (target, 0));
                }
                Event::SequenceStart(anchor) | Event::MappingStart(anchor) => {
                    let map = matches!(ev, Event::MappingStart(_));
                    let (parent, flags, mut key, as_key) = self.slot();
                    if as_key {
                        key = self.b.s("? (key)");
                    }
                    let id = self.b.add(parent, if map { Kind::Object } else { Kind::Array }, flags, key, (0, 0));
                    if anchor > 0 {
                        self.anchors.insert(anchor, id);
                    }
                    self.st.push(F { node: id, map, key: None, complex: as_key });
                }
                Event::SequenceEnd | Event::MappingEnd => {
                    if self.st.len() > 1 {
                        let f = self.st.pop().unwrap();
                        if f.complex {
                            let k = self.b.s("(value)");
                            self.st.last_mut().unwrap().key = Some(k);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut r = R { b: B::new(), st: Vec::new(), anchors: HashMap::new(), top: 0, full: false };
    r.top = r.b.add(NONE, Kind::Array, 0, (0, 0), (0, 0));
    r.st.push(F { node: r.top, map: false, key: None, complex: false });
    let mut p = Parser::new(src.chars());
    if let Err(e) = p.load(&mut r, true) {
        #[allow(deprecated)]
        let msg = std::error::Error::description(&e).to_string();
        let m = e.marker();
        return Err(ParseError { message: capitalise(&msg), line: m.line(), col: m.col() + 1 });
    }
    if r.full {
        return Err(ParseError { message: too_many(), line: 0, col: 0 });
    }
    let top = r.top;
    if r.b.nodes[top as usize].count == 0 {
        return Err(ParseError { message: "There's nothing in it".into(), line: 0, col: 0 });
    }
    Ok(r.b.finish(Format::Yaml, String::new(), top))
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

// ---------- TOML (toml_edit) ----------

fn parse_toml(src: String) -> Result<Tree, ParseError> {
    use toml_edit::{Item, Value};
    let doc = match toml_edit::Document::parse(src.as_str()) {
        Ok(d) => d,
        Err(e) => {
            let at = e.span().map(|r| r.start).unwrap_or(0);
            let msg = e.message().trim().to_string();
            return Err(err_at(&src, at, capitalise(&msg)));
        }
    };
    fn value(b: &mut B, parent: u32, flags: u8, key: (u32, u32), v: &Value) {
        if b.full() {
            return;
        }
        match v {
            Value::String(f) => {
                let s = b.s(f.value());
                b.add(parent, Kind::Str, flags, key, s);
            }
            Value::Integer(f) => {
                let s = b.s(&f.value().to_string());
                b.add(parent, Kind::Num, flags, key, s);
            }
            Value::Float(f) => {
                let s = b.s(&f.value().to_string());
                b.add(parent, Kind::Num, flags, key, s);
            }
            Value::Boolean(f) => {
                let s = b.s(if *f.value() { "true" } else { "false" });
                b.add(parent, Kind::Bool, flags, key, s);
            }
            Value::Datetime(f) => {
                let s = b.s(&f.value().to_string());
                b.add(parent, Kind::Date, flags, key, s);
            }
            Value::Array(a) => {
                let id = b.add(parent, Kind::Array, flags, key, (0, 0));
                for x in a.iter() {
                    value(b, id, K_IDX, (0, 0), x);
                }
            }
            Value::InlineTable(t) => {
                let id = b.add(parent, Kind::Object, flags, key, (0, 0));
                for (k, x) in t.iter() {
                    let k = b.s(k);
                    value(b, id, 0, k, x);
                }
            }
        }
    }
    fn item(b: &mut B, parent: u32, flags: u8, key: (u32, u32), it: &Item) {
        if b.full() {
            return;
        }
        match it {
            Item::None => {}
            Item::Value(v) => value(b, parent, flags, key, v),
            Item::Table(t) => {
                let id = b.add(parent, Kind::Object, flags, key, (0, 0));
                for (k, x) in t.iter() {
                    let k = b.s(k);
                    item(b, id, 0, k, x);
                }
            }
            Item::ArrayOfTables(a) => {
                let id = b.add(parent, Kind::Array, flags, key, (0, 0));
                for t in a.iter() {
                    let tid = b.add(id, Kind::Object, K_IDX, (0, 0), (0, 0));
                    for (k, x) in t.iter() {
                        let k = b.s(k);
                        item(b, tid, 0, k, x);
                    }
                }
            }
        }
    }
    let mut b = B::new();
    let root = b.add(NONE, Kind::Object, 0, (0, 0), (0, 0));
    for (k, x) in doc.as_table().iter() {
        let k = b.s(k);
        item(&mut b, root, 0, k, x);
    }
    if b.full() {
        return Err(ParseError { message: too_many(), line: 0, col: 0 });
    }
    Ok(b.finish(Format::Toml, String::new(), root))
}

// ---------- XML (quick-xml) ----------

fn parse_xml(src: String, cancel: &AtomicBool) -> Result<Tree, ParseError> {
    use quick_xml::events::Event;
    let mut b = B::new();
    let mut root = NONE;
    // (node, where its start tag begins)
    let mut st: Vec<(u32, usize)> = Vec::new();
    let mut r = quick_xml::Reader::from_str(&src);
    r.config_mut().trim_text(false);
    r.config_mut().check_end_names = true;
    let mut steps = 0u32;
    // An element's start: its node and its attributes.
    fn element(b: &mut B, parent: u32, e: &quick_xml::events::BytesStart, at: usize) -> Result<u32, String> {
        let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
        let k = b.s(&name);
        let id = b.add(parent, Kind::Element, V_SRC, k, (at as u32, 0));
        for a in e.attributes() {
            let a = a.map_err(|e| capitalise(&e.to_string()))?;
            let an = String::from_utf8_lossy(a.key.as_ref()).into_owned();
            let v = a.unescape_value().map(|v| v.into_owned()).unwrap_or_else(|_| String::from_utf8_lossy(&a.value).into_owned());
            let k = b.s(&an);
            let v = b.s(&v);
            b.add(id, Kind::Attr, 0, k, v);
        }
        Ok(id)
    }
    // An element closed at `end`: only text inside → its value.
    fn close(b: &mut B, id: u32, end: usize) {
        let n = b.nodes[id as usize];
        if n.count == 1 && b.nodes[n.first as usize].kind == Kind::Text {
            let t = b.nodes[n.first as usize];
            let node = &mut b.nodes[id as usize];
            node.val = t.val;
            node.flags &= !V_SRC;
            node.first = NONE;
            node.count = 0;
        } else if n.count == 0 {
            let node = &mut b.nodes[id as usize];
            node.val = (0, 0);
            node.flags &= !V_SRC;
        } else {
            let start = n.val.0 as usize;
            b.nodes[id as usize].val.1 = (end - start) as u32;
        }
    }
    loop {
        steps += 1;
        if steps & 0xFFFF == 0 {
            if cancel.load(Ordering::Relaxed) {
                return Err(ParseError { message: "Cancelled".into(), line: 0, col: 0 });
            }
            report(r.buffer_position() as usize, src.len());
        }
        if b.full() {
            return Err(ParseError { message: too_many(), line: 0, col: 0 });
        }
        let before = r.buffer_position() as usize;
        let ev = match r.read_event() {
            Ok(ev) => ev,
            Err(e) => {
                let at = r.error_position() as usize;
                return Err(err_at(&src, at, capitalise(&e.to_string())));
            }
        };
        let parent = st.last().map(|x| x.0).unwrap_or(NONE);
        match ev {
            Event::Start(e) | Event::Empty(e) if parent == NONE && root != NONE => {
                let _ = e;
                return Err(err_at(&src, before, "A second top-level element (XML has one root)"));
            }
            Event::Start(e) => {
                let id = element(&mut b, parent, &e, before).map_err(|m| err_at(&src, before, m))?;
                if parent == NONE {
                    root = id;
                }
                st.push((id, before));
            }
            Event::Empty(e) => {
                let id = element(&mut b, parent, &e, before).map_err(|m| err_at(&src, before, m))?;
                if parent == NONE {
                    root = id;
                }
                close(&mut b, id, r.buffer_position() as usize);
            }
            Event::End(_) => {
                if let Some((id, _)) = st.pop() {
                    close(&mut b, id, r.buffer_position() as usize);
                }
            }
            Event::Text(t) => {
                if parent == NONE {
                    continue;
                }
                let text = match t.unescape() {
                    Ok(x) => x.into_owned(),
                    Err(e) => return Err(err_at(&src, before, capitalise(&e.to_string()))),
                };
                let tt = text.trim();
                if !tt.is_empty() {
                    let v = b.s(tt);
                    b.add(parent, Kind::Text, 0, (0, 0), v);
                }
            }
            Event::CData(c) => {
                if parent != NONE {
                    let text = String::from_utf8_lossy(&c.into_inner()).into_owned();
                    let v = b.s(&text);
                    b.add(parent, Kind::Text, 0, (0, 0), v);
                }
            }
            Event::Comment(c) => {
                if parent != NONE {
                    let text = String::from_utf8_lossy(&c.into_inner()).trim().to_string();
                    let v = b.s(&text);
                    b.add(parent, Kind::Comment, 0, (0, 0), v);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if let Some((id, _)) = st.last() {
        let name = b.pool[b.nodes[*id as usize].key.0 as usize..][..b.nodes[*id as usize].key.1 as usize].to_string();
        return Err(err_at(&src, src.len(), format!("The file ends before <{name}> is closed")));
    }
    if root == NONE {
        return Err(ParseError { message: "There's no element in it".into(), line: 0, col: 0 });
    }
    Ok(b.finish(Format::Xml, src, root))
}

// ---------- INI (desktop entries, systemd units, git config...) ----------

fn parse_ini(src: String) -> Result<Tree, ParseError> {
    let mut b = B::new();
    let root = b.add(NONE, Kind::Object, 0, (0, 0), (0, 0));
    let mut cur = root;
    let mut offset = 0usize;
    let mut lines = src.split_inclusive('\n').peekable();
    while let Some(raw) = lines.next() {
        let at = offset;
        offset += raw.len();
        let t = raw.trim();
        if t.is_empty() || t.starts_with(';') || t.starts_with('#') {
            continue;
        }
        if t.starts_with('[') {
            let Some(end) = t.rfind(']') else {
                return Err(err_at(&src, at + raw.len() - raw.trim_start().len(), "This section name is missing its ]"));
            };
            let name = t[1..end].trim();
            let k = b.s(name);
            cur = b.add(root, Kind::Object, 0, k, (0, 0));
            continue;
        }
        let (k, mut v) = match t.find('=') {
            Some(i) => (t[..i].trim(), t[i + 1..].trim().to_string()),
            // git config: a bare name means true.
            None if !t.contains(char::is_whitespace) => (t, String::new()),
            None => return Err(err_at(&src, at + raw.len() - raw.trim_start().len(), "Expected name = value or a [section] here")),
        };
        // A value continued on the next line (ends with a backslash).
        while v.ends_with('\\') {
            v.pop();
            match lines.next() {
                Some(next) => {
                    offset += next.len();
                    v.push_str(next.trim());
                }
                None => break,
            }
        }
        let v = if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') { v[1..v.len() - 1].to_string() } else { v };
        let kind = match v.as_str() {
            "true" | "false" => Kind::Bool,
            _ if is_number(&v) && !v.starts_with('+') => Kind::Num,
            _ => Kind::Str,
        };
        let ks = b.s(k);
        let vs = b.s(&v);
        b.add(cur, kind, 0, ks, vs);
        if b.full() {
            return Err(ParseError { message: too_many(), line: 0, col: 0 });
        }
    }
    Ok(b.finish(Format::Ini, String::new(), root))
}

// ---------- reading the tree ----------

fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some(f) if f.is_alphabetic() || f == '_' || f == '$') && c.all(|x| x.is_alphanumeric() || x == '_' || x == '$')
}

impl Tree {
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    pub fn root(&self) -> u32 {
        self.root
    }
    pub fn kind(&self, n: u32) -> Kind {
        self.nodes[n as usize].kind
    }
    pub fn count(&self, n: u32) -> u32 {
        self.nodes[n as usize].count
    }
    pub fn parent(&self, n: u32) -> u32 {
        self.nodes[n as usize].parent
    }
    pub fn expandable(&self, n: u32) -> bool {
        self.nodes[n as usize].count > 0
    }
    pub fn first_child(&self, n: u32) -> u32 {
        self.nodes[n as usize].first
    }
    pub fn next_sibling(&self, n: u32) -> u32 {
        self.nodes[n as usize].next
    }
    pub fn children(&self, n: u32) -> Children<'_> {
        Children { t: self, next: self.nodes[n as usize].first }
    }
    /// Bytes the tree holds (for "too big" checks and tests).
    pub fn memory(&self) -> usize {
        self.src.len() + self.pool.len() + self.nodes.len() * std::mem::size_of::<Node>()
    }

    fn span(&self, src: bool, (o, l): (u32, u32)) -> &str {
        let s = if src { &self.src } else { &self.pool };
        s.get(o as usize..(o + l) as usize).unwrap_or("")
    }
    pub fn key_is_index(&self, n: u32) -> bool {
        self.nodes[n as usize].flags & K_IDX != 0
    }
    pub fn index(&self, n: u32) -> u32 {
        self.nodes[n as usize].key.0
    }
    /// The key (an index as its number; "" for the root and XML text).
    pub fn key(&self, n: u32) -> Cow<'_, str> {
        let x = &self.nodes[n as usize];
        if x.flags & K_IDX != 0 {
            return Cow::Owned(x.key.0.to_string());
        }
        let raw = self.span(x.flags & K_SRC != 0, x.key);
        if x.flags & K_RAW != 0 {
            json_unescape(raw)
        } else {
            Cow::Borrowed(raw)
        }
    }
    /// A leaf's value as text (strings without quotes or escapes).
    pub fn value(&self, n: u32) -> Cow<'_, str> {
        let x = &self.nodes[n as usize];
        if x.flags & V_ALIAS != 0 {
            return Cow::Owned(if x.val.0 == NONE { "(an anchor that isn't defined)".into() } else { format!("same as {}", self.path(x.val.0)) });
        }
        if x.kind.is_container() && x.count > 0 {
            return Cow::Borrowed("");
        }
        let raw = self.span(x.flags & V_SRC != 0, x.val);
        if x.flags & V_RAW != 0 {
            json_unescape(raw)
        } else {
            Cow::Borrowed(raw)
        }
    }
    /// The start of a leaf's value as text: at most `max` characters (a
    /// 50 MB string isn't unescaped whole to show a row), and whether there's more.
    pub fn value_head(&self, n: u32, max: usize) -> (Cow<'_, str>, bool) {
        let x = &self.nodes[n as usize];
        if x.flags & V_ALIAS != 0 || (x.kind.is_container() && x.count > 0) {
            return (self.value(n), false);
        }
        let raw = self.span(x.flags & V_SRC != 0, x.val);
        // Bytes enough for `max` characters (escapes take up to 12).
        let lim = max.saturating_mul(12).saturating_add(16);
        if raw.len() <= lim {
            let v = self.value(n);
            return match v.char_indices().nth(max) {
                Some((i, _)) => (Cow::Owned(v[..i].to_string()), true),
                None => (v, false),
            };
        }
        let mut cut = lim;
        while !raw.is_char_boundary(cut) {
            cut -= 1;
        }
        let head = &raw[..cut];
        let text: String = if x.flags & V_RAW != 0 {
            // Unescape the start only; drop a half escape at the cut.
            let mut h = head.to_string();
            if let Some(i) = h.rfind('\\') {
                if h.len() - i < 12 {
                    h.truncate(i);
                }
            }
            h.push(h.chars().next().unwrap_or('"'));
            json_unescape(&h).into_owned()
        } else {
            head.to_string()
        };
        let t: String = text.chars().take(max).collect();
        (Cow::Owned(t), true)
    }

    /// Every `size`-th child of `n` (where each page of children starts).
    pub fn page_starts(&self, n: u32, size: u32) -> Vec<u32> {
        self.children(n).enumerate().filter(|(i, _)| *i as u32 % size == 0).map(|(_, c)| c).collect()
    }

    /// The value as it is searched (no unescaping: a borrowed slice).
    fn search_text(&self, n: u32) -> (&str, &str) {
        let x = &self.nodes[n as usize];
        let key = if x.flags & K_IDX != 0 {
            ""
        } else {
            let k = self.span(x.flags & K_SRC != 0, x.key);
            if x.flags & K_RAW != 0 && k.len() >= 2 { &k[1..k.len() - 1] } else { k }
        };
        let val = if x.flags & V_ALIAS != 0 || (x.kind.is_container() && x.count > 0) {
            ""
        } else {
            let v = self.span(x.flags & V_SRC != 0, x.val);
            if x.flags & V_RAW != 0 && v.len() >= 2 { &v[1..v.len() - 1] } else { v }
        };
        (key, val)
    }
    /// The kind of value, in the format's own words.
    pub fn type_name(&self, n: u32) -> &'static str {
        let x = &self.nodes[n as usize];
        match (x.kind, self.format) {
            (Kind::Object, Format::Yaml) => "mapping",
            (Kind::Object, Format::Toml) => "table",
            (Kind::Object, Format::Ini) => "section",
            (Kind::Object, _) => "object",
            (Kind::Array, Format::Yaml) => "sequence",
            (Kind::Array, _) => "array",
            (Kind::Str, _) => "string",
            (Kind::Num, _) => {
                let v = self.span(x.flags & V_SRC != 0, x.val);
                if v.bytes().take(64).all(|c| c.is_ascii_digit() || c == b'-' || c == b'+' || c == b'_') || v.starts_with("0x") {
                    "integer"
                } else {
                    "number"
                }
            }
            (Kind::Bool, _) => "boolean",
            (Kind::Null, _) => "null",
            (Kind::Date, _) => "date",
            (Kind::Element, _) => "element",
            (Kind::Attr, _) => "attribute",
            (Kind::Text, _) => "text",
            (Kind::Comment, _) => "comment",
            (Kind::Alias, _) => "alias",
        }
    }
    /// A container's size as shown when it's closed: "{ 12 }", "[ 3 ]".
    pub fn summary(&self, n: u32) -> String {
        let x = &self.nodes[n as usize];
        match x.kind {
            Kind::Object => format!("{{ {} }}", x.count),
            Kind::Array => format!("[ {} ]", x.count),
            Kind::Element if x.count > 0 => format!("‹ {} ›", x.count),
            _ => String::new(),
        }
    }

    /// Where `n` is: `servers[2].host`, or `/config/server[3]/@host` (XML).
    pub fn path(&self, n: u32) -> String {
        let mut chain = Vec::new();
        let mut c = n;
        while c != NONE {
            chain.push(c);
            c = self.nodes[c as usize].parent;
        }
        chain.reverse();
        if self.format == Format::Xml {
            let mut out = String::new();
            for &c in &chain {
                let x = &self.nodes[c as usize];
                out.push('/');
                match x.kind {
                    Kind::Attr => {
                        out.push('@');
                        out.push_str(&self.key(c));
                    }
                    Kind::Text => out.push_str("text()"),
                    Kind::Comment => out.push_str("comment()"),
                    _ => {
                        let name = self.key(c);
                        out.push_str(&name);
                        if x.parent != NONE {
                            let (mut nth, mut same) = (0, 0);
                            for s in self.children(x.parent) {
                                if self.nodes[s as usize].kind == Kind::Element && self.key(s) == name {
                                    same += 1;
                                    if s == c {
                                        nth = same;
                                    }
                                }
                            }
                            if same > 1 {
                                out.push_str(&format!("[{nth}]"));
                            }
                        }
                    }
                }
            }
            return out;
        }
        let mut out = String::new();
        for &c in chain.iter().skip(1) {
            if self.key_is_index(c) {
                out.push_str(&format!("[{}]", self.index(c)));
            } else {
                let k = self.key(c);
                if is_ident(&k) {
                    if !out.is_empty() {
                        out.push('.');
                    }
                    out.push_str(&k);
                } else {
                    out.push('[');
                    out.push_str(&serde_json::to_string(k.as_ref()).unwrap_or_default());
                    out.push(']');
                }
            }
        }
        if out.is_empty() {
            "$".into()
        } else {
            out
        }
    }

    /// What Copy Value puts on the clipboard: a leaf's text; a container's
    /// own text from the file (JSON, XML) or as JSON (the others).
    pub fn copy_text(&self, n: u32) -> String {
        let x = &self.nodes[n as usize];
        if x.flags & V_ALIAS != 0 {
            return if x.val.0 == NONE { String::new() } else { self.copy_text(x.val.0) };
        }
        if x.kind.is_container() && x.count > 0 {
            if x.flags & V_SRC != 0 && x.val.1 > 0 {
                return self.span(true, x.val).to_string();
            }
            let mut out = String::new();
            self.to_json(n, 0, &mut out);
            return out;
        }
        self.value(n).into_owned()
    }

    fn to_json(&self, n: u32, depth: usize, out: &mut String) {
        let x = &self.nodes[n as usize];
        if depth > 20_000 {
            out.push_str("null");
            return;
        }
        let pad = |out: &mut String, d: usize| {
            out.push('\n');
            for _ in 0..d {
                out.push_str("  ");
            }
        };
        match x.kind {
            Kind::Object | Kind::Element if x.count > 0 => {
                out.push('{');
                let mut first = true;
                for c in self.children(n) {
                    if !first {
                        out.push(',');
                    }
                    first = false;
                    pad(out, depth + 1);
                    let k = match self.kind(c) {
                        Kind::Attr => format!("@{}", self.key(c)),
                        Kind::Text => "#text".into(),
                        Kind::Comment => "#comment".into(),
                        _ => self.key(c).into_owned(),
                    };
                    out.push_str(&serde_json::to_string(&k).unwrap_or_default());
                    out.push_str(": ");
                    self.to_json(c, depth + 1, out);
                }
                pad(out, depth);
                out.push('}');
            }
            Kind::Array if x.count > 0 => {
                out.push('[');
                let mut first = true;
                for c in self.children(n) {
                    if !first {
                        out.push(',');
                    }
                    first = false;
                    pad(out, depth + 1);
                    self.to_json(c, depth + 1, out);
                }
                pad(out, depth);
                out.push(']');
            }
            Kind::Object => out.push_str("{}"),
            Kind::Array => out.push_str("[]"),
            Kind::Num => {
                let v = self.value(n);
                if serde_json::from_str::<serde_json::Number>(&v).is_ok() {
                    out.push_str(&v);
                } else {
                    out.push_str(&serde_json::to_string(v.as_ref()).unwrap_or_default());
                }
            }
            Kind::Bool => out.push_str(if self.value(n).eq_ignore_ascii_case("true") { "true" } else { "false" }),
            Kind::Null => out.push_str("null"),
            Kind::Alias => {
                if x.val.0 != NONE && depth < 64 {
                    self.to_json(x.val.0, depth, out);
                } else {
                    out.push_str("null");
                }
            }
            _ => out.push_str(&serde_json::to_string(self.value(n).as_ref()).unwrap_or_default()),
        }
    }

    /// Nodes whose key or value has `query` in it (ignoring case): the first
    /// `cap`, in document order, and how many there are in all.
    pub fn search(&self, query: &str, cap: usize, cancel: &AtomicBool) -> (Vec<u32>, usize) {
        let mut hits = Vec::new();
        let mut total = 0usize;
        if query.is_empty() {
            return (hits, 0);
        }
        let ascii = query.is_ascii();
        let q = query.to_lowercase();
        let qb = q.as_bytes();
        let hit = |h: &str| -> bool {
            if h.len() < qb.len() {
                return false;
            }
            if ascii {
                ci_find(h.as_bytes(), qb)
            } else {
                h.to_lowercase().contains(&q)
            }
        };
        for n in 0..self.nodes.len() as u32 {
            if n & 0xFFFF == 0 && cancel.load(Ordering::Relaxed) {
                break;
            }
            if n == self.root && !self.nodes[n as usize].kind.is_container() {
                // A lone value: search it too.
            } else if n == self.root && self.format != Format::Xml {
                continue;
            } else if self.nodes[n as usize].parent == NONE && n != self.root {
                // The dropped synthetic root.
                continue;
            }
            let (k, v) = self.search_text(n);
            if hit(k) || hit(v) {
                total += 1;
                if hits.len() < cap {
                    hits.push(n);
                }
            }
        }
        (hits, total)
    }
}

/// `q` (lower-case ASCII) in `h`, ignoring ASCII case.
fn ci_find(h: &[u8], q: &[u8]) -> bool {
    if q.is_empty() {
        return true;
    }
    let (a, b) = (q[0], q[0].to_ascii_uppercase());
    let last = h.len() - q.len();
    let mut i = 0;
    while i <= last {
        let Some(off) = memchr::memchr2(a, b, &h[i..=last]) else { return false };
        i += off;
        if h[i..i + q.len()].iter().zip(q).all(|(x, y)| x.to_ascii_lowercase() == *y) {
            return true;
        }
        i += 1;
    }
    false
}

pub struct Children<'a> {
    t: &'a Tree,
    next: u32,
}

impl Iterator for Children<'_> {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.next == NONE {
            return None;
        }
        let c = self.next;
        self.next = self.t.nodes[c as usize].next;
        Some(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str, f: Format) -> Tree {
        parse(s.as_bytes().to_vec(), f, &AtomicBool::new(false)).unwrap()
    }
    fn find(t: &Tree, path: &str) -> u32 {
        (0..t.len() as u32).find(|&n| t.parent(n) != NONE && t.path(n) == path).unwrap_or_else(|| panic!("no {path}"))
    }

    #[test]
    fn json_paths_values() {
        let t = p(r#"{"servers": [{"host": "a"}, {"host": "bé"}, {"host": "c", "my key": 1.5e3}], "ok": true, "n": null}"#, Format::Json);
        let h = find(&t, "servers[1].host");
        assert_eq!(t.value(h), "bé");
        assert_eq!(t.type_name(find(&t, "servers[2][\"my key\"]")), "number");
        let s = find(&t, "servers");
        assert_eq!(t.summary(s), "[ 3 ]");
        assert_eq!(t.copy_text(find(&t, "servers[0]")), r#"{"host": "a"}"#);
        assert_eq!(t.kind(find(&t, "ok")), Kind::Bool);
        assert_eq!(t.kind(find(&t, "n")), Kind::Null);
        let (hits, total) = t.search("HOST", 10, &AtomicBool::new(false));
        assert_eq!((hits.len(), total), (3, 3));
    }

    #[test]
    fn json5_and_lines() {
        let t = p("// c\n{a: 1, 'b': 'x', c: [1,2,],/* d */ e: Infinity, f: 0x1F,}\n", Format::Json);
        assert_eq!(t.value(find(&t, "b")), "x");
        assert_eq!(t.count(find(&t, "c")), 2);
        let t = p("{\"a\":1}\n{\"a\":2}\n", Format::Json);
        assert_eq!(t.value(find(&t, "[1].a")), "2");
    }

    #[test]
    fn json_errors() {
        let e = parse(b"{\n  \"a\": 1\n  \"b\": 2\n}".to_vec(), Format::Json, &AtomicBool::new(false)).err().unwrap();
        assert_eq!((e.line, e.col), (3, 3), "{e}");
        let e = parse(b"[1, 2".to_vec(), Format::Json, &AtomicBool::new(false)).err().unwrap();
        assert!(e.message.contains("]"), "{e}");
        let e = parse(b"{\"a\": nope}".to_vec(), Format::Json, &AtomicBool::new(false)).err().unwrap();
        assert_eq!(e.col, 7);
    }

    #[test]
    fn yaml() {
        let t = p("a: &x\n  b: 1\n  c: [true, ~, \"s\"]\nd: *x\nlist:\n  - name: n1\n  - name: n2\n", Format::Yaml);
        assert_eq!(t.type_name(find(&t, "a")), "mapping");
        assert_eq!(t.kind(find(&t, "a.c[0]")), Kind::Bool);
        assert_eq!(t.kind(find(&t, "a.c[1]")), Kind::Null);
        assert_eq!(t.value(find(&t, "list[1].name")), "n2");
        assert_eq!(t.value(find(&t, "d")), "same as a");
        assert!(t.copy_text(find(&t, "a")).contains("\"b\": 1"));
        let e = parse(b"a: [1, 2\nb: 3\n".to_vec(), Format::Yaml, &AtomicBool::new(false)).err().unwrap();
        assert!(e.line >= 1, "{e}");
        let t = p("---\na: 1\n---\nb: 2\n", Format::Yaml);
        assert_eq!(t.value(find(&t, "[1].b")), "2");
    }

    #[test]
    fn toml() {
        let t = p("title = \"x\"\n[owner]\nname = \"Tom\"\ndob = 1979-05-27T07:32:00Z\n[[products]]\nname = \"a\"\n[[products]]\nname = \"b\"\nsku = 0x10\n", Format::Toml);
        assert_eq!(t.value(find(&t, "products[1].name")), "b");
        assert_eq!(t.kind(find(&t, "owner.dob")), Kind::Date);
        assert_eq!(t.type_name(find(&t, "owner")), "table");
        let e = parse(b"a = 1\nb = \n".to_vec(), Format::Toml, &AtomicBool::new(false)).err().unwrap();
        assert_eq!(e.line, 2, "{e}");
    }

    #[test]
    fn xml() {
        let t = p("<?xml version=\"1.0\"?>\n<config a=\"1\"><server host=\"x\"/><server host=\"y\">hi &amp; bye</server><!-- note --><name>n</name></config>", Format::Xml);
        assert_eq!(t.path(t.root()), "/config");
        let h = find(&t, "/config/server[2]/@host");
        assert_eq!(t.value(h), "y");
        assert_eq!(t.value(find(&t, "/config/server[2]/text()")), "hi & bye");
        assert_eq!(t.value(find(&t, "/config/name")), "n");
        assert_eq!(t.copy_text(find(&t, "/config/server[2]")), "<server host=\"y\">hi &amp; bye</server>");
        let e = parse(b"<a>\n<b></a>".to_vec(), Format::Xml, &AtomicBool::new(false)).err().unwrap();
        assert_eq!(e.line, 2, "{e}");
    }

    #[test]
    fn ini() {
        let t = p("top=1\n[Desktop Entry]\nName=Grove\nTerminal=false\n; c\n[core]\n\tbare\n", Format::Ini);
        assert_eq!(t.value(find(&t, "[\"Desktop Entry\"].Name")), "Grove");
        assert_eq!(t.kind(find(&t, "[\"Desktop Entry\"].Terminal")), Kind::Bool);
        assert_eq!(t.value(find(&t, "top")), "1");
    }

    #[test]
    fn formats_by_name() {
        assert_eq!(format_for("/x/Cargo.lock"), Some(Format::Toml));
        assert_eq!(format_for("a.JSON"), Some(Format::Json));
        assert_eq!(format_for("grove.desktop"), Some(Format::Ini));
        assert_eq!(format_for("x.rs"), None);
        assert_eq!(format_for(".json"), None);
    }

    #[test]
    fn monsters() {
        // 10 000 levels deep: parsed, pathed and copied without recursion trouble.
        let deep = "[".repeat(10_000) + &"]".repeat(10_000);
        let t = p(&deep, Format::Json);
        let mut n = t.root();
        let mut d = 0;
        while t.count(n) > 0 {
            n = t.children(n).next().unwrap();
            d += 1;
        }
        assert_eq!(d, 9_999);
        assert!(t.path(n).starts_with("[0][0]"));
        let t2 = p(&("<a>".repeat(5_000) + &"</a>".repeat(5_000)), Format::Xml);
        assert_eq!(t2.len(), 5_000);
        // One 5 MB string: its head comes back without unescaping it all.
        let long = format!("{{\"s\": \"{}\\n{}\"}}", "x".repeat(3_000_000), "y".repeat(2_000_000));
        let t = p(&long, Format::Json);
        let s = find(&t, "s");
        let (h, more) = t.value_head(s, 300);
        assert_eq!((h.chars().count(), more), (300, true));
        let (h, more) = t.value_head(s, 10);
        assert_eq!((h.as_ref(), more), ("xxxxxxxxxx", true));
        let short = p(r#"{"a": "h\u00e9llo"}"#, Format::Json);
        assert_eq!(short.value_head(find(&short, "a"), 300), (Cow::Borrowed("héllo"), false));
        // A budget too small for the tree: a plain message, not a crash.
        let mut many = String::from("[");
        for i in 0..300_000 {
            many.push_str(if i > 0 { ",1" } else { "1" });
        }
        many.push(']');
        let e = parse_with(many.into_bytes(), Format::Json, &AtomicBool::new(false), &std::sync::atomic::AtomicU32::new(0), 4 << 20).err().unwrap();
        assert!(e.message.contains("too big"), "{e}");
        // Page starts of a big array.
        let t = p(&format!("[{}]", vec!["0"; 2500].join(",")), Format::Json);
        assert_eq!(t.page_starts(t.root(), 1000).len(), 3);
    }

    #[test]
    fn big_json_is_compact() {
        let mut s = String::from("[");
        for i in 0..200_000 {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!("{{\"id\":{i},\"name\":\"item {i}\",\"tags\":[\"a\",\"b\"]}}"));
        }
        s.push(']');
        let len = s.len();
        let t = p(&s, Format::Json);
        assert_eq!(t.count(t.root()), 200_000);
        // The file plus ~40 bytes a value.
        assert!(t.memory() < len + t.len() * 48, "{} for {}", t.memory(), len);
    }
}
