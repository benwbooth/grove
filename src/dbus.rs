//! D-Bus: org.freedesktop.FileManager1 (browsers' "Show in folder", the
//! screenshot tool...) and grove's own name, so a second `grove path`
//! opens in the running instance.

use crate::bridge::ffi::{self, QString};
use cxx_qt::CxxQtThread;
use serde_json::json;
use std::sync::OnceLock;

pub const OWN_NAME: &str = "io.github.benwbooth.Grove";
const OWN_PATH: &str = "/io/github/benwbooth/Grove";

fn uris_to_paths(uris: &[String]) -> Vec<String> {
    uris.iter().filter_map(|u| grove_core::util::path_from_uri(u)).map(|p| p.to_string_lossy().into_owned()).collect()
}

struct FileManager1 {
    qt: CxxQtThread<ffi::Grove>,
}

impl FileManager1 {
    fn send(&self, action: &str, uris: Vec<String>) {
        let s = json!({"action": action, "paths": uris_to_paths(&uris)}).to_string();
        let _ = self.qt.queue(move |g| g.open_request(QString::from(s.as_str())));
    }
}

#[zbus::interface(name = "org.freedesktop.FileManager1")]
impl FileManager1 {
    fn show_folders(&self, uris: Vec<String>, _startup_id: &str) {
        self.send("folders", uris);
    }
    fn show_items(&self, uris: Vec<String>, _startup_id: &str) {
        self.send("items", uris);
    }
    fn show_item_properties(&self, uris: Vec<String>, _startup_id: &str) {
        self.send("properties", uris);
    }
}

struct Own {
    qt: CxxQtThread<ffi::Grove>,
}

#[zbus::interface(name = "io.github.benwbooth.Grove")]
impl Own {
    /// action: "folders" (open these folders), "items" (select these),
    /// "window" (a new empty window).
    fn open(&self, action: &str, paths: Vec<String>) {
        let s = json!({"action": action, "paths": paths}).to_string();
        let _ = self.qt.queue(move |g| g.open_request(QString::from(s.as_str())));
    }
}

static CONN: OnceLock<zbus::blocking::Connection> = OnceLock::new();

/// Serve both interfaces (once the controller exists).
pub fn serve(qt: CxxQtThread<ffi::Grove>) {
    if std::env::var_os("GROVE_NO_DBUS").is_some() || CONN.get().is_some() {
        return;
    }
    std::thread::spawn(move || {
        let build = || -> zbus::Result<zbus::blocking::Connection> {
            let c = zbus::blocking::connection::Builder::session()?
                .serve_at("/org/freedesktop/FileManager1", FileManager1 { qt: qt.clone() })?
                .serve_at(OWN_PATH, Own { qt: qt.clone() })?
                .build()?;
            c.request_name(OWN_NAME)?;
            // The file manager name: taken only if nobody else has it.
            if let Err(e) = c.request_name_with_flags("org.freedesktop.FileManager1", zbus::fdo::RequestNameFlags::DoNotQueue.into()) {
                eprintln!("grove: org.freedesktop.FileManager1 is taken ({e}); \"Show in folder\" goes to the other file manager");
            }
            Ok(c)
        };
        match build() {
            Ok(c) => {
                let _ = CONN.set(c);
            }
            Err(e) => eprintln!("grove: D-Bus unavailable: {e}"),
        }
    });
}

/// A running grove? Then hand it the request and return true.
pub fn forward(action: &str, paths: &[String]) -> bool {
    if std::env::var_os("GROVE_NO_DBUS").is_some() {
        return false;
    }
    let Ok(c) = zbus::blocking::Connection::session() else { return false };
    let Ok(dbus) = zbus::blocking::fdo::DBusProxy::new(&c) else { return false };
    let Ok(name) = zbus::names::BusName::try_from(OWN_NAME) else { return false };
    if !dbus.name_has_owner(name.clone()).unwrap_or(false) {
        return false;
    }
    c.call_method(Some(OWN_NAME), OWN_PATH, Some(OWN_NAME), "Open", &(action, paths.to_vec())).is_ok()
}
