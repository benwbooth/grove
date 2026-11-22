import QtQuick
import QtQuick.Controls.Basic
import Grove

// Column view (Finder's Miller columns): one column per folder from home
// (or /) down to the current folder, then a preview of the selected file.
// The last column is the pane's own model, so every action works on it.
Item {
    id: root
    property var pane
    property var fm: pane.model
    // Default column width: Alt-dragging a divider sets it for every column
    // (and remembers it); a plain drag resizes just that column, as in Finder.
    property real colW: pane.app && pane.app.settings.column_width > 0 ? pane.app.settings.column_width : Math.round(230 * Theme.pt)
    property var chain: []
    property var current: null      // selected item in the last column

    function computeChain() {
        const p = pane.path
        // Inside an archive: the folders down to the archive, the archive
        // itself ("archive://…#"), then its folders, each a column.
        if (p && p.indexOf("archive://") === 0) {
            const hash = p.indexOf("#")
            const file = hash >= 0 ? p.substring(10, hash) : p.substring(10)
            const inner = hash >= 0 ? p.substring(hash + 1) : ""
            const out = folderChain(file.substring(0, file.lastIndexOf("/")) || "/")
            out.push("archive://" + file + "#")
            let acc = ""
            for (const r of inner.split("/").filter(x => x)) { acc += (acc ? "/" : "") + r; out.push("archive://" + file + "#" + acc) }
            chain = out
            autoGlide()
            syncCols()
            return
        }
        if (!p || p.indexOf("/") !== 0) { chain = p ? [p] : []; syncCols(); return }
        chain = folderChain(p)
        autoGlide()
        syncCols()
    }
    function folderChain(p) {
        const home = pane.grove.home()
        const base = (p === home || p.indexOf(home + "/") === 0) ? home : "/"
        const out = [base]
        if (p !== base) {
            const rest = p.substring(base === "/" ? 1 : base.length + 1).split("/")
            let acc = base === "/" ? "" : base
            for (const r of rest) { acc += "/" + r; out.push(acc) }
        }
        return out
    }
    // The row path a column selects for the column after it (an archive's
    // own listing is selected by the archive file).
    function rowPathOf(colPath) {
        if (colPath.indexOf("archive://") === 0 && colPath.endsWith("#")) return colPath.substring(10, colPath.length - 1)
        return colPath
    }
    // The columns as a list model, changed in place: going deeper appends a
    // column, going up removes the ones on the right, and the columns you
    // already have stay as they are (an integer model rebuilt every column).
    ListModel { id: cols }

    // Sideways scrolling through the columns, with momentum like vertical.
    Kinetic { id: stripKinetic; target: strip; horizontal: true; notch: 120 }
    // The direction is chosen once per gesture (a pause of 200 ms starts a
    // new one) from whichever axis moves more, and only that axis is passed
    // on, so a slightly diagonal swipe doesn't jump between scrolling the
    // columns and scrolling a list. Shift+wheel scrolls sideways.
    property bool wheelHoriz: false
    property real wheelLast: 0
    property int routeLogLeft: 6
    function routeWheel(e, colKinetic) {
        if (e.modifiers & Qt.ControlModifier) return
        const shift = (e.modifiers & Qt.ShiftModifier) !== 0
        const now = Date.now()
        const dx = Math.abs(e.angleDelta.x) + Math.abs(e.pixelDelta.x)
        const dy = Math.abs(e.angleDelta.y) + Math.abs(e.pixelDelta.y)
        // Trackpad gestures start (and pause) with events that move nothing;
        // those mustn't pick the direction, or a sideways swipe locks to
        // up/down. They still go to the current direction's handler.
        // A new trackpad gesture (fingers down) picks the direction afresh.
        if (e.phase === Qt.ScrollBegin) wheelLast = 0
        if (dx + dy > 0 && e.phase !== Qt.ScrollMomentum) {
            if (now - wheelLast > 200) wheelHoriz = shift || dx > dy
            wheelLast = now
        }
        if (routeLogLeft > 0) {
            routeLogLeft--
            console.warn("grove column wheel: angle", e.angleDelta.x, e.angleDelta.y, "pixel", e.pixelDelta.x, e.pixelDelta.y,
                         "->", wheelHoriz ? "sideways" : "up/down", "strip", strip.contentX.toFixed(0), "/", (strip.contentWidth - strip.width).toFixed(0))
        }
        const ev = { device: e.device, isEndEvent: e.isEndEvent, phase: e.phase, modifiers: e.modifiers,
                     angleDelta: wheelHoriz ? Qt.point(shift ? e.angleDelta.y : e.angleDelta.x, 0) : Qt.point(0, e.angleDelta.y),
                     pixelDelta: wheelHoriz ? Qt.point(shift ? e.pixelDelta.y : e.pixelDelta.x, 0) : Qt.point(0, e.pixelDelta.y) }
        if (wheelHoriz) { autoGlideUntil = 0; glide.stop(); stripKinetic.wheel(ev) }
        else if (colKinetic) colKinetic.wheel(ev)
    }
    // Over the preview column (no list to scroll): sideways only.
    WheelHandler { acceptedDevices: PointerDevice.AllDevices; acceptedModifiers: Qt.KeyboardModifierMask; onWheel: (e) => root.routeWheel(e, null) }
    // WheelHandler only takes its own orientation (vertical by default), so a
    // purely sideways swipe needs a horizontal one; diagonal events already
    // reach the vertical handler, so this one only takes pure sideways ones.
    WheelHandler { acceptedDevices: PointerDevice.AllDevices; acceptedModifiers: Qt.KeyboardModifierMask; orientation: Qt.Horizontal
                   onWheel: (e) => { if (e.angleDelta.y === 0 && e.pixelDelta.y === 0) root.routeWheel(e, null) } }
    readonly property real stripX: strip.contentX
    function syncCols() {
        // The preview column ("") stays put at the end; folder columns are
        // removed and inserted in front of it, so nothing is recreated that
        // doesn't have to be (no blank frame anywhere).
        if (cols.count === 0) cols.append({ folder: "" })
        const n = cols.count - 1
        let keep = 0
        while (keep < n && keep < chain.length && cols.get(keep).folder === chain[keep]) keep++
        if (n > keep) cols.remove(keep, n - keep)
        for (let j = keep; j < chain.length; j++) cols.insert(j, { folder: chain[j] })
    }
    Component.onCompleted: computeChain()
    Connections {
        target: root.pane
        function onPathChanged() { root.computeChain() }
    }
    Connections {
        target: root.fm
        function onCurrentRowChanged() { root.updateCurrent() }
        function onSelectionCountChanged() { root.updateCurrent() }
    }
    // Until when layout changes may still slide the strip to the current
    // column (set on navigation, cleared when you scroll it yourself).
    property real autoGlideUntil: 0
    function autoGlide() { autoGlideUntil = Date.now() + 700 }
    function updateCurrent() {
        autoGlide()
        const r = fm.currentRow
        current = r >= 0 && fm.selectionCount === 1 ? pane.itemAt(r) : null
        Qt.callLater(() => strip.glideToEnd())
    }

    function positionAt(row) {
        const col = strip.itemAtIndex(chain.length - 1)
        if (col && col.list) col.list.positionViewAtIndex(row, ListView.Contain)
    }
    function handleKey(e) {
        const shift = (e.modifiers & Qt.ShiftModifier) !== 0
        if (e.key === Qt.Key_Down) { pane.step(1, shift); return true }
        if (e.key === Qt.Key_Up) { pane.step(-1, shift); return true }
        if (e.key === Qt.Key_Right) {
            const d = pane.browsePath(current)
            if (d) pane.navigate(d)
            return true
        }
        if (e.key === Qt.Key_Left) {
            if (chain.length > 1) pane.goUp()
            return true
        }
        return false
    }

    ListView {
        id: strip
        anchors.fill: parent
        orientation: ListView.Horizontal
        model: cols
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        interactive: false
        ScrollBar.horizontal: GScrollBar {}
        // Opening a folder: its column fades in while its listing arrives,
        // the preview column slides over to make room, and columns you leave
        // fade out. (Reduce motion: instant.)
        readonly property int animMs: Theme.reduceMotion ? 0 : 1
        add: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 180 * strip.animMs; easing.type: Easing.OutCubic } }
        remove: Transition { NumberAnimation { property: "opacity"; to: 0; duration: 120 * strip.animMs } }
        displaced: Transition { NumberAnimation { property: "x"; duration: 220 * strip.animMs; easing.type: Easing.OutCubic } }
        // Glide to show the newest column instead of jumping there.
        NumberAnimation { id: glide; target: strip; property: "contentX"; duration: 260 * strip.animMs; easing.type: Easing.OutCubic }
        function glideToEnd() {
            // Show the current folder's column at the right edge; the preview
            // column only when a file is selected (an empty preview at the end
            // made a narrow pane, e.g. a new split, look blank).
            let end = Math.max(originX, originX + contentWidth - width)
            const last = itemAtIndex(root.chain.length - 1)
            if (last && !root.current)
                end = Math.max(originX, Math.min(end, last.x + last.width - width))
            if (Math.abs(contentX - end) < 1) return
            stripKinetic.stop(); glide.stop(); glide.from = contentX; glide.to = end; glide.start()
        }
        onCountChanged: { root.autoGlide(); Qt.callLater(glideToEnd) }
        // Column widths settle after layout: aim again so the current column
        // ends at the edge, but only just after opening something. (The
        // width also changes while you scroll, as columns are made; aiming
        // then pulled a swipe back: "stutter and reverse".)
        onContentWidthChanged: if (Date.now() < root.autoGlideUntil) Qt.callLater(glideToEnd)
        // Keep every column made, so the width is exact while scrolling.
        cacheBuffer: 100000

        delegate: Item {
            id: column
            required property int index
            required property string folder
            // Wheel on the column (not inside the list, whose own scrolling is
            // off): smooth scrolling with momentum, as in the other views.
            Kinetic { id: colKinetic; target: lv; notch: 66 }
            WheelHandler { acceptedDevices: PointerDevice.AllDevices; acceptedModifiers: Qt.KeyboardModifierMask; onWheel: (e) => root.routeWheel(e, colKinetic) }
            WheelHandler { acceptedDevices: PointerDevice.AllDevices; acceptedModifiers: Qt.KeyboardModifierMask; orientation: Qt.Horizontal
                           onWheel: (e) => { if (e.angleDelta.y === 0 && e.pixelDelta.y === 0) root.routeWheel(e, colKinetic) } }
            readonly property bool isPreview: folder === ""
            readonly property bool isLast: index === root.chain.length - 1
            readonly property string selectedChild: index + 1 < root.chain.length ? root.chain[index + 1] : ""
            property alias list: lv
            // This column's own width once dragged; otherwise the default.
            property real ownWidth: 0
            width: isPreview ? Math.max(root.colW * 1.3, strip.width - root.chain.length * root.colW) : (ownWidth > 0 ? ownWidth : root.colW)
            height: strip.height

            FolderModel {
                id: own
                onLoaded: if (column.selectedChild) selectPaths(JSON.stringify([root.rowPathOf(column.selectedChild)]))
            }
            readonly property var colModel: isLast ? root.fm : own
            // A column that isn't the last lists its folder itself (the last one
            // shows the pane's model); one that stops being last loads then.
            property bool ownLoaded: false
            // Loaded for every folder column, the last one too (in the
            // background), so it has its contents ready the moment it stops
            // being the last column.
            function loadOwn() {
                if (isPreview || ownLoaded) return
                ownLoaded = true
                own.applyShowHidden(root.fm.showHidden)
                own.applyFoldersFirst(root.fm.foldersFirst)
                own.applySort("name", false)
                own.loadPath(folder)
            }
            Component.onCompleted: loadOwn()
            onSelectedChildChanged: if (!isPreview && selectedChild) own.selectPaths(JSON.stringify([root.rowPathOf(selectedChild)]))

            MouseArea {
                anchors.fill: parent
                visible: !column.isPreview
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                onPressed: (m) => {
                    root.pane.activated(); root.pane.forceActiveFocus()
                    if (!column.isLast) { root.pane.navigate(column.folder); return }
                    if (m.button === Qt.RightButton) root.pane.showBackgroundMenu()
                    else root.fm.clearSelection()
                }
            }
            ListView {
                id: lv
                visible: !column.isPreview
                anchors.fill: parent
                anchors.rightMargin: 1
                model: column.isPreview ? null : column.colModel
                clip: true
                reuseItems: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: GScrollBar {}
                // Scrolling is the handler's (with momentum), not the list's own.
                interactive: false
                delegate: Item {
                    id: r
                    required property int index
                    required property string name
                    required property string path
                    required property bool isDir
                    required property bool isArchive
                    required property bool selected
                    required property string iconName
                    required property string tags
                    required property string tagColors
                    required property bool hidden
                    required property bool cut
                    required property string sizeText
                    width: lv.width
                    readonly property bool browsable: r.isDir || (r.isArchive && root.pane.archiveLike(r))
                    height: Theme.rowHeight
                    readonly property bool renaming: column.isLast && root.pane.renamingRow === index
                    Rectangle {
                        anchors.fill: parent
                        anchors.leftMargin: 5; anchors.rightMargin: 5
                        radius: 5
                        color: r.selected ? (column.isLast && root.pane.active ? Theme.accent : Theme.selectedInactive) : drop.hovering ? Theme.dropTarget : "transparent"
                    }
                    GIcon { id: ic; x: 12; anchors.verticalCenter: parent.verticalCenter; size: 18; name: r.iconName; opacity: r.hidden || r.cut ? 0.5 : 1 }
                    GLabel {
                        id: colName
                        visible: !r.renaming
                        anchors.left: ic.right; anchors.leftMargin: 6
                        anchors.right: sizeLbl.left; anchors.rightMargin: 4
                        anchors.verticalCenter: parent.verticalCenter
                        text: r.name
                        elide: Text.ElideMiddle
                        color: r.selected && column.isLast && root.pane.active ? Theme.accentText : Theme.fg
                        opacity: r.hidden || r.cut ? 0.6 : 1
                    }
                    Loader {
                        active: r.renaming
                        anchors.left: ic.right; anchors.leftMargin: 4
                        anchors.right: parent.right; anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        sourceComponent: RenameField { pane: root.pane; row: r.index; original: r.name; isDir: r.isDir }
                    }
                    // Archives open like folders, so their size shows here
                    // (Alt+click selects one without opening it: full info).
                    GLabel {
                        id: sizeLbl
                        anchors.right: chevron.left; anchors.rightMargin: 4
                        anchors.verticalCenter: parent.verticalCenter
                        text: r.isArchive && !r.isDir ? r.sizeText : ""
                        font.pixelSize: Theme.small
                        color: r.selected && column.isLast && root.pane.active ? Theme.accentText : Theme.fgSecondary
                    }
                    TagDots { anchors.right: sizeLbl.left; anchors.rightMargin: 4; anchors.verticalCenter: parent.verticalCenter; tags: r.tags; colors: r.tagColors }
                    GLabel {
                        id: chevron
                        anchors.right: parent.right; anchors.rightMargin: 12
                        anchors.verticalCenter: parent.verticalCenter
                        text: r.browsable ? "›" : ""
                        font.pixelSize: Theme.title
                        color: r.selected && column.isLast && root.pane.active ? Theme.accentText : Theme.fgSecondary
                    }
                    MouseArea {
                        anchors.fill: parent
                        visible: !column.isLast
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        onClicked: (mouse) => {
                            root.pane.activated()
                            root.pane.forceActiveFocus()
                            const d = (mouse.modifiers & Qt.AltModifier) && !r.isDir ? "" : root.pane.browsePath(r)
                            if (d) root.pane.navigate(d)
                            else root.pane.navigate(column.folder, { select: [r.path] })
                        }
                    }
                    ItemMouse {
                        anchors.fill: parent
                        visible: column.isLast
                        enabled: column.isLast && !r.renaming
                        pane: root.pane
                        row: r.index
                        iconUrl: Theme.iconUrl(r.iconName)
                        onLeftPressed: (mods) => { if (root.pane.browsePath(r) && !(mods & (Qt.ShiftModifier | Qt.ControlModifier)) && !((mods & Qt.AltModifier) && !r.isDir)) openTimer.restart() }
                        Timer { id: openTimer; interval: 1; onTriggered: if (root.fm.selectionCount === 1) root.pane.navigate(root.pane.browsePath(r)) }
                    }
                    FolderDrop { id: drop; anchors.fill: parent; pane: root.pane; path: r.path; enabledFor: r.isDir }
                    // A name cut short shows in full on hover.
                    HoverHandler { acceptedDevices: PointerDevice.AllDevices; id: rowHover }
                    // Archives: how to see one's own info instead of its contents.
                    readonly property bool archiveRow: r.isArchive && !r.isDir && root.pane.archiveLike(r)
                    GTip { visible: rowHover.hovered && (colName.truncated || r.archiveRow) && !r.renaming
                           text: r.archiveRow ? r.name + " — " + r.sizeText + "\nClick to open it like a folder. Alt+click to select it and see its info." : r.name }
                }
            }
            Rectangle { visible: !column.isPreview; anchors.right: parent.right; width: 1; height: parent.height; color: Theme.separator }
            // Divider handle: drag to resize this column, Alt+drag for all.
            MouseArea {
                visible: !column.isPreview
                anchors.right: parent.right; anchors.rightMargin: -4
                width: 9; height: parent.height
                z: 10
                cursorShape: Qt.SplitHCursor
                hoverEnabled: true
                property real startX: 0
                property real startW: 0
                property bool all: false
                onPressed: (m) => {
                    startX = mapToItem(root, m.x, 0).x
                    startW = column.width
                    all = (m.modifiers & Qt.AltModifier) !== 0
                }
                onPositionChanged: (m) => {
                    if (!pressed) return
                    const w = Math.max(120, Math.min(800, startW + mapToItem(root, m.x, 0).x - startX))
                    if (all) { root.colW = w; for (let i = 0; i < strip.count; i++) { const c = strip.itemAtIndex(i); if (c) c.ownWidth = 0 } }
                    else column.ownWidth = w
                }
                onReleased: if (all && root.pane.app) root.pane.win.setSetting("column_width", Math.round(root.colW))
                onDoubleClicked: column.ownWidth = 0
                GTip { visible: parent.containsMouse && !parent.pressed; text: "Drag to resize this column; Alt+drag resizes all columns; double-click restores" }
            }

            // The preview column.
            Loader {
                anchors.fill: parent
                active: column.isPreview && root.current !== null && !root.current.isDir
                sourceComponent: PreviewPanel { pane: root.pane; item: root.current }
            }
        }
    }
}
