//! Git status for a folder's items (git porcelain output, in the
//! background): "M" modified, "A" added, "?" untracked, "D" deleted,
//! "R" renamed, "U" conflict. A folder shows the strongest status inside.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn rank(c: &str) -> u8 {
    match c {
        "U" => 6,
        "M" => 5,
        "D" => 4,
        "R" => 3,
        "A" => 2,
        "?" => 1,
        _ => 0,
    }
}

pub fn parse_porcelain(top: &Path, dir: &Path, out: &[u8]) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();
    let mut parts = out.split(|&b| b == 0).peekable();
    while let Some(rec) = parts.next() {
        if rec.len() < 4 {
            continue;
        }
        let (x, y) = (rec[0] as char, rec[1] as char);
        let rel = String::from_utf8_lossy(&rec[3..]).into_owned();
        if x == 'R' || x == 'C' {
            parts.next(); // the old name
        }
        let code = if x == '?' {
            "?"
        } else if x == 'U' || y == 'U' || (x == 'A' && y == 'A') || (x == 'D' && y == 'D') {
            "U"
        } else if y == 'M' || x == 'M' {
            "M"
        } else if x == 'A' {
            "A"
        } else if x == 'D' || y == 'D' {
            "D"
        } else if x == 'R' {
            "R"
        } else {
            continue;
        };
        let abs = top.join(rel.trim_end_matches('/'));
        // The item directly inside `dir` this belongs to.
        let Ok(inside) = abs.strip_prefix(dir) else { continue };
        let Some(first) = inside.components().next() else { continue };
        let key: PathBuf = dir.join(first.as_os_str());
        let k = key.to_string_lossy().into_owned();
        let cur = map.get(&k).cloned().unwrap_or_default();
        let code = if inside.components().count() > 1 && code == "?" { "?" } else { code };
        if rank(code) > rank(&cur) {
            map.insert(k, code.to_string());
        }
    }
    map
}

pub fn status(dir: &str) -> Value {
    let d = Path::new(dir);
    let top = match std::process::Command::new("git").args(["-C", dir, "rev-parse", "--show-toplevel"]).output() {
        Ok(o) if o.status.success() => PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()),
        _ => return json!({}),
    };
    let out = match std::process::Command::new("git").args(["-C", dir, "status", "--porcelain=v1", "-z", "--untracked-files=normal", "--", "."]).output() {
        Ok(o) if o.status.success() => o.stdout,
        _ => return json!({}),
    };
    let d = d.canonicalize().unwrap_or(d.to_path_buf());
    let top = top.canonicalize().unwrap_or(top);
    let map = parse_porcelain(&top, &d, &out);
    // Paths back in the folder's own spelling (not canonicalized).
    let orig = Path::new(dir);
    let mut m: HashMap<String, String> = map.into_iter().map(|(k, v)| (orig.join(Path::new(&k).strip_prefix(&d).unwrap_or(Path::new(&k))).to_string_lossy().into_owned(), v)).collect();
    m.insert("__repo".into(), "1".into());
    json!(m)
}

fn run_git(dir: &str, args: &[&str]) -> (bool, String) {
    match std::process::Command::new("git").arg("-C").arg(dir).args(args).env("GIT_TERMINAL_PROMPT", "0").output() {
        Ok(o) => {
            let mut t = String::from_utf8_lossy(&o.stdout).into_owned();
            let e = String::from_utf8_lossy(&o.stderr);
            if !e.trim().is_empty() {
                if !t.is_empty() {
                    t.push('\n');
                }
                t.push_str(e.trim());
            }
            (o.status.success(), t)
        }
        Err(_) => (false, "git isn't installed.".into()),
    }
}

/// Dolphin's git menu: pull, push, commit, add, log, diff.
pub fn action(dir: &str, action: &str, paths: &[PathBuf], arg: &str, dark: bool) -> Value {
    let rel: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let mut args: Vec<&str> = vec![];
    let (ok, out) = match action {
        "pull" => run_git(dir, &["pull", "--ff-only"]),
        "push" => run_git(dir, &["push"]),
        "add" => {
            args.extend(["add", "--"]);
            args.extend(rel.iter().map(String::as_str));
            run_git(dir, &args)
        }
        "commit" => {
            if arg.trim().is_empty() {
                return json!({"error": "A commit needs a message describing the change."});
            }
            if rel.is_empty() {
                run_git(dir, &["commit", "-a", "-m", arg])
            } else {
                args.extend(["commit", "-m", arg, "--"]);
                args.extend(rel.iter().map(String::as_str));
                run_git(dir, &args)
            }
        }
        "log" => {
            args.extend(["log", "--date=short", "--pretty=format:%h  %ad  %an  %s", "-n", "300", "--"]);
            args.extend(rel.iter().map(String::as_str));
            run_git(dir, &args)
        }
        "diff" => {
            args.extend(["diff", "HEAD", "--"]);
            args.extend(rel.iter().map(String::as_str));
            let (ok, t) = run_git(dir, &args);
            return json!({"ok": ok, "output": t, "html": crate::preview::highlight(&t, "x.diff", dark)});
        }
        _ => return json!({"error": format!("Unknown git action “{action}”.")}),
    };
    if ok {
        json!({"ok": true, "output": out})
    } else {
        json!({"error": if out.is_empty() { format!("git {action} didn't work.") } else { out }})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses() {
        let out = b" M src/main.rs\0?? notes.txt\0A  src/new.rs\0R  b.txt\0a.txt\0?? build/\0";
        let m = parse_porcelain(Path::new("/r"), Path::new("/r"), out);
        assert_eq!(m["/r/src"], "M");
        assert_eq!(m["/r/notes.txt"], "?");
        assert_eq!(m["/r/b.txt"], "R");
        assert_eq!(m["/r/build"], "?");
        let m = parse_porcelain(Path::new("/r"), Path::new("/r/src"), out);
        assert_eq!(m["/r/src/main.rs"], "M");
        assert_eq!(m["/r/src/new.rs"], "A");
        assert_eq!(m.len(), 2);
    }
}
