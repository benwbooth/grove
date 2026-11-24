//! Google Drive sync badges and actions from canopy (Ben's Google Drive
//! client, daemon `canopyd`), over its Unix socket: one JSON object per line
//! each way. Every call here blocks on the socket, so it only runs on
//! background threads (the controller's `answer`).
//!
//! Badges are "CODE/canopy": S synced, P syncing or waiting, E error,
//! C conflict, I ignored. When canopy isn't running, or the folder isn't
//! one of its synced folders, there are simply no badges.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// `$CANOPY_RUNTIME_DIR/canopy.sock`, else `$XDG_RUNTIME_DIR/canopy/canopy.sock`.
pub fn socket_path() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("CANOPY_RUNTIME_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d).join("canopy.sock"));
    }
    let rt = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty())?;
    Some(PathBuf::from(rt).join("canopy").join("canopy.sock"))
}

pub fn available() -> bool {
    socket_path().is_some_and(|p| p.exists())
}

const UNREACHABLE: &str = "Couldn't reach canopy, the Google Drive sync app. Make sure it's running, then try again.";

fn connect(timeout: Duration) -> Result<UnixStream, String> {
    let p = socket_path().ok_or_else(|| UNREACHABLE.to_string())?;
    let s = UnixStream::connect(&p).map_err(|_| UNREACHABLE.to_string())?;
    let _ = s.set_read_timeout(Some(timeout));
    let _ = s.set_write_timeout(Some(timeout));
    Ok(s)
}

/// One request; the `result` of its reply, or canopy's error.
pub fn request(cmd: &str, mut args: Value) -> Result<Value, String> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let mut s = connect(Duration::from_secs(10))?;
    if let Value::Object(m) = &mut args {
        m.insert("id".into(), json!(id));
        m.insert("cmd".into(), json!(cmd));
    }
    let mut line = args.to_string();
    line.push('\n');
    s.write_all(line.as_bytes()).map_err(|_| UNREACHABLE.to_string())?;
    let mut r = BufReader::new(s);
    let mut buf = String::new();
    loop {
        buf.clear();
        match r.read_line(&mut buf) {
            Ok(0) | Err(_) => return Err(UNREACHABLE.to_string()),
            Ok(_) => {}
        }
        let Ok(v) = serde_json::from_str::<Value>(buf.trim()) else { continue };
        if v.get("id").and_then(Value::as_u64) != Some(id) {
            continue;
        }
        if v.get("ok").and_then(Value::as_bool) == Some(true) {
            return Ok(v.get("result").cloned().unwrap_or(Value::Null));
        }
        let e = v.get("error").and_then(Value::as_str).unwrap_or("canopy couldn't do that.");
        return Err(e.to_string());
    }
}

/// Badges for a folder's items, keyed by full path (as vcs.rs), plus
/// `__root` when the folder is inside a synced folder. Empty when canopy
/// isn't running or doesn't know the folder.
pub fn folder_status(dir: &str) -> Value {
    if !dir.starts_with('/') || !available() {
        return json!({});
    }
    let Ok(Value::Object(m)) = request("folder_status", json!({ "dir": dir })) else { return json!({}) };
    let mut out = serde_json::Map::new();
    for (k, v) in m {
        if k == "__root" {
            out.insert(k, v);
        } else if let Some(s) = v.as_str() {
            out.insert(Path::new(dir).join(&k).to_string_lossy().into_owned(), json!(s));
        }
    }
    Value::Object(out)
}

fn err(e: String) -> Value {
    json!({ "error": e })
}

