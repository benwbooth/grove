import QtQuick
import QtQuick.Controls.Basic

// List view: sortable, resizable columns; folders expand in place with
// their disclosure triangles (Right/Left keys too). Virtualized.
Item {
    id: root
    property var pane
    property var fm: pane.model
    readonly property bool showWhere: pane.searching || pane.path.indexOf("recent:") === 0 || pane.inTrash || pane.inArchive
    property real wName: Math.max(220, width - wDate - wSize - wKind - wTags - (showWhere ? wWhere : 0) - 30)
    property real wDate: Math.round(170 * Theme.pt)
    property real wSize: Math.round(80 * Theme.pt)
    property real wKind: Math.round(130 * Theme.pt)
    property real wTags: Math.round(70 * Theme.pt)
    property real wWhere: Math.round(200 * Theme.pt)
    readonly property int rowH: Theme.rowHeight
    readonly property bool thumbs: !pane.app || pane.app.settings.show_thumbnails !== false

    function positionAt(row) { if (row >= 0) list.positionViewAtIndex(row, ListView.Contain) }
    function handleKey(e) {
        const shift = (e.modifiers & Qt.ShiftModifier) !== 0
        if (e.key === Qt.Key_Down && !(e.modifiers & Qt.ControlModifier)) { pane.step(1, shift); return true }
        if (e.key === Qt.Key_Up && !(e.modifiers & Qt.ControlModifier)) { pane.step(-1, shift); return true }
        if (e.key === Qt.Key_Home) { pane.step(-fm.count, shift); return true }
        if (e.key === Qt.Key_End) { pane.step(fm.count, shift); return true }
        if (e.key === Qt.Key_PageDown) { pane.step(Math.floor(list.height / rowH), shift); return true }
        if (e.key === Qt.Key_PageUp) { pane.step(-Math.floor(list.height / rowH), shift); return true }
        if (e.key === Qt.Key_Right || e.key === Qt.Key_Left) {
            const r = fm.currentRow
            if (r < 0) return true
            const it = pane.itemAt(r)
            if (e.key === Qt.Key_Right && it.isDir && !it.expanded) fm.toggleExpanded(r)
            else if (e.key === Qt.Key_Left && it.isDir && it.expanded) fm.toggleExpanded(r)
            else if (e.key === Qt.Key_Left && it.depth > 0) {
                const parent = it.path.substring(0, it.path.lastIndexOf("/"))
                const pr = fm.rowOf(parent)
                if (pr >= 0) { fm.select(pr, "set"); positionAt(pr) }
            }
            return true
        }
        return false
    }

    component HeaderCell: Item {
        id: hc
        property string label: ""
        property string key: ""
        property string tip: ""
        property real w: 100
        signal resized(real w)
        width: w
        height: parent.height
        readonly property bool sorted: root.fm.sortKey === key
        GLabel {
            x: 8
            width: parent.width - 24
            anchors.verticalCenter: parent.verticalCenter
            text: hc.label
            font.pixelSize: Theme.small
            font.bold: hc.sorted
            color: hc.sorted ? Theme.fg : Theme.fgSecondary
        }
        GLabel {
            visible: hc.sorted
            anchors.right: parent.right; anchors.rightMargin: 10
            anchors.verticalCenter: parent.verticalCenter
            text: root.fm.descending ? "▾" : "▴"
            font.pixelSize: Theme.small
            color: Theme.fgSecondary
        }
        MouseArea {
            id: hm
            anchors.fill: parent
            hoverEnabled: true
            enabled: hc.key !== ""
            onClicked: root.pane.sortBy(hc.key)
        }
        GTip { visible: hm.containsMouse && hc.tip !== ""; text: hc.tip }
        Rectangle { anchors.right: parent.right; width: 1; height: parent.height - 8; anchors.verticalCenter: parent.verticalCenter; color: Theme.separator }
        MouseArea {
            width: 8; height: parent.height
            anchors.right: parent.right; anchors.rightMargin: -4
            cursorShape: Qt.SplitHCursor
            property real startX: 0
            property real startW: 0
            onPressed: (m) => { startX = mapToItem(root, m.x, 0).x; startW = hc.w }
            onPositionChanged: (m) => { if (pressed) hc.resized(Math.max(50, startW + mapToItem(root, m.x, 0).x - startX)) }
        }
    }

    Rectangle {
        id: header
        width: parent.width
        height: Math.round(26 * Theme.pt)
        color: Theme.content
        z: 3
        Row {
            anchors.fill: parent
            anchors.leftMargin: 10
            HeaderCell { label: "Name"; key: "name"; w: root.wName; tip: "Sort by name (click again to reverse)" }
            HeaderCell { label: root.pane.inTrash ? "Original Location" : "Where"; w: root.wWhere; visible: root.showWhere; onResized: (w) => root.wWhere = w }
            HeaderCell { label: root.pane.inTrash ? "Date Deleted" : "Date Modified"; key: root.pane.inTrash ? "created" : "modified"; w: root.wDate; tip: "Sort by date (newest first)"; onResized: (w) => root.wDate = w }
            HeaderCell { label: "Size"; key: "size"; w: root.wSize; tip: "Sort by size (largest first)"; onResized: (w) => root.wSize = w }
            HeaderCell { label: "Kind"; key: "kind"; w: root.wKind; tip: "Sort by kind of file"; onResized: (w) => root.wKind = w }
            HeaderCell { label: "Tags"; w: root.wTags; onResized: (w) => root.wTags = w }
        }
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.separator }
    }

    MouseArea {
        anchors.fill: list
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onPressed: (m) => {
            root.pane.activated()
            root.pane.forceActiveFocus()
            if (m.button === Qt.RightButton) root.pane.showBackgroundMenu()
            else if (!(m.modifiers & (Qt.ControlModifier | Qt.ShiftModifier))) root.fm.clearSelection()
            root.pane.renamingRow = -1
        }
    }

    ListView {
        id: list
        anchors.top: header.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.topMargin: 2
        model: root.fm
        interactive: false
        clip: true
        reuseItems: true
        cacheBuffer: root.rowH * 20
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

        delegate: Item {
            id: rowItem
            required property int index
            required property string name
            required property string path
            required property bool isDir
            required property bool isLink
            required property bool selected
            required property bool expanded
            required property int depth
            required property string iconName
            required property string dateText
            required property string createdText
            required property string sizeText
            required property string kind
            required property string tags
            required property string tagColors
            required property string note
            required property string git
            required property bool cut
            required property bool hidden
            required property bool thumb
            required property real mtime
            width: list.width
            height: root.rowH
            readonly property bool renaming: root.pane.renamingRow === index

            Rectangle {
                anchors.fill: parent
                anchors.leftMargin: 6
                anchors.rightMargin: 6
                radius: 5
                color: rowItem.selected ? (root.pane.active ? Theme.accent : Theme.selectedInactive)
                     : drop.hovering ? Theme.dropTarget
                     : rowItem.index % 2 ? Theme.rowAlt : "transparent"
            }
            readonly property color textColor: rowItem.selected && root.pane.active ? Theme.accentText : Theme.fg
            readonly property color dimColor: rowItem.selected && root.pane.active ? Qt.rgba(1, 1, 1, 0.8) : Theme.fgSecondary
            Row {
                x: 10
                height: parent.height
                opacity: rowItem.cut || rowItem.hidden ? 0.55 : 1
                Item {
                    width: root.wName
                    height: parent.height
                    Item {
                        id: disclosure
                        x: rowItem.depth * 16
                        width: 16; height: parent.height
                        GLabel {
                            anchors.centerIn: parent
                            visible: rowItem.isDir && !root.pane.special
                            text: "›"
                            rotation: rowItem.expanded ? 90 : 0
                            font.pixelSize: Theme.title
                            color: rowItem.dimColor
                            Behavior on rotation { NumberAnimation { duration: 120 } }
                        }
                        MouseArea {
                            anchors.fill: parent
                            enabled: rowItem.isDir && !root.pane.special
                            onClicked: root.fm.toggleExpanded(rowItem.index)
                        }
                    }
                    GIcon {
                        id: ic
                        anchors.left: disclosure.right
                        anchors.leftMargin: 2
                        anchors.verticalCenter: parent.verticalCenter
                        size: 18
                        name: rowItem.iconName
                        visible: th.status !== Image.Ready
                    }
                    Image {
                        id: th
                        anchors.fill: ic
                        fillMode: Image.PreserveAspectFit
                        asynchronous: true
                        sourceSize.width: Math.ceil(18 * Screen.devicePixelRatio)
                        sourceSize.height: Math.ceil(18 * Screen.devicePixelRatio)
                        source: rowItem.thumb && root.thumbs && root.pane.app.settings.list_thumbnails ? "image://thumb/" + encodeURIComponent(rowItem.path) + "?m=" + rowItem.mtime : ""
                    }
                    GitBadge { status: rowItem.git; anchors.right: ic.right; anchors.bottom: ic.bottom; anchors.margins: -4; scale: 0.8 }
                    GLabel {
                        anchors.left: ic.right
                        anchors.leftMargin: 6
                        anchors.right: parent.right
                        anchors.rightMargin: 6
                        anchors.verticalCenter: parent.verticalCenter
                        visible: !rowItem.renaming
                        text: rowItem.name + (rowItem.isLink ? "  ↗" : "")
                        elide: Text.ElideMiddle
                        color: rowItem.textColor
                    }
                    Loader {
                        active: rowItem.renaming
                        anchors.left: ic.right
                        anchors.leftMargin: 4
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        sourceComponent: RenameField { pane: root.pane; row: rowItem.index; original: rowItem.name; isDir: rowItem.isDir }
                    }
                }
                GLabel { visible: root.showWhere; width: root.wWhere; leftPadding: 8; anchors.verticalCenter: parent.verticalCenter; text: rowItem.note; color: rowItem.dimColor; elide: Text.ElideMiddle }
                GLabel { width: root.wDate; leftPadding: 8; anchors.verticalCenter: parent.verticalCenter; text: root.pane.inTrash ? rowItem.createdText : rowItem.dateText; color: rowItem.dimColor }
                GLabel { width: root.wSize; leftPadding: 8; rightPadding: 12; horizontalAlignment: Text.AlignRight; anchors.verticalCenter: parent.verticalCenter; text: rowItem.sizeText; color: rowItem.dimColor }
                GLabel { width: root.wKind; leftPadding: 8; anchors.verticalCenter: parent.verticalCenter; text: rowItem.kind; color: rowItem.dimColor }
                Item {
                    width: root.wTags; height: parent.height
                    TagDots { x: 8; anchors.verticalCenter: parent.verticalCenter; tags: rowItem.tags; colors: rowItem.tagColors }
                }
            }
            ItemMouse {
                id: mouse
                anchors.fill: parent
                anchors.leftMargin: 10 + (rowItem.depth + 1) * 16
                pane: root.pane
                row: rowItem.index
                iconUrl: rowItem.iconName ? "image://icon/" + rowItem.iconName : ""
                enabled: !rowItem.renaming
            }
            FolderDrop {
                id: drop
                anchors.fill: parent
                pane: root.pane
                path: rowItem.path
                enabledFor: rowItem.isDir && !root.pane.special
            }
        }
    }

    WheelHandler {
        acceptedModifiers: Qt.NoModifier
        onWheel: (e) => {
            const d = e.pixelDelta.y !== 0 ? e.pixelDelta.y : e.angleDelta.y / 120 * root.rowH * 3
            list.contentY = Math.max(list.originY, Math.min(list.contentY - d, list.originY + Math.max(0, list.contentHeight - list.height)))
        }
    }
}
