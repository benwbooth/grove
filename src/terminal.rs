//! The terminal panel (F4, like Dolphin's): a real terminal emulator
//! (alacritty_terminal: VT parsing, the grid, the PTY) shown by QML as rows
//! of native-rendered rich text. It follows the folder you're in, sending
//! `cd` to the shell only while nothing is running in it.
//!
//! Why not embed Tessera or Konsole: Tessera's view needs its daemon and
//! isn't a reusable QML module, Konsole's KPart is KDE, and Wayland can't
//! embed another program's window. This is small and self-contained.

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::QString;
use std::borrow::Cow;
use std::collections::HashMap;
use std::os::fd::AsRawFd;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, title)]
        #[qproperty(bool, alive)]
        type TerminalSession = super::TerminalSessionRust;
    }

    impl cxx_qt::Threading for TerminalSession {}

    #[auto_cxx_name]
    unsafe extern "RustQt" {
        /// Start a shell in `dir` with this many columns/rows.
        #[qinvokable]
        fn start(self: Pin<&mut TerminalSession>, dir: &QString, cols: i32, rows: i32, cell_w: i32, cell_h: i32, dark: bool);
        #[qinvokable]
        fn resize(self: Pin<&mut TerminalSession>, cols: i32, rows: i32, cell_w: i32, cell_h: i32);
        /// A key press (Qt key code, text, modifiers).
        #[qinvokable]
        fn key(self: Pin<&mut TerminalSession>, key: i32, text: &QString, modifiers: i32);
        #[qinvokable]
        fn send_text(self: Pin<&mut TerminalSession>, text: &QString);
        /// The screen: JSON {rows: [html...], cx, cy, cursor}.
        #[qinvokable]
        fn screen(self: &TerminalSession) -> QString;
        #[qinvokable]
        fn scroll(self: Pin<&mut TerminalSession>, lines: i32);
        /// Go to `dir` if the shell isn't running anything (returns whether it did).
        #[qinvokable]
        fn follow(self: Pin<&mut TerminalSession>, dir: &QString) -> bool;
        #[qinvokable]
        fn stop(self: Pin<&mut TerminalSession>);
        /// New content to show (coalesced).
        #[qsignal]
        fn changed(self: Pin<&mut TerminalSession>);
    }
}

#[derive(Clone)]
struct Listener {
    qt: CxxQtThread<qobject::TerminalSession>,
    pending: Arc<AtomicBool>,
    sender: Arc<parking_lot::Mutex<Option<EventLoopSender>>>,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        match event {
            Event::Wakeup => {
                if !self.pending.swap(true, Ordering::SeqCst) {
                    let p = self.pending.clone();
                    let _ = self.qt.queue(move |t| {
                        p.store(false, Ordering::SeqCst);
                        t.changed();
                    });
                }
            }
            Event::PtyWrite(s) => {
                if let Some(tx) = self.sender.lock().as_ref() {
                    let _ = tx.send(Msg::Input(Cow::Owned(s.into_bytes())));
                }
            }
            Event::Title(t) => {
                let _ = self.qt.queue(move |t2| t2.set_title(QString::from(t.as_str())));
            }
            Event::Exit | Event::ChildExit(_) => {
                let _ = self.qt.queue(|mut t| {
                    t.as_mut().set_alive(false);
                    t.changed();
                });
            }
            _ => {}
        }
    }
}

