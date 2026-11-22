//! HexDoc: any file as a hex dump, the whole of it. The view is a list of
//! rows of 1 KB (64 lines of 16 bytes); a row asks for its bytes when it
//! comes on screen and they're read and formatted on a worker thread, so
//! a file of any size scrolls without being loaded (or the UI thread ever
//! touching the disk).

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use std::io::{Read, Seek, SeekFrom};
use std::pin::Pin;
use std::sync::mpsc;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, path)]
        #[qproperty(f64, size)]
        #[qproperty(i32, rows)]
        #[qproperty(bool, dark)]
        type HexDoc = super::HexDocRust;
    }

    impl cxx_qt::Threading for HexDoc {}

    #[auto_cxx_name]
    extern "RustQt" {
        /// A row's text is ready (HTML: offsets, hex, characters).
        #[qsignal]
        fn row_ready(self: Pin<&mut HexDoc>, row: i32, html: QString);
    }

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        /// Show this file (its size decides the rows).
        #[qinvokable]
        fn open(self: Pin<&mut HexDoc>, path: &QString);
        /// Read and format row `row` (answers with rowReady).
        #[qinvokable]
        fn request(self: Pin<&mut HexDoc>, row: i32);
    }
}

pub const ROW_BYTES: u64 = 1024;
const LINE: usize = 16;

#[derive(Default)]
pub struct HexDocRust {
    path: QString,
    size: f64,
    rows: i32,
    dark: bool,
    gen: u64,
    tx: Option<mpsc::Sender<(u64, i32)>>,
}

impl qobject::HexDoc {
    fn open(mut self: Pin<&mut Self>, path: &QString) {
        let p = path.to_string();
        let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        self.as_mut().rust_mut().gen += 1;
        // One reader per file: requests are answered in order, newest file only.
        let (tx, rx) = mpsc::channel::<(u64, i32)>();
        self.as_mut().rust_mut().tx = Some(tx);
        let qt = self.qt_thread();
        let dark = self.rust().dark;
        std::thread::Builder::new()
            .name("grove-hex".into())
            .spawn(move || {
                let Ok(mut f) = std::fs::File::open(&p) else { return };
                while let Ok((gen, row)) = rx.recv() {
                    let mut buf = vec![0u8; ROW_BYTES as usize];
                    let off = row as u64 * ROW_BYTES;
                    let n = f.seek(SeekFrom::Start(off)).and_then(|_| read_full(&mut f, &mut buf)).unwrap_or(0);
                    buf.truncate(n);
                    let html = format_row(off, &buf, dark);
                    let _ = qt.queue(move |mut h| {
                        if h.rust().gen == gen {
                            h.as_mut().row_ready(row, QString::from(html.as_str()));
                        }
                    });
                }
            })
            .ok();
        // Only now (the reader is ready): rows that appear ask it at once.
        self.as_mut().set_path(path.clone());
        self.as_mut().set_size(size as f64);
        self.as_mut().set_rows(size.div_ceil(ROW_BYTES).min(i32::MAX as u64) as i32);
    }

    fn request(self: Pin<&mut Self>, row: i32) {
        if let Some(tx) = &self.rust().tx {
            let _ = tx.send((self.rust().gen, row));
        }
    }
}

fn read_full(f: &mut std::fs::File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        let k = f.read(&mut buf[n..])?;
        if k == 0 {
            break;
        }
        n += k;
    }
    Ok(n)
}

/// "00000400  7f 45 4c 46 02 01 01 00  00 00 …  |.ELF....|" lines as HTML.
pub fn format_row(offset: u64, bytes: &[u8], dark: bool) -> String {
    let (off_c, hex_c, zero_c, txt_c) = if dark { ("#7f8794", "#e6e6e6", "#5a606b", "#8fc1ff") } else { ("#8a8f98", "#1d1d1f", "#b0b4ba", "#2a62c9") };
    let mut out = String::with_capacity(bytes.len() * 12 + 256);
    out.push_str("<pre style=\"margin:0\">");
    for (i, chunk) in bytes.chunks(LINE).enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&format!("<span style=\"color:{off_c}\">{:08x}</span>  ", offset + (i * LINE) as u64));
        for j in 0..LINE {
            if j == 8 {
                out.push(' ');
            }
            match chunk.get(j) {
                Some(b) => {
                    let c = if *b == 0 { zero_c } else { hex_c };
                    out.push_str(&format!("<span style=\"color:{c}\">{b:02x}</span> "));
                }
                None => out.push_str("   "),
            }
        }
        out.push_str(&format!(" <span style=\"color:{off_c}\">|</span><span style=\"color:{txt_c}\">"));
        for b in chunk {
            match *b {
                b'<' => out.push_str("&lt;"),
                b'>' => out.push_str("&gt;"),
                b'&' => out.push_str("&amp;"),
                0x20..=0x7e => out.push(*b as char),
                _ => out.push('·'),
            }
        }
        out.push_str(&format!("</span><span style=\"color:{off_c}\">|</span>"));
    }
    out.push_str("</pre>");
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn row_format() {
        let h = super::format_row(0x400, b"\x7fELF<x>\0", true);
        assert!(h.contains("00000400"));
        assert!(h.contains("7f</span>"));
        assert!(h.contains("&lt;x&gt;"));
        assert!(h.contains("·ELF") || h.contains("·"));
    }
}
