//! Dolphin's git menu through libgit2 (no git command), in the background:
//! add, commit, log, diff, pull, push. (Status badges: vcs.rs.)

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Credentials for fetch and push: ssh-agent, then the usual key files,
/// then git's credential helpers (https), then nothing.
pub(crate) fn callbacks<'a>(config: git2::Config, tried: &'a std::cell::Cell<u32>) -> git2::RemoteCallbacks<'a> {
    let mut cb = git2::RemoteCallbacks::new();
    cb.credentials(move |url, user, allowed| {
        let n = tried.get();
        tried.set(n + 1);
        if n > 6 {
            return Err(git2::Error::from_str("No credentials worked (ssh-agent, ~/.ssh keys, credential helpers)."));
        }
        let user = user.unwrap_or("git");
        if allowed.contains(git2::CredentialType::SSH_KEY) {
            if n == 0 {
                return git2::Cred::ssh_key_from_agent(user);
            }
            let home = grove_core::util::home();
            for k in ["id_ed25519", "id_ecdsa", "id_rsa"] {
                let key = home.join(".ssh").join(k);
                if key.is_file() && n as usize <= 3 {
                    return git2::Cred::ssh_key(user, None, &key, None);
                }
            }
        }
        if allowed.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
            return git2::Cred::credential_helper(&config, url, Some(user).filter(|u| *u != "git"));
        }
        if allowed.contains(git2::CredentialType::USERNAME) {
            return git2::Cred::username(user);
        }
        git2::Cred::default()
    });
    cb
}

fn workdir_rel(repo: &git2::Repository, dir: &str, p: &Path) -> String {
    let top = repo.workdir().map(Path::to_path_buf).unwrap_or_default();
    let abs = if p.is_absolute() { p.to_path_buf() } else { Path::new(dir).join(p) };
    let abs = abs.canonicalize().unwrap_or(abs);
    let top = top.canonicalize().unwrap_or(top);
    abs.strip_prefix(&top).map(|r| r.to_string_lossy().into_owned()).unwrap_or_else(|_| abs.to_string_lossy().into_owned())
}

pub(crate) fn add_paths(repo: &git2::Repository, rels: &[String]) -> Result<(), git2::Error> {
    let mut idx = repo.index()?;
    let specs: Vec<&str> = if rels.is_empty() { vec!["."] } else { rels.iter().map(String::as_str).collect() };
    idx.add_all(specs.iter(), git2::IndexAddOption::DEFAULT, None)?;
    // Deleted files are staged as deletions (as `git add` does).
    idx.update_all(specs.iter(), None)?;
    idx.write()
}

fn current_branch(repo: &git2::Repository) -> Result<String, String> {
    let head = repo.head().map_err(|e| e.message().to_string())?;
    if !head.is_branch() {
        return Err("Not on a branch (detached HEAD).".into());
    }
    Ok(head.shorthand().unwrap_or("").to_string())
}

fn pull(repo: &git2::Repository) -> Result<String, String> {
    let branch = current_branch(repo)?;
    let mut cfg = repo.config().map_err(|e| e.message().to_string())?;
    let remote_name = cfg.get_string(&format!("branch.{branch}.remote")).unwrap_or_else(|_| "origin".into());
    let merge = cfg.get_string(&format!("branch.{branch}.merge")).unwrap_or_else(|_| format!("refs/heads/{branch}"));
    let mut remote = repo.find_remote(&remote_name).map_err(|e| e.message().to_string())?;
    let tried = std::cell::Cell::new(0);
    let mut fo = git2::FetchOptions::new();
    fo.remote_callbacks(callbacks(cfg.snapshot().map_err(|e| e.message().to_string())?, &tried));
    remote.fetch(&[merge.as_str()], Some(&mut fo), None).map_err(|e| e.message().to_string())?;
    let fetch_head = repo.find_reference("FETCH_HEAD").map_err(|e| e.message().to_string())?;
    let theirs = repo.reference_to_annotated_commit(&fetch_head).map_err(|e| e.message().to_string())?;
    let (analysis, _) = repo.merge_analysis(&[&theirs]).map_err(|e| e.message().to_string())?;
    if analysis.is_up_to_date() {
        return Ok("Already up to date.".into());
    }
    if !analysis.is_fast_forward() {
        return Err("The branch and its remote have both changed: it can't be fast-forwarded (merge or rebase in a terminal).".into());
    }
    let refname = format!("refs/heads/{branch}");
    let mut r = repo.find_reference(&refname).map_err(|e| e.message().to_string())?;
    r.set_target(theirs.id(), "pull: fast-forward").map_err(|e| e.message().to_string())?;
    repo.set_head(&refname).map_err(|e| e.message().to_string())?;
    repo.checkout_head(Some(git2::build::CheckoutBuilder::default().safe())).map_err(|e| e.message().to_string())?;
    Ok(format!("Fast-forwarded {branch} to {}.", &theirs.id().to_string()[..7]))
}

