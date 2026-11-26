//! The Git Changes window's side (libgit2, no git command): a repository's
//! branch and its upstream, its branches, and its files by state (staged,
//! changed but not staged, untracked, conflicted, and ignored on request);
//! staging, unstaging and discarding, switching and making branches,
//! committing what's staged, fetching; and one file's diff as a patch file
//! for the delta-style diff view. Paths are relative to the repository's top.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn e(x: git2::Error) -> String {
    x.message().to_string()
}

fn open(dir: &str) -> Result<git2::Repository, String> {
    git2::Repository::discover(dir).map_err(|_| "This folder isn't in a git repository.".to_string())
}

fn top(repo: &git2::Repository) -> PathBuf {
    repo.workdir().map(Path::to_path_buf).unwrap_or_else(|| repo.path().to_path_buf())
}

/// What a status means for each list, as a letter: A added, M modified,
/// D deleted, R renamed, T type changed.
fn index_code(s: git2::Status) -> Option<&'static str> {
    use git2::Status as S;
    if s.contains(S::INDEX_NEW) {
        Some("A")
    } else if s.contains(S::INDEX_MODIFIED) {
        Some("M")
    } else if s.contains(S::INDEX_DELETED) {
        Some("D")
    } else if s.contains(S::INDEX_RENAMED) {
        Some("R")
    } else if s.contains(S::INDEX_TYPECHANGE) {
        Some("T")
    } else {
        None
    }
}

fn work_code(s: git2::Status) -> Option<&'static str> {
    use git2::Status as S;
    if s.contains(S::WT_MODIFIED) {
        Some("M")
    } else if s.contains(S::WT_DELETED) {
        Some("D")
    } else if s.contains(S::WT_RENAMED) {
        Some("R")
    } else if s.contains(S::WT_TYPECHANGE) {
        Some("T")
    } else {
        None
    }
}

fn state_word(s: git2::RepositoryState) -> &'static str {
    use git2::RepositoryState as R;
    match s {
        R::Clean => "",
        R::Merge => "A merge is in progress.",
        R::Revert | R::RevertSequence => "A revert is in progress.",
        R::CherryPick | R::CherryPickSequence => "A cherry-pick is in progress.",
        R::Bisect => "A bisect is in progress.",
        R::Rebase | R::RebaseInteractive | R::RebaseMerge => "A rebase is in progress.",
        R::ApplyMailbox | R::ApplyMailboxOrRebase => "Applying patches (git am) is in progress.",
    }
}

