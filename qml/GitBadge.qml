import QtQuick

// Git status overlay: M modified, A added, ? untracked, D deleted,
// R renamed, U conflict.
Rectangle {
    id: g
    property string status: ""
    visible: status !== ""
    width: 14; height: 14; radius: 7
    color: status === "M" ? "#ff9f0a" : status === "A" ? "#30d158" : status === "?" ? "#8e8e93" : status === "D" ? "#ff453a" : status === "U" ? "#ff375f" : "#64d2ff"
    border.width: 1.5
    border.color: Theme.content
    GLabel {
        anchors.centerIn: parent
        text: g.status
        color: "white"
        font.pixelSize: 9
        font.bold: true
    }
    GTip {
        visible: gm.containsMouse
        text: ({ "M": "Changed since the last git commit", "A": "Added to git, not committed yet", "?": "Not tracked by git",
                 "D": "Deleted (git)", "R": "Renamed (git)", "U": "Merge conflict (git)" })[g.status] || ""
    }
    MouseArea { id: gm; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
}
