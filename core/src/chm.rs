//! Microsoft Compiled HTML Help (.chm): its pages, in the order of its
//! table of contents, as one HTML document for previews (pictures written
//! to `media`, links between pages turned into links within the document).
//! Read with CHMLib (the `chmlib` crate builds its C source in; LGPL-2.1,
//! grove's source being public, see NOTICE).

use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Pages taken at most, and HTML written at most.
const MAX_PAGES: usize = 400;
const MAX_HTML: usize = 8 << 20;

/// A CHM file by its signature ("ITSF").
pub fn is_chm(path: &Path) -> bool {
    use std::io::Read;
    let mut b = [0u8; 4];
    std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut b)).is_ok() && &b == b"ITSF"
}

struct Chm(chmlib::ChmFile);

impl Chm {
    fn open(path: &Path) -> Option<Chm> {
        if !is_chm(path) {
            return None;
        }
        chmlib::ChmFile::open(path).ok().map(Chm)
    }
    /// Every file in it ("/index.htm", "/images/a.gif"...).
    fn files(&mut self) -> Vec<String> {
        let mut out = vec![];
        let _ = self.0.for_each(chmlib::Filter::NORMAL | chmlib::Filter::FILES, |_, u| {
            if let Some(p) = u.path() {
                out.push(p.to_string_lossy().into_owned());
            }
            chmlib::Continuation::Continue
        });
        out
    }
    fn read(&mut self, name: &str, limit: u64) -> Option<Vec<u8>> {
        let u = self.0.find(name)?;
        let len = u.length().min(limit) as usize;
        let mut buf = vec![0u8; len];
        let n = self.0.read(&u, 0, &mut buf).ok()?;
        buf.truncate(n);
        Some(buf)
    }
}

/// Bytes of a page as text: its <meta charset>, else UTF-8, else Windows-1252
/// (most help files are from Windows).
fn decode(b: &[u8]) -> String {
    let head = String::from_utf8_lossy(&b[..b.len().min(2048)]).to_ascii_lowercase();
    if let Some(i) = head.find("charset=") {
        let label: String = head[i + 8..].trim_start_matches(['"', '\'']).chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
        if let Some(enc) = encoding_rs::Encoding::for_label(label.as_bytes()) {
            return enc.decode(b).0.into_owned();
        }
    }
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::WINDOWS_1252.decode(b).0.into_owned(),
    }
}

/// The attribute `name`'s value in one tag's text (`<param name="Local" value="x.htm">`).
fn attr(tag: &str, name: &str) -> Option<String> {
    let l = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = l[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        // A whole attribute name: not the end of a longer one.
        if at > 0 && l.as_bytes()[at - 1].is_ascii_alphanumeric() {
            continue;
        }
        let rest = l[from..].trim_start();
        if !rest.starts_with('=') {
            continue;
        }
        let vstart = tag.len() - rest.len() + 1;
        let v = tag[vstart..].trim_start();
        let (q, body) = match v.chars().next() {
            Some(c @ ('"' | '\'')) => (Some(c), &v[1..]),
            _ => (None, v),
        };
        let end = match q {
            Some(c) => body.find(c).unwrap_or(body.len()),
            None => body.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(body.len()),
        };
        return Some(html_unescape(&body[..end]));
    }
    None
}

