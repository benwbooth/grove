//! grove as the desktop's file chooser: org.freedesktop.impl.portal.FileChooser
//! on org.freedesktop.impl.portal.desktop.grove (run as `grove --portal`;
//! xdg-desktop-portal calls it when its config names "grove"). Each
//! request opens grove's Finder-style chooser window and answers when the
//! user picks or cancels.

use crate::bridge::ffi::{self, QString};
use cxx_qt::CxxQtThread;
use futures_channel::oneshot;
use parking_lot::Mutex;
use serde_json::json;
use std::collections::HashMap;
use std::sync::OnceLock;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

/// (response, uris, choices, the file type filter it ended on)
type Answer = (u32, Vec<String>, Vec<(String, String)>, String);

fn waiting() -> &'static Mutex<HashMap<String, oneshot::Sender<Answer>>> {
    static W: OnceLock<Mutex<HashMap<String, oneshot::Sender<Answer>>>> = OnceLock::new();
    W.get_or_init(Default::default)
}

/// The chooser window finished (from QML). `extra_json`: {"choices": {id:
/// value}, "filter": name} (or, as before, just the choices map).
pub fn answer(id: &str, ok: bool, uris_json: &str, extra_json: &str) {
    let uris: Vec<String> = serde_json::from_str(uris_json).unwrap_or_default();
    let extra: serde_json::Value = serde_json::from_str(extra_json).unwrap_or_default();
    let (choices, filter) = match extra.get("choices") {
        Some(c) => (c.clone(), extra.get("filter").and_then(|f| f.as_str()).unwrap_or("").to_string()),
        None => (extra.clone(), String::new()),
    };
    let choices: Vec<(String, String)> = choices
        .as_object()
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))).collect())
        .unwrap_or_default();
    if let Some(tx) = waiting().lock().remove(id) {
        let _ = tx.send((if ok { 0 } else { 1 }, uris, choices, filter));
    }
}

// The chooser has no Qt handle of its own: the D-Bus name is claimed first
// thing in main (claim_early), before Qt starts, and Qt's thread arrives here
// once the interface has loaded (serve). Requests in between wait for it.
struct Chooser;

static QT: OnceLock<CxxQtThread<ffi::Grove>> = OnceLock::new();
static QT_WAITERS: Mutex<Vec<oneshot::Sender<()>>> = parking_lot::const_mutex(Vec::new());

async fn qt_ready() -> &'static CxxQtThread<ffi::Grove> {
    loop {
        if let Some(qt) = QT.get() {
            return qt;
        }
        let (tx, rx) = oneshot::channel();
        {
            let mut w = QT_WAITERS.lock();
            // Set between the check and taking the lock: don't wait.
            if QT.get().is_some() {
                continue;
            }
            w.push(tx);
        }
        let _ = rx.await;
    }
}

fn opt_str(o: &HashMap<String, OwnedValue>, k: &str) -> String {
    o.get(k).and_then(|v| v.try_clone().ok()).and_then(|v| String::try_from(v).ok()).unwrap_or_default()
}
/// A label without its mnemonic marker: apps send GTK's "_Open" ("_" before
/// the access key, "__" for a real underscore), which showed as "_Open".
fn without_mnemonic(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut it = label.chars().peekable();
    while let Some(c) = it.next() {
        if c == '_' {
            if it.peek() == Some(&'_') {
                it.next();
                out.push('_');
            }
            continue;
        }
        out.push(c);
    }
    out
}
fn opt_bool(o: &HashMap<String, OwnedValue>, k: &str) -> bool {
    o.get(k).and_then(|v| v.try_clone().ok()).and_then(|v| bool::try_from(v).ok()).unwrap_or(false)
}
fn opt_bytes(o: &HashMap<String, OwnedValue>, k: &str) -> String {
    let b: Vec<u8> = o.get(k).and_then(|v| v.try_clone().ok()).and_then(|v| Vec::<u8>::try_from(v).ok()).unwrap_or_default();
    String::from_utf8_lossy(&b).trim_end_matches('\0').to_string()
}

type Filter = (String, Vec<(u32, String)>);

/// a(sa(us)) from a variant, walked by hand (robust to how it was packed).
fn parse_filters(v: &Value) -> Vec<Filter> {
    let v = match v {
        Value::Value(inner) => inner.as_ref(),
        x => x,
    };
    let Value::Array(arr) = v else { return vec![] };
    let mut out = vec![];
    for item in arr.iter() {
        let Value::Structure(st) = item else { continue };
        let f = st.fields();
        let name = match f.first() {
            Some(Value::Str(s)) => s.to_string(),
            _ => continue,
        };
        let mut pats = vec![];
        if let Some(Value::Array(pa)) = f.get(1) {
            for p in pa.iter() {
                if let Value::Structure(ps) = p {
                    let pf = ps.fields();
                    if let (Some(Value::U32(k)), Some(Value::Str(pat))) = (pf.first(), pf.get(1)) {
                        pats.push((*k, pat.to_string()));
                    }
                }
            }
        }
        out.push((name, pats));
    }
    out
}

fn filters(o: &HashMap<String, OwnedValue>) -> Vec<serde_json::Value> {
    let f: Vec<Filter> = o.get("filters").map(|v| parse_filters(v)).unwrap_or_default();
    f.into_iter()
        .map(|(name, pats)| json!({"name": name, "patterns": pats.into_iter().map(|(k, p)| json!({"mime": k == 1, "pattern": p})).collect::<Vec<_>>()}))
        .collect()
}