/// Start the canopy window with `args` (its Share sheet, "Sync this folder").
fn spawn_canopy(args: &[&str]) -> Value {
    if std::env::var_os("GROVE_SCREENSHOT").is_some() {
        eprintln!("grove test canopy {args:?}");
        return json!({ "ok": true });
    }
    let mut c = std::process::Command::new("canopy");
    c.args(args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    // Its own session, so it outlives grove's windows.
    use std::os::unix::process::CommandExt;
    unsafe {
        c.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    match c.spawn() {
        Ok(mut ch) => {
            std::thread::spawn(move || ch.wait());
            json!({ "ok": true })
        }
        Err(_) => err("Couldn't open canopy, the Google Drive sync app. Is it installed?".into()),
    }
}

/// The Google Drive menu's actions: file_status, link, star (arg "1"/"0"),
/// unsync, share, add-folder. {error} when it didn't work.
pub fn action(action: &str, path: &str, arg: &str) -> Value {
    let r = match action {
        "file_status" => request("file_status", json!({ "path": path })),
        "link" => request("link", json!({ "path": path })),
        "star" => request("star", json!({ "path": path, "on": arg == "1" })),
        "unsync" => request("file_status", json!({ "path": path })).and_then(|st| {
            let rel = st.get("rel").cloned().unwrap_or(Value::Null);
            request("select", json!({ "account": st.get("account"), "root": st.get("root"), "path": rel, "synced": false }))
        }),
        "share" => return spawn_canopy(&["--share", path]),
        "add-folder" => return spawn_canopy(&["--add-folder", path]),
        _ => Err("Unknown action.".into()),
    };
    match r {
        Ok(v) if v.is_object() => v,
        Ok(_) => json!({ "ok": true }),
        Err(e) => err(e),
    }
}

/// Keeps a `subscribe` connection open and calls `changed` (at most every
/// ~300 ms) when canopy reports status changes; `available` whenever the
/// socket comes or goes. Reconnects quietly every 10 s.
pub fn watch(changed: impl Fn() + Send + 'static, available_cb: impl Fn(bool) + Send + 'static) {
    std::thread::Builder::new()
        .name("canopy-watch".into())
        .spawn(move || {
            let mut was: Option<bool> = None;
            loop {
                let up = available();
                if was != Some(up) {
                    was = Some(up);
                    available_cb(up);
                }
                if up {
                    if let Ok(s) = connect(Duration::from_millis(300)) {
                        // Reconnected: badges may be stale.
                        changed();
                        follow(s, &changed);
                        let now = available();
                        if was != Some(now) {
                            was = Some(now);
                            available_cb(now);
                        }
                    }
                }
                std::thread::sleep(Duration::from_secs(10));
            }
        })
        .ok();
}

/// Read events until the connection drops; debounce them.
fn follow(mut s: UnixStream, changed: &impl Fn()) {
    if s.write_all(b"{\"id\":1,\"cmd\":\"subscribe\"}\n").is_err() {
        return;
    }
    let mut r = BufReader::new(s);
    let mut buf = Vec::new();
    let mut due: Option<Instant> = None;
    loop {
        match r.read_until(b'\n', &mut buf) {
            Ok(0) => return,
            Ok(_) if buf.ends_with(b"\n") => {
                if let Ok(v) = serde_json::from_slice::<Value>(&buf) {
                    if matches!(v.get("event").and_then(Value::as_str), Some("status" | "changed")) && due.is_none() {
                        due = Some(Instant::now() + Duration::from_millis(300));
                    }
                }
                buf.clear();
            }
            Ok(_) => {}
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(_) => return,
        }
        if due.is_some_and(|d| Instant::now() >= d) {
            due = None;
            changed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_status_keys_are_full_paths() {
        let dir = std::env::temp_dir().join(format!("grove-canopy-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("canopy.sock");
        let _ = std::fs::remove_file(&sock);
        let l = std::os::unix::net::UnixListener::bind(&sock).unwrap();
        std::thread::spawn(move || {
            for c in l.incoming().flatten() {
                let mut r = BufReader::new(c.try_clone().unwrap());
                let mut line = String::new();
                r.read_line(&mut line).unwrap();
                let v: Value = serde_json::from_str(&line).unwrap();
                let reply = json!({ "id": v["id"], "ok": true, "result": { "a.txt": "S/canopy", "b": "P/canopy", "__root": "/r" } });
                let mut c = c;
                // An event line first: ignored.
                c.write_all(b"{\"event\":\"changed\"}\n").unwrap();
                c.write_all(format!("{reply}\n").as_bytes()).unwrap();
            }
        });
        std::env::set_var("CANOPY_RUNTIME_DIR", &dir);
        let v = folder_status("/r/x");
        assert_eq!(v["/r/x/a.txt"], "S/canopy");
        assert_eq!(v["/r/x/b"], "P/canopy");
        assert_eq!(v["__root"], "/r");
        std::env::remove_var("CANOPY_RUNTIME_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
