import QtQuick
import QtQuick.Controls.Basic

// The list view's preview pane, under the list: the selected file itself
// on the left (text, tables, pages, a player, the picture: FileViewer) and
// its details on the right. Drag the top edge to make it taller or
// shorter; double-click the edge to hide it (View Options brings it back).
Rectangle {
    id: bp
    property var pane
    property var item: null
    property var info: ({})
    // Height while dragging (saved when the drag ends).
    property real dragH: 0
    readonly property real savedH: pane && pane.app && pane.app.settings.list_preview_height > 0 ? pane.app.settings.list_preview_height : 260
    height: dragH > 0 ? dragH : savedH
    color: Theme.content

    // The selected file, once the selection has settled (one file only).
    function settle() {
        if (!pane || !pane.model) return
        const fm = pane.model
        const r = fm.currentRow
        const it = r >= 0 && fm.selectionCount === 1 ? pane.itemAt(r) : null
        if (it && item && it.path === item.path && it.mtime === item.mtime && it.size === item.size) return
        if (!it && !item) return
        item = it && !it.isDir ? it : null
        if (!item || item.path.indexOf("/") !== 0) { info = {}; return }
        const path = item.path, life = bp.life
        Async.call(pane.grove, k => pane.grove.info(k, path), r2 => { if (life.alive && bp.item && bp.item.path === path) bp.info = r2 })
    }
    property var life: ({ alive: true })
    Component.onDestruction: life.alive = false
    Connections {
        target: bp.pane ? bp.pane.model : null
        function onCurrentRowChanged() { Qt.callLater(bp.settle) }
        function onSelectionCountChanged() { Qt.callLater(bp.settle) }
    }
    Component.onCompleted: Qt.callLater(settle)
    onPaneChanged: Qt.callLater(settle)

    // The edge to drag.
    Rectangle {
        id: edge
        width: parent.width
        height: 6
        z: 2
        color: edgeMouse.containsMouse || edgeMouse.pressed ? Theme.hover : Theme.window
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.separator }
        Rectangle { anchors.centerIn: parent; width: 36; height: 2; radius: 1; color: Theme.fgTertiary }
        MouseArea {
            id: edgeMouse
            anchors.fill: parent
            anchors.topMargin: -3
            hoverEnabled: true
            cursorShape: Qt.SizeVerCursor
            property real startY: 0
            property real startH: 0
            onPressed: (m) => { startY = mapToItem(null, 0, m.y).y; startH = bp.height }
            onPositionChanged: (m) => {
                if (!pressed) return
                const y = mapToItem(null, 0, m.y).y
                const maxH = bp.parent ? bp.parent.height - 120 : 800
                bp.dragH = Math.max(120, Math.min(maxH, startH - (y - startY)))
            }
            onReleased: { if (bp.dragH > 0) { bp.pane.win.setSetting("list_preview_height", Math.round(bp.dragH)); } }
            onDoubleClicked: bp.pane.win.setSetting("list_preview", false)
        }
        GTip { visible: edgeMouse.containsMouse && !edgeMouse.pressed; text: "Drag to resize the preview; double-click to hide it (View Options shows it again)" }
    }
    // Back to the saved height once the setting has it (no jump on release).
    onSavedHChanged: dragH = 0

    Item {
        anchors { top: edge.bottom; left: parent.left; right: parent.right; bottom: parent.bottom }
        FileViewer {
            id: viewer
            anchors { top: parent.top; bottom: parent.bottom; left: parent.left; margins: 8 }
            width: Math.max(160, parent.width - facts.width - 24)
            app: bp.pane ? bp.pane.app : null
            item: bp.item
            active: bp.visible && !!bp.item
        }
        GLabel {
            anchors.centerIn: viewer
            visible: !bp.item
            text: "Select a file to preview it"
            color: Theme.fgTertiary
        }
        Rectangle { x: viewer.x + viewer.width + 8; width: 1; height: parent.height; color: Theme.separator }
        Column {
            id: facts
            anchors { top: parent.top; right: parent.right; topMargin: 12; rightMargin: 14 }
            width: Math.min(300, parent.width * 0.35)
            spacing: 6
            visible: !!bp.item
            GLabel { width: parent.width; text: bp.item ? bp.item.name : ""; font.bold: true; wrapMode: Text.WrapAnywhere; maximumLineCount: 3; elide: Text.ElideRight }
            GLabel { width: parent.width; text: (bp.info.kind || (bp.item ? bp.item.kind : "")) + (bp.info.sizeText ? " — " + bp.info.sizeText : ""); color: Theme.fgSecondary; elide: Text.ElideRight }
            Grid {
                columns: 2
                columnSpacing: 8
                rowSpacing: 4
                visible: bp.info.path !== undefined
                GLabel { text: "Modified"; color: Theme.fgSecondary; font.pixelSize: Theme.small }
                GLabel { text: bp.info.mtime ? new Date(bp.info.mtime * 1000).toLocaleString(Qt.locale(), Locale.ShortFormat) : "--"; font.pixelSize: Theme.small }
                GLabel { text: "Created"; color: Theme.fgSecondary; font.pixelSize: Theme.small }
                GLabel { text: bp.info.btime ? new Date(bp.info.btime * 1000).toLocaleString(Qt.locale(), Locale.ShortFormat) : "--"; font.pixelSize: Theme.small }
                GLabel { text: "Opens with"; color: Theme.fgSecondary; font.pixelSize: Theme.small; visible: !!bp.info.defaultApp }
                GLabel { text: bp.info.defaultApp ? bp.info.defaultApp.name : ""; font.pixelSize: Theme.small; visible: !!bp.info.defaultApp }
            }
            PushButton { text: "More Info…"; tip: "Open the Get Info window (Alt+Return)"; onClicked: bp.pane.win.showInfo([bp.item.path]) }
        }
    }
}
