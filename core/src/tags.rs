//! Tags and comments in extended attributes, the same ones Dolphin/Baloo
//! use: `user.xdg.tags` (comma-separated names) and `user.xdg.comment`.
//! Finder's colour tags are tags named after their colour.

use std::path::Path;

pub const TAGS_ATTR: &str = "user.xdg.tags";
pub const COMMENT_ATTR: &str = "user.xdg.comment";

/// Finder's seven colour tags, with their colours.
pub const COLOR_TAGS: [(&str, &str); 7] = [
    ("Red", "#ff4f4f"),
    ("Orange", "#ff9f0a"),
    ("Yellow", "#ffd60a"),
    ("Green", "#30d158"),
    ("Blue", "#0a84ff"),
    ("Purple", "#bf5af2"),
    ("Gray", "#98989d"),
];

pub fn color_of(tag: &str) -> Option<&'static str> {
    COLOR_TAGS.iter().find(|(n, _)| n.eq_ignore_ascii_case(tag) || (tag.eq_ignore_ascii_case("grey") && *n == "Gray")).map(|(_, c)| *c)
}

pub fn parse(value: &str) -> Vec<String> {
    let mut v: Vec<String> = value.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    v.dedup();
    v
}

pub fn get(path: &Path) -> Vec<String> {
    match xattr::get(path, TAGS_ATTR) {
        Ok(Some(b)) => parse(&String::from_utf8_lossy(&b)),
        _ => vec![],
    }
}

pub fn set(path: &Path, tags: &[String]) -> Result<(), String> {
    let mut clean: Vec<String> = Vec::new();
    for t in tags {
        let t = t.replace(',', " ").trim().to_string();
        if !t.is_empty() && !clean.contains(&t) {
            clean.push(t);
        }
    }
    let r = if clean.is_empty() {
        match xattr::remove(path, TAGS_ATTR) {
            Err(e) if e.raw_os_error() == Some(libc::ENODATA) => Ok(()),
            r => r,
        }
    } else {
        xattr::set(path, TAGS_ATTR, clean.join(",").as_bytes())
    };
    r.map_err(|e| match e.raw_os_error() {
        Some(libc::ENOTSUP) => format!("Tags can't be saved on this disk (it doesn't support extended attributes): {}", path.display()),
        _ => crate::util::plain_error(&e, path),
    })
}

pub fn add(path: &Path, tag: &str) -> Result<(), String> {
    let mut t = get(path);
    if !t.iter().any(|x| x == tag) {
        t.push(tag.to_string());
    }
    set(path, &t)
}

pub fn remove(path: &Path, tag: &str) -> Result<(), String> {
    let t: Vec<String> = get(path).into_iter().filter(|x| x != tag).collect();
    set(path, &t)
}

/// Tag on/off for a selection: if every item has it, remove it, else add.
pub fn toggle(paths: &[&Path], tag: &str) -> Result<bool, String> {
    let all = paths.iter().all(|p| get(p).iter().any(|x| x == tag));
    for p in paths {
        if all {
            remove(p, tag)?
        } else {
            add(p, tag)?
        }
    }
    Ok(!all)
}

pub fn comment(path: &Path) -> String {
    match xattr::get(path, COMMENT_ATTR) {
        Ok(Some(b)) => String::from_utf8_lossy(&b).into_owned(),
        _ => String::new(),
    }
}

pub fn set_comment(path: &Path, text: &str) -> Result<(), String> {
    let r = if text.is_empty() {
        match xattr::remove(path, COMMENT_ATTR) {
            Err(e) if e.raw_os_error() == Some(libc::ENODATA) => Ok(()),
            r => r,
        }
    } else {
        xattr::set(path, COMMENT_ATTR, text.as_bytes())
    };
    r.map_err(|e| crate::util::plain_error(&e, path))
}

/// Tag names known to grove: the colour tags, the user's own list
/// (~/.config/grove/tags.json) and any seen on files.
pub fn known() -> Vec<String> {
    let mut v: Vec<String> = COLOR_TAGS.iter().map(|(n, _)| n.to_string()).collect();
    if let Ok(s) = std::fs::read_to_string(crate::util::grove_config().join("tags.json")) {
        if let Ok(list) = serde_json::from_str::<Vec<String>>(&s) {
            for t in list {
                if !v.contains(&t) {
                    v.push(t);
                }
            }
        }
    }
    v
}

pub fn remember(tag: &str) {
    let p = crate::util::grove_config().join("tags.json");
    let mut list: Vec<String> = std::fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    if color_of(tag).is_none() && !list.iter().any(|t| t == tag) {
        list.push(tag.to_string());
        let _ = crate::util::write_atomic(&p, serde_json::to_string_pretty(&list).unwrap_or_default().as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        // tmpfs supports user xattrs on modern kernels; fall back to the
        // home cache when it doesn't.
        let base = std::env::temp_dir();
        let t = tempfile::tempdir_in(&base).unwrap();
        let f = t.path().join("f.txt");
        std::fs::write(&f, "x").unwrap();
        if xattr::set(&f, "user.grove.probe", b"1").is_err() {
            eprintln!("xattrs unsupported on {}; skipping", base.display());
            return;
        }
        assert!(get(&f).is_empty());
        set(&f, &["Red".into(), "Work, stuff".into(), "Red".into()]).unwrap();
        assert_eq!(get(&f), vec!["Red".to_string(), "Work  stuff".to_string()]);
        // Same bytes Dolphin writes.
        assert_eq!(xattr::get(&f, TAGS_ATTR).unwrap().unwrap(), b"Red,Work  stuff");
        add(&f, "Blue").unwrap();
        remove(&f, "Red").unwrap();
        assert_eq!(get(&f), vec!["Work  stuff".to_string(), "Blue".to_string()]);
        let p: &Path = &f;
        assert!(!toggle(&[p], "Blue").unwrap());
        assert!(toggle(&[p], "Green").unwrap());
        set(&f, &[]).unwrap();
        assert!(xattr::get(&f, TAGS_ATTR).unwrap().is_none());
        set_comment(&f, "hello").unwrap();
        assert_eq!(comment(&f), "hello");
        // A value written by Dolphin with stray spaces.
        xattr::set(&f, TAGS_ATTR, b"a, b,,c").unwrap();
        assert_eq!(get(&f), vec!["a", "b", "c"]);
        assert_eq!(color_of("grey"), Some("#98989d"));
    }
}
