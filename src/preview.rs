//! Quick Look: what to show for a file (images, media, PDF, code with
//! syntax highlighting, Markdown, archive listings, folder summaries).

use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

fn syntaxes() -> &'static SyntaxSet {
    static S: OnceLock<SyntaxSet> = OnceLock::new();
    // bat's set (two-face): ~200 languages, PowerShell, VBScript, TOML,
    // TypeScript, Nix, CMake, Dockerfile, INI, assembly... besides syntect's.
    S.get_or_init(|| {
        let mut b = two_face::syntax::extra_newlines().into_builder();
        // Languages bat lacks (data/syntaxes, each with its licence).
        for src in [
            include_str!("../data/syntaxes/PowerShell.sublime-syntax"),
            // Tessera's shell language (.tsh, .tessh, #!…tessh).
            include_str!("../data/syntaxes/tessh.sublime-syntax"),
        ] {
            match syntect::parsing::SyntaxDefinition::load_from_str(src, true, None) {
                Ok(d) => b.add(d),
                Err(e) => eprintln!("grove: a syntax didn't load: {e}"),
            }
        }
        b.build()
    })
}

fn themes() -> &'static ThemeSet {
    static T: OnceLock<ThemeSet> = OnceLock::new();
    T.get_or_init(ThemeSet::load_defaults)
}

const TEXT_LIMIT: usize = 256 * 1024;

fn is_texty(mime: &str, parents: &[String]) -> bool {
    mime.starts_with("text/")
        || parents.iter().any(|p| p == "text/plain")
        || matches!(mime, "application/json" | "application/xml" | "application/javascript" | "application/x-shellscript" | "application/toml" | "application/x-yaml" | "application/sql" | "application/x-desktop")
}

fn read_head(p: &Path, limit: usize) -> Option<(String, bool)> {
    let mut f = std::fs::File::open(p).ok()?;
    let mut buf = vec![0u8; limit];
    let mut n = 0;
    while n < limit {
        let k = f.read(&mut buf[n..]).ok()?;
        if k == 0 {
            break;
        }
        n += k;
    }
    buf.truncate(n);
    if buf.iter().take(8000).any(|&b| b == 0) {
        return None;
    }
    let truncated = n == limit;
    Some((String::from_utf8_lossy(&buf).into_owned(), truncated))
}

fn syntax_for<'a>(ss: &'a SyntaxSet, text: &str, name: &str) -> &'a syntect::parsing::SyntaxReference {
    let ext = Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    // Whole names first (Makefile, CMakeLists.txt, Dockerfile, .bashrc).
    ss.find_syntax_by_extension(name)
        .or_else(|| ss.find_syntax_by_extension(&name.to_lowercase()))
        .or_else(|| ss.find_syntax_by_extension(&ext))
        .or_else(|| ss.find_syntax_by_token(&ext))
        // Close relatives for what has no syntax of its own.
        .or_else(|| match ext.as_str() {
            "vbs" | "vb" | "bas" | "frm" | "cls" | "vba" => ss.find_syntax_by_name("ASP"),
            "s" | "inc" | "a51" | "nasm" => ss.find_syntax_by_extension("asm"),
            "ahk" | "au3" => ss.find_syntax_by_name("Batch File"),
            _ => None,
        })
        .or_else(|| text.lines().next().and_then(|l| ss.find_syntax_by_first_line(l)))
        .unwrap_or_else(|| ss.find_syntax_plain_text())
}

/// Whether the name (or the first line, like a shebang) picks a language.
pub fn known_by_name(name: &str, text: &str) -> bool {
    syntax_for(syntaxes(), text, name).name != "Plain Text"
}

/// The first `max_lines` lines as coloured runs (light theme), for text
/// thumbnails: [[["#rrggbb", "text"], ...], ...].
pub fn highlight_runs(text: &str, name: &str, max_lines: usize) -> Vec<Vec<(String, String)>> {
    let ss = syntaxes();
    let syntax = syntax_for(ss, text, name);
    let theme = &themes().themes["InspiredGitHub"];
    let mut h = syntect::easy::HighlightLines::new(syntax, theme);
    let mut out = vec![];
    for line in syntect::util::LinesWithEndings::from(text).take(max_lines) {
        let regions = h.highlight_line(line, ss).unwrap_or_default();
        let runs: Vec<(String, String)> = regions
            .into_iter()
            .map(|(st, piece)| {
                let c = st.foreground;
                (format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b), piece.trim_end_matches(['\n', '\r']).to_string())
            })
            .filter(|(_, t)| !t.is_empty())
            .collect();
        out.push(runs);
    }
    out
}

