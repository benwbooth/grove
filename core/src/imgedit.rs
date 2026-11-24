//! Grove Preview's pictures: open (any format the `image` crate reads, else
//! libvips), and the edit pipeline — Instant Alpha, Adjust Color, Markup,
//! crop, rotate/flip, Adjust Size — then export to PNG, JPEG, WebP, TIFF,
//! BMP, GIF... or PDF (through MuPDF, pdfedit). No Qt here.
//!
//! Markup ops are the Preview window's JSON objects in picture pixels:
//!   {t: pen, c, a, w, p:[x,y,...]}            freehand (a < 1: highlighter)
//!   {t: rect|rrect|ellipse, c, w, fill?, x1,y1,x2,y2}
//!   {t: star|poly|bubble, c, w, fill?, p:[closed outline]}
//!   {t: line|arrow, c, w, x1,y1,x2,y2}
//!   {t: text, c, s, font?, b?, i?, al?, fill?, x1,y1,x2,y2, text}
//!   {t: sign, c, w, x1,y1,x2,y2, strokes:[[0..1 x,y,...]]}
//!   {t: loupe, c, w, zoom, x1,y1,x2,y2}

use image::{DynamicImage, ImageDecoder, ImageReader, Rgba, RgbaImage};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

// ---------- opening ----------

/// The picture, turned as its EXIF orientation says.
pub fn load(path: &Path) -> Result<RgbaImage, String> {
    let r = ImageReader::open(path).map_err(|e| e.to_string())?.with_guessed_format().map_err(|e| e.to_string())?;
    let opened = r.into_decoder().and_then(|mut d| {
        let o = d.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
        let mut img = DynamicImage::from_decoder(d)?;
        img.apply_orientation(o);
        Ok(img)
    });
    match opened {
        Ok(img) => Ok(img.to_rgba8()),
        // HEIC, AVIF, RAW, JPEG XL, SVG...: libvips.
        Err(e) => {
            let png = crate::vips::thumbnail(path, 16384).ok_or_else(|| format!("This picture can't be opened ({e})."))?;
            let img = image::open(&png).map_err(|e| e.to_string());
            let _ = std::fs::remove_file(&png);
            Ok(img?.to_rgba8())
        }
    }
}

type Cached = (PathBuf, i64, u32, Arc<RgbaImage>, (u32, u32));

/// The picture at most `px` on its longer side, and its full size. The
/// last few are kept, so dragging a slider doesn't decode it again.
pub fn load_scaled(path: &Path, px: u32) -> Result<(Arc<RgbaImage>, (u32, u32)), String> {
    static CACHE: OnceLock<Mutex<Vec<Cached>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0);
    if let Some(c) = cache.lock().iter().find(|c| c.0 == path && c.1 == mtime && c.2 == px) {
        return Ok((c.3.clone(), c.4));
    }
    let full = load(path)?;
    let size = full.dimensions();
    let img = if size.0.max(size.1) > px {
        let k = px as f32 / size.0.max(size.1) as f32;
        image::imageops::resize(&full, ((size.0 as f32 * k).round() as u32).max(1), ((size.1 as f32 * k).round() as u32).max(1), image::imageops::FilterType::Triangle)
    } else {
        full
    };
    let img = Arc::new(img);
    let mut c = cache.lock();
    c.push((path.to_path_buf(), mtime, px, img.clone(), size));
    if c.len() > 4 {
        c.remove(0);
    }
    Ok((img, size))
}

// ---------- the edit spec ----------

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Spec {
    #[serde(default)]
    pub alpha: Vec<AlphaMark>,
    #[serde(default)]
    pub adjust: Adjust,
    #[serde(default)]
    pub ops: Vec<Value>,
    #[serde(default)]
    pub crop: Option<[f32; 4]>,
    #[serde(default)]
    pub rotate: i32,
    #[serde(default, rename = "flipH")]
    pub flip_h: bool,
    #[serde(default, rename = "flipV")]
    pub flip_v: bool,
    #[serde(default)]
    pub resize: Option<[u32; 2]>,
    #[serde(default)]
    pub dpi: Option<f32>,
}

