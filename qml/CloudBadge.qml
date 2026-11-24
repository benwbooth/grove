import QtQuick

// Google Drive sync overlay from canopy ("CODE/canopy", see canopy.rs):
// ✓ synced, ↻ syncing or waiting, ✕ error, ! changed in two places,
// – not synced (an ignore rule). Sits in a different corner from GitBadge.
Rectangle {
    id: c
    property string status: ""
    readonly property string code: status.split("/")[0]
    visible: ({ "S": 1, "P": 1, "E": 1, "C": 1, "I": 1 })[code] === 1
    width: 14
    height: 14
    radius: 7
    color: code === "S" ? "#30d158" : code === "P" ? Theme.accent : code === "E" ? "#ff453a" : code === "C" ? "#ff375f" : "#8e8e93"
    border.width: 1.5
    border.color: Theme.content
    GLabel {
        anchors.centerIn: parent
        text: ({ "S": "✓", "P": "↻", "E": "✕", "C": "!", "I": "–" })[c.code] || ""
        color: "white"
        font.pixelSize: 9
        font.bold: true
    }
    GTip {
        visible: cm.containsMouse
        text: ({ "S": "Synced with Google Drive",
                 "P": "Syncing with Google Drive…",
                 "E": "Couldn't sync: open canopy to see why",
                 "C": "Changed in two places: open canopy to choose",
                 "I": "Not synced (ignore rule)" })[c.code] || ""
    }
    MouseArea { id: cm; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
}
