//! Grove Preview's PDF reading and editing through MuPDF (csrc/pdfedit.c)
//! and printing through libcups (csrc/print.c).
//!
//! Coordinates are MuPDF page space: points, y down, with the page's own
//! rotation applied (the way the page is drawn). Reading calls answer
//! JSON strings made in C.

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::ffi::{c_char, c_float, c_int, c_void, CStr, CString};
use std::path::Path;

extern "C" {
    fn grove_pdf_free(p: *mut c_char);
    fn grove_pdf_info(path: *const c_char, pw: *const c_char, err: *mut c_char, errlen: c_int) -> *mut c_char;
    fn grove_pdf_render(path: *const c_char, pw: *const c_char, page: c_int, width: c_int, hide: *const c_int, nhide: c_int, w: *mut c_int, h: *mut c_int, err: *mut c_char, errlen: c_int) -> *mut u8;
    fn grove_pdf_select(path: *const c_char, pw: *const c_char, page: c_int, ax: c_float, ay: c_float, bx: c_float, by: c_float, mode: c_int, err: *mut c_char, errlen: c_int) -> *mut c_char;
    fn grove_pdf_search(path: *const c_char, pw: *const c_char, needle: *const c_char, max: c_int, err: *mut c_char, errlen: c_int) -> *mut c_char;
    fn grove_pdf_links(path: *const c_char, pw: *const c_char, page: c_int, err: *mut c_char, errlen: c_int) -> *mut c_char;
    fn grove_pdf_widgets(path: *const c_char, pw: *const c_char, err: *mut c_char, errlen: c_int) -> *mut c_char;
    fn grove_pdf_annots(path: *const c_char, pw: *const c_char, page: c_int, err: *mut c_char, errlen: c_int) -> *mut c_char;

    fn grove_pdfw_open(path: *const c_char, pw: *const c_char, err: *mut c_char, errlen: c_int) -> *mut c_void;
    fn grove_pdfw_close(h: *mut c_void);
    fn grove_pdfw_count(h: *mut c_void) -> c_int;
    fn grove_pdfw_graft(h: *mut c_void, src: *const c_char, pw: *const c_char, page: c_int, err: *mut c_char, errlen: c_int) -> c_int;
    fn grove_pdfw_blank(h: *mut c_void, w: c_float, hh: c_float, err: *mut c_char, errlen: c_int) -> c_int;
    fn grove_pdfw_image_page(h: *mut c_void, img: *const c_char, fit_w: c_float, fit_h: c_float, err: *mut c_char, errlen: c_int) -> c_int;
    fn grove_pdfw_rotate(h: *mut c_void, page: c_int, deg: c_int, err: *mut c_char, errlen: c_int) -> c_int;
    fn grove_pdfw_crop(h: *mut c_void, page: c_int, x0: c_float, y0: c_float, x1: c_float, y1: c_float, err: *mut c_char, errlen: c_int) -> c_int;
    fn grove_pdfw_field(h: *mut c_void, page: c_int, index: c_int, value: *const c_char, err: *mut c_char, errlen: c_int) -> c_int;
    fn grove_pdfw_delete_annots(h: *mut c_void, page: c_int, idx: *const c_int, n: c_int, err: *mut c_char, errlen: c_int) -> c_int;
    fn grove_pdfw_annot(h: *mut c_void, page: c_int, kind: c_int, rgb: *const c_float, alpha: c_float, width: c_float, pts: *const c_float, npts: c_int, counts: *const c_int, nstrokes: c_int, text: *const c_char, font_size: c_float, fill: *const c_float, author: *const c_char, err: *mut c_char, errlen: c_int) -> c_int;
    fn grove_pdfw_reorder(h: *mut c_void, pages: *const c_int, n: c_int, err: *mut c_char, errlen: c_int) -> c_int;
    fn grove_pdfw_meta(h: *mut c_void, key: *const c_char, value: *const c_char, err: *mut c_char, errlen: c_int) -> c_int;
    fn grove_pdfw_save(h: *mut c_void, out: *const c_char, user_pw: *const c_char, owner_pw: *const c_char, allow_print: c_int, allow_copy: c_int, err: *mut c_char, errlen: c_int) -> c_int;

    fn grove_print_free(p: *mut c_char);
    fn grove_print_dests() -> *mut c_char;
    fn grove_print_file(printer: *const c_char, path: *const c_char, title: *const c_char, opts: *const c_char, err: *mut c_char, errlen: c_int) -> c_int;
}

