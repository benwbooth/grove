//! hearth's shared accounts (Settings > Accounts): servers and Google
//! accounts set up once for every app, through the session D-Bus service
//! io.github.benwbooth.Accounts (hearth/helpers/hearth-accounts).
//!
//! When the service is there (running, or D-Bus can start it), grove's
//! saved SFTP/SMB/WebDAV servers and its Google sign-ins come from it; when
//! it isn't, grove keeps its own (servers.json, Google sign-in in
//! settings.json) exactly as before. The first time the service shows up,
//! grove copies its own servers and Google accounts into it once (and
//! leaves its own copies alone, so going back still works).
//!
//! Every call here blocks on D-Bus: call them from worker threads only.
//! `available()` and `cached()` read what the watcher thread last saw.

use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::Duration;
use zbus::blocking::Connection;

pub const NAME: &str = "io.github.benwbooth.Accounts";
pub const PATH: &str = "/io/github/benwbooth/Accounts";
pub const IFACE: &str = "io.github.benwbooth.Accounts1";

/// Why the service couldn't give something.
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// The provider ended the sign-in: sign in again.
    SignedOut(String),
    /// The account isn't allowed this use (no grant with the scope).
    NotAllowed(String),
    Failed(String),
}

impl Error {
    pub fn message(&self) -> &str {
        match self {
            Error::SignedOut(m) | Error::NotAllowed(m) | Error::Failed(m) => m,
        }
    }
    pub fn reason(&self) -> &'static str {
        match self {
            Error::SignedOut(_) => "signedout",
            Error::NotAllowed(_) => "notallowed",
            Error::Failed(_) => "failed",
        }
    }
}

impl From<zbus::Error> for Error {
    fn from(e: zbus::Error) -> Self {
        match e {
            zbus::Error::MethodError(name, detail, _) => {
                let msg = detail.unwrap_or_else(|| name.to_string());
                match name.as_str().rsplit('.').next().unwrap_or("") {
                    "SignedOut" => Error::SignedOut(msg),
                    "NotAllowed" => Error::NotAllowed(msg),
                    _ => Error::Failed(msg),
                }
            }
            e => Error::Failed(format!("The accounts service didn't answer: {e}")),
        }
    }
}

#[derive(Default)]
struct State {
    available: bool,
    accounts: Vec<Value>,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(Default::default)
}

/// Sign-ins grove started (SignIn sessions), to report their failures.
fn pending() -> &'static Mutex<HashSet<String>> {
    static P: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    P.get_or_init(Default::default)
}

fn conn() -> Result<Connection, Error> {
    static C: OnceLock<Mutex<Option<Connection>>> = OnceLock::new();
    let mut c = C.get_or_init(Default::default).lock();
    if let Some(c) = c.as_ref() {
        return Ok(c.clone());
    }
    let n = Connection::session().map_err(|e| Error::Failed(format!("No session bus: {e}")))?;
    *c = Some(n.clone());
    Ok(n)
}

fn call<B>(method: &str, body: &B) -> Result<zbus::Message, Error>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    Ok(conn()?.call_method(Some(NAME), PATH, Some(IFACE), method, body)?)
}

fn call_str<B>(method: &str, body: &B) -> Result<String, Error>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    let m = call(method, body)?;
    m.body().deserialize::<String>().map_err(|e| Error::Failed(e.to_string()))
}

/// Whether the service is running or D-Bus can start it (blocking).
pub fn probe() -> bool {
    let Ok(c) = conn() else { return false };
    let owned = c
        .call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "NameHasOwner", &(NAME,))
        .ok()
        .and_then(|r| r.body().deserialize::<bool>().ok());
    if owned == Some(true) {
        return true;
    }
    c.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "ListActivatableNames", &())
        .ok()
        .and_then(|r| r.body().deserialize::<Vec<String>>().ok())
        .is_some_and(|v| v.iter().any(|n| n == NAME))
}

/// What the watcher last saw (doesn't block).
pub fn available() -> bool {
    state().lock().available
}

/// The accounts the watcher last listed (no secrets).
pub fn cached() -> Vec<Value> {
    state().lock().accounts.clone()
}

pub fn list() -> Result<Vec<Value>, Error> {
    let s = call_str("List", &())?;
    serde_json::from_str(&s).map_err(|e| Error::Failed(e.to_string()))
}

pub fn password(id: &str) -> Result<String, Error> {
    call_str("Password", &(id,))
}

/// {token, expires_in, email}
pub fn access_token(id: &str, purpose: &str) -> Result<Value, Error> {
    let s = call_str("AccessToken", &(id, purpose))?;
    serde_json::from_str(&s).map_err(|e| Error::Failed(e.to_string()))
}