/// The repository at a glance (`ignored`: list ignored files and folders too).
pub fn overview(dir: &str, ignored: bool) -> Value {
    let repo = match open(dir) {
        Ok(r) => r,
        Err(err) => return json!({"error": err}),
    };
    if repo.is_bare() {
        return json!({"error": "This is a bare repository: it has no files to show."});
    }
    let root = top(&repo);
    // Branch, upstream, ahead/behind.
    let head = repo.head().ok();
    let unborn = head.is_none();
    let detached = repo.head_detached().unwrap_or(false);
    let branch = match &head {
        Some(h) if h.is_branch() => h.shorthand().unwrap_or("").to_string(),
        Some(h) => h.target().map(|t| t.to_string()[..7].to_string()).unwrap_or_default(),
        // A new repository: HEAD names a branch with no commits yet.
        None => repo.find_reference("HEAD").ok().and_then(|r| r.symbolic_target().ok().flatten().map(|t| t.trim_start_matches("refs/heads/").to_string())).unwrap_or_default(),
    };
    let (mut upstream, mut ahead, mut behind) = (String::new(), 0usize, 0usize);
    if let Some(h) = head.as_ref().filter(|h| h.is_branch()) {
        if let Ok(b) = repo.find_branch(&branch, git2::BranchType::Local) {
            if let Ok(u) = b.upstream() {
                upstream = u.name().ok().flatten().unwrap_or("").to_string();
                if let (Some(l), Some(r)) = (h.target(), u.get().target()) {
                    if let Ok((a, b)) = repo.graph_ahead_behind(l, r) {
                        (ahead, behind) = (a, b);
                    }
                }
            }
        }
    }
    // Branches: the newest first.
    let mut locals: Vec<(i64, Value)> = vec![];
    let mut remotes: Vec<(i64, Value)> = vec![];
    if let Ok(bs) = repo.branches(None) {
        for (b, kind) in bs.flatten() {
            let Ok(Some(name)) = b.name() else { continue };
            if name.ends_with("/HEAD") {
                continue;
            }
            let c = b.get().peel_to_commit().ok();
            let when = c.as_ref().map(|c| c.time().seconds()).unwrap_or(0);
            let v = json!({
                "name": name,
                "current": kind == git2::BranchType::Local && b.is_head(),
                "summary": c.as_ref().and_then(|c| c.summary().ok().flatten().map(str::to_string)).unwrap_or_default(),
                "when": when,
            });
            if kind == git2::BranchType::Local { locals.push((when, v)) } else { remotes.push((when, v)) }
        }
    }
    locals.sort_by(|a, b| b.0.cmp(&a.0));
    remotes.sort_by(|a, b| b.0.cmp(&a.0));
    // Files.
    let mut o = git2::StatusOptions::new();
    o.include_untracked(true)
        .recurse_untracked_dirs(false)
        .include_ignored(ignored)
        .recurse_ignored_dirs(false)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);
    let (mut staged, mut changed, mut untracked, mut conflicts, mut ignored_list) = (vec![], vec![], vec![], vec![], vec![]);
    match repo.statuses(Some(&mut o)) {
        Ok(st) => {
            for x in st.iter() {
                let s = x.status();
                let path = x.path().unwrap_or("").to_string();
                if s.contains(git2::Status::CONFLICTED) {
                    conflicts.push(json!({"path": path, "code": "U"}));
                    continue;
                }
                if s.contains(git2::Status::IGNORED) {
                    ignored_list.push(json!({"path": path, "code": "I"}));
                    continue;
                }
                if let Some(c) = index_code(s) {
                    let old = x.head_to_index().and_then(|d| d.old_file().path().map(|p| p.to_string_lossy().into_owned())).filter(|o| *o != path);
                    let shown = x.head_to_index().and_then(|d| d.new_file().path().map(|p| p.to_string_lossy().into_owned())).unwrap_or(path.clone());
                    staged.push(json!({"path": shown, "code": c, "old": old}));
                }
                if s.contains(git2::Status::WT_NEW) {
                    untracked.push(json!({"path": path, "code": "?"}));
                } else if let Some(c) = work_code(s) {
                    let shown = x.index_to_workdir().and_then(|d| d.new_file().path().map(|p| p.to_string_lossy().into_owned())).unwrap_or(path.clone());
                    changed.push(json!({"path": shown, "code": c}));
                }
            }
        }
        Err(err) => return json!({"error": e(err)}),
    }
    json!({
        "root": root.to_string_lossy().trim_end_matches('/'),
        "branch": branch,
        "detached": detached,
        "unborn": unborn,
        "upstream": upstream,
        "ahead": ahead,
        "behind": behind,
        "state": state_word(repo.state()),
        "branches": locals.into_iter().map(|x| x.1).collect::<Vec<_>>(),
        "remotes": remotes.into_iter().map(|x| x.1).collect::<Vec<_>>(),
        "staged": staged,
        "changed": changed,
        "untracked": untracked,
        "conflicts": conflicts,
        "ignored": ignored_list,
    })
}

/// Just the branch name and how many files changed (the path bar's chip),
/// quickly: no ignored files, no branch list.
pub fn summary(dir: &str) -> Value {
    let Ok(repo) = git2::Repository::discover(dir) else { return json!({}) };
    if repo.is_bare() {
        return json!({});
    }
    let head = repo.head().ok();
    let branch = match &head {
        Some(h) if h.is_branch() => h.shorthand().unwrap_or("").to_string(),
        Some(h) => h.target().map(|t| t.to_string()[..7].to_string()).unwrap_or_default(),
        None => repo.find_reference("HEAD").ok().and_then(|r| r.symbolic_target().ok().flatten().map(|t| t.trim_start_matches("refs/heads/").to_string())).unwrap_or_default(),
    };
    let mut o = git2::StatusOptions::new();
    o.include_untracked(true).recurse_untracked_dirs(false).include_ignored(false);
    let changes = repo.statuses(Some(&mut o)).map(|s| s.len()).unwrap_or(0);
    json!({"root": top(&repo).to_string_lossy().trim_end_matches('/'), "branch": branch, "detached": repo.head_detached().unwrap_or(false), "changes": changes})
}

fn head_commit(repo: &git2::Repository) -> Option<git2::Commit<'_>> {
    repo.head().ok().and_then(|h| h.peel_to_commit().ok())
}

