//! Remote places without KIO: small standard FUSE tools mount them under
//! $XDG_RUNTIME_DIR/grove/<name>, and they then show up like any disk.
//!   SFTP  -> sshfs
//!   SMB   -> rclone (its smb backend; no root needed)
//!   WebDAV / cloud (Google Drive... from `rclone config`) -> rclone mount
//!   Phones (MTP) -> simple-mtpfs (or jmtpfs)
//! Saved servers live in hearth's accounts service (Settings > Accounts,
//! see accounts.rs) when it's there: SFTP/SMB/WebDAV servers and their
//! passwords. Without it, and for rclone drives and phones, in
//! ~/.config/grove/servers.json with passwords in the keyring (Secret Service).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct Server {
    pub name: String,
    /// sftp, smb, webdav, rclone, mtp
    pub kind: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub user: String,
    /// Folder on the server (SFTP/WebDAV) or share/folder (SMB).
    #[serde(default)]
    pub path: String,
    /// rclone remote name (for kind "rclone").
    #[serde(default)]
    pub remote: String,
    /// MTP device number (1 = first phone).
    #[serde(default)]
    pub device: String,
}

fn servers_path() -> PathBuf {
    grove_core::util::grove_config().join("servers.json")
}

pub fn load() -> Vec<Server> {
    std::fs::read_to_string(servers_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save(v: &[Server]) {
    let _ = grove_core::util::write_atomic(&servers_path(), serde_json::to_string_pretty(v).unwrap().as_bytes());
}

pub fn mount_point(name: &str) -> PathBuf {
    let rt = std::env::var("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    let safe: String = name.chars().map(|c| if c == '/' { '_' } else { c }).collect();
    rt.join("grove").join(safe)
}

fn is_mounted(p: &std::path::Path) -> bool {
    let s = p.to_string_lossy();
    grove_core::mounts::mounts().iter().any(|m| m.mount_point == s)
}

pub fn saved() -> Value {
    let icon = |kind: &str| match kind {
        "mtp" => "phone|smartphone",
        "rclone" | "webdav" => "folder-cloud|network-server",
        _ => "network-server|folder-network",
    };
    let service = crate::accounts::servers();
    let mut v: Vec<Value> = load()
        .into_iter()
        // With the accounts service, its servers replace grove's own
        // SFTP/SMB/WebDAV ones (rclone drives and phones stay grove's).
        .filter(|s| service.is_none() || !crate::accounts::is_service_protocol(&s.kind))
        .map(|s| {
            let mp = mount_point(&s.name);
            json!({"name": s.name, "kind": s.kind, "host": s.host, "user": s.user, "path": s.path, "port": s.port, "icon": icon(&s.kind), "mounted": is_mounted(&mp), "mount": mp})
        })
        .collect();
    for mut a in service.unwrap_or_default() {
        let mp = mount_point(a["name"].as_str().unwrap_or(""));
        a["icon"] = json!(icon(a["kind"].as_str().unwrap_or("")));
        a["mounted"] = json!(is_mounted(&mp));
        a["mount"] = json!(mp);
        v.push(a);
    }
    json!(v)
}

fn which(c: &str) -> bool {
    grove_core::thumbs::which(c).is_some()
}

fn rclone_obscure(pw: &str) -> Result<String, String> {
    let o = Command::new("rclone").args(["obscure", "-"]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().and_then(|mut ch| {
        ch.stdin.take().unwrap().write_all(pw.as_bytes())?;
        ch.wait_with_output()
    });
    match o {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).trim().to_string()),
        _ => Err("rclone couldn't prepare the password.".into()),
    }
}

/// Connect (and optionally remember) a server. JSON: Server fields plus
/// "password", "remember" (bool), "savePassword" (bool).
pub fn connect(j: &str) -> Value {
    let v: Value = serde_json::from_str(j).unwrap_or(json!({}));
    let mut s: Server = serde_json::from_value(v.clone()).unwrap_or_default();
    if s.name.is_empty() {
        s.name = match s.kind.as_str() {
            "rclone" => s.remote.trim_end_matches(':').to_string(),
            "mtp" => "Phone".into(),
            _ => {
                if s.user.is_empty() {
                    s.host.clone()
                } else {
                    format!("{}@{}", s.user, s.host)
                }
            }
        };
    }
    // With hearth's accounts service: a saved server's password comes from
    // it, and remembering a server saves it there.
    let service = crate::accounts::available() && crate::accounts::is_service_protocol(&s.kind);
    let account = v["account"].as_str().map(str::to_string).filter(|a| !a.is_empty()).or_else(|| {
        if service { crate::accounts::server_by_name(&s.name).and_then(|a| a["id"].as_str().map(str::to_string)) } else { None }
    });
    let typed = v["password"].as_str().unwrap_or("").to_string();
    let mut password = typed.clone();
    if password.is_empty() {
        password = match (&account, service) {
            (Some(id), true) => crate::accounts::password(id).unwrap_or_default(),
            _ => crate::secrets::lookup(&s.name).unwrap_or_default(),
        };
    }
    let mp = mount_point(&s.name);
    if is_mounted(&mp) {
        return json!({"ok": true, "path": mp});
    }
    if let Err(e) = std::fs::create_dir_all(&mp) {
        return json!({"error": format!("Couldn't make a place to mount it: {e}")});
    }
    let r = match s.kind.as_str() {
        "sftp" => mount_sftp(&s, &password, &mp),
        "smb" => mount_rclone_backend(&s, &password, &mp, "smb"),
        "webdav" => mount_rclone_backend(&s, &password, &mp, "webdav"),
        "rclone" => mount_rclone_named(&s, &mp),
        "mtp" => mount_mtp(&s, &mp),
        k => Err(format!("Unknown kind of server “{k}”.")),
    };
    match r {
        Ok(()) => {
            let from_sidebar = v["account"].as_str().is_some_and(|a| !a.is_empty()) && typed.is_empty();
            if v["remember"].as_bool().unwrap_or(true) && service {
                if from_sidebar {
                    // Already saved there, nothing new to keep.
                    return json!({"ok": true, "path": mp});
                }
                let pw = v["savePassword"].as_bool().unwrap_or(false).then_some(typed.as_str());
                let mut rec = crate::accounts::server_record(&s, pw);
                if let Some(id) = &account {
                    rec["id"] = json!(id);
                }
                if let Err(e) = crate::accounts::save(&rec) {
                    crate::controller::notify_any("info", &format!("Couldn't save the server in Settings › Accounts: {}", e.message()));
                }
            } else if v["remember"].as_bool().unwrap_or(true) {
                let mut all = load();
                all.retain(|x| x.name != s.name);
                all.push(s.clone());
                save(&all);
                if v["savePassword"].as_bool().unwrap_or(false) && !password.is_empty() {
                    if let Err(e) = crate::secrets::store(&s.name, &password) {
                        crate::controller::notify_any("info", &e);
                    }
                }
            }
            json!({"ok": true, "path": mp})
        }
        Err(e) => {
            let _ = std::fs::remove_dir(&mp);
            json!({"error": e})
        }
    }
}

fn wait_mounted(mp: &std::path::Path) -> bool {
    for _ in 0..60 {
        if is_mounted(mp) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}

fn mount_sftp(s: &Server, pw: &str, mp: &std::path::Path) -> Result<(), String> {
    if !which("sshfs") {
        return Err("Connecting with SFTP needs sshfs, which isn't installed.".into());
    }
    let target = format!("{}{}:{}", if s.user.is_empty() { String::new() } else { format!("{}@", s.user) }, s.host, s.path);
    let mut c = Command::new("sshfs");
    c.arg(&target).arg(mp).args(["-o", "reconnect,ServerAliveInterval=15,ServerAliveCountMax=3,idmap=user,StrictHostKeyChecking=accept-new"]);
    if s.port != 0 {
        c.args(["-p", &s.port.to_string()]);
    }
    if !pw.is_empty() {
        c.args(["-o", "password_stdin"]);
    } else {
        c.args(["-o", "BatchMode=yes"]);
    }
    c.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
    let mut ch = c.spawn().map_err(|e| e.to_string())?;
    if !pw.is_empty() {
        let _ = ch.stdin.take().unwrap().write_all(format!("{pw}\n").as_bytes());
    } else {
        drop(ch.stdin.take());
    }
    let o = ch.wait_with_output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        let err = String::from_utf8_lossy(&o.stderr);
        return Err(if err.contains("Permission denied") {
            "The server didn't accept the user name or password.".into()
        } else if err.contains("Could not resolve") || err.contains("Name or service not known") {
            format!("The server “{}” can't be found. Check the name.", s.host)
        } else if err.contains("Connection refused") {
            format!("“{}” refused the connection (is SSH running there?).", s.host)
        } else {
            format!("Couldn't connect: {}", err.trim())
        });
    }
    Ok(())
}

fn rclone_daemon(args: Vec<String>, mp: &std::path::Path) -> Result<(), String> {
    if !which("rclone") {
        return Err("This kind of server needs rclone, which isn't installed.".into());
    }
    let o = Command::new("rclone").args(&args).arg(mp).args(["--daemon", "--vfs-cache-mode", "writes", "--daemon-wait", "8s"]).output().map_err(|e| e.to_string())?;
    if !o.status.success() || !wait_mounted(mp) {
        return Err(format!("Couldn't connect: {}", String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("rclone failed").trim()));
    }
    Ok(())
}

fn mount_rclone_backend(s: &Server, pw: &str, mp: &std::path::Path, backend: &str) -> Result<(), String> {
    let mut opts = vec![];
    match backend {
        "smb" => {
            opts.push(format!("host={}", s.host));
            if !s.user.is_empty() {
                opts.push(format!("user={}", s.user));
            }
            if s.port != 0 {
                opts.push(format!("port={}", s.port));
            }
        }
        _ => {
            let url = if s.host.starts_with("http") { s.host.clone() } else { format!("https://{}", s.host) };
            opts.push(format!("url={url}"));
            if !s.user.is_empty() {
                opts.push(format!("user={}", s.user));
            }
        }
    }
    if !pw.is_empty() {
        opts.push(format!("pass={}", rclone_obscure(pw)?));
    }
    let quoted: Vec<String> = opts.iter().map(|o| match o.split_once('=') {
        Some((k, v)) => format!("{k}=\"{}\"", v.replace('"', "\"\"")),
        None => o.clone(),
    }).collect();
    let remote = format!(":{backend},{}:{}", quoted.join(","), s.path.trim_start_matches('/'));
    rclone_daemon(vec!["mount".into(), remote], mp)
}

fn mount_rclone_named(s: &Server, mp: &std::path::Path) -> Result<(), String> {
    let r = if s.remote.ends_with(':') { s.remote.clone() } else { format!("{}:", s.remote) };
    rclone_daemon(vec!["mount".into(), format!("{r}{}", s.path)], mp)
}

fn mount_mtp(s: &Server, mp: &std::path::Path) -> Result<(), String> {
    // A device's name (from the sidebar): its number in simple-mtpfs's list.
    let dev = if s.device.is_empty() {
        "1".to_string()
    } else if s.device.parse::<u32>().is_ok() {
        s.device.clone()
    } else if which("simple-mtpfs") {
        let list = Command::new("simple-mtpfs").arg("-l").output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        crate::mtp::device_number(&s.device, &list).ok_or("The device isn't there any more (unplugged?).")?
    } else {
        "1".to_string()
    };
    let o = if which("simple-mtpfs") {
        Command::new("simple-mtpfs").args(["--device", &dev]).arg(mp).output()
    } else if which("jmtpfs") {
        Command::new("jmtpfs").arg(mp).output()
    } else {
        return Err("Phones need simple-mtpfs or jmtpfs, and neither is installed.".into());
    }
    .map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err("Couldn't open the phone. Unlock it and choose “File transfer” on its USB notification, then try again.".into());
    }
    Ok(())
}

pub fn forget(name: &str) -> Value {
    // A server from hearth's accounts service: forget it there. grove's own
    // copy (servers.json, keyring) stays untouched, for going back.
    if crate::accounts::available() {
        if let Some(a) = crate::accounts::server_by_name(name) {
            return match crate::accounts::remove(a["id"].as_str().unwrap_or("")) {
                Ok(()) => json!({"ok": true}),
                Err(e) => json!({"error": e.message()}),
            };
        }
    }
    let mut all = load();
    all.retain(|x| x.name != name);
    save(&all);
    crate::secrets::delete(name);
    json!({"ok": true})
}

/// rclone remotes from `rclone config` (Google Drive, Dropbox, ...).
fn rclone_remotes() -> Vec<String> {
    if !which("rclone") {
        return vec![];
    }
    Command::new("rclone").arg("listremotes").output().map(|o| String::from_utf8_lossy(&o.stdout).lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()).unwrap_or_default()
}

/// Servers on the local network (mDNS, in-process: mdns-sd, no
/// avahi-browse) and rclone remotes.
pub fn discover() -> Value {
    let mut hosts: Vec<Value> = vec![];
    if let Ok(d) = mdns_sd::ServiceDaemon::new() {
        let types = [("_sftp-ssh._tcp.local.", "sftp"), ("_ssh._tcp.local.", "sftp"), ("_smb._tcp.local.", "smb"), ("_webdav._tcp.local.", "webdav"), ("_webdavs._tcp.local.", "webdav")];
        let rxs: Vec<_> = types.iter().filter_map(|(t, k)| d.browse(t).ok().map(|rx| (rx, *k))).collect();
        // Answers come within a second or two; listen for 3.
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < until {
            for (rx, kind) in &rxs {
                while let Ok(ev) = rx.try_recv() {
                    if let mdns_sd::ServiceEvent::ServiceResolved(info) = ev {
                        let Some(addr) = info.get_addresses_v4().into_iter().next() else { continue };
                        let full = info.get_fullname();
                        let name = mdns_sd::instance_name_unescaped(full);
                        let host = info.get_hostname().trim_end_matches('.').to_string();
                        let v = json!({"name": name, "kind": kind, "host": host, "address": addr.to_string(), "port": info.get_port()});
                        if !hosts.iter().any(|h| h["host"] == v["host"] && h["kind"] == v["kind"]) {
                            hosts.push(v);
                        }
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let _ = d.shutdown();
    }
    json!({"hosts": hosts, "remotes": rclone_remotes(), "avahi": true})
}

// ---------------------------------------------------------------- rclone setup
//
// The Add Cloud Drive wizard drives rclone's own question flow
// (`rclone config create NAME TYPE --all --non-interactive`, then
// `config update NAME --continue --state S --result R`): every backend's
// options and its sign-in steps (OAuth opens the browser) come from rclone
// itself, so any kind of drive rclone knows can be set up.

/// Backends people pick most, shown first (rclone names).
const POPULAR: &[&str] = &[
    "drive", "onedrive", "dropbox", "iclouddrive", "protondrive", "box", "pcloud", "mega", "s3", "b2", "sftp", "webdav", "smb", "ftp",
    "gphotos", "koofr", "yandex", "seafile", "jottacloud", "internetarchive",
];

/// Every kind of drive rclone can set up: [{name, description, popular}].
pub fn rclone_providers() -> Value {
    if !which("rclone") {
        return json!({"error": "Cloud drives need rclone, which isn't installed."});
    }
    let o = match Command::new("rclone").args(["config", "providers"]).output() {
        Ok(o) if o.status.success() => o,
        _ => return json!({"error": "rclone couldn't list the kinds of drives."}),
    };
    let all: Vec<Value> = serde_json::from_slice(&o.stdout).unwrap_or_default();
    let mut v: Vec<Value> = all
        .iter()
        .filter(|p| p["Hide"].as_bool() != Some(true))
        .map(|p| {
            let name = p["Name"].as_str().unwrap_or("").to_string();
            let rank = POPULAR.iter().position(|x| *x == name).unwrap_or(999);
            json!({"name": name, "description": p["Description"].as_str().unwrap_or(""), "popular": rank < 999, "rank": rank})
        })
        .collect();
    v.sort_by(|a, b| a["rank"].as_u64().cmp(&b["rank"].as_u64()).then(a["description"].as_str().unwrap_or("").to_lowercase().cmp(&b["description"].as_str().unwrap_or("").to_lowercase())));
    json!({"providers": v})
}

/// Drives already set up: [{name, type}] (secrets left out).
pub fn rclone_drives() -> Value {
    let o = Command::new("rclone").args(["config", "dump"]).output();
    let dump: Value = o.ok().and_then(|o| serde_json::from_slice(&o.stdout).ok()).unwrap_or(json!({}));
    let mut v: Vec<Value> = dump.as_object().map(|m| m.iter().map(|(k, x)| json!({"name": k, "type": x["type"]})).collect()).unwrap_or_default();
    v.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    json!({"drives": v})
}

pub fn rclone_delete(name: &str) -> Value {
    match Command::new("rclone").args(["config", "delete", name]).output() {
        Ok(o) if o.status.success() => json!({"ok": true}),
        Ok(o) => json!({"error": String::from_utf8_lossy(&o.stderr).trim().to_string()}),
        Err(e) => json!({"error": e.to_string()}),
    }
}

static RCLONE_CHILD: std::sync::Mutex<Option<u32>> = std::sync::Mutex::new(None);

/// Stop a step that's waiting (a browser sign-in the user abandoned).
pub fn rclone_cancel() {
    if let Some(pid) = RCLONE_CHILD.lock().unwrap().take() {
        unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    }
}

fn rclone_json(args: &[String]) -> Result<Value, String> {
    let child = Command::new("rclone").args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    *RCLONE_CHILD.lock().unwrap() = Some(child.id());
    let o = child.wait_with_output().map_err(|e| e.to_string());
    *RCLONE_CHILD.lock().unwrap() = None;
    let o = o?;
    let out = String::from_utf8_lossy(&o.stdout);
    match serde_json::from_str::<Value>(out.trim()) {
        Ok(v) => Ok(v),
        Err(_) if o.status.success() => Ok(json!({"State": ""})),
        Err(_) => {
            let err = String::from_utf8_lossy(&o.stderr);
            let line = err.lines().filter(|l| !l.contains("NOTICE")).last().unwrap_or("rclone stopped").trim();
            Err(line.split(": ").skip(1).collect::<Vec<_>>().join(": ").trim().to_string().chars().take(400).collect::<String>().if_empty(line))
        }
    }
}

trait IfEmpty {
    fn if_empty(self, other: &str) -> String;
}
impl IfEmpty for String {
    fn if_empty(self, other: &str) -> String {
        if self.is_empty() { other.to_string() } else { self }
    }
}

/// One step of the setup. JSON in: {name, type, state, result, password
/// (the answer is a password: obscure it), defaults (answer every
/// optional question with its default until something needs the user)}.
/// Out: {state, option} for the next question, {done: true}, or {error}.
pub fn rclone_step(j: &str) -> Value {
    let v: Value = serde_json::from_str(j).unwrap_or(json!({}));
    let name = v["name"].as_str().unwrap_or("").trim().to_string();
    let typ = v["type"].as_str().unwrap_or("").to_string();
    let mut state = v["state"].as_str().unwrap_or("").to_string();
    let mut result = v["result"].as_str().unwrap_or("").to_string();
    let defaults = v["defaults"].as_bool().unwrap_or(false);
    if name.is_empty() || name.contains(':') || name.contains('/') {
        return json!({"error": "Give the drive a name without “:” or “/”."});
    }
    if v["password"].as_bool() == Some(true) && !result.is_empty() {
        match rclone_obscure(&result) {
            Ok(o) => result = o,
            Err(e) => return json!({"error": e}),
        }
    }
    let mut first = state.is_empty() && v["started"].as_bool() != Some(true);
    for _ in 0..400 {
        let args: Vec<String> = if first {
            if rclone_drives()["drives"].as_array().is_some_and(|d| d.iter().any(|x| x["name"] == name)) {
                return json!({"error": format!("There's already a drive called “{name}”. Pick another name.")});
            }
            vec!["config".into(), "create".into(), name.clone(), typ.clone(), "--all".into(), "--non-interactive".into()]
        } else {
            vec!["config".into(), "update".into(), name.clone(), "--continue".into(), "--state".into(), state.clone(), "--result".into(), result.clone(), "--all".into(), "--non-interactive".into()]
        };
        first = false;
        let r = match rclone_json(&args) {
            Ok(r) => r,
            Err(e) => return json!({"error": e}),
        };
        let next = r["State"].as_str().unwrap_or("").to_string();
        if next.is_empty() {
            return json!({"done": true, "name": name, "error": r["Error"]});
        }
        let opt = &r["Option"];
        // Skipping ahead: optional questions of the option list (not the
        // drive's own steps, like signing in) take their defaults.
        if defaults && next.starts_with("*all") && opt["Required"].as_bool() != Some(true) && r["Error"].as_str().unwrap_or("").is_empty() {
            state = next;
            result = opt["DefaultStr"].as_str().unwrap_or("").to_string();
            continue;
        }
        return json!({"state": next, "option": opt, "error": r["Error"], "advanced": next.starts_with("*all-set") && next.ends_with(",true")});
    }
    json!({"error": "rclone kept asking questions; stopped."})
}


// ---------------------------------------------------------------- Google Docs

/// rclone remotes of type "drive" (Google Drive), for exporting Google
/// Docs/Sheets/Slides that synced folders keep only as links.
pub fn google_drives() -> Vec<String> {
    if !which("rclone") {
        return vec![];
    }
    let o = Command::new("rclone").args(["config", "dump"]).output();
    let dump: Value = o.ok().and_then(|o| serde_json::from_slice(&o.stdout).ok()).unwrap_or(json!({}));
    dump.as_object().map(|m| m.iter().filter(|(_, x)| x["type"] == "drive").map(|(k, _)| k.clone()).collect()).unwrap_or_default()
}

/// A Google Docs/Sheets/Slides link file's document, exported by rclone
/// (by its file id: .docx, .xlsx, .pptx, .pdf), cached by the link file's
/// time. Tries each Google Drive remote until one has it.
///
/// The error is {error} plus, with hearth's accounts service, {service:
/// true, reason: signedout|notallowed|none, account} for the preview's
/// sign-in buttons.
pub fn gdoc_export(path: &std::path::Path, settings: &grove_core::settings::Settings) -> Result<std::path::PathBuf, Value> {
    let fail = |m: String| json!({"error": m});
    let link = grove_core::gdocs::link(path).ok_or_else(|| fail("This isn't a Google Docs link file.".into()))?;
    let kind = grove_core::gdocs::kind(&path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).ok_or_else(|| fail("Not a Google file.".into()))?;
    if kind.export.is_empty() {
        return Err(fail(format!("{} can't be exported for a preview; open it in the browser.", kind.app)));
    }
    if link.file_id.is_empty() {
        return Err(fail("The link file has no document id.".into()));
    }
    let mtime = std::fs::metadata(path).map(|m| std::os::unix::fs::MetadataExt::mtime(&m)).unwrap_or(0);
    let dir = grove_core::util::cache_home().join("grove/gdocs").join(format!("{}-{mtime}", link.file_id));
    if let Some(f) = std::fs::read_dir(&dir).ok().and_then(|rd| rd.flatten().map(|e| e.path()).find(|p| p.is_file())) {
        return Ok(f);
    }
    let _ = std::fs::create_dir_all(&dir);
    let service = crate::accounts::available();
    // grove's own Google sign-ins (after the accounts service's, when it's
    // there): the link's account, else any.
    let accounts: Vec<String> = {
        let mut a = settings.google_accounts.clone();
        a.sort_by_key(|x| x != &link.email);
        a
    };
    let mime = match kind.export {
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        _ => "application/pdf",
    };
    let stem: String = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into()).chars().filter(|c| *c != '/').collect();
    let mut last = String::new();
    let write = |bytes: Vec<u8>| -> Result<std::path::PathBuf, Value> {
        let out = dir.join(format!("{stem}.{}", kind.export));
        std::fs::write(&out, bytes).map_err(|e| fail(e.to_string()))?;
        Ok(out)
    };
    // hearth's accounts service: any Google account allowed to read files
    // (the link's account first). Its sign-in trouble decides the buttons.
    let mut signin: Option<Value> = None;
    if service {
        let gs = crate::accounts::google_file_accounts(&link.email);
        if gs.is_empty() {
            signin = Some(json!({"service": true, "reason": "none", "account": "",
                "error": "Sign in with Google in Settings › Accounts to see what's in Google documents here."}));
        }
        for g in gs {
            let id = g["id"].as_str().unwrap_or("").to_string();
            let email = g["email"].as_str().unwrap_or("").to_string();
            let r = crate::accounts::access_token(&id, "files").map_err(|e| (e.reason(), e.message().to_string())).and_then(|t| {
                crate::google::export_with(t["token"].as_str().unwrap_or(""), &email, &link.file_id, mime).map_err(|e| ("failed", e))
            });
            match r {
                Ok(bytes) => return write(bytes),
                Err((reason, msg)) => {
                    if reason != "failed" && signin.as_ref().is_none_or(|s| s["reason"] == "failed") {
                        signin = Some(json!({"service": true, "reason": reason, "account": id, "error": msg}));
                    } else if signin.is_none() {
                        signin = Some(json!({"service": true, "reason": "failed", "account": id, "error": msg}));
                    }
                    last = msg;
                }
            }
        }
    }
    for a in &accounts {
        match crate::google::export(settings, a, &link.file_id, mime) {
            Ok(bytes) => return write(bytes),
            Err(e) => last = e,
        }
    }
    let remotes = google_drives();
    if remotes.is_empty() {
        let _ = std::fs::remove_dir_all(&dir);
        if let Some(s) = signin {
            return Err(s);
        }
        return Err(fail(if last.is_empty() { "Sign in with Google to see what's in Google documents here.".into() } else { last }));
    }
    for r in remotes {
        let o = Command::new("rclone")
            .args(["backend", "copyid", &format!("{r}:"), &link.file_id, &format!("{}/", dir.display()), "--drive-export-formats", kind.export])
            .output()
            .map_err(|e| fail(e.to_string()))?;
        if let Some(f) = std::fs::read_dir(&dir).ok().and_then(|rd| rd.flatten().map(|e| e.path()).find(|p| p.is_file())) {
            return Ok(f);
        }
        last = String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("").to_string();
    }
    let _ = std::fs::remove_dir_all(&dir);
    if let Some(s) = signin.filter(|s| s["reason"] != "failed") {
        return Err(s);
    }
    Err(fail(if last.is_empty() { "Google Drive didn't send the document.".into() } else { format!("Google Drive didn't send it: {last}") }))
}
