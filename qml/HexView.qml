import QtQuick
import QtQuick.Controls.Basic
import Grove

// Binary files: the whole file in hex, 16 bytes a line with the offset and
// the printable characters, read a kilobyte at a time as you scroll (any
// size). A strip on top gives the size and a Go to offset field.
Item {
    id: hv
    property string path: ""
    property bool active: true

    HexDoc { id: hex; dark: Theme.dark }
    onPathChanged: Qt.callLater(reopen)
    onActiveChanged: Qt.callLater(reopen)
    Connections { target: Theme; function onDarkChanged() { hv.reopen() } }
    function reopen() { const want = active ? path : ""; if (want !== hex.path) hex.open(want) }

    FontMetrics { id: fm; font.family: Theme.mono; font.pixelSize: Theme.body * 0.92 }
    // A line's height as the rows really draw it (measured from the first
    // row that arrives; font metrics alone left gaps or overlaps).
    property real measured: 0
    readonly property real lineH: measured > 0 ? measured : Math.ceil(fm.height)
    readonly property real rowH: lineH * 64
    readonly property real textW: fm.averageCharacterWidth * 78 + 32

    Rectangle {
        id: bar
        width: parent.width
        height: Math.round(36 * Theme.pt)
        color: Theme.toolbar
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.separator }
        GLabel {
            anchors.verticalCenter: parent.verticalCenter
            x: 12
            width: (goField.visible ? goField.x : bar.width) - 24
            elide: Text.ElideRight
            text: hex.size.toLocaleString(Qt.locale(), "f", 0) + " bytes (0x" + Math.floor(hex.size).toString(16) + ")"
            color: Theme.fgSecondary
            font.pixelSize: Theme.small
        }
        GField {
            id: goField
            visible: bar.width > 380
            anchors.verticalCenter: parent.verticalCenter
            anchors.right: parent.right
            anchors.rightMargin: 8
            width: Math.round(170 * Theme.pt)
            height: Math.round(26 * Theme.pt)
            iconName: "edit-find"
            placeholderText: "Go to offset (0x1f40)"
            tip: "Jump to a byte offset: hex with 0x, else decimal"
            onAccepted: {
                const t = text.trim()
                const n = t.toLowerCase().startsWith("0x") ? parseInt(t.slice(2), 16) : parseInt(t, 10)
                if (!isNaN(n)) {
                    const row = Math.floor(n / 1024), line = Math.floor((n % 1024) / 16)
                    list.positionViewAtIndex(Math.min(row, hex.rows - 1), ListView.Beginning)
                    list.contentY += line * hv.lineH
                }
            }
        }
    }

    PanWheel { anchors.fill: list; flickable: list; notch: hv.lineH * 4 }
    ListView {
        id: list
        anchors { top: bar.bottom; left: parent.left; right: parent.right; bottom: parent.bottom }
        clip: true
        interactive: false
        model: hex.rows
        cacheBuffer: hv.rowH
        contentWidth: Math.max(width, hv.textW)
        flickableDirection: Flickable.HorizontalAndVerticalFlick
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: GScrollBar { parent: list; x: list.width - width; height: list.height }
        ScrollBar.horizontal: GScrollBar { parent: list; y: list.height - height; width: list.width }
        delegate: TextEdit {
            id: rowItem
            required property int index
            // The last row may be short.
            readonly property int lines: Math.max(1, Math.ceil(Math.min(1024, hex.size - index * 1024) / 16))
            width: list.contentWidth
            height: text !== "" ? contentHeight : lines * hv.lineH
            leftPadding: 16
            readOnly: true
            selectByMouse: true
            textFormat: TextEdit.RichText
            font.family: Theme.mono
            font.pixelSize: Theme.body * 0.92
            renderType: Text.NativeRendering
            color: Theme.fg
            selectionColor: Theme.accent
            selectedTextColor: Theme.accentText
            text: ""
            Component.onCompleted: hex.request(index)
            Connections {
                target: hex
                function onRowReady(row, html) {
                    if (row !== rowItem.index) return
                    rowItem.text = html
                    if (hv.measured === 0 && rowItem.lines > 1) hv.measured = rowItem.contentHeight / rowItem.lines
                }
                // Another file (or the same one opened again): ask again.
                function onPathChanged() { rowItem.text = ""; hex.request(rowItem.index) }
            }
        }
    }
    GLabel { anchors.centerIn: parent; visible: hex.rows === 0 && hv.path !== ""; text: "This file is empty"; color: Theme.fgSecondary }
}