fn push(repo: &git2::Repository) -> Result<String, String> {
    let branch = current_branch(repo)?;
    let mut cfg = repo.config().map_err(|e| e.message().to_string())?;
    let remote_name = cfg.get_string(&format!("branch.{branch}.pushRemote")).or_else(|_| cfg.get_string(&format!("branch.{branch}.remote"))).unwrap_or_else(|_| "origin".into());
    let merge = cfg.get_string(&format!("branch.{branch}.merge")).unwrap_or_else(|_| format!("refs/heads/{branch}"));
    let mut remote = repo.find_remote(&remote_name).map_err(|e| e.message().to_string())?;
    let tried = std::cell::Cell::new(0);
    let rejected = std::cell::RefCell::new(None::<String>);
    let mut cb = callbacks(cfg.snapshot().map_err(|e| e.message().to_string())?, &tried);
    cb.push_update_reference(|r, status| {
        if let Some(s) = status {
            *rejected.borrow_mut() = Some(format!("{r}: {s}"));
        }
        Ok(())
    });
    let mut po = git2::PushOptions::new();
    po.remote_callbacks(cb);
    remote.push(&[format!("refs/heads/{branch}:{merge}")], Some(&mut po)).map_err(|e| e.message().to_string())?;
    drop(po);
    if let Some(r) = rejected.into_inner() {
        return Err(format!("The remote refused the push ({r}). Pull first."));
    }
    Ok(format!("Pushed {branch} to {remote_name}."))
}

/// Commit: everything tracked (`rels` empty, `git commit -a`), or just the
/// paths named as they are now (`git commit -- paths`), leaving anything
/// else that's staged staged.
fn commit(repo: &git2::Repository, rels: &[String], msg: &str) -> Result<String, String> {
    let e = |x: git2::Error| x.message().to_string();
    let sig = repo.signature().map_err(|_| "git doesn't know who you are: set user.name and user.email.".to_string())?;
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let tree_id = if rels.is_empty() {
        let mut idx = repo.index().map_err(e)?;
        idx.update_all(["."].iter(), None).map_err(e)?;
        idx.write().map_err(e)?;
        idx.write_tree().map_err(e)?
    } else {
        // Stage the paths in the real index (so status agrees), and build
        // the commit's tree from HEAD plus only those paths.
        add_paths(repo, rels).map_err(e)?;
        let real = repo.index().map_err(e)?;
        let mut tmp = git2::Index::new().map_err(e)?;
        if let Some(p) = &parent {
            tmp.read_tree(&p.tree().map_err(e)?).map_err(e)?;
        }
        let inside = |p: &[u8]| {
            let s = String::from_utf8_lossy(p);
            rels.iter().any(|r| r.is_empty() || s == r.as_str() || s.starts_with(&format!("{}/", r.trim_end_matches('/'))))
        };
        let gone: Vec<Vec<u8>> = tmp.iter().filter(|x| inside(&x.path)).map(|x| x.path.clone()).collect();
        for g in gone {
            tmp.remove_path(Path::new(&*String::from_utf8_lossy(&g))).map_err(e)?;
        }
        for x in real.iter().filter(|x| inside(&x.path)) {
            tmp.add(&x).map_err(e)?;
        }
        tmp.write_tree_to(repo).map_err(e)?
    };
    let tree = repo.find_tree(tree_id).map_err(e)?;
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    if let Some(p) = &parent {
        if p.tree_id() == tree_id {
            return Err("Nothing to commit: those files haven't changed.".into());
        }
    }
    let id = repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &parents).map_err(e)?;
    Ok(format!("[{}] {}", &id.to_string()[..7], msg.lines().next().unwrap_or("")))
}

