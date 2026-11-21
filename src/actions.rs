//! Right-click actions: built-in quick actions and Dolphin service menus
//! (~/.local/share/kio/servicemenus/*.desktop and the system's), so
//! existing ones keep working.

use crate::bridge::ffi::{self, QString};
use cxx_qt::CxxQtThread;
use grove_core::{apps, util};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub fn quick(qt: &CxxQtThread<ffi::Grove>, action: &str, paths: &[PathBuf], arg: &str) -> Value {
    let mut errors = vec![];
    let mut results = vec![];
    match action {
        "rotate-left" | "rotate-right" | "rotate-180" => {
            let deg = match action {
                "rotate-left" => -90,
                "rotate-right" => 90,
                _ => 180,
            };
            for p in paths {
                let e = ffi::grove_image_rotate(&QString::from(p.to_string_lossy().as_ref()), deg).to_string();
                if !e.is_empty() {
                    errors.push(e);
                }
            }
        }
        "convert" => {
            for p in paths {
                let r = ffi::grove_image_convert(&QString::from(p.to_string_lossy().as_ref()), &QString::from(arg)).to_string();
                if let Some(e) = r.strip_prefix('!') {
                    errors.push(e.to_string());
                } else {
                    results.push(r);
                }
            }
            if !results.is_empty() {
                push_exists(qt, "Convert Image", &results);
            }
        }
        "open-as-root" => {
            let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "grove".into());
            let mut argv = vec!["pkexec".to_string(), "env".into()];
            for v in ["WAYLAND_DISPLAY", "XDG_RUNTIME_DIR", "DISPLAY", "XAUTHORITY", "QT_QPA_PLATFORM"] {
                if let Ok(x) = std::env::var(v) {
                    argv.push(format!("{v}={x}"));
                }
            }
            argv.push("GROVE_NO_DBUS=1".into());
            argv.push(exe);
            argv.push("--new-window".into());
            argv.extend(paths.iter().map(|p| p.to_string_lossy().into_owned()));
            if let Err(e) = apps::spawn_detached(&argv, None) {
                errors.push(e);
            }
        }
        _ => errors.push(format!("Unknown action “{action}”.")),
    }
    json!({"errors": errors, "results": results})
}

fn push_exists(_qt: &CxxQtThread<ffi::Grove>, label: &str, paths: &[String]) {
    crate::controller::push_history_any(label, grove_core::ops::Action::Exists { paths: paths.iter().map(PathBuf::from).collect() });
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServiceAction {
    pub name: String,
    pub icon: String,
    pub exec: String,
    pub submenu: String,
}

pub fn servicemenu_dirs() -> Vec<PathBuf> {
    let mut v = vec![util::data_home().join("kio/servicemenus"), util::data_home().join("kservices5/ServiceMenus")];
    for d in util::data_dirs() {
        v.push(d.join("kio/servicemenus"));
        v.push(d.join("kservices5/ServiceMenus"));
    }
    v.push(PathBuf::from("/run/current-system/sw/share/kio/servicemenus"));
    v.dedup();
    v
}

fn mime_matches(patterns: &[String], mimes: &[Vec<String>], all_dirs: bool, all_files: bool) -> bool {
    if patterns.is_empty() {
        return false;
    }
    mimes.iter().all(|item| {
        patterns.iter().any(|p| {
            let p = p.as_str();
            p == "all/all"
                || (p == "all/allfiles" && all_files)
                || (p == "inode/directory" && all_dirs && item.iter().any(|m| m == "inode/directory"))
                || item.iter().any(|m| m == p || (p.ends_with("/*") && m.starts_with(&p[..p.len() - 1])))
        })
    })
}

pub fn parse_servicemenu(text: &str) -> (Vec<String>, Vec<ServiceAction>, String, u32, u32) {
    let mut group = String::new();
    let mut mimes = vec![];
    let mut submenu = String::new();
    let mut order: Vec<String> = vec![];
    let mut min_files = 0;
    let mut max_files = 0;
    let mut acts: std::collections::HashMap<String, ServiceAction> = Default::default();
    let mut desktop_type_ok = true;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            group = l.trim_matches(|c| c == '[' || c == ']').to_string();
            continue;
        }
        let Some((k, v)) = l.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim());
        if group == "Desktop Entry" {
            match k {
                "MimeType" => mimes = v.split(';').filter(|s| !s.is_empty()).map(String::from).collect(),
                "Actions" => order = v.split(';').filter(|s| !s.is_empty()).map(String::from).collect(),
                "X-KDE-Submenu" => submenu = v.to_string(),
                "X-KDE-MinNumberOfUrls" => min_files = v.parse().unwrap_or(0),
                "X-KDE-MaxNumberOfUrls" => max_files = v.parse().unwrap_or(0),
                "Type" => desktop_type_ok = v == "Service",
                _ => {}
            }
        } else if let Some(a) = group.strip_prefix("Desktop Action ") {
            let e = acts.entry(a.to_string()).or_insert(ServiceAction { name: String::new(), icon: String::new(), exec: String::new(), submenu: String::new() });
            match k {
                "Name" => e.name = v.to_string(),
                "Icon" => e.icon = v.to_string(),
                "Exec" => e.exec = v.to_string(),
                _ => {}
            }
        }
    }
    let _ = desktop_type_ok;
    let list = order.into_iter().filter_map(|a| acts.remove(&a)).filter(|a| !a.exec.is_empty() && !a.name.is_empty()).map(|mut a| {
        a.submenu = submenu.clone();
        a
    }).collect();
    (mimes, list, submenu, min_files, max_files)
}

