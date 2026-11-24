import QtQuick
import QtQuick.Controls.Basic

// Go › Snapshots…: the disk's btrfs snapshots (snapper's hourly ones),
// newest first. Choosing one opens this folder as it was then, read-only
// (a banner says so, with a way back). Files in it can be opened, copied
// out or restored; nothing in a snapshot can be changed.
Sheet {
    id: ss
    property var app
    property var win
    property string path: ""
    property var list: []
    property bool loading: false
    readonly property string folderName: path === "/" ? "/" : path.substring(path.lastIndexOf("/") + 1)
    title: "Snapshots"
    width: Math.min(parent ? parent.width - 40 : 620, Math.round(560 * Theme.pt))
    function start(p) {
        path = p; list = []; loading = true
        open()
        Async.call(app.grove, k => app.grove.snapshots(k, p), r => { ss.list = (r && r.list) || []; ss.loading = false })
    }
    GLabel {
        width: parent.width
        wrapMode: Text.Wrap; elide: Text.ElideNone
        color: Theme.fgSecondary
        text: ss.loading ? "Looking for snapshots…"
            : ss.list.length ? "“" + ss.folderName + "” as it was in each of this disk's " + ss.list.length + " snapshots. They open read-only: copy things out, or drag them back, to restore them."
            : "This disk has no snapshots grove can read (btrfs snapshots made by snapper, in .snapshots)."
    }
    Rectangle {
        width: parent.width
        height: Math.min(360, Math.max(40, lv.contentHeight + 8))
        radius: 8
        color: Theme.field
        KineticWheel { flickable: lv }
        ListView {
            id: lv
            anchors.fill: parent
            anchors.margins: 4
            clip: true
            model: ss.list
            ScrollBar.vertical: GScrollBar {}
            delegate: Item {
                required property var modelData
                width: lv.width
                height: Theme.rowHeight + 8
                GLabel { x: 8; anchors.verticalCenter: parent.verticalCenter; width: parent.width * 0.5; text: new Date(modelData.date * 1000).toLocaleString(Qt.locale(), "ddd d MMM yyyy, h:mm AP") }
                GLabel {
                    x: parent.width * 0.5; width: parent.width * 0.5 - openBtn.width - 16
                    anchors.verticalCenter: parent.verticalCenter
                    text: !modelData.exists ? "(this folder didn't exist yet)" : modelData.description || ""
                    color: Theme.fgSecondary
                    elide: Text.ElideRight
                }
                GButton {
                    id: openBtn
                    anchors.right: parent.right; anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    text: "Open"; flat: false; capsule: true
                    tip: modelData.exists ? "Open “" + ss.folderName + "” as it was then (read-only)" : "Open the nearest folder above it that existed then (read-only)"
                    onClicked: { ss.close(); ss.win.go(modelData.path) }
                }
            }
        }
    }
    buttons: [ PushButton { text: "Done"; primary: true; onClicked: ss.close() } ]
}
