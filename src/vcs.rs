//! Version control badges for a folder's items, read in-process (no git,
//! hg, svn or fossil commands): which folders are repositories (git,
//! Mercurial, Subversion, Jujutsu, Fossil, Bazaar), and every item's state
//! in the repository holding the folder: C clean, M modified, A added,
//! D deleted, R renamed, U conflict, ? untracked, I ignored.
//!
//! Values are "CODE/KIND" ("M/git", "C/hg"); a repository's own folder is
//! "@/KIND". A folder shows the strongest state inside it.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Which kind of repository `dir` itself is (its own marker inside).
pub fn repo_kind(dir: &Path) -> Option<&'static str> {
    if dir.join(".jj").is_dir() {
        return Some("jj");
    }
    if dir.join(".git").exists() {
        return Some("git");
    }
    if dir.join(".hg").is_dir() {
        return Some("hg");
    }
    if dir.join(".svn").is_dir() {
        return Some("svn");
    }
    if dir.join(".fslckout").is_file() || dir.join("_FOSSIL_").is_file() {
        return Some("fossil");
    }
    if dir.join(".bzr").is_dir() {
        return Some("bzr");
    }
    None
}

/// The branch a git repository's folder is on, from its HEAD file (cheap:
/// no libgit2): "main", or the short commit when detached. Worktrees and
/// submodules (a `.git` file pointing elsewhere) are followed.
pub fn git_branch(dir: &Path) -> Option<String> {
    let dotgit = dir.join(".git");
    let gitdir = if dotgit.is_file() {
        let t = std::fs::read_to_string(&dotgit).ok()?;
        let p = PathBuf::from(t.trim().strip_prefix("gitdir:")?.trim());
        if p.is_absolute() { p } else { dir.join(p) }
    } else {
        dotgit
    };
    let head = std::fs::read_to_string(gitdir.join("HEAD")).ok()?;
    let head = head.trim();
    match head.strip_prefix("ref:") {
        Some(r) => Some(r.trim().trim_start_matches("refs/heads/").to_string()),
        None => Some(head.chars().take(7).collect()),
    }
}

/// The repository holding `dir` (it or a parent): (top, kind).
fn find_repo(dir: &Path) -> Option<(PathBuf, &'static str)> {
    let mut d = Some(dir);
    while let Some(p) = d {
        if let Some(k) = repo_kind(p) {
            return Some((p.to_path_buf(), k));
        }
        d = p.parent();
    }
    None
}

fn rank(c: &str) -> u8 {
    match c {
        "U" => 8,
        "M" => 7,
        "D" => 6,
        "R" => 5,
        "A" => 4,
        "?" => 3,
        "C" => 2,
        "I" => 1,
        _ => 0,
    }
}

/// Fold `rel`'s state (relative to the repository's top) into the item
/// directly inside `dir` it belongs to.
fn fold(map: &mut HashMap<String, String>, top: &Path, dir: &Path, rel: &str, code: &str) {
    let abs = top.join(rel.trim_end_matches('/'));
    let Ok(inside) = abs.strip_prefix(dir) else { return };
    let Some(first) = inside.components().next() else { return };
    let k = dir.join(first.as_os_str()).to_string_lossy().into_owned();
    // A folder that's only partly ignored isn't ignored.
    let code = if code == "I" && inside.components().count() > 1 { "C" } else { code };
    let cur = map.get(&k).map(String::as_str).unwrap_or("");
    if rank(code) > rank(cur) {
        map.insert(k, code.to_string());
    }
}

// ---------------------------------------------------------------- git

fn git_code(s: git2::Status) -> &'static str {
    use git2::Status as S;
    if s.contains(S::CONFLICTED) {
        "U"
    } else if s.intersects(S::WT_MODIFIED | S::INDEX_MODIFIED | S::WT_TYPECHANGE | S::INDEX_TYPECHANGE) {
        "M"
    } else if s.contains(S::INDEX_NEW) {
        "A"
    } else if s.intersects(S::INDEX_DELETED | S::WT_DELETED) {
        "D"
    } else if s.intersects(S::INDEX_RENAMED | S::WT_RENAMED) {
        "R"
    } else if s.contains(S::WT_NEW) {
        "?"
    } else if s.contains(S::IGNORED) {
        "I"
    } else {
        "C"
    }
}