pub fn save(rec: &Value) -> Result<String, Error> {
    call_str("Save", &(rec.to_string(),))
}

pub fn remove(id: &str) -> Result<(), Error> {
    call("Remove", &(id,)).map(|_| ())
}

pub fn registrations() -> Result<Value, Error> {
    let s = call_str("Registrations", &())?;
    serde_json::from_str(&s).map_err(|e| Error::Failed(e.to_string()))
}

pub fn save_registration(provider: &str, rec: &Value) -> Result<(), Error> {
    call("SaveRegistration", &(provider, rec.to_string())).map(|_| ())
}

/// Start the service's browser sign-in; its failure (if any) comes back as
/// a notification when SignInDone arrives, its success as Changed.
pub fn sign_in(provider: &str, id: &str) -> Result<String, Error> {
    let s = call_str("SignIn", &(provider, id))?;
    pending().lock().insert(s.clone());
    Ok(s)
}

/// Settings > Accounts (at `id`); false when hearth isn't there to show it.
pub fn open_settings(id: &str) -> bool {
    call("OpenSettings", &(id,)).ok().and_then(|m| m.body().deserialize::<bool>().ok()).unwrap_or(false)
}

// ---------------------------------------------------------------- servers

/// The service's SFTP/SMB/WebDAV servers as grove's sidebar lists saved
/// servers (None without the service).
pub fn servers() -> Option<Vec<Value>> {
    if !available() {
        return None;
    }
    Some(cached().into_iter().filter(|a| a["kind"] == "server").map(|a| server_entry(&a)).collect())
}

fn server_entry(a: &Value) -> Value {
    let name = a["name"].as_str().filter(|s| !s.is_empty()).or(a["host"].as_str()).unwrap_or("Server").to_string();
    json!({
        "name": name,
        "kind": a["protocol"].as_str().unwrap_or("sftp"),
        "host": a["host"],
        "user": a["user"],
        "path": a["path"],
        "port": a["port"].as_u64().or_else(|| a["port"].as_str().and_then(|p| p.parse().ok())).unwrap_or(0),
        "account": a["id"],
    })
}

/// The service's server account behind a sidebar name.
pub fn server_by_name(name: &str) -> Option<Value> {
    cached().into_iter().find(|a| a["kind"] == "server" && (server_entry(a)["name"] == name || a["app_ids"]["grove"] == name))
}

/// A server as the service keeps it.
pub fn server_record(s: &crate::remote::Server, password: Option<&str>) -> Value {
    let mut rec = json!({
        "kind": "server", "protocol": s.kind, "host": s.host, "user": s.user, "path": s.path, "name": s.name,
        "app_ids": {"grove": s.name},
    });
    if s.port != 0 {
        rec["port"] = json!(s.port);
    }
    if let Some(p) = password.filter(|p| !p.is_empty()) {
        rec["password"] = json!(p);
    }
    rec
}

pub fn is_service_protocol(kind: &str) -> bool {
    matches!(kind, "sftp" | "smb" | "webdav")
}

// ---------------------------------------------------------------- Google

/// Google accounts allowed to read files, the one named by `prefer` first.
pub fn google_file_accounts(prefer: &str) -> Vec<Value> {
    let mut v: Vec<Value> = cached().into_iter().filter(|a| a["kind"] == "google").collect();
    v.sort_by_key(|a| (a["uses"].as_array().is_none_or(|u| !u.iter().any(|x| x == "files")), a["email"].as_str().unwrap_or("") != prefer));
    v
}

// ---------------------------------------------------------------- migration

fn marker() -> std::path::PathBuf {
    grove_core::util::grove_config().join("accounts-migrated.json")
}

/// Copy grove's own servers and Google sign-ins into the service, once.
/// grove's own entries stay (servers.json, settings, keyring).
pub fn migrate() -> Result<Value, Error> {
    if marker().exists() {
        return Ok(json!({"skipped": true}));
    }
    let mut servers = 0;
    for s in crate::remote::load() {
        if !is_service_protocol(&s.kind) {
            continue; // rclone drives and phones stay grove's own
        }
        let pw = crate::secrets::lookup(&s.name);
        save(&server_record(&s, pw.as_deref()))?;
        servers += 1;
    }
    let settings = grove_core::settings::load();
    let (cid, secret) = (settings.google_client_id.trim().to_string(), settings.google_client_secret.trim().to_string());
    let mut google = 0;
    if !cid.is_empty() {
        let regs = registrations()?;
        if regs["google"]["ready"].as_bool() != Some(true) {
            save_registration("google", &json!({"client_id": cid, "client_secret": secret}))?;
        }
        for email in &settings.google_accounts {
            let mut grant = json!({"client_id": cid, "client_secret": secret, "scope": crate::google::SCOPES});
            if let Some(t) = crate::secrets::lookup(&crate::google::keyring_key(email)) {
                grant["refresh_token"] = json!(t);
            }
            save(&json!({"kind": "google", "email": email, "grants": [grant]}))?;
            google += 1;
        }
    }
    let done = json!({"servers": servers, "google": google, "when": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)});
    let _ = grove_core::util::write_atomic(&marker(), done.to_string().as_bytes());
    Ok(done)
}

