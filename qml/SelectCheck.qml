import QtQuick

// Dolphin's selection mode: a round checkbox on an item. A click toggles
// the item without touching the rest of the selection.
Rectangle {
    id: ck
    property var pane
    property int row: -1
    property bool checked: false
    width: 18; height: 18; radius: 9
    color: checked ? Theme.accent : Qt.rgba(Theme.content.r, Theme.content.g, Theme.content.b, 0.85)
    border.width: 1.5
    border.color: checked ? Theme.accentText : Theme.fgTertiary
    GLabel { anchors.centerIn: parent; visible: ck.checked; text: "✓"; color: Theme.accentText; font.pixelSize: 12; font.bold: true }
    MouseArea {
        id: m
        anchors.fill: parent
        anchors.margins: -3
        hoverEnabled: true
        onClicked: { ck.pane.activated(); ck.pane.forceActiveFocus(); ck.pane.model.select(ck.row, "toggle") }
    }
    GTip { visible: m.containsMouse; text: ck.checked ? "Deselect this item (Space)" : "Select this item (Space)" }
}
