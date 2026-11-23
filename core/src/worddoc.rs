//! Old Word documents' text, read in Rust (no LibreOffice, antiword or
//! catdoc): Word 97-2003 (OLE, piece table), Word 6/95, Word for Windows
//! 1/2, Word for DOS and Write. And ".doc" files that are plain text, as
//! DOS-era manuals often are (HELPME.DOC).

use std::io::Read;
use std::path::Path;

/// What a .doc file is.
#[derive(Debug, PartialEq)]
pub enum Kind {
    Ole,
    WinWord12,
    DosWord,
    PlainText,
    Unknown,
}

fn head(path: &Path, n: usize) -> Option<Vec<u8>> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut b = vec![0u8; n];
    let mut got = 0;
    while got < n {
        match f.read(&mut b[got..]) {
            Ok(0) | Err(_) => break,
            Ok(k) => got += k,
        }
    }
    b.truncate(got);
    Some(b)
}

pub fn kind(path: &Path) -> Kind {
    let Some(b) = head(path, 8192) else { return Kind::Unknown };
    if b.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        Kind::Ole
    } else if b.starts_with(&[0xDB, 0xA5]) || b.starts_with(&[0x9B, 0xA5]) {
        Kind::WinWord12
    } else if b.len() >= 4 && (b[0] == 0x31 || b[0] == 0x32) && b[1] == 0xBE && b[2] == 0 && b[3] == 0 {
        Kind::DosWord
    } else if !b.is_empty() && !b.contains(&0) {
        Kind::PlainText
    } else {
        Kind::Unknown
    }
}

/// A plain-text file with a .doc name (shown as text, not as a document).
pub fn is_plain_text(path: &Path) -> bool {
    kind(path) == Kind::PlainText
}

fn u16le(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at + 2).map(|x| u16::from_le_bytes([x[0], x[1]]))
}
fn u32le(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}

/// Word's special characters as plain text: paragraph and cell marks,
/// breaks; field instructions dropped (their results kept); pictures and
/// annotation anchors dropped.
fn clean(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    // Inside a field's instructions (between 0x13 and 0x14), per nesting level.
    let mut fields: Vec<bool> = Vec::new();
    for c in raw.chars() {
        match c {
            '\u{13}' => fields.push(true),
            '\u{14}' => {
                if let Some(f) = fields.last_mut() {
                    *f = false;
                }
            }
            '\u{15}' => {
                fields.pop();
            }
            _ if fields.iter().any(|&instr| instr) => {}
            '\r' => out.push('\n'),
            '\u{07}' => out.push('\t'),
            '\u{0B}' => out.push('\n'),
            '\u{0C}' => out.push_str("\n\n"),
            '\u{1E}' => out.push('-'),
            '\u{1F}' | '\u{01}' | '\u{02}' | '\u{05}' | '\u{08}' | '\0' => {}
            c => out.push(c),
        }
    }
    out
}

fn cp1252(b: &[u8]) -> String {
    encoding_rs::WINDOWS_1252.decode_without_bom_handling(b).0.into_owned()
}