/// Syntax colouring carried across blocks of one file (Quick Look shows
/// a file as many blocks; the parser state runs on from one to the next).
pub struct Highlighter {
    h: syntect::easy::HighlightLines<'static>,
}

impl Highlighter {
    pub fn new(text: &str, name: &str, dark: bool) -> Self {
        let ss = syntaxes();
        let syntax = syntax_for(ss, text, name);
        let theme = &themes().themes[if dark { "base16-ocean.dark" } else { "InspiredGitHub" }];
        Highlighter { h: syntect::easy::HighlightLines::new(syntax, theme) }
    }

    /// Lines (no trailing newline) as `<pre>` HTML.
    pub fn html(&mut self, block: &str) -> String {
        let ss = syntaxes();
        let mut out = String::with_capacity(block.len() * 3);
        out.push_str("<pre style=\"margin:0\">");
        let with_nl = format!("{block}\n");
        for line in syntect::util::LinesWithEndings::from(&with_nl) {
            let regions = self.h.highlight_line(line, ss).unwrap_or_default();
            for (style, piece) in regions {
                let c = style.foreground;
                let piece = piece.strip_suffix('\n').unwrap_or(piece);
                let esc = piece.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
                out.push_str(&format!("<span style=\"color:#{:02x}{:02x}{:02x}\">{}</span>", c.r, c.g, c.b, esc));
            }
            if line.ends_with('\n') {
                out.push('\n');
            }
        }
        if out.ends_with('\n') {
            out.pop();
        }
        out.push_str("</pre>");
        out
    }
}

/// .md / .markdown / .mdown / .mkd (lower-case name).
pub fn is_markdown_name(lower: &str) -> bool {
    [".md", ".markdown", ".mdown", ".mkd", ".mkdn"].iter().any(|e| lower.ends_with(e))
}

pub fn highlight(text: &str, name: &str, dark: bool) -> String {
    let ss = syntaxes();
    let syntax = syntax_for(ss, text, name);
    let theme = &themes().themes[if dark { "base16-ocean.dark" } else { "InspiredGitHub" }];
    // Limit lines so the rich text stays quick.
    let shown: String = text.lines().take(2000).map(|l| format!("{l}\n")).collect();
    let mut h = syntect::easy::HighlightLines::new(syntax, theme);
    let mut out = String::from("<pre style=\"margin:0\">");
    for line in syntect::util::LinesWithEndings::from(&shown) {
        let regions = h.highlight_line(line, ss).unwrap_or_default();
        for (style, piece) in regions {
            let c = style.foreground;
            let esc = piece.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
            out.push_str(&format!("<span style=\"color:#{:02x}{:02x}{:02x}\">{}</span>", c.r, c.g, c.b, esc));
        }
    }
    out.push_str("</pre>");
    out
}