fn git(dir: &Path, map: &mut HashMap<String, String>) -> Option<()> {
    let repo = git2::Repository::discover(dir).ok()?;
    let top = repo.workdir()?.canonicalize().ok()?;
    let rel_dir = dir.strip_prefix(&top).ok()?.to_string_lossy().into_owned();
    let spec = if rel_dir.is_empty() { String::new() } else { format!("{rel_dir}/") };
    // Every tracked file under the folder starts clean...
    if let Ok(idx) = repo.index() {
        for e in idx.iter() {
            let p = String::from_utf8_lossy(&e.path);
            if p.starts_with(&spec) {
                fold(map, &top, dir, &p, "C");
            }
        }
    }
    // ...then what changed, what's new, and what's ignored.
    let mut o = git2::StatusOptions::new();
    o.include_untracked(true).recurse_untracked_dirs(false).include_ignored(true).recurse_ignored_dirs(false).renames_head_to_index(true);
    if !rel_dir.is_empty() {
        o.pathspec(&rel_dir);
    }
    for e in repo.statuses(Some(&mut o)).ok()?.iter() {
        if let Ok(p) = e.path() {
            fold(map, &top, dir, p, git_code(e.status()));
        }
    }
    Some(())
}

// ---------------------------------------------------------------- Mercurial

/// `.hg/dirstate` (version 1): the tracked files with their recorded size
/// and time; a size or time that differs from the file's means modified.
fn hg(top: &Path, dir: &Path, map: &mut HashMap<String, String>) -> Option<()> {
    let ds = top.join(".hg/dirstate");
    let b = std::fs::read(&ds).ok()?;
    // Entries hg left to check later (size/time -1): changed if written
    // after hg last wrote its dirstate.
    let ds_time = std::fs::metadata(&ds).ok().and_then(|m| m.modified().ok());
    if b.starts_with(b"dirstate-v2") || b.len() < 40 {
        return None;
    }
    let be = |i: usize| i32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let mut i = 40;
    let mut tracked = std::collections::HashSet::new();
    while i + 17 <= b.len() {
        let state = b[i];
        let (size, mtime, len) = (be(i + 5), be(i + 9), be(i + 13).max(0) as usize);
        i += 17;
        if i + len > b.len() {
            break;
        }
        let name = &b[i..i + len];
        i += len;
        let name = String::from_utf8_lossy(name.split(|&c| c == 0).next().unwrap_or(name)).into_owned();
        let code = match state {
            b'a' => "A",
            b'r' => "D",
            b'm' => "M",
            _ => match std::fs::symlink_metadata(top.join(&name)) {
                Err(_) => "D",
                Ok(m) => {
                    use std::os::unix::fs::MetadataExt;
                    if size < 0 || mtime < 0 {
                        let newer = match (m.modified().ok(), ds_time) {
                            (Some(f), Some(d)) => f > d,
                            _ => false,
                        };
                        if newer { "M" } else { "C" }
                    } else if m.len() as i64 != size as i64 || (mtime > 0 && m.mtime() as i32 != mtime) {
                        "M"
                    } else {
                        "C"
                    }
                }
            },
        };
        fold(map, top, dir, &name, code);
        tracked.insert(top.join(&name));
    }
    untracked(dir, &tracked, map);
    Some(())
}

/// Items right inside `dir` that the repository doesn't know: untracked.
fn untracked(dir: &Path, tracked: &std::collections::HashSet<PathBuf>, map: &mut HashMap<String, String>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name();
        if matches!(name.to_str(), Some(".hg" | ".svn" | ".fslckout" | "_FOSSIL_" | ".bzr")) {
            continue;
        }
        let k = p.to_string_lossy().into_owned();
        if map.contains_key(&k) {
            continue;
        }
        let known = tracked.contains(&p) || (p.is_dir() && tracked.iter().any(|t| t.starts_with(&p)));
        if !known {
            map.insert(k, "?".into());
        }
    }
}

// ---------------------------------------------------------------- Subversion

/// `.svn/wc.db` (SQLite): each node's newest layer; added/copied above the
/// base, deleted, or changed on disk since its recorded size and time.
fn svn(top: &Path, dir: &Path, map: &mut HashMap<String, String>) -> Option<()> {
    let db = rusqlite::Connection::open_with_flags(top.join(".svn/wc.db"), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let mut st = db
        .prepare(
            "SELECT local_relpath, kind, presence, op_depth, translated_size, last_mod_time FROM nodes n \
             WHERE op_depth = (SELECT MAX(op_depth) FROM nodes m WHERE m.wc_id = n.wc_id AND m.local_relpath = n.local_relpath)",
        )
        .ok()?;
    let rows = st
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, Option<i64>>(4)?, r.get::<_, Option<i64>>(5)?)))
        .ok()?;
    let mut tracked = std::collections::HashSet::new();
    for (rel, kind, presence, depth, size, mtime) in rows.flatten() {
        if rel.is_empty() {
            continue;
        }
        let p = top.join(&rel);
        let code = if presence == "base-deleted" || presence == "not-present" {
            "D"
        } else if depth > 0 {
            "A"
        } else if kind == "file" {
            match std::fs::symlink_metadata(&p) {
                Err(_) => "D",
                Ok(m) => {
                    use std::os::unix::fs::MetadataExt;
                    let us = m.mtime() * 1_000_000 + m.mtime_nsec() / 1000;
                    let changed = size.is_some_and(|s| s >= 0 && s != m.len() as i64) || mtime.is_some_and(|t| t != us);
                    if changed { "M" } else { "C" }
                }
            }
        } else {
            "C"
        };
        fold(map, top, dir, &rel, code);
        tracked.insert(p);
    }
    untracked(dir, &tracked, map);
    Some(())
}

