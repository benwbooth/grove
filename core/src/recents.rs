//! Recently used files: the shared ~/.local/share/recently-used.xbel that
//! GTK and KDE apps write too.

use crate::util;
use quick_xml::events::Event;
use std::path::PathBuf;

pub fn xbel_path() -> PathBuf {
    util::data_home().join("recently-used.xbel")
}

/// (path, modified seconds), newest first, existing files only.
pub fn parse(text: &str) -> Vec<(PathBuf, i64)> {
    let mut out = Vec::new();
    let mut r = quick_xml::Reader::from_str(text);
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.name().as_ref() == b"bookmark" => {
                let mut href = None;
                let mut when = 0i64;
                for a in e.attributes().flatten() {
                    let v = a.unescape_value().map(|c| c.into_owned()).unwrap_or_default();
                    match a.key.as_ref() {
                        b"href" => href = Some(v),
                        b"modified" | b"visited" => when = when.max(parse_utc(&v).unwrap_or(0)),
                        _ => {}
                    }
                }
                if let Some(p) = href.as_deref().and_then(util::path_from_uri) {
                    out.push((p, when));
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out
}

pub fn list(limit: usize) -> Vec<PathBuf> {
    let text = std::fs::read_to_string(xbel_path()).unwrap_or_default();
    parse(&text).into_iter().map(|(p, _)| p).filter(|p| p.exists()).take(limit).collect()
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// "2024-05-01T10:00:00.123456Z" -> seconds.
pub fn parse_utc(s: &str) -> Option<i64> {
    let s = s.trim().trim_end_matches('Z');
    let (d, t) = s.split_once('T')?;
    let dp: Vec<i64> = d.split('-').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    let t = t.split('.').next()?;
    let tp: Vec<i64> = t.split(':').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    if dp.len() != 3 || tp.len() != 3 {
        return None;
    }
    Some(days_from_civil(dp[0], dp[1], dp[2]) * 86400 + tp[0] * 3600 + tp[1] * 60 + tp[2])
}

pub fn format_utc(secs: i64) -> String {
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // civil_from_days
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000000Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

/// Record that grove opened `path` (type `mime`).
pub fn add(path: &std::path::Path, mime: &str) {
    let file = xbel_path();
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    let new = add_to(&text, path, mime, util::now_secs());
    let _ = util::write_atomic(&file, new.as_bytes());
}

pub fn add_to(text: &str, path: &std::path::Path, mime: &str, now: i64) -> String {
    let uri = util::file_uri(path);
    let stamp = format_utc(now);
    let href = format!("href=\"{}\"", esc(&uri));
    if let Some(i) = text.find(&href) {
        // Update modified/visited on the existing bookmark tag.
        let tag_start = text[..i].rfind('<').unwrap_or(i);
        let tag_end = i + text[i..].find('>').unwrap_or(0);
        let tag = &text[tag_start..tag_end];
        let re = regex::Regex::new(r#"(modified|visited)="[^"]*""#).unwrap();
        let new_tag = re.replace_all(tag, |c: &regex::Captures| format!("{}=\"{stamp}\"", &c[1])).into_owned();
        return format!("{}{}{}", &text[..tag_start], new_tag, &text[tag_end..]);
    }
    let entry = format!(
        "  <bookmark {href} added=\"{stamp}\" modified=\"{stamp}\" visited=\"{stamp}\">\n    <info>\n      <metadata owner=\"http://freedesktop.org\">\n        <mime:mime-type type=\"{}\"/>\n        <bookmark:applications>\n          <bookmark:application name=\"grove\" exec=\"&apos;grove %u&apos;\" modified=\"{stamp}\" count=\"1\"/>\n        </bookmark:applications>\n      </metadata>\n    </info>\n  </bookmark>\n",
        esc(mime)
    );
    match text.rfind("</xbel>") {
        Some(i) => format!("{}{entry}{}", &text[..i], &text[i..]),
        None => format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xbel version=\"1.0\"\n      xmlns:bookmark=\"http://www.freedesktop.org/standards/desktop-bookmarks\"\n      xmlns:mime=\"http://www.freedesktop.org/standards/shared-mime-info\"\n>\n{entry}</xbel>\n"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let t = add_to("", std::path::Path::new("/a/b c.txt"), "text/plain", 1_700_000_000);
        let t = add_to(&t, std::path::Path::new("/a/d.png"), "image/png", 1_700_000_100);
        let p = parse(&t);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].0, PathBuf::from("/a/d.png"));
        let t2 = add_to(&t, std::path::Path::new("/a/b c.txt"), "text/plain", 1_700_000_200);
        let p = parse(&t2);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].0, PathBuf::from("/a/b c.txt"));
        assert_eq!(p[0].1, 1_700_000_200);
        assert_eq!(parse_utc(&format_utc(1_700_000_000)), Some(1_700_000_000));
    }
}
