//! Remote places without KIO: small standard FUSE tools mount them under
//! $XDG_RUNTIME_DIR/grove/<name>, and they then show up like any disk.
//!   SFTP  -> sshfs
//!   SMB   -> rclone (its smb backend; no root needed)
//!   WebDAV / cloud (Google Drive... from `rclone config`) -> rclone mount
//!   Phones (MTP) -> simple-mtpfs (or jmtpfs)
//! Saved servers live in ~/.config/grove/servers.json; passwords in the
//! keyring (Secret Service).

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
    let v: Vec<Value> = load()
        .into_iter()
        .map(|s| {
            let mp = mount_point(&s.name);
            let icon = match s.kind.as_str() {
                "mtp" => "phone|smartphone",
                "rclone" | "webdav" => "folder-cloud|network-server",
                _ => "network-server|folder-network",
            };
            json!({"name": s.name, "kind": s.kind, "host": s.host, "user": s.user, "path": s.path, "icon": icon, "mounted": is_mounted(&mp), "mount": mp})
        })
        .collect();
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
    let mut password = v["password"].as_str().unwrap_or("").to_string();
    if password.is_empty() {
        password = crate::secrets::lookup(&s.name).unwrap_or_default();
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
            if v["remember"].as_bool().unwrap_or(true) {
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
    let dev = if s.device.is_empty() { "1".to_string() } else { s.device.clone() };
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

/// Servers on the local network (mDNS via avahi-browse) and rclone remotes.
pub fn discover() -> Value {
    let mut hosts = vec![];
    if which("avahi-browse") {
        let mut outs = vec![];
        for t in ["_sftp-ssh._tcp", "_ssh._tcp", "_smb._tcp", "_webdav._tcp", "_webdavs._tcp"] {
            if let Ok(o) = Command::new("timeout").args(["4", "avahi-browse", "-tpr", t]).output() {
                outs.push(String::from_utf8_lossy(&o.stdout).into_owned());
            }
        }
        for out in outs {
            for l in out.lines() {
                let f: Vec<&str> = l.split(';').collect();
                if f.len() >= 9 && f[0] == "=" && f[2] == "IPv4" {
                    let kind = match f[4] {
                        "_smb._tcp" => "smb",
                        "_webdav._tcp" | "_webdavs._tcp" => "webdav",
                        _ => "sftp",
                    };
                    let v = json!({"name": f[3].replace("\\032", " "), "kind": kind, "host": f[6], "address": f[7], "port": f[8].parse::<u16>().unwrap_or(0)});
                    if !hosts.iter().any(|h: &Value| h["host"] == v["host"] && h["kind"] == v["kind"]) {
                        hosts.push(v);
                    }
                }
            }
        }
    }
    json!({"hosts": hosts, "remotes": rclone_remotes(), "avahi": which("avahi-browse")})
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
