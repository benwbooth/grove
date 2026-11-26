import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window

// Disk Usage (like KDirStat): scans a folder once (stays on its disk),
// then shows what takes the room two ways that follow each other: a tree
// of folders with percentage bars on top, and a cushion treemap below
// where every file is a tile sized by its space and coloured by its type.
// Click a tile to find it in the tree; double-click to zoom into its folder.
Window {
    id: du
    property var app
    property var win              // the browser window (to show items there)
    property string path: ""
    property int scan: -1
    property var prog: ({})
    readonly property bool ready: !!prog.ready
    property var expanded: [0]
    property int sel: 0
    property int mapRoot: 0
    property bool apparent: false
    property string sort: "size"
    property var rows: []
    property var types: ({ cats: [], exts: [] })
    property int rev: 0
    property var selRect: []
    property string trashAsk: ""

    width: Math.round(980 * Theme.pt)
    height: Math.round(760 * Theme.pt)
    minimumWidth: 560
    minimumHeight: 420
    color: Theme.window
    title: "Disk Usage — " + (path.split("/").filter(s => s).pop() || "/")

    Component.onCompleted: start()
    onClosing: if (scan >= 0) app.grove.duForget(scan)

    function start() {
        if (scan >= 0) app.grove.duForget(scan)
        rows = []; types = { cats: [], exts: [] }; expanded = [0]; sel = 0; mapRoot = 0; selRect = []
        prog = {}
        scan = app.grove.duStart(path)
        poll.start()
    }
    Timer {
        id: poll
        interval: 120; repeat: true
        onTriggered: {
            du.prog = JSON.parse(du.app.grove.duProgress(du.scan))
            if (du.prog.ready || du.prog.error) { stop(); if (du.prog.ready) du.refresh(true) }
        }
    }
    function refresh(all) {
        const s = scan
        Async.call(app.grove, k => app.grove.duRows(k, s, JSON.stringify(expanded), apparent, sort), r => { if (s === du.scan) du.rows = r.rows || [] })
        if (all) {
            Async.call(app.grove, k => app.grove.duTypes(k, s, mapRoot, apparent), r => { if (s === du.scan) du.types = r })
            rev++
        }
    }
    function nodePath(n) { return app.grove.duPath(scan, n) }
    function isOpen(n) { return expanded.indexOf(n) >= 0 }
    function toggle(n, open) {
        const e = expanded.filter(x => x !== n)
        if (open === undefined ? !isOpen(n) : open) e.push(n)
        expanded = e
        refresh(false)
    }
    // Select a node; open every folder above it so it shows in the tree.
    function select(n, reveal) {
        if (n < 0) return
        sel = n
        if (reveal) {
            const chain = JSON.parse(app.grove.duAncestors(scan, n))
            const e = expanded.slice()
            for (const a of chain.slice(0, -1)) if (e.indexOf(a) < 0) e.push(a)
            expanded = e
            pendingScroll = n
            refresh(false)
        }
        updateSelRect()
    }
    property int pendingScroll: -1
    onRowsChanged: {
        if (pendingScroll < 0) return
        for (let i = 0; i < rows.length; i++) if (rows[i].node === pendingScroll) { list.positionViewAtIndex(i, ListView.Contain); break }
        pendingScroll = -1
    }
    function updateSelRect() { selRect = JSON.parse(app.grove.duRect(scan, sel)) }
    function zoom(n) {
        // Zoom the treemap into a folder (a file: its folder).
        let d = n
        const r = rows.find(x => x.node === n)
        if (r && !r.isDir) d = app.grove.duParent(scan, n)
        if (d < 0) return
        mapRoot = d
        refresh(true)
    }
    function zoomOut() { const p = app.grove.duParent(scan, mapRoot); if (p >= 0) { mapRoot = p; refresh(true) } }
    function selRow() { return rows.find(x => x.node === sel) }
    function openSel() { if (sel >= 0) app.grove.openPaths(JSON.stringify([nodePath(sel)])) }
    function showSel() {
        const p = nodePath(sel)
        const r = selRow()
        if (r && r.isDir) app.openOrTab([p], [])
        else app.openOrTab([p.substring(0, p.lastIndexOf("/")) || "/"], [p])
    }
    function askTrash() { if (sel > 0) trashAsk = nodePath(sel) }
    function doTrash() {
        const p = trashAsk, n = sel
        trashAsk = ""
        Async.call(app.grove, k => app.grove.trashPaths(k, JSON.stringify([p])), r => {
            if (r.errors && r.errors.length) { errText.text = r.errors.join("\n"); return }
            if (r.denied && r.denied.length) { errText.text = "“" + p.substring(p.lastIndexOf("/") + 1) + "” isn't yours to move to the Trash. Delete it from a Grove window to do it as administrator."; return }
            app.grove.duRemoved(du.scan, n)
            du.sel = du.app.grove.duParent(du.scan, n)
            app.refreshTrash()
            du.refresh(true)
        })
    }
    function fmtDate(t) { return t ? new Date(t * 1000).toLocaleDateString(Qt.locale(), Locale.ShortFormat) : "" }
    function catColor(c) { const x = (types.cats || []).find(k => k.cat === c); return x ? x.color : Theme.fgTertiary }
    // Bars get a colour per depth, like KDirStat's tree.
    readonly property var depthColors: ["#3f7fd9", "#3fae6a", "#d98f3f", "#a85fd1", "#d95f7f", "#3fb3c4", "#a0a03c"]

    Shortcut { sequences: ["Ctrl+W", StandardKey.Close]; onActivated: du.close() }
    Shortcut { sequence: "F5"; onActivated: du.start() }
    Shortcut { sequence: "Delete"; enabled: du.ready; onActivated: du.askTrash() }

    Item {
        anchors.fill: parent
        transform: Translate { x: 0.001; y: 0.001 }

        // ---- toolbar
        Rectangle {
            id: bar
            width: parent.width
            height: Theme.toolbarHeight
            color: Theme.toolbar
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.separator }
            Row {
                anchors.verticalCenter: parent.verticalCenter
                x: 10
                spacing: 6
                GButton { iconName: "folder-open"; tip: "Analyze a different folder"
                    onClicked: du.win.chooseFiles("Analyze Folder", { directory: true }, ps => { if (ps && ps.length) { du.path = ps[0]; du.start() } }) }
                GButton { iconName: "view-refresh"; tip: Help.tip("Scan again (after changes made elsewhere)", "Rescan (F5)"); onClicked: du.start() }
                GButton { iconName: "zoom-out"; enabled: du.ready && du.mapRoot !== 0; tip: "Treemap: zoom back out to the enclosing folder"; onClicked: du.zoomOut() }
                GButton { iconName: "zoom-original"; enabled: du.ready && du.mapRoot !== 0; tip: "Treemap: show the whole scanned folder again"; onClicked: { du.mapRoot = 0; du.refresh(true) } }
                Item { width: 6; height: 1 }
                GButton { iconName: "document-open"; enabled: du.ready && du.sel >= 0; tip: "Open the selected item"; onClicked: du.openSel() }
                GButton { iconName: "system-file-manager|folder"; enabled: du.ready && du.sel >= 0; tip: "Show the selected item in a Grove window"; onClicked: du.showSel() }
                GButton { iconName: "user-trash"; enabled: du.ready && du.sel > 0; tip: "Move the selected item to the Trash (Delete)"; onClicked: du.askTrash() }
            }
            Row {
                anchors.verticalCenter: parent.verticalCenter
                anchors.right: parent.right
                anchors.rightMargin: 12
                spacing: 12
                GCheck { text: "Apparent sizes"; checked: du.apparent; tip: "Off: the space files take on the disk. On: the files' own lengths (sparse files and compression make these differ)."
                    onToggled: { du.apparent = checked; du.refresh(true) } }
                GLabel { anchors.verticalCenter: parent.verticalCenter; color: Theme.fgSecondary; font.pixelSize: Theme.small
                    text: du.ready ? (du.prog.files || 0).toLocaleString(Qt.locale(), "f", 0) + " files in " + (du.prog.secs || 0).toFixed(1) + " s" + (du.prog.unreadable ? " · " + du.prog.unreadable + " unreadable folders" : "") : "" }
            }
        }

        // ---- scanning
        Column {
            visible: !du.ready
            anchors.centerIn: parent
            width: Math.min(parent.width - 60, 520)
            spacing: 10
            BusyDots { running: !du.ready && !du.prog.error; anchors.horizontalCenter: parent.horizontalCenter }
            GLabel { width: parent.width; horizontalAlignment: Text.AlignHCenter; font.pixelSize: Theme.title
                text: du.prog.error ? "Couldn't scan this folder" : du.prog.cancelled ? "Stopping…" : "Scanning " + du.path + "…" ; wrapMode: Text.WrapAnywhere; elide: Text.ElideNone }
            GLabel { width: parent.width; horizontalAlignment: Text.AlignHCenter; color: Theme.fgSecondary
                text: du.prog.error ? du.prog.error : (du.prog.files || 0).toLocaleString(Qt.locale(), "f", 0) + " files, " + (du.prog.bytesText || "0 bytes") + " so far" }
            GLabel { width: parent.width; horizontalAlignment: Text.AlignHCenter; color: Theme.fgTertiary; font.pixelSize: Theme.small; elide: Text.ElideMiddle; text: du.prog.current || "" }
            PushButton { anchors.horizontalCenter: parent.horizontalCenter; visible: !du.prog.error; text: "Stop"; tip: "Stop scanning (what was counted so far is shown)"; onClicked: du.app.grove.duCancel(du.scan) }
        }

        // ---- tree (top) and treemap (bottom)
        SplitView {
            id: split
            visible: du.ready
            anchors { top: bar.bottom; left: parent.left; right: parent.right; bottom: parent.bottom }
            orientation: Qt.Vertical
            handle: Rectangle {
                implicitHeight: 6
                color: SplitHandle.hovered || SplitHandle.pressed ? Theme.hover : Theme.window
                Rectangle { anchors.centerIn: parent; width: 36; height: 2; radius: 1; color: Theme.fgTertiary }
            }

            Item {
                SplitView.preferredHeight: split.height * 0.5
                SplitView.minimumHeight: 120
                // column header (click to sort)
                Row {
                    id: header
                    width: parent.width
                    height: Theme.rowHeight + 4
                    component Head: Item {
                        property string label; property string key: ""; property int w: 100; property string tipText
                        width: w; height: parent.height
                        GLabel { anchors.verticalCenter: parent.verticalCenter; x: 8; text: parent.label + (du.sort === parent.key ? "  ▾" : ""); font.pixelSize: Theme.small; color: Theme.fgSecondary; font.bold: du.sort === parent.key }
                        MouseArea { id: hm; anchors.fill: parent; hoverEnabled: true; enabled: parent.key !== ""; onClicked: { du.sort = parent.key; du.refresh(false) } }
                        GTip { text: parent.tipText; visible: hm.containsMouse && parent.tipText !== "" }
                    }
                    Head { label: "Name"; key: "name"; w: header.width - 120 - 64 - 90 - 80 - 100; tipText: "Sort by name" }
                    Head { label: "Share"; w: 120; tipText: "How much of the enclosing folder this takes" }
                    Head { label: "%"; w: 64 }
                    Head { label: "Size"; key: "size"; w: 90; tipText: "Sort by size (biggest first)" }
                    Head { label: "Items"; key: "items"; w: 80; tipText: "Sort by how many files and folders are inside" }
                    Head { label: "Last Change"; key: "date"; w: 100; tipText: "Sort by the newest change anywhere inside" }
                }
                Rectangle { anchors.top: header.bottom; width: parent.width; height: 1; color: Theme.separator }
                Item {
                    anchors { top: header.bottom; topMargin: 1; left: parent.left; right: parent.right; bottom: parent.bottom }
                    KineticWheel { flickable: list; notch: Theme.rowHeight * 3 }
                    ListView {
                        id: list
                        anchors.fill: parent
                        clip: true
                        model: du.rows
                        focus: true
                        boundsBehavior: Flickable.StopAtBounds
                        ScrollBar.vertical: GScrollBar {}
                        Keys.onPressed: (e) => {
                            const i = du.rows.findIndex(r => r.node === du.sel)
                            if (e.key === Qt.Key_Down && i + 1 < du.rows.length) { du.select(du.rows[i + 1].node); list.positionViewAtIndex(i + 1, ListView.Contain); e.accepted = true }
                            else if (e.key === Qt.Key_Up && i > 0) { du.select(du.rows[i - 1].node); list.positionViewAtIndex(i - 1, ListView.Contain); e.accepted = true }
                            else if (e.key === Qt.Key_Right && i >= 0 && du.rows[i].hasKids) { du.toggle(du.sel, true); e.accepted = true }
                            else if (e.key === Qt.Key_Left && i >= 0) {
                                if (du.rows[i].expanded) du.toggle(du.sel, false)
                                else { const p = du.app.grove.duParent(du.scan, du.sel); if (p >= 0) du.select(p, true) }
                                e.accepted = true
                            }
                            else if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) { du.openSel(); e.accepted = true }
                        }
                        delegate: Rectangle {
                            id: rowItem
                            required property var modelData
                            required property int index
                            readonly property var r: modelData
                            readonly property bool chosen: r.node === du.sel && r.node >= 0
                            width: list.width
                            height: Theme.rowHeight
                            color: chosen ? Theme.accent : index % 2 ? Theme.rowAlt : "transparent"
                            readonly property color textColor: chosen ? Theme.accentText : Theme.fg
                            MouseArea {
                                anchors.fill: parent
                                acceptedButtons: Qt.LeftButton | Qt.RightButton
                                onClicked: (m) => { list.forceActiveFocus(); if (rowItem.r.node >= 0) du.select(rowItem.r.node); if (m.button === Qt.RightButton && rowItem.r.node >= 0) menu.popup() }
                                onDoubleClicked: { if (rowItem.r.hasKids) du.toggle(rowItem.r.node); else if (rowItem.r.node >= 0) du.openSel() }
                            }
                            Row {
                                anchors.verticalCenter: parent.verticalCenter
                                height: parent.height
                                Item {
                                    width: header.width - 120 - 64 - 90 - 80 - 100
                                    height: parent.height
                                    clip: true
                                    Row {
                                        x: 6 + rowItem.r.depth * 16
                                        anchors.verticalCenter: parent.verticalCenter
                                        spacing: 4
                                        Item {
                                            width: 14; height: 14
                                            anchors.verticalCenter: parent.verticalCenter
                                            GIcon { anchors.centerIn: parent; visible: rowItem.r.hasKids; size: 12; name: rowItem.r.expanded ? "go-down" : "go-next"; tint: rowItem.textColor }
                                            MouseArea { anchors.fill: parent; anchors.margins: -4; enabled: rowItem.r.hasKids; onClicked: du.toggle(rowItem.r.node) }
                                        }
                                        Item {
                                            width: 16; height: 16
                                            anchors.verticalCenter: parent.verticalCenter
                                            GIcon { anchors.centerIn: parent; visible: rowItem.r.isDir; size: 16; name: "folder" }
                                            Rectangle { anchors.centerIn: parent; visible: !rowItem.r.isDir && rowItem.r.cat >= 0; width: 10; height: 10; radius: 2; color: du.catColor(rowItem.r.cat) }
                                        }
                                        GLabel {
                                            anchors.verticalCenter: parent.verticalCenter
                                            text: rowItem.r.depth === 0 ? du.path : rowItem.r.name
                                            color: rowItem.r.node < 0 ? Theme.fgSecondary : rowItem.textColor
                                            font.italic: rowItem.r.node < 0
                                        }
                                        GIcon { visible: !!rowItem.r.error; size: 14; name: "dialog-warning"; anchors.verticalCenter: parent.verticalCenter }
                                    }
                                }
                                Item {
                                    width: 120; height: parent.height
                                    Rectangle {
                                        anchors.verticalCenter: parent.verticalCenter
                                        x: 6; width: parent.width - 12; height: 10; radius: 3
                                        color: Theme.field
                                        border.color: Theme.separator
                                        Rectangle {
                                            width: Math.max(rowItem.r.pct > 0 ? 2 : 0, parent.width * rowItem.r.pct)
                                            height: parent.height; radius: 3
                                            color: du.depthColors[rowItem.r.depth % du.depthColors.length]
                                        }
                                    }
                                }
                                GLabel { width: 64; height: parent.height; verticalAlignment: Text.AlignVCenter; horizontalAlignment: Text.AlignRight; rightPadding: 8; color: rowItem.textColor; text: (rowItem.r.pct * 100).toFixed(1) + "%" }
                                GLabel { width: 90; height: parent.height; verticalAlignment: Text.AlignVCenter; horizontalAlignment: Text.AlignRight; rightPadding: 8; color: rowItem.textColor; text: rowItem.r.sizeText }
                                GLabel { width: 80; height: parent.height; verticalAlignment: Text.AlignVCenter; horizontalAlignment: Text.AlignRight; rightPadding: 8; color: rowItem.chosen ? Theme.accentText : Theme.fgSecondary; text: rowItem.r.isDir ? rowItem.r.items.toLocaleString(Qt.locale(), "f", 0) : "" }
                                GLabel { width: 100; height: parent.height; verticalAlignment: Text.AlignVCenter; leftPadding: 8; color: rowItem.chosen ? Theme.accentText : Theme.fgSecondary; text: du.fmtDate(rowItem.r.mtime) }
                            }
                        }
                    }
                }
            }

            Item {
                SplitView.minimumHeight: 120
                // legend: file types (click a colour to see what's in it)
                Flow {
                    id: legend
                    anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: 8 }
                    spacing: 14
                    Repeater {
                        model: du.types.cats || []
                        Row {
                            required property var modelData
                            spacing: 5
                            Rectangle { width: 11; height: 11; radius: 2; color: parent.modelData.color; anchors.verticalCenter: parent.verticalCenter }
                            GLabel { font.pixelSize: Theme.small; text: parent.modelData.name + " " + parent.modelData.sizeText; color: Theme.fgSecondary
                                HoverHandler { id: lh }
                                GTip { visible: lh.hovered; text: parent.parent.modelData.files.toLocaleString(Qt.locale(), "f", 0) + " files. Biggest kinds: " + (du.types.exts || []).filter(e => e.color === parent.parent.modelData.color).slice(0, 6).map(e => e.ext + " " + e.sizeText).join(", ") }
                            }
                        }
                    }
                }
                Item {
                    id: mapBox
                    anchors { left: parent.left; right: parent.right; top: parent.top; bottom: legend.top; margins: 8; bottomMargin: 8 }
                    clip: true
                    // Redraw after resizing settles (each size is a fresh render).
                    property size want: Qt.size(Math.max(1, Math.round(width)), Math.max(1, Math.round(height)))
                    property size drawn: Qt.size(1, 1)
                    onWantChanged: settle.restart()
                    Timer { id: settle; interval: 120; onTriggered: { mapBox.drawn = mapBox.want } }
                    Image {
                        id: map
                        width: mapBox.drawn.width
                        height: mapBox.drawn.height
                        asynchronous: true
                        cache: false
                        smooth: false
                        sourceSize: mapBox.drawn
                        source: du.ready && mapBox.drawn.width > 1 ? "image://treemap/" + du.scan + "/" + du.mapRoot + "/" + (du.apparent ? 1 : 0) + "/" + du.rev + "x" + mapBox.drawn.width + "x" + mapBox.drawn.height : ""
                        onStatusChanged: if (status === Image.Ready) du.updateSelRect()
                    }
                    PreviewSpinner { image: map; anchors.centerIn: parent; size: 24 }
                    Rectangle {
                        visible: du.selRect.length === 4 && map.status === Image.Ready
                        x: du.selRect[0] || 0; y: du.selRect[1] || 0
                        width: Math.max(3, du.selRect[2] || 0); height: Math.max(3, du.selRect[3] || 0)
                        color: "transparent"
                        border.color: "white"; border.width: 2
                        Rectangle { anchors.fill: parent; anchors.margins: -1; color: "transparent"; border.color: "black"; border.width: 1 }
                    }
                    MouseArea {
                        id: mapMouse
                        anchors.fill: map
                        hoverEnabled: true
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        property int hoverNode: -1
                        onPositionChanged: (m) => { hoverNode = du.app.grove.duHit(du.scan, m.x, m.y) }
                        onExited: hoverNode = -1
                        onClicked: (m) => { const n = du.app.grove.duHit(du.scan, m.x, m.y); du.select(n, true); list.forceActiveFocus(); if (m.button === Qt.RightButton && n >= 0) menu.popup() }
                        onDoubleClicked: (m) => du.zoom(du.app.grove.duHit(du.scan, m.x, m.y))
                    }
                    GTip {
                        visible: mapMouse.containsMouse && mapMouse.hoverNode >= 0
                        text: mapMouse.hoverNode >= 0 ? du.nodePath(mapMouse.hoverNode) : ""
                    }
                }
            }
        }

        // ---- move-to-Trash confirmation and errors
        Rectangle {
            visible: du.trashAsk !== "" || errText.text !== ""
            anchors.horizontalCenter: parent.horizontalCenter
            y: bar.height + 12
            width: Math.min(parent.width - 40, 520)
            height: askCol.implicitHeight + 24
            radius: Theme.radius
            color: Theme.glassMenu
            border.color: Theme.glassRim
            Column {
                id: askCol
                x: 14; y: 12
                width: parent.width - 28
                spacing: 10
                GLabel { width: parent.width; wrapMode: Text.WrapAnywhere; elide: Text.ElideNone; font.bold: true
                    text: errText.text !== "" ? "Couldn't move it to the Trash" : "Move “" + du.trashAsk.split("/").pop() + "” to the Trash?" }
                GLabel { id: errText; width: parent.width; wrapMode: Text.Wrap; elide: Text.ElideNone; color: Theme.fgSecondary; visible: text !== ""; text: "" }
                GLabel { width: parent.width; wrapMode: Text.Wrap; elide: Text.ElideNone; color: Theme.fgSecondary; visible: errText.text === ""
                    text: (du.selRow() ? du.selRow().sizeText + " will be freed when the Trash is emptied. " : "") + "You can put it back from the Trash." }
                Row {
                    anchors.right: parent.right
                    spacing: 8
                    PushButton { text: errText.text !== "" ? "OK" : "Cancel"; tip: "Keep it"; onClicked: { du.trashAsk = ""; errText.text = "" } }
                    PushButton { visible: errText.text === ""; text: "Move to Trash"; destructive: true; tip: "Move it to the Trash"; onClicked: du.doTrash() }
                }
            }
        }
    }

    GMenu {
        id: menu
        GMenuItem { text: "Open"; iconName: "document-open"; onTriggered: du.openSel() }
        GMenuItem { text: "Show in Grove"; iconName: "system-file-manager"; onTriggered: du.showSel() }
        GMenuItem { text: "Zoom Treemap Here"; iconName: "zoom-in"; onTriggered: du.zoom(du.sel) }
        GMenuItem { text: "Copy Path"; iconName: "edit-copy-path"; onTriggered: du.app.grove.copyText(du.nodePath(du.sel)) }
        GMenuSeparator {}
        GMenuItem { text: "Move to Trash"; iconName: "user-trash"; enabled: du.sel > 0; onTriggered: du.askTrash() }
    }
}
