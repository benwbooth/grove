//! grove's settings (~/.config/grove/settings.json), favourites and the
//! view each folder was last shown in.

use crate::util;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

fn t() -> bool {
    true
}
fn icon_default() -> u32 {
    64
}
fn view_default() -> String {
    "icons".into()
}
fn theme_default() -> String {
    "auto".into()
}
fn icon_style_default() -> String {
    "macos".into()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    /// Command for "Open in Terminal" ("" = automatic).
    #[serde(default)]
    pub terminal: String,
    #[serde(default)]
    pub show_hidden: bool,
    #[serde(default = "t")]
    pub folders_first: bool,
    /// View for folders never seen before: icons, list, columns, gallery.
    #[serde(default = "view_default")]
    pub default_view: String,
    #[serde(default = "icon_default")]
    pub icon_size: u32,
    /// Remember each folder's view (Finder does).
    #[serde(default = "t")]
    pub per_folder_views: bool,
    /// Open items with one click (Dolphin's default) instead of two.
    #[serde(default)]
    pub single_click: bool,
    /// Ask before moving to the Trash.
    #[serde(default)]
    pub confirm_trash: bool,
    /// Ask before emptying the Trash.
    #[serde(default = "t")]
    pub confirm_empty_trash: bool,
    #[serde(default = "t")]
    pub show_thumbnails: bool,
    /// Thumbnails of files bigger than this many MB are skipped (0 = none).
    #[serde(default)]
    pub thumbnail_max_mb: u32,
    /// Show a preview in item tooltips.
    #[serde(default)]
    pub preview_tooltips: bool,
    /// auto (follow hearth / the system), dark, light.
    #[serde(default = "theme_default")]
    pub theme: String,
    /// macos (WhiteSur icons, Finder-like) or system (the desktop's icon theme).
    #[serde(default = "icon_style_default")]
    pub icon_style: String,
    #[serde(default = "t")]
    pub show_path_bar: bool,
    #[serde(default = "t")]
    pub show_status_bar: bool,
    /// No separate title bar: grove's toolbar is the title bar, with
    /// Breeze-style window buttons at its right end. Off: the desktop's title bar.
    #[serde(default = "t")]
    pub client_decorations: bool,
    /// Show file extensions.
    #[serde(default = "t")]
    pub show_extensions: bool,
    #[serde(default = "t")]
    pub show_git_status: bool,
    /// Search content as well as names in the toolbar search.
    #[serde(default)]
    pub search_contents: bool,
    /// Respect .gitignore in searches.
    #[serde(default)]
    pub search_respect_ignore: bool,
    #[serde(default)]
    pub favourites: Vec<String>,
    /// Sidebar sections the user collapsed.
    #[serde(default)]
    pub collapsed_sections: Vec<String>,
    /// Shown the welcome sheet.
    #[serde(default)]
    pub welcomed: bool,
    #[serde(default)]
    pub sidebar_width: u32,
    /// Column view: width of a column (0 = default), set by Alt-dragging a divider.
    #[serde(default)]
    pub column_width: u32,
    /// List view columns after the name, in order.
    #[serde(default = "default_columns")]
    pub list_columns: Vec<String>,
    /// Dolphin's Information panel / Finder's preview pane on the right.
    #[serde(default)]
    pub info_panel: bool,
    /// List view: a preview pane under the list, and its height (0 = default).
    #[serde(default = "t")]
    pub list_preview: bool,
    #[serde(default)]
    pub list_preview_height: u32,
    /// List and icon views: the Information panel under the files (else at the side).
    #[serde(default = "t")]
    pub info_bottom: bool,
    /// Google sign-in (grove's own OAuth client, from the user's Google
    /// Cloud project) and the accounts signed in (tokens in the keyring).
    #[serde(default)]
    pub google_client_id: String,
    #[serde(default)]
    pub google_client_secret: String,
    #[serde(default)]
    pub google_accounts: Vec<String>,
    /// Width of the side Information panel (0 = default).
    #[serde(default)]
    pub info_panel_width: u32,
    /// Dolphin's Folders panel (a tree) next to the sidebar.
    #[serde(default)]
    pub folders_panel: bool,
    /// The terminal panel (F4) at the bottom.
    #[serde(default)]
    pub terminal_panel: bool,
    /// Reopen last time's windows and tabs.
    #[serde(default = "t")]
    pub restore_session: bool,
    /// Opening an archive shows what's inside (as a folder).
    #[serde(default = "t")]
    pub browse_archives: bool,
    #[serde(default)]
    pub list_thumbnails: bool,
    /// View Options saved with "Use as Defaults" (a FolderView without a view).
    #[serde(default)]
    pub view_defaults: serde_json::Value,
    /// Toolbar items in order (Customize Toolbar); empty = the default set.
    #[serde(default)]
    pub toolbar: Vec<String>,
    /// Sidebar entries the user hid (paths, "tag:Red", "smart:Name"...).
    #[serde(default)]
    pub hidden_places: Vec<String>,
    /// Keyboard shortcuts changed from the defaults: {action id: [keys]}.
    #[serde(default)]
    pub shortcuts: serde_json::Value,
    /// Grove Archiver, Extract Here: "beside" (next to the archive) or "ask".
    #[serde(default = "beside_default")]
    pub extract_location: String,
    /// Extracting: "auto" (a folder unless it's all in one already),
    /// "always", "never".
    #[serde(default = "auto_default")]
    pub extract_folder: String,
    /// Show what was extracted (or the new archive) in grove afterwards.
    #[serde(default = "t")]
    pub archive_reveal: bool,
    /// Move the archive to the Trash after extracting it all.
    #[serde(default)]
    pub archive_trash_after: bool,
    /// Add to Archive's starting format and level (7-Zip's 0-9).
    #[serde(default = "seven_default")]
    pub archive_format: String,
    #[serde(default = "level_default")]
    pub archive_level: u32,
}

