import QtQuick
import QtQuick.Controls.Basic

// The list and grid views' preview pane, under the files: the selected file itself
// on the left (text, tables, pages, a player, the picture: FileViewer) and
// its details on the right. Drag the top edge to make it taller or
// shorter; double-click the edge for the usual height. F11 / View > Information
// Panel hides and shows it.
Rectangle {
    id: bp
    property var pane
    property var item: null
    property var info: ({})
    property alias viewer: viewer
    // Height while dragging (saved when the drag ends).
    property real dragH: 0
    readonly property real savedH: pane && pane.app && pane.app.settings.list_preview_height > 0 ? pane.app.settings.list_preview_height : 260
    implicitHeight: dragH > 0 ? dragH : savedH
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
            onPressed: (m) => { startY = mapToItem(null, 0, m.y).y; startH = bp.implicitHeight }
            onPositionChanged: (m) => {
                if (!pressed) return
                const y = mapToItem(null, 0, m.y).y
                // (Not bp.parent: that is the Loader, as tall as this pane.)
                const maxH = bp.pane ? bp.pane.height - 160 : 800
                bp.dragH = Math.max(120, Math.min(maxH, startH - (y - startY)))
            }
            onReleased: { if (bp.dragH > 0) { bp.pane.win.setSetting("list_preview_height", Math.round(bp.dragH)); } }
            onDoubleClicked: { bp.dragH = 0; bp.pane.win.setSetting("list_preview_height", 0) }
        }
        GTip { visible: edgeMouse.containsMouse && !edgeMouse.pressed; text: "Drag to resize the preview; double-click for its usual height (F11 hides it)" }
    }
    // Back to the saved height once the setting has it (no jump on release).
    onSavedHChanged: dragH = 0

    // A faint tint behind the card, so the file reads as a thing on a shelf.
    Rectangle { anchors { top: edge.bottom; left: parent.left; right: parent.right; bottom: parent.bottom } color: Theme.rowAlt }

    Item {
        id: body
        anchors { top: edge.bottom; left: parent.left; right: parent.right; bottom: parent.bottom }
        // The details' column: fixed width, narrower in small windows.
        readonly property real factsW: Math.round(Math.max(190, Math.min(280, width * 0.32)))
        // The file itself, in a rounded card.
        Rectangle {
            id: card
            anchors { top: parent.top; bottom: parent.bottom; left: parent.left; right: rule.left; margins: 10 }
            radius: Math.round(8 * Theme.pt)
            color: Theme.content
            border.width: 1
            border.color: Theme.separator
            Item {
                anchors.fill: parent
                anchors.margins: 1
                clip: true
                FileViewer {
                    id: viewer
                    anchors.fill: parent
                    slim: true
                    app: bp.pane ? bp.pane.app : null
                    item: bp.item
                    active: bp.visible && !!bp.item
                }
            }
            GLabel {
                anchors.centerIn: parent
                visible: !bp.item
                text: bp.pane && bp.pane.model && bp.pane.model.selectionCount > 1 ? bp.pane.model.selectionCount + " items selected" : "Select a file to preview it"
                color: Theme.fgTertiary
            }
        }
        Rectangle { id: rule; anchors { right: facts.left; top: parent.top; bottom: parent.bottom; topMargin: 10; bottomMargin: 10 } width: 1; color: Theme.separator; visible: facts.visible }
        // Move it to the side of the window.
        GButton {
            z: 3
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 8
            iconName: "view-split-left-right"
            iconSize: 14
            tip: "Show the Information panel at the side instead (View > Information Panel Below Files)"
            onClicked: bp.pane.win.placeInfo(false)
        }
        Item {
            id: facts
            anchors { top: parent.top; bottom: parent.bottom; right: parent.right }
            width: visible ? body.factsW : 0
            visible: !!bp.item
            clip: true
            Column {
                anchors { top: parent.top; left: parent.left; right: parent.right; margins: 16; topMargin: 14; rightMargin: 40 }
                spacing: 10
                Row {
                    width: parent.width
                    spacing: 10
                    GIcon { id: factIcon; size: 36; name: bp.item ? (bp.item.icon || bp.item.iconName || "") : ""; anchors.verticalCenter: parent.verticalCenter }
                    Column {
                        width: parent.width - factIcon.width - 10
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 2
                        GLabel { width: parent.width; text: bp.item ? bp.item.name : ""; font.bold: true; font.pixelSize: Math.round(Theme.body * 1.08); wrapMode: Text.WrapAnywhere; maximumLineCount: 2; elide: Text.ElideRight }
                        GLabel { width: parent.width; text: (bp.info.kind || (bp.item ? bp.item.kind : "")) + (bp.info.sizeText ? " · " + bp.info.sizeText : ""); color: Theme.fgSecondary; font.pixelSize: Theme.small; elide: Text.ElideRight }
                    }
                }
                Rectangle { width: parent.width; height: 1; color: Theme.separator; visible: bp.info.path !== undefined }
                Grid {
                    id: grid
                    width: parent.width
                    columns: 2
                    columnSpacing: 10
                    rowSpacing: 5
                    visible: bp.info.path !== undefined
                    readonly property real keyW: Math.round(parent.width * 0.36)
                    GLabel { width: grid.keyW; horizontalAlignment: Text.AlignRight; text: "Modified"; color: Theme.fgSecondary; font.pixelSize: Theme.small }
                    GLabel { width: grid.width - grid.keyW - 10; elide: Text.ElideRight; text: bp.info.mtime ? new Date(bp.info.mtime * 1000).toLocaleString(Qt.locale(), Locale.ShortFormat) : "--"; font.pixelSize: Theme.small }
                    GLabel { width: grid.keyW; horizontalAlignment: Text.AlignRight; text: "Created"; color: Theme.fgSecondary; font.pixelSize: Theme.small }
                    GLabel { width: grid.width - grid.keyW - 10; elide: Text.ElideRight; text: bp.info.btime ? new Date(bp.info.btime * 1000).toLocaleString(Qt.locale(), Locale.ShortFormat) : "--"; font.pixelSize: Theme.small }
                    GLabel { width: grid.keyW; horizontalAlignment: Text.AlignRight; text: "Opens with"; color: Theme.fgSecondary; font.pixelSize: Theme.small; visible: !!bp.info.defaultApp }
                    GLabel { width: grid.width - grid.keyW - 10; elide: Text.ElideRight; text: bp.info.defaultApp ? bp.info.defaultApp.name : ""; font.pixelSize: Theme.small; visible: !!bp.info.defaultApp }
                }
                PushButton { text: "More Info…"; tip: "Open the Get Info window (Alt+Return)"; onClicked: bp.pane.win.showInfo([bp.item.path]) }
            }
        }
    }
}