/// The app's extra controls, a(ssa(ss)s): (id, label, options (id, label),
/// default). No options: a checkbox ("true"/"false"); options: a drop-down.
fn parse_choices(o: &HashMap<String, OwnedValue>) -> Vec<serde_json::Value> {
    let Some(v) = o.get("choices") else { return vec![] };
    let v: &Value = v;
    let v = match v {
        Value::Value(inner) => inner.as_ref(),
        x => x,
    };
    let Value::Array(arr) = v else { return vec![] };
    let s = |x: Option<&Value>| match x {
        Some(Value::Str(s)) => s.to_string(),
        _ => String::new(),
    };
    let mut out = vec![];
    for item in arr.iter() {
        let Value::Structure(st) = item else { continue };
        let f = st.fields();
        let mut opts = vec![];
        if let Some(Value::Array(oa)) = f.get(2) {
            for o in oa.iter() {
                if let Value::Structure(os) = o {
                    let of = os.fields();
                    opts.push(json!({"id": s(of.first()), "label": without_mnemonic(&s(of.get(1)))}));
                }
            }
        }
        out.push(json!({"id": s(f.first()), "label": without_mnemonic(&s(f.get(1))), "options": opts, "default": s(f.get(3))}));
    }
    out
}

fn current_filter(o: &HashMap<String, OwnedValue>) -> String {
    let Some(v) = o.get("current_filter") else { return String::new() };
    let v: &Value = v;
    let arr = Value::Array(zbus::zvariant::Array::from(vec![v.try_clone().unwrap_or(Value::from(""))]));
    parse_filters(&arr).into_iter().next().map(|x| x.0).unwrap_or_default()
}

impl Chooser {
    async fn ask(&self, mode: &str, handle: ObjectPath<'_>, app_id: &str, title: &str, options: HashMap<String, OwnedValue>) -> (u32, HashMap<String, OwnedValue>) {
        let id = handle.to_string();
        let (tx, rx) = oneshot::channel();
        waiting().lock().insert(id.clone(), tx);
        let req = json!({
            "id": id, "mode": mode, "app": app_id, "title": title,
            "acceptLabel": without_mnemonic(&opt_str(&options, "accept_label")),
            "multiple": opt_bool(&options, "multiple"),
            "directory": opt_bool(&options, "directory"),
            "currentName": opt_str(&options, "current_name"),
            "currentFolder": opt_bytes(&options, "current_folder"),
            "currentFile": opt_bytes(&options, "current_file"),
            "filters": filters(&options),
            "currentFilter": current_filter(&options),
            "choices": parse_choices(&options),
        })
        .to_string();
        let _ = qt_ready().await.queue(move |g| g.chooser_request(QString::from(req.as_str())));
        let (code, uris, choices, filter) = rx.await.unwrap_or((2, vec![], vec![], String::new()));
        let mut out: HashMap<String, OwnedValue> = HashMap::new();
        if code == 0 {
            if let Ok(v) = Value::from(uris).try_into() {
                out.insert("uris".into(), v);
            }
            if !choices.is_empty() {
                if let Ok(v) = Value::from(choices).try_into() {
                    out.insert("choices".into(), v);
                }
            }
            // The file type it ended on, as the app gave it: apps such as
            // LibreOffice save in the format of the chosen filter.
            let all: Vec<Filter> = options.get("filters").map(|v| parse_filters(v)).unwrap_or_default();
            if let Some((name, pats)) = all.into_iter().find(|f| !filter.is_empty() && f.0 == filter) {
                if let Ok(v) = Value::from((name, pats)).try_into() {
                    out.insert("current_filter".into(), v);
                }
            }
        }
        (code, out)
    }
}

#[zbus::interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl Chooser {
    async fn open_file(&self, handle: ObjectPath<'_>, app_id: &str, _parent_window: &str, title: &str, options: HashMap<String, OwnedValue>) -> (u32, HashMap<String, OwnedValue>) {
        self.ask("open", handle, app_id, title, options).await
    }
    async fn save_file(&self, handle: ObjectPath<'_>, app_id: &str, _parent_window: &str, title: &str, options: HashMap<String, OwnedValue>) -> (u32, HashMap<String, OwnedValue>) {
        self.ask("save", handle, app_id, title, options).await
    }
    async fn save_files(&self, handle: ObjectPath<'_>, app_id: &str, _parent_window: &str, title: &str, options: HashMap<String, OwnedValue>) -> (u32, HashMap<String, OwnedValue>) {
        self.ask("savefiles", handle, app_id, title, options).await
    }
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        4
    }
}

static CONN: OnceLock<zbus::blocking::Connection> = OnceLock::new();

/// Claims org.freedesktop.impl.portal.desktop.grove right away, before Qt
/// starts (main, in portal mode). xdg-desktop-portal starts `grove --portal`
/// and waits for this name; claimed only after Qt and the QML had loaded,
/// anything Qt asked the portal while starting (its theme, its file dialog
/// helper) waited on the portal waiting on grove, until D-Bus's 25 s
/// timeout: a grey desktop for 25 s at login and apps (the Claude app) that
/// missed dark mode (2026-11-25, twice).
pub fn claim_early() {
    if CONN.get().is_some() {
        return;
    }
    let r = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.serve_at("/org/freedesktop/portal/desktop", Chooser))
        .and_then(|b| b.name("org.freedesktop.impl.portal.desktop.grove"))
        .and_then(|b| b.build());
    match r {
        Ok(c) => {
            let _ = CONN.set(c);
        }
        Err(e) => eprintln!("grove --portal: can't serve the file chooser: {e}"),
    }
}

/// Qt is up: requests that arrived meanwhile (and every later one) go to it.
pub fn serve(qt: CxxQtThread<ffi::Grove>) {
    let _ = QT.set(qt);
    for tx in QT_WAITERS.lock().drain(..) {
        let _ = tx.send(());
    }
    claim_early();
}