fn beside_default() -> String {
    "beside".into()
}
fn auto_default() -> String {
    "auto".into()
}
fn seven_default() -> String {
    "7z".into()
}
fn level_default() -> u32 {
    5
}

fn default_columns() -> Vec<String> {
    ["modified", "size", "kind", "tags"].iter().map(|s| s.to_string()).collect()
}

impl Default for Settings {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}

pub fn path() -> PathBuf {
    util::grove_config().join("settings.json")
}

pub fn default_favourites() -> Vec<String> {
    let h = util::home();
    let mut v = vec![h.to_string_lossy().into_owned()];
    for d in ["Desktop", "Documents", "Downloads", "Pictures", "Music", "Videos"] {
        let p = h.join(d);
        if p.is_dir() {
            v.push(p.to_string_lossy().into_owned());
        }
    }
    v
}

pub fn load() -> Settings {
    let mut s: Settings = std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    if s.favourites.is_empty() && !path().exists() {
        s.favourites = default_favourites();
    }
    s
}

pub fn save(s: &Settings) -> Result<(), String> {
    util::write_atomic(&path(), serde_json::to_string_pretty(s).unwrap().as_bytes()).map_err(|e| e.to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct FolderView {
    #[serde(default)]
    pub view: String,
    #[serde(default)]
    pub sort: String,
    #[serde(default)]
    pub descending: bool,
    #[serde(default)]
    pub icon_size: u32,
    #[serde(default)]
    pub group: String,
    /// Icon view options (Finder's View Options, Ctrl+J). 0 / "" = default.
    #[serde(default)]
    pub grid_spacing: u32,
    #[serde(default)]
    pub text_size: u32,
    /// "bottom" (default) or "right".
    #[serde(default)]
    pub label_pos: String,
    /// "" (default), "color:#rrggbb" or "picture:/path/to/image".
    #[serde(default)]
    pub background: String,
    /// A second line under names: size, item count, picture dimensions.
    #[serde(default)]
    pub item_info: bool,
    /// Per-folder previews (Dolphin): None follows the global setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previews: Option<bool>,
    /// Finder's stacks in icon view: "" (off), "kind" or "date".
    #[serde(default)]
    pub stacks: String,
}

fn views_path() -> PathBuf {
    util::grove_state().join("folder-views.json")
}

pub fn folder_views() -> HashMap<String, FolderView> {
    std::fs::read_to_string(views_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_folder_views(m: &HashMap<String, FolderView>) {
    let _ = util::write_atomic(&views_path(), serde_json::to_string(m).unwrap().as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_roundtrip() {
        let _g = crate::test_env_lock();
        let t = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CONFIG_HOME", t.path());
        let s = load();
        assert!(s.folders_first && s.per_folder_views && !s.show_hidden);
        assert_eq!(s.default_view, "icons");
        let mut s2 = s.clone();
        s2.show_hidden = true;
        save(&s2).unwrap();
        assert!(load().show_hidden);
        std::env::remove_var("XDG_CONFIG_HOME");
    }
}
