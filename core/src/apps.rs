//! Applications: freedesktop desktop entries, which app opens which type
//! (mimeapps.list, read and written), and launching them.

use crate::util;
use serde::Serialize;
use std::collections::HashMap;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, PartialEq, Default)]
pub struct App {
    /// Desktop file id ("org.kde.konsole.desktop").
    pub id: String,
    pub name: String,
    pub generic_name: String,
    pub comment: String,
    pub icon: String,
    pub exec: String,
    pub terminal: bool,
    pub no_display: bool,
    pub hidden: bool,
    pub mime_types: Vec<String>,
    pub categories: Vec<String>,
    pub path: String,
    #[serde(skip)]
    pub file: PathBuf,
    /// Actions ([Desktop Action x]) as (name, exec).
    #[serde(skip)]
    pub actions: Vec<(String, String)>,
}

/// Split an Exec value into arguments (desktop entry spec quoting).
pub fn split_exec(exec: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut have = false;
    let mut it = exec.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' => {
                in_q = !in_q;
                have = true;
            }
            '\\' if in_q => {
                if let Some(n) = it.next() {
                    cur.push(n);
                }
            }
            c if c.is_whitespace() && !in_q => {
                if have || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    have = false;
                }
            }
            c => cur.push(c),
        }
    }
    if have || !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn unescape_value(v: &str) -> String {
    let mut s = String::new();
    let mut it = v.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('s') => s.push(' '),
                Some('n') => s.push('\n'),
                Some('t') => s.push('\t'),
                Some('r') => s.push('\r'),
                Some('\\') => s.push('\\'),
                Some(o) => {
                    s.push('\\');
                    s.push(o)
                }
                None => {}
            }
        } else {
            s.push(c);
        }
    }
    s
}

fn locale_keys() -> Vec<String> {
    let lang = std::env::var("LC_ALL").or_else(|_| std::env::var("LC_MESSAGES")).or_else(|_| std::env::var("LANG")).unwrap_or_default();
    let base = lang.split('.').next().unwrap_or("").to_string();
    let mut v = vec![];
    if !base.is_empty() && base != "C" && base != "POSIX" {
        v.push(base.clone());
        if let Some((l, _)) = base.split_once('_') {
            v.push(l.to_string());
        }
    }
    v
}

pub fn parse_desktop(id: &str, file: &Path, text: &str) -> Option<App> {
    let locales = locale_keys();
    let mut app = App { id: id.to_string(), file: file.to_path_buf(), ..Default::default() };
    let mut group = String::new();
    let mut localized: HashMap<String, (usize, String)> = HashMap::new();
    let mut action_names: HashMap<String, String> = HashMap::new();
    let mut action_execs: HashMap<String, String> = HashMap::new();
    let mut is_app = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            group = line[1..line.len() - 1].to_string();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim());
        if let Some(act) = group.strip_prefix("Desktop Action ") {
            match k {
                "Name" => {
                    action_names.insert(act.to_string(), unescape_value(v));
                }
                "Exec" => {
                    action_execs.insert(act.to_string(), unescape_value(v));
                }
                _ => {}
            }
            continue;
        }
        if group != "Desktop Entry" {
            continue;
        }
        let (key, loc) = match k.find('[') {
            Some(i) if k.ends_with(']') => (&k[..i], Some(&k[i + 1..k.len() - 1])),
            _ => (k, None),
        };
        if let Some(l) = loc {
            if let Some(rank) = locales.iter().position(|x| x == l) {
                let e = localized.entry(key.to_string()).or_insert((usize::MAX, String::new()));
                if rank < e.0 {
                    *e = (rank, unescape_value(v));
                }
            }
            continue;
        }
        let val = unescape_value(v);
        match key {
            "Type" => is_app = val == "Application",
            "Name" => app.name = val,
            "GenericName" => app.generic_name = val,
            "Comment" => app.comment = val,
            "Icon" => app.icon = val,
            "Exec" => app.exec = val,
            "Path" => app.path = val,
            "Terminal" => app.terminal = val == "true",
            "NoDisplay" => app.no_display = val == "true",
            "Hidden" => app.hidden = val == "true",
            "MimeType" => app.mime_types = val.split(';').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect(),
            "Categories" => app.categories = val.split(';').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect(),
            _ => {}
        }
    }
    for (k, (_, v)) in localized {
        match k.as_str() {
            "Name" => app.name = v,
            "GenericName" => app.generic_name = v,
            "Comment" => app.comment = v,
            _ => {}
        }
    }
    for (a, n) in action_names {
        if let Some(e) = action_execs.get(&a) {
            app.actions.push((n, e.clone()));
        }
    }
    app.actions.sort();
    (is_app && !app.name.is_empty()).then_some(app)
}