pub fn preview(path: &str, dark: bool) -> Value {
    let p = Path::new(path);
    let meta = match std::fs::metadata(p) {
        Ok(m) => m,
        Err(e) => return json!({"kind": "error", "error": grove_core::util::plain_error(&e, p)}),
    };
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if meta.is_dir() {
        let mut files = 0u64;
        let mut dirs = 0u64;
        let mut names = vec![];
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if n.starts_with('.') {
                    continue;
                }
                if e.path().is_dir() {
                    dirs += 1
                } else {
                    files += 1
                }
                if names.len() < 12 {
                    names.push(n);
                }
            }
        }
        names.sort_by(|a, b| grove_core::util::natural_cmp(a, b));
        return json!({"kind": "folder", "name": name, "files": files, "folders": dirs, "sample": names});
    }
    let m = crate::mime::deep(path);
    let parents = crate::mime::with_parents(&m.name);
    let base = json!({"name": name, "mime": m.name, "kindText": m.comment, "size": meta.len(), "sizeText": grove_core::util::human_size(meta.len())});
    let with = |mut v: Value| {
        for (k, x) in base.as_object().unwrap() {
            v[k] = x.clone();
        }
        v
    };
    use grove_core::previews::{self, Kind};
    let lower = name.to_lowercase();
    // Tables (CSV, TSV, spreadsheets): Quick Look's table view reads them
    // itself (TableData), whole, off the UI thread.
    if crate::table::is_table_name(&lower) {
        return with(json!({"kind": "table"}));
    }
    // SQLite databases (by their header, whatever the name): a browser of
    // their tables, opened read-only.
    if crate::table::is_sqlite(path) {
        return with(json!({"kind": "table", "sqlite": true}));
    }
    let never = std::sync::atomic::AtomicBool::new(false);
    // Books, DjVu, XPS, PostScript, comics: the cover (or icon) at once,
    // then every page once it's been made into a PDF (Quick Look asks).
    if crate::docconv::kind(&name).is_some() {
        return with(json!({"kind": "thumb", "convert": true}));
    }
    match previews::kind_of(&parents, &name) {
        Kind::Picture => {
            if crate::bridge::ffi::grove_can_read_image(&crate::bridge::ffi::QString::from(path)) {
                return with(json!({"kind": "image", "src": path}));
            }
            // HEIC/AVIF/RAW/PSD...: a big thumbnail (system thumbnailers,
            // vips or ImageMagick), made by the thumbnail provider.
            return with(json!({"kind": "thumb"}));
        }
        Kind::Video | Kind::Audio => {
            // The length (for trimming before the player knows it).
            let secs = crate::media::probe(path).map(|x| x.2).unwrap_or(0.0);
            return with(json!({"kind": "media", "video": m.name.starts_with("video/"), "seconds": secs}));
        }
        Kind::Pdf => return with(pdf_json(p, &never)),
        Kind::Epub | Kind::Comic | Kind::Model => {
            let count = if previews::kind_of(&parents, &name) == Kind::Comic {
                previews::archive_names(p, &never).iter().filter(|n| previews::is_picture_name(n)).count()
            } else {
                0
            };
            return with(json!({"kind": "thumb", "pages": count}));
        }
        Kind::Font => return with(json!({"kind": "font"})),
        Kind::Office => {
            // The whole text is read by Quick Look's TextDoc; here only
            // whether there is any.
            let has_text = previews::office_text(p, 2000, &never).is_some();
            return with(json!({"kind": "office", "hasText": has_text, "canConvert": previews::has_libreoffice()}));
        }
        _ => {}
    }
    // A .torrent: the files it would download (it's bencoded binary).
    if lower.ends_with(".torrent") || m.name == "application/x-bittorrent" {
        if let Some(v) = torrent_json(p) {
            let mut out = with(v.clone());
            out["kindText"] = v["kindText"].clone();
            return out;
        }
    }
    if crate::archive::is_archive(&m.name, &name) {
        let entries = crate::archive::list(path).unwrap_or_default();
        let total = entries.len();
        let shown: Vec<Value> = entries.into_iter().take(500).map(|e| json!({"path": e.path, "size": e.size, "dir": e.is_dir})).collect();
        return with(json!({"kind": "archive", "entries": shown, "total": total}));
    }
    if is_texty(&m.name, &parents) || meta.len() < TEXT_LIMIT as u64 {
        // Only a look at the start (binary or not): Quick Look's TextDoc
        // reads, colours and renders the whole file in blocks.
        let _ = dark;
        if let Some((text, _)) = read_head(p, 8192) {
            if m.name == "text/markdown" || m.name == "text/x-markdown" || is_markdown_name(&lower) {
                return with(json!({"kind": "markdown"}));
            }
            // Web pages: rendered, or their source (DocView's switch).
            if m.name == "text/html" || m.name == "application/xhtml+xml" || lower.ends_with(".html") || lower.ends_with(".htm") || lower.ends_with(".xhtml") {
                return with(json!({"kind": "html"}));
            }
            if is_texty(&m.name, &parents) || !text.is_empty() {
                // A language worked out from the content says so in the footer.
                let mut v = with(json!({"kind": "code"}));
                if !known_by_name(&name, &text) {
                    if let Some(ext) = crate::langguess::guess_ext(path, &text) {
                        let lang = syntax_for(syntaxes(), "", &format!("x.{ext}")).name.clone();
                        if lang != "Plain Text" {
                            v["kindText"] = json!(format!("{lang} (recognised from its contents)"));
                        }
                    }
                }
                return v;
            }
        }
    }
    // Anything else FFmpeg can play (game music, tracker modules, odd
    // containers): the player, as VLC would.
    if let Some((w, _h, secs)) = crate::media::probe(path) {
        if secs > 0.0 {
            return with(json!({"kind": "media", "video": w > 0, "seconds": secs}));
        }
    }
    // Binary data of a type grove can't name: the hex view. A known type
    // (a Flatpak bundle, a disk image...) gets its card, hex on request.
    let unknown = matches!(m.name.as_str(), "application/octet-stream" | "application/x-zerosize" | "") || m.comment.is_empty();
    with(json!({"kind": "other", "hexFirst": unknown}))
}