fn unstage(repo: &git2::Repository, rels: &[String]) -> Result<String, String> {
    match head_commit(repo) {
        Some(c) => {
            let specs: Vec<&str> = if rels.is_empty() { vec!["."] } else { rels.iter().map(String::as_str).collect() };
            repo.reset_default(Some(c.as_object()), specs.iter()).map_err(e)?;
        }
        // No commits yet: unstaging is taking them out of the index.
        None => {
            let mut idx = repo.index().map_err(e)?;
            if rels.is_empty() {
                idx.clear().map_err(e)?;
            } else {
                for r in rels {
                    let _ = idx.remove_path(Path::new(r));
                    let _ = idx.remove_dir(Path::new(r), 0);
                }
            }
            idx.write().map_err(e)?;
        }
    }
    Ok(format!("Unstaged {}.", count(rels)))
}

fn count(rels: &[String]) -> String {
    match rels.len() {
        0 => "everything".into(),
        1 => rels[0].clone(),
        n => format!("{n} files"),
    }
}

/// Throw away changes not staged: tracked files go back to what's staged
/// (or committed); untracked ones go to the Trash.
fn discard(repo: &git2::Repository, rels: &[String]) -> Result<String, String> {
    let root = top(repo);
    let mut tracked = vec![];
    let mut trashed = 0;
    for r in rels {
        let st = repo.status_file(Path::new(r.trim_end_matches('/'))).unwrap_or(git2::Status::WT_NEW);
        if st.contains(git2::Status::WT_NEW) || r.ends_with('/') {
            grove_core::trash::trash(&root.join(r.trim_end_matches('/')))?;
            trashed += 1;
        } else {
            tracked.push(r.clone());
        }
    }
    if !tracked.is_empty() {
        let mut co = git2::build::CheckoutBuilder::new();
        co.force();
        for r in &tracked {
            co.path(r);
        }
        repo.checkout_index(None, Some(&mut co)).map_err(e)?;
    }
    Ok(match (tracked.len(), trashed) {
        (0, n) => format!("Moved {n} untracked item{} to the Trash.", if n == 1 { "" } else { "s" }),
        (t, 0) => format!("Put back {} as staged/committed.", if t == 1 { tracked[0].clone() } else { format!("{t} files") }),
        (t, n) => format!("Put back {t} file{} and moved {n} untracked item{} to the Trash.", if t == 1 { "" } else { "s" }, if n == 1 { "" } else { "s" }),
    })
}

fn switch(repo: &git2::Repository, name: &str) -> Result<String, String> {
    // A remote branch ("origin/feature"): a local one following it.
    let local = match repo.find_branch(name, git2::BranchType::Local) {
        Ok(b) => b,
        Err(_) => {
            let rb = repo.find_branch(name, git2::BranchType::Remote).map_err(|_| format!("There's no branch “{name}”."))?;
            let short = name.split_once('/').map(|x| x.1).unwrap_or(name);
            if let Ok(b) = repo.find_branch(short, git2::BranchType::Local) {
                b
            } else {
                let c = rb.get().peel_to_commit().map_err(e)?;
                let mut b = repo.branch(short, &c, false).map_err(e)?;
                b.set_upstream(Some(name)).map_err(e)?;
                b
            }
        }
    };
    let refname = local.get().name().map_err(|_| "That branch has a name git can't read.".to_string())?.to_string();
    let target = local.get().peel(git2::ObjectType::Commit).map_err(e)?;
    // Safe: refuses to overwrite changes that the switch would touch.
    let mut co = git2::build::CheckoutBuilder::new();
    co.safe();
    repo.checkout_tree(&target, Some(&mut co)).map_err(|x| {
        if x.code() == git2::ErrorCode::Conflict {
            "Your changes to some files would be lost by switching: commit or stash them first.".to_string()
        } else {
            e(x)
        }
    })?;
    repo.set_head(&refname).map_err(e)?;
    Ok(format!("Switched to {}.", refname.trim_start_matches("refs/heads/")))
}

fn new_branch(repo: &git2::Repository, name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || !git2::Branch::name_is_valid(name).unwrap_or(false) {
        return Err(format!("“{name}” can't be a branch name (no spaces, ~ ^ : ? * [ or ..)."));
    }
    let c = head_commit(repo).ok_or("Make a first commit before branching.")?;
    repo.branch(name, &c, false).map_err(|x| if x.code() == git2::ErrorCode::Exists { format!("There's already a branch “{name}”.") } else { e(x) })?;
    switch(repo, name)
}