pub fn application_dirs() -> Vec<PathBuf> {
    let mut v = vec![util::data_home().join("applications")];
    v.extend(util::data_dirs().into_iter().map(|d| d.join("applications")));
    v
}

fn scan_dir(base: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            scan_dir(base, &p, out);
        } else if p.extension().map(|x| x == "desktop").unwrap_or(false) {
            let rel = p.strip_prefix(base).unwrap_or(&p).to_string_lossy().replace('/', "-");
            out.push((rel, p));
        }
    }
}

/// Every installed application (first definition of each id wins).
pub fn all() -> Vec<App> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for d in application_dirs() {
        let mut files = vec![];
        scan_dir(&d, &d, &mut files);
        files.sort();
        for (id, p) in files {
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Ok(t) = std::fs::read_to_string(&p) {
                if let Some(a) = parse_desktop(&id, &p, &t) {
                    if !a.hidden {
                        out.push(a);
                    }
                }
            }
        }
    }
    out
}

pub fn by_id(id: &str) -> Option<App> {
    for d in application_dirs() {
        // "org.kde.foo.desktop" may live in a subfolder "org/kde/foo.desktop".
        let mut candidates = vec![d.join(id)];
        let parts: Vec<&str> = id.split('-').collect();
        for i in 1..parts.len() {
            candidates.push(d.join(parts[..i].join("/")).join(parts[i..].join("-")));
        }
        for p in candidates {
            if let Ok(t) = std::fs::read_to_string(&p) {
                return parse_desktop(id, &p, &t);
            }
        }
    }
    None
}

/// mimeapps.list files, most important first (spec order).
pub fn mimeapps_files() -> Vec<PathBuf> {
    let desktops: Vec<String> = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().split(':').filter(|s| !s.is_empty()).map(|s| s.to_lowercase()).collect();
    let mut dirs = vec![util::config_home()];
    dirs.extend(util::config_dirs());
    dirs.push(util::data_home().join("applications"));
    dirs.extend(util::data_dirs().into_iter().map(|d| d.join("applications")));
    let mut v = Vec::new();
    for d in dirs {
        for de in &desktops {
            v.push(d.join(format!("{de}-mimeapps.list")));
        }
        v.push(d.join("mimeapps.list"));
    }
    v
}

#[derive(Default, Debug)]
pub struct MimeApps {
    pub defaults: HashMap<String, Vec<String>>,
    pub added: HashMap<String, Vec<String>>,
    pub removed: HashMap<String, Vec<String>>,
}

pub fn parse_mimeapps(text: &str, into: &mut MimeApps) {
    let mut group = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            group = line.trim_matches(|c| c == '[' || c == ']').to_string();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let ids: Vec<String> = v.split(';').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect();
        let map = match group.as_str() {
            "Default Applications" => &mut into.defaults,
            "Added Associations" => &mut into.added,
            "Removed Associations" => &mut into.removed,
            _ => continue,
        };
        // Earlier (more important) files win: append after what's there.
        let e = map.entry(k.trim().to_string()).or_default();
        for id in ids {
            if !e.contains(&id) {
                e.push(id);
            }
        }
    }
}

pub fn mimeapps() -> MimeApps {
    let mut m = MimeApps::default();
    for f in mimeapps_files() {
        if let Ok(t) = std::fs::read_to_string(&f) {
            parse_mimeapps(&t, &mut m);
        }
    }
    // mimeinfo.cache from each applications dir (what installed apps say they open).
    for d in application_dirs() {
        if let Ok(t) = std::fs::read_to_string(d.join("mimeinfo.cache")) {
            let mut cache = MimeApps::default();
            for line in t.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    let ids: Vec<String> = v.split(';').filter(|s| !s.is_empty()).map(String::from).collect();
                    cache.added.entry(k.to_string()).or_default().extend(ids);
                }
            }
            for (k, ids) in cache.added {
                let e = m.added.entry(k).or_default();
                for id in ids {
                    if !e.contains(&id) {
                        e.push(id);
                    }
                }
            }
        }
    }
    m
}

/// Apps that open `mimes` (the type first, then its parent types): the
/// default first. `all_apps` is the installed list.
pub fn apps_for(mimes: &[String], all_apps: &[App]) -> Vec<App> {
    let m = mimeapps();
    let mut ids: Vec<String> = Vec::new();
    let removed: Vec<String> = mimes.iter().flat_map(|t| m.removed.get(t).cloned().unwrap_or_default()).collect();
    for t in mimes {
        for id in m.defaults.get(t).into_iter().flatten().chain(m.added.get(t).into_iter().flatten()) {
            if !ids.contains(id) && !removed.contains(id) {
                ids.push(id.clone());
            }
        }
        for a in all_apps {
            if a.mime_types.iter().any(|x| x == t) && !ids.contains(&a.id) && !removed.contains(&a.id) {
                ids.push(a.id.clone());
            }
        }
    }
    ids.into_iter().filter_map(|id| all_apps.iter().find(|a| a.id == id).cloned().or_else(|| by_id(&id))).collect()
}