#[derive(Debug, Deserialize, Clone, Copy)]
pub struct AlphaMark {
    pub x: f32,
    pub y: f32,
    #[serde(default = "tol")]
    pub tol: f32,
}
fn tol() -> f32 {
    40.0
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
#[serde(default)]
pub struct Adjust {
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub saturation: f32,
    pub temperature: f32,
    pub tint: f32,
    pub sepia: f32,
    pub sharpness: f32,
    pub black: f32,
    pub white: f32,
    pub gamma: f32,
}

impl Default for Adjust {
    fn default() -> Self {
        Adjust { exposure: 0.0, contrast: 0.0, highlights: 0.0, shadows: 0.0, saturation: 0.0, temperature: 0.0, tint: 0.0, sepia: 0.0, sharpness: 0.0, black: 0.0, white: 1.0, gamma: 1.0 }
    }
}

impl Adjust {
    pub fn is_identity(&self) -> bool {
        *self == Adjust::default()
    }
}

fn c255(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// Adjust Color: levels, exposure, highlights/shadows, contrast (one tone
/// curve), then saturation, temperature, tint, sepia, sharpness.
pub fn apply_adjust(img: &mut RgbaImage, a: &Adjust) {
    if a.is_identity() {
        return;
    }
    let range = (a.white - a.black).max(0.02);
    let expo = 2f32.powf(a.exposure);
    let cf = if a.contrast >= 0.0 { 1.0 + a.contrast * 1.5 } else { 1.0 + a.contrast * 0.8 };
    let mut lut = [0u8; 256];
    for (i, l) in lut.iter_mut().enumerate() {
        let mut v = ((i as f32 / 255.0 - a.black) / range).clamp(0.0, 1.0);
        v = v.powf(1.0 / a.gamma.max(0.1));
        v = (v * expo).clamp(0.0, 1.0);
        v += a.shadows * 1.2 * (1.0 - v) * (1.0 - v) * v;
        v += a.highlights * 1.2 * v * v * (1.0 - v) + if a.highlights < 0.0 { a.highlights * 0.25 * v * v * v } else { 0.0 };
        v = (v - 0.5) * cf + 0.5;
        *l = c255(v * 255.0);
    }
    let sat = 1.0 + a.saturation;
    let (rt, bt, gt) = (1.0 + 0.18 * a.temperature, 1.0 - 0.18 * a.temperature, 1.0 - 0.14 * a.tint);
    let sp = a.sepia.clamp(0.0, 1.0);
    for p in img.pixels_mut() {
        let (mut r, mut g, mut b) = (lut[p[0] as usize] as f32, lut[p[1] as usize] as f32, lut[p[2] as usize] as f32);
        if sat != 1.0 {
            let l = 0.299 * r + 0.587 * g + 0.114 * b;
            r = l + (r - l) * sat;
            g = l + (g - l) * sat;
            b = l + (b - l) * sat;
        }
        r *= rt;
        b *= bt;
        g *= gt;
        if sp > 0.0 {
            let (sr, sg, sb) = (0.393 * r + 0.769 * g + 0.189 * b, 0.349 * r + 0.686 * g + 0.168 * b, 0.272 * r + 0.534 * g + 0.131 * b);
            r += (sr - r) * sp;
            g += (sg - g) * sp;
            b += (sb - b) * sp;
        }
        p.0 = [c255(r), c255(g), c255(b), p[3]];
    }
    if a.sharpness != 0.0 && img.width() > 2 && img.height() > 2 {
        // Unsharp mask (a soft blur below zero) with a 3x3 box.
        let src = img.clone();
        let k = if a.sharpness > 0.0 { a.sharpness * 1.6 } else { a.sharpness };
        let (w, h) = img.dimensions();
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let mut s = [0f32; 3];
                for dy in 0..3 {
                    for dx in 0..3 {
                        let q = src.get_pixel(x + dx - 1, y + dy - 1);
                        for c in 0..3 {
                            s[c] += q[c] as f32;
                        }
                    }
                }
                let o = src.get_pixel(x, y);
                let mut n = *o;
                for c in 0..3 {
                    n[c] = c255(o[c] as f32 + k * (o[c] as f32 - s[c] / 9.0));
                }
                img.put_pixel(x, y, n);
            }
        }
    }
}