fn cs(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

fn err_text(e: &[u8]) -> String {
    let s = CStr::from_bytes_until_nul(e).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    if s.is_empty() { "Something went wrong reading the document.".into() } else { s }
}

fn take(p: *mut c_char, e: &[u8]) -> Result<String, String> {
    if p.is_null() {
        return Err(err_text(e));
    }
    let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
    unsafe { grove_pdf_free(p) };
    Ok(s)
}

/// Pages, sizes, metadata, outline, password state (JSON).
pub fn info(path: &str, pw: &str) -> Result<String, String> {
    let mut e = vec![0u8; 512];
    let p = unsafe { grove_pdf_info(cs(path).as_ptr(), cs(pw).as_ptr(), e.as_mut_ptr() as *mut c_char, 512) };
    take(p, &e)
}

/// A page drawn `width` px wide: (w, h, RGBA bytes). `hide`: annotation
/// indices to leave out.
pub fn render(path: &str, pw: &str, page: u32, width: u32, hide: &[i32]) -> Result<(u32, u32, Vec<u8>), String> {
    let mut e = vec![0u8; 512];
    let (mut w, mut h) = (0, 0);
    let p = unsafe { grove_pdf_render(cs(path).as_ptr(), cs(pw).as_ptr(), page as c_int, width.clamp(8, 8000) as c_int, hide.as_ptr(), hide.len() as c_int, &mut w, &mut h, e.as_mut_ptr() as *mut c_char, 512) };
    if p.is_null() {
        return Err(err_text(&e));
    }
    let n = w as usize * h as usize * 4;
    let v = unsafe { std::slice::from_raw_parts(p, n) }.to_vec();
    unsafe { libc::free(p as *mut c_void) };
    Ok((w as u32, h as u32, v))
}

/// Text selection between two points. mode: 0 characters, 1 words,
/// 2 lines, 3 the whole page. {quads, text}
pub fn select(path: &str, pw: &str, page: u32, a: (f32, f32), b: (f32, f32), mode: i32) -> Result<String, String> {
    let mut e = vec![0u8; 512];
    let p = unsafe { grove_pdf_select(cs(path).as_ptr(), cs(pw).as_ptr(), page as c_int, a.0, a.1, b.0, b.1, mode, e.as_mut_ptr() as *mut c_char, 512) };
    take(p, &e)
}

/// Every match of `needle` (case-insensitive): [{page, quads, line}].
pub fn search(path: &str, pw: &str, needle: &str, max: u32) -> Result<String, String> {
    let mut e = vec![0u8; 512];
    let p = unsafe { grove_pdf_search(cs(path).as_ptr(), cs(pw).as_ptr(), cs(needle).as_ptr(), max as c_int, e.as_mut_ptr() as *mut c_char, 512) };
    take(p, &e)
}

pub fn links(path: &str, pw: &str, page: u32) -> Result<String, String> {
    let mut e = vec![0u8; 512];
    let p = unsafe { grove_pdf_links(cs(path).as_ptr(), cs(pw).as_ptr(), page as c_int, e.as_mut_ptr() as *mut c_char, 512) };
    take(p, &e)
}

pub fn widgets(path: &str, pw: &str) -> Result<String, String> {
    let mut e = vec![0u8; 512];
    let p = unsafe { grove_pdf_widgets(cs(path).as_ptr(), cs(pw).as_ptr(), e.as_mut_ptr() as *mut c_char, 512) };
    take(p, &e)
}

pub fn annots(path: &str, pw: &str, page: u32) -> Result<String, String> {
    let mut e = vec![0u8; 512];
    let p = unsafe { grove_pdf_annots(cs(path).as_ptr(), cs(pw).as_ptr(), page as c_int, e.as_mut_ptr() as *mut c_char, 512) };
    take(p, &e)
}

/// The printers (JSON).
pub fn printers() -> String {
    let p = unsafe { grove_print_dests() };
    if p.is_null() {
        return "[]".into();
    }
    let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
    unsafe { grove_print_free(p) };
    s
}

/// Print a file; the job id.
pub fn print_file(printer: &str, path: &Path, title: &str, opts: &[(String, String)]) -> Result<i32, String> {
    let o: String = opts.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    let mut e = vec![0u8; 512];
    let id = unsafe { grove_print_file(cs(printer).as_ptr(), cs(&path.to_string_lossy()).as_ptr(), cs(title).as_ptr(), cs(&o).as_ptr(), e.as_mut_ptr() as *mut c_char, 512) };
    if id == 0 {
        let m = err_text(&e);
        return Err(if m.contains("went wrong") { "The printer didn't take the job.".into() } else { m });
    }
    Ok(id)
}

// ---------- saving ----------

/// The document as the Preview window has it: the pages in order, each
/// from some PDF (or blank, or a picture), with its changes.
#[derive(Debug, Deserialize, Default)]
pub struct Spec {
    /// The PDF being written (its own pages stay its own); empty: a new PDF.
    #[serde(default)]
    pub primary: String,
    #[serde(default)]
    pub passwords: HashMap<String, String>,
    pub pages: Vec<PageSpec>,
    #[serde(default)]
    pub encrypt: Option<Encrypt>,
    #[serde(default)]
    pub meta: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Encrypt {
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default = "yes")]
    pub print: bool,
    #[serde(default = "yes")]
    pub copy: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Deserialize, Default)]
