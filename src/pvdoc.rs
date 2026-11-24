//! Grove Preview's controller calls (declared in bridge.rs): documents and
//! pictures read, searched, selected, saved, exported, printed and copied
//! off the UI thread; answers come back through async_reply.

use crate::bridge::ffi::{self, QString};
use crate::controller::save_edit;
use cxx_qt::Threading;
use grove_core::{imgedit, pdfedit, util};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Kinds MuPDF opens as paged documents; everything else is a picture.
pub fn is_document(path: &str) -> bool {
    let ext = Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "pdf" | "xps" | "oxps" | "epub" | "cbz" | "fb2" | "mobi" | "ai")
}

fn res(r: Result<String, String>) -> Value {
    match r {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| json!({"error": e.to_string()})),
        Err(e) => json!({"error": e}),
    }
}

fn signatures_file() -> PathBuf {
    util::config_home().join("grove/signatures.json")
}

/// Write to `dest` through a temporary file beside it.
fn write_to(dest: &Path, make: impl Fn(&Path) -> Result<(), String>) -> Value {
    let dir = dest.parent().unwrap_or(Path::new("/"));
    let tmp = dir.join(format!(".{}.grove-part", dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
    if let Err(e) = make(&tmp) {
        let _ = std::fs::remove_file(&tmp);
        return json!({"error": e});
    }
    if let Err(e) = std::fs::rename(&tmp, dest) {
        let _ = std::fs::remove_file(&tmp);
        return json!({"error": e.to_string()});
    }
    crate::controller::push_history_any("Export", grove_core::ops::Action::Exists { paths: vec![dest.to_path_buf()] });
    json!({"path": dest.to_string_lossy(), "replaced": false})
}

impl ffi::Grove {
    pub fn pv_info(&self, key: &QString, path: &QString, password: &QString) {
        let (p, pw) = (path.to_string(), password.to_string());
        self.answer_on("pv", key, move || {
            if is_document(&p) {
                let mut v = res(pdfedit::info(&p, &pw));
                v["kind"] = json!("pdf");
                v
            } else {
                let mut v = imgedit::info(Path::new(&p));
                v["kind"] = json!("image");
                v
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn pv_select(&self, key: &QString, path: &QString, password: &QString, page: i32, ax: f64, ay: f64, bx: f64, by: f64, mode: i32) {
        let (p, pw) = (path.to_string(), password.to_string());
        self.answer_on("pv", key, move || res(pdfedit::select(&p, &pw, page.max(0) as u32, (ax as f32, ay as f32), (bx as f32, by as f32), mode)));
    }

    pub fn pv_search(&self, key: &QString, path: &QString, password: &QString, needle: &QString) {
        let (p, pw, n) = (path.to_string(), password.to_string(), needle.to_string());
        self.answer_on("pvsearch", key, move || res(pdfedit::search(&p, &pw, &n, 2000)));
    }

    pub fn pv_links(&self, key: &QString, path: &QString, password: &QString, page: i32) {
        let (p, pw) = (path.to_string(), password.to_string());
        self.answer_on("pv", key, move || res(pdfedit::links(&p, &pw, page.max(0) as u32)));
    }

    pub fn pv_widgets(&self, key: &QString, path: &QString, password: &QString) {
        let (p, pw) = (path.to_string(), password.to_string());
        self.answer_on("pv", key, move || res(pdfedit::widgets(&p, &pw)));
    }

    pub fn pv_annots(&self, key: &QString, path: &QString, password: &QString, page: i32) {
        let (p, pw) = (path.to_string(), password.to_string());
        self.answer_on("pv", key, move || res(pdfedit::annots(&p, &pw, page.max(0) as u32)));
    }

    pub fn pv_save_pdf(&self, key: &QString, path: &QString, spec: &QString, mode: &QString, dest: &QString) {
        let (p, s, mode, dest) = (PathBuf::from(path.to_string()), spec.to_string(), mode.to_string(), dest.to_string());
        let qt = self.qt_thread();
        self.answer(key, move || {
            let spec: pdfedit::Spec = match serde_json::from_str(&s) {
                Ok(v) => v,
                Err(e) => return json!({"error": e.to_string()}),
            };
            let make = |d: &Path| pdfedit::save(&spec, d);
            if mode == "to" {
                write_to(Path::new(&dest), make)
            } else {
                save_edit(&p, &mode, "edited", "pdf", "Save", &qt, make)
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn pv_export_image(&self, key: &QString, src: &QString, spec: &QString, mode: &QString, dest: &QString, format: &QString, quality: i32) {
        let (p, s, mode, dest, fmt) = (PathBuf::from(src.to_string()), spec.to_string(), mode.to_string(), dest.to_string(), format.to_string());
        let qt = self.qt_thread();
        self.answer(key, move || {
            let spec: imgedit::Spec = match serde_json::from_str(&s) {
                Ok(v) => v,
                Err(e) => return json!({"error": e.to_string()}),
            };
            let q = quality.clamp(1, 100) as u8;
            let make = |d: &Path| imgedit::export(&p, &spec, d, &fmt, q);
            if mode == "to" {
                write_to(Path::new(&dest), make)
            } else {
                let ext = if fmt == "jpeg" { p.extension().map(|e| e.to_string_lossy().to_lowercase()).filter(|e| e == "jpg" || e == "jpeg").unwrap_or_else(|| "jpg".into()) } else { fmt.clone() };
                save_edit(&p, &mode, "edited", &ext, "Save", &qt, make)
            }
        });
    }

    pub fn pv_images_pdf(&self, key: &QString, items: &QString, dest: &QString) {
        let (items, dest) = (items.to_string(), PathBuf::from(dest.to_string()));
        self.answer(key, move || {
            let list: Vec<Value> = serde_json::from_str(&items).unwrap_or_default();
            write_to(&dest, |d| {
                let mut pics = Vec::new();
                let mut temps = Vec::new();
                for it in &list {
                    let src = PathBuf::from(it["path"].as_str().unwrap_or(""));
                    let spec: imgedit::Spec = serde_json::from_value(it["spec"].clone()).unwrap_or_default();
                    let img = imgedit::pipeline(imgedit::load(&src)?, &spec);
                    let png = grove_core::previews::temp_file("png");
                    imgedit::encode(&img, "png", 100, 72.0, &png)?;
                    let k = 72.0 / spec.dpi.unwrap_or(72.0).max(1.0);
                    pics.push((png.clone(), img.width() as f32 * k, img.height() as f32 * k));
                    temps.push(png);
                }
                let r = pdfedit::image_pdf(&pics, d);
                for t in temps {
                    let _ = std::fs::remove_file(t);
                }
                r
            })
        });
    }

    pub fn pv_printers(&self, key: &QString) {
        self.answer_on("print", key, || serde_json::from_str(&pdfedit::printers()).unwrap_or_else(|_| json!([])));
    }

    pub fn pv_print(&self, key: &QString, job: &QString) {
        let job = job.to_string();
        self.answer_on("print", key, move || {
            let j: Value = serde_json::from_str(&job).unwrap_or_default();
            let path = PathBuf::from(j["path"].as_str().unwrap_or(""));
            let printer = j["printer"].as_str().unwrap_or("").to_string();
            let title = j["title"].as_str().unwrap_or("Document").to_string();
            // Edits first: the document or picture as it looks now.
            let mut temp = None;
            let file = if !j["pdf"].is_null() {
                let spec: pdfedit::Spec = match serde_json::from_value(j["pdf"].clone()) {
                    Ok(s) => s,
                    Err(e) => return json!({"error": e.to_string()}),
                };
                let t = grove_core::previews::temp_file("pdf");
                if let Err(e) = pdfedit::save(&spec, &t) {
                    return json!({"error": e});
                }
                temp = Some(t.clone());
                t
            } else if !j["image"].is_null() {
                let spec: imgedit::Spec = serde_json::from_value(j["image"].clone()).unwrap_or_default();
                let t = grove_core::previews::temp_file("png");
                if let Err(e) = imgedit::export(&path, &spec, &t, "png", 100) {
                    return json!({"error": e});
                }
                temp = Some(t.clone());
                t
            } else {
                path.clone()
            };
            let opts: Vec<(String, String)> = j["options"].as_object().map(|o| o.iter().map(|(k, v)| (k.clone(), v.as_str().map(String::from).unwrap_or_else(|| v.to_string()))).collect()).unwrap_or_default();
            let r = pdfedit::print_file(&printer, &file, &title, &opts);
            // CUPS has its own copy once the job is in.
            if let Some(t) = temp {
                let _ = std::fs::remove_file(t);
            }
            match r {
                Ok(id) => json!({"job": id}),
                Err(e) => json!({"error": e}),
            }
        });
    }

    pub fn pv_copy_region(&self, key: &QString, what: &QString) {
        let w = what.to_string();
        let qt = self.qt_thread();
        self.answer_on("pv", key, move || {
            let v: Value = serde_json::from_str(&w).unwrap_or_default();
            let rect: [f32; 4] = serde_json::from_value(v["rect"].clone()).unwrap_or([0.0; 4]);
            let png: Result<Vec<u8>, String> = if v["kind"] == "pdf" {
                pdfedit::region_png(v["path"].as_str().unwrap_or(""), v["password"].as_str().unwrap_or(""), v["page"].as_u64().unwrap_or(0) as u32, v["pageW"].as_f64().unwrap_or(612.0) as f32, rect, 3.0)
            } else {
                let mut spec: imgedit::Spec = serde_json::from_value(v["spec"].clone()).unwrap_or_default();
                spec.crop = Some([rect[0].min(rect[2]), rect[1].min(rect[3]), (rect[2] - rect[0]).abs(), (rect[3] - rect[1]).abs()]);
                spec.rotate = 0;
                spec.flip_h = false;
                spec.flip_v = false;
                spec.resize = None;
                imgedit::load(Path::new(v["path"].as_str().unwrap_or(""))).and_then(|img| {
                    let out = imgedit::pipeline(img, &spec);
                    let mut buf = std::io::Cursor::new(Vec::new());
                    image::DynamicImage::ImageRgba8(out).write_to(&mut buf, image::ImageFormat::Png).map_err(|e| e.to_string())?;
                    Ok(buf.into_inner())
                })
            };
            match png {
                Ok(bytes) => {
                    let _ = qt.queue(move |_g| ffi::grove_set_clipboard_png(&bytes));
                    json!({"ok": true})
                }
                Err(e) => json!({"error": e}),
            }
        });
    }

    pub fn pv_signatures(&self, key: &QString) {
        self.answer(key, || std::fs::read_to_string(signatures_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| json!([])));
    }

    pub fn pv_set_signatures(&self, key: &QString, list: &QString) {
        let l = list.to_string();
        self.answer(key, move || {
            let f = signatures_file();
            if let Some(d) = f.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            match std::fs::write(&f, l) {
                Ok(()) => json!({"ok": true}),
                Err(e) => json!({"error": e.to_string()}),
            }
        });
    }
}