struct Size {
    cols: usize,
    lines: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.lines
    }
    fn screen_lines(&self) -> usize {
        self.lines
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

#[derive(Default)]
pub struct TerminalSessionRust {
    title: QString,
    alive: bool,
    term: Option<Arc<FairMutex<Term<Listener>>>>,
    sender: Option<EventLoopSender>,
    fd: i32,
    child: i32,
    dark: bool,
}

fn palette(i: usize, dark: bool) -> Rgb {
    const BASE: [(u8, u8, u8); 16] = [
        (0x1d, 0x1f, 0x21), (0xcc, 0x66, 0x66), (0x8c, 0xb4, 0x6c), (0xde, 0xb9, 0x74), (0x5f, 0x9f, 0xd6), (0xb2, 0x94, 0xbb), (0x5e, 0xb5, 0xae), (0xc5, 0xc8, 0xc6),
        (0x66, 0x66, 0x66), (0xff, 0x7b, 0x72), (0xa6, 0xda, 0x95), (0xf0, 0xc6, 0x74), (0x7f, 0xb8, 0xff), (0xd2, 0xa8, 0xff), (0x7d, 0xd6, 0xcf), (0xff, 0xff, 0xff),
    ];
    if i < 16 {
        let (r, g, b) = BASE[i];
        // Light backgrounds: darker whites so they stay readable.
        if !dark && (i == 7 || i == 15) {
            return Rgb { r: 0x55, g: 0x55, b: 0x55 };
        }
        return Rgb { r, g, b };
    }
    if i < 232 {
        let i = i - 16;
        let c = |v: usize| if v == 0 { 0 } else { (55 + v * 40) as u8 };
        return Rgb { r: c(i / 36), g: c((i / 6) % 6), b: c(i % 6) };
    }
    let v = (8 + (i - 232) * 10) as u8;
    Rgb { r: v, g: v, b: v }
}

fn resolve(c: Color, dark: bool, fg: bool) -> Option<Rgb> {
    match c {
        Color::Spec(rgb) => Some(rgb),
        Color::Indexed(i) => Some(palette(i as usize, dark)),
        Color::Named(n) => {
            let idx = n as usize;
            if idx < 16 {
                Some(palette(idx, dark))
            } else {
                match n {
                    NamedColor::Foreground | NamedColor::BrightForeground => None,
                    NamedColor::Background => None,
                    NamedColor::DimForeground => if fg { Some(Rgb { r: 0x99, g: 0x99, b: 0x99 }) } else { None },
                    _ => {
                        if (NamedColor::DimBlack as usize..=NamedColor::DimWhite as usize).contains(&idx) {
                            Some(palette(idx - NamedColor::DimBlack as usize, dark))
                        } else {
                            None
                        }
                    }
                }
            }
        }
    }
}

fn esc(c: char, out: &mut String) {
    match c {
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '&' => out.push_str("&amp;"),
        ' ' => out.push_str("&nbsp;"),
        c => out.push(c),
    }
}

/// Bytes for a key press (xterm conventions).
pub fn encode_key(key: i32, text: &str, mods: i32, app_cursor: bool) -> Vec<u8> {
    const SHIFT: i32 = 0x0200_0000;
    const CTRL: i32 = 0x0400_0000;
    const ALT: i32 = 0x0800_0000;
    let ctrl = mods & CTRL != 0;
    let alt = mods & ALT != 0;
    let shift = mods & SHIFT != 0;
    let modn = 1 + if shift { 1 } else { 0 } + if alt { 2 } else { 0 } + if ctrl { 4 } else { 0 };
    let csi = |fin: &str| -> Vec<u8> {
        if modn > 1 {
            format!("\x1b[1;{modn}{fin}").into_bytes()
        } else if app_cursor {
            format!("\x1bO{fin}").into_bytes()
        } else {
            format!("\x1b[{fin}").into_bytes()
        }
    };
    let tilde = |n: u32| -> Vec<u8> { if modn > 1 { format!("\x1b[{n};{modn}~").into_bytes() } else { format!("\x1b[{n}~").into_bytes() } };
    match key {
        0x0100_0004 | 0x0100_0005 => return if alt { b"\x1b\r".to_vec() } else { b"\r".to_vec() }, // Return, Enter
        0x0100_0003 => return if ctrl { vec![0x08] } else if alt { b"\x1b\x7f".to_vec() } else { vec![0x7f] }, // Backspace
        0x0100_0001 => return b"\t".to_vec(),
        0x0100_0002 => return b"\x1b[Z".to_vec(), // Backtab
        0x0100_0000 => return vec![0x1b],
        0x0100_0013 => return csi("A"),
        0x0100_0015 => return csi("B"),
        0x0100_0014 => return csi("C"),
        0x0100_0012 => return csi("D"),
        0x0100_0010 => return csi("H"),
        0x0100_0011 => return csi("F"),
        0x0100_0016 => return tilde(5),
        0x0100_0017 => return tilde(6),
        0x0100_0006 => return tilde(2),
        0x0100_0007 => return tilde(3),
        k if (0x0100_0030..=0x0100_003b).contains(&k) => {
            let n = (k - 0x0100_0030) as usize;
            let seq = ["P", "Q", "R", "S", "15~", "17~", "18~", "19~", "20~", "21~", "23~", "24~"][n];
            return if n < 4 { format!("\x1bO{seq}").into_bytes() } else { format!("\x1b[{seq}").into_bytes() };
        }
        _ => {}
    }
    if ctrl && !text.is_empty() || ctrl && (0x41..=0x5a).contains(&key) {
        // Ctrl+letter -> control byte.
        let c = if (0x41..=0x5a).contains(&key) { key as u8 } else { text.as_bytes()[0].to_ascii_uppercase() };
        let b = match c {
            b'@'..=b'_' => c - b'@',
            b' ' => 0,
            b'?' => 0x7f,
            _ => c,
        };
        return if alt { vec![0x1b, b] } else { vec![b] };
    }
    if text.is_empty() {
        return vec![];
    }
    let mut v = Vec::new();
    if alt {
        v.push(0x1b);
    }
    v.extend_from_slice(text.as_bytes());
    v
}

impl qobject::TerminalSession {
    fn start(mut self: Pin<&mut Self>, dir: &QString, cols: i32, rows: i32, cell_w: i32, cell_h: i32, dark: bool) {
        if self.rust().term.is_some() {
            return;
        }
        let (cols, rows) = (cols.max(10) as usize, rows.max(3) as usize);
        let ws = WindowSize { num_lines: rows as u16, num_cols: cols as u16, cell_width: cell_w.max(1) as u16, cell_height: cell_h.max(1) as u16 };
        let mut env = HashMap::new();
        env.insert("TERM".to_string(), "xterm-256color".to_string());
        env.insert("COLORTERM".to_string(), "truecolor".to_string());
        let opts = tty::Options { shell: None, working_directory: Some(dir.to_string().into()), drain_on_exit: false, env };
        let pty = match tty::new(&opts, ws, 0) {
            Ok(p) => p,
            Err(e) => {
                crate::controller::notify_any("error", &format!("Couldn't start a shell for the terminal panel: {e}"));
                return;
            }
        };
        let fd = pty.file().as_raw_fd();
        let child = pty.child().id() as i32;
        let pending = Arc::new(AtomicBool::new(false));
        let sender_slot = Arc::new(parking_lot::Mutex::new(None));
        let listener = Listener { qt: self.qt_thread(), pending, sender: sender_slot.clone() };
        let term = Arc::new(FairMutex::new(Term::new(Config::default(), &Size { cols, lines: rows }, listener.clone())));
        let el = match EventLoop::new(term.clone(), listener, pty, false, false) {
            Ok(e) => e,
            Err(e) => {
                crate::controller::notify_any("error", &format!("Couldn't start the terminal: {e}"));
                return;
            }
        };
        let tx = el.channel();
        *sender_slot.lock() = Some(tx.clone());
        el.spawn();
        let mut r = self.as_mut().rust_mut();
        r.term = Some(term);
        r.sender = Some(tx);
        r.fd = fd;
        r.child = child;
        r.dark = dark;
        drop(r);
        self.as_mut().set_alive(true);
    }

    fn resize(self: Pin<&mut Self>, cols: i32, rows: i32, cell_w: i32, cell_h: i32) {
        let r = self.rust();
        let (Some(term), Some(tx)) = (&r.term, &r.sender) else { return };
        let (cols, rows) = (cols.max(10) as usize, rows.max(3) as usize);
        term.lock().resize(Size { cols, lines: rows });
        let _ = tx.send(Msg::Resize(WindowSize { num_lines: rows as u16, num_cols: cols as u16, cell_width: cell_w.max(1) as u16, cell_height: cell_h.max(1) as u16 }));
    }

    fn write(&self, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }
        if let Some(tx) = &self.rust().sender {
            let _ = tx.send(Msg::Input(Cow::Owned(bytes)));
        }
    }

    fn key(self: Pin<&mut Self>, key: i32, text: &QString, modifiers: i32) {
        let app_cursor = self.rust().term.as_ref().map(|t| t.lock().mode().contains(TermMode::APP_CURSOR)).unwrap_or(false);
        // Typing scrolls back to the bottom.
        if let Some(t) = &self.rust().term {
            t.lock().scroll_display(Scroll::Bottom);
        }
        self.write(encode_key(key, &text.to_string(), modifiers, app_cursor));
    }

    fn send_text(self: Pin<&mut Self>, text: &QString) {
        let bracketed = self.rust().term.as_ref().map(|t| t.lock().mode().contains(TermMode::BRACKETED_PASTE)).unwrap_or(false);
        let t = text.to_string().replace("\r\n", "\r").replace('\n', "\r");
        let bytes = if bracketed { format!("\x1b[200~{t}\x1b[201~").into_bytes() } else { t.into_bytes() };
        self.write(bytes);
    }

    fn screen(&self) -> QString {
        let r = self.rust();
        let Some(term) = &r.term else { return QString::from("{\"rows\":[]}") };
        let t = term.lock();
        let grid = t.grid();
        let lines = grid.screen_lines();
        let cols = grid.columns();
        let off = grid.display_offset() as i32;
        let mut rows = Vec::with_capacity(lines);
        for l in 0..lines {
            let line = Line(l as i32 - off);
            let row = &grid[line];
            let mut html = String::new();
            let mut cur: Option<(Option<Rgb>, Option<Rgb>, Flags)> = None;
            for c in 0..cols {
                let cell = &row[Column(c)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                let (mut fg, mut bg) = (resolve(cell.fg, r.dark, true), resolve(cell.bg, r.dark, false));
                if cell.flags.contains(Flags::INVERSE) {
                    std::mem::swap(&mut fg, &mut bg);
                    if fg.is_none() {
                        fg = Some(if r.dark { Rgb { r: 0x1e, g: 0x1e, b: 0x20 } } else { Rgb { r: 255, g: 255, b: 255 } });
                    }
                    if bg.is_none() {
                        bg = Some(if r.dark { Rgb { r: 0xf5, g: 0xf5, b: 0xf7 } } else { Rgb { r: 0x1d, g: 0x1d, b: 0x1f } });
                    }
                }
                let style = (fg, bg, cell.flags & (Flags::BOLD | Flags::ITALIC | Flags::UNDERLINE | Flags::STRIKEOUT));
                if cur != Some(style) {
                    if cur.is_some() {
                        html.push_str("</span>");
                    }
                    let mut css = String::new();
                    if let Some(f) = fg {
                        css.push_str(&format!("color:#{:02x}{:02x}{:02x};", f.r, f.g, f.b));
                    }
                    if let Some(b) = bg {
                        css.push_str(&format!("background-color:#{:02x}{:02x}{:02x};", b.r, b.g, b.b));
                    }
                    if style.2.contains(Flags::BOLD) {
                        css.push_str("font-weight:bold;");
                    }
                    if style.2.contains(Flags::ITALIC) {
                        css.push_str("font-style:italic;");
                    }
                    if style.2.contains(Flags::UNDERLINE) {
                        css.push_str("text-decoration:underline;");
                    }
                    html.push_str(&format!("<span style=\"{css}\">"));
                    cur = Some(style);
                }
                let ch = if cell.c == '\0' { ' ' } else { cell.c };
                esc(ch, &mut html);
                if let Some(z) = cell.zerowidth() {
                    for &zc in z {
                        esc(zc, &mut html);
                    }
                }
            }
            if cur.is_some() {
                html.push_str("</span>");
            }
            rows.push(html);
        }
        let cursor = t.grid().cursor.point;
        let visible = t.mode().contains(TermMode::SHOW_CURSOR) && off == 0;
        let v = serde_json::json!({"rows": rows, "cx": cursor.column.0, "cy": cursor.line.0, "cursor": visible, "scrolled": off > 0});
        QString::from(v.to_string().as_str())
    }

    fn scroll(self: Pin<&mut Self>, lines: i32) {
        if let Some(t) = &self.rust().term {
            t.lock().scroll_display(Scroll::Delta(lines));
        }
        let mut s = self;
        s.as_mut().changed();
    }

    fn follow(self: Pin<&mut Self>, dir: &QString) -> bool {
        let r = self.rust();
        if r.term.is_none() || r.fd <= 0 {
            return false;
        }
        // Only while the shell itself is in the foreground (nothing running).
        let fg = unsafe { libc::tcgetpgrp(r.fd) };
        if fg != r.child {
            return false;
        }
        let d = dir.to_string().replace('\'', "'\\''");
        // A leading space keeps it out of most shells' history.
        self.write(format!(" cd -- '{d}'\r").into_bytes());
        true
    }

    fn stop(mut self: Pin<&mut Self>) {
        if let Some(tx) = self.as_mut().rust_mut().sender.take() {
            let _ = tx.send(Msg::Shutdown);
        }
        self.as_mut().rust_mut().term = None;
        self.set_alive(false);
    }
}

#[cfg(test)]
mod tests {
    use super::encode_key;
    #[test]
    fn keys() {
        assert_eq!(encode_key(0x0100_0004, "\r", 0, false), b"\r");
        assert_eq!(encode_key(0x0100_0013, "", 0, false), b"\x1b[A");
        assert_eq!(encode_key(0x0100_0013, "", 0, true), b"\x1bOA");
        assert_eq!(encode_key(0x43, "c", 0x0400_0000, false), vec![3]);
        assert_eq!(encode_key(0x41, "a", 0, false), b"a");
        assert_eq!(encode_key(0x41, "a", 0x0800_0000, false), b"\x1ba");
        assert_eq!(encode_key(0x0100_0012, "", 0x0400_0000, false), b"\x1b[1;5D");
    }
}
