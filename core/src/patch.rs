//! Patches and diffs (unified diff: `git diff`, `git format-patch`,
//! `diff -u`/`-ruN`, `.rej` hunks) read into files, hunks and lines, for the
//! viewer's diff view (src/diffdoc.rs). Hunks are read by their line counts,
//! so a removed line that itself starts with "--" isn't taken for a header.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub kind: LineKind,
    pub text: String,
    /// "\ No newline at end of file" followed it.
    pub no_newline: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hunk {
    pub old_start: u32,
    pub old_len: u32,
    pub new_start: u32,
    pub new_len: u32,
    /// What follows the second "@@" (the function or section it's in).
    pub context: String,
    pub lines: Vec<Line>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FilePatch {
    /// The names without git's a/ and b/ ("" when there's no such side).
    pub old: String,
    pub new: String,
    pub status: Status,
    pub binary: bool,
    /// Lines between the header and the hunks that say something
    /// ("new file mode 100755", "similarity index 92%").
    pub notes: Vec<String>,
    pub hunks: Vec<Hunk>,
}

impl FilePatch {
    /// The name to show: the new one, else the old.
    pub fn name(&self) -> &str {
        if self.new.is_empty() { &self.old } else { &self.new }
    }
    pub fn counts(&self) -> (usize, usize) {
        let mut a = 0;
        let mut r = 0;
        for l in self.hunks.iter().flat_map(|h| &h.lines) {
            match l.kind {
                LineKind::Added => a += 1,
                LineKind::Removed => r += 1,
                LineKind::Context => {}
            }
        }
        (a, r)
    }
}

/// A piece of the patch, in order: text outside any file (a mail's
/// headers and message, a diffstat), or a file's changes.
#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    Text(Vec<String>),
    File(FilePatch),
}

/// "a/src/x.rs" → "src/x.rs"; "/dev/null" → ""; a tab and timestamp cut.
fn side_name(s: &str, strip: bool) -> String {
    let s = s.split('\t').next().unwrap_or("").trim_end();
    let s = s.strip_prefix('"').and_then(|x| x.strip_suffix('"')).unwrap_or(s);
    if s == "/dev/null" {
        return String::new();
    }
    if strip {
        if let Some(rest) = s.strip_prefix("a/").or_else(|| s.strip_prefix("b/")) {
            return rest.to_string();
        }
    }
    s.to_string()
}

/// "@@ -12,5 +12,7 @@ fn main() {" → (12, 5, 12, 7, "fn main() {").
fn hunk_header(l: &str) -> Option<(u32, u32, u32, u32, String)> {
    let rest = l.strip_prefix("@@ -")?;
    let end = rest.find(" @@")?;
    let (ranges, after) = (&rest[..end], &rest[end + 3..]);
    let (old, new) = ranges.split_once(" +")?;
    let range = |r: &str| -> Option<(u32, u32)> {
        match r.split_once(',') {
            Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
            None => Some((r.parse().ok()?, 1)),
        }
    };
    let (os, ol) = range(old)?;
    let (ns, nl) = range(new)?;
    Some((os, ol, ns, nl, after.strip_prefix(' ').unwrap_or(after).to_string()))
}

/// Whether the text reads as a patch (some file header or hunk in it).
pub fn looks_like_patch(text: &str) -> bool {
    let mut minus = false;
    for l in text.lines().take(400) {
        if l.starts_with("diff --git ") || (minus && l.starts_with("+++ ")) {
            return true;
        }
        if hunk_header(l).is_some() {
            return true;
        }
        minus = l.starts_with("--- ");
    }
    false
}

