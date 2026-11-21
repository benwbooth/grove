import QtQuick
import QtQuick.Controls.Basic

// First run: what's where, in a few lines.
Sheet {
    id: w
    property var app
    property var win
    title: "Welcome to Grove"
    width: Math.min(parent ? parent.width - 40 : 560, Math.round(520 * Theme.pt))
    Repeater {
        model: [
            { icon: "sidebar-show-symbolic|view-split-left-right", t: "The sidebar", d: "Your favourite folders, disks (with eject), network places and tags. Drag a folder onto Favourites to add it." },
            { icon: "view-list-icons", t: "Four ways to look", d: "Icons, List, Columns and Gallery (Ctrl+1 to 4). Each folder remembers how you left it." },
            { icon: "quickview", t: "Quick Look", d: "Select a file and press Space to preview it. Arrow keys move to the next one." },
            { icon: "view-split-left-right", t: "Split view and tabs", d: "F3 shows two folders side by side; drag files between them. Ctrl+T opens a tab." },
            { icon: "edit-undo", t: "Undo anything", d: "Moves, renames, new folders and the Trash can all be undone with Ctrl+Z." },
            { icon: "help-contents", t: "Help", d: "Hover over a button to see what it does and its shortcut. F1 lists every shortcut." }
        ]
        Row {
            required property var modelData
            spacing: 12
            width: parent.width
            GIcon { size: 28; name: modelData.icon; anchors.verticalCenter: parent.verticalCenter }
            Column {
                width: parent.width - 44
                GLabel { text: modelData.t; font.bold: true }
                GLabel { text: modelData.d; width: parent.width; color: Theme.fgSecondary; wrapMode: Text.Wrap; elide: Text.ElideNone }
            }
        }
    }
    buttons: [
        PushButton { text: "Show Shortcuts"; onClicked: { w.close(); w.win.showHelp("") } },
        PushButton { text: "Start"; primary: true; focus: true; onClicked: w.close() }
    ]
    onClosed: app.grove.setSetting("welcomed", "true")
}
