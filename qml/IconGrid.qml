import QtQuick
import QtQuick.Controls.Basic

// Icon view: a virtualized grid (only visible items exist, so 100 000
// files scroll smoothly), thumbnails from the freedesktop cache, rubber-band
// selection, zoom with Ctrl+wheel or the slider.
Item {
    id: root
    property var pane
    property var fm: pane.model
    readonly property int icon: pane.iconSize
    readonly property int labelLines: 2
    readonly property real cellW: Math.max(icon + 36, Math.round(icon * 1.45) + 20)
    readonly property real cellH: icon + 14 + Math.ceil(Theme.body * 1.3) * labelLines + 12
    readonly property int cols: Math.max(1, Math.floor((grid.width) / cellW))
    readonly property bool thumbs: pane.app.settings.show_thumbnails !== false

    function positionAt(row) { if (row >= 0) grid.positionViewAtIndex(row, GridView.Contain) }
    function handleKey(e) {
        const shift = (e.modifiers & Qt.ShiftModifier) !== 0
        if (e.key === Qt.Key_Right) { pane.step(1, shift); return true }
        if (e.key === Qt.Key_Left) { pane.step(-1, shift); return true }
        if (e.key === Qt.Key_Down && !(e.modifiers & Qt.ControlModifier)) { pane.step(cols, shift); return true }
        if (e.key === Qt.Key_Up && !(e.modifiers & Qt.ControlModifier)) { pane.step(-cols, shift); return true }
        if (e.key === Qt.Key_Home) { pane.step(-fm.count, shift); return true }
        if (e.key === Qt.Key_End) { pane.step(fm.count, shift); return true }
        if (e.key === Qt.Key_PageDown) { pane.step(cols * Math.floor(grid.height / cellH), shift); return true }
        if (e.key === Qt.Key_PageUp) { pane.step(-cols * Math.floor(grid.height / cellH), shift); return true }
        return false
    }

    // Background: rubber band, clicks on empty space, the folder menu.
    MouseArea {
        id: band
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        property point start
        property bool active: false
        property bool additive: false
        onPressed: (m) => {
            pane.activated()
            pane.forceActiveFocus()
            if (m.button === Qt.RightButton) { pane.showBackgroundMenu(); return }
            start = Qt.point(m.x, m.y + grid.contentY - grid.originY)
            additive = (m.modifiers & (Qt.ControlModifier | Qt.ShiftModifier)) !== 0
            if (!additive) fm.clearSelection()
            pane.renamingRow = -1
            active = true
        }
        onPositionChanged: (m) => {
            if (!active) return
            const cur = Qt.point(m.x, m.y + grid.contentY - grid.originY)
            bandRect.x = Math.min(start.x, cur.x)
            bandRect.y = Math.min(start.y, cur.y) - grid.contentY + grid.originY
            bandRect.width = Math.abs(cur.x - start.x)
            bandRect.height = Math.abs(cur.y - start.y)
            bandRect.visible = bandRect.width > 3 || bandRect.height > 3
            // Rows/columns the band touches.
            const x1 = Math.min(start.x, cur.x) - grid.x, x2 = Math.max(start.x, cur.x) - grid.x
            const y1 = Math.min(start.y, cur.y) - grid.y, y2 = Math.max(start.y, cur.y) - grid.y
            const c1 = Math.max(0, Math.floor(x1 / cellW)), c2 = Math.min(cols - 1, Math.floor(x2 / cellW))
            const r1 = Math.max(0, Math.floor(y1 / cellH)), r2 = Math.floor(y2 / cellH)
            const rows = []
            for (let r = r1; r <= r2; r++)
                for (let c = c1; c <= c2; c++) {
                    const i = r * cols + c
                    if (i < fm.count) rows.push(i)
                }
            fm.selectRows(JSON.stringify(rows), additive)
            // Scroll when dragging past the edges.
            autoScroll.dy = m.y < 20 ? -12 : m.y > height - 20 ? 12 : 0
        }
        onReleased: { active = false; bandRect.visible = false; autoScroll.dy = 0 }
        onDoubleClicked: (m) => { if (m.button === Qt.LeftButton && pane.app.settings.double_click_up) pane.goUp() }
    }
    Timer {
        id: autoScroll
        property int dy: 0
        running: dy !== 0
        interval: 16
        repeat: true
        onTriggered: grid.contentY = Math.max(grid.originY, Math.min(grid.contentY + dy, grid.originY + Math.max(0, grid.contentHeight - grid.height)))
    }

    GridView {
        id: grid
        anchors.fill: parent
        anchors.leftMargin: Math.max(8, (root.width - root.cols * root.cellW) / 2)
        anchors.topMargin: 8
        model: root.fm
        cellWidth: root.cellW
        cellHeight: root.cellH
        interactive: false
        clip: true
        reuseItems: true
        cacheBuffer: Math.round(root.cellH * 2)
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded; parent: root; anchors.right: root.right; anchors.top: root.top; anchors.bottom: root.bottom }

        delegate: Item {
            id: cell
            required property int index
            required property string name
            required property string path
            required property bool isDir
            required property bool isLink
            required property bool selected
            required property string iconName
            required property bool thumb
            required property real mtime
            required property string tags
            required property string tagColors
            required property string git
            required property bool cut
            required property bool hidden
            width: grid.cellWidth
            height: grid.cellHeight
            readonly property bool renaming: root.pane.renamingRow === index

            Rectangle {
                id: iconBg
                anchors.horizontalCenter: parent.horizontalCenter
                y: 4
                width: root.icon + 14
                height: root.icon + 10
                radius: Math.round(8 * Theme.pt)
                color: cell.selected ? Theme.selectedInactive : (drop.hovering ? Theme.dropTarget : (mouse.containsMouse ? Theme.hover : "transparent"))
            }
            GIcon {
                id: ic
                anchors.horizontalCenter: iconBg.horizontalCenter
                anchors.verticalCenter: iconBg.verticalCenter
                size: root.icon
                name: cell.iconName
                visible: th.status !== Image.Ready
                opacity: cell.cut || cell.hidden ? 0.5 : 1
            }
            Image {
                id: th
                anchors.centerIn: iconBg
                width: root.icon
                height: root.icon
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                cache: true
                smooth: true
                sourceSize.width: Math.ceil(root.icon * Screen.devicePixelRatio)
                sourceSize.height: Math.ceil(root.icon * Screen.devicePixelRatio)
                source: cell.thumb && root.thumbs && root.icon >= 32 ? "image://thumb/" + encodeURIComponent(cell.path) + "?m=" + cell.mtime : ""
                opacity: cell.cut || cell.hidden ? 0.5 : 1
            }
            // Alias arrow.
            Rectangle {
                visible: cell.isLink
                x: iconBg.x + 6; y: iconBg.y + iconBg.height - 18
                width: 14; height: 14; radius: 3
                color: "white"
                border.color: Qt.rgba(0, 0, 0, 0.3)
                GLabel { anchors.centerIn: parent; text: "↗"; color: "black"; font.pixelSize: 10 }
            }
            GitBadge { status: cell.git; x: iconBg.x + iconBg.width - width - 2; y: iconBg.y + 2 }

            Rectangle {
                id: labelBg
                visible: !cell.renaming
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.top: iconBg.bottom
                anchors.topMargin: 3
                width: Math.min(label.implicitWidth, grid.cellWidth - 6) + 10 + (dots.visible ? dots.width + 4 : 0)
                height: label.height + 2
                radius: 5
                color: cell.selected ? Theme.accent : "transparent"
                Row {
                    anchors.centerIn: parent
                    spacing: 4
                    TagDots { id: dots; tags: cell.tags; colors: cell.tagColors; anchors.verticalCenter: parent.verticalCenter }
                    GLabel {
                        id: label
                        width: Math.min(implicitWidth, grid.cellWidth - 16 - (dots.visible ? dots.width + 4 : 0))
                        text: cell.name
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.WrapAnywhere
                        maximumLineCount: cell.selected ? 4 : root.labelLines
                        elide: Text.ElideRight
                        color: cell.selected ? Theme.accentText : Theme.fg
                        opacity: cell.cut || cell.hidden ? 0.6 : 1
                    }
                }
            }
            Loader {
                active: cell.renaming
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.top: iconBg.bottom
                anchors.topMargin: 2
                width: grid.cellWidth - 6
                sourceComponent: RenameField { pane: root.pane; row: cell.index; original: cell.name; isDir: cell.isDir }
            }

            ItemMouse {
                id: mouse
                anchors.fill: iconBg
                anchors.bottomMargin: -(labelBg.height + 3)
                pane: root.pane
                row: cell.index
                iconUrl: th.status === Image.Ready ? th.source : (cell.iconName ? "image://icon/" + cell.iconName : "")
                enabled: !cell.renaming
            }
            FolderDrop {
                id: drop
                anchors.fill: mouse
                pane: root.pane
                path: cell.path
                enabledFor: cell.isDir && !root.pane.special
            }
            GTip {
                visible: mouse.containsMouse && !mouse.pressed && label.truncated
                text: cell.name
            }
        }
    }

    Rectangle {
        id: bandRect
        visible: false
        color: Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.15)
        border.color: Theme.accent
        border.width: 1
    }

    WheelHandler {
        acceptedModifiers: Qt.NoModifier
        onWheel: (e) => {
            const d = e.pixelDelta.y !== 0 ? e.pixelDelta.y : e.angleDelta.y / 120 * root.cellH * 0.6
            grid.contentY = Math.max(grid.originY, Math.min(grid.contentY - d, grid.originY + Math.max(0, grid.contentHeight - grid.height)))
        }
    }
    WheelHandler {
        acceptedModifiers: Qt.ControlModifier
        onWheel: (e) => root.pane.zoom(e.angleDelta.y > 0 ? 1 : -1)
    }
}