/// Instant Alpha: the area around each mark, of about the mark's colour,
/// made transparent (flood fill). `scale`: picture pixels to these pixels.
pub fn apply_alpha(img: &mut RgbaImage, marks: &[AlphaMark], scale: f32) {
    let (w, h) = img.dimensions();
    for m in marks {
        let (sx, sy) = ((m.x * scale) as i64, (m.y * scale) as i64);
        if sx < 0 || sy < 0 || sx >= w as i64 || sy >= h as i64 {
            continue;
        }
        let seed = *img.get_pixel(sx as u32, sy as u32);
        let t2 = m.tol * m.tol * 3.0;
        let mut seen = vec![false; (w * h) as usize];
        let mut stack = vec![(sy as u32) * w + sx as u32];
        while let Some(i) = stack.pop() {
            if std::mem::replace(&mut seen[i as usize], true) {
                continue;
            }
            let (x, y) = (i % w, i / w);
            let p = img.get_pixel_mut(x, y);
            let d: f32 = (0..3).map(|c| (p[c] as f32 - seed[c] as f32).powi(2)).sum();
            if d > t2 || (p[3] == 0 && seed[3] != 0) {
                continue;
            }
            p[3] = 0;
            if x > 0 {
                stack.push(i - 1);
            }
            if x + 1 < w {
                stack.push(i + 1);
            }
            if y > 0 {
                stack.push(i - w);
            }
            if y + 1 < h {
                stack.push(i + w);
            }
        }
    }
}

// ---------- Markup ----------

fn color(v: &Value, alpha: f32) -> Option<tiny_skia::Color> {
    let s = v.as_str()?.trim_start_matches('#');
    if s.len() < 6 {
        return None;
    }
    let n = u32::from_str_radix(&s[0..6], 16).ok()?;
    let a = if s.len() >= 8 { u8::from_str_radix(&s[6..8], 16).ok()? as f32 / 255.0 } else { 1.0 };
    Some(tiny_skia::Color::from_rgba8((n >> 16) as u8, (n >> 8) as u8, n as u8, (a * alpha * 255.0).round().clamp(0.0, 255.0) as u8))
}

fn num(o: &Value, k: &str) -> f32 {
    o[k].as_f64().unwrap_or(0.0) as f32
}

fn points(v: &Value) -> Vec<(f32, f32)> {
    let n: Vec<f32> = v.as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_default();
    n.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0], c[1])).collect()
}

fn to_pixmap(img: &RgbaImage) -> Pixmap {
    let mut pm = Pixmap::new(img.width().max(1), img.height().max(1)).unwrap();
    for (d, s) in pm.pixels_mut().iter_mut().zip(img.pixels()) {
        *d = tiny_skia::ColorU8::from_rgba(s[0], s[1], s[2], s[3]).premultiply();
    }
    pm
}

fn from_pixmap(pm: &Pixmap, img: &mut RgbaImage) {
    for (s, d) in pm.pixels().iter().zip(img.pixels_mut()) {
        let c = s.demultiply();
        d.0 = [c.red(), c.green(), c.blue(), c.alpha()];
    }
}

fn fonts() -> &'static fontdb::Database {
    static DB: OnceLock<fontdb::Database> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        if let Ok(d) = std::env::var("GROVE_FONT_DIR") {
            db.load_fonts_dir(d);
        }
        db.set_sans_serif_family("Inter");
        db
    })
}

/// Load the system's font list ahead of the first save with text.
pub fn warm_fonts() {
    let _ = fonts();
}

/// A font by family name (sans-serif, serif, monospace or a real name).
fn font(family: &str, bold: bool, italic: bool) -> Option<ab_glyph::FontArc> {
    let db = fonts();
    let fam = match family.to_lowercase().as_str() {
        "" | "sans-serif" | "sans" => fontdb::Family::SansSerif,
        "serif" => fontdb::Family::Serif,
        "monospace" | "mono" => fontdb::Family::Monospace,
        _ => fontdb::Family::Name(family),
    };
    let q = fontdb::Query {
        families: &[fam, fontdb::Family::SansSerif],
        weight: if bold { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL },
        style: if italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
        ..Default::default()
    };
    let id = db.query(&q).or_else(|| db.faces().next().map(|f| f.id))?;
    db.with_face_data(id, |data, index| ab_glyph::FontVec::try_from_vec_and_index(data.to_vec(), index).ok().map(ab_glyph::FontArc::new)).flatten()
}

