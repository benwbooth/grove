//! PvCanvas: Grove Preview's drawing surface, a QQuickPaintedItem written
//! in Rust. It shows one PDF page (MuPDF, some annotations left out) or a
//! picture (with Instant Alpha and Adjust Color applied), drawn on a
//! worker thread at the size asked for and painted scaled to the item.
//!
//!   PvCanvas {
//!       request: JSON.stringify({kind: "pdf", path, page, password, hide: [1, 2], px: 1200})
//!       // or {kind: "image", path, px, alpha: [...], adjust: {...}}
//!   }
//!
//! `ready` turns true once something is drawn; `error` says why not.

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QImage, QRect, QString};
use grove_core::{imgedit, pdfedit};
use serde_json::Value;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qpainter.h");
        type QPainter = cxx_qt_lib::QPainter;
        include!("cxx-qt-lib/qsizef.h");
        type QSizeF = cxx_qt_lib::QSizeF;
        include!(<QtQuick/QQuickPaintedItem>);
        type QQuickPaintedItem;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QQuickPaintedItem]
        #[qproperty(QString, request, READ, WRITE = set_request, NOTIFY)]
        #[qproperty(bool, ready)]
        #[qproperty(QString, error)]
        #[qproperty(i32, image_width, cxx_name = "imageWidth")]
        #[qproperty(i32, image_height, cxx_name = "imageHeight")]
        type PvCanvas = super::PvCanvasRust;

        #[cxx_override]
        unsafe fn paint(self: Pin<&mut PvCanvas>, painter: *mut QPainter);

        #[cxx_name = "setRequest"]
        fn set_request(self: Pin<&mut PvCanvas>, request: QString);
    }

    unsafe extern "RustQt" {
        #[inherit]
        fn size(self: &PvCanvas) -> QSizeF;
        #[inherit]
        fn update(self: Pin<&mut PvCanvas>);
    }

    impl cxx_qt::Threading for PvCanvas {}
    // Made by QML with no arguments (QQuickPaintedItem's own constructor).
    impl cxx_qt::Initialize for PvCanvas {}
}

pub struct PvCanvasRust {
    request: QString,
    ready: bool,
    error: QString,
    image_width: i32,
    image_height: i32,
    image: Option<QImage>,
    generation: Arc<AtomicU64>,
}

impl Default for PvCanvasRust {
    fn default() -> Self {
        PvCanvasRust { request: QString::default(), ready: false, error: QString::default(), image_width: 0, image_height: 0, image: None, generation: Arc::new(AtomicU64::new(0)) }
    }
}

type Job = Box<dyn FnOnce() + Send>;

/// Work waiting for the drawing threads; the newest is drawn first (the
/// page just scrolled to).
static JOBS: Mutex<Vec<Job>> = Mutex::new(Vec::new());

/// Queue work for the three drawing threads (started on first use).
fn submit(j: Job) {
    static WAKE: OnceLock<Mutex<mpsc::Sender<()>>> = OnceLock::new();
    let wake = WAKE.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<()>();
        let rx = Arc::new(Mutex::new(rx));
        for i in 0..3 {
            let rx = rx.clone();
            std::thread::Builder::new()
                .name(format!("grove-pvdraw-{i}"))
                .spawn(move || loop {
                    if rx.lock().unwrap().recv().is_err() {
                        return;
                    }
                    let job = JOBS.lock().unwrap().pop();
                    if let Some(j) = job {
                        j();
                    }
                })
                .unwrap();
        }
        Mutex::new(tx)
    });
    JOBS.lock().unwrap().push(j);
    let _ = wake.lock().unwrap().send(());
}

fn draw(req: &Value) -> Result<image::RgbaImage, String> {
    let path = req["path"].as_str().unwrap_or("");
    let px = req["px"].as_u64().unwrap_or(800).clamp(16, 8000) as u32;
    if req["kind"] == "pdf" {
        let hide: Vec<i32> = req["hide"].as_array().map(|a| a.iter().filter_map(|v| v.as_i64()).map(|v| v as i32).collect()).unwrap_or_default();
        let (w, h, px) = pdfedit::render(path, req["password"].as_str().unwrap_or(""), req["page"].as_u64().unwrap_or(0) as u32, px, &hide)?;
        image::RgbaImage::from_raw(w, h, px).ok_or_else(|| "The page couldn't be drawn.".to_string())
    } else {
        let alpha: Vec<imgedit::AlphaMark> = serde_json::from_value(req["alpha"].clone()).unwrap_or_default();
        let adjust: imgedit::Adjust = serde_json::from_value(req["adjust"].clone()).unwrap_or_default();
        imgedit::preview(std::path::Path::new(path), px, &alpha, &adjust)
    }
}

impl cxx_qt::Initialize for qobject::PvCanvas {
    fn initialize(self: Pin<&mut Self>) {}
}

impl qobject::PvCanvas {
    pub fn set_request(mut self: Pin<&mut Self>, request: QString) {
        if request == self.rust().request {
            return;
        }
        self.as_mut().rust_mut().request = request.clone();
        self.as_mut().request_changed();
        let text = request.to_string();
        let generation = self.rust().generation.clone();
        let mine = generation.fetch_add(1, Ordering::SeqCst) + 1;
        if text.is_empty() {
            self.as_mut().rust_mut().image = None;
            self.as_mut().set_ready(false);
            self.as_mut().update();
            return;
        }
        let qt = self.qt_thread();
        submit(Box::new(move || {
            // Already asked for something else: skip.
            if generation.load(Ordering::SeqCst) != mine {
                return;
            }
            let req: Value = serde_json::from_str(&text).unwrap_or_default();
            let r = draw(&req);
            let _ = qt.queue(move |mut c: Pin<&mut qobject::PvCanvas>| {
                if c.rust().generation.load(Ordering::SeqCst) != mine {
                    return;
                }
                match r {
                    Ok(img) => {
                        let (w, h) = (img.width() as i32, img.height() as i32);
                        c.as_mut().rust_mut().image = Some(QImage::from(img));
                        c.as_mut().set_image_width(w);
                        c.as_mut().set_image_height(h);
                        c.as_mut().set_error(QString::default());
                        c.as_mut().set_ready(true);
                    }
                    Err(e) => {
                        c.as_mut().set_error(QString::from(&e));
                    }
                }
                c.as_mut().update();
            });
        }));
    }

    /// # Safety
    /// Qt calls this with a valid painter while painting the item.
    pub unsafe fn paint(self: Pin<&mut Self>, painter: *mut qobject::QPainter) {
        let Some(img) = self.rust().image.as_ref() else { return };
        let Some(p) = (unsafe { painter.as_mut() }) else { return };
        let mut p = unsafe { Pin::new_unchecked(p) };
        let s = self.size();
        p.as_mut().set_render_hint(cxx_qt_lib::QPainterRenderHint::SmoothPixmapTransform, true);
        p.as_mut().draw_image(&QRect::new(0, 0, s.width().round() as i32, s.height().round() as i32), img);
    }
}
