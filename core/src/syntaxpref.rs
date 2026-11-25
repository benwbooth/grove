//! The syntax colouring language chosen for a text file (the viewer's
//! language menu). Kept on the file in `user.grove.syntax`, so it follows
//! the file through renames and moves; where the file can't carry it
//! (no extended attributes, read-only, not ours) in
//! ~/.config/grove/syntax.json, keyed by path. No choice: guessed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const SYNTAX_ATTR: &str = "user.grove.syntax";

fn map_path() -> PathBuf {
    crate::util::grove_config().join("syntax.json")
}

fn load_map() -> BTreeMap<String, String> {
    std::fs::read_to_string(map_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_map(m: &BTreeMap<String, String>) -> Result<(), String> {
    crate::util::write_atomic(&map_path(), serde_json::to_string_pretty(m).unwrap().as_bytes()).map_err(|e| e.to_string())
}

fn key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The language chosen for `path` (a syntax name, "Rust"), if any.
pub fn get(path: &Path) -> Option<String> {
    if let Ok(Some(b)) = xattr::get(path, SYNTAX_ATTR) {
        let s = String::from_utf8_lossy(&b).trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    load_map().remove(&key(path))
}

/// Choose `lang` for `path`; None (Automatic) forgets the choice.
pub fn set(path: &Path, lang: Option<&str>) -> Result<(), String> {
    let lang = lang.map(str::trim).filter(|l| !l.is_empty());
    let mut map = load_map();
    let had = map.contains_key(&key(path));
    let on_file = match lang {
        Some(l) => xattr::set(path, SYNTAX_ATTR, l.as_bytes()).is_ok(),
        None => {
            let _ = xattr::remove(path, SYNTAX_ATTR);
            false
        }
    };
    match lang {
        Some(l) if !on_file => {
            map.insert(key(path), l.to_string());
        }
        _ if had => {
            map.remove(&key(path));
        }
        _ => return Ok(()),
    }
    save_map(&map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_fallback() {
        let _g = crate::test_env_lock();
        let t = tempfile::tempdir().unwrap();
        let old = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", t.path().join("config"));
        let f = t.path().join("notes.txt");
        std::fs::write(&f, "x").unwrap();
        assert_eq!(get(&f), None);
        set(&f, Some("Rust")).unwrap();
        assert_eq!(get(&f).as_deref(), Some("Rust"));
        if xattr::set(&f, "user.grove.probe", b"1").is_ok() {
            // On the file: it follows a rename.
            let g = t.path().join("moved.txt");
            std::fs::rename(&f, &g).unwrap();
            assert_eq!(get(&g).as_deref(), Some("Rust"));
            std::fs::rename(&g, &f).unwrap();
        }
        set(&f, None).unwrap();
        assert_eq!(get(&f), None);
        // A file that can't carry it (no such file here): the map.
        let gone = t.path().join("nowhere/x.txt");
        set(&gone, Some("Python")).unwrap();
        assert_eq!(get(&gone).as_deref(), Some("Python"));
        set(&gone, None).unwrap();
        assert_eq!(get(&gone), None);
        assert!(load_map().is_empty());
        match old {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }
}