// ---------------------------------------------------------------- Fossil

/// The checkout database (`.fslckout`, SQLite): vfile's changed, deleted
/// and added (no rid yet) flags, plus files touched since checkout.
fn fossil(top: &Path, dir: &Path, map: &mut HashMap<String, String>) -> Option<()> {
    let f = [".fslckout", "_FOSSIL_"].iter().map(|n| top.join(n)).find(|p| p.is_file())?;
    let db = rusqlite::Connection::open_with_flags(f, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let mut st = db.prepare("SELECT pathname, chnged, deleted, rid, mtime FROM vfile").ok()?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, Option<i64>>(4)?))).ok()?;
    let mut tracked = std::collections::HashSet::new();
    for (rel, chnged, deleted, rid, mtime) in rows.flatten() {
        let p = top.join(&rel);
        let code = if deleted != 0 {
            "D"
        } else if rid == 0 {
            "A"
        } else if chnged != 0 {
            "M"
        } else {
            use std::os::unix::fs::MetadataExt;
            match std::fs::symlink_metadata(&p) {
                Err(_) => "D",
                Ok(m) if mtime.is_some_and(|t| t != m.mtime()) => "M",
                Ok(_) => "C",
            }
        };
        fold(map, top, dir, &rel, code);
        tracked.insert(p);
    }
    untracked(dir, &tracked, map);
    Some(())
}

/// Every item's badge in `dir` (see the module comment).
pub fn status(dir: &str) -> Value {
    let orig = Path::new(dir);
    let d = orig.canonicalize().unwrap_or(orig.to_path_buf());
    let mut map: HashMap<String, String> = HashMap::new();
    let found = find_repo(&d);
    let mut kind = "";
    if let Some((top, k)) = &found {
        kind = k;
        let ok = match *k {
            // Jujutsu usually keeps a git repository alongside.
            "git" | "jj" => git(&d, &mut map),
            "hg" => hg(top, &d, &mut map),
            "svn" => svn(top, &d, &mut map),
            "fossil" => fossil(top, &d, &mut map),
            _ => None,
        };
        if ok.is_none() && *k == "jj" {
            map.clear();
        }
    }
    let mut out: HashMap<String, String> = map
        .into_iter()
        .map(|(k, v)| (orig.join(Path::new(&k).strip_prefix(&d).unwrap_or(Path::new(&k))).to_string_lossy().into_owned(), format!("{v}/{kind}")))
        .collect();
    // Repositories among the items: their own badge.
    if let Ok(rd) = std::fs::read_dir(&d) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if let Some(k) = repo_kind(&p) {
                    // git (and jj's git) folders: "@/git/BRANCH" (the views show it by the name).
                    let branch = if k == "git" || k == "jj" { git_branch(&p) } else { None };
                    let v = match branch { Some(b) if !b.is_empty() => format!("@/{k}/{b}"), _ => format!("@/{k}") };
                    out.insert(orig.join(e.file_name()).to_string_lossy().into_owned(), v);
                }
            }
        }
    }
    let mut v = json!(out);
    if found.is_some() {
        v["__repo"] = json!("1");
        v["__vcs"] = json!(kind);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_branches() {
        let d = std::env::temp_dir().join(format!("grove-vcs-branch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("a/.git")).unwrap();
        std::fs::write(d.join("a/.git/HEAD"), "ref: refs/heads/feature/x\n").unwrap();
        std::fs::create_dir_all(d.join("wt")).unwrap();
        std::fs::write(d.join("wt/.git"), "gitdir: ../a/.git\n").unwrap();
        assert_eq!(git_branch(&d.join("a")).as_deref(), Some("feature/x"));
        assert_eq!(git_branch(&d.join("wt")).as_deref(), Some("feature/x"));
        std::fs::write(d.join("a/.git/HEAD"), "0123456789abcdef\n").unwrap();
        assert_eq!(git_branch(&d.join("a")).as_deref(), Some("0123456"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn folds_strongest() {
        let mut m = HashMap::new();
        for (rel, c) in [("src/a.rs", "C"), ("src/b.rs", "M"), ("x.txt", "C"), ("build/", "I"), ("docs/a/b.md", "I")] {
            fold(&mut m, Path::new("/r"), Path::new("/r"), rel, c);
        }
        assert_eq!(m["/r/src"], "M");
        assert_eq!(m["/r/x.txt"], "C");
        assert_eq!(m["/r/build"], "I");
        assert_eq!(m["/r/docs"], "C");
    }
}