fn log(repo: &git2::Repository, rels: &[String]) -> Result<String, String> {
    let e = |x: git2::Error| x.message().to_string();
    let mut walk = repo.revwalk().map_err(e)?;
    walk.push_head().map_err(e)?;
    walk.set_sorting(git2::Sort::TIME).map_err(e)?;
    let mut out = Vec::new();
    for (scanned, oid) in walk.enumerate() {
        if out.len() >= 300 || scanned > 20_000 {
            break;
        }
        let c = repo.find_commit(oid.map_err(e)?).map_err(e)?;
        if !rels.is_empty() {
            // Only commits that changed the paths.
            let tree = c.tree().map_err(e)?;
            let ptree = c.parent(0).ok().and_then(|p| p.tree().ok());
            let mut o = git2::DiffOptions::new();
            for r in rels {
                o.pathspec(r);
            }
            let d = repo.diff_tree_to_tree(ptree.as_ref(), Some(&tree), Some(&mut o)).map_err(e)?;
            if d.deltas().len() == 0 {
                continue;
            }
        }
        let when = grove_core::util::local_timestamp(c.time().seconds());
        out.push(format!("{}  {}  {}  {}", &c.id().to_string()[..7], &when[..10], c.author().name().unwrap_or(""), c.summary().ok().flatten().unwrap_or("")));
    }
    Ok(out.join("\n"))
}

fn diff(repo: &git2::Repository, rels: &[String]) -> Result<String, String> {
    let e = |x: git2::Error| x.message().to_string();
    let head = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
    let mut o = git2::DiffOptions::new();
    for r in rels {
        o.pathspec(r);
    }
    let d = repo.diff_tree_to_workdir_with_index(head.as_ref(), Some(&mut o)).map_err(e)?;
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

/// Dolphin's git menu: pull, push, commit, add, log, diff (libgit2).
pub fn action(dir: &str, action: &str, paths: &[PathBuf], arg: &str, dark: bool) -> Value {
    let repo = match git2::Repository::discover(dir) {
        Ok(r) => r,
        Err(_) => return json!({"error": "This folder isn't in a git repository."}),
    };
    let rels: Vec<String> = paths.iter().map(|p| workdir_rel(&repo, dir, p)).collect();
    let r = match action {
        "pull" => pull(&repo),
        "push" => push(&repo),
        "add" => add_paths(&repo, &rels).map(|_| format!("Added {} item{}.", rels.len(), if rels.len() == 1 { "" } else { "s" })).map_err(|e| e.message().to_string()),
        "commit" => {
            if arg.trim().is_empty() {
                return json!({"error": "A commit needs a message describing the change."});
            }
            commit(&repo, &rels, arg)
        }
        "log" => log(&repo, &rels),
        "diff" => {
            return match diff(&repo, &rels) {
                Ok(t) => json!({"ok": true, "output": t, "html": crate::preview::highlight(&t, "x.diff", dark)}),
                Err(e) => json!({"error": e}),
            };
        }
        _ => return json!({"error": format!("Unknown git action “{action}”.")}),
    };
    match r {
        Ok(out) => json!({"ok": true, "output": out}),
        Err(e) => json!({"error": e}),
    }
}