// ---------------------------------------------------------------- watching

/// Re-list the accounts; true when something changed.
fn refresh() -> bool {
    match list() {
        Ok(v) => {
            let mut s = state().lock();
            let changed = !s.available || s.accounts != v;
            s.available = true;
            s.accounts = v;
            changed
        }
        Err(_) => {
            let ok = probe();
            let mut s = state().lock();
            let changed = s.available != ok;
            s.available = ok;
            changed
        }
    }
}

/// Watch the service: whether it's there (checked now and every minute),
/// and its Changed / SignInDone signals. `changed(available)` runs on a
/// worker thread when what grove sees changed (queue it to the Qt thread).
pub fn watch(changed: impl Fn(bool) + Send + Sync + 'static) {
    if std::env::var_os("GROVE_NO_ACCOUNTS").is_some() {
        return;
    }
    let changed = std::sync::Arc::new(changed);
    let c2 = changed.clone();
    let _ = std::thread::Builder::new().name("grove-accounts".into()).spawn(move || loop {
        let there = probe();
        let differs = if there {
            if let Err(e) = migrate() {
                eprintln!("grove: copying accounts to Settings > Accounts failed (will try again): {}", e.message());
            }
            refresh()
        } else {
            let mut s = state().lock();
            let d = s.available;
            s.available = false;
            s.accounts.clear();
            d
        };
        if differs {
            c2(there);
        }
        std::thread::sleep(Duration::from_secs(60));
    });
    let _ = std::thread::Builder::new().name("grove-accounts-signals".into()).spawn(move || {
        let Ok(c) = conn() else { return };
        let Ok(rule) = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).interface(IFACE).and_then(|b| b.path(PATH)).map(|b| b.build()) else { return };
        let Ok(it) = zbus::blocking::MessageIterator::for_match_rule(rule, &c, Some(64)) else { return };
        for m in it.flatten() {
            let h = m.header();
            match h.member().map(|m| m.as_str()) {
                Some("Changed") => {
                    if refresh() {
                        changed(true);
                    }
                }
                Some("SignInDone") => {
                    let Ok((session, result)) = m.body().deserialize::<(String, String)>() else { continue };
                    if !pending().lock().remove(&session) {
                        continue;
                    }
                    let r: Value = serde_json::from_str(&result).unwrap_or(json!({}));
                    if r["ok"].as_bool() != Some(true) && r["cancelled"].as_bool() != Some(true) {
                        crate::controller::notify_any("error", r["error"].as_str().unwrap_or("Signing in didn't work."));
                    }
                }
                _ => {}
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_by_name() {
        let e = |n: &str| match n.rsplit('.').next().unwrap() {
            "SignedOut" => Error::SignedOut("x".into()),
            "NotAllowed" => Error::NotAllowed("x".into()),
            _ => Error::Failed("x".into()),
        };
        assert_eq!(e("io.github.benwbooth.Accounts1.SignedOut").reason(), "signedout");
        assert_eq!(e("io.github.benwbooth.Accounts1.NotAllowed").reason(), "notallowed");
        assert_eq!(e("org.freedesktop.DBus.Error.ServiceUnknown").reason(), "failed");
    }

    #[test]
    fn server_entries() {
        let a = json!({"id": "a1", "kind": "server", "protocol": "smb", "host": "nas", "user": "ben", "path": "Public", "name": "NAS", "port": 445});
        let e = server_entry(&a);
        assert_eq!(e["name"], "NAS");
        assert_eq!(e["kind"], "smb");
        assert_eq!(e["port"], 445);
        assert_eq!(e["account"], "a1");
        let s = crate::remote::Server { name: "box".into(), kind: "sftp".into(), host: "h".into(), user: "u".into(), ..Default::default() };
        let r = server_record(&s, Some("pw"));
        assert_eq!(r["protocol"], "sftp");
        assert_eq!(r["app_ids"]["grove"], "box");
        assert_eq!(r["password"], "pw");
        assert!(r.get("port").is_none());
        assert!(server_record(&s, Some("")).get("password").is_none());
    }
}
