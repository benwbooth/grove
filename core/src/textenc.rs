//! Text encodings: what a file's bytes are, and decoding them.
//! UTF-8 (and its BOM), UTF-16 with or without a BOM, DOS code page 437
//! (box drawing, .nfo/.diz/.ans art), and everything Firefox's detector
//! (chardetng: Windows-1252, ISO-8859-x, Shift_JIS, GBK, Big5, EUC-KR,
//! KOI8-R...) recognises statistically.

use encoding_rs::Encoding;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Enc {
    Utf8,
    Utf16Le,
    Utf16Be,
    Cp437,
    Other(&'static Encoding),
}

impl Enc {
    /// A short name for the viewer ("" for UTF-8, the usual case).
    pub fn label(self) -> String {
        match self {
            Enc::Utf8 => String::new(),
            Enc::Utf16Le => "UTF-16 LE".into(),
            Enc::Utf16Be => "UTF-16 BE".into(),
            Enc::Cp437 => "DOS (CP437)".into(),
            Enc::Other(e) => e.name().to_string(),
        }
    }
    /// One byte per character (a block cut anywhere decodes the same).
    pub fn single_byte(self) -> bool {
        match self {
            Enc::Utf8 | Enc::Utf16Le | Enc::Utf16Be => false,
            Enc::Cp437 => true,
            Enc::Other(e) => e.is_single_byte(),
        }
    }
}

/// Names that are DOS art by convention.
fn dos_name(name: &str) -> bool {
    let n = name.to_lowercase();
    [".nfo", ".diz", ".ans", ".asc"].iter().any(|e| n.ends_with(e))
}

/// Valid UTF-8, allowing a character cut off at the end of `b` (a head).
fn utf8_ok(b: &[u8]) -> bool {
    match std::str::from_utf8(b) {
        Ok(_) => true,
        Err(e) => e.error_len().is_none() && b.len() - e.valid_up_to() < 4,
    }
}

/// UTF-16 without a BOM: one byte of most pairs is zero (Latin text).
fn utf16_guess(b: &[u8]) -> Option<Enc> {
    if b.len() < 8 {
        return None;
    }
    let pairs = b.len() / 2;
    let (mut even0, mut odd0) = (0usize, 0usize);
    for p in b.chunks_exact(2) {
        if p[0] == 0 {
            even0 += 1;
        }
        if p[1] == 0 {
            odd0 += 1;
        }
    }
    if odd0 * 10 > pairs * 6 && even0 * 20 < pairs {
        Some(Enc::Utf16Le)
    } else if even0 * 10 > pairs * 6 && odd0 * 20 < pairs {
        Some(Enc::Utf16Be)
    } else {
        None
    }
}

/// How much `b` looks like DOS text: box drawing, shades and blocks
/// (0xB0-0xDF), often in runs, make most of its high bytes.
fn cp437_score(b: &[u8]) -> f64 {
    let high: Vec<u8> = b.iter().copied().filter(|&c| c >= 0x80).collect();
    if high.is_empty() {
        return 0.0;
    }
    let boxy = high.iter().filter(|&&c| (0xB0..=0xDF).contains(&c)).count();
    let mut runs = 0usize;
    for w in b.windows(3) {
        if w[0] >= 0xB0 && w[0] <= 0xDF && w[0] == w[1] && w[1] == w[2] {
            runs += 1;
        }
    }
    boxy as f64 / high.len() as f64 + (runs as f64 / 8.0).min(0.5)
}

/// The encoding of a file from its first bytes (and name).
pub fn detect(head: &[u8], name: &str) -> Enc {
    if head.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Enc::Utf8;
    }
    if head.starts_with(&[0xFF, 0xFE]) {
        return Enc::Utf16Le;
    }
    if head.starts_with(&[0xFE, 0xFF]) {
        return Enc::Utf16Be;
    }
    if let Some(e) = utf16_guess(head) {
        return e;
    }
    if utf8_ok(head) {
        return Enc::Utf8;
    }
    if dos_name(name) || cp437_score(head) >= 0.8 {
        return Enc::Cp437;
    }
    let mut d = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Allow);
    d.feed(head, true);
    let e = d.guess(None, chardetng::Utf8Detection::Deny);
    if e == encoding_rs::UTF_8 {
        Enc::Other(encoding_rs::WINDOWS_1252)
    } else {
        Enc::Other(e)
    }
}

/// CP437's upper half (0x80-0xFF) and 0x7F.
const CP437_HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', 'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ',
    'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐',
    '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧', '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀',
    'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', '≡', '±', '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];
/// CP437's pictures for control codes other than tab, newlines, form feed
/// and the DOS end-of-file mark (shown in DOS art).
const CP437_LOW: [char; 32] = [
    '\0', '☺', '☻', '♥', '♦', '♣', '♠', '•', '◘', '\t', '\n', '♂', '\x0c', '\r', '♫', '☼', '►', '◄', '↕', '‼', '¶', '§', '▬', '↨', '↑', '↓', '→', '←', '∟', '↔', '▲', '▼',
];

fn cp437(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len());
    for &c in b {
        match c {
            // DOS end-of-file: the rest is padding.
            0x1A => break,
            0x00..=0x1F => s.push(CP437_LOW[c as usize]),
            0x7F => s.push('⌂'),
            0x80..=0xFF => s.push(CP437_HIGH[(c - 0x80) as usize]),
            _ => s.push(c as char),
        }
    }
    s
}

/// Decode `b` as `enc` (bad bytes become U+FFFD).
pub fn decode(b: &[u8], enc: Enc) -> String {
    match enc {
        Enc::Utf8 => String::from_utf8_lossy(b.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(b)).into_owned(),
        Enc::Utf16Le => encoding_rs::UTF_16LE.decode(b).0.into_owned(),
        Enc::Utf16Be => encoding_rs::UTF_16BE.decode(b).0.into_owned(),
        Enc::Cp437 => cp437(b),
        Enc::Other(e) => e.decode_without_bom_handling(b).0.into_owned(),
    }
}

/// Detect and decode in one go: (text, encoding).
pub fn read(b: &[u8], name: &str) -> (String, Enc) {
    let e = detect(&b[..b.len().min(1 << 16)], name);
    (decode(b, e), e)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects() {
        assert_eq!(detect(b"plain ascii", "a.txt"), Enc::Utf8);
        assert_eq!(detect("héllo".as_bytes(), "a.txt"), Enc::Utf8);
        // A DOS box: ╔═══╗ / ║hi ║ / ╚═══╝
        let box_art = [0xC9, 0xCD, 0xCD, 0xCD, 0xBB, b'\n', 0xBA, b'h', b'i', b' ', 0xBA, b'\n', 0xC8, 0xCD, 0xCD, 0xCD, 0xBC];
        assert_eq!(detect(&box_art, "readme.txt"), Enc::Cp437);
        assert_eq!(decode(&box_art, Enc::Cp437), "╔═══╗\n║hi ║\n╚═══╝");
        // Windows-1252: "café" with é = 0xE9.
        assert_eq!(detect(b"caf\xE9 au lait, cr\xE8me br\xFBl\xE9e", "menu.txt").label(), "windows-1252");
        let u16: Vec<u8> = "hello world".encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
        assert_eq!(detect(&u16, "x.txt"), Enc::Utf16Le);
        assert_eq!(decode(&u16, Enc::Utf16Le), "hello world");
        assert_eq!(detect(&[0xB0, 0xB1], "x.nfo"), Enc::Cp437);
    }
}