/// Text wrapped into `box_` (picture pixels), drawn into the pixmap.
fn draw_text(pm: &mut Pixmap, o: &Value, col: tiny_skia::Color) {
    use ab_glyph::{Font, ScaleFont};
    let Some(f) = font(o["font"].as_str().unwrap_or(""), o["b"].as_bool().unwrap_or(false), o["i"].as_bool().unwrap_or(false)) else { return };
    let size = num(o, "s").max(6.0);
    let sf = f.as_scaled(size);
    let (x1, y1, x2, y2) = (num(o, "x1").min(num(o, "x2")), num(o, "y1").min(num(o, "y2")), num(o, "x1").max(num(o, "x2")), num(o, "y1").max(num(o, "y2")));
    let maxw = (x2 - x1 - 4.0).max(size);
    let width = |s: &str| -> f32 {
        let mut w = 0.0;
        let mut prev = None;
        for ch in s.chars() {
            let g = sf.glyph_id(ch);
            if let Some(p) = prev {
                w += sf.kern(p, g);
            }
            w += sf.h_advance(g);
            prev = Some(g);
        }
        w
    };
    // Lines: the text's own, wrapped at spaces (or anywhere for long words).
    let mut lines: Vec<String> = Vec::new();
    for para in o["text"].as_str().unwrap_or("").split('\n') {
        let mut cur = String::new();
        for word in para.split(' ') {
            let cand = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
            if width(&cand) <= maxw || cur.is_empty() {
                cur = cand;
                while width(&cur) > maxw && cur.chars().count() > 1 {
                    let mut cut = cur.chars().count() - 1;
                    while cut > 1 && width(&cur.chars().take(cut).collect::<String>()) > maxw {
                        cut -= 1;
                    }
                    lines.push(cur.chars().take(cut).collect());
                    cur = cur.chars().skip(cut).collect();
                }
            } else {
                lines.push(std::mem::take(&mut cur));
                cur = word.to_string();
            }
        }
        lines.push(cur);
    }
    let align = o["al"].as_str().unwrap_or("left");
    let line_h = sf.height() + sf.line_gap();
    let (pw, ph) = (pm.width() as i32, pm.height() as i32);
    let c8 = col.to_color_u8();
    let data = pm.pixels_mut();
    for (n, line) in lines.iter().enumerate() {
        let lw = width(line);
        let mut x = match align { "center" => x1 + (x2 - x1 - lw) / 2.0, "right" => x2 - 2.0 - lw, _ => x1 + 2.0 };
        let base = y1 + sf.ascent() + n as f32 * line_h;
        if base - sf.ascent() > y2 + 1.0 && n > 0 {
            break;
        }
        let mut prev = None;
        for ch in line.chars() {
            let gid = sf.glyph_id(ch);
            if let Some(p) = prev {
                x += sf.kern(p, gid);
            }
            let g = gid.with_scale_and_position(size, ab_glyph::point(x, base));
            if let Some(og) = f.outline_glyph(g) {
                let bb = og.px_bounds();
                og.draw(|gx, gy, cov| {
                    let (px, py) = (bb.min.x as i32 + gx as i32, bb.min.y as i32 + gy as i32);
                    if px < 0 || py < 0 || px >= pw || py >= ph {
                        return;
                    }
                    let a = cov.clamp(0.0, 1.0) * c8.alpha() as f32 / 255.0;
                    let d = &mut data[(py * pw + px) as usize];
                    let inv = 1.0 - a;
                    let mix = |s: u8, dv: u8| ((s as f32 * a) + dv as f32 * inv).round().clamp(0.0, 255.0) as u8;
                    *d = tiny_skia::PremultipliedColorU8::from_rgba(mix(c8.red(), d.red()), mix(c8.green(), d.green()), mix(c8.blue(), d.blue()), ((a * 255.0) + d.alpha() as f32 * inv).round().clamp(0.0, 255.0) as u8)
                        .unwrap_or(*d);
                });
            }
            x += sf.h_advance(gid);
            prev = Some(gid);
        }
    }
}