/// A PDF for Quick Look's page view: page count and sizes (or, without
/// pdftoppm/pdfinfo, QtPdf's own view).
// ---------- .torrent (bencode) ----------

enum Ben {
    Int(i64),
    Bytes(Vec<u8>),
    List(Vec<Ben>),
    Dict(Vec<(Vec<u8>, Ben)>),
}

impl Ben {
    fn get(&self, k: &str) -> Option<&Ben> {
        match self {
            Ben::Dict(d) => d.iter().find(|(key, _)| key == k.as_bytes()).map(|(_, v)| v),
            _ => None,
        }
    }
    fn text(&self) -> Option<String> {
        match self {
            Ben::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        }
    }
    fn int(&self) -> Option<i64> {
        match self {
            Ben::Int(i) => Some(*i),
            _ => None,
        }
    }
}

fn ben_parse(b: &[u8], i: &mut usize, depth: usize) -> Option<Ben> {
    if depth > 64 {
        return None;
    }
    match *b.get(*i)? {
        b'i' => {
            let end = *i + b[*i..].iter().position(|&c| c == b'e')?;
            let n = std::str::from_utf8(&b[*i + 1..end]).ok()?.parse().ok()?;
            *i = end + 1;
            Some(Ben::Int(n))
        }
        b'l' => {
            *i += 1;
            let mut v = vec![];
            while *b.get(*i)? != b'e' {
                v.push(ben_parse(b, i, depth + 1)?);
            }
            *i += 1;
            Some(Ben::List(v))
        }
        b'd' => {
            *i += 1;
            let mut v = vec![];
            while *b.get(*i)? != b'e' {
                let Ben::Bytes(k) = ben_parse(b, i, depth + 1)? else { return None };
                let val = ben_parse(b, i, depth + 1)?;
                v.push((k, val));
            }
            *i += 1;
            Some(Ben::Dict(v))
        }
        b'0'..=b'9' => {
            let colon = *i + b[*i..].iter().position(|&c| c == b':')?;
            let n: usize = std::str::from_utf8(&b[*i..colon]).ok()?.parse().ok()?;
            let start = colon + 1;
            let end = start.checked_add(n)?;
            let bytes = b.get(start..end)?.to_vec();
            *i = end;
            Some(Ben::Bytes(bytes))
        }
        _ => None,
    }
}

/// A torrent's files (as an archive-style listing) and its facts.
fn torrent_json(p: &Path) -> Option<Value> {
    let data = std::fs::read(p).ok()?;
    if data.len() > 64 << 20 {
        return None;
    }
    let mut i = 0;
    let root = ben_parse(&data, &mut i, 0)?;
    let info = root.get("info")?;
    let name = info.get("name.utf-8").or_else(|| info.get("name")).and_then(Ben::text).unwrap_or_default();
    let mut entries = vec![];
    let mut seen_dirs = std::collections::HashSet::new();
    let mut total: i64 = 0;
    match info.get("files") {
        Some(Ben::List(files)) => {
            for f in files {
                let size = f.get("length").and_then(Ben::int).unwrap_or(0);
                total += size;
                let parts: Vec<String> = match f.get("path.utf-8").or_else(|| f.get("path")) {
                    Some(Ben::List(ps)) => ps.iter().filter_map(Ben::text).collect(),
                    _ => vec![],
                };
                // Its folders listed once, before what's in them.
                let mut acc = name.clone();
                for d in parts.iter().take(parts.len().saturating_sub(1)) {
                    acc = format!("{acc}/{d}");
                    if seen_dirs.insert(acc.clone()) {
                        entries.push(json!({"path": acc.clone(), "size": 0, "dir": true}));
                    }
                }
                entries.push(json!({"path": format!("{name}/{}", parts.join("/")), "size": size, "dir": false}));
            }
        }
        _ => {
            total = info.get("length").and_then(Ben::int).unwrap_or(0);
            entries.push(json!({"path": name.clone(), "size": total, "dir": false}));
        }
    }
    let n = entries.iter().filter(|e| e["dir"] == false).count();
    if info.get("files").is_some() {
        entries.insert(0, json!({"path": name.clone(), "size": 0, "dir": true}));
    }
    let shown: Vec<Value> = entries.into_iter().take(2000).collect();
    let trackers = root.get("announce").and_then(Ben::text).unwrap_or_default();
    Some(json!({
        "kind": "archive", "torrent": true, "entries": shown, "total": n,
        "kindText": format!("BitTorrent file — {} {}, {} to download", n, if n == 1 { "file" } else { "files" }, grove_core::util::human_size(total.max(0) as u64)),
        "note": trackers,
    }))
}

