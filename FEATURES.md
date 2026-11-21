# Finder + Dolphin parity

grove aims to have everything Finder and Dolphin have. Status: ✅ done,
🟡 partial (note says what's missing), ❌ not yet. "F" = Finder has it,
"D" = Dolphin has it.

## Browsing and views

| Feature | F | D | grove |
|---|---|---|---|
| Icon view with zoom slider / Ctrl+wheel | ✅ | ✅ | ✅ |
| List/details view, sortable columns | ✅ | ✅ | ✅ |
| Expandable folders in list view | ✅ | ✅ | ✅ (triangles, Right/Left) |
| Choosable columns (dates, size, kind, tags, rating, permissions, owner, group, path) | ✅ | ✅ | ✅ right-click the header; 🟡 no image dimensions / media duration columns |
| Column view (Miller columns) + preview column | ✅ | – | ✅ |
| Gallery view (big preview + filmstrip) | ✅ | – | ✅ |
| Per-folder view memory (view, sort, icon size, groups) | ✅ | ✅ | ✅ |
| Group by kind/date/size/name/tags | ✅ | ✅ | 🟡 list view only (icon view has no group headers yet) |
| Stacks | ✅ | – | ❌ |
| View options: grid spacing, label position, background | ✅ | – | ❌ (icon size only) |
| Sort by name/kind/date modified/created/added/last opened/size | ✅ | ✅ | ✅ ("Date Added" uses the inode change time) |
| Folders first | – | ✅ | ✅ |
| Hidden files toggle (Ctrl+H, Ctrl+Shift+.) | ✅ | ✅ | ✅ |
| Previews (thumbnails), freedesktop cache shared with other apps | ✅ | ✅ | ✅ built-in images/PDF + system thumbnailers; prioritized, cancelled when scrolled away |
| Previews per folder toggle | – | ✅ | 🟡 global setting |
| 100 000-item folders | ✅ | ✅ | ✅ streamed listing, virtualized views (see README) |
| Tabs, drag files onto tabs | ✅ | ✅ | ✅ |
| Drag tabs between windows | ✅ | ✅ | ❌ |
| Split view (F3), drag between panes, copy/move to other side | – | ✅ | ✅ (Shift+F5 / Shift+F6) |
| Multiple windows | ✅ | ✅ | ✅ |
| Session restore of windows and tabs | ✅ | ✅ | ✅ |
| Keyboard-only navigation, type-to-select | ✅ | ✅ | ✅ |
| Selection mode (checkboxes) | – | ✅ | ❌ (Ctrl/Shift-click and rubber band instead) |
| Spring-loaded folders when dragging | ✅ | – | ✅ |
| Status bar with selection summary, free space | – | ✅ | 🟡 summary yes; free space shows in the sidebar capacity bars and Get Info |

## Sidebar and panels

| Feature | F | D | grove |
|---|---|---|---|
| Favourites/Places, drag to add, reorder, remove | ✅ | ✅ | ✅ |
| Hidden places entries | – | ✅ | ❌ |
| Recents | ✅ | ✅ | ✅ (shared recently-used.xbel) |
| Locations: home, computer, disks with capacity + eject, unmounted disks (click to mount) | ✅ | ✅ | ✅ (udisks2 over D-Bus) |
| Network: saved servers, connected shares, mDNS discovery | ✅ | ✅ | ✅ |
| Tags in the sidebar | ✅ | ✅ | ✅ |
| Smart Folders (saved searches) | ✅ | ✅ | ✅ 🟡 criteria: name/content/kind/date/tag in the query format; the UI saves name/content searches |
| Information panel / preview pane (F11, Ctrl+Shift+P) | ✅ | ✅ | ✅ with ratings |
| Folders tree panel (F7) | – | ✅ | ✅ |
| Terminal panel (F4) following the folder | – | ✅ | ✅ built-in emulator (alacritty_terminal) |

## Search and filter

| Feature | F | D | grove |
|---|---|---|---|
| Filter bar (Ctrl+I) | – | ✅ | ✅ |
| Search by name, streaming results | ✅ | ✅ | ✅ (parallel walk, `ignore` crate) |
| Search in contents | ✅ | ✅ | ✅ (ripgrep's searcher) |
| Scopes: home / this folder / computer | ✅ | ✅ | ✅ |
| Metadata filters (kind, date, tag) | ✅ | ✅ | 🟡 in the engine and Smart Folders; no filter chips in the search bar yet |
| Show Enclosing Folder | ✅ | ✅ | ✅ |

## File operations

| Feature | F | D | grove |
|---|---|---|---|
| Copy/move/alias(link)/duplicate with progress, pause, cancel | ✅ | ✅ | ✅ |
| Conflicts: replace / skip / keep both / merge, apply to all | ✅ | ✅ | ✅ (replaced items go to the Trash, so Undo works) |
| Undo/redo for move, rename, trash, new folder/file, copy, extract, compress | ✅ | ✅ | ✅ |
| Drag and drop with other apps (text/uri-list) | ✅ | ✅ | ✅ |
| Cut/copy/paste, interoperable clipboard (GNOME + KDE cut markers) | ✅ | ✅ | ✅ |
| Trash: per-disk trash dirs, put back, empty with confirmation | ✅ | ✅ | ✅ (freedesktop spec) |
| New folder, new file, templates (~/Templates) | ✅ | ✅ | ✅ |
| Inline rename (name selected without extension) | ✅ | ✅ | ✅ |
| Batch rename (replace, add text, numbering, case, preview) | ✅ | ✅ | ✅ |
| Copy path | ✅ | ✅ | ✅ |
| Compress / extract, archives as folders | ✅ | ✅ | ✅ (bsdtar: zip, tar.*, 7z, rar, iso...) |
| Open as administrator | – | ✅ | ✅ (pkexec) |
| Mount ISO / disk images | ✅ | ✅ | ✅ (udisks loop-setup, read-only) |
| Burn discs | ✅ | – | ❌ |
| Compare files | – | ✅ | ✅ (diff view; opens meld/kompare/kdiff3 if installed) |
| Previous versions (Time Machine style, snapper/btrfs snapshots), show + restore | ✅ | – | ✅ |

## Information

| Feature | F | D | grove |
|---|---|---|---|
| Get Info / Properties window | ✅ | ✅ | ✅ |
| Folder size (async) and item count | ✅ | ✅ | ✅ |
| Dates (created, modified, opened) | ✅ | ✅ | ✅ |
| Permissions and owner, editable (polkit when needed) | ✅ | ✅ | ✅ |
| Locked flag | ✅ | – | ✅ (removes write permission) |
| Comments (user.xdg.comment) | ✅ | ✅ | ✅ |
| Tags with colours + names (user.xdg.tags, shared with Dolphin) | ✅ | ✅ | ✅ |
| Ratings (user.baloo.rating) | – | ✅ | ✅ |
| Open With + "Change All" (mimeapps.list) | ✅ | ✅ | ✅ |
| Checksums MD5/SHA-1/SHA-256/SHA-512 with verify | – | ✅ | ✅ |
| Custom folder icons (.directory Icon=, Dolphin-compatible) | ✅ | ✅ | ✅ 🟡 by icon name or image path; no file-icon pasting |

## Quick Look and previews

| Feature | F | D | grove |
|---|---|---|---|
| Space to preview, arrows to move | ✅ | – | ✅ |
| Images incl. HEIC/AVIF/RAW (via thumbnailers), video/audio, PDF, text/code with highlighting, Markdown, archives, folders | ✅ | 🟡 | ✅ |
| Markup (draw/annotate), trim video | ✅ | – | ❌ (rotate/convert images are Quick Actions) |
| Tooltip previews | – | ✅ | ❌ |

## Integration

| Feature | F | D | grove |
|---|---|---|---|
| Default file manager (inode/directory), org.freedesktop.FileManager1 | ✅ | ✅ | ✅ (ShowFolders/ShowItems/ShowItemProperties) |
| Single instance (second launch opens a tab) | ✅ | ✅ | ✅ |
| Open/save dialog for every app (xdg-desktop-portal FileChooser) | ✅ | ✅ | ✅ `grove --portal` |
| Open With from desktop entries | ✅ | ✅ | ✅ |
| Open in Terminal (tessera, konsole, ...) | ✅ | ✅ | ✅ |
| Service menus / Quick Actions | ✅ | ✅ | ✅ reads Dolphin service menus; built-in rotate/convert/compress/root |
| Version control overlays + actions (git: add, commit, pull, push, diff, log) | – | ✅ | ✅ |
| Remote: SFTP, SMB, WebDAV, cloud (rclone), MTP phones | ✅ | ✅ | ✅ (sshfs, rclone, simple-mtpfs; passwords in the Secret Service keyring) |
| Share with nearby devices (AirDrop) | ✅ | – | ✅ LocalSend protocol (send; receiving is left to the LocalSend app) |
| Customizable toolbar | ✅ | ✅ | ❌ |
| Customizable shortcuts | – | ✅ | ❌ (Finder and Dolphin shortcuts both built in) |
| Help with searchable shortcuts, tooltips everywhere, first-run welcome | 🟡 | 🟡 | ✅ |