pub fn service_menus(paths: &[PathBuf]) -> Value {
    if paths.is_empty() {
        return json!([]);
    }
    let mimes: Vec<Vec<String>> = paths
        .iter()
        .map(|p| {
            if p.is_dir() {
                vec!["inode/directory".to_string()]
            } else {
                crate::mime::with_parents(&crate::mime::deep(&p.to_string_lossy()).name)
            }
        })
        .collect();
    let all_dirs = paths.iter().all(|p| p.is_dir());
    let all_files = paths.iter().all(|p| !p.is_dir());
    let mut out = vec![];
    let mut seen = std::collections::HashSet::new();
    for d in servicemenu_dirs() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().map(|x| x == "desktop").unwrap_or(false)).collect();
        files.sort();
        for f in files {
            if !seen.insert(f.file_name().unwrap().to_owned()) {
                continue;
            }
            let Ok(t) = std::fs::read_to_string(&f) else { continue };
            let (pats, acts, _sub, min, max) = parse_servicemenu(&t);
            if (min > 0 && (paths.len() as u32) < min) || (max > 0 && (paths.len() as u32) > max) {
                continue;
            }
            if !mime_matches(&pats, &mimes, all_dirs, all_files) {
                continue;
            }
            for a in acts {
                out.push(json!({"name": a.name, "icon": a.icon, "exec": a.exec, "submenu": a.submenu}));
            }
        }
    }
    json!(out)
}

pub fn run_service_menu(exec: &str, paths: &[PathBuf]) -> Result<(), String> {
    let app = apps::App { name: "service menu".into(), exec: exec.to_string(), ..Default::default() };
    // %d / %D: the folder(s) of the items (KDE extensions).
    let dir = paths.first().and_then(|p| p.parent()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let exec = exec.replace("%d", &dir).replace("%D", &dir);
    let cwd = paths.first().and_then(|p| p.parent()).map(Path::to_path_buf);
    for argv in apps::expand_app_exec(&app, &exec, paths) {
        apps::spawn_detached(&argv, cwd.as_deref())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_dolphin_servicemenu() {
        let t = "[Desktop Entry]\nType=Service\nMimeType=image/*;\nActions=a;b;\nX-KDE-Submenu=Images\n\n[Desktop Action a]\nName=Shrink\nExec=convert %f -resize 50% %f\n\n[Desktop Action b]\nName=Tag\nIcon=tag\nExec=tagit %F\n";
        let (m, acts, sub, _, _) = parse_servicemenu(t);
        assert_eq!(m, vec!["image/*"]);
        assert_eq!(acts.len(), 2);
        assert_eq!(sub, "Images");
        assert!(mime_matches(&m, &[vec!["image/png".into()]], false, true));
        assert!(!mime_matches(&m, &[vec!["text/plain".into()]], false, true));
        assert!(mime_matches(&["all/allfiles".into()], &[vec!["text/plain".into()]], false, true));
    }
}