fn html_unescape(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// A path inside the file from a page's reference (relative to the page's
/// folder; "..", "./", "%20", "#anchor" and "?query" handled; other files
/// and web addresses: None).
fn resolve(base_dir: &str, href: &str) -> Option<String> {
    let h = href.trim();
    if h.is_empty() || h.starts_with('#') {
        return None;
    }
    let lower = h.to_ascii_lowercase();
    if lower.contains("://") && !lower.starts_with("ms-its:") && !lower.starts_with("mk:@msitstore:") {
        return None;
    }
    if lower.starts_with("javascript:") || lower.starts_with("mailto:") {
        return None;
    }
    // "ms-its:other.chm::/page.htm": a page of this file, if it names no other.
    let h = match h.find("::") {
        Some(i) => &h[i + 2..],
        None => h,
    };
    let h = h.split(['#', '?']).next().unwrap_or("");
    let h = percent_encoding::percent_decode_str(h).decode_utf8_lossy().replace('\\', "/");
    let joined = if h.starts_with('/') { h.to_string() } else { format!("{base_dir}/{h}") };
    let mut parts: Vec<&str> = vec![];
    for p in joined.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    Some(format!("/{}", parts.join("/")))
}

fn dir_of(p: &str) -> &str {
    p.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

/// The table of contents (.hhc): (depth, title, page), pages in reading order.
fn contents(chm: &mut Chm, files: &[String]) -> Vec<(usize, String, Option<String>)> {
    let Some(hhc) = files.iter().find(|f| f.to_ascii_lowercase().ends_with(".hhc")) else { return vec![] };
    let Some(b) = chm.read(hhc, 16 << 20) else { return vec![] };
    let t = decode(&b);
    let l = t.to_ascii_lowercase();
    let base = dir_of(hhc).to_string();
    let mut out = vec![];
    let mut depth: usize = 0;
    let mut i = 0;
    while i < l.len() {
        let Some(n) = l[i..].find('<') else { break };
        let at = i + n;
        let rest = &l[at..];
        if rest.starts_with("<ul") {
            depth += 1;
        } else if rest.starts_with("</ul") {
            depth = depth.saturating_sub(1);
        } else if rest.starts_with("<object") {
            let end = l[at..].find("</object").map(|e| at + e).unwrap_or(l.len());
            let obj = &t[at..end];
            let lo = &l[at..end];
            let (mut name, mut local) = (String::new(), None);
            let mut j = 0;
            while let Some(p) = lo[j..].find("<param") {
                let s = j + p;
                let e = lo[s..].find('>').map(|e| s + e + 1).unwrap_or(lo.len());
                let tag = &obj[s..e];
                match attr(tag, "name").map(|n| n.to_ascii_lowercase()).as_deref() {
                    Some("name") if name.is_empty() => name = attr(tag, "value").unwrap_or_default(),
                    Some("local") if local.is_none() => local = attr(tag, "value").and_then(|v| resolve(&base, &v)),
                    _ => {}
                }
                j = e;
            }
            if !name.trim().is_empty() || local.is_some() {
                out.push((depth.max(1) - 1, name.trim().to_string(), local));
            }
            i = end;
            continue;
        }
        i = at + 1;
    }
    out
}

/// One page's body, cleaned for the preview: scripts, styles and embedded
/// objects out; pictures written to `media`; links to other pages in the
/// file point at their place in the document (others are left as text).
fn page_body(chm: &mut Chm, page: &str, html: &str, anchors: &HashMap<String, usize>, media: &Path) -> String {
    let l = html.to_ascii_lowercase();
    let start = l.find("<body").and_then(|b| l[b..].find('>').map(|e| b + e + 1)).unwrap_or(0);
    let end = l.rfind("</body").filter(|&e| e >= start).unwrap_or(html.len());
    let (body, lb) = (&html[start..end], &l[start..end]);
    let base = dir_of(page).to_string();
    let mut out = String::with_capacity(body.len());
    let mut i = 0;
    while i < body.len() {
        let Some(n) = lb[i..].find('<') else {
            out.push_str(&body[i..]);
            break;
        };
        let at = i + n;
        out.push_str(&body[i..at]);
        let rest = &lb[at..];
        // Whole elements dropped.
        let mut skipped = false;
        for tag in ["script", "style", "object", "applet", "iframe", "noscript"] {
            if rest.starts_with(&format!("<{tag}")) && rest[tag.len() + 1..].starts_with([' ', '>', '\t', '\n', '\r', '/']) {
                let close = format!("</{tag}");
                i = lb[at..].find(&close).and_then(|c| lb[at + c..].find('>').map(|e| at + c + e + 1)).unwrap_or(body.len());
                skipped = true;
                break;
            }
        }
        if skipped {
            continue;
        }
        let e = lb[at..].find('>').map(|e| at + e + 1).unwrap_or(body.len());
        let tag = &body[at..e];
        let lt = &lb[at..e];
        if lt.starts_with("<img") {
            match attr(tag, "src").and_then(|s| resolve(&base, &s)).and_then(|p| picture(chm, &p, media)) {
                Some(url) => {
                    let alt = attr(tag, "alt").unwrap_or_default();
                    let _ = write!(out, "<img src=\"{}\" alt=\"{}\">", esc(&url).replace('"', "&quot;"), esc(&alt).replace('"', "&quot;"));
                }
                None => {}
            }
        } else if lt.starts_with("<a ") || lt.starts_with("<a\t") || lt.starts_with("<a\n") {
            let href = attr(tag, "href").unwrap_or_default();
            let hl = href.to_ascii_lowercase();
            if hl.starts_with("http://") || hl.starts_with("https://") || hl.starts_with("mailto:") {
                let _ = write!(out, "<a href=\"{}\">", esc(&href).replace('"', "&quot;"));
            } else if let Some(n) = resolve(&base, &href).and_then(|p| anchors.get(&p.to_ascii_lowercase())) {
                let _ = write!(out, "<a href=\"#chm{n}\">");
            } else if let Some(name) = attr(tag, "name") {
                let _ = write!(out, "<a name=\"{}\">", esc(&name).replace('"', "&quot;"));
            } else {
                out.push_str("<a>");
            }
        } else if lt.starts_with("<link") || lt.starts_with("<meta") || lt.starts_with("<!--") || lt.starts_with("<param") || lt.starts_with("<embed") {
            // (comments: up to their end)
            if lt.starts_with("<!--") {
                let ce = lb[at..].find("-->").map(|c| at + c + 3).unwrap_or(body.len());
                i = ce;
                continue;
            }
        } else {
            out.push_str(tag);
        }
        i = e;
    }
    out
}

/// A picture of the file, written to `media` (once): its file URL.
fn picture(chm: &mut Chm, inner: &str, media: &Path) -> Option<String> {
    let lower = inner.to_ascii_lowercase();
    if !["png", "gif", "jpg", "jpeg", "bmp", "ico", "svg", "webp"].iter().any(|e| lower.ends_with(&format!(".{e}"))) {
        return None;
    }
    let name: String = inner.trim_start_matches('/').chars().map(|c| if c == '/' || c == '\\' { '_' } else { c }).collect();
    let out = media.join(name);
    if !out.is_file() {
        let b = chm.read(inner, 32 << 20)?;
        std::fs::create_dir_all(media).ok()?;
        std::fs::write(&out, b).ok()?;
    }
    Some(crate::util::file_uri(&out))
}

/// The pages to show, in order: the contents' pages, then (when there's no
/// contents) every page, by name.
fn pages(chm: &mut Chm, files: &[String], toc: &[(usize, String, Option<String>)]) -> Vec<String> {
    let lower_files: HashMap<String, &String> = files.iter().map(|f| (f.to_ascii_lowercase(), f)).collect();
    let mut seen = std::collections::HashSet::new();
    let mut out = vec![];
    for (_, _, p) in toc {
        if let Some(p) = p {
            if let Some(f) = lower_files.get(&p.to_ascii_lowercase()) {
                if seen.insert(f.to_ascii_lowercase()) {
                    out.push((*f).clone());
                }
            }
        }
    }
    if out.is_empty() {
        let mut all: Vec<String> = files.iter().filter(|f| { let l = f.to_ascii_lowercase(); (l.ends_with(".htm") || l.ends_with(".html")) && !l.starts_with("/#") && !l.starts_with("/$") }).cloned().collect();
        all.sort_by(|a, b| crate::util::natural_cmp(a, b));
        out = all;
    }
    let _ = chm;
    out.truncate(MAX_PAGES);
    out
}

/// The help file's title (#SYSTEM's title, else the contents' first entry).
fn title(chm: &mut Chm) -> Option<String> {
    let b = chm.read("/#SYSTEM", 1 << 20)?;
    // Records after a 4-byte version: code (u16), length (u16), data; code 3 is the title.
    let mut i = 4;
    while i + 4 <= b.len() {
        let code = u16::from_le_bytes([b[i], b[i + 1]]);
        let len = u16::from_le_bytes([b[i + 2], b[i + 3]]) as usize;
        let d = b.get(i + 4..i + 4 + len)?;
        if code == 3 {
            let t = decode(d.split(|&c| c == 0).next().unwrap_or(d));
            let t = t.trim().to_string();
            return (!t.is_empty()).then_some(t);
        }
        i += 4 + len;
    }
    None
}

/// The whole help file as one HTML document: its title, its contents
/// (linked), then every page.
pub fn html(path: &Path, media: &Path, cancel: &AtomicBool) -> Option<String> {
    let mut chm = Chm::open(path)?;
    let files = chm.files();
    let toc = contents(&mut chm, &files);
    let pages = pages(&mut chm, &files, &toc);
    if pages.is_empty() {
        return None;
    }
    let anchors: HashMap<String, usize> = pages.iter().enumerate().map(|(i, p)| (p.to_ascii_lowercase(), i)).collect();
    let mut out = String::new();
    if let Some(t) = title(&mut chm) {
        let _ = writeln!(out, "<h1>{}</h1>", esc(&t));
    }
    if !toc.is_empty() {
        out.push_str("<h2>Contents</h2>\n");
        for (depth, name, page) in toc.iter().take(2000) {
            let indent = "&emsp;".repeat(*depth);
            match page.as_ref().and_then(|p| anchors.get(&p.to_ascii_lowercase())) {
                Some(n) => { let _ = writeln!(out, "<p style=\"margin:0\">{indent}<a href=\"#chm{n}\">{}</a></p>", esc(name)); }
                None => { let _ = writeln!(out, "<p style=\"margin:0\">{indent}<b>{}</b></p>", esc(name)); }
            }
        }
        out.push_str("<hr>\n");
    }
    for (n, page) in pages.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) || out.len() > MAX_HTML {
            break;
        }
        let Some(b) = chm.read(page, 8 << 20) else { continue };
        let text = decode(&b);
        let _ = writeln!(out, "<a name=\"chm{n}\"></a>");
        out.push_str(&page_body(&mut chm, page, &text, &anchors, media));
        out.push_str("\n<hr>\n");
    }
    Some(out)
}

