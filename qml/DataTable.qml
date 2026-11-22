import QtQuick
import QtQuick.Controls.Basic
import Grove

// CSV, TSV and spreadsheets in Quick Look: every row, with sheet tabs,
// search, sorting (click a header), column filters, columns to drag into
// another order, resize or hide, and SQL over the sheets. TableData reads,
// sorts, filters and queries on worker threads; the list shows the rows
// on screen.
Item {
    id: dt
    property string path: ""
    property bool active: false
    property var app

    // View state (the model keeps the data): column order, hidden
    // columns, widths in px.
    property var cols: []
    property var order: []
    property var hidden: ({})
    property var widths: ({})
    property var filters: []        // [{col, op, value}]
    property bool showFilters: false
    property bool showQuery: false
    readonly property var shown: order.filter(c => !hidden[c])
    readonly property real gutter: Math.max(44, String(tbl.total).length * fm.averageCharacterWidth + 20)
    readonly property real totalWidth: gutter + shown.reduce((a, c) => a + colWidth(c), 0)
    readonly property var sheetNames: { try { return JSON.parse(tbl.sheets) } catch (e) { return [] } }

    function colWidth(c) { return widths[c] !== undefined ? widths[c] : Math.min(420, Math.max(64, ((cols[c] || {}).width || 8) * fm.averageCharacterWidth + 24)) }
    function reload() { tbl.load(active ? path : ""); filters = []; showQuery = false; search.text = ""; query.text = "" }
    onPathChanged: Qt.callLater(reload)
    onActiveChanged: Qt.callLater(reload)
    function focusSearch() { search.forceActiveFocus(); search.selectAll() }
    function sendFilters() { tbl.setFilters(JSON.stringify(filters.filter(f => f.op === "empty" || f.op === "notempty" || f.value !== ""))) }
    function moveColumn(from, toIndex) {
        const o = order.slice()
        const i = o.indexOf(from)
        if (i < 0) return
        o.splice(i, 1)
        o.splice(Math.max(0, Math.min(o.length, toIndex > i ? toIndex - 1 : toIndex)), 0, from)
        order = o
    }
    function sortClick(c) {
        // Ascending, descending, then file order.
        if (tbl.sortColumn !== c) tbl.sortBy(c, false)
        else if (!tbl.descending) tbl.sortBy(c, true)
        else tbl.sortBy(-1, false)
    }

    TableData {
        id: tbl
        onColumnsChanged: {
            let c = []
            try { c = JSON.parse(columns) } catch (e) {}
            const same = c.length === dt.cols.length && c.every((x, i) => x.name === dt.cols[i].name)
            dt.cols = c
            if (!same) {
                dt.order = c.map((x, i) => i)
                dt.hidden = {}
                dt.widths = {}
                dt.filters = []
            }
        }
    }

    FontMetrics { id: fm; font.family: Theme.family; font.pixelSize: Theme.body }
    FontMetrics { id: fmMono; font.family: Theme.mono; font.pixelSize: Theme.body }

    // Tools: sheets, search, filters, query, columns, count.
    Column {
        id: tools
        width: parent.width
        spacing: 6
        topPadding: 8
        bottomPadding: 6

        Row {
            x: 12
            width: parent.width - 24
            spacing: 6
            // Sheet tabs (spreadsheets with more than one sheet).
            Repeater {
                model: dt.sheetNames.length > 1 ? dt.sheetNames : []
                delegate: GButton {
                    required property var modelData
                    required property int index
                    anchors.verticalCenter: parent.verticalCenter
                    text: modelData
                    capsule: true
                    flat: !(index === tbl.sheet && !tbl.querying)
                    tip: "Show the sheet “" + modelData + "” (in SQL it's table \"" + modelData + "\")"
                    onClicked: tbl.showSheet(index)
                }
            }
            GField {
                id: search
                width: Math.min(260, Math.max(140, parent.width * 0.28))
                iconName: "edit-find"
                clearable: true
                placeholderText: "Search rows"
                tip: "Show only the rows with this text in any cell (Ctrl+F)"
                onTextChanged: searchDelay.restart()
                Timer { id: searchDelay; interval: 150; onTriggered: tbl.setSearch(search.text) }
                Keys.onEscapePressed: (e) => { if (search.text !== "") { search.clear(); e.accepted = true } else e.accepted = false }
            }
            GButton {
                anchors.verticalCenter: parent.verticalCenter
                text: dt.filters.length ? "Filters (" + dt.filters.length + ")" : "Filter"
                iconName: "view-filter"
                capsule: true
                flat: !dt.showFilters
                tip: "Filter by column: contains, =, >, < and more (numbers compare as numbers)"
                onClicked: { dt.showFilters = !dt.showFilters; if (dt.showFilters && dt.filters.length === 0) dt.filters = [{ col: dt.shown[0] || 0, op: "contains", value: "" }] }
            }
            GButton {
                anchors.verticalCenter: parent.verticalCenter
                text: "SQL"
                capsule: true
                flat: !dt.showQuery
                tip: "Query with SQL (SQLite): the sheet is table t, every sheet is also a table under its own name"
                onClicked: { dt.showQuery = !dt.showQuery; if (dt.showQuery) query.forceActiveFocus() }
            }
            GButton {
                id: colsButton
                anchors.verticalCenter: parent.verticalCenter
                text: "Columns"
                capsule: true
                tip: "Show or hide columns (drag a column's header to move it, its edge to resize it)"
                onClicked: colMenu.popup(colsButton, 0, colsButton.height + 4)
            }
            GButton {
                anchors.verticalCenter: parent.verticalCenter
                iconName: "edit-copy"
                tip: "Copy the rows shown (tab-separated, with the header) for pasting into a spreadsheet"
                onClicked: if (dt.app) dt.app.grove.copyText(tbl.rowsTsv(0, Math.max(0, tbl.count - 1), JSON.stringify(dt.shown)))
            }
            GLabel {
                anchors.verticalCenter: parent.verticalCenter
                color: Theme.fgSecondary
                text: tbl.loading ? "Reading…"
                    : (tbl.count === tbl.total ? tbl.total.toLocaleString(Qt.locale(), "f", 0) + " rows"
                       : tbl.count.toLocaleString(Qt.locale(), "f", 0) + " of " + tbl.total.toLocaleString(Qt.locale(), "f", 0) + " rows")
                      + (tbl.querying ? " (query result)" : "") + (tbl.busy ? " …" : "")
            }
        }

        // Column filters.
        Column {
            visible: dt.showFilters
            x: 12
            spacing: 4
            Repeater {
                model: dt.showFilters ? dt.filters : []
                delegate: Row {
                    id: fr
                    required property var modelData
                    required property int index
                    spacing: 6
                    readonly property var ops: [["contains", "contains"], ["!contains", "doesn't contain"], ["=", "="], ["!=", "≠"], [">", ">"], ["<", "<"], [">=", "≥"], ["<=", "≤"], ["empty", "is empty"], ["notempty", "isn't empty"]]
                    function set(k, v) { const f = dt.filters.slice(); f[index] = Object.assign({}, f[index]); f[index][k] = v; dt.filters = f; dt.sendFilters() }
                    GCombo {
                        width: 180
                        model: dt.cols.map(c => c.name)
                        currentIndex: fr.modelData.col
                        tip: "The column to filter"
                        onActivated: (i) => fr.set("col", i)
                    }
                    GCombo {
                        width: 150
                        model: fr.ops.map(o => o[1])
                        currentIndex: Math.max(0, fr.ops.findIndex(o => o[0] === fr.modelData.op))
                        tip: "How to compare (numbers compare as numbers, text naturally)"
                        onActivated: (i) => fr.set("op", fr.ops[i][0])
                    }
                    GField {
                        width: 200
                        visible: fr.modelData.op !== "empty" && fr.modelData.op !== "notempty"
                        text: fr.modelData.value
                        placeholderText: "Value"
                        tip: "The value to compare with"
                        onTextEdited: fr.set("value", text)
                    }
                    GButton {
                        anchors.verticalCenter: parent.verticalCenter
                        iconName: "list-remove"
                        tip: "Remove this filter"
                        onClicked: { const f = dt.filters.slice(); f.splice(fr.index, 1); dt.filters = f; dt.sendFilters(); if (!f.length) dt.showFilters = false }
                    }
                    GButton {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: fr.index === dt.filters.length - 1
                        iconName: "list-add"
                        tip: "Add a filter (rows must pass all of them)"
                        onClicked: dt.filters = dt.filters.concat([{ col: dt.shown[0] || 0, op: "contains", value: "" }])
                    }
                }
            }
        }

        // SQL.
        Row {
            visible: dt.showQuery
            x: 12
            spacing: 6
            GField {
                id: query
                width: dt.width - 24 - runQ.width - backQ.width - 12
                font.family: Theme.mono
                placeholderText: "SELECT * FROM t WHERE …  (Return runs it)"
                tip: "SQLite SQL. Table t is the sheet shown; every sheet is also a table under its own name. Quote odd column names with \"double quotes\""
                onAccepted: tbl.runQuery(text)
            }
            GButton { id: runQ; text: "Run"; capsule: true; flat: false; tip: "Run the query (Return)"; onClicked: tbl.runQuery(query.text) }
            GButton { id: backQ; text: "Show Sheet"; capsule: true; enabled: tbl.querying; tip: "Back to the sheet itself"; onClicked: tbl.runQuery("") }
        }
        GLabel {
            visible: tbl.queryError !== "" || tbl.error !== ""
            x: 12
            width: dt.width - 24
            wrapMode: Text.Wrap
            color: Theme.danger
            text: tbl.error !== "" ? tbl.error : "SQL: " + tbl.queryError
        }
    }

    GMenu {
        id: colMenu
        Instantiator {
            model: dt.order
            delegate: GMenuItem {
                required property var modelData
                text: (dt.cols[modelData] || {}).name || ""
                checkable: true
                checked: !dt.hidden[modelData]
                tip: "Show or hide this column"
                onTriggered: {
                    const h = Object.assign({}, dt.hidden)
                    if (h[modelData]) delete h[modelData]
                    else if (dt.shown.length > 1) h[modelData] = true
                    dt.hidden = h
                }
            }
            onObjectAdded: (i, o) => colMenu.insertItem(i, o)
            onObjectRemoved: (i, o) => colMenu.removeItem(o)
        }
        GMenuSeparator {}
        GMenuItem {
            text: "Show All Columns"
            onTriggered: dt.hidden = {}
        }
        GMenuItem {
            text: "Original Order and Widths"
            onTriggered: { dt.order = dt.cols.map((x, i) => i); dt.widths = {} }
        }
    }

    // Header (follows the list sideways).
    Rectangle {
        id: header
        anchors.top: tools.bottom
        width: parent.width
        height: Theme.rowHeight + 4
        color: Theme.dark ? Qt.rgba(1, 1, 1, 0.06) : Qt.rgba(0, 0, 0, 0.04)
        clip: true
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.separator }
        property int dragCol: -1
        property real dropX: -1
        property int dropIndex: -1
        Row {
            id: headRow
            x: -list.contentX
            height: parent.height
            Item { width: dt.gutter; height: parent.height }
            Repeater {
                model: dt.shown
                delegate: Item {
                    id: hc
                    required property var modelData
                    required property int index
                    readonly property var col: dt.cols[modelData] || {}
                    width: dt.colWidth(modelData)
                    height: parent.height
                    Rectangle { anchors.fill: parent; color: hm.containsMouse && !hm.pressed ? Theme.hover : "transparent"; opacity: header.dragCol === hc.modelData ? 0.5 : 1 }
                    GLabel {
                        x: 8
                        width: parent.width - 16 - (sortMark.visible ? sortMark.width + 4 : 0)
                        anchors.verticalCenter: parent.verticalCenter
                        text: hc.col.name || ""
                        font.bold: true
                        elide: Text.ElideRight
                        horizontalAlignment: hc.col.numeric ? Text.AlignRight : Text.AlignLeft
                    }
                    GLabel {
                        id: sortMark
                        visible: tbl.sortColumn === hc.modelData
                        anchors.right: parent.right; anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        text: tbl.descending ? "▼" : "▲"
                        font.pixelSize: Theme.caption
                        color: Theme.accent
                    }
                    Rectangle { anchors.right: parent.right; width: 1; height: parent.height - 8; anchors.verticalCenter: parent.verticalCenter; color: Theme.separator }
                    MouseArea {
                        id: hm
                        anchors.fill: parent
                        anchors.rightMargin: 5
                        hoverEnabled: true
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        property real pressX: 0
                        property bool moved: false
                        onPressed: (e) => { pressX = e.x; moved = false }
                        onPositionChanged: (e) => {
                            if (!pressed) return
                            if (!moved && Math.abs(e.x - pressX) > 8) { moved = true; header.dragCol = hc.modelData }
                            if (!moved) return
                            // Where it would land: before the column under the pointer's middle.
                            const hx = mapToItem(headRow, e.x, 0).x
                            let acc = dt.gutter, idx = 0
                            for (; idx < dt.shown.length; idx++) {
                                const w = dt.colWidth(dt.shown[idx])
                                if (hx < acc + w / 2) break
                                acc += w
                            }
                            header.dropIndex = idx
                            header.dropX = acc - list.contentX
                        }
                        onReleased: (e) => {
                            if (moved && header.dropIndex >= 0) {
                                // Index in `shown` → index in `order`.
                                const target = header.dropIndex < dt.shown.length ? dt.order.indexOf(dt.shown[header.dropIndex]) : dt.order.length
                                dt.moveColumn(hc.modelData, target)
                            } else if (!moved && e.button === Qt.LeftButton) dt.sortClick(hc.modelData)
                            else if (!moved && e.button === Qt.RightButton) { headMenu.col = hc.modelData; headMenu.popup() }
                            header.dragCol = -1; header.dropIndex = -1; header.dropX = -1
                        }
                        GTip { visible: hm.containsMouse && !hm.pressed; text: (hc.col.name || "") + " — click to sort, drag to move, right-click for more" }
                    }
                    // Resize handle.
                    MouseArea {
                        anchors.right: parent.right
                        width: 8; height: parent.height
                        cursorShape: Qt.SplitHCursor
                        property real startX: 0
                        property real startW: 0
                        onPressed: (e) => { startX = mapToItem(dt, e.x, 0).x; startW = hc.width }
                        onPositionChanged: (e) => {
                            if (!pressed) return
                            const w = Object.assign({}, dt.widths)
                            w[hc.modelData] = Math.max(36, startW + mapToItem(dt, e.x, 0).x - startX)
                            dt.widths = w
                        }
                        onDoubleClicked: { const w = Object.assign({}, dt.widths); delete w[hc.modelData]; dt.widths = w }
                    }
                }
            }
        }
        Rectangle { visible: header.dropX >= 0; x: header.dropX - 1; width: 3; height: parent.height; color: Theme.accent }
    }

    GMenu {
        id: headMenu
        property int col: -1
        GMenuItem { text: "Sort Ascending"; onTriggered: tbl.sortBy(headMenu.col, false) }
        GMenuItem { text: "Sort Descending"; onTriggered: tbl.sortBy(headMenu.col, true) }
        GMenuItem { text: "File Order"; enabled: tbl.sortColumn >= 0; onTriggered: tbl.sortBy(-1, false) }
        GMenuSeparator {}
        GMenuItem {
            text: "Filter This Column…"
            onTriggered: { dt.filters = dt.filters.concat([{ col: headMenu.col, op: "contains", value: "" }]); dt.showFilters = true }
        }
        GMenuItem {
            text: "Hide Column"
            enabled: dt.shown.length > 1
            onTriggered: { const h = Object.assign({}, dt.hidden); h[headMenu.col] = true; dt.hidden = h }
        }
        GMenuItem {
            text: "Fit Width"
            onTriggered: { const w = Object.assign({}, dt.widths); delete w[headMenu.col]; dt.widths = w }
        }
    }

    PanWheel { anchors.top: header.bottom; anchors.bottom: parent.bottom; width: parent.width; flickable: list; notch: Theme.rowHeight * 3 }

    ListView {
        id: list
        anchors.top: header.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        clip: true
        interactive: false
        model: tbl
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.HorizontalAndVerticalFlick
        contentWidth: Math.max(width, dt.totalWidth)
        reuseItems: true
        ScrollBar.vertical: GScrollBar {}
        ScrollBar.horizontal: GScrollBar {}
        delegate: Rectangle {
            id: rowItem
            required property string cells
            required property int row
            required property int index
            readonly property var parts: cells.split("\u001f")
            width: list.contentWidth
            height: Theme.rowHeight
            color: index % 2 ? Theme.rowAlt : "transparent"
            Row {
                height: parent.height
                GLabel {
                    width: dt.gutter
                    height: parent.height
                    rightPadding: 10
                    horizontalAlignment: Text.AlignRight
                    verticalAlignment: Text.AlignVCenter
                    text: tbl.querying ? rowItem.index + 1 : rowItem.row
                    color: Theme.fgTertiary
                    font.pixelSize: Theme.small
                }
                Repeater {
                    model: dt.shown
                    delegate: Item {
                        id: cellItem
                        required property var modelData
                        readonly property string t: rowItem.parts[modelData] || ""
                        readonly property bool hit: search.text !== "" && t.toLowerCase().indexOf(search.text.toLowerCase()) >= 0
                        width: dt.colWidth(modelData)
                        height: rowItem.height
                        Rectangle {
                            visible: cellItem.hit
                            anchors.fill: parent; anchors.margins: 2
                            radius: 4
                            color: Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, Theme.dark ? 0.35 : 0.2)
                        }
                        GLabel {
                            id: cellText
                            x: 8
                            width: parent.width - 16
                            anchors.verticalCenter: parent.verticalCenter
                            text: cellItem.t
                            elide: Text.ElideRight
                            horizontalAlignment: (dt.cols[cellItem.modelData] || {}).numeric ? Text.AlignRight : Text.AlignLeft
                        }
                        // The whole cell when it doesn't fit (or has more lines).
                        MouseArea {
                            id: cellMouse
                            anchors.fill: parent
                            hoverEnabled: cellText.truncated || cellItem.t.length >= 400
                            acceptedButtons: Qt.NoButton
                        }
                        Loader {
                            active: cellMouse.containsMouse
                            sourceComponent: GTip { visible: true; text: tbl.cell(rowItem.index, cellItem.modelData).substring(0, 2000) }
                        }
                    }
                }
            }
        }
    }

    GLabel {
        anchors.centerIn: list
        visible: !tbl.loading && tbl.count === 0 && tbl.error === ""
        text: tbl.total === 0 ? "This table is empty" : "No rows match"
        color: Theme.fgSecondary
    }
}