pub fn parse(text: &str) -> Vec<Part> {
    let lines: Vec<&str> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    let n = if text.ends_with('\n') { lines.len() - 1 } else { lines.len() };
    let mut parts: Vec<Part> = vec![];
    let mut loose: Vec<String> = vec![];
    let mut cur: Option<FilePatch> = None;
    let mut i = 0;
    let flush_loose = |parts: &mut Vec<Part>, loose: &mut Vec<String>| {
        // (Blank lines alone aren't worth a block.)
        if loose.iter().any(|l| !l.trim().is_empty()) {
            parts.push(Part::Text(std::mem::take(loose)));
        }
        loose.clear();
    };
    let finish = |parts: &mut Vec<Part>, cur: &mut Option<FilePatch>| {
        if let Some(f) = cur.take() {
            parts.push(Part::File(f));
        }
    };
    while i < n {
        let l = lines[i];
        if let Some(rest) = l.strip_prefix("diff --git ") {
            flush_loose(&mut parts, &mut loose);
            finish(&mut parts, &mut cur);
            // "a/x b/x": split in the middle where both halves match, else at " b/".
            let (a, b) = match rest.find(" b/") {
                Some(k) => (&rest[..k], &rest[k + 1..]),
                None => rest.split_once(' ').unwrap_or((rest, rest)),
            };
            cur = Some(FilePatch { old: side_name(a, true), new: side_name(b, true), ..Default::default() });
            i += 1;
            continue;
        }
        // Other diff programs' file lines ("diff -ruN a/x b/x", "Index: x").
        if (l.starts_with("diff ") || l.starts_with("Index: ")) && cur.as_ref().is_none_or(|f| !f.hunks.is_empty() || f.binary) {
            let next_is_header = lines[i + 1..n].iter().take(4).any(|x| x.starts_with("--- ") || x.starts_with("Binary files "));
            if next_is_header {
                flush_loose(&mut parts, &mut loose);
                finish(&mut parts, &mut cur);
                cur = Some(FilePatch::default());
                i += 1;
                continue;
            }
        }
        if l.starts_with("--- ") && i + 1 < n && lines[i + 1].starts_with("+++ ") {
            let git = cur.as_ref().is_some_and(|f| f.hunks.is_empty() && (!f.old.is_empty() || !f.new.is_empty()));
            let strip = git || (l[4..].starts_with("a/") && lines[i + 1][4..].starts_with("b/"));
            if cur.as_ref().is_some_and(|f| !f.hunks.is_empty()) {
                finish(&mut parts, &mut cur);
            }
            flush_loose(&mut parts, &mut loose);
            let f = cur.get_or_insert_with(FilePatch::default);
            f.old = side_name(&l[4..], strip);
            f.new = side_name(&lines[i + 1][4..], strip);
            if f.old.is_empty() && !f.new.is_empty() {
                f.status = Status::Added;
            } else if f.new.is_empty() && !f.old.is_empty() {
                f.status = Status::Deleted;
            } else if f.status == Status::Modified && f.old != f.new {
                f.status = Status::Renamed;
            }
            i += 2;
            continue;
        }
        if let Some((os, ol, ns, nl, context)) = hunk_header(l) {
            flush_loose(&mut parts, &mut loose);
            let f = cur.get_or_insert_with(FilePatch::default);
            let mut h = Hunk { old_start: os, old_len: ol, new_start: ns, new_len: nl, context, lines: vec![] };
            let (mut old_left, mut new_left) = (ol, nl);
            i += 1;
            while i < n && (old_left > 0 || new_left > 0 || lines[i].starts_with('\\')) {
                let x = lines[i];
                match x.as_bytes().first() {
                    Some(b'+') => {
                        h.lines.push(Line { kind: LineKind::Added, text: x[1..].to_string(), no_newline: false });
                        new_left = new_left.saturating_sub(1);
                    }
                    Some(b'-') => {
                        h.lines.push(Line { kind: LineKind::Removed, text: x[1..].to_string(), no_newline: false });
                        old_left = old_left.saturating_sub(1);
                    }
                    Some(b'\\') => {
                        if let Some(last) = h.lines.last_mut() {
                            last.no_newline = true;
                        }
                    }
                    // A context line (an empty one may have lost its space).
                    Some(b' ') | None => {
                        h.lines.push(Line { kind: LineKind::Context, text: x.get(1..).unwrap_or("").to_string(), no_newline: false });
                        old_left = old_left.saturating_sub(1);
                        new_left = new_left.saturating_sub(1);
                    }
                    // Cut short (a hand-edited patch): the hunk ends here.
                    _ => break,
                }
                i += 1;
            }
            f.hunks.push(h);
            continue;
        }
        if let Some(f) = cur.as_mut().filter(|f| f.hunks.is_empty()) {
            // The git header's lines.
            if let Some(r) = l.strip_prefix("rename from ") {
                f.old = r.to_string();
                f.status = Status::Renamed;
            } else if let Some(r) = l.strip_prefix("rename to ") {
                f.new = r.to_string();
                f.status = Status::Renamed;
            } else if let Some(r) = l.strip_prefix("copy from ") {
                f.old = r.to_string();
                f.status = Status::Copied;
            } else if let Some(r) = l.strip_prefix("copy to ") {
                f.new = r.to_string();
                f.status = Status::Copied;
            } else if l.starts_with("new file mode") {
                f.status = Status::Added;
                f.notes.push(l.to_string());
            } else if l.starts_with("deleted file mode") {
                f.status = Status::Deleted;
                f.notes.push(l.to_string());
            } else if l.starts_with("Binary files ") || l == "GIT binary patch" {
                f.binary = true;
                // (The base85 data of a binary patch: skipped.)
                if l == "GIT binary patch" {
                    while i + 1 < n && !lines[i + 1].starts_with("diff ") && !lines[i + 1].starts_with("-- ") {
                        i += 1;
                    }
                }
            } else if l.starts_with("index ") {
                // (Blob hashes: nothing to show.)
            } else if l.starts_with("old mode") || l.starts_with("new mode") || l.starts_with("similarity index") || l.starts_with("dissimilarity index") {
                f.notes.push(l.to_string());
            } else if l.starts_with("===") || l.starts_with("RCS file:") || l.starts_with("retrieving revision") {
                // (CVS/svn header lines.)
            } else {
                finish(&mut parts, &mut cur);
                loose.push(l.to_string());
            }
            i += 1;
            continue;
        }
        // Between files or after the last: text of its own.
        finish(&mut parts, &mut cur);
        loose.push(l.to_string());
        i += 1;
    }
    finish(&mut parts, &mut cur);
    flush_loose(&mut parts, &mut loose);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIT: &str = "From 1234 Mon Sep 17 00:00:00 2001\nFrom: A <a@b>\nSubject: [PATCH] Fix it\n\n---\n src/a.rs | 3 ++-\n 1 file changed\n\ndiff --git a/src/a.rs b/src/a.rs\nindex 111..222 100644\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,4 +1,5 @@ fn main() {\n let a = 1;\n--- x\n+let b = 2;\n+let c = 3;\n \n done();\n\\ No newline at end of file\ndiff --git a/old.txt b/new.txt\nsimilarity index 90%\nrename from old.txt\nrename to new.txt\ndiff --git a/img.png b/img.png\nnew file mode 100644\nBinary files /dev/null and b/img.png differ\n-- \n2.44.0\n";

    #[test]
    fn reads_git_patches() {
        let p = parse(GIT);
        let files: Vec<&FilePatch> = p.iter().filter_map(|x| if let Part::File(f) = x { Some(f) } else { None }).collect();
        assert_eq!(files.len(), 3);
        let a = files[0];
        assert_eq!((a.old.as_str(), a.new.as_str(), a.status), ("src/a.rs", "src/a.rs", Status::Modified));
        let h = &a.hunks[0];
        assert_eq!((h.old_start, h.old_len, h.new_start, h.new_len, h.context.as_str()), (1, 4, 1, 5, "fn main() {"));
        // "--- x" inside the hunk is a removed line ("-- x"), not a header.
        assert_eq!(h.lines[1], Line { kind: LineKind::Removed, text: "-- x".into(), no_newline: false });
        assert!(h.lines.last().unwrap().no_newline);
        assert_eq!(a.counts(), (2, 1));
        assert_eq!((files[1].old.as_str(), files[1].new.as_str(), files[1].status), ("old.txt", "new.txt", Status::Renamed));
        assert!(files[2].binary && files[2].status == Status::Added);
        // The mail's headers before, the signature after.
        assert!(matches!(&p[0], Part::Text(t) if t[2] == "Subject: [PATCH] Fix it"));
        assert!(matches!(p.last().unwrap(), Part::Text(t) if t.iter().any(|l| l == "2.44.0")));
        assert!(looks_like_patch(GIT));
    }

    #[test]
    fn reads_plain_diffs() {
        let d = "diff -ruN a/x.c b/x.c\n--- a/x.c\t2024-01-01 10:00:00\n+++ b/x.c\t2024-01-02 10:00:00\n@@ -3 +3 @@\n-old\n+new\n--- /dev/null\n+++ y.c\n@@ -0,0 +1,2 @@\n+one\n+two\n";
        let p = parse(d);
        let files: Vec<&FilePatch> = p.iter().filter_map(|x| if let Part::File(f) = x { Some(f) } else { None }).collect();
        assert_eq!(files.len(), 2);
        assert_eq!((files[0].old.as_str(), files[0].new.as_str()), ("x.c", "x.c"));
        assert_eq!((files[0].hunks[0].old_len, files[0].hunks[0].new_len), (1, 1));
        assert_eq!((files[1].old.as_str(), files[1].new.as_str(), files[1].status), ("", "y.c", Status::Added));
        assert_eq!(files[1].counts(), (2, 0));
        assert!(!looks_like_patch("just some\ntext\n--- not a diff\n"));
    }
}