pub struct PageSpec {
    /// The PDF the page comes from (0-based `page`).
    #[serde(default)]
    pub src: String,
    #[serde(default)]
    pub page: i32,
    /// A blank page of this size (points).
    #[serde(default)]
    pub blank: Option<[f32; 2]>,
    /// A picture made into a page.
    #[serde(default)]
    pub image: Option<String>,
    /// Turned this much more (degrees).
    #[serde(default)]
    pub rot: i32,
    #[serde(default)]
    pub crop: Option<[f32; 4]>,
    #[serde(default)]
    pub fields: Vec<Field>,
    /// The page's own annotations to remove (indices).
    #[serde(default, rename = "delAnnots")]
    pub del_annots: Vec<i32>,
    /// New annotations (Markup ops, page space).
    #[serde(default)]
    pub annots: Vec<Value>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Field {
    pub i: i32,
    pub value: String,
}

struct Handle(*mut c_void);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { grove_pdfw_close(self.0) }
    }
}

fn check(rc: c_int, e: &[u8]) -> Result<c_int, String> {
    if rc < 0 { Err(err_text(e)) } else { Ok(rc) }
}

fn rgb(c: &Value) -> [f32; 3] {
    let s = c.as_str().unwrap_or("#ff3b30").trim_start_matches('#');
    let v = u32::from_str_radix(s.get(0..6).unwrap_or("ff3b30"), 16).unwrap_or(0xff3b30);
    [((v >> 16) & 255) as f32 / 255.0, ((v >> 8) & 255) as f32 / 255.0, (v & 255) as f32 / 255.0]
}

fn f(v: &Value, k: &str) -> f32 {
    v[k].as_f64().unwrap_or(0.0) as f32
}

fn nums(v: &Value) -> Vec<f32> {
    v.as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_default()
}

/// Who made the annotations: the user's full name, else the login.
fn author() -> String {
    unsafe {
        let pw = libc::getpwuid(libc::getuid());
        if !pw.is_null() && !(*pw).pw_gecos.is_null() {
            let g = CStr::from_ptr((*pw).pw_gecos).to_string_lossy();
            let n = g.split(',').next().unwrap_or("").trim().to_string();
            if !n.is_empty() {
                return n;
            }
        }
    }
    std::env::var("USER").unwrap_or_default()
}