pub fn pdf_json(p: &Path, cancel: &std::sync::atomic::AtomicBool) -> Value {
    use grove_core::previews;
    if previews::can_draw_pdf() {
        if let Some(info) = previews::pdf_info(p, 3000, cancel) {
            let sizes: Vec<Value> = info.sizes.iter().map(|(w, h)| json!([w, h])).collect();
            return json!({"kind": "pdf", "paged": true, "src": p, "pages": info.pages, "sizes": sizes, "title": info.title});
        }
    }
    json!({"kind": "pdf", "paged": false, "src": p})
}

/// An office document as a PDF (LibreOffice), for Quick Look.
pub fn office_pdf(path: &str) -> Value {
    let never = std::sync::atomic::AtomicBool::new(false);
    match grove_core::previews::office_pdf(Path::new(path), &never) {
        Some(pdf) => pdf_json(&pdf, &never),
        None => json!({"error": "LibreOffice couldn't convert it"}),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_many_languages() {
        let ss = super::syntaxes();
        let mut bad = vec![];
        for n in ["a.tsh", "b.tessh", "s.ps1", "RUN.BAT", "Makefile", "CMakeLists.txt", "a.toml", "x.nix", "v.vbs", "a.ts", "a.lua", "a.asm", "a.s", "c.ini", "Dockerfile", "a.qml", "a.kt", "a.swift", "a.zig", "a.dart", "a.ex", "a.hs", "a.vue", "a.svelte", "a.cs", "a.fs", "a.vb", "a.ahk", "a.reg", "a.proto", "a.graphql", "a.glsl", "a.cue", "a.srt", "a.diff", "a.r", "a.jl", "a.nim", "a.tcl", "a.pas", "a.f90", "a.cob", "a.v", "a.vhd", "a.scss", "a.less"] {
            let got = &super::syntax_for(ss, "", n).name;
            if got == "Plain Text" { bad.push(n) }
        }
        // Not worth a syntax of their own: cue sheets, subtitles, COBOL.
        assert_eq!(bad, ["a.cue", "a.srt", "a.cob"], "no syntax for {bad:?}");
    }

    #[test]
    fn highlighter_blocks() {
        let mut h = super::Highlighter::new("fn a() {}\n", "a.rs", false);
        let a = h.html("fn a() {\n    1 < 2");
        assert!(a.starts_with("<pre") && a.ends_with("</pre>"));
        assert!(a.contains("&lt;"));
        assert_eq!(a.matches('\n').count(), 1);
        assert!(super::is_markdown_name("readme.markdown"));
    }

    #[test]
    fn tessh_by_name_and_shebang() {
        let ss = super::syntaxes();
        assert_eq!(super::syntax_for(ss, "", "build.tsh").name, "tessh");
        assert_eq!(super::syntax_for(ss, "", "x.tessh").name, "tessh");
        assert_eq!(super::syntax_for(ss, "#!/usr/bin/env tessh\nls\n", "run").name, "tessh");
        let h = super::highlight("name = world\nprint hello $name  # hi\nls -la ~/src | head -3\n", "a.tsh", true);
        assert!(h.matches("<span").count() > 6, "{h}");
    }

    #[test]
    fn highlights_rust() {
        let h = super::highlight("fn main() { let x = 1; }\n", "a.rs", true);
        assert!(h.contains("<span style=\"color:#"));
        assert!(h.contains("main"));
    }
}
