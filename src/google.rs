//! Google sign-in and Drive, done by grove itself (no rclone): OAuth 2.0
//! for installed apps with PKCE and a loopback redirect. The browser shows
//! Google's own sign-in; grove only receives the code on 127.0.0.1 and
//! trades it for tokens. The refresh token is kept in the keyring (Secret
//! Service); access tokens live in memory. The OAuth client (ID and
//! secret, from the user's own Google Cloud project; GoogleSheet.qml walks
//! through making it) is in grove's settings.
//!
//! Used to export Google Docs/Sheets/Slides that synced folders keep only
//! as link files (gdocs.rs), so previews show their content.

use serde_json::{json, Value};
use sha2::Digest;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const AUTH: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN: &str = "https://oauth2.googleapis.com/token";
pub const SCOPES: &str = "openid email https://www.googleapis.com/auth/drive.readonly";

fn b64url(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut s = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..(c.len() + 1) {
            s.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    s
}

fn b64url_decode(s: &str) -> Vec<u8> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'-' | b'+' => Some(62),
        b'_' | b'/' => Some(63),
        _ => None,
    };
    let v: Vec<u8> = s.bytes().filter_map(val).collect();
    let mut out = Vec::new();
    for c in v.chunks(4) {
        let n = c.iter().enumerate().fold(0u32, |n, (i, &x)| n | (x as u32) << (18 - 6 * i));
        for i in 0..c.len().saturating_sub(1) {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    out
}

fn random_token(n: usize) -> String {
    let b: Vec<u8> = (0..n).map(|_| rand::random::<u8>()).collect();
    b64url(&b)
}

fn pct(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

pub fn keyring_key(email: &str) -> String {
    format!("google-oauth:{email}")
}

fn client(settings: &grove_core::settings::Settings) -> Result<(String, String), String> {
    let (id, secret) = (settings.google_client_id.trim().to_string(), settings.google_client_secret.trim().to_string());
    if id.is_empty() {
        return Err("Set up grove's Google sign-in first (its client ID and secret).".into());
    }
    Ok((id, secret))
}

/// Sign in: Google's page in the browser, the answer on a loopback port.
/// Returns the account's email (its refresh token is in the keyring).
pub fn sign_in(settings: &grove_core::settings::Settings) -> Result<String, String> {
    let (id, secret) = client(settings)?;
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}");
    let verifier = random_token(48);
    let challenge = b64url(&sha2::Sha256::digest(verifier.as_bytes()));
    let state = random_token(16);
    let url = format!(
        "{AUTH}?client_id={}&redirect_uri={}&response_type=code&scope={}&code_challenge={challenge}&code_challenge_method=S256&state={state}&access_type=offline&prompt=consent",
        pct(&id),
        pct(&redirect),
        pct(SCOPES)
    );
    grove_core::apps::open_url(&url)?;
    // Wait (up to 5 minutes) for Google to send the browser back here.
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(300);
    let (mut stream, _) = loop {
        match listener.accept() {
            Ok(s) => break s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    return Err("Signing in took too long (5 minutes). Try again.".into());
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    let _ = stream.set_nonblocking(false);
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line).map_err(|e| e.to_string())?;
    let target = line.split_whitespace().nth(1).unwrap_or("");
    let q: HashMap<String, String> = target
        .split_once('?')
        .map(|(_, q)| q)
        .unwrap_or("")
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.to_string(), percent_encoding::percent_decode_str(&v.replace('+', " ")).decode_utf8_lossy().into_owned()))
        .collect();
    let page = |ok: bool, msg: &str| {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<!doctype html><meta charset=utf-8><title>grove</title><body style=\"font:16px system-ui;margin:4em;text-align:center\"><h2>{}</h2><p>{}</p></body>",
            if ok { "You're signed in to grove" } else { "grove couldn't sign in" },
            msg
        )
    };
    if q.get("state") != Some(&state) {
        let _ = stream.write_all(page(false, "The answer didn't match (try again from grove).").as_bytes());
        return Err("The sign-in answer didn't match. Try again.".into());
    }
    let Some(code) = q.get("code") else {
        let why = q.get("error").cloned().unwrap_or_else(|| "no code".into());
        let _ = stream.write_all(page(false, &format!("Google said: {why}")).as_bytes());
        return Err(if why == "access_denied" { "Signing in was cancelled.".into() } else { format!("Google didn't sign you in: {why}") });
    };
    let mut r = ureq::post(TOKEN)
        .send_form([
            ("code", code.as_str()),
            ("client_id", id.as_str()),
            ("client_secret", secret.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("grant_type", "authorization_code"),
            ("code_verifier", verifier.as_str()),
        ])
        .map_err(|e| format!("Google didn't give grove a token: {e}"))?;
    let t: Value = r.body_mut().read_json().map_err(|e| e.to_string())?;
    let refresh = t["refresh_token"].as_str().ok_or("Google gave no refresh token (try signing in again).")?.to_string();
    // The account's email, from the ID token (a JWT straight from Google).
    let email = t["id_token"]
        .as_str()
        .and_then(|j| j.split('.').nth(1))
        .and_then(|p| serde_json::from_slice::<Value>(&b64url_decode(p)).ok())
        .and_then(|c| c["email"].as_str().map(str::to_string))
        .unwrap_or_else(|| "Google account".into());
    crate::secrets::store(&keyring_key(&email), &refresh)?;
    if let (Some(a), Some(e)) = (t["access_token"].as_str(), t["expires_in"].as_u64()) {
        tokens().lock().unwrap().insert(email.clone(), (a.to_string(), Instant::now() + Duration::from_secs(e.saturating_sub(60))));
    }
    let _ = stream.write_all(page(true, &format!("Signed in as {email}. You can close this tab and go back to grove.")).as_bytes());
    Ok(email)
}