/// Word 97-2003: the main text through the piece table (CLX) in the table
/// stream; Word 6/95: the text run from fcMin to fcMac.
fn ole_text(path: &Path) -> Result<String, String> {
    let mut c = cfb::open(path).map_err(|e| format!("This document can't be read: {e}"))?;
    let mut wd = Vec::new();
    c.open_stream("/WordDocument").map_err(|_| "This isn't a Word document (no WordDocument stream).".to_string())?.read_to_end(&mut wd).map_err(|e| e.to_string())?;
    let bad = || "This Word document's header is damaged.".to_string();
    let ident = u16le(&wd, 0).ok_or_else(bad)?;
    if ident != 0xA5EC && ident != 0xA5DC {
        return Err("This isn't a Word document.".into());
    }
    let nfib = u16le(&wd, 2).ok_or_else(bad)?;
    let flags = u16le(&wd, 0x0A).ok_or_else(bad)?;
    if flags & 0x0100 != 0 {
        return Err("This document is password-protected.".into());
    }
    if nfib < 0xC1 {
        // Word 6 / 95: 8-bit text from fcMin to fcMac (fast-saved files can
        // have pieces elsewhere; this reads the main run).
        let (a, b) = (u32le(&wd, 0x18).ok_or_else(bad)? as usize, u32le(&wd, 0x1C).ok_or_else(bad)? as usize);
        let t = wd.get(a..b.min(wd.len())).ok_or_else(bad)?;
        return Ok(clean(&cp1252(t)));
    }
    let table_name = if flags & 0x0200 != 0 { "/1Table" } else { "/0Table" };
    let mut table = Vec::new();
    c.open_stream(table_name).map_err(|_| bad())?.read_to_end(&mut table).map_err(|e| e.to_string())?;
    let ccp_text = u32le(&wd, 0x4C).unwrap_or(u32::MAX) as usize;
    let (fc_clx, lcb_clx) = (u32le(&wd, 0x01A2).ok_or_else(bad)? as usize, u32le(&wd, 0x01A6).ok_or_else(bad)? as usize);
    let clx = table.get(fc_clx..fc_clx + lcb_clx).ok_or_else(bad)?;
    // Skip the Prc records (formatting), to the Pcdt (the piece table).
    let mut i = 0;
    while i < clx.len() && clx[i] == 0x01 {
        let cb = u16le(clx, i + 1).ok_or_else(bad)? as usize;
        i += 3 + cb;
    }
    if clx.get(i) != Some(&0x02) {
        return Err(bad());
    }
    let lcb = u32le(clx, i + 1).ok_or_else(bad)? as usize;
    let plc = clx.get(i + 5..i + 5 + lcb).ok_or_else(bad)?;
    // n+1 character positions, then n 8-byte piece descriptors.
    let n = (lcb.saturating_sub(4)) / 12;
    let mut text = String::new();
    let mut cp_done = 0usize;
    for k in 0..n {
        let (cp0, cp1) = (u32le(plc, k * 4).ok_or_else(bad)? as usize, u32le(plc, (k + 1) * 4).ok_or_else(bad)? as usize);
        if cp0 >= ccp_text {
            break;
        }
        let len = cp1.saturating_sub(cp0).min(ccp_text - cp0);
        let pcd = (n + 1) * 4 + k * 8;
        let fc = u32le(plc, pcd + 2).ok_or_else(bad)?;
        if fc & 0x4000_0000 != 0 {
            // Compressed: one byte per character (Windows-1252), at fc/2.
            let at = ((fc & !0x4000_0000) / 2) as usize;
            if let Some(b) = wd.get(at..at + len) {
                text.push_str(&cp1252(b));
            }
        } else {
            let at = fc as usize;
            if let Some(b) = wd.get(at..at + len * 2) {
                let u: Vec<u16> = b.chunks_exact(2).map(|x| u16::from_le_bytes([x[0], x[1]])).collect();
                text.push_str(&String::from_utf16_lossy(&u));
            }
        }
        cp_done += len;
        if cp_done >= ccp_text {
            break;
        }
    }
    Ok(clean(&text))
}

/// A .doc's text (any of the kinds above), or why not.
pub fn text(path: &Path) -> Result<String, String> {
    match kind(path) {
        Kind::Ole => ole_text(path),
        Kind::WinWord12 => {
            let b = std::fs::read(path).map_err(|e| e.to_string())?;
            let (a, e) = (u32le(&b, 0x18).unwrap_or(0) as usize, u32le(&b, 0x1C).unwrap_or(0) as usize);
            b.get(a..e.min(b.len())).map(|t| clean(&cp1252(t))).ok_or_else(|| "This Word document is damaged.".into())
        }
        Kind::DosWord => {
            // Text from byte 128 to fcMac; DOS's code page (or Windows' for Write).
            let b = std::fs::read(path).map_err(|e| e.to_string())?;
            let e = u32le(&b, 0x0E).unwrap_or(b.len() as u32) as usize;
            let t = b.get(128..e.min(b.len())).ok_or("This Word document is damaged.")?;
            let enc = crate::textenc::detect(t, "x.nfo");
            let enc = if crate::textenc::detect(t, "x.txt") == crate::textenc::Enc::Utf8 { crate::textenc::Enc::Utf8 } else { enc };
            Ok(clean(&crate::textenc::decode(t, enc)))
        }
        Kind::PlainText => {
            let b = std::fs::read(path).map_err(|e| e.to_string())?;
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            Ok(crate::textenc::read(&b, &name).0)
        }
        Kind::Unknown => Err("This .doc file isn't in a format grove knows.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleans_fields_and_marks() {
        assert_eq!(clean("a\u{13} HYPERLINK \"x\" \u{14}link\u{15} b\rc\u{07}d"), "alink b\nc\td");
        // Nested fields: only results survive.
        assert_eq!(clean("\u{13}IF \u{13}PAGE\u{14}1\u{15}\u{14}yes\u{15}"), "yes");
    }
}
