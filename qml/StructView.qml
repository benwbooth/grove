import QtQuick
import QtQuick.Controls.Basic
import Grove

// A JSON, YAML, TOML, XML or INI file as a tree (DocView's Tree view):
// rows with disclosure triangles, each with its key or index, the value
// coloured by type and a type hint; search that opens the containers its
// matches are in; copy a value or its path. TreeDoc parses on a worker and
// holds only the open rows, so a huge file opens at its top level at once.
Item {
    id: sv
    property string path: ""
    property string format: ""
    property bool active: false
    property bool slim: false
    property real zoom: 1
    readonly property alias doc: tdoc
    readonly property alias currentRow: list.currentIndex

    TreeDoc { id: tdoc }
    function reload() {
        search.text = ""
        tdoc.load(active ? path : "", active ? format : "")
    }
    onPathChanged: Qt.callLater(reload)
    onFormatChanged: Qt.callLater(reload)
    onActiveChanged: Qt.callLater(reload)

    function focusSearch() { search.forceActiveFocus(); search.selectAll() }
    function focusTree() { list.forceActiveFocus() }

    // Rows: open/close, move, copy.
    function toggle(row, deep) {
        if (row < 0 || row >= list.count) return
        const open = !tdoc.data(tdoc.index(row, 0), 0x0100 + 8)
        tdoc.setOpen(row, open, deep)
    }
    function rowData(row, role) { return tdoc.data(tdoc.index(row, 0), 0x0100 + ["depth", "key", "keyKind", "kind", "vkind", "value", "summary", "expandable", "expanded", "hit", "node"].indexOf(role)) }
    function go(row) {
        if (list.count === 0) return
        list.currentIndex = Math.max(0, Math.min(list.count - 1, row))
        list.positionViewAtIndex(list.currentIndex, ListView.Contain)
    }
    function goHit(dir) {
        const r = tdoc.nextHit(list.currentIndex, dir)
        if (r >= 0) { list.currentIndex = r; list.positionViewAtIndex(r, ListView.Center) }
    }
    function copy(what) { if (list.currentIndex >= 0) tdoc.copy(list.currentIndex, what) }
    property string copiedText: ""
    Connections {
        target: tdoc
        function onCopied(what) { sv.copiedText = what === "path" ? "Path copied" : what === "key" ? "Key copied" : "Value copied"; copiedTimer.restart() }
        function onSearchDone() {
            if (tdoc.hitCount > 0) { list.currentIndex = tdoc.nextHit(-1, 1); list.positionViewAtIndex(list.currentIndex, ListView.Center) }
        }
    }
    Timer { id: copiedTimer; interval: 1600; onTriggered: sv.copiedText = "" }

    // Tests (GROVE_TEST qlview=...): "tree:expandall", "tree:collapseall",
    // "tree:search:TEXT", "tree:open:ROW", "tree:select:ROW", "tree:next",
    // "tree:copy:path", "tree:dump".
    function testAction(a) {
        if (a === "tree:expandall") tdoc.expandAll()
        else if (a === "tree:collapseall") tdoc.collapseAll()
        else if (a.startsWith("tree:search:")) search.text = a.substring(12)
        else if (a.startsWith("tree:open:")) tdoc.setOpen(parseInt(a.substring(10)), true, false)
        else if (a.startsWith("tree:select:")) go(parseInt(a.substring(12)))
        else if (a === "tree:next") goHit(1)
        else if (a.startsWith("tree:copy:")) copy(a.substring(10))
        else if (a === "tree:menu") { rowMenu.row = list.currentIndex; rowMenu.popup(sv, sv.width / 3, 80) }
        let rows = []
        for (let i = 0; i < Math.min(list.count, 12); i++) rows.push("  ".repeat(rowData(i, "depth")) + rowData(i, "key") + " = " + (rowData(i, "value") || rowData(i, "summary")) + " (" + rowData(i, "kind") + ")" + (rowData(i, "hit") ? " *" : ""))
        console.warn("grove test tree", a, "rows", list.count, "values", tdoc.values, "error", JSON.stringify(tdoc.error), tdoc.errorLine, tdoc.errorCol, "hits", tdoc.hitCount, tdoc.hitTotal, "current", list.currentIndex, list.currentIndex >= 0 ? tdoc.pathOf(list.currentIndex) : "", "\n" + rows.join("\n"))
    }

    readonly property real rowH: Math.round(Theme.body * zoom * 1.62)
    readonly property real indent: Math.round(16 * zoom)
    // Value colours, by type (dark and light).
    readonly property color cString: Theme.dark ? "#a5d6a7" : "#1a7f37"
    readonly property color cNumber: Theme.dark ? "#79c0ff" : "#0550ae"
    readonly property color cBool: Theme.dark ? "#d2a8ff" : "#8250df"
    readonly property color cDate: Theme.dark ? "#56d4dd" : "#0b7285"
    readonly property color cAlias: Theme.dark ? "#ffa657" : "#bc4c00"
    readonly property color cKey: Theme.dark ? "#f0b4f0" : "#953800"
    readonly property color cAttr: Theme.dark ? "#ffb86b" : "#b35900"
    readonly property color cElement: Theme.dark ? "#8ab4f8" : "#1f4fbf"
    function valueColor(vk) {
        return vk === "string" ? cString : vk === "number" ? cNumber : vk === "boolean" ? cBool : vk === "date" ? cDate : vk === "alias" ? cAlias : Theme.fgSecondary
    }
    function keyColor(kk) {
        return kk === "index" || kk === "special" ? Theme.fgSecondary : kk === "attr" ? cAttr : kk === "element" ? cElement : cKey
    }

    // Search, matches, open/close everything.
    Item {
        id: bar
        width: parent.width
        height: Math.round((sv.slim ? 30 : 36) * Theme.pt)
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.separator }
        GField {
            id: search
            x: sv.slim ? 6 : 10
            anchors.verticalCenter: parent.verticalCenter
            width: Math.max(90, Math.min(300, parent.width - tools.width - x - 24))
            height: Math.round((sv.slim ? 22 : 26) * Theme.pt)
            iconName: "edit-find"
            clearable: true
            placeholderText: "Search keys and values"
            tip: "Show the rows whose key or value has this text (Ctrl+F); Enter goes to the next match, Shift+Enter the previous, Esc clears"
            font.pixelSize: Theme.small
            onTextChanged: searchTimer.restart()
            Keys.onReturnPressed: (e) => { sv.goHit((e.modifiers & Qt.ShiftModifier) ? -1 : 1); e.accepted = true }
            Keys.onEnterPressed: (e) => { sv.goHit((e.modifiers & Qt.ShiftModifier) ? -1 : 1); e.accepted = true }
            Keys.onEscapePressed: (e) => { if (text !== "") { text = ""; e.accepted = true } else { list.forceActiveFocus(); e.accepted = true } }
            Keys.onDownPressed: list.forceActiveFocus()
        }
        Timer { id: searchTimer; interval: 220; onTriggered: tdoc.search(search.text) }
        Row {
            id: tools
            anchors.right: parent.right
            anchors.rightMargin: sv.slim ? 4 : 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: sv.slim ? 2 : 4
            GLabel {
                anchors.verticalCenter: parent.verticalCenter
                visible: text !== ""
                color: sv.copiedText !== "" ? Theme.accent : Theme.fgSecondary
                font.pixelSize: Theme.small
                rightPadding: 4
                text: {
                    if (sv.copiedText !== "") return sv.copiedText
                    if (tdoc.searching) return "Searching…"
                    if (search.text.trim() === "") return ""
                    if (tdoc.hitTotal === 0) return "No matches"
                    const n = tdoc.hitNumber(list.currentIndex)
                    const all = tdoc.hitTotal.toLocaleString(Qt.locale(), "f", 0)
                    return (n > 0 ? n.toLocaleString(Qt.locale(), "f", 0) + " of " + all : all + (tdoc.hitTotal === 1 ? " match" : " matches")) + (tdoc.hitTotal > tdoc.hitCount ? " (first " + tdoc.hitCount.toLocaleString(Qt.locale(), "f", 0) + " shown)" : "")
                }
            }
            GButton {
                anchors.verticalCenter: parent.verticalCenter
                visible: search.text.trim() !== ""
                enabled: tdoc.hitCount > 0
                iconName: "go-up"; iconSize: sv.slim ? 14 : 16
                tip: "Previous match (Shift+Enter in the search field)"
                onClicked: sv.goHit(-1)
            }
            GButton {
                anchors.verticalCenter: parent.verticalCenter
                visible: search.text.trim() !== ""
                enabled: tdoc.hitCount > 0
                iconName: "go-down"; iconSize: sv.slim ? 14 : 16
                tip: "Next match (Enter in the search field)"
                onClicked: sv.goHit(1)
            }
            Rectangle { visible: search.text.trim() !== ""; width: 1; height: sv.slim ? 14 : 18; color: Theme.separator; anchors.verticalCenter: parent.verticalCenter }
            GButton {
                anchors.verticalCenter: parent.verticalCenter
                iconName: "expand-all|view-list-tree"; iconSize: sv.slim ? 14 : 16
                enabled: list.count > 0
                tip: "Expand all: open every level that fits (Ctrl+Shift+Right)"
                onClicked: tdoc.expandAll()
            }
            GButton {
                anchors.verticalCenter: parent.verticalCenter
                iconName: "collapse-all|view-list-details"; iconSize: sv.slim ? 14 : 16
                enabled: list.count > 0
                tip: "Collapse all: close everything down to the top level (Ctrl+Shift+Left)"
                onClicked: tdoc.collapseAll()
            }
        }
    }

    PanWheel { anchors.fill: list; flickable: list }

    ListView {
        id: list
        anchors.top: bar.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        clip: true
        interactive: false
        model: tdoc
        currentIndex: -1
        keyNavigationEnabled: false
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.HorizontalAndVerticalFlick
        // As wide as the widest row laid out so far (long values scroll sideways).
        property real widest: 0
        contentWidth: Math.max(width, widest)
        cacheBuffer: 400
        header: Item { height: 4; width: 1 }
        footer: Item { height: 12; width: 1 }
        highlightMoveDuration: 0
        ScrollBar.vertical: GScrollBar {}
        ScrollBar.horizontal: GScrollBar {}
        Connections { target: tdoc; function onModelReset() { list.widest = 0 } }

        Keys.onPressed: (e) => {
            const r = list.currentIndex
            const ctrl = e.modifiers & Qt.ControlModifier, shift = e.modifiers & Qt.ShiftModifier, alt = e.modifiers & Qt.AltModifier
            const page = Math.max(1, Math.floor(list.height / sv.rowH) - 1)
            if (e.key === Qt.Key_F && ctrl) sv.focusSearch()
            else if (e.key === Qt.Key_C && ctrl) sv.copy(shift ? "path" : "value")
            else if (e.key === Qt.Key_Right && ctrl && shift) tdoc.expandAll()
            else if (e.key === Qt.Key_Left && ctrl && shift) { tdoc.collapseAll(); sv.go(0) }
            else if (e.key === Qt.Key_Down) sv.go(r + 1)
            else if (e.key === Qt.Key_Up) sv.go(r < 0 ? 0 : r - 1)
            else if (e.key === Qt.Key_PageDown) sv.go(r + page)
            else if (e.key === Qt.Key_PageUp) sv.go(r - page)
            else if (e.key === Qt.Key_Home) sv.go(0)
            else if (e.key === Qt.Key_End) sv.go(list.count - 1)
            else if (e.key === Qt.Key_Right && r >= 0) {
                if (sv.rowData(r, "expandable") && (!sv.rowData(r, "expanded") || alt)) tdoc.setOpen(r, true, alt)
                else if (sv.rowData(r, "expanded")) sv.go(r + 1)
            }
            else if (e.key === Qt.Key_Left && r >= 0) {
                if (sv.rowData(r, "expanded")) tdoc.setOpen(r, false, alt)
                else { const p = tdoc.parentRow(r); if (p >= 0) { sv.go(p); if (alt) tdoc.setOpen(p, false, true) } }
            }
            else if (e.key === Qt.Key_Asterisk && r >= 0) tdoc.setOpen(r, true, true)
            else if ((e.key === Qt.Key_Return || e.key === Qt.Key_Enter) && r >= 0 && sv.rowData(r, "expandable")) sv.toggle(r, alt)
            else { e.accepted = false; return }
            e.accepted = true
        }

        delegate: Item {
            id: row
            required property int index
            required property int depth
            required property string key
            required property string keyKind
            required property string kind
            required property string vkind
            required property string value
            required property string summary
            required property bool expandable
            required property bool expanded
            required property bool hit
            readonly property bool current: index === list.currentIndex
            width: list.contentWidth
            height: sv.rowH
            Rectangle {
                anchors.fill: parent
                color: row.current ? (list.activeFocus ? Theme.accent : Theme.selectedInactive)
                     : row.hit ? Qt.rgba(1, 0.8, 0, Theme.dark ? 0.22 : 0.28)
                     : mouse.containsMouse ? Theme.hover : "transparent"
            }
            readonly property color ink: row.current && list.activeFocus ? Theme.accentText : Theme.fg
            Item {
                id: tri
                x: 8 + row.depth * sv.indent
                width: Math.round(14 * sv.zoom)
                height: parent.height
                Text {
                    visible: row.expandable
                    anchors.centerIn: parent
                    text: "▶"
                    rotation: row.expanded ? 90 : 0
                    Behavior on rotation { enabled: !Theme.reduceMotion; NumberAnimation { duration: Theme.fast } }
                    color: row.current && list.activeFocus ? Theme.accentText : Theme.fgSecondary
                    font.pixelSize: Math.round(Theme.small * 0.78 * sv.zoom)
                    renderType: Text.NativeRendering
                }
                MouseArea {
                    anchors.fill: parent
                    enabled: row.expandable
                    cursorShape: Qt.PointingHandCursor
                    onClicked: (m) => { list.currentIndex = row.index; list.forceActiveFocus(); sv.toggle(row.index, !!(m.modifiers & Qt.AltModifier)) }
                }
            }
            Row {
                id: content
                x: tri.x + tri.width + 4
                anchors.verticalCenter: parent.verticalCenter
                spacing: 0
                Text {
                    id: keyText
                    text: row.keyKind === "attr" ? "@" + row.key : row.key
                    color: row.current && list.activeFocus ? Theme.accentText : sv.keyColor(row.keyKind)
                    font.family: Theme.mono
                    font.pixelSize: Theme.body * 0.92 * sv.zoom
                    font.italic: row.keyKind === "special"
                    renderType: Text.NativeRendering
                }
                Text {
                    text: row.value !== "" || row.summary !== "" ? (row.keyKind === "element" ? "  " : ": ") : ""
                    color: row.current && list.activeFocus ? Theme.accentText : Theme.fgTertiary
                    font: keyText.font
                    renderType: Text.NativeRendering
                }
                Text {
                    id: valText
                    text: row.vkind === "container" ? (row.expanded ? "" : row.summary) : row.value
                    color: row.current && list.activeFocus ? Theme.accentText : sv.valueColor(row.vkind)
                    font.family: Theme.mono
                    font.pixelSize: Theme.body * 0.92 * sv.zoom
                    font.italic: row.vkind === "null" || row.vkind === "comment"
                    renderType: Text.NativeRendering
                }
            }
            Text {
                id: typeText
                anchors.verticalCenter: parent.verticalCenter
                x: Math.max(content.x + content.width + 16, list.width + list.contentX - width - 12)
                text: row.kind + (row.vkind === "container" && row.expanded ? " · " + row.summary.replace(/[^0-9]/g, "") : "")
                color: row.current && list.activeFocus ? Theme.accentText : Theme.fgTertiary
                font.family: Theme.family
                font.pixelSize: Theme.small * sv.zoom
                renderType: Text.NativeRendering
            }
            Component.onCompleted: { const w = content.x + content.width + typeText.width + 40; if (w > list.widest) list.widest = w }
            MouseArea {
                id: mouse
                anchors.fill: parent
                anchors.leftMargin: tri.x + tri.width
                hoverEnabled: true
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                onPressed: (m) => { list.currentIndex = row.index; list.forceActiveFocus() }
                onClicked: (m) => { if (m.button === Qt.RightButton) { rowMenu.row = row.index; rowMenu.popup() } }
                onDoubleClicked: (m) => { if (m.button === Qt.LeftButton && row.expandable) sv.toggle(row.index, !!(m.modifiers & Qt.AltModifier)) }
            }
            GTip {
                visible: mouse.containsMouse && !rowMenu.visible
                text: {
                    if (!visible) return ""
                    const full = row.vkind === "container" ? "" : tdoc.fullValue(row.index)
                    return tdoc.pathOf(row.index) + "  (" + row.kind + ")" + (full !== "" && (full.length > 60 || full.indexOf("\n") >= 0) ? "\n\n" + full : "") + "\n\nRight-click to copy its value or path"
                }
            }
        }
    }

    GMenu {
        id: rowMenu
        property int row: -1
        readonly property bool container: row >= 0 && row < list.count && !!sv.rowData(row, "expandable")
        GMenuItem { text: "Copy Value"; iconName: "edit-copy"; shortcutText: "Ctrl+C"; tip: rowMenu.container ? (tdoc.format === "json" || tdoc.format === "xml" ? "Copy this part of the file, as it's written" : "Copy everything inside, as JSON") : "Copy the value as text"; onTriggered: tdoc.copy(rowMenu.row, "value") }
        GMenuItem { text: "Copy Path"; iconName: "edit-copy-path|edit-copy"; shortcutText: "Ctrl+Shift+C"; tip: tdoc.format === "xml" ? "Copy where it is, as an XPath (/config/server[2]/@host)" : "Copy where it is (servers[2].host)"; onTriggered: tdoc.copy(rowMenu.row, "path") }
        GMenuItem { text: "Copy Key"; tip: "Copy its name or index"; onTriggered: tdoc.copy(rowMenu.row, "key") }
        GMenuSeparator {}
        GMenuItem { enabled: rowMenu.container; text: "Expand Everything Inside"; shortcutText: "Alt+Right"; tip: "Open this and every level inside it (as many as fit)"; onTriggered: tdoc.setOpen(rowMenu.row, true, true) }
        GMenuItem { enabled: rowMenu.container; text: "Collapse Everything Inside"; shortcutText: "Alt+Left"; tip: "Close this and everything inside it"; onTriggered: tdoc.setOpen(rowMenu.row, false, true) }
        GMenuSeparator {}
        GMenuItem { text: "Expand All"; shortcutText: "Ctrl+Shift+Right"; tip: "Open every level of the file that fits"; onTriggered: tdoc.expandAll() }
        GMenuItem { text: "Collapse All"; shortcutText: "Ctrl+Shift+Left"; tip: "Close everything down to the top level"; onTriggered: tdoc.collapseAll() }
    }

    // "Reading…" only once it's been reading a while (quick files would flicker it).
    property bool readingLong: false
    Timer { interval: 500; running: tdoc.loading; onRunningChanged: { if (running) sv.readingLong = false } onTriggered: sv.readingLong = true }
    GLabel {
        anchors.centerIn: list
        visible: (tdoc.loading && sv.readingLong) || (!tdoc.loading && tdoc.error === "" && list.count === 0 && tdoc.path !== "")
        text: tdoc.loading ? "Reading…" : "Empty"
        color: Theme.fgSecondary
    }
    GLabel {
        anchors.bottom: parent.bottom
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottomMargin: 8
        visible: tdoc.note !== ""
        text: tdoc.note
        width: Math.min(implicitWidth, parent.width - 24)
        elide: Text.ElideRight
        font.pixelSize: Theme.small
        color: Theme.fgSecondary
        Rectangle { z: -1; anchors.fill: parent; anchors.margins: -5; radius: 6; color: Theme.glassMenu; border.color: Theme.glassEdge }
    }
}
