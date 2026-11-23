//! Google Docs, Sheets, Slides... as synced folders keep them: small JSON
//! files holding the document's link (Insync: .gddoc, .gdsheet, .gdslides;
//! Google Drive for desktop: .gdoc, .gsheet, .gslides; and the rest).
//! Opening one opens its page; its content comes from Google (an rclone
//! Google Drive remote exports it, see remote.rs).

use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kind {
    /// What Google calls it ("Google Sheets").
    pub app: &'static str,
    /// Our MIME type (Google's own names for them).
    pub mime: &'static str,
    pub comment: &'static str,
    pub icon: &'static str,
    /// What an export turns it into.
    pub export: &'static str,
}

const DOC: Kind = Kind { app: "Google Docs", mime: "application/vnd.google-apps.document", comment: "Google Docs document", icon: "x-office-document", export: "docx" };
const SHEET: Kind = Kind { app: "Google Sheets", mime: "application/vnd.google-apps.spreadsheet", comment: "Google Sheets spreadsheet", icon: "x-office-spreadsheet", export: "xlsx" };
const SLIDES: Kind = Kind { app: "Google Slides", mime: "application/vnd.google-apps.presentation", comment: "Google Slides presentation", icon: "x-office-presentation", export: "pptx" };
const DRAWING: Kind = Kind { app: "Google Drawings", mime: "application/vnd.google-apps.drawing", comment: "Google Drawings drawing", icon: "x-office-drawing", export: "pdf" };
const FORM: Kind = Kind { app: "Google Forms", mime: "application/vnd.google-apps.form", comment: "Google Forms form", icon: "x-office-document", export: "" };
const MAP: Kind = Kind { app: "Google My Maps", mime: "application/vnd.google-apps.map", comment: "Google My Maps map", icon: "applications-geography|x-office-document", export: "" };
const SITE: Kind = Kind { app: "Google Sites", mime: "application/vnd.google-apps.site", comment: "Google Sites site", icon: "text-html", export: "" };
const TABLE: Kind = Kind { app: "Google Tables", mime: "application/vnd.google-apps.table", comment: "Google Tables table", icon: "x-office-spreadsheet", export: "" };
const SCRIPT: Kind = Kind { app: "Google Apps Script", mime: "application/vnd.google-apps.script", comment: "Google Apps Script project", icon: "text-x-script", export: "" };
const JAM: Kind = Kind { app: "Google Jamboard", mime: "application/vnd.google-apps.jam", comment: "Google Jamboard", icon: "x-office-drawing", export: "pdf" };
const VIDS: Kind = Kind { app: "Google Vids", mime: "application/vnd.google-apps.vid", comment: "Google Vids video", icon: "video-x-generic", export: "" };

/// The kind of Google file a name is, if any.
pub fn kind(name: &str) -> Option<Kind> {
    let l = name.to_ascii_lowercase();
    let ext = l.rsplit_once('.').map(|(_, e)| e)?;
    Some(match ext {
        "gddoc" | "gdoc" => DOC,
        "gdsheet" | "gsheet" => SHEET,
        "gdslides" | "gslides" => SLIDES,
        "gddraw" | "gdraw" => DRAWING,
        "gdform" | "gform" => FORM,
        "gdmap" | "gmap" => MAP,
        "gdsite" | "gsite" => SITE,
        "gdtable" | "gtable" => TABLE,
        "gdscript" | "gscript" => SCRIPT,
        "gdjam" | "gjam" => JAM,
        "gdvid" | "gvid" => VIDS,
        _ => return None,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub url: String,
    pub file_id: String,
    pub email: String,
}

/// The link inside such a file (Insync: url, file_id, account_email;
/// Google Drive: url, doc_id or resource_id "type:ID", email).
pub fn link(path: &Path) -> Option<Link> {
    kind(&path.file_name()?.to_string_lossy())?;
    let b = std::fs::read(path).ok()?;
    if b.len() > 64 * 1024 {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&b).ok()?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let mut id = s("file_id");
    if id.is_empty() {
        id = s("doc_id");
    }
    if id.is_empty() {
        id = s("resource_id").rsplit(':').next().unwrap_or("").to_string();
    }
    let mut url = s("url");
    if url.is_empty() && !id.is_empty() {
        url = format!("https://drive.google.com/open?id={id}");
    }
    let email = if s("account_email").is_empty() { s("email") } else { s("account_email") };
    (!url.is_empty()).then_some(Link { url, file_id: id, email })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_links() {
        let t = tempfile::tempdir().unwrap();
        let a = t.path().join("Budget.gdsheet");
        std::fs::write(&a, r#"{"url": "https://docs.google.com/spreadsheets/d/X1/edit", "file_id": "X1", "account_email": "a@b.c"}"#).unwrap();
        let l = link(&a).unwrap();
        assert_eq!((l.file_id.as_str(), l.email.as_str()), ("X1", "a@b.c"));
        let b = t.path().join("Notes.gdoc");
        std::fs::write(&b, r#"{"doc_id": "D2", "resource_id": "document:D2", "email": "x@y.z"}"#).unwrap();
        let l = link(&b).unwrap();
        assert_eq!(l.url, "https://drive.google.com/open?id=D2");
        assert_eq!(kind("x.gslides").unwrap().app, "Google Slides");
        assert!(kind("x.txt").is_none());
    }
}