fn add_annot(h: *mut c_void, page: c_int, o: &Value, who: &CString) -> Result<(), String> {
    let t = o["t"].as_str().unwrap_or("");
    let c = rgb(&o["c"]);
    let a = o["a"].as_f64().unwrap_or(1.0) as f32;
    let w = o["w"].as_f64().unwrap_or(2.0) as f32;
    let fill = o.get("fill").filter(|v| v.as_str().is_some_and(|s| !s.is_empty())).map(rgb);
    let fillp = fill.as_ref().map(|f| f.as_ptr()).unwrap_or(std::ptr::null());
    let text = cs(o["text"].as_str().unwrap_or(""));
    let box4 = vec![f(o, "x1"), f(o, "y1"), f(o, "x2"), f(o, "y2")];
    let (kind, pts, counts): (c_int, Vec<f32>, Vec<c_int>) = match t {
        "pen" => {
            let p = nums(&o["p"]);
            let n = (p.len() / 2) as c_int;
            (0, p, vec![n])
        }
        "sign" => {
            // Strokes 0..1 inside the box x1,y1 – x2,y2.
            let (x, y, bw, bh) = (box4[0], box4[1], box4[2] - box4[0], box4[3] - box4[1]);
            let mut pts = Vec::new();
            let mut counts = Vec::new();
            for s in o["strokes"].as_array().into_iter().flatten() {
                let p = nums(s);
                counts.push((p.len() / 2) as c_int);
                for q in p.chunks(2) {
                    if q.len() == 2 {
                        pts.push(x + q[0] * bw);
                        pts.push(y + q[1] * bh);
                    }
                }
            }
            (0, pts, counts)
        }
        "rect" => (1, box4, vec![]),
        "ellipse" => (2, box4, vec![]),
        "line" => (3, box4, vec![]),
        "arrow" => (4, box4, vec![]),
        "text" => (5, box4, vec![]),
        "note" => (6, vec![box4[0], box4[1]], vec![]),
        "hl" | "ul" | "so" | "sq" => {
            let k = match t { "hl" => 7, "ul" => 8, "so" => 9, _ => 10 };
            let mut p = Vec::new();
            for q in o["quads"].as_array().into_iter().flatten() {
                p.extend(nums(q));
            }
            (k, p, vec![])
        }
        _ => return Ok(()),
    };
    let mut e = vec![0u8; 512];
    let size = o["s"].as_f64().unwrap_or(14.0) as f32;
    let rc = unsafe {
        grove_pdfw_annot(h, page, kind, c.as_ptr(), a, w, pts.as_ptr(), pts.len() as c_int, counts.as_ptr(), counts.len() as c_int, text.as_ptr(), size, fillp, who.as_ptr(), e.as_mut_ptr() as *mut c_char, 512)
    };
    check(rc, &e).map(|_| ())
}

