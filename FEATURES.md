# Finder + Dolphin parity

grove aims to have everything Finder and Dolphin have. Status: ✅ done,
🟡 partial (note says what's missing), ❌ not yet. "F" = Finder has it,
"D" = Dolphin has it.

## Browsing and views

| Feature | F | D | grove |
|---|---|---|---|
| Icon view with zoom slider / Ctrl+wheel | ✅ | ✅ | ✅ |
| An icon per file type | ✅ | theme | ✅ grove's own (scripts/gen-filetype-icons.py): a page in the type's colour with a picture of the kind of file and, from 40 px up, the extension or type name written on it (JPEG, RUST, ZIP); ~770 extensions, names (Makefile, LICENSE) and content types; anything else is a grey page with its extension. Folders keep the icon theme's |
| List/details view, sortable columns | ✅ | ✅ | ✅ |
| Expandable folders in list view | ✅ | ✅ | ✅ (triangles, Right/Left) |
| Choosable columns (dates, size, kind, tags, rating, permissions, owner, group, path) | ✅ | ✅ | ✅ right-click the header; also Dimensions (pictures, video) and Duration (video, audio), read off the UI thread |
| Column view (Miller columns) + preview column | ✅ | – | ✅ |
| Gallery view (big preview + filmstrip) | ✅ | – | ✅ |
| Per-folder view memory (view, sort, icon size, groups) | ✅ | ✅ | ✅ |
| Group by kind/date/size/name/tags | ✅ | ✅ | ✅ list and icon view, with headings and counts |
| Stacks | ✅ | – | ✅ icon view, by kind (Finder's families) or date; open in place; View › Use Stacks (Ctrl+Alt+0), off by default |
| View options: grid spacing, label position, background | ✅ | – | ✅ View Options (Ctrl+J): icon size, grid spacing, text size, label bottom/right, item info, previews, background colour/picture, Use as Defaults; per folder |
| Sort by name/kind/date modified/created/added/last opened/size | ✅ | ✅ | ✅ ("Date Added" uses the inode change time) |
| Folders first | – | ✅ | ✅ |
| Hidden files toggle (Ctrl+H, Ctrl+Shift+.) | ✅ | ✅ | ✅ |
| Previews (thumbnails), freedesktop cache shared with other apps | ✅ | ✅ | ✅ built-in images/PDF + system thumbnailers; prioritized, cancelled when scrolled away |
| Previews per folder toggle | – | ✅ | ✅ View Options › Show icon preview (Settings has the default) |
| 100 000-item folders | ✅ | ✅ | ✅ streamed listing, virtualized views (see README) |
| Tabs, drag files onto tabs | ✅ | ✅ | ✅ |
| Drag tabs between windows | ✅ | ✅ | ✅ drag to reorder, onto another window's tabs, or away to tear off a new window; folder, history, view and split go along |
| Split view (F3), drag between panes, copy/move to other side | – | ✅ | ✅ (Shift+F5 / Shift+F6) |
| Multiple windows | ✅ | ✅ | ✅ |
| Session restore of windows and tabs | ✅ | ✅ | ✅ |
| Keyboard-only navigation, type-to-select | ✅ | ✅ | ✅ |
| Selection mode (checkboxes) | – | ✅ | ✅ toolbar, menu or Ctrl+Shift+S; clicks and Space tick items; a bar with actions; Esc leaves |
| Spring-loaded folders when dragging | ✅ | – | ✅ |
| Status bar with selection summary, free space | – | ✅ | ✅ summary and “… available” |

## Sidebar and panels

| Feature | F | D | grove |
|---|---|---|---|
| Favourites/Places, drag to add, reorder, remove | ✅ | ✅ | ✅ |
| Hidden places entries | – | ✅ | ✅ right-click › Hide; right-click a heading › Show Hidden Entries / Unhide All |
| Recents | ✅ | ✅ | ✅ (shared recently-used.xbel) |
| Locations: home, computer, disks with capacity + eject, unmounted disks (click to mount) | ✅ | ✅ | ✅ (udisks2 over D-Bus) |
| Network: saved servers, connected shares, mDNS discovery | ✅ | ✅ | ✅ |
| Tags in the sidebar | ✅ | ✅ | ✅ |
| Smart Folders (saved searches) | ✅ | ✅ | ✅ saves the text, Names/Contents and the kind/date/tag chips; reopening restores the chips |
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
| Metadata filters (kind, date, tag) | ✅ | ✅ | ✅ Kind / Date / Tag chips in the search bar (they work without search text too) |
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
| Burn discs | ✅ | – | ✅ Burn to Disc… (selection, folder, toolbar): xorriso to CD/DVD/BD with a drive picker, disc name, eject, test burn, progress in the jobs list; or a disc image (.iso); no burner: says so and offers the image |
| Compare files | – | ✅ | ✅ (diff view; opens meld/kompare/kdiff3 if installed) |
| Previous versions (Time Machine style, snapper/btrfs snapshots), show + restore | ✅ | – | ✅ |
| btrfs subvolumes: Kind "Subvolume"/"Snapshot" + icon badge, New Subvolume, Take Snapshot (read-only, dated), Restore from Snapshot (current contents kept aside), Delete (root through pkexec when the disk needs it; snapper's snapshots left to snapper) | – | – | ✅ (kernel ioctls, no btrfs tool) |

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
| Markup (draw/annotate), trim video | ✅ | – | ✅ Quick Look: pen, highlighter, shapes, arrows, text, crop, rotate; trim video/audio (ffmpeg stream copy when the cut starts on a keyframe, exact re-encode otherwise); Save (original to the Trash, Undo) or Save as Copy |
| Tooltip previews | – | ✅ | ✅ Settings › Previews in tooltips: preview, kind, size, dimensions, date (icon and list views) |

## Grove Preview (macOS Preview; replaces Okular and Gwenview)

`grove --preview FILES` (grove-preview.desktop for PDFs and pictures). Rust + MuPDF + libcups; pictures through the `image` crate (libvips for HEIC, AVIF, RAW...).

| Feature | Preview | grove |
|---|---|---|
| Several documents in one window, sidebar of thumbnails | ✅ | ✅ drag pages to reorder or into another PDF; drop files to insert |
| EPUB, MOBI, FB2, XPS, comics (CBZ/CBR/CB7/CBT), DjVu, PostScript/EPS, Office documents | 🟡 | ✅ read-only: shown, searched, printed and exported as PDF from a PDF made once (Quick Look's conversions; Office through LibreOffice) |
| Table of contents, highlights and notes, bookmarks, contact sheet | ✅ | ✅ |
| Continuous, single page, two pages; actual size, fit, zoom, pinch | ✅ | ✅ |
| Search with results list, text selection, copy, links | ✅ | ✅ |
| Markup: sketch, highlighter, shapes (line, arrow, rectangle, rounded, oval, speech bubble, star, polygon, loupe), text, notes, signatures | ✅ | ✅ select, move, resize, line width, border/fill colour, font; saved as real PDF annotations |
| Highlight, underline, strikethrough text | ✅ | ✅ |
| Fill in PDF forms | ✅ | ✅ text, check boxes, radio buttons, lists |
| Rotate, delete, insert (file or blank), crop pages | ✅ | ✅ |
| Pictures: Instant Alpha, Adjust Color (levels, exposure, contrast, highlights, shadows, saturation, temperature, tint, sepia, sharpness), Adjust Size, crop, rotate, flip | ✅ | ✅ |
| Save, Duplicate, Revert, Export (PDF with password, PNG, JPEG, WebP, TIFF, BMP, GIF), pictures to one PDF | ✅ | ✅ Save puts the previous version in the Trash |
| Print (printer, copies, pages, paper, layout, two-sided, colour) | ✅ | ✅ CUPS |
| Inspector, editable PDF info, slideshow | ✅ | ✅ |

## Grove Player (QuickTime Player; libmpv)

`grove --play FILES|URLS` (grove-player.desktop for video files), its own process. libmpv, driven from Rust (src/mpv.rs); the picture through mpv's OpenGL render API (cpp/mpv_gl.cpp, the only C++).

| Feature | QuickTime | grove |
|---|---|---|
| Borderless window that is all picture, controls fade in on movement | ✅ | ✅ glass control bar, title in a fading top strip |
| Play/pause, ±15 s, scrubber with times, volume | ✅ | ✅ chapter ticks, buffered part for streams, time under the pointer |
| Sound output picker (AirPlay) | ✅ | ✅ every PipeWire/Pulse output |
| Picture in picture, full screen, Float on Top | ✅ | ✅ |
| Actual Size, Fit, Fill | ✅ | ✅ |
| Formats | 🟡 Apple's | ✅ everything FFmpeg reads (MKV, WebM, AVI, HEVC, AV1, VP9, ...), hardware decoding (VA-API) |
| Music with cover art | ✅ | ✅ title, artist, album |
| Subtitles (embedded and beside the file), audio languages | ✅ | ✅ track picker, Add Subtitle File |
| Open Location (URLs) | ✅ | ✅ YouTube and the other yt-dlp sites, streams |
| DVD and Blu-ray folders and images | ❌ | ✅ through libdvdnav/libbluray |
| Trim (yellow handles), Split Clip, delete clips, Rotate, Flip, Undo | ✅ | ✅ shown at once (mpv EDL); not on URLs or discs |
| Export As 4K/1080p/720p/480p/Audio Only | ✅ | ✅ in the background with progress and Cancel (H.264/AAC MP4, AAC M4A) through mpv's encoder |
| New Audio Recording | ✅ | ✅ microphone to Music as M4A |
| New Screen Recording | ✅ | ✅ opens hearth's recorder |
| Movie Inspector | ✅ | ✅ codecs, resolution, frame rate, data rates, colour, hardware decoding |
| Remembers where you stopped | ✅ | ✅ |

## Grove Disk Utility (GParted's features; replaces GParted / KDE Partition Manager)

| Feature | Status |
|---|---|
| Disks with their partition map drawn to scale (used space, logical partitions inside the extended one), partition table | done |
| New, delete, resize/move, format, label, GPT name, flags, new partition table (GPT/MBR), check and repair, copy/paste | done |
| Operations queued and shown as they will be; Undo, Clear, Apply in order with progress | done |
| Mount / unmount, information | done |
| The system's disks (/, /home, /boot, /nix, swap) locked until unlocked | done |
| Format… a removable drive (name + exFAT / FAT32 / NTFS / ext4 / Btrfs, one confirm) from the sidebar | done |
| Moving the start of a logical (MBR) partition | not yet |

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
| Customizable toolbar | ✅ | ✅ | ✅ Customize Toolbar… (right-click the toolbar): drag items, space, flexible space; Restore Defaults |
| Customizable shortcuts | – | ✅ | ✅ Settings › Keyboard: every action, click to record, conflict warnings, reset one or all; Help and menus show your bindings |
| Help with searchable shortcuts, tooltips everywhere, first-run welcome | 🟡 | 🟡 | ✅ |