/// Commit what's staged, as it is (`amend`: replace the last commit).
fn commit_staged(repo: &git2::Repository, msg: &str, amend: bool) -> Result<String, String> {
    if msg.trim().is_empty() {
        return Err("A commit needs a message describing the change.".into());
    }
    let sig = repo.signature().map_err(|_| "git doesn't know who you are: set user.name and user.email.".to_string())?;
    let mut idx = repo.index().map_err(e)?;
    if idx.has_conflicts() {
        return Err("Some files still have conflicts: resolve them and stage them first.".into());
    }
    let tree = repo.find_tree(idx.write_tree().map_err(e)?).map_err(e)?;
    let parent = head_commit(repo);
    let id = if amend {
        let p = parent.ok_or("There's no commit to amend yet.")?;
        p.amend(Some("HEAD"), None, Some(&sig), None, Some(msg), Some(&tree)).map_err(e)?
    } else {
        if parent.as_ref().is_some_and(|p| p.tree_id() == tree.id()) {
            return Err("Nothing is staged: stage some changes first.".into());
        }
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &parents).map_err(e)?
    };
    Ok(format!("[{}] {}", &id.to_string()[..7], msg.lines().next().unwrap_or("")))
}

fn fetch(repo: &git2::Repository) -> Result<String, String> {
    let mut cfg = repo.config().map_err(e)?;
    let names: Vec<String> = repo.remotes().map_err(e)?.iter().filter_map(|n| n.ok().flatten().map(str::to_string)).collect();
    if names.is_empty() {
        return Err("This repository has no remotes to fetch from.".into());
    }
    for n in &names {
        let mut remote = repo.find_remote(n).map_err(e)?;
        let tried = std::cell::Cell::new(0);
        let mut fo = git2::FetchOptions::new();
        fo.remote_callbacks(crate::gitstatus::callbacks(cfg.snapshot().map_err(e)?, &tried));
        let specs: Vec<String> = Vec::new();
        remote.fetch(&specs, Some(&mut fo), None).map_err(e)?;
    }
    Ok(format!("Fetched {}.", names.join(", ")))
}

/// stage, unstage, discard, switch (arg: branch), newbranch (arg: name),
/// commit (arg: {"message", "amend"}), fetch; pull and push go to gitstatus.
pub fn change(dir: &str, action: &str, rels: &[String], arg: &str) -> Value {
    if action == "pull" || action == "push" {
        return crate::gitstatus::action(dir, action, &[], "", true);
    }
    let repo = match open(dir) {
        Ok(r) => r,
        Err(err) => return json!({"error": err}),
    };
    let r = match action {
        "stage" => crate::gitstatus::add_paths(&repo, rels).map(|_| format!("Staged {}.", count(rels))).map_err(e),
        "unstage" => unstage(&repo, rels),
        "discard" => discard(&repo, rels),
        "switch" => switch(&repo, arg),
        "newbranch" => new_branch(&repo, arg),
        "commit" => {
            let a: Value = serde_json::from_str(arg).unwrap_or(json!({}));
            commit_staged(&repo, a["message"].as_str().unwrap_or(""), a["amend"].as_bool().unwrap_or(false))
        }
        "lastmessage" => return json!({"message": head_commit(&repo).and_then(|c| c.message().ok().map(str::to_string)).unwrap_or_default()}),
        "fetch" => fetch(&repo),
        _ => Err(format!("Unknown git action “{action}”.")),
    };
    match r {
        Ok(out) => json!({"ok": true, "output": out}),
        Err(err) => json!({"error": err}),
    }
}

/// One file's (or folder's) changes as a patch: `which` "staged" (HEAD to
/// the index), "changed" (index to the files), "untracked" (all of it,
/// added), "conflicts" (the file with its conflict markers, against HEAD).
pub fn diff_text(dir: &str, rel: &str, which: &str) -> Result<String, String> {
    let repo = open(dir)?;
    let mut o = git2::DiffOptions::new();
    o.pathspec(rel.trim_end_matches('/')).include_untracked(true).recurse_untracked_dirs(true).show_untracked_content(true).context_lines(3);
    let d = match which {
        "staged" => {
            let head = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
            repo.diff_tree_to_index(head.as_ref(), None, Some(&mut o)).map_err(e)?
        }
        "conflicts" => {
            let head = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
            repo.diff_tree_to_workdir(head.as_ref(), Some(&mut o)).map_err(e)?
        }
        _ => repo.diff_index_to_workdir(None, Some(&mut o)).map_err(e)?,
    };
    let mut text = String::new();
    d.print(git2::DiffFormat::Patch, |_, _, line| {
        let c = line.origin();
        if matches!(c, '+' | '-' | ' ') {
            text.push(c);
        }
        text.push_str(&String::from_utf8_lossy(line.content()));
        true
    })
    .map_err(e)?;
    Ok(text)
}