pub fn default_for(mimes: &[String], all_apps: &[App]) -> Option<App> {
    let m = mimeapps();
    for t in mimes {
        for id in m.defaults.get(t).into_iter().flatten() {
            if let Some(a) = all_apps.iter().find(|a| &a.id == id).cloned().or_else(|| by_id(id)) {
                return Some(a);
            }
        }
    }
    apps_for(mimes, all_apps).into_iter().next()
}

/// Make `app_id` the default for `mime` in ~/.config/mimeapps.list,
/// keeping everything else in the file as it was.
pub fn set_default(mime: &str, app_id: &str) -> Result<(), String> {
    let path = util::config_home().join("mimeapps.list");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let new = set_default_in(&text, mime, app_id);
    util::write_atomic(&path, new.as_bytes()).map_err(|e| util::plain_error(&e, &path))
}

pub fn set_default_in(text: &str, mime: &str, app_id: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let mut group_start = None;
    let mut group_end = lines.len();
    for (i, l) in lines.iter().enumerate() {
        let t = l.trim();
        if t.starts_with('[') {
            if group_start.is_some() {
                group_end = i;
                break;
            }
            if t == "[Default Applications]" {
                group_start = Some(i);
            }
        }
    }
    let entry = format!("{mime}={app_id};");
    match group_start {
        None => {
            if !lines.is_empty() && !lines.last().unwrap().trim().is_empty() {
                lines.push(String::new());
            }
            lines.push("[Default Applications]".into());
            lines.push(entry);
        }
        Some(s) => {
            let existing = (s + 1..group_end).find(|&i| lines[i].split_once('=').map(|(k, _)| k.trim() == mime).unwrap_or(false));
            match existing {
                Some(i) => lines[i] = entry,
                None => {
                    // After the group's last non-empty line.
                    let mut at = group_end;
                    while at > s + 1 && lines[at - 1].trim().is_empty() {
                        at -= 1;
                    }
                    lines.insert(at, entry);
                }
            }
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Fill in an Exec line's field codes for `files`.
pub fn expand_app_exec(app: &App, exec: &str, files: &[PathBuf]) -> Vec<Vec<String>> {
    let args = split_exec(exec);
    let multi = args.iter().any(|a| a == "%F" || a == "%U");
    let single = args.iter().any(|a| a.contains("%f") || a.contains("%u"));
    let groups: Vec<Vec<PathBuf>> = if multi || !single {
        vec![files.to_vec()]
    } else {
        files.iter().map(|f| vec![f.clone()]).collect()
    };
    let groups = if groups.is_empty() { vec![vec![]] } else { groups };
    groups
        .into_iter()
        .map(|g| {
            let mut out = Vec::new();
            for a in &args {
                match a.as_str() {
                    "%F" => out.extend(g.iter().map(|f| f.to_string_lossy().into_owned())),
                    "%U" => out.extend(g.iter().map(|f| util::file_uri(f))),
                    "%i" => {
                        if !app.icon.is_empty() {
                            out.push("--icon".into());
                            out.push(app.icon.clone());
                        }
                    }
                    _ => {
                        let mut s = String::new();
                        let mut it = a.chars().peekable();
                        while let Some(c) = it.next() {
                            if c == '%' {
                                match it.next() {
                                    Some('f') => s.push_str(&g.first().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()),
                                    Some('u') => s.push_str(&g.first().map(|f| util::file_uri(f)).unwrap_or_default()),
                                    Some('c') => s.push_str(&app.name),
                                    Some('k') => s.push_str(&app.file.to_string_lossy()),
                                    Some('%') => s.push('%'),
                                    _ => {}
                                }
                            } else {
                                s.push(c);
                            }
                        }
                        if !s.is_empty() || !a.starts_with('%') {
                            out.push(s);
                        }
                    }
                }
            }
            out
        })
        .collect()
}

/// Start a program detached from grove (its own session, no shared stdio).
pub fn spawn_detached(argv: &[String], cwd: Option<&Path>) -> Result<(), String> {
    let (cmd, args) = argv.split_first().ok_or("Nothing to run.")?;
    let mut c = std::process::Command::new(cmd);
    c.args(args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    unsafe {
        c.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    // Reap it in the background so it doesn't linger as a zombie.
    match c.spawn() {
        Ok(mut ch) => {
            std::thread::spawn(move || {
                let _ = ch.wait();
            });
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(format!("The program “{cmd}” isn't installed.")),
        Err(e) => Err(format!("Couldn't start “{cmd}”: {e}")),
    }
}

/// Open `files` with `app`.
pub fn launch(app: &App, files: &[PathBuf]) -> Result<(), String> {
    launch_exec(app, &app.exec, files)
}

pub fn launch_exec(app: &App, exec: &str, files: &[PathBuf]) -> Result<(), String> {
    let cwd = if app.path.is_empty() { files.first().and_then(|f| f.parent()).map(Path::to_path_buf) } else { Some(PathBuf::from(&app.path)) };
    for mut argv in expand_app_exec(app, exec, files) {
        if app.terminal {
            let mut t = terminal_command(cwd.as_deref().unwrap_or(Path::new("/")));
            t.push("-e".into());
            t.append(&mut argv);
            argv = t;
        }
        spawn_detached(&argv, cwd.as_deref())?;
    }
    Ok(())
}

/// The terminal to use: grove's setting, $TERMINAL, else the first of
/// tessera, konsole and friends that's installed. Returns the command
/// that opens it in `dir`.
pub fn terminal_command(dir: &Path) -> Vec<String> {
    let setting = crate::settings::load().terminal;
    let mut candidates: Vec<String> = vec![];
    if !setting.is_empty() {
        candidates.push(setting);
    }
    if let Ok(t) = std::env::var("TERMINAL") {
        candidates.push(t);
    }
    candidates.extend(["tessera", "konsole", "foot", "kitty", "alacritty", "wezterm", "ghostty", "xterm"].iter().map(|s| s.to_string()));
    let d = dir.to_string_lossy().into_owned();
    for c in candidates {
        let argv = split_exec(&c);
        let Some(exe) = argv.first() else { continue };
        if crate::thumbs::which(exe).is_none() {
            continue;
        }
        let name = Path::new(exe).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut v = argv.clone();
        match name.as_str() {
            "konsole" => {
                v.push("--workdir".into());
                v.push(d.clone())
            }
            "foot" | "kitty" | "ghostty" => v.push(format!("--working-directory={d}")),
            "alacritty" => {
                v.push("--working-directory".into());
                v.push(d.clone())
            }
            "wezterm" => {
                v.extend(["start".into(), "--cwd".into(), d.clone()]);
            }
            _ => {}
        }
        return v;
    }
    vec!["xterm".into()]
}

pub fn open_terminal(dir: &Path) -> Result<(), String> {
    spawn_detached(&terminal_command(dir), Some(dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_exec() {
        assert_eq!(split_exec(r#"foo "a b" c\ d %F"#), vec!["foo", "a b", r"c\", "d", "%F"]);
        assert_eq!(split_exec(r#"sh -c "echo \"hi\"""#), vec!["sh", "-c", "echo \"hi\""]);
        assert_eq!(split_exec(r#"x """#), vec!["x", ""]);
    }

    #[test]
    fn parses_entry() {
        let t = "[Desktop Entry]\nType=Application\nName=Viewer\nName[de]=Betrachter\nExec=viewer %U\nMimeType=image/png;image/jpeg;\nIcon=viewer\n\n[Desktop Action new]\nName=New Window\nExec=viewer --new\n";
        let a = parse_desktop("viewer.desktop", Path::new("/a/viewer.desktop"), t).unwrap();
        assert_eq!(a.mime_types, vec!["image/png", "image/jpeg"]);
        assert_eq!(a.actions, vec![("New Window".to_string(), "viewer --new".to_string())]);
        let files = vec![PathBuf::from("/x/a b.png"), PathBuf::from("/y.png")];
        assert_eq!(expand_app_exec(&a, &a.exec, &files), vec![vec!["viewer".to_string(), "file:///x/a%20b.png".into(), "file:///y.png".into()]]);
        let single = App { exec: "edit %f".into(), ..a.clone() };
        assert_eq!(expand_app_exec(&single, &single.exec, &files).len(), 2);
        assert!(parse_desktop("l.desktop", Path::new("/l"), "[Desktop Entry]\nType=Link\nName=x\n").is_none());
    }

    #[test]
    fn mimeapps_edit() {
        let src = "[Added Associations]\ntext/plain=a.desktop;\n\n[Default Applications]\nimage/png=old.desktop;\ntext/html=b.desktop;\n\n[Removed Associations]\n";
        let out = set_default_in(src, "image/png", "new.desktop");
        assert!(out.contains("image/png=new.desktop;"));
        assert!(!out.contains("old.desktop"));
        assert!(out.contains("text/plain=a.desktop;"));
        let out2 = set_default_in(&out, "video/mp4", "mpv.desktop");
        let mut m = MimeApps::default();
        parse_mimeapps(&out2, &mut m);
        assert_eq!(m.defaults["video/mp4"], vec!["mpv.desktop"]);
        assert_eq!(m.defaults["text/html"], vec!["b.desktop"]);
        assert!(m.removed.is_empty());
        let fresh = set_default_in("", "a/b", "c.desktop");
        assert_eq!(fresh, "[Default Applications]\na/b=c.desktop;\n");
    }
}
