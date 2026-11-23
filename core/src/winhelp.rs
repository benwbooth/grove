//! WinHelp (.hlp, Windows 3.x/95 help) read in Rust (libhlp: its file
//! system, LZ77 and phrase compression): every topic as a page, with
//! headings, bold/italic/underline, tables. And ".hlp" files that are
//! plain text (S-Lang's, many DOS programs').

use std::fmt::Write as _;
use std::path::Path;

/// A real WinHelp file (by its magic number, 0x00035F3F).
pub fn is_winhelp(path: &Path) -> bool {
    let mut b = [0u8; 4];
    std::fs::File::open(path).and_then(|mut f| std::io::Read::read_exact(&mut f, &mut b)).is_ok() && b == [0x3F, 0x5F, 0x03, 0x00]
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn paragraph_html(p: &libhlp::Paragraph) -> String {
    let mut out = String::new();
    for run in &p.runs {
        match run {
            libhlp::Run::Text { text, format, link } => {
                let mut open = String::new();
                let mut close = String::new();
                for (on, o, c) in [(format.bold, "<b>", "</b>"), (format.italic, "<i>", "</i>"), (format.underline || link.is_some(), "<u>", "</u>"), (format.strikeout, "<s>", "</s>")] {
                    if on {
                        open.push_str(o);
                        close.insert_str(0, c);
                    }
                }
                // Jumps show in the help's link colour.
                if link.is_some() {
                    open.insert_str(0, "<span style=\"color:#2e8b57\">");
                    close.push_str("</span>");
                }
                let _ = write!(out, "{open}{}{close}", esc(text));
            }
            libhlp::Run::Tab => out.push_str("&emsp;"),
            libhlp::Run::LineBreak => out.push_str("<br/>"),
            libhlp::Run::Image(_) => {}
        }
    }
    out
}

/// The whole help file as HTML: its title, then each topic (a heading and
/// its paragraphs and tables), one top-level element per line.
pub fn html(path: &Path) -> Option<String> {
    let hlp = libhlp::HlpFile::open(path).ok()?;
    let mut out = String::new();
    if let Some(t) = hlp.title().filter(|t| !t.trim().is_empty()) {
        let _ = writeln!(out, "<h1>{}</h1>", esc(t));
    }
    for topic in hlp.topics().ok()? {
        if !topic.title.trim().is_empty() {
            let _ = writeln!(out, "<h2>{}</h2>", esc(&topic.title));
        }
        for block in &topic.blocks {
            match block {
                libhlp::Block::Paragraph(p) => {
                    let h = paragraph_html(p);
                    if !h.trim().is_empty() {
                        let _ = writeln!(out, "<p>{h}</p>");
                    }
                }
                libhlp::Block::Table(t) => {
                    out.push_str("<table>");
                    for row in &t.rows {
                        out.push_str("<tr>");
                        for cell in &row.cells {
                            let inner: Vec<String> = cell.paragraphs.iter().map(paragraph_html).collect();
                            let _ = write!(out, "<td>{}</td>", inner.join("<br/>"));
                        }
                        out.push_str("</tr>");
                    }
                    out.push_str("</table>\n");
                }
            }
        }
        out.push_str("<hr/>\n");
    }
    Some(out)
}

/// The help file's plain text (for "has text" and search).
pub fn text(path: &Path) -> Option<String> {
    let hlp = libhlp::HlpFile::open(path).ok()?;
    let mut out = String::new();
    for topic in hlp.topics().ok()? {
        out.push_str(&topic.title);
        out.push('\n');
        out.push_str(&topic.text());
        out.push_str("\n\n");
    }
    (!out.trim().is_empty()).then_some(out)
}