/// Write the document `spec` describes to `out`.
pub fn save(spec: &Spec, out: &Path) -> Result<(), String> {
    let mut e = vec![0u8; 512];
    let pw = |p: &str| cs(spec.passwords.get(p).map(String::as_str).unwrap_or(""));
    let h = unsafe { grove_pdfw_open(cs(&spec.primary).as_ptr(), pw(&spec.primary).as_ptr(), e.as_mut_ptr() as *mut c_char, 512) };
    if h.is_null() {
        return Err(err_text(&e));
    }
    let h = Handle(h);
    let own = if spec.primary.is_empty() { 0 } else { unsafe { grove_pdfw_count(h.0) } };
    let mut used = vec![false; own.max(0) as usize];
    let mut index = Vec::with_capacity(spec.pages.len());
    for p in &spec.pages {
        let i = if let Some([w, hh]) = p.blank {
            check(unsafe { grove_pdfw_blank(h.0, w, hh, e.as_mut_ptr() as *mut c_char, 512) }, &e)?
        } else if let Some(img) = &p.image {
            check(unsafe { grove_pdfw_image_page(h.0, cs(img).as_ptr(), 0.0, 0.0, e.as_mut_ptr() as *mut c_char, 512) }, &e)?
        } else if p.src == spec.primary && p.page >= 0 && p.page < own && !used[p.page as usize] {
            used[p.page as usize] = true;
            p.page
        } else {
            check(unsafe { grove_pdfw_graft(h.0, cs(&p.src).as_ptr(), pw(&p.src).as_ptr(), p.page, e.as_mut_ptr() as *mut c_char, 512) }, &e)?
        };
        index.push(i);
    }
    let who = cs(&author());
    for (p, &i) in spec.pages.iter().zip(&index) {
        if !p.del_annots.is_empty() {
            check(unsafe { grove_pdfw_delete_annots(h.0, i, p.del_annots.as_ptr(), p.del_annots.len() as c_int, e.as_mut_ptr() as *mut c_char, 512) }, &e)?;
        }
        for fl in &p.fields {
            check(unsafe { grove_pdfw_field(h.0, i, fl.i, cs(&fl.value).as_ptr(), e.as_mut_ptr() as *mut c_char, 512) }, &e)?;
        }
        for o in &p.annots {
            add_annot(h.0, i, o, &who)?;
        }
        if let Some([x0, y0, x1, y1]) = p.crop {
            check(unsafe { grove_pdfw_crop(h.0, i, x0, y0, x1, y1, e.as_mut_ptr() as *mut c_char, 512) }, &e)?;
        }
        if p.rot % 360 != 0 {
            check(unsafe { grove_pdfw_rotate(h.0, i, p.rot, e.as_mut_ptr() as *mut c_char, 512) }, &e)?;
        }
    }
    if index.is_empty() {
        return Err("A PDF needs at least one page.".into());
    }
    let total = unsafe { grove_pdfw_count(h.0) };
    let identity = total as usize == index.len() && index.iter().enumerate().all(|(k, &i)| k as c_int == i);
    if !identity {
        check(unsafe { grove_pdfw_reorder(h.0, index.as_ptr(), index.len() as c_int, e.as_mut_ptr() as *mut c_char, 512) }, &e)?;
    }
    for (k, v) in &spec.meta {
        check(unsafe { grove_pdfw_meta(h.0, cs(k).as_ptr(), cs(v).as_ptr(), e.as_mut_ptr() as *mut c_char, 512) }, &e)?;
    }
    let (u, o, pr, cp) = match &spec.encrypt {
        Some(x) => (x.user.as_str(), x.owner.as_str(), x.print, x.copy),
        None => ("", "", true, true),
    };
    check(unsafe { grove_pdfw_save(h.0, cs(&out.to_string_lossy()).as_ptr(), cs(u).as_ptr(), cs(o).as_ptr(), pr as c_int, cp as c_int, e.as_mut_ptr() as *mut c_char, 512) }, &e)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("grove-pdfedit-{}-{name}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        d
    }

    #[test]
    fn blank_pages_annotate_reorder_encrypt() {
        let d = tmp("a");
        let a = d.join("a.pdf");
        let spec: Spec = serde_json::from_value(serde_json::json!({
            "pages": [ { "blank": [612, 792] }, { "blank": [300, 200] } ]
        })).unwrap();
        save(&spec, &a).unwrap();
        let i: Value = serde_json::from_str(&info(a.to_str().unwrap(), "").unwrap()).unwrap();
        assert_eq!(i["pages"], 2);
        // Swap, rotate, annotate, encrypt.
        let b = d.join("b.pdf");
        let spec: Spec = serde_json::from_value(serde_json::json!({
            "primary": a.to_str().unwrap(),
            "pages": [
                { "src": a.to_str().unwrap(), "page": 1, "rot": 90 },
                { "src": a.to_str().unwrap(), "page": 0, "annots": [
                    { "t": "pen", "c": "#ff0000", "w": 3, "p": [10, 10, 50, 60, 90, 20] },
                    { "t": "rect", "c": "#0000ff", "w": 2, "x1": 100, "y1": 100, "x2": 200, "y2": 160 },
                    { "t": "arrow", "c": "#00ff00", "w": 2, "x1": 100, "y1": 300, "x2": 200, "y2": 360 },
                    { "t": "text", "c": "#000000", "s": 14, "x1": 50, "y1": 400, "x2": 300, "y2": 430, "text": "Hello" },
                    { "t": "note", "c": "#ffcc00", "x1": 500, "y1": 50, "text": "A note" },
                    { "t": "sign", "c": "#000000", "w": 1.5, "x1": 300, "y1": 600, "x2": 450, "y2": 650, "strokes": [[0, 0.5, 0.5, 0, 1, 1]] }
                ] },
                { "src": a.to_str().unwrap(), "page": 0 }
            ],
            "encrypt": { "user": "pw" }
        })).unwrap();
        save(&spec, &b).unwrap();
        let locked: Value = serde_json::from_str(&info(b.to_str().unwrap(), "").unwrap()).unwrap();
        assert_eq!(locked["locked"], true);
        let i: Value = serde_json::from_str(&info(b.to_str().unwrap(), "pw").unwrap()).unwrap();
        assert_eq!(i["pages"], 3);
        // The 300x200 page, turned: 200 wide.
        assert_eq!(i["sizes"][0][0].as_f64().unwrap().round(), 200.0);
        let an: Value = serde_json::from_str(&annots(b.to_str().unwrap(), "pw", 1).unwrap()).unwrap();
        assert_eq!(an.as_array().unwrap().len(), 6, "{an}");
        let (w, h, px) = render(b.to_str().unwrap(), "pw", 1, 306, &[]).unwrap();
        assert_eq!((w, h), (306, 396));
        assert_eq!(px.len(), (w * h * 4) as usize);
        // Remove two of them.
        let c = d.join("c.pdf");
        let spec: Spec = serde_json::from_value(serde_json::json!({
            "primary": b.to_str().unwrap(), "passwords": { b.to_str().unwrap(): "pw" },
            "pages": [ { "src": b.to_str().unwrap(), "page": 1, "delAnnots": [0, 1] } ]
        })).unwrap();
        save(&spec, &c).unwrap();
        let an: Value = serde_json::from_str(&annots(c.to_str().unwrap(), "", 0).unwrap()).unwrap();
        assert_eq!(an.as_array().unwrap().len(), 4);
        let _ = std::fs::remove_dir_all(&d);
    }
}
