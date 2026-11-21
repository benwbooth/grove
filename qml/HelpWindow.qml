import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window

// Help: a short introduction and every keyboard shortcut, searchable.
// Finder and Dolphin shortcuts both work where they don't clash; the
// notes say which is which.
Window {
    id: hw
    property var app
    property string query: ""
    signal changeShortcuts()
    width: Math.round(640 * Theme.pt)
    height: Math.round(640 * Theme.pt)
    color: Theme.window
    title: "Grove Help"
    onVisibleChanged: if (visible) { field.text = query; field.forceActiveFocus() }

    Item {
        anchors.fill: parent
        transform: Translate { x: 0.001; y: 0.001 }
        Column {
            id: head
            x: 20; y: 16
            width: parent.width - 40
            spacing: 10
            GLabel { text: "Grove Help"; font.pixelSize: Theme.large; font.bold: true }
            GLabel {
                width: parent.width
                wrapMode: Text.Wrap; elide: Text.ElideNone
                color: Theme.fgSecondary
                text: "Grove works like Finder: the sidebar on the left, four views (Ctrl+1–4), Space for Quick Look. It also has Dolphin's extras: split view (F3), the filter bar (Ctrl+I), tabs, and Dolphin's shortcuts where they don't clash. Hover over any button to see what it does. Right-click items for more."
            }
            Row {
                width: parent.width
                spacing: 8
                GField { id: field; width: parent.width - changeBtn.width - 8; iconName: "edit-find"; clearable: true; placeholderText: "Search shortcuts and actions (e.g. “rename”, “hidden”, “tab”)"; tip: "Type part of an action's name or its keys" }
                PushButton { id: changeBtn; text: "Change Shortcuts…"; tip: "Settings › Keyboard: give any action your own shortcut"; onClicked: hw.changeShortcuts() }
            }
        }
        ListView {
            id: list
            anchors.top: head.bottom
            anchors.topMargin: 10
            anchors.bottom: parent.bottom
            x: 20
            width: parent.width - 40
            clip: true
            ScrollBar.vertical: GScrollBar {}
            model: Help.shortcuts.filter(s => {
                const q = field.text.toLowerCase()
                return !q || (s.action + " " + Help.display(s).join(" ") + " " + s.note + " " + s.group).toLowerCase().indexOf(q) >= 0
            })
            section.property: "group"
            section.delegate: GLabel { required property string section; text: section; font.bold: true; topPadding: 12; bottomPadding: 4; color: Theme.fgSecondary }
            delegate: Item {
                required property var modelData
                width: list.width
                height: Math.max(Theme.rowHeight + 6, noteL.implicitHeight + 24)
                Rectangle { anchors.fill: parent; radius: 6; color: hm.containsMouse ? Theme.hover : "transparent" }
                MouseArea { id: hm; anchors.fill: parent; hoverEnabled: true }
                GLabel { x: 8; y: 4; width: parent.width * 0.42; text: modelData.action }
                GLabel { id: noteL; x: 8; y: 4 + Theme.body * 1.4; width: parent.width * 0.42; text: modelData.note; font.pixelSize: Theme.caption; color: Theme.fgTertiary; wrapMode: Text.Wrap; elide: Text.ElideNone }
                Row {
                    x: parent.width * 0.45
                    y: 4
                    spacing: 6
                    Repeater {
                        // The user's own bindings (Settings › Keyboard).
                        model: Help.display(modelData)
                        Rectangle {
                            required property string modelData
                            visible: modelData !== ""
                            height: Theme.rowHeight - 4
                            width: kl.implicitWidth + 14
                            radius: 5
                            color: Theme.field
                            border.color: Theme.separator
                            GLabel { id: kl; anchors.centerIn: parent; text: modelData; font.pixelSize: Theme.small }
                        }
                    }
                }
            }
        }
    }
}
