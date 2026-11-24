//! Office documents as HTML for reading (no pandoc): Word (.docx/.docm/
//! .dotx), OpenDocument text (.odt), PowerPoint (.pptx, its slides' text)
//! and RTF (its paragraphs). Headings, paragraphs, bold/italic/underline/
//! strikethrough, bulleted and numbered lists, tables, links and pictures
//! (written to `media`, referenced by file URL). One top-level element per
//! line, as pandoc's output, so callers can cut it into blocks.

use quick_xml::events::{BytesStart, Event};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn attr(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.as_ref() == name).map(|a| a.unescape_value().map(|v| v.into_owned()).unwrap_or_default())
}

/// The local part of a tag name ("w:p" -> "p").
fn local(n: &[u8]) -> &[u8] {
    n.iter().rposition(|&c| c == b':').map(|i| &n[i + 1..]).unwrap_or(n)
}

fn member(path: &Path, name: &str, cancel: &AtomicBool) -> Option<String> {
    crate::archives::read_member(path, name, 256 << 20, cancel).map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// A picture inside the package, written to `media` (once): its file URL.
fn picture(path: &Path, inner: &str, media: &Path, cancel: &AtomicBool) -> Option<String> {
    let name = inner.rsplit('/').next()?;
    let out = media.join(name);
    if !out.is_file() {
        let b = crate::archives::read_member(path, inner, 64 << 20, cancel)?;
        std::fs::create_dir_all(media).ok()?;
        std::fs::write(&out, b).ok()?;
    }
    Some(crate::util::file_uri(&out))
}

/// Lists open at each level, closed as paragraphs leave them.
#[derive(Default)]
struct Lists {
    open: Vec<&'static str>,
}
impl Lists {
    fn to(&mut self, out: &mut String, depth: usize, kind: &'static str) {
        while self.open.len() > depth || (self.open.len() == depth && depth > 0 && self.open[depth - 1] != kind) {
            let k = self.open.pop().unwrap();
            let _ = write!(out, "</{k}>");
        }
        while self.open.len() < depth {
            let _ = write!(out, "<{kind}>");
            self.open.push(kind);
        }
        if self.open.is_empty() {
            out.push('\n');
        }
    }
}

// ---------------------------------------------------------------- Word

pub fn docx(path: &Path, media: &Path, cancel: &AtomicBool) -> Option<String> {
    let doc = member(path, "word/document.xml", cancel)?;
    // Relationships: pictures and links.
    let mut rels: HashMap<String, String> = HashMap::new();
    if let Some(r) = member(path, "word/_rels/document.xml.rels", cancel) {
        let mut x = quick_xml::Reader::from_str(&r);
        loop {
            match x.read_event() {
                Ok(Event::Empty(e)) | Ok(Event::Start(e)) if local(e.name().as_ref()) == b"Relationship" => {
                    if let (Some(id), Some(t)) = (attr(&e, b"Id"), attr(&e, b"Target")) {
                        rels.insert(id, t);
                    }
                }
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
    }
    // Heading styles: style id -> level ("Heading2" / "heading 2" / Title).
    let mut heading: HashMap<String, usize> = HashMap::new();
    if let Some(st) = member(path, "word/styles.xml", cancel) {
        let mut x = quick_xml::Reader::from_str(&st);
        let mut cur = String::new();
        loop {
            match x.read_event() {
                Ok(Event::Start(e)) if local(e.name().as_ref()) == b"style" => cur = attr(&e, b"w:styleId").unwrap_or_default(),
                Ok(Event::Empty(e)) if local(e.name().as_ref()) == b"name" && !cur.is_empty() => {
                    let n = attr(&e, b"w:val").unwrap_or_default().to_lowercase();
                    if n == "title" {
                        heading.insert(cur.clone(), 1);
                    } else if let Some(l) = n.strip_prefix("heading ").and_then(|l| l.trim().parse::<usize>().ok()) {
                        heading.insert(cur.clone(), l.clamp(1, 6));
                    }
                }
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
    }
    // Numbering: numId -> bullet or number (first level's format).
    let mut bullets: HashMap<String, bool> = HashMap::new();
    if let Some(n) = member(path, "word/numbering.xml", cancel) {
        let mut abs: HashMap<String, bool> = HashMap::new();
        let mut x = quick_xml::Reader::from_str(&n);
        let (mut cur_abs, mut cur_num, mut first_fmt) = (String::new(), String::new(), true);
        loop {
            match x.read_event() {
                Ok(Event::Start(e)) if local(e.name().as_ref()) == b"abstractNum" => {
                    cur_abs = attr(&e, b"w:abstractNumId").unwrap_or_default();
                    first_fmt = true;
                }
                Ok(Event::Empty(e)) if local(e.name().as_ref()) == b"numFmt" && first_fmt && !cur_abs.is_empty() => {
                    abs.insert(cur_abs.clone(), attr(&e, b"w:val").as_deref() == Some("bullet"));
                    first_fmt = false;
                }
                Ok(Event::Start(e)) if local(e.name().as_ref()) == b"num" => cur_num = attr(&e, b"w:numId").unwrap_or_default(),
                Ok(Event::Empty(e)) if local(e.name().as_ref()) == b"abstractNumId" && !cur_num.is_empty() => {
                    let a = attr(&e, b"w:val").unwrap_or_default();
                    bullets.insert(cur_num.clone(), *abs.get(&a).unwrap_or(&true));
                }
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
    }

    let mut out = String::new();
    let mut lists = Lists::default();
    let mut x = quick_xml::Reader::from_str(&doc);
    // Paragraph state.
    let mut para = String::new();
    let (mut level, mut list_depth, mut list_bullet) = (0usize, 0usize, true);
    // Run formatting.
    let (mut b, mut i, mut u, mut s) = (false, false, false, false);
    let mut in_rpr = false;
    let mut in_text = false;
    let mut link: Option<String> = None;
    // Tables: cells collect their paragraphs.
    let mut table_depth = 0usize;
    let mut cell = String::new();
    let mut in_cell = false;
    let flag = |e: &BytesStart| !matches!(attr(e, b"w:val").as_deref(), Some("0" | "false" | "none"));
    let run_open = |b: bool, i: bool, u: bool, s: bool| {
        let mut t = String::new();
        if b {
            t.push_str("<b>");
        }
        if i {
            t.push_str("<i>");
        }
        if u {
            t.push_str("<u>");
        }
        if s {
            t.push_str("<s>");
        }
        t
    };
    let run_close = |b: bool, i: bool, u: bool, s: bool| {
        let mut t = String::new();
        if s {
            t.push_str("</s>");
        }
        if u {
            t.push_str("</u>");
        }
        if i {
            t.push_str("</i>");
        }
        if b {
            t.push_str("</b>");
        }
        t
    };
    let mut run_text = String::new();
    loop {
        let ev = match x.read_event() {
            Ok(e) => e,
            Err(_) => break,
        };
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let e = e.clone();
                match local(e.name().as_ref()) {
                    b"p" => {
                        para.clear();
                        level = 0;
                        list_depth = 0;
                    }
                    b"pStyle" => level = attr(&e, b"w:val").and_then(|v| heading.get(&v).copied()).unwrap_or(0),
                    b"ilvl" => list_depth = attr(&e, b"w:val").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0) + 1,
                    b"numId" => {
                        let id = attr(&e, b"w:val").unwrap_or_default();
                        if id == "0" {
                            list_depth = 0;
                        } else {
                            list_bullet = *bullets.get(&id).unwrap_or(&true);
                            if list_depth == 0 {
                                list_depth = 1;
                            }
                        }
                    }
                    b"r" => {
                        b = false;
                        i = false;
                        u = false;
                        s = false;
                        run_text.clear();
                    }
                    b"rPr" => in_rpr = true,
                    b"b" if in_rpr => b = flag(&e),
                    b"i" if in_rpr => i = flag(&e),
                    b"u" if in_rpr => u = flag(&e),
                    b"strike" | b"dstrike" if in_rpr => s = flag(&e),
                    b"t" => in_text = true,
                    b"tab" if !in_rpr => run_text.push_str("&emsp;"),
                    b"br" | b"cr" => run_text.push_str("<br/>"),
                    b"blip" => {
                        if let Some(t) = attr(&e, b"r:embed").and_then(|id| rels.get(&id).cloned()) {
                            let inner = if t.starts_with('/') { t.trim_start_matches('/').to_string() } else { format!("word/{t}") };
                            if let Some(url) = picture(path, &inner, media, cancel) {
                                let _ = write!(run_text, "<img src=\"{}\" style=\"max-width:100%\"/>", esc(&url));
                            }
                        }
                    }
                    b"hyperlink" => {
                        link = attr(&e, b"r:id").and_then(|id| rels.get(&id).cloned());
                        if let Some(l) = &link {
                            let _ = write!(para, "<a href=\"{}\">", esc(l));
                        }
                    }
                    b"tbl" => {
                        if table_depth == 0 {
                            lists.to(&mut out, 0, "ul");
                            out.push_str("<table>");
                        }
                        table_depth += 1;
                    }
                    b"tr" if table_depth == 1 => out.push_str("<tr>"),
                    b"tc" if table_depth == 1 => {
                        cell.clear();
                        in_cell = true;
                    }
                    _ => {}
                }
            }
            Event::Text(t) if in_text => {
                if let Ok(v) = t.unescape() {
                    run_text.push_str(&esc(&v));
                }
            }
            Event::End(e) => match local(e.name().as_ref()) {
                b"t" => in_text = false,
                b"rPr" => in_rpr = false,
                b"r" => {
                    if !run_text.is_empty() {
                        para.push_str(&run_open(b, i, u, s));
                        para.push_str(&run_text);
                        para.push_str(&run_close(b, i, u, s));
                    }
                    run_text.clear();
                }
                b"hyperlink" => {
                    if link.take().is_some() {
                        para.push_str("</a>");
                    }
                }
                b"p" => {
                    if in_cell {
                        if !cell.is_empty() {
                            cell.push_str("<br/>");
                        }
                        cell.push_str(&para);
                    } else if table_depth == 0 {
                        if list_depth > 0 {
                            lists.to(&mut out, list_depth, if list_bullet { "ul" } else { "ol" });
                            let _ = write!(out, "<li>{para}</li>");
                        } else {
                            lists.to(&mut out, 0, "ul");
                            if level > 0 {
                                let _ = writeln!(out, "<h{level}>{para}</h{level}>");
                            } else if !para.trim().is_empty() {
                                let _ = writeln!(out, "<p>{para}</p>");
                            }
                        }
                    }
                    para.clear();
                }
                b"tc" if table_depth == 1 => {
                    let _ = write!(out, "<td>{cell}</td>");
                    in_cell = false;
                }
                b"tr" if table_depth == 1 => out.push_str("</tr>"),
                b"tbl" => {
                    table_depth = table_depth.saturating_sub(1);
                    if table_depth == 0 {
                        out.push_str("</table>\n");
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    lists.to(&mut out, 0, "ul");
    Some(out)
}

// ---------------------------------------------------------------- OpenDocument

pub fn odt(path: &Path, media: &Path, cancel: &AtomicBool) -> Option<String> {
    let xml = member(path, "content.xml", cancel)?;
    // Automatic styles: name -> (bold, italic, underline, strike).
    let mut styles: HashMap<String, (bool, bool, bool, bool)> = HashMap::new();
    {
        let mut x = quick_xml::Reader::from_str(&xml);
        let mut cur = String::new();
        loop {
            match x.read_event() {
                Ok(Event::Start(e)) if local(e.name().as_ref()) == b"style" => cur = attr(&e, b"style:name").unwrap_or_default(),
                Ok(Event::Empty(e)) | Ok(Event::Start(e)) if local(e.name().as_ref()) == b"text-properties" && !cur.is_empty() => {
                    let bold = attr(&e, b"fo:font-weight").is_some_and(|w| w == "bold" || w.parse::<u32>().is_ok_and(|n| n >= 600));
                    let ital = attr(&e, b"fo:font-style").is_some_and(|s| s == "italic" || s == "oblique");
                    let und = attr(&e, b"style:text-underline-style").is_some_and(|s| s != "none");
                    let strike = attr(&e, b"style:text-line-through-style").is_some_and(|s| s != "none");
                    styles.insert(cur.clone(), (bold, ital, und, strike));
                }
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
    }
    let mut out = String::new();
    let mut x = quick_xml::Reader::from_str(&xml);
    let mut in_body = false;
    // The open blocks' text, innermost last.
    let mut para: Option<(String, usize)> = None; // (html, heading level)
    let mut spans: Vec<String> = Vec::new(); // closing tags per open span
    let mut list_depth = 0usize;
    let mut table_depth = 0usize;
    let mut cell: Option<String> = None;
    loop {
        let ev = match x.read_event() {
            Ok(e) => e,
            Err(_) => break,
        };
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev, Event::Empty(_));
                let n = local(e.name().as_ref()).to_vec();
                if n == b"text" && e.name().as_ref() == b"office:text" {
                    in_body = true;
                    continue;
                }
                if !in_body {
                    continue;
                }
                match n.as_slice() {
                    b"h" if !empty => para = Some((String::new(), attr(e, b"text:outline-level").and_then(|l| l.parse().ok()).unwrap_or(1).clamp(1, 6))),
                    b"p" if !empty => para = Some((String::new(), 0)),
                    b"span" if !empty => {
                        let (bo, it, un, st) = attr(e, b"text:style-name").and_then(|s| styles.get(&s).copied()).unwrap_or_default();
                        let mut open = String::new();
                        let mut close = String::new();
                        for (on, o, c) in [(bo, "<b>", "</b>"), (it, "<i>", "</i>"), (un, "<u>", "</u>"), (st, "<s>", "</s>")] {
                            if on {
                                open.push_str(o);
                                close.insert_str(0, c);
                            }
                        }
                        if let Some((p, _)) = para.as_mut() {
                            p.push_str(&open);
                        }
                        spans.push(close);
                    }
                    b"a" if !empty => {
                        if let (Some((p, _)), Some(h)) = (para.as_mut(), attr(e, b"xlink:href")) {
                            let _ = write!(p, "<a href=\"{}\">", esc(&h));
                            spans.push("</a>".into());
                        } else {
                            spans.push(String::new());
                        }
                    }
                    b"s" => {
                        let c: usize = attr(e, b"text:c").and_then(|c| c.parse().ok()).unwrap_or(1);
                        if let Some((p, _)) = para.as_mut() {
                            p.push_str(&"&nbsp;".repeat(c.min(64)));
                        }
                    }
                    b"tab" => {
                        if let Some((p, _)) = para.as_mut() {
                            p.push_str("&emsp;");
                        }
                    }
                    b"line-break" => {
                        if let Some((p, _)) = para.as_mut() {
                            p.push_str("<br/>");
                        }
                    }
                    b"image" => {
                        if let Some(h) = attr(e, b"xlink:href") {
                            if let Some(url) = picture(path, &h, media, cancel) {
                                let img = format!("<img src=\"{}\" style=\"max-width:100%\"/>", esc(&url));
                                match para.as_mut() {
                                    Some((p, _)) => p.push_str(&img),
                                    None => {
                                        let _ = writeln!(out, "<p>{img}</p>");
                                    }
                                }
                            }
                        }
                    }
                    b"list" if !empty => {
                        if list_depth == 0 && table_depth == 0 {
                            out.push('\n');
                        }
                        list_depth += 1;
                        out.push_str("<ul>");
                    }
                    b"list-item" if !empty => out.push_str("<li>"),
                    b"table" if !empty && e.name().as_ref() == b"table:table" => {
                        if table_depth == 0 {
                            out.push_str("<table>");
                        }
                        table_depth += 1;
                    }
                    b"table-row" if !empty && table_depth == 1 => out.push_str("<tr>"),
                    b"table-cell" if table_depth == 1 => {
                        if empty {
                            out.push_str("<td></td>");
                        } else {
                            cell = Some(String::new());
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(t) if in_body => {
                if let (Some((p, _)), Ok(v)) = (para.as_mut(), t.unescape()) {
                    p.push_str(&esc(&v));
                }
            }
            Event::End(e) => {
                let n = local(e.name().as_ref()).to_vec();
                if e.name().as_ref() == b"office:text" {
                    in_body = false;
                    continue;
                }
                if !in_body {
                    continue;
                }
                match n.as_slice() {
                    b"span" | b"a" => {
                        let c = spans.pop().unwrap_or_default();
                        if let Some((p, _)) = para.as_mut() {
                            p.push_str(&c);
                        }
                    }
                    b"h" | b"p" => {
                        if let Some((p, lvl)) = para.take() {
                            if let Some(c) = cell.as_mut() {
                                if !c.is_empty() {
                                    c.push_str("<br/>");
                                }
                                c.push_str(&p);
                            } else if list_depth > 0 {
                                out.push_str(&p);
                            } else if lvl > 0 {
                                let _ = writeln!(out, "<h{lvl}>{p}</h{lvl}>");
                            } else if !p.trim().is_empty() {
                                let _ = writeln!(out, "<p>{p}</p>");
                            }
                        }
                    }
                    b"list-item" => out.push_str("</li>"),
                    b"list" => {
                        out.push_str("</ul>");
                        list_depth = list_depth.saturating_sub(1);
                        if list_depth == 0 && table_depth == 0 {
                            out.push('\n');
                        }
                    }
                    b"table-cell" if table_depth == 1 => {
                        let _ = write!(out, "<td>{}</td>", cell.take().unwrap_or_default());
                    }
                    b"table-row" if table_depth == 1 => out.push_str("</tr>"),
                    b"table" if e.name().as_ref() == b"table:table" => {
                        table_depth = table_depth.saturating_sub(1);
                        if table_depth == 0 {
                            out.push_str("</table>\n");
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Some(out)
}

// ---------------------------------------------------------------- PowerPoint

/// Each slide's text: a heading per slide, its paragraphs under it.
pub fn pptx(path: &Path, cancel: &AtomicBool) -> Option<String> {
    let names = crate::archives::names(path, cancel);
    let mut slides: Vec<&String> = names.iter().filter(|n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml")).collect();
    slides.sort_by(|a, b| crate::util::natural_cmp(a, b));
    let mut out = String::new();
    for (k, s) in slides.iter().enumerate() {
        let xml = member(path, s, cancel)?;
        let _ = writeln!(out, "<h2>Slide {}</h2>", k + 1);
        let mut x = quick_xml::Reader::from_str(&xml);
        let (mut para, mut in_t) = (String::new(), false);
        loop {
            match x.read_event() {
                Ok(Event::Start(e)) if local(e.name().as_ref()) == b"t" => in_t = true,
                Ok(Event::Text(t)) if in_t => para.push_str(&esc(&t.unescape().unwrap_or_default())),
                Ok(Event::End(e)) => match local(e.name().as_ref()) {
                    b"t" => in_t = false,
                    b"p" => {
                        if !para.trim().is_empty() {
                            let _ = writeln!(out, "<p>{para}</p>");
                        }
                        para.clear();
                    }
                    _ => {}
                },
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
    }
    Some(out)
}

// ---------------------------------------------------------------- RTF

/// RTF's text as paragraphs (control words dropped; \par splits).
pub fn rtf(path: &Path) -> Option<String> {
    let b = std::fs::read(path).ok()?;
    let t = String::from_utf8_lossy(&b);
    let mut out = String::new();
    let mut para = String::new();
    // Groups whose text isn't the document's (font table, styles...).
    let mut skip: Vec<bool> = vec![false];
    let mut chars = t.chars().peekable();
    let mut uc_skip = 0usize;
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                let parent = *skip.last().unwrap_or(&false);
                skip.push(parent);
            }
            '}' => {
                skip.pop();
            }
            '\\' => {
                let mut word = String::new();
                while let Some(&n) = chars.peek() {
                    if n.is_ascii_alphabetic() {
                        word.push(n);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if word.is_empty() {
                    match chars.next() {
                        Some('*') => {
                            if let Some(s) = skip.last_mut() {
                                *s = true;
                            }
                        }
                        Some('\'') => {
                            let h: String = chars.by_ref().take(2).collect();
                            if let (Ok(v), false) = (u8::from_str_radix(&h, 16), *skip.last().unwrap_or(&false)) {
                                para.push(v as char);
                            }
                        }
                        Some(ch @ ('\\' | '{' | '}')) if !*skip.last().unwrap_or(&false) => para.push(ch),
                        _ => {}
                    }
                    continue;
                }
                let mut num = String::new();
                while let Some(&n) = chars.peek() {
                    if n.is_ascii_digit() || (n == '-' && num.is_empty()) {
                        num.push(n);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if chars.peek() == Some(&' ') {
                    chars.next();
                }
                match word.as_str() {
                    "fonttbl" | "colortbl" | "stylesheet" | "info" | "pict" | "header" | "footer" | "listtable" | "listoverridetable" => {
                        if let Some(s) = skip.last_mut() {
                            *s = true;
                        }
                    }
                    "par" | "line" if !*skip.last().unwrap_or(&false) => {
                        if !para.trim().is_empty() {
                            let _ = writeln!(out, "<p>{}</p>", esc(&para));
                        }
                        para.clear();
                    }
                    "tab" => para.push('\t'),
                    "uc" => uc_skip = num.parse().unwrap_or(1),
                    "u" if !*skip.last().unwrap_or(&false) => {
                        if let Some(ch) = num.parse::<i32>().ok().map(|n| if n < 0 { n + 65536 } else { n }).and_then(|n| char::from_u32(n as u32)) {
                            para.push(ch);
                        }
                        for _ in 0..uc_skip.max(1) {
                            chars.next();
                        }
                    }
                    _ => {}
                }
            }
            '\r' | '\n' => {}
            _ if !*skip.last().unwrap_or(&false) => para.push(c),
            _ => {}
        }
    }
    if !para.trim().is_empty() {
        let _ = writeln!(out, "<p>{}</p>", esc(&para));
    }
    Some(out)
}

/// The HTML for `path` by its extension (None: not one of these).
pub fn convert(path: &Path, media: &PathBuf, cancel: &AtomicBool) -> Option<String> {
    let l = path.to_string_lossy().to_lowercase();
    let ext = l.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match ext {
        "docx" | "docm" | "dotx" | "dotm" => docx(path, media, cancel),
        "odt" | "ott" | "fodt" => odt(path, media, cancel),
        "pptx" | "pptm" => pptx(path, cancel),
        "rtf" => rtf(path),
        "hlp" => crate::winhelp::html(path),
        "chm" => crate::chm::html(path, media, cancel),
        "doc" | "dot" => {
            // Old Word: its text, a paragraph per line (worddoc.rs).
            let t = crate::worddoc::text(path).ok()?;
            let mut out = String::new();
            for para in t.split('\n') {
                if para.trim().is_empty() {
                    continue;
                }
                let cells: Vec<&str> = para.split('\t').collect();
                let _ = writeln!(out, "<p>{}</p>", cells.iter().map(|c| esc(c)).collect::<Vec<_>>().join("&emsp;"));
            }
            Some(out)
        }
        _ => None,
    }
}