/// Draw Markup ops onto the picture.
pub fn draw_ops(img: &mut RgbaImage, ops: &[Value]) {
    if ops.is_empty() {
        return;
    }
    let source = img.clone();
    let mut pm = to_pixmap(img);
    for o in ops {
        let t = o["t"].as_str().unwrap_or("");
        let alpha = o["a"].as_f64().unwrap_or(1.0) as f32;
        let col = color(&o["c"], alpha).unwrap_or(tiny_skia::Color::from_rgba8(255, 59, 48, 255));
        let fill = color(&o["fill"], 1.0);
        let w = o["w"].as_f64().unwrap_or(4.0) as f32;
        let mut paint = Paint::default();
        paint.set_color(col);
        paint.anti_alias = true;
        let stroke = Stroke { width: w.max(0.5), line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
        let (x1, y1, x2, y2) = (num(o, "x1"), num(o, "y1"), num(o, "x2"), num(o, "y2"));
        let rect = Rect::from_ltrb(x1.min(x2), y1.min(y2), x1.max(x2).max(x1.min(x2) + 0.5), y1.max(y2).max(y1.min(y2) + 0.5));
        let shape = |pm: &mut Pixmap, pb: PathBuilder, closed_fill: bool| {
            if let Some(path) = pb.finish() {
                if closed_fill {
                    if let Some(fc) = fill {
                        let mut fp = Paint::default();
                        fp.set_color(fc);
                        fp.anti_alias = true;
                        pm.fill_path(&path, &fp, FillRule::Winding, Transform::identity(), None);
                    }
                }
                pm.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
        };
        match t {
            "pen" => {
                let pts = points(&o["p"]);
                let mut pb = PathBuilder::new();
                for (i, (x, y)) in pts.iter().enumerate() {
                    if i == 0 { pb.move_to(*x, *y) } else { pb.line_to(*x, *y) }
                }
                if pts.len() == 1 {
                    pb.line_to(pts[0].0 + 0.01, pts[0].1);
                }
                shape(&mut pm, pb, false);
            }
            "sign" => {
                let mut pb = PathBuilder::new();
                for s in o["strokes"].as_array().into_iter().flatten() {
                    for (i, (x, y)) in points(s).iter().enumerate() {
                        let (px, py) = (x1.min(x2) + x * (x2 - x1).abs(), y1.min(y2) + y * (y2 - y1).abs());
                        if i == 0 { pb.move_to(px, py) } else { pb.line_to(px, py) }
                    }
                }
                shape(&mut pm, pb, false);
            }
            "rect" => {
                if let Some(r) = rect {
                    let mut pb = PathBuilder::new();
                    pb.push_rect(r);
                    shape(&mut pm, pb, true);
                }
            }
            "rrect" => {
                if let Some(r) = rect {
                    let k = r.width().min(r.height()) * 0.18;
                    let mut pb = PathBuilder::new();
                    let (l, tp, rr, b) = (r.left(), r.top(), r.right(), r.bottom());
                    pb.move_to(l + k, tp);
                    pb.line_to(rr - k, tp);
                    pb.quad_to(rr, tp, rr, tp + k);
                    pb.line_to(rr, b - k);
                    pb.quad_to(rr, b, rr - k, b);
                    pb.line_to(l + k, b);
                    pb.quad_to(l, b, l, b - k);
                    pb.line_to(l, tp + k);
                    pb.quad_to(l, tp, l + k, tp);
                    pb.close();
                    shape(&mut pm, pb, true);
                }
            }
            "ellipse" => {
                if let Some(r) = rect {
                    if let Some(p) = PathBuilder::from_oval(r) {
                        let mut pb = PathBuilder::new();
                        pb.push_path(&p);
                        shape(&mut pm, pb, true);
                    }
                }
            }
            "star" | "poly" | "bubble" => {
                let pts = points(&o["p"]);
                let mut pb = PathBuilder::new();
                for (i, (x, y)) in pts.iter().enumerate() {
                    if i == 0 { pb.move_to(*x, *y) } else { pb.line_to(*x, *y) }
                }
                pb.close();
                shape(&mut pm, pb, true);
            }
            "line" | "arrow" => {
                let mut pb = PathBuilder::new();
                pb.move_to(x1, y1);
                pb.line_to(x2, y2);
                if t == "arrow" {
                    let ang = (y1 - y2).atan2(x1 - x2);
                    let len = (w * 4.0).max(12.0);
                    for d in [0.49f32, -0.49] {
                        pb.move_to(x2, y2);
                        pb.line_to(x2 + (ang + d).cos() * len, y2 + (ang + d).sin() * len);
                    }
                }
                shape(&mut pm, pb, false);
            }
            "text" => {
                if let (Some(fc), Some(r)) = (fill, rect) {
                    let mut fp = Paint::default();
                    fp.set_color(fc);
                    pm.fill_rect(r, &fp, Transform::identity(), None);
                }
                draw_text(&mut pm, o, col);
            }
            "loupe" => {
                // The picture under the circle, magnified.
                if let Some(r) = rect {
                    let zoom = o["zoom"].as_f64().unwrap_or(2.0).max(1.0) as f32;
                    let (cx, cy, rx, ry) = ((r.left() + r.right()) / 2.0, (r.top() + r.bottom()) / 2.0, r.width() / 2.0, r.height() / 2.0);
                    let (pw, ph) = (pm.width() as i32, pm.height() as i32);
                    let data = pm.pixels_mut();
                    for y in r.top().floor() as i32..r.bottom().ceil() as i32 {
                        for x in r.left().floor() as i32..r.right().ceil() as i32 {
                            if x < 0 || y < 0 || x >= pw || y >= ph {
                                continue;
                            }
                            let (dx, dy) = ((x as f32 + 0.5 - cx) / rx, (y as f32 + 0.5 - cy) / ry);
                            if dx * dx + dy * dy > 1.0 {
                                continue;
                            }
                            let sx = (cx + (x as f32 + 0.5 - cx) / zoom).clamp(0.0, source.width() as f32 - 1.0) as u32;
                            let sy = (cy + (y as f32 + 0.5 - cy) / zoom).clamp(0.0, source.height() as f32 - 1.0) as u32;
                            let s = source.get_pixel(sx, sy);
                            data[(y * pw + x) as usize] = tiny_skia::ColorU8::from_rgba(s[0], s[1], s[2], s[3]).premultiply();
                        }
                    }
                    if let Some(p) = PathBuilder::from_oval(r) {
                        let mut pb = PathBuilder::new();
                        pb.push_path(&p);
                        shape(&mut pm, pb, false);
                    }
                }
            }
            "hl" => {
                // A see-through highlight over rectangles.
                let mut hp = Paint::default();
                let mut hc = col;
                hc.set_alpha(0.35);
                hp.set_color(hc);
                for q in o["quads"].as_array().into_iter().flatten() {
                    let pts = points(q);
                    let (mut l, mut tp, mut rr, mut b) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                    for (x, y) in pts {
                        l = l.min(x);
                        tp = tp.min(y);
                        rr = rr.max(x);
                        b = b.max(y);
                    }
                    if let Some(r) = Rect::from_ltrb(l, tp, rr, b) {
                        pm.fill_rect(r, &hp, Transform::identity(), None);
                    }
                }
            }
            _ => {}
        }
    }
    from_pixmap(&pm, img);
}

/// The whole pipeline: Instant Alpha, Adjust Color, Markup, crop,
/// rotate/flip, Adjust Size.
pub fn pipeline(mut img: RgbaImage, spec: &Spec) -> RgbaImage {
    apply_alpha(&mut img, &spec.alpha, 1.0);
    apply_adjust(&mut img, &spec.adjust);
    draw_ops(&mut img, &spec.ops);
    if let Some([x, y, w, h]) = spec.crop {
        let (iw, ih) = img.dimensions();
        let x0 = (x.max(0.0) as u32).min(iw.saturating_sub(1));
        let y0 = (y.max(0.0) as u32).min(ih.saturating_sub(1));
        let cw = (w.round() as u32).min(iw - x0);
        let ch = (h.round() as u32).min(ih - y0);
        if cw > 1 && ch > 1 {
            img = image::imageops::crop_imm(&img, x0, y0, cw, ch).to_image();
        }
    }
    img = match spec.rotate.rem_euclid(360) {
        90 => image::imageops::rotate90(&img),
        180 => image::imageops::rotate180(&img),
        270 => image::imageops::rotate270(&img),
        _ => img,
    };
    if spec.flip_h {
        image::imageops::flip_horizontal_in_place(&mut img);
    }
    if spec.flip_v {
        image::imageops::flip_vertical_in_place(&mut img);
    }
    if let Some([w, h]) = spec.resize {
        if w > 0 && h > 0 && (w, h) != img.dimensions() {
            img = image::imageops::resize(&img, w, h, image::imageops::FilterType::Lanczos3);
        }
    }
    img
}

/// The formats pictures can be exported as (lower-case names).
pub const FORMATS: &[&str] = &["png", "jpeg", "webp", "tiff", "bmp", "gif", "pdf"];

/// Encode a picture as `format` (quality 1-100 for JPEG).
pub fn encode(img: &RgbaImage, format: &str, quality: u8, dpi: f32, dest: &Path) -> Result<(), String> {
    let f = format.to_lowercase();
    let out = || std::fs::File::create(dest).map(std::io::BufWriter::new).map_err(|e| e.to_string());
    match f.as_str() {
        "jpeg" | "jpg" => {
            // On white where it was see-through.
            let mut rgb = image::RgbImage::new(img.width(), img.height());
            for (d, s) in rgb.pixels_mut().zip(img.pixels()) {
                let a = s[3] as f32 / 255.0;
                d.0 = [0, 1, 2].map(|c| (s[c] as f32 * a + 255.0 * (1.0 - a)).round() as u8);
            }
            let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(out()?, quality.clamp(1, 100));
            enc.encode_image(&rgb).map_err(|e| e.to_string())
        }
        "pdf" => {
            let png = crate::previews::temp_file("png");
            img.save_with_format(&png, image::ImageFormat::Png).map_err(|e| e.to_string())?;
            let k = 72.0 / if dpi > 1.0 { dpi } else { 72.0 };
            let r = crate::pdfedit::image_pdf(&[(png.clone(), img.width() as f32 * k, img.height() as f32 * k)], dest);
            let _ = std::fs::remove_file(&png);
            r
        }
        _ => {
            let fmt = match f.as_str() {
                "png" => image::ImageFormat::Png,
                "webp" => image::ImageFormat::WebP,
                "tiff" | "tif" => image::ImageFormat::Tiff,
                "bmp" => image::ImageFormat::Bmp,
                "gif" => image::ImageFormat::Gif,
                "ico" => image::ImageFormat::Ico,
                "tga" => image::ImageFormat::Tga,
                "qoi" => image::ImageFormat::Qoi,
                _ => return Err(format!("Pictures can't be saved as {format}.")),
            };
            let mut w = out()?;
            DynamicImage::ImageRgba8(img.clone()).write_to(&mut w, fmt).map_err(|e| e.to_string())
        }
    }
}

/// Open `src`, run the pipeline, write `dest` as `format`.
pub fn export(src: &Path, spec: &Spec, dest: &Path, format: &str, quality: u8) -> Result<(), String> {
    let img = pipeline(load(src)?, spec);
    encode(&img, format, quality, spec.dpi.unwrap_or(72.0), dest)
}

/// Size, format, histogram (64 bins), automatic levels (0.5% clipped),
/// file size.
pub fn info(path: &Path) -> Value {
    let (img, (w, h)) = match load_scaled(path, 512) {
        Ok(v) => v,
        Err(e) => return json!({"error": e}),
    };
    let format = ImageReader::open(path).ok().and_then(|r| r.with_guessed_format().ok()).and_then(|r| r.format()).map(|f| format!("{f:?}")).unwrap_or_default();
    let mut hist = [0u64; 256];
    let mut n = 0u64;
    let mut alpha = false;
    for p in img.pixels() {
        if p[3] < 255 {
            alpha = true;
        }
        if p[3] < 16 {
            continue;
        }
        hist[((p[0] as u32 * 299 + p[1] as u32 * 587 + p[2] as u32 * 114) / 1000) as usize] += 1;
        n += 1;
    }
    let mut bins = [0u64; 64];
    for (i, v) in hist.iter().enumerate() {
        bins[i / 4] += v;
    }
    let peak = bins.iter().copied().max().unwrap_or(1).max(1);
    let clip = n / 200;
    let (mut acc, mut lo) = (0, 0);
    for (i, v) in hist.iter().enumerate() {
        acc += v;
        if acc > clip {
            lo = i;
            break;
        }
    }
    let (mut acc, mut hi) = (0, 255);
    for (i, v) in hist.iter().enumerate().rev() {
        acc += v;
        if acc > clip {
            hi = i;
            break;
        }
    }
    if hi <= lo {
        (lo, hi) = (0, 255);
    }
    json!({
        "w": w, "h": h, "format": format, "alpha": alpha,
        "hist": bins.iter().map(|b| *b as f64 / peak as f64).collect::<Vec<_>>(),
        "autoBlack": lo as f64 / 255.0, "autoWhite": hi as f64 / 255.0,
        "bytes": std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        "formats": FORMATS,
    })
}

/// The picture (scaled to `px`) with Instant Alpha and Adjust Color, for
/// showing while editing.
pub fn preview(path: &Path, px: u32, alpha: &[AlphaMark], adjust: &Adjust) -> Result<RgbaImage, String> {
    let (img, (fw, fh)) = load_scaled(path, px)?;
    let mut img = (*img).clone();
    let scale = img.width() as f32 / fw.max(1) as f32;
    let _ = fh;
    apply_alpha(&mut img, alpha, scale);
    apply_adjust(&mut img, adjust);
    Ok(img)
}

/// A solid image, for tests.
pub fn solid(w: u32, h: u32, c: [u8; 4]) -> RgbaImage {
    RgbaImage::from_pixel(w, h, Rgba(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_crop_rotate_resize_alpha() {
        let mut img = solid(200, 100, [10, 200, 30, 255]);
        img.put_pixel(150, 50, Rgba([255, 0, 0, 255]));
        let spec: Spec = serde_json::from_value(json!({
            "alpha": [{"x": 5, "y": 5, "tol": 10}],
            "ops": [
                {"t": "rect", "c": "#0000ff", "w": 4, "x1": 10, "y1": 10, "x2": 60, "y2": 60, "fill": "#ffffff"},
                {"t": "pen", "c": "#ff0000", "w": 3, "p": [0, 0, 100, 100]},
                {"t": "text", "c": "#000000", "s": 14, "x1": 0, "y1": 0, "x2": 200, "y2": 40, "text": "Hello"},
                {"t": "loupe", "c": "#888888", "w": 2, "zoom": 2, "x1": 120, "y1": 20, "x2": 180, "y2": 80}
            ],
            "crop": [0, 0, 160, 100], "rotate": 90, "resize": [50, 80]
        })).unwrap();
        let out = pipeline(img, &spec);
        assert_eq!(out.dimensions(), (50, 80));
    }

    #[test]
    fn adjust_changes_pixels_and_identity_keeps_them() {
        let mut a = solid(4, 4, [100, 100, 100, 255]);
        apply_adjust(&mut a, &Adjust::default());
        assert_eq!(a.get_pixel(0, 0).0, [100, 100, 100, 255]);
        let adj = Adjust { exposure: 1.0, ..Default::default() };
        apply_adjust(&mut a, &adj);
        assert!(a.get_pixel(0, 0)[0] > 150);
    }

    #[test]
    fn export_formats() {
        let d = std::env::temp_dir().join(format!("grove-imgedit-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let src = d.join("a.png");
        solid(64, 32, [200, 10, 10, 255]).save(&src).unwrap();
        for f in FORMATS {
            let out = d.join(format!("out.{f}"));
            export(&src, &Spec::default(), &out, f, 85).unwrap_or_else(|e| panic!("{f}: {e}"));
            assert!(std::fs::metadata(&out).unwrap().len() > 0, "{f}");
        }
        let i = info(&src);
        assert_eq!(i["w"], 64);
        let _ = std::fs::remove_dir_all(&d);
    }
}
