pragma Singleton
import QtQuick

// Every action and shortcut, for tooltips and the searchable Help window.
// `keys` is grove's shortcut (Ctrl stands where a Mac has ⌘); `finder` and
// `dolphin` say where it comes from (both work unless noted).
QtObject {
    // Every action: `defaults` are its shortcuts (Ctrl stands where a Mac has
    // ⌘), changed in Settings › Keyboard (saved as settings.shortcuts
    // {id: [keys]}); `fixed` ones (typing, Return, Space, Esc, Tab) can't be.
    readonly property var shortcuts: [
        { id: "back", group: "Navigate", action: "Back", defaults: ["Alt+Left", "Ctrl+["], fixed: "", note: "Finder: ⌘[  ·  Dolphin: Alt+Left. Backspace also goes back" },
        { id: "forward", group: "Navigate", action: "Forward", defaults: ["Alt+Right", "Ctrl+]"], fixed: "", note: "Finder: ⌘]  ·  Dolphin: Alt+Right" },
        { id: "up", group: "Navigate", action: "Enclosing folder", defaults: ["Alt+Up", "Ctrl+Up"], fixed: "", note: "Finder: ⌘↑  ·  Dolphin: Alt+Up" },
        { id: "open", group: "Navigate", action: "Open", defaults: ["Ctrl+Down"], fixed: "", note: "Return and double-click always open. Finder: ⌘↓ (Return renames in Finder; here F2 renames)" },
        { id: "gotofolder", group: "Navigate", action: "Go to folder / edit the path", defaults: ["Ctrl+L", "Ctrl+Shift+G", "F6"], fixed: "", note: "Dolphin: Ctrl+L  ·  Finder: ⇧⌘G" },
        { id: "home", group: "Navigate", action: "Home", defaults: ["Ctrl+Shift+H", "Alt+Home"], fixed: "", note: "Finder: ⇧⌘H  ·  Dolphin: Alt+Home" },
        { id: "desktop", group: "Navigate", action: "Desktop", defaults: ["Ctrl+Shift+D"], fixed: "", note: "Finder: ⇧⌘D" },
        { id: "documents", group: "Navigate", action: "Documents", defaults: ["Ctrl+Shift+O"], fixed: "", note: "Finder: ⇧⌘O" },
        { id: "downloads", group: "Navigate", action: "Downloads", defaults: ["Ctrl+Alt+L"], fixed: "", note: "Finder: ⌥⌘L" },
        { id: "recents", group: "Navigate", action: "Recents", defaults: ["Ctrl+Shift+F"], fixed: "", note: "Finder: ⇧⌘F" },
        { id: "computer", group: "Navigate", action: "Computer", defaults: ["Ctrl+Shift+C"], fixed: "", note: "Finder: ⇧⌘C" },
        { id: "trash", group: "Navigate", action: "Trash", defaults: ["Ctrl+Shift+T"], fixed: "", note: "" },
        { id: "archiveselect", group: "Navigate", action: "Select an archive without opening it (column view: shows its info)", defaults: [], fixed: "Alt+click", note: "Archives open like folders" },
        { id: "snapshots", group: "Navigate", action: "Snapshots: this folder as it was in each btrfs snapshot (read-only)", defaults: ["Ctrl+Alt+Y"], fixed: "", note: "Like Time Machine; snapper's hourly snapshots" },
        { id: "diskutility", group: "Navigate", action: "Grove Disk Utility (partitions, formatting)", defaults: ["Ctrl+Alt+D"], fixed: "", note: "Like GParted; also Go › Disk Utility…" },
        { id: "diskusage", group: "Navigate", action: "Disk Usage (what takes the room)", defaults: ["Ctrl+Shift+U"], fixed: "", note: "Like KDirStat / Filelight" },
        { id: "connect", group: "Navigate", action: "Connect to Server\u2026", defaults: ["Ctrl+K"], fixed: "", note: "Finder: ⌘K" },
        { id: "reload", group: "Navigate", action: "Reload", defaults: ["F5", "Ctrl+R"], fixed: "", note: "Dolphin: F5" },
        { id: "typeselect", group: "Navigate", action: "Select items by typing their name", defaults: [], fixed: "letters", note: "Both" },

        { id: "viewicons", group: "View", action: "Icons", defaults: ["Ctrl+1"], fixed: "", note: "Finder: ⌘1" },
        { id: "viewlist", group: "View", action: "List", defaults: ["Ctrl+2"], fixed: "", note: "Finder: ⌘2" },
        { id: "viewcolumns", group: "View", action: "Columns", defaults: ["Ctrl+3"], fixed: "", note: "Finder: ⌘3" },
        { id: "viewgallery", group: "View", action: "Gallery", defaults: ["Ctrl+4"], fixed: "", note: "Finder: ⌘4" },
        { id: "zoomin", group: "View", action: "Bigger icons", defaults: ["Ctrl++", "Ctrl+="], fixed: "", note: "Both" },
        { id: "zoomout", group: "View", action: "Smaller icons", defaults: ["Ctrl+-"], fixed: "", note: "Both" },
        { id: "usage", group: "View", action: "Show disk usage (a bar and size for every item)", defaults: ["Ctrl+Alt+U"], fixed: "", note: "Sizes are worked out in the background; the darker part of a bar is what deleting it would free" },
        { id: "hidden", group: "View", action: "Show/hide hidden files", defaults: ["Ctrl+H", "Ctrl+Shift+."], fixed: "", note: "Dolphin: Ctrl+H  ·  Finder: ⇧⌘." },
        { id: "viewoptions", group: "View", action: "View Options (spacing, text size, labels, background)", defaults: ["Ctrl+J"], fixed: "", note: "Finder: ⌘J" },
        { id: "stacks", group: "View", action: "Use Stacks (icon view)", defaults: ["Ctrl+Alt+0"], fixed: "", note: "Finder: ⌃⌘0. Click a stack to open it in place" },
        { id: "filter", group: "View", action: "Filter this folder", defaults: ["Ctrl+I"], fixed: "", note: "Dolphin's filter bar (Finder uses ⌘I for Get Info: here Alt+Return)" },
        { id: "quicklook", group: "View", action: "Quick Look", defaults: ["Ctrl+Y"], fixed: "", note: "Space always works too. Finder: Space, ⌘Y. Arrow keys move between files while it's open" },
        { id: "previewsideways", group: "View", action: "Scroll a preview sideways (long lines, wide tables, zoomed pages)", defaults: [], fixed: "Shift+wheel", note: "Trackpad: swipe sideways. Text: the Wrap button wraps long lines instead; Markdown: Rendered / Source" },
        { id: "syntaxlang", group: "View", action: "Text preview: choose the syntax colouring language", defaults: [], fixed: "click the language in the viewer's bar", note: "Type to narrow the list, Return picks. Remembered for that file (it follows the file when renamed or moved); Automatic goes back to the guess" },
        { id: "pdfzoom", group: "View", action: "Zoom PDF and book pages", defaults: [], fixed: "Ctrl+wheel, Ctrl+= / Ctrl+−, Ctrl+0", note: "Click the percentage to fit the width again" },
        { id: "videofull", group: "View", action: "Video full screen", defaults: [], fixed: "double-click the video", note: "Esc, F or double-click to go back" },
        { id: "qltablesearch", group: "View", action: "Quick Look on a table (CSV, TSV, spreadsheet): search its rows", defaults: [], fixed: "Ctrl+F", note: "Click a header to sort, drag it to move the column, drag its edge to resize, right-click it to filter or hide; SQL runs queries (table t)" },
        { id: "treeview", group: "View", action: "Data file preview (JSON, YAML, TOML, XML, INI): Tree or Source", defaults: [], fixed: "the Tree / Source switch in the viewer's bar", note: "Remembered. A file that doesn't parse shows its source with where it went wrong (Show Line)" },
        { id: "treenav", group: "View", action: "Tree preview: move, open and close rows", defaults: [], fixed: "↑ ↓, → opens, ← closes (or goes to the container)", note: "Click the triangle or double-click a row; Alt+→ / Alt+← or Alt-click: everything inside too; * opens everything inside" },
        { id: "treeall", group: "View", action: "Tree preview: expand all / collapse all", defaults: [], fixed: "Ctrl+Shift+→ / Ctrl+Shift+←", note: "Expand All opens every level that fits (huge files say how far it went)" },
        { id: "treesearch", group: "View", action: "Tree preview: search keys and values", defaults: [], fixed: "Ctrl+F", note: "Opens the containers the matches are in; Return / Shift+Return next and previous, Esc clears" },
        { id: "treecopy", group: "View", action: "Tree preview: copy the value / its path", defaults: [], fixed: "Ctrl+C / Ctrl+Shift+C", note: "Paths like servers[2].host (XPath for XML: /config/server[2]/@host). Right-click a row for Copy Key too" },
        { id: "tablecopy", group: "View", action: "Table preview: copy the selected cells, rows or columns (tab-separated)", defaults: [], fixed: "Ctrl+C", note: "Drag over cells or the row numbers to select (Shift-click extends), Ctrl-click a header for its column; Ctrl+A selects all; right-click for Copy with Column Names; double-click a cell to see all of it" },
        { id: "split", group: "View", action: "Split view", defaults: ["F3"], fixed: "", note: "Dolphin: F3" },
        { id: "terminalpanel", group: "View", action: "Terminal panel", defaults: ["F4"], fixed: "", note: "Dolphin: F4" },
        { id: "infopanel", group: "View", action: "Information panel / preview pane", defaults: ["F11", "Ctrl+Shift+P"], fixed: "", note: "Dolphin: F11  ·  Finder: ⇧⌘P" },
        { id: "folderspanel", group: "View", action: "Folders panel (tree)", defaults: ["F7"], fixed: "", note: "Dolphin: F7" },
        { id: "sidebar", group: "View", action: "Sidebar", defaults: ["Ctrl+Alt+S", "F9"], fixed: "", note: "Finder: ⌃⌘S  ·  Dolphin: F9" },
        { id: "expand", group: "View", action: "Expand / collapse folder (list view)", defaults: [], fixed: "Right / Left", note: "Finder" },

        { id: "newwindow", group: "Tabs & windows", action: "New window", defaults: ["Ctrl+N"], fixed: "", note: "Both" },
        { id: "newtab", group: "Tabs & windows", action: "New tab", defaults: ["Ctrl+T"], fixed: "", note: "Both" },
        { id: "closetab", group: "Tabs & windows", action: "Close tab", defaults: ["Ctrl+W"], fixed: "", note: "Both" },
        { id: "nexttab", group: "Tabs & windows", action: "Next tab", defaults: ["Ctrl+Tab"], fixed: "", note: "Both" },
        { id: "prevtab", group: "Tabs & windows", action: "Previous tab", defaults: ["Ctrl+Shift+Tab", "Ctrl+Backtab"], fixed: "", note: "Both" },
        { id: "switchpane", group: "Tabs & windows", action: "Switch split pane", defaults: [], fixed: "Tab", note: "When the view has focus" },
        { id: "dragtab", group: "Tabs & windows", action: "Move a tab to another window", defaults: [], fixed: "drag the tab", note: "Finder and Dolphin. Drop it away from any tab bar to open it in a new window; its folder, history and view go with it" },

        { id: "copy", group: "Edit", action: "Copy", defaults: ["Ctrl+C"], fixed: "", note: "Both" },
        { id: "cut", group: "Edit", action: "Cut (move on paste)", defaults: ["Ctrl+X"], fixed: "", note: "Dolphin (Finder: ⌥⌘V moves)" },
        { id: "paste", group: "Edit", action: "Paste", defaults: ["Ctrl+V"], fixed: "", note: "Both" },
        { id: "movehere", group: "Edit", action: "Move here (paste and remove the originals)", defaults: ["Ctrl+Alt+V"], fixed: "", note: "Finder: ⌥⌘V" },
        { id: "copypath", group: "Edit", action: "Copy path", defaults: ["Ctrl+Alt+C"], fixed: "", note: "Finder: ⌥⌘C" },
        { id: "copyother", group: "Edit", action: "Copy to the other side (split view)", defaults: ["Shift+F5"], fixed: "", note: "Dolphin: Copy to Other View" },
        { id: "moveother", group: "Edit", action: "Move to the other side (split view)", defaults: ["Shift+F6"], fixed: "", note: "Dolphin: Move to Other View" },
        { id: "duplicate", group: "Edit", action: "Duplicate", defaults: ["Ctrl+D"], fixed: "", note: "Finder: ⌘D" },
        { id: "alias", group: "Edit", action: "Make alias (link)", defaults: ["Ctrl+Alt+A"], fixed: "", note: "Finder: ⌃⌘A" },
        { id: "rename", group: "Edit", action: "Rename", defaults: ["F2"], fixed: "", note: "Dolphin: F2 (several items: batch rename)" },
        { id: "batchrename", group: "Edit", action: "Batch rename", defaults: ["Shift+F2"], fixed: "", note: "" },
        { id: "newfolder", group: "Edit", action: "New folder", defaults: ["Ctrl+Shift+N", "F10"], fixed: "", note: "Finder: ⇧⌘N  ·  Dolphin: F10" },
        { id: "newfile", group: "Edit", action: "New file", defaults: ["Ctrl+Alt+N"], fixed: "", note: "" },
        { id: "newsubvol", group: "Edit", action: "New btrfs subvolume (a folder that can be snapshotted on its own)", defaults: ["Ctrl+Alt+Shift+N"], fixed: "", note: "Only on btrfs disks" },
        { id: "snapshot", group: "Edit", action: "Take a snapshot of the selected subvolume (or the one shown)", defaults: ["Ctrl+Alt+Shift+S"], fixed: "", note: "Read-only and named with the date by default" },
        { id: "movetotrash", group: "Edit", action: "Move to Trash", defaults: ["Delete", "Ctrl+Backspace"], fixed: "", note: "Dolphin: Del  ·  Finder: ⌘⌫" },
        { id: "delete", group: "Edit", action: "Delete permanently", defaults: ["Shift+Delete"], fixed: "", note: "Dolphin: Shift+Del (asks first)" },
        { id: "undo", group: "Edit", action: "Undo", defaults: ["Ctrl+Z"], fixed: "", note: "Both" },
        { id: "redo", group: "Edit", action: "Redo", defaults: ["Ctrl+Shift+Z"], fixed: "", note: "Both" },
        { id: "selectall", group: "Edit", action: "Select all", defaults: ["Ctrl+A"], fixed: "", note: "Both" },
        { id: "invert", group: "Edit", action: "Invert selection", defaults: ["Ctrl+Shift+A"], fixed: "", note: "Dolphin" },
        { id: "selectionmode", group: "Edit", action: "Selection mode", defaults: ["Ctrl+Shift+S"], fixed: "", note: "Dolphin: checkboxes on items. Space ticks the current item, Esc leaves" },
        { id: "clearselection", group: "Edit", action: "Clear selection", defaults: [], fixed: "Escape", note: "Both" },
        { id: "getinfo", group: "Edit", action: "Get Info", defaults: ["Alt+Return", "Ctrl+Shift+I"], fixed: "", note: "Dolphin: Alt+Return (Finder: ⌘I, here the filter bar)" },
        { id: "tagred", group: "Edit", action: "Tag: Red", defaults: ["Ctrl+Alt+1"], fixed: "", note: "Toggle the Red tag on the selection" },
        { id: "tagorange", group: "Edit", action: "Tag: Orange", defaults: ["Ctrl+Alt+2"], fixed: "", note: "Toggle the Orange tag on the selection" },
        { id: "tagyellow", group: "Edit", action: "Tag: Yellow", defaults: ["Ctrl+Alt+3"], fixed: "", note: "Toggle the Yellow tag on the selection" },
        { id: "taggreen", group: "Edit", action: "Tag: Green", defaults: ["Ctrl+Alt+4"], fixed: "", note: "Toggle the Green tag on the selection" },
        { id: "tagblue", group: "Edit", action: "Tag: Blue", defaults: ["Ctrl+Alt+5"], fixed: "", note: "Toggle the Blue tag on the selection" },
        { id: "tagpurple", group: "Edit", action: "Tag: Purple", defaults: ["Ctrl+Alt+6"], fixed: "", note: "Toggle the Purple tag on the selection" },
        { id: "taggray", group: "Edit", action: "Tag: Gray", defaults: ["Ctrl+Alt+7"], fixed: "", note: "Toggle the Gray tag on the selection" },

        { id: "search", group: "Search", action: "Search", defaults: ["Ctrl+F"], fixed: "", note: "Both" },
        { id: "savesearch", group: "Search", action: "Save search as Smart Folder", defaults: [], fixed: "", note: "From the search bar: Kind, Date and Tag chips narrow a search and are saved with it" },

        { id: "terminal", group: "Other", action: "Open in Terminal", defaults: ["Ctrl+Alt+T"], fixed: "", note: "" },
        { id: "eject", group: "Other", action: "Eject", defaults: ["Ctrl+E"], fixed: "", note: "Finder: ⌘E" },
        { id: "settings", group: "Other", action: "Settings", defaults: ["Ctrl+,"], fixed: "", note: "Finder: ⌘," },
        { id: "hideplace", group: "Other", action: "Hide a sidebar entry", defaults: [], fixed: "right-click › Hide", note: "Dolphin. Right-click a section heading › Show Hidden Entries to bring it back" },
        { id: "customizetoolbar", group: "Other", action: "Customize Toolbar…", defaults: [], fixed: "", note: "Finder: right-click the toolbar" },
        { id: "burn", group: "Other", action: "Burn to Disc…", defaults: [], fixed: "", note: "Finder: File › Burn to Disc" },
        { id: "markup", group: "View", action: "Markup (in Quick Look)", defaults: [], fixed: "", note: "Finder: pen, highlighter, shapes, text, crop, rotate" },
        { id: "qlpages", group: "View", action: "Previous / next page (PDFs in Quick Look)", defaults: [], fixed: "Page Up / Page Down", note: "Preview: the page counter's arrows do the same" },
        { id: "arcopen", group: "Grove Archiver", defaults: [], action: "Open an archive", fixed: "Ctrl+O", note: "Grove Archiver shows any archive with 7-Zip's columns. From grove: right-click › Archive › Open in Grove Archiver" },
        { id: "arcup", group: "Grove Archiver", defaults: [], action: "Up one folder inside the archive", fixed: "Backspace, Alt+Up", note: "Return or double-click opens the selected folder on its own" },
        { id: "arcdisclose", group: "Grove Archiver", defaults: [], action: "Expand / collapse a folder in place", fixed: "Right / Left, click its triangle", note: "Alt+Shift+Right/Left or Alt-click the triangle: everything inside too. Ctrl+Alt+Right/Left: every folder (Expand All / Collapse All)" },
        { id: "arcopenfile", group: "Grove Archiver", defaults: [], action: "Open a file from the archive", fixed: "Return, double-click", note: "It's taken out to grove's cache and opened in its app" },
        { id: "arcextract", group: "Grove Archiver", defaults: [], action: "Extract what's selected (or everything) with options", fixed: "Ctrl+E", note: "Where to, make a folder or not, keep folders, files already there, password" },
        { id: "arcextractall", group: "Grove Archiver", defaults: [], action: "Extract everything", fixed: "Ctrl+Shift+E", note: "Extract Here (from grove or the desktop) uses Settings › Archives" },
        { id: "arccopyto", group: "Grove Archiver", defaults: [], action: "Copy the selection to a folder", fixed: "F5", note: "7-Zip's Copy To" },
        { id: "arctest", group: "Grove Archiver", defaults: [], action: "Test the archive", fixed: "Ctrl+T", note: "Checks every file against its stored checksum" },
        { id: "arcadd", group: "Grove Archiver", defaults: [], action: "Add files to the archive", fixed: "Ctrl+Shift+A, Insert, drop files on the window", note: "7z, zip and tar archives (rar, iso and the like can only be read)" },
        { id: "arcdelete", group: "Grove Archiver", defaults: [], action: "Delete from the archive", fixed: "Delete", note: "Removed from the archive only; it can't be undone" },
        { id: "arcrename", group: "Grove Archiver", defaults: [], action: "Rename inside the archive", fixed: "F2", note: "" },
        { id: "arcmkdir", group: "Grove Archiver", defaults: [], action: "New folder inside the archive", fixed: "Ctrl+Shift+N", note: "" },
        { id: "arcinfo", group: "Grove Archiver", defaults: [], action: "Archive info (format, method, solid, encryption, volumes)", fixed: "Ctrl+I, Alt+Return", note: "" },
        { id: "arcfilter", group: "Grove Archiver", defaults: [], action: "Filter by name", fixed: "Ctrl+F", note: "Esc clears it" },
        { id: "arcsort", group: "Grove Archiver", defaults: [], action: "Sort by a column", fixed: "click the column header", note: "Click again to reverse" },
        { id: "arccompress", group: "Grove Archiver", defaults: [], action: "Add to Archive (new archive)", fixed: "right-click › Archive › Add to Archive…", note: "7z, zip, tar.zst/xz/gz/bz2: level, method, solid, cores, split into volumes, password (7z can hide file names; zip AES-256 or ZipCrypto)" },
        { id: "arcpause", group: "Grove Archiver", defaults: [], action: "Pause or cancel a job", fixed: "Pause / Cancel in its window, Esc", note: "Hide keeps it going in the background; the window comes back if it needs you" },
        { id: "pl.open", group: "Grove Player", action: "Open movies and music", defaults: ["Ctrl+O"], fixed: "", note: "QuickTime: ⌘O. Or drop files on the window" },
        { id: "pl.location", group: "Grove Player", action: "Open a web address (YouTube and the other sites yt-dlp knows, streams)", defaults: ["Ctrl+L"], fixed: "", note: "QuickTime: ⌘L" },
        { id: "pl.close", group: "Grove Player", action: "Close the window", defaults: ["Ctrl+W"], fixed: "", note: "Where it was is remembered for next time" },
        { id: "pl.screenrec", group: "Grove Player", action: "New screen recording (hearth's recorder)", defaults: ["Ctrl+Shift+N"], fixed: "", note: "QuickTime: ⌃⌘N" },
        { id: "pl.audiorec", group: "Grove Player", action: "New audio recording (the microphone, into Music)", defaults: ["Ctrl+Alt+N"], fixed: "", note: "QuickTime: ⌥⌘N" },
        { id: "pl.undo", group: "Grove Player", action: "Undo an edit", defaults: ["Ctrl+Z"], fixed: "", note: "" },
        { id: "pl.redo", group: "Grove Player", action: "Redo", defaults: ["Ctrl+Shift+Z"], fixed: "", note: "" },
        { id: "pl.trim", group: "Grove Player", action: "Trim (drag the yellow handles, then Trim)", defaults: ["Ctrl+T"], fixed: "", note: "Return trims, Esc cancels. Export As saves the result" },
        { id: "pl.split", group: "Grove Player", action: "Split the clip at the playhead", defaults: ["Ctrl+Y"], fixed: "", note: "Click a clip and press Delete to remove it" },
        { id: "pl.rotleft", group: "Grove Player", action: "Rotate left", defaults: ["Ctrl+Shift+L"], fixed: "", note: "" },
        { id: "pl.rotright", group: "Grove Player", action: "Rotate right", defaults: ["Ctrl+Shift+R"], fixed: "", note: "" },
        { id: "pl.fliph", group: "Grove Player", action: "Flip horizontal", defaults: ["Ctrl+Shift+H"], fixed: "", note: "" },
        { id: "pl.flipv", group: "Grove Player", action: "Flip vertical", defaults: ["Ctrl+Shift+V"], fixed: "", note: "" },
        { id: "pl.actual", group: "Grove Player", action: "Actual size (one movie pixel per screen pixel)", defaults: ["Ctrl+1"], fixed: "", note: "" },
        { id: "pl.fit", group: "Grove Player", action: "Fit to screen (the whole picture)", defaults: ["Ctrl+3"], fixed: "", note: "" },
        { id: "pl.fill", group: "Grove Player", action: "Fill screen (no black bars, edges cut off)", defaults: ["Ctrl+4"], fixed: "", note: "" },
        { id: "pl.ontop", group: "Grove Player", action: "Float on top", defaults: ["Ctrl+Alt+T"], fixed: "", note: "" },
        { id: "pl.pip", group: "Grove Player", action: "Picture in picture", defaults: ["Ctrl+Shift+P"], fixed: "", note: "A small window in the corner that stays on top; Esc brings it back" },
        { id: "pl.fullscreen", group: "Grove Player", action: "Full screen", defaults: ["Ctrl+F"], fixed: "", note: "Also F, or double-click the picture; Esc leaves" },
        { id: "pl.subs", group: "Grove Player", action: "Show or hide subtitles", defaults: ["Ctrl+Alt+S"], fixed: "", note: "Pick the language from the speech-bubble button or View › Subtitles" },
        { id: "pl.loop", group: "Grove Player", action: "Loop", defaults: ["Ctrl+Alt+L"], fixed: "", note: "" },
        { id: "pl.inspector", group: "Grove Player", action: "Movie Inspector (codec, size, frame rate, data rate)", defaults: ["Ctrl+I"], fixed: "", note: "" },
        { id: "pl.minimize", group: "Grove Player", action: "Minimize", defaults: ["Ctrl+M"], fixed: "", note: "" },
        { id: "pl.play", group: "Grove Player", action: "Play / pause", defaults: [], fixed: "Space, K", note: "Click the picture too" },
        { id: "pl.seek", group: "Grove Player", action: "Back / forward 5 seconds (paused: one frame)", defaults: [], fixed: "← / →", note: "Shift: 15 seconds. J / L: 10 seconds. Alt+← / Alt+→: start / end" },
        { id: "pl.volume", group: "Grove Player", action: "Volume", defaults: [], fixed: "↑ / ↓", note: "M mutes" },
        { id: "pv.open", group: "Grove Preview", action: "Open PDFs and pictures", defaults: ["Ctrl+O"], fixed: "", note: "Preview: ⌘O. Or drop files on the window or its sidebar" },
        { id: "pv.close", group: "Grove Preview", action: "Close the window", defaults: ["Ctrl+W"], fixed: "", note: "Asks to save changes first" },
        { id: "pv.save", group: "Grove Preview", action: "Save", defaults: ["Ctrl+S"], fixed: "", note: "The previous version goes to the Trash" },
        { id: "pv.duplicate", group: "Grove Preview", action: "Duplicate (save a copy and open it)", defaults: ["Ctrl+Shift+S"], fixed: "", note: "Preview: ⇧⌘S" },
        { id: "pv.print", group: "Grove Preview", action: "Print", defaults: ["Ctrl+P"], fixed: "", note: "Printer, copies, pages, paper, two-sided, black and white" },
        { id: "pv.undo", group: "Grove Preview", action: "Undo", defaults: ["Ctrl+Z"], fixed: "", note: "" },
        { id: "pv.redo", group: "Grove Preview", action: "Redo", defaults: ["Ctrl+Shift+Z", "Ctrl+Y"], fixed: "", note: "" },
        { id: "pv.copy", group: "Grove Preview", action: "Copy the selected text or picture", defaults: ["Ctrl+C"], fixed: "", note: "A rectangular selection copies as a picture" },
        { id: "pv.selectall", group: "Grove Preview", action: "Select all (the page's text, or the whole picture)", defaults: ["Ctrl+A"], fixed: "", note: "" },
        { id: "pv.find", group: "Grove Preview", action: "Search", defaults: ["Ctrl+F"], fixed: "", note: "Matches list in the sidebar" },
        { id: "pv.findnext", group: "Grove Preview", action: "Next match", defaults: ["Ctrl+G", "F3"], fixed: "", note: "Return in the search field too" },
        { id: "pv.findprev", group: "Grove Preview", action: "Previous match", defaults: ["Ctrl+Shift+G", "Shift+F3"], fixed: "", note: "" },
        { id: "pv.sidebar", group: "Grove Preview", action: "Show or hide the sidebar", defaults: ["Ctrl+Alt+S"], fixed: "", note: "Preview: ⌥⌘S" },
        { id: "pv.thumbs", group: "Grove Preview", action: "Sidebar: thumbnails", defaults: ["Ctrl+Alt+2"], fixed: "", note: "Drag pages to reorder them or move them to another document" },
        { id: "pv.toc", group: "Grove Preview", action: "Sidebar: table of contents", defaults: ["Ctrl+Alt+3"], fixed: "", note: "" },
        { id: "pv.notes", group: "Grove Preview", action: "Sidebar: highlights and notes", defaults: ["Ctrl+Alt+4"], fixed: "", note: "" },
        { id: "pv.bookmarks", group: "Grove Preview", action: "Sidebar: bookmarks", defaults: ["Ctrl+Alt+5"], fixed: "", note: "" },
        { id: "pv.contact", group: "Grove Preview", action: "Contact sheet", defaults: ["Ctrl+Alt+6"], fixed: "", note: "Every page small" },
        { id: "pv.actual", group: "Grove Preview", action: "Actual size", defaults: ["Ctrl+0"], fixed: "", note: "" },
        { id: "pv.fit", group: "Grove Preview", action: "Zoom to fit", defaults: ["Ctrl+9"], fixed: "", note: "" },
        { id: "pv.zoomin", group: "Grove Preview", action: "Zoom in", defaults: ["Ctrl+=", "Ctrl++"], fixed: "", note: "Ctrl+wheel or pinch too" },
        { id: "pv.zoomout", group: "Grove Preview", action: "Zoom out", defaults: ["Ctrl+-"], fixed: "", note: "" },
        { id: "pv.markup", group: "Grove Preview", action: "Show or hide the Markup toolbar", defaults: ["Ctrl+Shift+A"], fixed: "", note: "Preview: ⇧⌘A" },
        { id: "pv.slideshow", group: "Grove Preview", action: "Slideshow", defaults: ["Ctrl+Shift+F"], fixed: "", note: "←/→ to move, Esc to stop" },
        { id: "pv.goto", group: "Grove Preview", action: "Go to page", defaults: ["Ctrl+Alt+G"], fixed: "", note: "" },
        { id: "pv.prevpage", group: "Grove Preview", action: "Previous page", defaults: ["Alt+Up"], fixed: "", note: "Page Up scrolls a screen" },
        { id: "pv.nextpage", group: "Grove Preview", action: "Next page", defaults: ["Alt+Down"], fixed: "", note: "Page Down or Space scrolls a screen" },
        { id: "pv.prevdoc", group: "Grove Preview", action: "Previous document in the window", defaults: ["Alt+Left"], fixed: "", note: "" },
        { id: "pv.nextdoc", group: "Grove Preview", action: "Next document in the window", defaults: ["Alt+Right"], fixed: "", note: "" },
        { id: "pv.inspector", group: "Grove Preview", action: "Inspector (about the file; PDF title, author, keywords)", defaults: ["Ctrl+I"], fixed: "", note: "Preview: ⌘I" },
        { id: "pv.adjustcolor", group: "Grove Preview", action: "Adjust Color (pictures)", defaults: ["Ctrl+Alt+C"], fixed: "", note: "Preview: ⌥⌘C" },
        { id: "pv.rotleft", group: "Grove Preview", action: "Rotate left", defaults: ["Ctrl+L"], fixed: "", note: "Preview: ⌘L. The selected pages" },
        { id: "pv.rotright", group: "Grove Preview", action: "Rotate right", defaults: ["Ctrl+R"], fixed: "", note: "Preview: ⌘R" },
        { id: "pv.crop", group: "Grove Preview", action: "Crop to the rectangular selection", defaults: ["Ctrl+K"], fixed: "", note: "Preview: ⌘K" },
        { id: "pv.bookmark", group: "Grove Preview", action: "Add or remove a bookmark", defaults: ["Ctrl+D"], fixed: "", note: "Preview: ⌘D" },
        { id: "pv.highlight", group: "Grove Preview", action: "Highlight text", defaults: ["Ctrl+Shift+H"], fixed: "", note: "Select text with it on to highlight it" },
        { id: "pvdelete", group: "Grove Preview", defaults: [], action: "Delete the selected shape, or the selected pages", fixed: "Delete", note: "" },
        { id: "pvshift", group: "Grove Preview", defaults: [], action: "Square shapes, 45° lines while drawing", fixed: "Shift+drag", note: "" },
        { id: "pvtext", group: "Grove Preview", defaults: [], action: "Select a word / a line", fixed: "Double-click / triple-click", note: "" },
        { id: "gd.badges", group: "Google Drive (canopy)", defaults: [], action: "Sync badges on items in folders canopy syncs", fixed: "✓ ↻ ✕ ! –", note: "Synced, syncing, couldn't sync (open canopy to see why), changed in two places (open canopy to choose), not synced (an ignore rule). Hover one to read it; turn them off in Settings › General › Appearance › Google Drive badges" },
        { id: "gd.link", group: "Google Drive (canopy)", defaults: [], action: "Copy an item's Google Drive link, or open it on the web", fixed: "right-click › Google Drive › Copy Link / View on Google Drive", note: "" },
        { id: "gd.share", group: "Google Drive (canopy)", defaults: [], action: "Share an item, star it", fixed: "right-click › Google Drive › Share… / Star", note: "Share… opens canopy's Share sheet" },
        { id: "gd.unsync", group: "Google Drive (canopy)", defaults: [], action: "Free up room: keep an item only in Google Drive", fixed: "right-click › Google Drive › Remove from This Computer…", note: "The copy here goes to the Trash; the one in Google Drive is kept" },
        { id: "gd.add", group: "Google Drive (canopy)", defaults: [], action: "Sync a folder with Google Drive", fixed: "right-click a folder › Sync with Google Drive…", note: "Shown while canopy is running; opens canopy to pick the account and where it goes" },
        { id: "du-new", group: "Grove Disk Utility", action: "New partition in the selected free space", defaults: [], fixed: "Ctrl+N", note: "Or double-click free space" },
        { id: "du-delete", group: "Grove Disk Utility", action: "Delete the selected partition", defaults: [], fixed: "Delete", note: "Queued: nothing changes until Apply" },
        { id: "du-resize", group: "Grove Disk Utility", action: "Resize/Move the selected partition", defaults: [], fixed: "Ctrl+R", note: "Drag the ends or the middle" },
        { id: "du-copy", group: "Grove Disk Utility", action: "Copy / paste a partition", defaults: [], fixed: "Ctrl+C, Ctrl+V", note: "Paste into free space, on this disk or another" },
        { id: "du-label", group: "Grove Disk Utility", action: "Label and name the selected partition", defaults: [], fixed: "F2", note: "" },
        { id: "du-info", group: "Grove Disk Utility", action: "Information about the selected partition", defaults: [], fixed: "Ctrl+I", note: "Or double-click it" },
        { id: "du-undo", group: "Grove Disk Utility", action: "Undo the last pending operation", defaults: [], fixed: "Ctrl+Z", note: "" },
        { id: "du-apply", group: "Grove Disk Utility", action: "Apply all pending operations", defaults: [], fixed: "Ctrl+Return", note: "Asks first" },
        { id: "du-refresh", group: "Grove Disk Utility", action: "Look at the disks again", defaults: [], fixed: "F5", note: "Pending operations are kept" },
        { id: "help", group: "Other", action: "Help and shortcuts", defaults: ["F1", "Ctrl+?", "Ctrl+Shift+/"], fixed: "", note: "Dolphin: F1  ·  Finder: ⇧⌘?" }
    ]

    // The user's changes (settings.shortcuts), set by Main.qml.
    property var custom: ({})
    function entry(id) {
        for (let i = 0; i < shortcuts.length; i++) if (shortcuts[i].id === id) return shortcuts[i]
        return null
    }
    // The shortcuts in use for an action id.
    function seqs(id) {
        const c = custom ? custom[id] : undefined
        if (c !== undefined && c !== null) return c
        const e = entry(id)
        return e ? e.defaults : []
    }
    function changed(id) { return !!custom && custom[id] !== undefined && custom[id] !== null }
    // What Help and tooltips show for an entry.
    function display(e) {
        if (e.fixed) return [e.fixed]
        return seqs(e.id)
    }
    function keysFor(action) {
        for (let i = 0; i < shortcuts.length; i++)
            if (shortcuts[i].action === action || shortcuts[i].id === action) return display(shortcuts[i])[0] || ""
        return ""
    }
    // A menu's shortcut text, given the default: what that action uses now.
    function shown(def) {
        for (let i = 0; i < shortcuts.length; i++)
            if (shortcuts[i].defaults.indexOf(def) >= 0) return seqs(shortcuts[i].id)[0] || ""
        return def
    }
    // Which other action already uses `seq` (its entry), or null.
    function owner(seq, exceptId) {
        const n = normal(seq)
        for (let i = 0; i < shortcuts.length; i++) {
            const e = shortcuts[i]
            if (e.id === exceptId || e.fixed) continue
            if (seqs(e.id).some(x => normal(x) === n)) return e
        }
        return null
    }
    function normal(seq) { return (seq || "").toLowerCase().replace(/\s+/g, "") }
    // "Back (Alt+Left)" for tooltips.
    function tip(text, action) {
        const k = keysFor(action || text)
        return k ? text + "  (" + k + ")" : text
    }
}