fn tokens() -> &'static Mutex<HashMap<String, (String, Instant)>> {
    static T: std::sync::OnceLock<Mutex<HashMap<String, (String, Instant)>>> = std::sync::OnceLock::new();
    T.get_or_init(Default::default)
}

/// A current access token for `email` (refreshed when it has expired).
fn access_token(settings: &grove_core::settings::Settings, email: &str) -> Result<String, String> {
    if let Some((t, until)) = tokens().lock().unwrap().get(email) {
        if Instant::now() < *until {
            return Ok(t.clone());
        }
    }
    let (id, secret) = client(settings)?;
    let refresh = crate::secrets::lookup(&keyring_key(email)).ok_or_else(|| format!("Sign in to Google as {email} again."))?;
    let mut r = ureq::post(TOKEN)
        .send_form([("client_id", id.as_str()), ("client_secret", secret.as_str()), ("refresh_token", refresh.as_str()), ("grant_type", "refresh_token")])
        .map_err(|e| match e {
            // invalid_grant: the sign-in ended (apps in Testing: after 7 days).
            ureq::Error::StatusCode(400 | 401) => {
                tokens().lock().unwrap().remove(email);
                format!("Sign in to Google again ({email}): Google ends the sign-in every 7 days for your own Google Cloud app.")
            }
            e => format!("Google didn't answer ({e})."),
        })?;
    let t: Value = r.body_mut().read_json().map_err(|e| e.to_string())?;
    let a = t["access_token"].as_str().ok_or("Google gave no access token.")?.to_string();
    let e = t["expires_in"].as_u64().unwrap_or(3600);
    tokens().lock().unwrap().insert(email.to_string(), (a.clone(), Instant::now() + Duration::from_secs(e.saturating_sub(60))));
    Ok(a)
}

/// Export a Google document as `mime` (Drive API files.export).
pub fn export(settings: &grove_core::settings::Settings, email: &str, file_id: &str, mime: &str) -> Result<Vec<u8>, String> {
    let token = access_token(settings, email)?;
    export_with(&token, email, file_id, mime)
}

/// Export with an access token from elsewhere (hearth's accounts service).
pub fn export_with(token: &str, email: &str, file_id: &str, mime: &str) -> Result<Vec<u8>, String> {
    let url = format!("https://www.googleapis.com/drive/v3/files/{}/export?mimeType={}", pct(file_id), pct(mime));
    let mut r = ureq::get(&url).header("Authorization", &format!("Bearer {token}")).call().map_err(|e| match e {
        ureq::Error::StatusCode(404) => format!("{email} can't see this document (signed in to the right account?)."),
        ureq::Error::StatusCode(403) => "Google didn't allow the export (the Drive API may be off in your Google Cloud project, or the file is too big to export).".to_string(),
        e => format!("Google Drive didn't send it: {e}"),
    })?;
    r.body_mut().with_config().limit(256 << 20).read_to_vec().map_err(|e| e.to_string())
}

pub fn sign_out(email: &str) {
    crate::secrets::delete(&keyring_key(email));
    tokens().lock().unwrap().remove(email);
}

/// What the settings know: the client, and the accounts signed in.
pub fn status(settings: &grove_core::settings::Settings) -> Value {
    json!({"configured": !settings.google_client_id.trim().is_empty(), "accounts": settings.google_accounts})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn base64url() {
        assert_eq!(b64url(b"hello"), "aGVsbG8");
        assert_eq!(b64url_decode("aGVsbG8"), b"hello");
        assert_eq!(b64url_decode(&b64url(&[0xff, 0xfe, 0x00, 0x10])), vec![0xff, 0xfe, 0x00, 0x10]);
    }
}
