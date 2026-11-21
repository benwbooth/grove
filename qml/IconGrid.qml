import QtQuick
import QtQuick.Controls.Basic

// Icon view: a virtualized grid (only visible items exist, so 100 000
// files scroll smoothly), thumbnails from the freedesktop cache, rubber-band
// selection, zoom with Ctrl+wheel or the slider. Finder's View Options
// (Ctrl+J) set the grid spacing, text size, label position and background.
// With groups or stacks the grid is laid out in visual rows instead: group
// headings, and Finder's stacks that open in place.
Item {
    id: root
    property var pane
    property var fm: pane.model
    readonly property int icon: pane.iconSize
    readonly property bool labelRight: pane.labelPos === "right"
    readonly property int labelLines: labelRight ? 2 : 2
    readonly property real textPx: pane.textSize > 0 ? Math.round(pane.textSize * Theme.pt * Theme.fontScale * 10) / 10 : Theme.body
    readonly property bool itemInfo: pane.itemInfo
    // Grid spacing 1–10 (Finder's slider); 4 is the default.
    readonly property int spacingExtra: ((pane.gridSpacing > 0 ? pane.gridSpacing : 4) - 4) * Math.round(6 * Theme.pt)
    readonly property real lineH: Math.ceil(textPx * 1.3)
    readonly property real cellW: labelRight
        ? Math.max(icon + 120, icon + 14 + 12 + Math.round(textPx * 12)) + spacingExtra
        : Math.max(icon + 24, Math.max(icon + 36, Math.round(icon * 1.45) + 20) + spacingExtra)
    readonly property real cellH: labelRight
        ? Math.max(icon + 18, lineH * labelLines + (itemInfo ? lineH : 0) + 10) + Math.max(-6, spacingExtra / 3)
        : icon + 14 + lineH * labelLines + 12 + (itemInfo ? lineH : 0) + Math.max(-6, spacingExtra / 2)
    readonly property int cols: Math.max(1, Math.floor((root.width - 16) / cellW))
    readonly property real leftMargin: Math.max(8, Math.floor((root.width - root.cols * root.cellW) / 2))
    readonly property bool thumbs: pane.previewsOn
    readonly property bool stacks: pane.stacks !== ""
    readonly property bool grouped: pane.groupBy !== "" || stacks
    readonly property real headerH: Math.round(30 * Theme.pt)
    readonly property var flick: grouped ? glist : grid
    // Text that stays readable on a custom background colour.
    readonly property bool colorBg: pane.background.indexOf("color:") === 0
    property color bgColor: colorBg ? pane.background.substring(6) : "transparent"
    readonly property bool bgDark: colorBg ? (0.299 * bgColor.r + 0.587 * bgColor.g + 0.114 * bgColor.b) < 0.55 : Theme.dark
    readonly property color textColor: colorBg ? (bgDark ? "#f5f5f7" : "#1d1d1f") : Theme.fg
    readonly property color textSecondary: colorBg ? (bgDark ? Qt.rgba(1, 1, 1, 0.6) : Qt.rgba(0, 0, 0, 0.55)) : Theme.fgSecondary

    // ---------- grouped / stacked layout ----------
    property int rev: 0                    // bumps when item data changes
    property var vrows: []                 // visual rows
    property var expanded: ({})            // stack label -> true
    property var vrOf: []                  // model row -> visual row
    property var flat: []                  // model rows in visual order (stacks: first item)
    function bump() { rev++ }
    function relayout() {
        if (!grouped) { vrows = []; return }
        const ranges = JSON.parse(fm.groupRanges())
        const out = []
        const c = cols
        const pushItems = (start, count, inStack) => {
            for (let s = start; s < start + count; s += c) out.push({ t: "i", start: s, n: Math.min(c, start + count - s), stack: inStack || "" })
        }
        if (!stacks) {
            for (const r of ranges) {
                out.push({ t: "h", label: r.label || "Other", count: r.count })
                pushItems(r.start, r.count, "")
            }
        } else {
            const cells = []
            for (const r of ranges) {
                if (r.label === "" || r.count < 2) {
                    for (let i = r.start; i < r.start + r.count; i++) cells.push({ k: "item", row: i })
                } else cells.push({ k: "stack", label: r.label, start: r.start, count: r.count })
            }
            for (let i = 0; i < cells.length; i += c) {
                const rowCells = cells.slice(i, i + c)
                out.push({ t: "c", cells: rowCells })
                for (const x of rowCells)
                    if (x.k === "stack" && expanded[x.label]) {
                        out.push({ t: "s", label: x.label, count: x.count })
                        pushItems(x.start, x.count, x.label)
                    }
            }
        }
        // Where each model row is, for keys and scrolling.
        const vo = new Array(fm.count).fill(-1)
        const fl = []
        for (let v = 0; v < out.length; v++) {
            const r = out[v]
            if (r.t === "i") for (let k = 0; k < r.n; k++) { vo[r.start + k] = v; fl.push(r.start + k) }
            else if (r.t === "c") for (const x of r.cells) {
                if (x.k === "item") { vo[x.row] = v; fl.push(x.row) }
                else { for (let k = 0; k < x.count; k++) if (vo[x.start + k] < 0) vo[x.start + k] = v; fl.push(x.start) }
            }
        }
        vrOf = vo
        flat = fl
        vrows = out
    }
    function rowsOf(v) {
        const r = vrows[v]
        if (!r) return []
        if (r.t === "i") { const a = []; for (let k = 0; k < r.n; k++) a.push(r.start + k); return a }
        if (r.t === "c") return r.cells.map(x => x.k === "item" ? x.row : x.start)
        return []
    }
    function toggleStack(label) {
        const e = Object.assign({}, expanded)
        if (e[label]) delete e[label]; else e[label] = true
        expanded = e
        relayout()
    }
    onGroupedChanged: Qt.callLater(relayout)
    onStacksChanged: { expanded = {}; Qt.callLater(relayout) }
    onColsChanged: Qt.callLater(relayout)
    Connections {
        target: root.fm
        enabled: root.grouped
        function onDataChanged() { Qt.callLater(root.bump) }
        function onRowsInserted() { Qt.callLater(root.relayout) }
        function onRowsRemoved() { Qt.callLater(root.relayout) }
        function onModelReset() { Qt.callLater(root.relayout) }
        function onLayoutChanged() { Qt.callLater(root.relayout); Qt.callLater(root.bump) }
    }
    Connections {
        target: root.pane
        function onGroupByChanged() { Qt.callLater(root.relayout) }
        function onPathChanged() { root.expanded = {} }
    }
    Component.onCompleted: relayout()

    function positionAt(row) {
        if (row < 0) return
        if (!grouped) { grid.positionViewAtIndex(row, GridView.Contain); return }
        const v = vrOf[row]
        if (v !== undefined && v >= 0) glist.positionViewAtIndex(v, ListView.Contain)
    }
    function handleKey(e) {
        const shift = (e.modifiers & Qt.ShiftModifier) !== 0
        if (grouped) return groupedKey(e, shift)
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
    function groupedKey(e, shift) {
        if (!flat.length) return false
        const cur = fm.currentRow
        const go = (r) => { fm.select(r, shift ? "range" : "set"); positionAt(r); pane.win.quickLookFollow(pane); return true }
        let fi = flat.indexOf(cur)
        if (e.key === Qt.Key_Right) return go(flat[fi < 0 ? 0 : Math.min(flat.length - 1, fi + 1)])
        if (e.key === Qt.Key_Left) return go(flat[fi < 0 ? flat.length - 1 : Math.max(0, fi - 1)])
        if (e.key === Qt.Key_Home) return go(flat[0])
        if (e.key === Qt.Key_End) return go(flat[flat.length - 1])
        const vertical = { [Qt.Key_Down]: 1, [Qt.Key_Up]: -1, [Qt.Key_PageDown]: 8, [Qt.Key_PageUp]: -8 }
        const d = vertical[e.key]
        if (!d || (e.modifiers & Qt.ControlModifier)) return false
        if (cur < 0 || vrOf[cur] < 0) return go(flat[d > 0 ? 0 : flat.length - 1])
        const here = rowsOf(vrOf[cur])
        const col = Math.max(0, here.indexOf(cur) >= 0 ? here.indexOf(cur) : here.findIndex(r => r <= cur))
        let v = vrOf[cur], steps = Math.abs(d), last = -1
        while (steps > 0) {
            v += d > 0 ? 1 : -1
            if (v < 0 || v >= vrows.length) break
            if (rowsOf(v).length) { last = v; steps-- }
        }
        if (last < 0) return true
        const there = rowsOf(last)
        return go(there[Math.min(col, there.length - 1)])
    }

    // ---------- background (View Options) ----------
    Rectangle {
        anchors.fill: parent
        visible: root.colorBg
        color: root.bgColor
    }
    Image {
        anchors.fill: parent
        visible: pane.background.indexOf("picture:") === 0
        source: visible ? "file://" + encodeURI(pane.background.substring(8)) : ""
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        sourceSize.width: Math.ceil(root.width * Screen.devicePixelRatio)
        opacity: 0.999
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
            start = Qt.point(m.x, m.y + root.flick.contentY - root.flick.originY)
            additive = (m.modifiers & (Qt.ControlModifier | Qt.ShiftModifier)) !== 0 || pane.selectionMode
            if (!additive) fm.clearSelection()
            pane.renamingRow = -1
            active = true
        }
        onPositionChanged: (m) => {
            if (!active) return
            const f = root.flick
            const cur = Qt.point(m.x, m.y + f.contentY - f.originY)
            bandRect.x = Math.min(start.x, cur.x)
            bandRect.y = Math.min(start.y, cur.y) - f.contentY + f.originY
            bandRect.width = Math.abs(cur.x - start.x)
            bandRect.height = Math.abs(cur.y - start.y)
            bandRect.visible = bandRect.width > 3 || bandRect.height > 3
            const x1 = Math.min(start.x, cur.x) - root.leftMargin, x2 = Math.max(start.x, cur.x) - root.leftMargin
            const c1 = Math.max(0, Math.floor(x1 / cellW)), c2 = Math.min(cols - 1, Math.floor(x2 / cellW))
            const rows = []
            if (!root.grouped) {
                // Rows/columns the band touches.
                const y1 = Math.min(start.y, cur.y) - grid.anchors.topMargin, y2 = Math.max(start.y, cur.y) - grid.anchors.topMargin
                const r1 = Math.max(0, Math.floor(y1 / cellH)), r2 = Math.floor(y2 / cellH)
                for (let r = r1; r <= r2; r++)
                    for (let c = c1; c <= c2; c++) {
                        const i = r * cols + c
                        if (i < fm.count) rows.push(i)
                    }
            } else {
                // Visual rows that exist (visible or cached) and meet the band.
                const ya = Math.min(start.y, cur.y) + f.originY, yb = Math.max(start.y, cur.y) + f.originY
                for (let v = 0; v < root.vrows.length; v++) {
                    const it = glist.itemAtIndex(v)
                    if (!it || it.y + it.height < ya || it.y > yb) continue
                    const r = root.vrows[v]
                    if (r.t === "i") { for (let k = c1; k <= Math.min(c2, r.n - 1); k++) rows.push(r.start + k) }
                    else if (r.t === "c") for (let k = c1; k <= Math.min(c2, r.cells.length - 1); k++) {
                        const x = r.cells[k]
                        if (x.k === "item") rows.push(x.row)
                        else for (let j = 0; j < x.count; j++) rows.push(x.start + j)
                    }
                }
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
        onTriggered: { const f = root.flick; f.contentY = Math.max(f.originY, Math.min(f.contentY + dy, f.originY + Math.max(0, f.contentHeight - f.height))) }
    }

    // ---------- plain grid ----------
    GridView {
        id: grid
        visible: !root.grouped
        anchors.fill: parent
        anchors.leftMargin: root.leftMargin
        anchors.topMargin: 8
        model: root.grouped ? null : root.fm
        cellWidth: root.cellW
        cellHeight: root.cellH
        interactive: false
        clip: true
        reuseItems: true
        cacheBuffer: Math.round(root.cellH * 2)
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

        delegate: Item {
            id: gd
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
            required property string sizeText
            required property string dims
            width: grid.cellWidth
            height: grid.cellHeight
            IconCell {
                anchors.fill: parent
                grid: root
                index: gd.index; name: gd.name; path: gd.path; isDir: gd.isDir; isLink: gd.isLink
                selected: gd.selected; iconName: gd.iconName; thumb: gd.thumb; mtime: gd.mtime
                tags: gd.tags; tagColors: gd.tagColors; git: gd.git; cut: gd.cut; hidden: gd.hidden
                info: gd.isDir ? "" : (gd.dims !== "" ? gd.dims : gd.sizeText)
            }
        }
    }

    // ---------- groups and stacks ----------
    ListView {
        id: glist
        visible: root.grouped
        anchors.fill: parent
        anchors.topMargin: 4
        model: root.grouped ? root.vrows : []
        interactive: false
        clip: true
        cacheBuffer: Math.round(root.cellH * 3)
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
        delegate: Item {
            id: vr
            required property var modelData
            required property int index
            readonly property var r: modelData
            width: glist.width
            height: r.t === "h" ? root.headerH : r.t === "s" ? Math.round(root.headerH * 0.85) : root.cellH
            // Group heading (Finder: bold title, count, a hairline).
            Item {
                visible: vr.r.t === "h"
                anchors.fill: parent
                GLabel {
                    x: root.leftMargin + 6
                    anchors.bottom: parent.bottom; anchors.bottomMargin: 6
                    text: vr.r.label || ""
                    font.bold: true
                    font.pixelSize: Theme.body
                    color: root.textColor
                }
                GLabel {
                    anchors.right: parent.right; anchors.rightMargin: root.leftMargin + 6
                    anchors.bottom: parent.bottom; anchors.bottomMargin: 6
                    text: vr.r.count === 1 ? "1 item" : (vr.r.count || 0) + " items"
                    font.pixelSize: Theme.small
                    color: root.textSecondary
                }
                Rectangle { x: root.leftMargin; width: parent.width - 2 * root.leftMargin; height: 1; anchors.bottom: parent.bottom; color: Theme.separator }
            }
            // An open stack's band and heading.
            Rectangle {
                visible: vr.r.t === "s" || (vr.r.t === "i" && vr.r.stack !== "")
                x: 6; width: parent.width - 12; height: parent.height
                color: Theme.dark ? Qt.rgba(1, 1, 1, 0.04) : Qt.rgba(0, 0, 0, 0.035)
            }
            Item {
                visible: vr.r.t === "s"
                anchors.fill: parent
                GLabel {
                    x: root.leftMargin + 6
                    anchors.verticalCenter: parent.verticalCenter
                    text: (vr.r.label || "") + "  ·  " + (vr.r.count || 0) + " items"
                    font.pixelSize: Theme.small
                    font.bold: true
                    color: root.textSecondary
                }
                GButton {
                    tint: root.textColor
                    anchors.right: parent.right; anchors.rightMargin: root.leftMargin
                    anchors.verticalCenter: parent.verticalCenter
                    text: "Close Stack"
                    tip: "Fold this stack back into one icon (or click the stack)"
                    onClicked: root.toggleStack(vr.r.label)
                }
            }
            // Items: a run of rows, or a row of loose items and stacks.
            Repeater {
                model: vr.r.t === "i" ? vr.r.n : vr.r.t === "c" ? vr.r.cells.length : 0
                Item {
                    id: slot
                    required property int index
                    readonly property var c: vr.r.t === "c" ? vr.r.cells[index] : null
                    readonly property bool isStack: c !== null && c.k === "stack"
                    readonly property int row: vr.r.t === "i" ? vr.r.start + index : (c && c.k === "item" ? c.row : -1)
                    readonly property var d: { root.rev; return row >= 0 ? JSON.parse(root.fm.get(row)) : {} }
                    x: root.leftMargin + index * root.cellW
                    width: root.cellW
                    height: root.cellH
                    IconCell {
                        visible: !slot.isStack && slot.row >= 0
                        anchors.fill: parent
                        grid: root
                        index: slot.row
                        name: slot.d.name || ""; path: slot.d.path || ""; isDir: !!slot.d.isDir; isLink: !!slot.d.isLink
                        selected: !!slot.d.selected; iconName: slot.d.icon || ""; thumb: !!slot.d.thumb; mtime: slot.d.mtime || 0
                        tags: slot.d.tagText || ""; tagColors: slot.d.tagColors || ""; git: slot.d.git || ""; cut: !!slot.d.cut; hidden: !!slot.d.hidden
                        info: slot.d.isDir ? "" : (slot.d.dims || slot.d.sizeText || "")
                    }
                    Loader {
                        active: slot.isStack
                        anchors.fill: parent
                        sourceComponent: StackCell {
                            grid: root
                            stack: slot.c
                            rev: root.rev
                            open: !!root.expanded[slot.c.label]
                        }
                    }
                }
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
            const f = root.flick
            const d = e.pixelDelta.y !== 0 ? e.pixelDelta.y : e.angleDelta.y / 120 * root.cellH * 0.6
            f.contentY = Math.max(f.originY, Math.min(f.contentY - d, f.originY + Math.max(0, f.contentHeight - f.height)))
        }
    }
    WheelHandler {
        acceptedModifiers: Qt.ControlModifier
        onWheel: (e) => root.pane.zoom(e.angleDelta.y > 0 ? 1 : -1)
    }
}