/// The diff as a .diff file for the viewer (in the runtime folder, one per
/// file and list, replaced each time).
pub fn diff_file(dir: &str, rel: &str, which: &str) -> Value {
    use std::hash::{Hash, Hasher};
    let text = match diff_text(dir, rel, which) {
        Ok(t) => t,
        Err(err) => return json!({"error": err}),
    };
    if text.trim().is_empty() {
        return json!({"empty": true});
    }
    let base = std::env::var("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir()).join("grove").join("diffs");
    if std::fs::create_dir_all(&base).is_err() {
        return json!({"error": "Couldn't make a place for the diff."});
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (dir, rel, which).hash(&mut h);
    // (A new name each time, so the viewer reloads it.)
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let path = base.join(format!("{:016x}-{n}.diff", h.finish()));
    // Old diffs of this file go.
    if let Ok(rd) = std::fs::read_dir(&base) {
        let prefix = format!("{:016x}-", h.finish());
        for x in rd.flatten() {
            if x.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_file(x.path());
            }
        }
    }
    match std::fs::write(&path, text) {
        Ok(()) => json!({"path": path.to_string_lossy()}),
        Err(err) => json!({"error": err.to_string()}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git").args(args).current_dir(dir).env("GIT_CONFIG_GLOBAL", "/dev/null").env("GIT_CONFIG_SYSTEM", "/dev/null").status().map(|s| s.success()).unwrap_or(false);
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn stages_switches_and_commits() {
        let d = std::env::temp_dir().join(format!("grove-gitrepo-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        if std::process::Command::new("git").arg("--version").output().is_err() {
            return;
        }
        run(&d, &["init", "-q", "-b", "main"]);
        run(&d, &["config", "user.name", "T"]);
        run(&d, &["config", "user.email", "t@example.com"]);
        std::fs::write(d.join("a.txt"), "one\ntwo\n").unwrap();
        std::fs::write(d.join(".gitignore"), "build/\n").unwrap();
        run(&d, &["add", "."]);
        run(&d, &["commit", "-q", "-m", "first"]);
        std::fs::write(d.join("a.txt"), "one\nTWO\n").unwrap();
        std::fs::write(d.join("new.txt"), "hi\n").unwrap();
        std::fs::create_dir_all(d.join("build")).unwrap();
        std::fs::write(d.join("build/x.o"), "x").unwrap();
        let dir = d.to_str().unwrap();
        let o = overview(dir, true);
        assert_eq!(o["branch"], "main");
        assert_eq!(o["changed"][0]["path"], "a.txt");
        assert_eq!(o["untracked"][0]["path"], "new.txt");
        assert_eq!(o["ignored"][0]["path"], "build/");
        assert!(diff_text(dir, "a.txt", "changed").unwrap().contains("+TWO"));
        // Stage, then it's staged and not changed.
        assert_eq!(change(dir, "stage", &["a.txt".into()], "")["ok"], true);
        let o = overview(dir, false);
        assert_eq!((o["staged"][0]["path"].as_str(), o["staged"][0]["code"].as_str()), (Some("a.txt"), Some("M")));
        assert!(o["changed"].as_array().unwrap().is_empty());
        assert!(diff_text(dir, "a.txt", "staged").unwrap().contains("-two"));
        // Unstage and stage again; commit what's staged only.
        change(dir, "unstage", &["a.txt".into()], "");
        assert!(overview(dir, false)["staged"].as_array().unwrap().is_empty());
        change(dir, "stage", &["a.txt".into()], "");
        let r = change(dir, "commit", &[], &json!({"message": "second"}).to_string());
        assert_eq!(r["ok"], true, "{r}");
        let o = overview(dir, false);
        assert!(o["staged"].as_array().unwrap().is_empty());
        assert_eq!(o["untracked"][0]["path"], "new.txt");
        // A new branch, then back.
        assert_eq!(change(dir, "newbranch", &[], "feature")["ok"], true);
        assert_eq!(overview(dir, false)["branch"], "feature");
        assert_eq!(change(dir, "switch", &[], "main")["ok"], true);
        assert_eq!(overview(dir, false)["branches"].as_array().unwrap().len(), 2);
        // Discarding a change puts the file back.
        std::fs::write(d.join("a.txt"), "changed again\n").unwrap();
        change(dir, "discard", &["a.txt".into()], "");
        assert_eq!(std::fs::read_to_string(d.join("a.txt")).unwrap(), "one\nTWO\n");
        let _ = std::fs::remove_dir_all(&d);
    }
}