/// The text of the help file (for searching and the short preview).
pub fn text(path: &Path, limit: usize) -> Option<String> {
    let mut chm = Chm::open(path)?;
    let files = chm.files();
    let toc = contents(&mut chm, &files);
    let pages = pages(&mut chm, &files, &toc);
    let mut out = String::new();
    for page in pages {
        if out.len() >= limit {
            break;
        }
        let Some(b) = chm.read(&page, 8 << 20) else { continue };
        let html = decode(&b);
        let mut in_tag = false;
        let mut skip = 0usize;
        let l = html.to_ascii_lowercase();
        let mut prev_space = true;
        for (i, c) in html.char_indices() {
            if c == '<' {
                in_tag = true;
                if l[i..].starts_with("<script") || l[i..].starts_with("<style") {
                    skip += 1;
                } else if (l[i..].starts_with("</script") || l[i..].starts_with("</style")) && skip > 0 {
                    skip -= 1;
                } else if l[i..].starts_with("<p") || l[i..].starts_with("<br") || l[i..].starts_with("<h") || l[i..].starts_with("<li") || l[i..].starts_with("<tr") {
                    out.push('\n');
                    prev_space = true;
                }
                continue;
            }
            if c == '>' {
                in_tag = false;
                continue;
            }
            if in_tag || skip > 0 {
                continue;
            }
            if c.is_whitespace() {
                if !prev_space {
                    out.push(' ');
                    prev_space = true;
                }
            } else {
                out.push(c);
                prev_space = false;
            }
        }
        out.push('\n');
    }
    let out = html_unescape(&out).replace("&nbsp;", " ");
    Some(out.chars().take(limit).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_and_attrs() {
        assert_eq!(resolve("/html", "../img/a b.gif").as_deref(), Some("/img/a b.gif"));
        assert_eq!(resolve("/html", "page.htm#top").as_deref(), Some("/html/page.htm"));
        assert_eq!(resolve("", "ms-its:x.chm::/a/b.htm").as_deref(), Some("/a/b.htm"));
        assert_eq!(resolve("/x", "http://example.com/"), None);
        assert_eq!(attr(r#"<param name="Local" value="a&amp;b.htm">"#, "value").as_deref(), Some("a&b.htm"));
        assert_eq!(attr("<img SRC=pic.gif alt='x'>", "src").as_deref(), Some("pic.gif"));
    }
}
