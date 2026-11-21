import QtQuick
import QtQuick.Controls.Basic

// List view (Finder's list, Dolphin's details): sortable, resizable,
// choosable columns (right-click the header); folders expand in place with
// their disclosure triangles (Right/Left keys too). Virtualized.
Item {
    clip: true
    id: root
    property var pane
    property var fm: pane.model
    readonly property bool showWhere: pane.searching || pane.path.indexOf("recent:") === 0 || pane.inTrash || pane.inArchive
    readonly property int rowH: Theme.rowHeight
    readonly property bool thumbs: pane.previewsOn
    readonly property var allColumns: ({
        where: { label: pane.inTrash ? "Original Location" : "Where", w: 200 },
        modified: { label: "Date Modified", w: 170, sort: "modified" },
        created: { label: "Date Created", w: 170, sort: "created" },
        added: { label: "Date Added", w: 170, sort: "added" },
        opened: { label: "Last Opened", w: 170, sort: "accessed" },
        size: { label: "Size", w: 80, sort: "size", right: true },
        kind: { label: "Kind", w: 130, sort: "kind" },
        tags: { label: "Tags", w: 70 },
        rating: { label: "Rating", w: 90 },
        permissions: { label: "Permissions", w: 110 },
        owner: { label: "Owner", w: 90 },
        group: { label: "Group", w: 90 },
        path: { label: "Path", w: 240 }
    })
    readonly property var columns: {
        let c = (pane.app && pane.app.settings.list_columns) ? pane.app.settings.list_columns.slice() : ["modified", "size", "kind", "tags"]
        if (pane.inTrash) c = c.map(k => k === "modified" ? "created" : k)
        if (showWhere && c.indexOf("where") < 0) c.unshift("where")
        return c.filter(k => allColumns[k] !== undefined)
    }
    property var widths: ({})
    function colW(k) { return (widths[k] || allColumns[k].w) * Theme.pt }
    readonly property real fixedW: { let t = 0; for (const k of columns) t += colW(k); return t }
    readonly property real wName: Math.max(220, width - fixedW - 30)

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
    function stars(n) { let s = ""; for (let i = 0; i < 5; i++) s += (n >= (i + 1) * 2) ? "★" : (n === i * 2 + 1 ? "⯪" : "☆"); return n > 0 ? s : "" }

    component HeaderCell: Item {
        id: hc
        property string label: ""
        property string key: ""
        property string col: ""
        property string tip: ""
        property real w: 100
        width: w
        height: parent.height
        readonly property bool sorted: key !== "" && root.fm.sortKey === key
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
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onClicked: (m) => { if (m.button === Qt.RightButton) colMenu.popup(); else if (hc.key !== "") root.pane.sortBy(hc.key) }
        }
        GTip { visible: hm.containsMouse; text: (hc.key ? "Sort by " + hc.label.toLowerCase() + " (click again to reverse). " : "") + "Right-click to choose columns." }
        Rectangle { anchors.right: parent.right; width: 1; height: parent.height - 8; anchors.verticalCenter: parent.verticalCenter; color: Theme.separator }
        MouseArea {
            visible: hc.col !== ""
            width: 8; height: parent.height
            anchors.right: parent.right; anchors.rightMargin: -4
            cursorShape: Qt.SplitHCursor
            property real startX: 0
            property real startW: 0
            onPressed: (m) => { startX = mapToItem(root, m.x, 0).x; startW = hc.w }
            onPositionChanged: (m) => {
                if (!pressed) return
                const w = Object.assign({}, root.widths)
                w[hc.col] = Math.max(50, startW + mapToItem(root, m.x, 0).x - startX) / Theme.pt
                root.widths = w
            }
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
            HeaderCell { label: "Name"; key: "name"; w: root.wName }
            Repeater {
                model: root.columns
                HeaderCell {
                    required property string modelData
                    col: modelData
                    label: root.allColumns[modelData].label
                    key: root.allColumns[modelData].sort || ""
                    w: root.colW(modelData)
                }
            }
        }
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.separator }
    }

    GMenu {
        id: colMenu
        Instantiator {
            onObjectAdded: (i, o) => colMenu.insertItem(i, o)
            onObjectRemoved: (i, o) => colMenu.removeItem(o)
            model: ["modified", "created", "added", "opened", "size", "kind", "tags", "rating", "permissions", "owner", "group", "path"]
            GMenuItem {
                required property string modelData
                text: root.allColumns[modelData].label
                checkable: true
                checked: root.columns.indexOf(modelData) >= 0
                onTriggered: {
                    let c = (root.pane.app.settings.list_columns || ["modified", "size", "kind", "tags"]).slice()
                    const i = c.indexOf(modelData)
                    if (i >= 0) c.splice(i, 1); else c.push(modelData)
                    root.pane.app.grove.setSetting("list_columns", JSON.stringify(c))
                }
            }
        }
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
        section.property: root.pane.groupBy !== "" ? "section" : ""
        section.criteria: ViewSection.FullString
        section.delegate: Item {
            required property string section
            width: list.width
            height: Math.round(30 * Theme.pt)
            GLabel { x: 14; anchors.bottom: parent.bottom; anchors.bottomMargin: 5; text: parent.section; font.bold: true; color: Theme.fgSecondary }
            Rectangle { x: 10; width: parent.width - 20; height: 1; anchors.bottom: parent.bottom; color: Theme.separator }
        }

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
            required property string addedText
            required property string openedText
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
            required property int rating
            required property string permissions
            required property string owner
            required property string group
            required property string fullPath
            width: list.width
            height: root.rowH
            readonly property bool renaming: root.pane.renamingRow === index
            function cell(k) {
                switch (k) {
                case "where": return note
                case "modified": return dateText
                case "created": return createdText
                case "added": return addedText
                case "opened": return openedText
                case "size": return sizeText
                case "kind": return kind
                case "rating": return root.stars(rating)
                case "permissions": return permissions
                case "owner": return owner
                case "group": return group
                case "path": return fullPath
                }
                return ""
            }

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
                    SelectCheck {
                        id: rowCheck
                        visible: root.pane.selectionMode
                        anchors.verticalCenter: parent.verticalCenter
                        pane: root.pane
                        row: rowItem.index
                        checked: rowItem.selected
                    }
                    Item {
                        id: disclosure
                        x: rowItem.depth * 16 + (root.pane.selectionMode ? rowCheck.width + 6 : 0)
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
                        visible: th.status !== Image.Ready || th.implicitWidth <= 1
                    }
                    Image {
                        id: th
                        anchors.fill: ic
                        fillMode: Image.PreserveAspectFit
                        asynchronous: true
                        sourceSize.width: Math.ceil(18 * Screen.devicePixelRatio)
                        sourceSize.height: Math.ceil(18 * Screen.devicePixelRatio)
                        source: rowItem.thumb && root.thumbs && root.pane.app && root.pane.app.settings.list_thumbnails ? "image://thumb/" + encodeURIComponent(rowItem.path) + "?m=" + rowItem.mtime : ""
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
                Repeater {
                    model: root.columns
                    Item {
                        required property string modelData
                        width: root.colW(modelData)
                        height: rowItem.height
                        GLabel {
                            visible: parent.modelData !== "tags"
                            anchors.fill: parent
                            leftPadding: 8
                            rightPadding: 12
                            horizontalAlignment: root.allColumns[parent.modelData].right ? Text.AlignRight : Text.AlignLeft
                            text: rowItem.cell(parent.modelData)
                            color: parent.modelData === "rating" ? Theme.accent : rowItem.dimColor
                            font.family: parent.modelData === "permissions" ? Theme.mono : Theme.family
                            elide: parent.modelData === "where" || parent.modelData === "path" ? Text.ElideMiddle : Text.ElideRight
                        }
                        TagDots { visible: parent.modelData === "tags" && rowItem.tags !== ""; x: 8; anchors.verticalCenter: parent.verticalCenter; tags: rowItem.tags; colors: rowItem.tagColors }
                    }
                }
            }
            ItemMouse {
                id: mouse
                anchors.fill: parent
                anchors.leftMargin: 10 + (rowItem.depth + 1) * 16
                pane: root.pane
                row: rowItem.index
                iconUrl: Theme.iconUrl(rowItem.iconName)
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
