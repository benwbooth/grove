pragma Singleton
import QtQuick

// Every action and shortcut, for tooltips and the searchable Help window.
// `keys` is grove's shortcut (Ctrl stands where a Mac has ⌘); `finder` and
// `dolphin` say where it comes from (both work unless noted).
QtObject {
    readonly property var shortcuts: [
        { group: "Navigate", action: "Back", keys: "Alt+Left", also: "Ctrl+[, Backspace", note: "Finder: ⌘[  ·  Dolphin: Alt+Left, Backspace" },
        { group: "Navigate", action: "Forward", keys: "Alt+Right", also: "Ctrl+]", note: "Finder: ⌘]  ·  Dolphin: Alt+Right" },
        { group: "Navigate", action: "Enclosing folder", keys: "Alt+Up", also: "Ctrl+Up", note: "Finder: ⌘↑  ·  Dolphin: Alt+Up" },
        { group: "Navigate", action: "Open", keys: "Return", also: "Ctrl+Down, double-click", note: "Finder: ⌘↓ (Return renames in Finder; here F2 renames)" },
        { group: "Navigate", action: "Go to folder / edit the path", keys: "Ctrl+L", also: "Ctrl+Shift+G", note: "Dolphin: Ctrl+L  ·  Finder: ⇧⌘G" },
        { group: "Navigate", action: "Home", keys: "Ctrl+Shift+H", also: "Alt+Home", note: "Finder: ⇧⌘H  ·  Dolphin: Alt+Home" },
        { group: "Navigate", action: "Desktop", keys: "Ctrl+Shift+D", also: "", note: "Finder: ⇧⌘D" },
        { group: "Navigate", action: "Documents", keys: "Ctrl+Shift+O", also: "", note: "Finder: ⇧⌘O" },
        { group: "Navigate", action: "Downloads", keys: "Ctrl+Alt+L", also: "", note: "Finder: ⌥⌘L" },
        { group: "Navigate", action: "Recents", keys: "Ctrl+Shift+F", also: "", note: "Finder: ⇧⌘F" },
        { group: "Navigate", action: "Computer", keys: "Ctrl+Shift+C", also: "", note: "Finder: ⇧⌘C" },
        { group: "Navigate", action: "Trash", keys: "Ctrl+Shift+T", also: "", note: "" },
        { group: "Navigate", action: "Connect to Server…", keys: "Ctrl+K", also: "", note: "Finder: ⌘K" },
        { group: "Navigate", action: "Reload", keys: "F5", also: "Ctrl+R", note: "Dolphin: F5" },
        { group: "Navigate", action: "Select items by typing their name", keys: "letters", also: "", note: "Both" },

        { group: "View", action: "Icons", keys: "Ctrl+1", also: "", note: "Finder: ⌘1" },
        { group: "View", action: "List", keys: "Ctrl+2", also: "", note: "Finder: ⌘2" },
        { group: "View", action: "Columns", keys: "Ctrl+3", also: "", note: "Finder: ⌘3" },
        { group: "View", action: "Gallery", keys: "Ctrl+4", also: "", note: "Finder: ⌘4" },
        { group: "View", action: "Bigger icons", keys: "Ctrl++", also: "Ctrl+=", note: "Both" },
        { group: "View", action: "Smaller icons", keys: "Ctrl+-", also: "", note: "Both" },
        { group: "View", action: "Show/hide hidden files", keys: "Ctrl+H", also: "Ctrl+Shift+.", note: "Dolphin: Ctrl+H  ·  Finder: ⇧⌘." },
        { group: "View", action: "View Options (spacing, text size, labels, background)", keys: "Ctrl+J", also: "", note: "Finder: ⌘J" },
        { group: "View", action: "Use Stacks (icon view)", keys: "Ctrl+Alt+0", also: "", note: "Finder: ⌃⌘0. Click a stack to open it in place" },
        { group: "View", action: "Filter this folder", keys: "Ctrl+I", also: "", note: "Dolphin's filter bar (Finder uses ⌘I for Get Info: here Alt+Return)" },
        { group: "View", action: "Quick Look", keys: "Space", also: "Ctrl+Y", note: "Finder: Space, ⌘Y. Arrow keys move between files while it's open" },
        { group: "View", action: "Split view", keys: "F3", also: "", note: "Dolphin: F3" },
        { group: "View", action: "Terminal panel", keys: "F4", also: "", note: "Dolphin: F4" },
        { group: "View", action: "Information panel / preview pane", keys: "F11", also: "Ctrl+Shift+P", note: "Dolphin: F11  ·  Finder: ⇧⌘P" },
        { group: "View", action: "Folders panel (tree)", keys: "F7", also: "", note: "Dolphin: F7" },
        { group: "View", action: "Sidebar", keys: "Ctrl+Alt+S", also: "F9", note: "Finder: ⌃⌘S  ·  Dolphin: F9" },
        { group: "View", action: "Expand / collapse folder (list view)", keys: "Right / Left", also: "", note: "Finder" },

        { group: "Tabs & windows", action: "New window", keys: "Ctrl+N", also: "", note: "Both" },
        { group: "Tabs & windows", action: "New tab", keys: "Ctrl+T", also: "", note: "Both" },
        { group: "Tabs & windows", action: "Close tab", keys: "Ctrl+W", also: "", note: "Both" },
        { group: "Tabs & windows", action: "Next / previous tab", keys: "Ctrl+Tab", also: "Ctrl+Shift+Tab", note: "Both" },
        { group: "Tabs & windows", action: "Switch split pane", keys: "Tab", also: "", note: "When the view has focus" },
        { group: "Tabs & windows", action: "Move a tab to another window", keys: "drag the tab", also: "", note: "Finder and Dolphin. Drop it away from any tab bar to open it in a new window; its folder, history and view go with it" },

        { group: "Edit", action: "Copy", keys: "Ctrl+C", also: "", note: "Both" },
        { group: "Edit", action: "Cut (move on paste)", keys: "Ctrl+X", also: "", note: "Dolphin (Finder: ⌥⌘V moves)" },
        { group: "Edit", action: "Paste", keys: "Ctrl+V", also: "", note: "Both" },
        { group: "Edit", action: "Move here (paste and remove the originals)", keys: "Ctrl+Alt+V", also: "", note: "Finder: ⌥⌘V" },
        { group: "Edit", action: "Copy path", keys: "Ctrl+Alt+C", also: "", note: "Finder: ⌥⌘C" },
        { group: "Edit", action: "Copy to the other side (split view)", keys: "Shift+F5", also: "", note: "Dolphin: Copy to Other View" },
        { group: "Edit", action: "Move to the other side (split view)", keys: "Shift+F6", also: "", note: "Dolphin: Move to Other View" },
        { group: "Edit", action: "Duplicate", keys: "Ctrl+D", also: "", note: "Finder: ⌘D" },
        { group: "Edit", action: "Make alias (link)", keys: "Ctrl+Alt+A", also: "", note: "Finder: ⌃⌘A" },
        { group: "Edit", action: "Rename", keys: "F2", also: "", note: "Dolphin: F2 (several items: batch rename)" },
        { group: "Edit", action: "Batch rename", keys: "Shift+F2", also: "", note: "" },
        { group: "Edit", action: "New folder", keys: "Ctrl+Shift+N", also: "F10", note: "Finder: ⇧⌘N  ·  Dolphin: F10" },
        { group: "Edit", action: "New file", keys: "Ctrl+Alt+N", also: "", note: "" },
        { group: "Edit", action: "Move to Trash", keys: "Delete", also: "Ctrl+Backspace", note: "Dolphin: Del  ·  Finder: ⌘⌫" },
        { group: "Edit", action: "Delete permanently", keys: "Shift+Delete", also: "", note: "Dolphin: Shift+Del (asks first)" },
        { group: "Edit", action: "Undo", keys: "Ctrl+Z", also: "", note: "Both" },
        { group: "Edit", action: "Redo", keys: "Ctrl+Shift+Z", also: "", note: "Both" },
        { group: "Edit", action: "Select all", keys: "Ctrl+A", also: "", note: "Both" },
        { group: "Edit", action: "Invert selection", keys: "Ctrl+Shift+A", also: "", note: "Dolphin" },
        { group: "Edit", action: "Selection mode", keys: "Ctrl+Shift+S", also: "", note: "Dolphin: checkboxes on items. Space ticks the current item, Esc leaves" },
        { group: "Edit", action: "Clear selection", keys: "Escape", also: "", note: "Both" },
        { group: "Edit", action: "Get Info", keys: "Alt+Return", also: "Ctrl+Shift+I", note: "Dolphin: Alt+Return (Finder: ⌘I, here the filter bar)" },
        { group: "Edit", action: "Tags: Red … Gray", keys: "Ctrl+Alt+1…7", also: "", note: "Toggle a colour tag on the selection" },

        { group: "Search", action: "Search", keys: "Ctrl+F", also: "", note: "Both" },
        { group: "Search", action: "Save search as Smart Folder", keys: "", also: "", note: "From the search bar" },

        { group: "Other", action: "Open in Terminal", keys: "Ctrl+Alt+T", also: "", note: "" },
        { group: "Other", action: "Eject", keys: "Ctrl+E", also: "", note: "Finder: ⌘E" },
        { group: "Other", action: "Settings", keys: "Ctrl+,", also: "", note: "Finder: ⌘," },
        { group: "Other", action: "Help and shortcuts", keys: "F1", also: "Ctrl+?", note: "Dolphin: F1  ·  Finder: ⇧⌘?" }
    ]

    function keysFor(action) {
        for (let i = 0; i < shortcuts.length; i++)
            if (shortcuts[i].action === action) return shortcuts[i].keys
        return ""
    }
    // "Back (Alt+Left)" for tooltips.
    function tip(text, action) {
        const k = keysFor(action || text)
        return k ? text + "  (" + k + ")" : text
    }
}
