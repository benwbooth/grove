import QtQuick

// A git repository's folder: the branch it's on, beside its name in the
// views ("⎇ main"), from its git status ("@/git/main", vcs.rs). Click it
// for Git Changes of that repository, as the path bar's chip does.
Rectangle {
    id: bt
    property string status: ""        // the row's git status
    property string path: ""          // the folder
    property var pane
    property bool onAccent: false     // drawn on a selected (accent) row
    property real px: Theme.small
    // "@/git/feature/x" → "feature/x".
    readonly property string branch: status.indexOf("@/") === 0 ? status.split("/").slice(2).join("/") : ""
    // Uncommitted changes (-1: not read yet): an orange dot and the count.
    readonly property int changes: pane && pane.repoDirty && pane.repoDirty[path] !== undefined ? pane.repoDirty[path] : -1
    // The space left beside the name: the name always comes first, so the
    // tag shortens to fit and hides when even a short tag won't.
    property real room: 1e9
    property real maxWidth: 160
    readonly property real natural: arrow.implicitWidth + 3 + label.implicitWidth + 10 + (bt.changes > 0 ? dirty.implicitWidth + 3 : 0)
    readonly property bool fits: branch !== "" && room >= Math.min(natural, 52)
    visible: fits
    width: visible ? Math.min(natural, maxWidth, room) : 0
    height: Math.round(px + 6)
    radius: height / 2
    color: onAccent ? Qt.rgba(1, 1, 1, tm.containsMouse ? 0.32 : 0.22)
         : tm.containsMouse ? Qt.rgba(0.18, 0.63, 0.26, 0.32) : Qt.rgba(0.18, 0.63, 0.26, 0.18)
    clip: true
    Row {
        id: row
        x: 5
        anchors.verticalCenter: parent.verticalCenter
        spacing: 3
        GLabel { id: arrow; text: "⎇"; font.pixelSize: bt.px; color: bt.onAccent ? Theme.accentText : Theme.fg; anchors.verticalCenter: parent.verticalCenter }
        GLabel {
            id: label
            text: bt.branch
            font.pixelSize: bt.px
            color: bt.onAccent ? Theme.accentText : Theme.fg
            elide: Text.ElideMiddle
            width: Math.max(0, Math.min(implicitWidth, bt.width - 10 - arrow.implicitWidth - 3 - (bt.changes > 0 ? dirty.implicitWidth + 3 : 0)))
            anchors.verticalCenter: parent.verticalCenter
        }
        GLabel {
            id: dirty
            visible: bt.changes > 0
            text: "● " + bt.changes
            font.pixelSize: bt.px
            font.features: { "tnum": 1 }
            color: bt.onAccent ? Theme.accentText : "#e8912d"
            anchors.verticalCenter: parent.verticalCenter
        }
    }
    MouseArea {
        id: tm
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: if (bt.pane && bt.pane.win) bt.pane.win.showGitChanges(bt.path)
    }
    GTip { visible: tm.containsMouse; text: "On branch " + bt.branch + (bt.changes > 0 ? ", " + bt.changes + (bt.changes === 1 ? " uncommitted change" : " uncommitted changes") : bt.changes === 0 ? ", nothing uncommitted" : "") + ". Click for Git Changes: stage, commit, switch branches" }
}
