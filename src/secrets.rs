//! Passwords for saved servers, in the desktop's keyring through the
//! freedesktop Secret Service API (KWallet and others provide it).
//! A minimal client: "plain" session, one collection ("default" alias).

use std::collections::HashMap;
use zbus::blocking::Connection;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

const DEST: &str = "org.freedesktop.secrets";

type Secret = (OwnedObjectPath, Vec<u8>, Vec<u8>, String);

fn session(c: &Connection) -> zbus::Result<OwnedObjectPath> {
    let r = c.call_method(Some(DEST), "/org/freedesktop/secrets", Some("org.freedesktop.Secret.Service"), "OpenSession", &("plain", Value::from("")))?;
    let (_out, path): (OwnedValue, OwnedObjectPath) = r.body().deserialize()?;
    Ok(path)
}

fn attrs(server: &str) -> HashMap<&'static str, String> {
    let mut a = HashMap::new();
    a.insert("application", "grove".to_string());
    a.insert("server", server.to_string());
    a
}

fn unlock(c: &Connection, objs: &[OwnedObjectPath]) {
    let _ = c.call_method(Some(DEST), "/org/freedesktop/secrets", Some("org.freedesktop.Secret.Service"), "Unlock", &(objs.to_vec(),));
}

pub fn store(server: &str, password: &str) -> Result<(), String> {
    let c = Connection::session().map_err(|e| e.to_string())?;
    let s = session(&c).map_err(|_| "No keyring (Secret Service) is running, so the password can't be saved.".to_string())?;
    let collection = ObjectPath::try_from("/org/freedesktop/secrets/aliases/default").unwrap();
    unlock(&c, &[collection.clone().into()]);
    let mut props: HashMap<&str, Value> = HashMap::new();
    props.insert("org.freedesktop.Secret.Item.Label", Value::from(format!("grove: {server}")));
    props.insert("org.freedesktop.Secret.Item.Attributes", Value::from(attrs(server)));
    let secret: Secret = (s, vec![], password.as_bytes().to_vec(), "text/plain".into());
    c.call_method(Some(DEST), &collection, Some("org.freedesktop.Secret.Collection"), "CreateItem", &(props, secret, true)).map_err(|e| format!("The keyring refused the password: {e}"))?;
    Ok(())
}

pub fn lookup(server: &str) -> Option<String> {
    let c = Connection::session().ok()?;
    let r = c.call_method(Some(DEST), "/org/freedesktop/secrets", Some("org.freedesktop.Secret.Service"), "SearchItems", &(attrs(server),)).ok()?;
    let (unlocked, locked): (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) = r.body().deserialize().ok()?;
    if !locked.is_empty() {
        unlock(&c, &locked);
    }
    let item = unlocked.into_iter().chain(locked).next()?;
    let s = session(&c).ok()?;
    let r = c.call_method(Some(DEST), &item, Some("org.freedesktop.Secret.Item"), "GetSecret", &(s,)).ok()?;
    let secret: Secret = r.body().deserialize().ok()?;
    String::from_utf8(secret.2).ok()
}

pub fn delete(server: &str) {
    let Ok(c) = Connection::session() else { return };
    let Ok(r) = c.call_method(Some(DEST), "/org/freedesktop/secrets", Some("org.freedesktop.Secret.Service"), "SearchItems", &(attrs(server),)) else { return };
    let Ok((a, b)): Result<(Vec<OwnedObjectPath>, Vec<OwnedObjectPath>), _> = r.body().deserialize() else { return };
    for item in a.into_iter().chain(b) {
        let _ = c.call_method(Some(DEST), &item, Some("org.freedesktop.Secret.Item"), "Delete", &());
    }
}
