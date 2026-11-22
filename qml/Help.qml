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
        { id: "hidden", group: "View", action: "Show/hide hidden files", defaults: ["Ctrl+H", "Ctrl+Shift+."], fixed: "", note: "Dolphin: Ctrl+H  ·  Finder: ⇧⌘." },
        { id: "viewoptions", group: "View", action: "View Options (spacing, text size, labels, background)", defaults: ["Ctrl+J"], fixed: "", note: "Finder: ⌘J" },
        { id: "stacks", group: "View", action: "Use Stacks (icon view)", defaults: ["Ctrl+Alt+0"], fixed: "", note: "Finder: ⌃⌘0. Click a stack to open it in place" },
        { id: "filter", group: "View", action: "Filter this folder", defaults: ["Ctrl+I"], fixed: "", note: "Dolphin's filter bar (Finder uses ⌘I for Get Info: here Alt+Return)" },
        { id: "quicklook", group: "View", action: "Quick Look", defaults: ["Ctrl+Y"], fixed: "", note: "Space always works too. Finder: Space, ⌘Y. Arrow keys move between files while it's open" },
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
