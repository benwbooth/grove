//! Share with nearby devices over LocalSend (an open protocol, like AirDrop:
//! phones and computers running the LocalSend app on the same network).
//! grove finds them with the protocol's UDP multicast announcement and
//! sends files with its HTTP(S) upload API (v2). Sending only: receiving
//! is left to the LocalSend app.

use serde_json::{json, Value};
use std::io::Read;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 167);
const PORT: u16 = 53317;

#[derive(Clone, Debug, serde::Serialize)]
pub struct Device {
    pub alias: String,
    pub ip: String,
    pub port: u16,
    pub protocol: String,
    pub device_type: String,
    pub device_model: String,
    pub fingerprint: String,
}

fn fingerprint() -> &'static str {
    static F: OnceLock<String> = OnceLock::new();
    F.get_or_init(|| {
        use rand::Rng;
        let mut r = rand::rng();
        (0..32).map(|_| format!("{:x}", r.random_range(0..16u8))).collect()
    })
}

fn alias() -> String {
    let host = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap_or_default().trim().to_string();
    if host.is_empty() {
        "Grove".into()
    } else {
        format!("Grove on {host}")
    }
}

fn info(announce: bool) -> Value {
    json!({
        "alias": alias(), "version": "2.1", "deviceModel": "Linux", "deviceType": "desktop",
        "fingerprint": fingerprint(), "port": PORT, "protocol": "http", "download": false, "announce": announce,
    })
}

fn socket() -> std::io::Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    s.set_reuse_address(true)?;
    s.set_reuse_port(true)?;
    s.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, PORT).into())?;
    let u: UdpSocket = s.into();
    u.join_multicast_v4(&GROUP, &Ipv4Addr::UNSPECIFIED)?;
    u.set_multicast_loop_v4(false)?;
    u.set_read_timeout(Some(Duration::from_millis(200)))?;
    Ok(u)
}

/// Announce ourselves and collect the devices that answer within `wait`.
pub fn discover(wait: Duration) -> Result<Vec<Device>, String> {
    let s = socket().map_err(|e| format!("Can't look for nearby devices: {e}"))?;
    let hello = info(true).to_string();
    let _ = s.send_to(hello.as_bytes(), SocketAddrV4::new(GROUP, PORT));
    let start = Instant::now();
    let mut found: Vec<Device> = Vec::new();
    let mut buf = [0u8; 8192];
    let mut resent = false;
    while start.elapsed() < wait {
        if !resent && start.elapsed() > wait / 2 {
            resent = true;
            let _ = s.send_to(hello.as_bytes(), SocketAddrV4::new(GROUP, PORT));
        }
        let Ok((n, from)) = s.recv_from(&mut buf) else { continue };
        let Ok(v) = serde_json::from_slice::<Value>(&buf[..n]) else { continue };
        let fp = v["fingerprint"].as_str().unwrap_or("").to_string();
        if fp == fingerprint() || fp.is_empty() || found.iter().any(|d| d.fingerprint == fp) {
            continue;
        }
        // Answer announcements so the other side lists us too.
        if v["announce"].as_bool().unwrap_or(false) {
            let _ = s.send_to(info(false).to_string().as_bytes(), SocketAddrV4::new(GROUP, PORT));
        }
        found.push(Device {
            alias: v["alias"].as_str().unwrap_or("Device").to_string(),
            ip: from.ip().to_string(),
            port: v["port"].as_u64().unwrap_or(PORT as u64) as u16,
            protocol: v["protocol"].as_str().unwrap_or("https").to_string(),
            device_type: v["deviceType"].as_str().unwrap_or("").to_string(),
            device_model: v["deviceModel"].as_str().unwrap_or("").to_string(),
            fingerprint: fp,
        });
    }
    Ok(found)
}

fn agent() -> ureq::Agent {
    // LocalSend devices use self-signed certificates.
    ureq::Agent::config_builder()
        .tls_config(ureq::tls::TlsConfig::builder().disable_verification(true).build())
        .timeout_global(None)
        .http_status_as_error(false)
        .build()
        .new_agent()
}

struct Counting<'a, R: Read> {
    inner: R,
    done: &'a mut u64,
    tick: &'a mut dyn FnMut(u64),
    cancel: &'a AtomicBool,
}

impl<R: Read> Read for Counting<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::other("cancelled"));
        }
        let n = self.inner.read(buf)?;
        *self.done += n as u64;
        (self.tick)(*self.done);
        Ok(n)
    }
}

/// Send files to a device. `progress(done_bytes, total_bytes)`.
pub fn send(dev: &Device, files: &[PathBuf], cancel: &AtomicBool, mut progress: impl FnMut(u64, u64)) -> Result<(), String> {
    let base = format!("{}://{}:{}/api/localsend/v2", dev.protocol, dev.ip, dev.port);
    let mut meta = serde_json::Map::new();
    let mut total = 0u64;
    for (i, f) in files.iter().enumerate() {
        let m = std::fs::metadata(f).map_err(|e| grove_core::util::plain_error(&e, f))?;
        if m.is_dir() {
            return Err("Folders can't be sent with LocalSend; compress the folder first (right-click › Compress).".into());
        }
        total += m.len();
        let mime = crate::mime::deep(&f.to_string_lossy()).name;
        meta.insert(format!("f{i}"), json!({"id": format!("f{i}"), "fileName": f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), "size": m.len(), "fileType": mime}));
    }
    let a = agent();
    let resp = a
        .post(&format!("{base}/prepare-upload"))
        .send_json(json!({"info": info(false), "files": meta}))
        .map_err(|e| format!("Couldn't reach “{}”: {e}", dev.alias))?;
    match resp.status().as_u16() {
        200 => {}
        204 => return Ok(()),
        403 => return Err(format!("“{}” declined the files.", dev.alias)),
        409 => return Err(format!("“{}” is busy receiving something else.", dev.alias)),
        429 => return Err(format!("“{}” is getting too many requests; try again in a moment.", dev.alias)),
        s => return Err(format!("“{}” answered with an error ({s}).", dev.alias)),
    }
    let v: Value = resp.into_body().read_json().map_err(|e| e.to_string())?;
    let session = v["sessionId"].as_str().unwrap_or("").to_string();
    let mut done = 0u64;
    for (i, f) in files.iter().enumerate() {
        let id = format!("f{i}");
        let Some(token) = v["files"][&id].as_str() else { continue };
        let file = std::fs::File::open(f).map_err(|e| grove_core::util::plain_error(&e, f))?;
        let mut tick = |d: u64| progress(d, total);
        let mut reader = Counting { inner: file, done: &mut done, tick: &mut tick, cancel };
        let url = format!("{base}/upload?sessionId={session}&fileId={id}&token={token}");
        let r = a.post(&url).send(ureq::SendBody::from_reader(&mut reader)).map_err(|e| {
            if cancel.load(Ordering::Relaxed) {
                "Cancelled".to_string()
            } else {
                format!("Sending stopped: {e}")
            }
        })?;
        if r.status().as_u16() != 200 {
            return Err(format!("“{}” didn't accept {} ({}).", dev.alias, f.display(), r.status()));
        }
    }
    Ok(())
}
