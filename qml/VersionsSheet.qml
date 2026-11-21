import QtQuick
import QtQuick.Controls.Basic

// Browse Previous Versions (Time Machine style) from the hourly btrfs
// snapshots: each distinct earlier copy, newest first. Show opens the
// copy (read-only) in the pane; Restore copies it back (you're asked
// whether to replace the current one or keep both).
Sheet {
    id: vs
    property var app
    property var win
    property string path: ""
    property var list: []
    property bool loading: false
    title: "Previous Versions of “" + path.substring(path.lastIndexOf("/") + 1) + "”"
    width: Math.min(parent ? parent.width - 40 : 620, Math.round(560 * Theme.pt))
    function start(p) {
        path = p; list = []; loading = true
        open()
        Async.call(app.grove, k => app.grove.versions(k, p), r => { vs.list = r || []; vs.loading = false })
    }
    GLabel {
        width: parent.width
        wrapMode: Text.Wrap; elide: Text.ElideNone
        color: Theme.fgSecondary
        text: vs.loading ? "Looking through snapshots…" : vs.list.length ? vs.list.length + " earlier versions found in the snapshots." : "No earlier versions: this item hasn't changed in any snapshot, or this disk has no snapshots (snapper)."
    }
    Rectangle {
        width: parent.width
        height: Math.min(320, Math.max(40, lv.contentHeight + 8))
        radius: 8
        color: Theme.field
        ListView {
            id: lv
            anchors.fill: parent
            anchors.margins: 4
            clip: true
            model: vs.list
            ScrollBar.vertical: GScrollBar {}
            delegate: Item {
                required property var modelData
                width: lv.width
                height: Theme.rowHeight + 8
                GLabel { x: 8; anchors.verticalCenter: parent.verticalCenter; width: parent.width * 0.45; text: new Date(modelData.date * 1000).toLocaleString(Qt.locale(), "ddd d MMM yyyy, h:mm AP") }
                GLabel { x: parent.width * 0.45; anchors.verticalCenter: parent.verticalCenter; text: modelData.is_dir ? "Folder" : vs.app.size(modelData.size); color: Theme.fgSecondary }
                Row {
                    anchors.right: parent.right; anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 6
                    GButton {
                        text: "Show"; flat: false; capsule: true
                        tip: "Open this version (read-only) in the window"
                        onClicked: {
                            const p = modelData.path
                            vs.close()
                            if (modelData.is_dir) vs.win.go(p)
                            else vs.win.activePane.navigate(p.substring(0, p.lastIndexOf("/")), { select: [p] })
                        }
                    }
                    GButton {
                        text: "Restore"; flat: false; capsule: true
                        tip: "Copy this version back to where the item is now (you'll be asked about the current one)"
                        onClicked: {
                            const dest = vs.path.substring(0, vs.path.lastIndexOf("/")) || "/"
                            vs.app.grove.runJob("copy", JSON.stringify([modelData.path]), dest)
                            vs.close()
                        }
                    }
                }
            }
        }
    }
    buttons: [ PushButton { text: "Done"; primary: true; onClicked: vs.close() } ]
}
