//! Batch rename: find/replace, numbering, case changes, with a preview
//! and checks for clashes before anything is touched.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Rule {
    /// "replace", "number", "case", "prefix", "suffix".
    pub mode: String,
    #[serde(default)]
    pub find: String,
    #[serde(default)]
    pub replace: String,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub case_sensitive: bool,
    /// Numbering: base name ("Photo"), start, digits.
    #[serde(default)]
    pub base: String,
    #[serde(default)]
    pub start: u32,
    #[serde(default)]
    pub digits: u32,
    /// Case: "lower", "upper", "title", "sentence".
    #[serde(default)]
    pub case: String,
    #[serde(default)]
    pub text: String,
    /// Leave the extension alone.
    #[serde(default)]
    pub keep_extension: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Planned {
    pub from: String,
    pub to: String,
    /// Plain-language problem, if any ("Two items would be called …").
    pub problem: String,
}

fn title_case(s: &str) -> String {
    let mut out = String::new();
    let mut start = true;
    for c in s.chars() {
        if start && c.is_alphanumeric() {
            out.extend(c.to_uppercase());
            start = false;
        } else {
            out.extend(c.to_lowercase());
        }
        if c.is_whitespace() || c == '-' || c == '_' {
            start = true;
        }
    }
    out
}

pub fn apply_one(rule: &Rule, name: &str, index: usize, is_dir: bool) -> String {
    let (stem, ext) = if rule.keep_extension { crate::ops::split_name(name, is_dir) } else { (name.to_string(), String::new()) };
    let new_stem = match rule.mode.as_str() {
        "replace" => {
            if rule.find.is_empty() {
                stem.clone()
            } else if rule.regex || !rule.case_sensitive {
                let pat = if rule.regex { rule.find.clone() } else { regex::escape(&rule.find) };
                let pat = if rule.case_sensitive { pat } else { format!("(?i){pat}") };
                match regex::Regex::new(&pat) {
                    Ok(r) => r.replace_all(&stem, rule.replace.as_str()).into_owned(),
                    Err(_) => stem.clone(),
                }
            } else {
                stem.replace(&rule.find, &rule.replace)
            }
        }
        "number" => {
            let n = rule.start as usize + index;
            let w = rule.digits.max(1) as usize;
            let base = if rule.base.is_empty() { stem.clone() } else { rule.base.clone() };
            format!("{base} {n:0w$}")
        }
        "case" => match rule.case.as_str() {
            "lower" => stem.to_lowercase(),
            "upper" => stem.to_uppercase(),
            "title" => title_case(&stem),
            "sentence" => {
                let l = stem.to_lowercase();
                let mut c = l.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => l,
                }
            }
            _ => stem.clone(),
        },
        "prefix" => format!("{}{stem}", rule.text),
        "suffix" => format!("{stem}{}", rule.text),
        _ => stem.clone(),
    };
    format!("{new_stem}{ext}")
}

/// The new names for `paths` (all in their own folders), with problems.
pub fn plan(rule: &Rule, paths: &[PathBuf]) -> Vec<Planned> {
    let mut out: Vec<Planned> = paths
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let to = apply_one(rule, &name, i, p.is_dir());
            Planned { from: p.to_string_lossy().into_owned(), to, problem: String::new() }
        })
        .collect();
    let renamed: std::collections::HashSet<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    for i in 0..out.len() {
        let to = out[i].to.clone();
        let dir = Path::new(&out[i].from).parent().unwrap_or(Path::new("/")).to_path_buf();
        let target = dir.join(&to);
        if to.is_empty() {
            out[i].problem = "The new name would be empty.".into();
        } else if to.contains('/') {
            out[i].problem = "Names can't contain “/”.".into();
        } else if out.iter().enumerate().any(|(j, o)| j != i && Path::new(&o.from).parent() == Some(dir.as_path()) && o.to == to) {
            out[i].problem = format!("Two items would be called “{to}”.");
        } else if target.to_string_lossy() != out[i].from && target.symlink_metadata().is_ok() && !renamed.contains(&*target.to_string_lossy()) {
            out[i].problem = format!("“{to}” already exists in this folder.");
        }
    }
    out
}

