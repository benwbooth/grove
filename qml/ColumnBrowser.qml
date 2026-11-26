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
    // The selected file for the preview column, worked out once the
    // selection has settled (a click passes through "nothing selected",
    // which tore the preview down and built it again: a flash, and a
    // playing track stopped). The same file keeps the same object.
    function updateCurrent() {
        // Right-click (Finder): nothing moves while its menu is open, and the
        // preview catches up in place when it closes (see onItemMenuOpenChanged).
        if (pane.itemMenuOpen) return
        autoGlide()
        Qt.callLater(settleCurrent)
    }
    Connections {
        target: root.pane
        function onItemMenuOpenChanged() { if (!root.pane.itemMenuOpen) root.settleCurrent(true) }
    }
    function settleCurrent(stay) {
        const r = fm.currentRow
        const it = r >= 0 && fm.selectionCount === 1 ? pane.itemAt(r) : null
        if (it && current && it.path === current.path && it.mtime === current.mtime && it.size === current.size) return
        if (!it && !current) return
        current = it
        if (!stay) Qt.callLater(() => strip.glideToEnd())
    }

    // Tests (GROVE_TEST dumpcols): each column's folder, scroll and model.
    function testScroll(y) { const c = strip.itemAtIndex(chain.length - 1); if (c && c.list) c.list.contentY = y }
    function testDump() {
        for (let i = 0; i < cols.count; i++) {
            const c = strip.itemAtIndex(i)
            if (c && c.list) console.warn("grove test col", i, c.folder, "y", Math.round(c.list.contentY), "count", c.list.count, "fm", c.colModel === fm)
        }
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

            // Google Drive badges (canopy) for this column's folder; again
            // when canopy reports changes under its synced folder.
            property string cloudRoot: ""
            function cloudBadges() {
                const pane = root.pane, folder = column.folder
                if (!pane.app || pane.app.settings.show_cloud_status === false || folder.indexOf("/") !== 0) { own.setCloudStatus("{}"); cloudRoot = ""; return }
                Async.call(pane.grove, k => pane.grove.cloudStatus(k, folder), r => {
                    if (own.path !== folder) return
                    column.cloudRoot = (r && r.__root) || ""
                    if (column.cloudRoot) pane.noteCloudRoot(column.cloudRoot)
                    own.setCloudStatus(JSON.stringify(r || {}))
                })
            }
            Connections {
                target: root.pane.grove
                enabled: column.cloudRoot !== ""
                function onCloudChanged() { if (!column.isPreview) column.cloudBadges() }
            }
            FolderModel {
                id: own
                onLoaded: {
                    if (column.selectedChild) selectPaths(JSON.stringify([root.rowPathOf(column.selectedChild)]))
                    // Version control badges for this column's folder too.
                    const pane = root.pane, folder = column.folder
                    if (pane.app && pane.app.settings.show_git_status && folder.indexOf("/") === 0)
                        Async.call(pane.grove, k => pane.grove.gitStatus(k, folder), r => { if (own.path === folder) own.setGitStatus(JSON.stringify(r || {})) })
                    column.cloudBadges()
                }
            }
            // The last column shows the pane's model, but only once it has
            // this folder loaded: until then its own (already filled) copy,
            // so clicking an item in an earlier column doesn't empty the
            // column and draw everything again.
            readonly property bool fmReady: root.fm.path === folder && !root.fm.loading
            readonly property bool ownReady: ownLoaded && own.path === folder && !own.loading
            readonly property var colModel: isLast && (fmReady || !ownReady) ? root.fm : own
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
                own.applySort(root.fm.sortKey || "name", root.fm.descending)
                own.loadPath(folder)
            }
            Component.onCompleted: { loadOwn(); swapModel(); if (!isPreview) root.pane.usageFor(folder) }
            // Disk usage mode: this column's folder too.
            Connections {
                target: root.pane
                function onUsageOnChanged() { if (!column.isPreview) root.pane.usageFor(column.folder) }
            }
            // Sorted as the current folder is (the toolbar's Sort, View > Sort By).
            Connections {
                target: root.fm
                function onSortKeyChanged() { if (column.ownLoaded) own.applySort(root.fm.sortKey || "name", root.fm.descending) }
                function onDescendingChanged() { if (column.ownLoaded) own.applySort(root.fm.sortKey || "name", root.fm.descending) }
                // Hidden files / folders first switched since this column
                // loaded: its copy follows (it was shown stale, hiding
                // items, once you clicked into a folder in it).
                function onShowHiddenChanged() { if (column.ownLoaded) own.applyShowHidden(root.fm.showHidden) }
                function onFoldersFirstChanged() { if (column.ownLoaded) own.applyFoldersFirst(root.fm.foldersFirst) }
            }
            // The list changes model when the column stops (or starts) being
            // the last one: the same folder from another copy. A new model
            // scrolls a ListView back to the top (clicking an archive jumped
            // its column up), so keep where it was.
            onColModelChanged: swapModel()
            function swapModel() {
                const m = isPreview ? null : colModel
                if (lv.model === m) return
                const had = !!lv.model
                const y = lv.contentY
                lv.model = m
                if (!had) return
                const back = () => { lv.contentY = Math.max(lv.originY, Math.min(y, lv.originY + lv.contentHeight - lv.height)) }
                back()
                Qt.callLater(back)
            }
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
                // (Set by column.swapModel, keeping the scroll position.)
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
                    required property string git
                    required property string cloud
                    width: lv.width
                    readonly property bool browsable: r.isDir || (r.isArchive && root.pane.archiveLike(r))
                    // An archive is a file (a click selects it and previews
                    // it); its arrow, grey until it's open, or a double-click
                    // shows what's inside, as a folder.
                    readonly property bool archiveRow: r.isArchive && !r.isDir && root.pane.archiveLike(r)
                    readonly property bool arrowButton: r.archiveRow
                    readonly property bool opened: column.selectedChild !== "" && column.selectedChild === root.pane.browsePath(r)
                    function clickOpens() { return r.opened || r.isDir || r.path.indexOf("mount:") === 0 }
                    height: Theme.rowHeight
                    readonly property bool renaming: column.isLast && root.pane.renamingRow === index
                    Rectangle {
                        anchors.fill: parent
                        anchors.leftMargin: 5; anchors.rightMargin: 5
                        radius: 5
                        color: r.selected ? (column.isLast && root.pane.active ? Theme.accent : Theme.selectedInactive) : drop.hovering ? Theme.dropTarget : "transparent"
                    }
                    UsageBar {
                        pane: root.pane
                        path: r.path
                        x: 8; width: parent.width - 16
                        height: parent.height - 4
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    GIcon { id: ic; x: 12; anchors.verticalCenter: parent.verticalCenter; size: 18; name: r.iconName; opacity: r.hidden || r.cut ? 0.5 : 1 }
                    GitBadge { status: r.git; anchors.right: ic.right; anchors.bottom: ic.bottom; anchors.margins: -5; scale: 0.7 }
                    CloudBadge { status: r.cloud; anchors.left: ic.left; anchors.bottom: ic.bottom; anchors.margins: -5; scale: 0.7 }
                    GLabel {
                        id: colName
                        visible: !r.renaming
                        anchors.left: ic.right; anchors.leftMargin: 6
                        anchors.right: colBranch.visible ? colBranch.left : sizeLbl.left; anchors.rightMargin: 4
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
                    BranchTag {
                        id: colBranch
                        z: 5
                        visible: !r.renaming && branch !== ""
                        anchors.right: sizeLbl.left; anchors.rightMargin: 4
                        anchors.verticalCenter: parent.verticalCenter
                        maxWidth: 110
                        status: r.git; path: r.path; pane: root.pane
                        onAccent: r.selected && column.isLast && root.pane.active
                    }
                    // An archive's size, on its button with the arrow.
                    GLabel {
                        id: sizeLbl
                        anchors.right: chevron.left; anchors.rightMargin: 4
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.pane.usageOn ? root.pane.usageLabel(r.path) : r.archiveRow ? r.sizeText : ""
                        font.pixelSize: Theme.small
                        color: r.selected && column.isLast && root.pane.active ? Theme.accentText : Theme.fgSecondary
                    }
                    UsageSpinner {
                        id: usageSpin
                        running: root.pane.usageOn && root.pane.usagePending(r.path)
                        anchors.right: chevron.left; anchors.rightMargin: 6
                        anchors.verticalCenter: parent.verticalCenter
                        color: r.selected && column.isLast && root.pane.active ? Theme.accentText : Theme.fgSecondary
                    }
                    TagDots { anchors.right: sizeLbl.left; anchors.rightMargin: 4; anchors.verticalCenter: parent.verticalCenter; tags: r.tags; colors: r.tagColors }
                    // An unopened archive's arrow is a small button: a faint
                    // inset well that lights up under the pointer.
                    Rectangle {
                        visible: r.arrowButton && !r.opened
                        anchors.right: chevron.right; anchors.rightMargin: -6
                        anchors.verticalCenter: parent.verticalCenter
                        width: sizeLbl.implicitWidth + chevron.implicitWidth + 4 + 16
                        height: 20; radius: 5
                        color: arrowMouse.pressed ? Qt.rgba(Theme.fg.r, Theme.fg.g, Theme.fg.b, 0.22)
                             : arrowMouse.containsMouse ? Qt.rgba(Theme.fg.r, Theme.fg.g, Theme.fg.b, 0.14)
                             : Qt.rgba(0, 0, 0, Theme.dark ? 0.18 : 0.05)
                        border.width: 1
                        border.color: arrowMouse.containsMouse ? Qt.rgba(Theme.fg.r, Theme.fg.g, Theme.fg.b, 0.35) : Qt.rgba(Theme.fg.r, Theme.fg.g, Theme.fg.b, 0.12)
                        Behavior on color { ColorAnimation { duration: 90 } }
                    }
                    GLabel {
                        id: chevron
                        anchors.right: parent.right; anchors.rightMargin: 12
                        anchors.verticalCenter: parent.verticalCenter
                        text: r.browsable ? "›" : ""
                        font.pixelSize: Theme.title
                        color: r.selected && column.isLast && root.pane.active ? Theme.accentText : Theme.fgSecondary
                        opacity: r.arrowButton && !r.opened && !arrowMouse.containsMouse ? 0.55 : 1
                    }
                    MouseArea {
                        anchors.fill: parent
                        visible: !column.isLast
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        onClicked: (mouse) => {
                            root.pane.activated()
                            root.pane.forceActiveFocus()
                            // Right-click: this column becomes the current one with the
                            // item selected, then its menu opens (the menu's actions
                            // work on the current folder's selection).
                            if (mouse.button === Qt.RightButton) {
                                root.pane.pendingMenu = r.path
                                root.pane.navigate(column.folder, { select: [r.path] })
                                return
                            }
                            const d = r.clickOpens() ? root.pane.browsePath(r) : ""
                            if (d) root.pane.navigate(d)
                            else root.pane.navigate(column.folder, { select: [r.path] })
                        }
                        onDoubleClicked: { const d = root.pane.browsePath(r); if (d) root.pane.navigate(d) }
                    }
                    ItemMouse {
                        anchors.fill: parent
                        visible: column.isLast
                        // (Not while the column still shows its own copy: rows
                        // would be the pane model's.)
                        enabled: column.isLast && !r.renaming && column.colModel === root.fm
                        pane: root.pane
                        row: r.index
                        iconUrl: Theme.iconUrl(r.iconName)
                        onLeftPressed: (mods) => { if (r.clickOpens() && !(mods & (Qt.ShiftModifier | Qt.ControlModifier))) openTimer.restart() }
                        Timer { id: openTimer; interval: 1; onTriggered: if (root.fm.selectionCount === 1) root.pane.navigate(root.pane.browsePath(r)) }
                    }
                    FolderDrop { id: drop; anchors.fill: parent; pane: root.pane; path: r.path; enabledFor: r.isDir }
                    // An archive's arrow: show what's inside (as a folder).
                    MouseArea {
                        id: arrowMouse
                        visible: r.arrowButton
                        anchors.right: parent.right; anchors.rightMargin: 4
                        width: sizeLbl.implicitWidth + chevron.implicitWidth + 4 + 22; height: parent.height
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: { root.pane.activated(); root.pane.forceActiveFocus(); root.pane.navigate(root.pane.browsePath(r)) }
                        GTip { visible: parent.containsMouse; text: "Show what's inside “" + r.name + "” (or double-click it, or press →)" }
                    }
                    // A name cut short shows in full on hover.
                    HoverHandler { acceptedDevices: PointerDevice.AllDevices; id: rowHover }
                    GTip { visible: rowHover.hovered && (colName.truncated || root.pane.usageOn) && !r.renaming
                           text: root.pane.usageOn ? r.name + "\n" + root.pane.usageTip(r.path) : r.name }
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
            property alias previewLoader: previewLoader
            Loader {
                id: previewLoader
                anchors.fill: parent
                active: column.isPreview && root.current !== null && !root.current.isDir
                sourceComponent: PreviewPanel { pane: root.pane; item: root.current }
            }
        }
    }

    // The session: the strip's scroll and the current column's.
    function scrollState() {
        const c = strip.itemAtIndex(chain.length - 1)
        return { x: Math.round(strip.contentX), y: c && c.list ? Math.round(c.list.contentY) : 0 }
    }
    function restoreScroll(s) {
        autoGlideUntil = 0
        strip.contentX = Math.max(0, Math.min(s.x || 0, strip.contentWidth - strip.width))
        const c = strip.itemAtIndex(chain.length - 1)
        if (c && c.list) c.list.contentY = Math.max(0, Math.min(s.y || 0, c.list.contentHeight - c.list.height))
    }
    // The preview column's viewer (the file shown there).
    function previewViewer() {
        const c = strip.itemAtIndex(cols.count - 1)
        return c && c.previewLoader && c.previewLoader.item ? c.previewLoader.item.viewer : null
    }
}