/// Carry out a plan (refusing if any item has a problem). Uses temporary
/// names first so swaps (a→b, b→a) work. Returns the undo action.
pub fn execute(planned: &[Planned]) -> Result<crate::ops::Action, String> {
    if let Some(p) = planned.iter().find(|p| !p.problem.is_empty()) {
        return Err(p.problem.clone());
    }
    let mut temps = Vec::new();
    for (i, p) in planned.iter().enumerate() {
        let from = PathBuf::from(&p.from);
        let target = from.parent().unwrap_or(Path::new("/")).join(&p.to);
        if target == from {
            continue;
        }
        let tmp = from.parent().unwrap_or(Path::new("/")).join(format!(".grove-rename-{}-{i}", std::process::id()));
        std::fs::rename(&from, &tmp).map_err(|e| crate::util::plain_error(&e, &from))?;
        temps.push((from, tmp, target));
    }
    let mut pairs = Vec::new();
    for (from, tmp, target) in temps {
        if let Err(e) = std::fs::rename(&tmp, &target) {
            let _ = std::fs::rename(&tmp, &from);
            return Err(crate::util::plain_error(&e, &target));
        }
        pairs.push((from, target));
    }
    Ok(crate::ops::Action::Moved { pairs })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        let r = Rule { mode: "replace".into(), find: "IMG".into(), replace: "Photo".into(), keep_extension: true, ..Default::default() };
        assert_eq!(apply_one(&r, "img_001.JPG", 0, false), "Photo_001.JPG");
        let r = Rule { mode: "replace".into(), find: r"(\d+)".into(), replace: "#$1".into(), regex: true, case_sensitive: true, ..Default::default() };
        assert_eq!(apply_one(&r, "a12b3", 0, false), "a#12b#3");
        let r = Rule { mode: "number".into(), base: "Trip".into(), start: 1, digits: 3, keep_extension: true, ..Default::default() };
        assert_eq!(apply_one(&r, "x.jpg", 4, false), "Trip 005.jpg");
        let r = Rule { mode: "case".into(), case: "title".into(), keep_extension: true, ..Default::default() };
        assert_eq!(apply_one(&r, "my holiday-pics.PNG", 0, false), "My Holiday-Pics.PNG");
        let r = Rule { mode: "prefix".into(), text: "2024 ".into(), ..Default::default() };
        assert_eq!(apply_one(&r, "a.txt", 0, false), "2024 a.txt");
    }

    #[test]
    fn plan_checks_and_swaps() {
        let _g = crate::test_env_lock();
        let t = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_DATA_HOME", t.path().join("data"));
        let a = t.path().join("a.txt");
        let b = t.path().join("b.txt");
        std::fs::write(&a, "A").unwrap();
        std::fs::write(&b, "B").unwrap();
        std::fs::write(t.path().join("taken.txt"), "").unwrap();
        // Two would collide.
        let r = Rule { mode: "replace".into(), find: "[ab]".into(), replace: "x".into(), regex: true, case_sensitive: true, ..Default::default() };
        let p = plan(&r, &[a.clone(), b.clone()]);
        assert!(p.iter().all(|x| x.problem.contains("Two items")));
        assert!(execute(&p).is_err());
        // Existing name.
        let r = Rule { mode: "replace".into(), find: "a".into(), replace: "taken".into(), case_sensitive: true, ..Default::default() };
        assert!(plan(&r, &[a.clone()])[0].problem.contains("already exists"));
        // Swap via upper case then back: a.txt -> A.TXT on case-sensitive disks.
        let r = Rule { mode: "case".into(), case: "upper".into(), ..Default::default() };
        let p = plan(&r, &[a.clone(), b.clone()]);
        assert!(p.iter().all(|x| x.problem.is_empty()), "{p:?}");
        let undo = execute(&p).unwrap();
        assert_eq!(std::fs::read_to_string(t.path().join("A.TXT")).unwrap(), "A");
        undo.invert().unwrap();
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "A");
        std::env::remove_var("XDG_DATA_HOME");
    }
}
