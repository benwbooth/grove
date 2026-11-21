import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window
import QtQuick.Layouts

// A grove window: the glass sidebar, the toolbar, tabs (each one or two
// panes), the status bar with the zoom slider, and the dialogs.
ApplicationWindow {
    id: bw
    property var app
    property var initialPaths: []
    property var initialSelect: []
    property var initialStates: []       // tab states (a torn-off tab, the session)
    readonly property bool clientDecorations: app.settings.client_decorations !== false
    readonly property bool chooserMode: false
    property bool sidebarShown: true
    property int sidebarWidth: app.settings.sidebar_width > 0 ? app.settings.sidebar_width : Math.round(200 * Theme.pt)
    property var currentTab: tabRepeater.count > 0 ? tabRepeater.itemAt(tabBar.currentIndex) : null
    property var activePane: currentTab ? currentTab.activePane : null
    property bool modalOpen: toolbarSheet.opened || goSheet.opened || shareSheet.opened || commitSheet.opened || versionsSheet.opened || conflict.opened || confirmSheet.opened || renameSheet.opened || connectSheet.opened || saveSheet.opened || openWithSheet.opened

    width: 1180
    height: 720
    minimumWidth: 560
    minimumHeight: 360
    visible: false
    color: Theme.window
    flags: clientDecorations ? (Qt.Window | Qt.FramelessWindowHint) : Qt.Window
    title: activePane ? activePane.title + " — Grove" : "Grove"

    onActiveChanged: if (active) app.activeWindow = bw

    ListModel { id: tabs }
    Connections {
        target: bw.app
        function onSettingsChanged() {
            const h = !!bw.app.settings.show_hidden, f = bw.app.settings.folders_first !== false
            bw.forEachPane(p => {
                if (p.model.showHidden !== h) p.model.applyShowHidden(h)
                if (p.model.foldersFirst !== f) p.model.applyFoldersFirst(f)
            })
        }
    }

    function newTab(path, select) {
        tabs.append({ path: path || app.grove.home(), select: JSON.stringify(select || []), tstate: "" })
        tabBar.currentIndex = tabs.count - 1
    }
    // A tab from somewhere else (another window), at `index` (-1: the end).
    function insertTabState(st, index) {
        const i = index < 0 || index > tabs.count ? tabs.count : index
        tabs.insert(i, { path: st.path, select: "[]", tstate: JSON.stringify(st) })
        tabBar.currentIndex = i
    }
    function tabStates() {
        const out = []
        for (let i = 0; i < tabRepeater.count; i++) { const t = tabRepeater.itemAt(i); if (t) out.push(t.tabState()) }
        return out
    }
    // Tab dragging: another grove window (or this one) dropped tab `index`
    // of `from` here at `at`.
    function acceptTab(from, index, at) {
        const t = from.tabAt(index)
        if (!t) return
        if (from === bw) {
            if (at === index || at === index + 1) return
            const to = at > index ? at - 1 : at
            tabs.move(index, to, 1)
            tabBar.currentIndex = to
            return
        }
        insertTabState(t.tabState(), at)
        from.removeTab(index)
        bw.raise(); bw.requestActivate()
    }
    // Dropped outside any tab bar: the tab becomes a window of its own.
    function tearOffTab(index) {
        const t = tabAt(index)
        if (!t) return
        if (tabs.count <= 1) return
        const st = t.tabState()
        const w = app.openWindowWithStates([st])
        w.x = bw.x + 60; w.y = bw.y + 60
        w.width = bw.width; w.height = bw.height
        removeTab(index)
    }
    // Remove a tab that moved to another window (not "closed": no reopen).
    function removeTab(i) {
        if (tabs.count <= 1) { bw.close(); return }
        tabs.remove(i)
        tabBar.currentIndex = Math.min(tabBar.currentIndex, tabs.count - 1)
    }
    function closeTab(i) {
        if (tabs.count <= 1) { bw.close(); return }
        const t = tabRepeater.itemAt(i)
        if (t) app.closedTabs.push(t.activePane.path)
        tabs.remove(i)
        tabBar.currentIndex = Math.min(tabBar.currentIndex, tabs.count - 1)
    }
    function tabAt(i) { return tabRepeater.itemAt(i) }
    function tabPaths() {
        const out = []
        for (let i = 0; i < tabRepeater.count; i++) { const t = tabRepeater.itemAt(i); if (t && t.activePane) out.push(t.activePane.path.indexOf("search:") === 0 ? t.activePane.folderBeforeSearch : t.activePane.path) }
        return out
    }
    onActivePaneChanged: if (app) app.saveSession()
    Connections { target: bw.activePane; function onPathChanged() { bw.app.saveSession() } }
    function go(path) { if (activePane) { activePane.navigate(path); activePane.forceActiveFocus() } }
    function setSetting(name, value) { app.grove.setSetting(name, JSON.stringify(value)) }
    function toggleHidden() {
        const v = !app.settings.show_hidden
        setSetting("show_hidden", v)
        forEachPane(p => p.model.applyShowHidden(v))
        toast("info", v ? "Showing hidden files (Ctrl+H to hide them)" : "Hidden files are hidden (Ctrl+H to show them)")
    }
    function forEachPane(f) {
        for (let i = 0; i < tabRepeater.count; i++) {
            const t = tabRepeater.itemAt(i)
            if (!t) continue
            f(t.activePane)
            const o = t.otherPane()
            if (o) f(o)
        }
    }
    function foldersTouched(dirs) {
        forEachPane(p => { if (p.special && !p.searching) p.model.reload() })
        app.refreshTrash()
    }
    function selectAfterJob(dest, results) {
        if (activePane && activePane.path === dest && results.length) {
            activePane.selectWhenThere(results[0], false)
            Qt.callLater(() => activePane.model.selectPaths(JSON.stringify(results)))
        }
    }

    // ---------- dialogs & helpers ----------
    function toast(level, message) { toastBox.show(level, message) }
    function confirm(title, message, action, fn, destructive) {
        confirmSheet.title = title
        confirmSheet.message = message
        confirmSheet.actionText = action
        confirmSheet.destructive = !!destructive
        confirmSheet.fn = fn
        confirmSheet.open()
    }
    function confirmDelete(paths) {
        const what = paths.length === 1 ? "“" + paths[0].substring(paths[0].lastIndexOf("/") + 1) + "”" : paths.length + " items"
        confirm("Delete " + what + " permanently?", "This can't be undone: the items won't go to the Trash.", "Delete", () =>
            Async.call(app.grove, k => app.grove.deletePaths(k, JSON.stringify(paths)), r => {
                if (r.errors && r.errors.length) toast("error", r.errors.join("\n"))
                app.refreshTrash()
                forEachPane(p => { if (p.inTrash) p.model.reload() })
            }), true)
    }
    function emptyTrash() {
        const go = () => Async.call(app.grove, k => app.grove.emptyTrash(k), r => {
            if (r.errors && r.errors.length) toast("error", r.errors.join("\n"))
            else toast("info", "The Trash is empty")
            app.refreshTrash()
            forEachPane(p => { if (p.inTrash) p.model.reload() })
        })
        if (app.settings.confirm_empty_trash !== false)
            confirm("Empty the Trash?", "All " + app.trashCount + " items in the Trash will be deleted permanently. This can't be undone.", "Empty Trash", go, true)
        else go()
    }
    function eject(path, name) {
        Async.call(app.grove, k => app.grove.eject(k, path), r => {
            if (r.error) toast("error", r.error)
            else toast("info", "“" + name + "” can be removed safely now")
            forEachPane(p => { if (p.path === path || p.path.indexOf(path + "/") === 0) p.navigate(app.grove.home()) })
        })
    }
    function askConflict(id, info) { conflict.ask(id, info) }
    function showInfo(paths) {
        for (const p of paths.slice(0, 10)) {
            const w = infoComp.createObject(null, { app: app, path: p })
            w.show()
        }
    }
    function quickLook(pane) {
        if (ql.visible) { ql.close(); return }
        const it = pane.currentItem()
        if (!it) return
        ql.pane = pane
        ql.showItem(it)
    }
    function quickLookFollow(pane) {
        if (ql.visible && ql.pane === pane) {
            const it = pane.currentItem()
            if (it) ql.showItem(it)
        }
    }
    function batchRename(paths) { renameSheet.start(paths) }
    function connectServer() { connectSheet.start() }
    function connectSaved(s) {
        toast("info", "Connecting to “" + s.name + "”…")
        Async.call(app.grove, k => app.grove.connectServer(k, JSON.stringify({ name: s.name, kind: s.kind, host: s.host, user: s.user, path: s.path, remember: true })), r => {
            if (r.error) { toast("error", r.error); connectSheet.start(s) }
            else go(r.path)
        })
    }
    function saveSmartFolder(pane) { saveSheet.pane = pane; saveSheet.open() }
    function openWithDialog(path) { openWithSheet.start(path) }
    function quickAction(action, paths, arg) {
        Async.call(app.grove, k => app.grove.quickAction(k, action, JSON.stringify(paths), arg), r => {
            if (r.errors && r.errors.length) toast("error", r.errors.join("\n"))
        })
    }
    function showVersions(path) { versionsSheet.start(path) }
    function share(paths) { shareSheet.start(paths) }
    function showText(heading, text, html, tool, action) {
        const w = textComp.createObject(null, { heading: heading, text: text || "", html: html || "", tool: tool || "", toolAction: action || null })
        w.show()
    }
    function gitAction(action, dir, paths, arg) {
        Async.call(app.grove, k => app.grove.gitAction(k, dir, action, JSON.stringify(paths || []), arg || ""), r => {
            if (r.error) { toast("error", r.error); return }
            if (action === "log") showText("Git history", r.output, "", "")
            else if (action === "diff") showText("Changes (git diff)", r.output, r.output ? r.html : "", "")
            else toast("info", "git " + action + ": " + (r.output || "done").split("\n").slice(-2).join(" "))
            forEachPane(p => { if (p.path.indexOf(dir) === 0) p.reload() })
        })
    }
    function compareFiles(a, b) {
        Async.call(app.grove, k => app.grove.compareFiles(k, a, b, Theme.dark), r => {
            if (r.error) toast("error", r.error)
            else if (r.same) toast("info", "The two items are identical.")
            else showText("Differences", r.text, r.html, r.tool || "", r.tool ? () => app.grove.runCommand(JSON.stringify([r.tool, a, b]), "") : null)
        })
    }
    function mountImage(path) {
        toast("info", "Opening the disk image…")
        Async.call(app.grove, k => app.grove.mountImage(k, path), r => { if (r.error) toast("error", r.error); else go(r.path) })
    }
    function toOtherPane(move) {
        const o = currentTab && currentTab.otherPane()
        if (!o) { toast("info", "Open split view (F3) first: this copies to the other side."); return }
        const sel = activePane.selectedPaths()
        if (sel.length) app.grove.runJob(move ? "move" : "copy", JSON.stringify(sel), o.path)
    }
    function setPanel(name, on) { setSetting(name, on) }
    function askCommit(dir, paths) { commitSheet.dir = dir; commitSheet.paths = paths; commitSheet.open() }

    function showSettings() { settingsWin.show(); settingsWin.raise(); settingsWin.requestActivate() }
    function showHelp(search) { helpWin.query = search || ""; helpWin.show(); helpWin.raise(); helpWin.requestActivate() }

    Component { id: infoComp; InfoWindow {} }
    Component { id: chooserComp; FileChooser {} }
    // grove's own open/save dialog for grove itself: fn(paths).
    function chooseFiles(title, opts, fn) {
        const req = Object.assign({ title: title, mode: "open" }, opts || {})
        const c = chooserComp.createObject(null, { app: app, request: req, onChosen: fn })
        c.show(); c.raise(); c.requestActivate()
        return c
    }
    Component { id: textComp; TextViewer {} }

    // ---------- layout ----------
    Item {
        id: root
        anchors.fill: parent
        // Text on exact half pixels smears at 4/3 scale.
        transform: Translate { x: 0.001; y: 0.001 }
        Rectangle { anchors.fill: parent; color: Theme.window }

        Sidebar {
            id: sidebar
            win: bw
            visible: bw.sidebarShown
            x: 8; y: 8
            width: bw.sidebarWidth
            height: parent.height - 16
            currentPath: bw.activePane ? bw.activePane.path : ""
        }
        FoldersPanel {
            id: foldersPanel
            win: bw
            visible: !!bw.app.settings.folders_panel
            x: bw.sidebarShown ? sidebar.x + sidebar.width + 8 : 8
            y: 8
            width: Math.round(220 * Theme.pt)
            height: parent.height - 16
        }
        MouseArea {
            visible: bw.sidebarShown
            x: sidebar.x + sidebar.width
            width: 8
            height: parent.height
            cursorShape: Qt.SplitHCursor
            property real sx: 0
            property real sw: 0
            onPressed: (m) => { sx = mapToItem(root, m.x, 0).x; sw = bw.sidebarWidth }
            onPositionChanged: (m) => { if (pressed) bw.sidebarWidth = Math.max(150, Math.min(400, sw + mapToItem(root, m.x, 0).x - sx)) }
            onReleased: bw.setSetting("sidebar_width", Math.round(bw.sidebarWidth))
        }

        Item {
            id: main
            x: (bw.sidebarShown ? sidebar.x + sidebar.width + 8 : 0) + (foldersPanel.visible ? foldersPanel.width + 8 : 0)
            width: parent.width - x
            height: parent.height

            // ---------- toolbar ----------
            Item {
                id: toolbar
                width: parent.width
                height: Theme.toolbarHeight
                MouseArea {
                    anchors.fill: parent
                    acceptedButtons: Qt.LeftButton | Qt.RightButton
                    onPressed: (m) => { if (m.button === Qt.RightButton) toolbarMenu.popup(); else bw.startSystemMove() }
                    onDoubleClicked: bw.visibility === Window.Maximized ? bw.showNormal() : bw.showMaximized()
                }
                GButton {
                    id: closeBtn
                    visible: !bw.sidebarShown && bw.clientDecorations
                    x: 12
                    anchors.verticalCenter: parent.verticalCenter
                    iconName: "window-close"; iconSize: 12
                    tip: "Close window"
                    onClicked: bw.close()
                }
                // The items from Customize Toolbar (settings "toolbar").
                RowLayout {
                    id: toolRow
                    anchors.left: closeBtn.visible ? closeBtn.right : parent.left
                    anchors.leftMargin: closeBtn.visible ? 8 : (bw.sidebarShown ? 4 : (bw.clientDecorations ? 12 : 8))
                    anchors.right: parent.right
                    anchors.rightMargin: 10
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8
                    Repeater {
                        model: bw.toolbarItems
                        Loader {
                            required property string modelData
                            readonly property bool flexible: modelData === "path" || modelData === "flexspace"
                            Layout.fillWidth: flexible
                            Layout.minimumWidth: modelData === "path" ? 120 : 0
                            Layout.preferredWidth: modelData === "space" ? 16 : flexible ? 10 : (item ? item.implicitWidth : 0)
                            Layout.preferredHeight: Theme.controlHeight
                            visible: modelData !== "split" || main.width > 860
                            sourceComponent: bw.toolComponent(modelData)
                        }
                    }
                    // Without a flexible item the rest sits on the left.
                    Item { visible: bw.toolbarItems.indexOf("path") < 0 && bw.toolbarItems.indexOf("flexspace") < 0; Layout.fillWidth: true }
                }
            }

            // ---------- tabs ----------
            TabStrip {
                id: tabBar
                win: bw
                model: tabs
                anchors.top: toolbar.bottom
                width: parent.width
                // Also shown while a tab is dragged, so a one-tab window takes it.
                height: tabs.count > 1 || bw.app.dragTab ? Math.round(30 * Theme.pt) : 0
                visible: height > 0
            }

            StackLayout {
                id: stack
                anchors.top: tabBar.bottom
                anchors.bottom: termPanel.visible ? termSplit.top : statusBar.top
                width: parent.width - (infoPanel.visible ? infoPanel.width : 0)
                currentIndex: tabBar.currentIndex
                Repeater {
                    id: tabRepeater
                    model: tabs
                    TabPage {
                        required property string path
                        required property string select
                        required property string tstate
                        required property int index
                        win: bw
                        initialPath: path
                        initialSelect: JSON.parse(select)
                        initialState: tstate ? JSON.parse(tstate) : null
                    }
                }
            }

            // Dolphin's Information panel / Finder's preview pane (F11, Ctrl+Shift+P).
            Item {
                id: infoPanel
                visible: !!bw.app.settings.info_panel
                anchors.top: tabBar.bottom
                anchors.bottom: statusBar.top
                anchors.right: parent.right
                width: Math.round(260 * Theme.pt)
                property var item: null
                Rectangle { width: 1; height: parent.height; color: Theme.separator }
                Connections {
                    target: bw.activePane ? bw.activePane.model : null
                    function onCurrentRowChanged() { infoPanel.refresh() }
                    function onSelectionCountChanged() { infoPanel.refresh() }
                }
                function refresh() {
                    const p = bw.activePane
                    if (!p) { item = null; return }
                    item = p.model.currentRow >= 0 && p.model.selectionCount > 0 ? p.itemAt(p.model.currentRow) : null
                }
                PreviewPanel {
                    anchors.fill: parent
                    anchors.bottomMargin: 50
                    pane: bw.activePane
                    item: infoPanel.item
                    visible: infoPanel.item !== null
                }
                GLabel {
                    anchors.centerIn: parent
                    visible: infoPanel.item === null
                    text: bw.activePane ? bw.activePane.model.status : ""
                    color: Theme.fgSecondary
                }
                // Rating (Dolphin's stars, user.baloo.rating).
                Row {
                    visible: infoPanel.item !== null && bw.activePane && !bw.activePane.special
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 16
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: 4
                    Repeater {
                        model: 5
                        GLabel {
                            required property int index
                            text: infoPanel.item && infoPanel.item.rating >= (index + 1) * 2 ? "★" : "☆"
                            color: Theme.accent
                            font.pixelSize: Theme.large
                            MouseArea {
                                id: starMouse
                                anchors.fill: parent
                                hoverEnabled: true
                                onClicked: bw.activePane.setRating(infoPanel.item.rating === (parent.index + 1) * 2 ? 0 : (parent.index + 1) * 2, () => infoPanel.refresh())
                            }
                            GTip { visible: starMouse.containsMouse; text: "Rate " + (parent.index + 1) + " star" + (parent.index ? "s" : "") + " (shared with Dolphin)" }
                        }
                    }
                }
            }

            // Terminal panel (F4).
            MouseArea {
                id: termSplit
                visible: termPanel.visible
                anchors.bottom: termPanel.top
                width: parent.width
                height: 6
                cursorShape: Qt.SplitVCursor
                property real sy: 0
                property real sh: 0
                onPressed: (m) => { sy = mapToItem(main, 0, m.y).y; sh = termPanel.height }
                onPositionChanged: (m) => { if (pressed) termPanel.height = Math.max(80, Math.min(main.height - 200, sh - (mapToItem(main, 0, m.y).y - sy))) }
            }
            TerminalPanel {
                id: termPanel
                win: bw
                visible: !!bw.app.settings.terminal_panel
                anchors.bottom: statusBar.top
                width: parent.width
                height: Math.round(main.height * 0.32)
            }

            // ---------- status bar ----------
            Item {
                id: statusBar
                anchors.bottom: parent.bottom
                width: parent.width
                height: app.settings.show_status_bar !== false ? Math.round(26 * Theme.pt) : 0
                visible: height > 0
                Rectangle { width: parent.width; height: 1; color: Theme.separator }
                GLabel {
                    x: 12
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - zoomRow.width - 40
                    text: bw.activePane ? bw.activePane.model.status : ""
                    font.pixelSize: Theme.small
                    color: Theme.fgSecondary
                }
                Row {
                    id: zoomRow
                    anchors.right: parent.right
                    anchors.rightMargin: 14
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 6
                    visible: bw.activePane && bw.activePane.viewMode === "icons"
                    GIcon { name: "zoom-out"; size: 12; anchors.verticalCenter: parent.verticalCenter; opacity: 0.6 }
                    Slider {
                        id: zoom
                        width: 110
                        height: 18
                        from: 0; to: 8; stepSize: 1
                        snapMode: Slider.SnapAlways
                        readonly property var sizes: [16, 24, 32, 48, 64, 96, 128, 192, 256]
                        value: bw.activePane ? Math.max(0, sizes.indexOf(bw.activePane.iconSize)) : 4
                        onMoved: { bw.activePane.iconSize = sizes[value]; bw.activePane.rememberView() }
                        hoverEnabled: true
                        background: Rectangle {
                            x: zoom.leftPadding; y: zoom.topPadding + zoom.availableHeight / 2 - 2
                            width: zoom.availableWidth; height: 4; radius: 2
                            color: Theme.separator
                        }
                        handle: Rectangle {
                            x: zoom.leftPadding + zoom.visualPosition * (zoom.availableWidth - width)
                            y: zoom.topPadding + zoom.availableHeight / 2 - height / 2
                            width: 14; height: 14; radius: 7
                            color: "white"
                            border.color: Qt.rgba(0, 0, 0, 0.25)
                        }
                        GTip { visible: zoom.hovered; text: "Icon size (Ctrl+= / Ctrl+-, or Ctrl+scroll)" }
                    }
                    GIcon { name: "zoom-in"; size: 16; anchors.verticalCenter: parent.verticalCenter; opacity: 0.6 }
                }
            }
        }

        // Window edge resizing for grove's own decorations.
        Repeater {
            model: bw.clientDecorations && bw.visibility !== Window.Maximized ? [
                { e: Qt.LeftEdge, x: 0, y: 6, w: 5, h: -12, c: Qt.SizeHorCursor },
                { e: Qt.RightEdge, x: -5, y: 6, w: 5, h: -12, c: Qt.SizeHorCursor },
                { e: Qt.TopEdge, x: 6, y: 0, w: -12, h: 4, c: Qt.SizeVerCursor },
                { e: Qt.BottomEdge, x: 6, y: -5, w: -12, h: 5, c: Qt.SizeVerCursor },
                { e: Qt.TopEdge | Qt.LeftEdge, x: 0, y: 0, w: 8, h: 8, c: Qt.SizeFDiagCursor },
                { e: Qt.BottomEdge | Qt.RightEdge, x: -8, y: -8, w: 8, h: 8, c: Qt.SizeFDiagCursor },
                { e: Qt.TopEdge | Qt.RightEdge, x: -8, y: 0, w: 8, h: 8, c: Qt.SizeBDiagCursor },
                { e: Qt.BottomEdge | Qt.LeftEdge, x: 0, y: -8, w: 8, h: 8, c: Qt.SizeBDiagCursor }
            ] : []
            MouseArea {
                required property var modelData
                x: modelData.x < 0 ? root.width + modelData.x : modelData.x
                y: modelData.y < 0 ? root.height + modelData.y : modelData.y
                width: modelData.w <= 0 ? root.width + modelData.w : modelData.w
                height: modelData.h <= 0 ? root.height + modelData.h : modelData.h
                cursorShape: modelData.c
                onPressed: bw.startSystemResize(modelData.e)
            }
        }

        Toast { id: toastBox; anchors.horizontalCenter: main.horizontalCenter; anchors.bottom: parent.bottom; anchors.bottomMargin: 40 + (bw.activePane && bw.activePane.selectionMode ? Theme.controlHeight + 14 : 0) }
    }


    // ---------- toolbar items ----------
    readonly property var toolbarDefaults: ["nav", "sidebar", "path", "view", "split", "selection", "action", "jobs", "search"]
    readonly property var toolbarItems: app.settings.toolbar && app.settings.toolbar.length ? app.settings.toolbar : toolbarDefaults
    readonly property var toolbarAll: ["nav", "sidebar", "path", "view", "split", "selection", "action", "jobs", "search", "newfolder", "trash", "info", "share", "quicklook", "terminal", "hidden", "viewopts", "connect", "eject", "burn", "space", "flexspace"]
    readonly property var toolbarCatalog: ({
        nav: { label: "Back/Forward", icon: "go-previous", tip: "Back, forward and the enclosing folder" },
        sidebar: { label: "Sidebar", icon: "sidebar-show-symbolic|view-split-left-right", tip: "Show or hide the sidebar" },
        path: { label: "Path", icon: "folder", tip: "Where you are; click to type a path (flexible width)" },
        view: { label: "View", icon: "view-grid-symbolic|view-list-icons", tip: "Icons, list, columns or gallery" },
        split: { label: "Split View", icon: "view-split-left-right", tip: "Two folders side by side" },
        selection: { label: "Selection Mode", icon: "checkbox-checked|edit-select-all", tip: "Checkboxes on items" },
        action: { label: "Action", icon: "view-more-horizontal-symbolic|application-menu", tip: "The action menu" },
        jobs: { label: "Progress", icon: "view-refresh", tip: "Copies and moves under way (shows while busy)" },
        search: { label: "Search", icon: "edit-find", tip: "The search field" },
        newfolder: { label: "New Folder", icon: "folder-new", tip: "Make a new folder here" },
        trash: { label: "Delete", icon: "user-trash", tip: "Move the selection to the Trash" },
        info: { label: "Get Info", icon: "document-properties|documentinfo", tip: "Facts about the selection" },
        share: { label: "Share", icon: "document-share|emblem-shared", tip: "Send to a nearby device" },
        quicklook: { label: "Quick Look", icon: "document-preview|view-preview", tip: "Preview the selection" },
        terminal: { label: "Terminal", icon: "utilities-terminal", tip: "Open a terminal here" },
        hidden: { label: "Hidden Files", icon: "view-hidden|visibility", tip: "Show or hide hidden files" },
        viewopts: { label: "View Options", icon: "configure|preferences-system", tip: "Spacing, text size, background" },
        connect: { label: "Connect", icon: "network-connect|network-server", tip: "Connect to a server" },
        eject: { label: "Eject", icon: "media-eject", tip: "Eject the disk you're in" },
        burn: { label: "Burn", icon: "media-optical-burn|media-optical", tip: "Burn the selection or this folder to a disc" },
        space: { label: "Space", icon: "", tip: "A fixed gap" },
        flexspace: { label: "Flexible Space", icon: "", tip: "A gap that grows to fill the room" }
    })
    // Parts of the toolbar other code reaches (null when not in the toolbar).
    property Item pathBar: null
    property Item search: null
    property Item actionButton: null
    function toolComponent(k) {
        return ({ nav: tcNav, sidebar: tcSidebar, path: tcPath, view: tcView, split: tcSplit, selection: tcSelection, action: tcAction,
                  jobs: tcJobs, search: tcSearch, newfolder: tcNewFolder, trash: tcTrash, info: tcInfo, share: tcShare, quicklook: tcQuickLook,
                  terminal: tcTerminal, hidden: tcHidden, viewopts: tcViewOpts, connect: tcConnect, eject: tcEject, burn: tcBurn,
                  space: tcSpace, flexspace: tcSpace })[k] || tcSpace
    }
    function ejectCurrent() {
        const p = activePane.path
        for (const l of (sidebar.places.locations || []).concat(sidebar.places.network || []))
            if (l.ejectable && (p === l.path || p.indexOf(l.path + "/") === 0)) { eject(l.path, l.name); return }
        toast("info", "The folder you're in isn't on a disk that can be ejected.")
    }
    Component { id: tcSpace; Item { implicitWidth: 16 } }
    Component {
        id: tcNav
        Item {
            implicitWidth: navGroup.implicitWidth + 4
            implicitHeight: Theme.controlHeight
            GlassPanel { anchors.fill: parent; radius: height / 2; shadow: false; tint: Theme.field }
            Row {
                id: navGroup
                anchors.centerIn: parent
                GButton { iconName: "go-previous"; capsule: true; enabled: bw.activePane && bw.activePane.backStack.length > 0; tip: Help.tip("Back"); onClicked: bw.activePane.goBack() }
                GButton { iconName: "go-next"; capsule: true; enabled: bw.activePane && bw.activePane.fwdStack.length > 0; tip: Help.tip("Forward"); onClicked: bw.activePane.goForward() }
                GButton { iconName: "go-up"; capsule: true; enabled: bw.activePane && bw.activePane.parentOf(bw.activePane.path) !== ""; tip: Help.tip("Enclosing folder"); onClicked: bw.activePane.goUp() }
            }
        }
    }
    Component { id: tcSidebar; GButton { iconName: "sidebar-show-symbolic|view-split-left-right|sidebar-expand-left"; tip: Help.tip("Show or hide the sidebar", "Sidebar"); onClicked: bw.sidebarShown = !bw.sidebarShown } }
    Component {
        id: tcPath
        PathBar {
            pane: bw.activePane
            Component.onCompleted: bw.pathBar = this
            Component.onDestruction: if (bw.pathBar === this) bw.pathBar = null
        }
    }
    Component {
        id: tcView
        Segmented {
            items: [
                { icon: "view-grid-symbolic|view-list-icons", tip: Help.tip("View as icons", "Icons"), value: "icons" },
                { icon: "view-list-symbolic|view-list-details", tip: Help.tip("View as a list (folders expand in place)", "List"), value: "list" },
                { icon: "view-column-symbolic|view-file-columns|view-split-left-right", tip: Help.tip("View in columns", "Columns"), value: "columns" },
                { icon: "view-paged-symbolic|view-preview|view-list-images", tip: Help.tip("View as a gallery", "Gallery"), value: "gallery" }
            ]
            current: bw.activePane ? bw.activePane.viewMode : ""
            onPicked: (v) => bw.activePane.setView(v)
        }
    }
    Component { id: tcSplit; GButton { iconName: "view-split-left-right"; checked: bw.currentTab && bw.currentTab.split; tip: Help.tip("Split view: two folders side by side", "Split view"); onClicked: bw.currentTab.toggleSplit() } }
    Component { id: tcSelection; GButton { iconName: "checkbox-checked|edit-select-all"; checked: bw.activePane && bw.activePane.selectionMode; tip: Help.tip("Selection mode: tick items with checkboxes", "Selection mode"); onClicked: bw.toggleSelectionMode() } }
    Component {
        id: tcAction
        GButton {
            id: ab
            iconName: "view-more-horizontal-symbolic|application-menu|open-menu-symbolic|overflow-menu"
            tip: "More actions: new folder, sort, show hidden files, settings, help"
            onClicked: actionMenu.popup(ab, 0, ab.height + 4)
            Component.onCompleted: bw.actionButton = this
            Component.onDestruction: if (bw.actionButton === this) bw.actionButton = null
        }
    }
    Component { id: tcJobs; JobsButton { app: bw.app; win: bw } }
    Component {
        id: tcSearch
        GField {
            id: sf
            implicitWidth: Math.max(110, Math.min(Math.round(190 * Theme.pt), main.width * 0.18))
            iconName: "edit-find"
            clearable: true
            placeholderText: "Search"
            tip: Help.tip("Search names below this folder (choose Contents in the bar to search inside files)", "Search")
            onTextEdited: bw.activePane.startSearch(text)
            onTextChanged: if (text === "" && bw.activePane && bw.activePane.searching) bw.activePane.endSearch(true)
            Keys.onEscapePressed: { text = ""; bw.activePane.forceActiveFocus() }
            Keys.onDownPressed: bw.activePane.forceActiveFocus()
            Keys.onReturnPressed: bw.activePane.forceActiveFocus()
            Connections {
                target: bw.activePane
                function onSearchingChanged() { if (!bw.activePane.searching && sf.text !== "") sf.text = "" }
            }
            Component.onCompleted: bw.search = this
            Component.onDestruction: if (bw.search === this) bw.search = null
        }
    }
    Component { id: tcNewFolder; GButton { iconName: "folder-new"; enabled: bw.activePane && !bw.activePane.special; tip: Help.tip("New folder", "New folder"); onClicked: bw.activePane.newFolder() } }
    Component { id: tcTrash; GButton { iconName: "user-trash"; enabled: bw.activePane && bw.activePane.model.selectionCount > 0; tip: Help.tip("Move the selection to the Trash", "Move to Trash"); onClicked: bw.activePane.trashSelected() } }
    Component { id: tcInfo; GButton { iconName: "document-properties|documentinfo"; tip: Help.tip("Get Info about the selection (or this folder)", "Get Info"); onClicked: { const s = bw.activePane.selectedPaths(); bw.showInfo(s.length ? s : [bw.activePane.path]) } } }
    Component { id: tcShare; GButton { iconName: "document-share|emblem-shared"; enabled: bw.activePane && bw.activePane.model.selectionCount > 0; tip: "Share the selection with a nearby device (LocalSend)"; onClicked: bw.share(bw.activePane.selectedPaths()) } }
    Component { id: tcQuickLook; GButton { iconName: "document-preview|view-preview"; tip: Help.tip("Quick Look the selection", "Quick Look"); onClicked: bw.activePane.quickLook() } }
    Component { id: tcTerminal; GButton { iconName: "utilities-terminal"; tip: Help.tip("Open a terminal in this folder", "Open in Terminal"); onClicked: bw.app.grove.openTerminal(bw.activePane.path) } }
    Component { id: tcHidden; GButton { iconName: "view-hidden|visibility"; checked: !!bw.app.settings.show_hidden; tip: Help.tip("Show or hide hidden files", "Show/hide hidden files"); onClicked: bw.toggleHidden() } }
    Component { id: tcViewOpts; GButton { iconName: "configure|preferences-system"; checked: viewOptions.opened; tip: Help.tip("View Options for this folder", "View Options (spacing, text size, labels, background)"); onClicked: bw.toggleViewOptions() } }
    Component { id: tcConnect; GButton { iconName: "network-connect|network-server"; tip: Help.tip("Connect to a server", "Connect to Server…"); onClicked: bw.connectServer() } }
    Component { id: tcEject; GButton { iconName: "media-eject"; tip: Help.tip("Eject the disk you're in", "Eject"); onClicked: bw.ejectCurrent() } }
    Component { id: tcBurn; GButton { iconName: "media-optical-burn|media-optical"; tip: Help.tip("Burn the selection (or this folder) to a disc", "Burn to Disc…"); onClicked: bw.burnSelection() } }

    GMenu {
        id: toolbarMenu
        GMenuItem { text: "Customize Toolbar…"; iconName: "configure"; tip: "Add, remove and rearrange toolbar items"; onTriggered: toolbarSheet.open() }
        GMenuItem { text: "Restore Default Toolbar"; tip: "Put back grove's standard toolbar"; onTriggered: bw.setSetting("toolbar", []) }
    }
    ToolbarSheet { id: toolbarSheet; win: bw }

    // ---------- menus ----------
    GMenu {
        id: actionMenu
        GMenuItem { text: "New Window"; iconName: "window-new"; shortcutText: Help.shown("Ctrl+N"); onTriggered: bw.app.openWindow([bw.activePane.path.indexOf("/") === 0 ? bw.activePane.path : bw.app.grove.home()]) }
        GMenuItem { text: "New Tab"; iconName: "tab-new"; shortcutText: Help.shown("Ctrl+T"); onTriggered: bw.newTab(bw.activePane.path.indexOf("/") === 0 ? bw.activePane.path : bw.app.grove.home(), []) }
        GMenuItem { text: "New Folder"; iconName: "folder-new"; shortcutText: Help.shown("Ctrl+Shift+N"); onTriggered: bw.activePane.newFolder() }
        GMenuSeparator {}
        GMenuItem { text: bw.app.grove.undoLabel ? "Undo " + bw.app.grove.undoLabel : "Undo"; iconName: "edit-undo"; shortcutText: Help.shown("Ctrl+Z"); enabled: bw.app.grove.undoLabel !== ""; onTriggered: bw.app.grove.undo() }
        GMenuItem { text: bw.app.grove.redoLabel ? "Redo " + bw.app.grove.redoLabel : "Redo"; iconName: "edit-redo"; shortcutText: Help.shown("Ctrl+Shift+Z"); enabled: bw.app.grove.redoLabel !== ""; onTriggered: bw.app.grove.redo() }
        GMenuSeparator {}
        GMenuItem { text: "Filter This Folder"; iconName: "view-filter"; shortcutText: Help.shown("Ctrl+I"); onTriggered: bw.openFilter() }
        GMenuItem { text: "Show View Options"; iconName: "configure"; shortcutText: Help.shown("Ctrl+J"); onTriggered: bw.toggleViewOptions() }
        GMenuItem { text: "Use Stacks"; checkable: true; checked: bw.activePane && bw.activePane.stacks !== ""; shortcutText: Help.shown("Ctrl+Alt+0"); onTriggered: bw.toggleStacks() }
        GMenuItem { text: "Selection Mode"; checkable: true; checked: bw.activePane && bw.activePane.selectionMode; shortcutText: Help.shown("Ctrl+Shift+S"); tip: "Checkboxes on items: clicks and Space tick them"; onTriggered: bw.toggleSelectionMode() }
        GMenuItem { text: "Show Hidden Files"; checkable: true; checked: !!bw.app.settings.show_hidden; shortcutText: Help.shown("Ctrl+H"); onTriggered: bw.toggleHidden() }
        GMenuItem { text: "Information Panel"; checkable: true; checked: !!bw.app.settings.info_panel; shortcutText: Help.shown("F11"); onTriggered: bw.setPanel("info_panel", !bw.app.settings.info_panel) }
        GMenuItem { text: "Terminal Panel"; checkable: true; checked: !!bw.app.settings.terminal_panel; shortcutText: Help.shown("F4"); onTriggered: bw.setPanel("terminal_panel", !bw.app.settings.terminal_panel) }
        GMenuItem { text: "Folders Panel"; checkable: true; checked: !!bw.app.settings.folders_panel; shortcutText: Help.shown("F7"); onTriggered: bw.setPanel("folders_panel", !bw.app.settings.folders_panel) }
        GMenuItem { text: "Split View"; checkable: true; checked: bw.currentTab && bw.currentTab.split; shortcutText: Help.shown("F3"); onTriggered: bw.currentTab.toggleSplit() }
        GMenuItem { text: "Open in Terminal"; iconName: "utilities-terminal"; shortcutText: Help.shown("Ctrl+Alt+T"); onTriggered: bw.app.grove.openTerminal(bw.activePane.path) }
        GMenuItem { text: "Connect to Server…"; iconName: "network-connect"; shortcutText: Help.shown("Ctrl+K"); onTriggered: bw.connectServer() }
        GMenuSeparator {}
        GMenuItem { text: "Empty Trash…"; iconName: "trash-empty"; enabled: bw.app.trashCount > 0; onTriggered: bw.emptyTrash() }
        GMenuSeparator {}
        GMenuItem { text: "Customize Toolbar…"; iconName: "configure"; tip: "Add, remove and rearrange toolbar items"; onTriggered: toolbarSheet.open() }
        GMenuItem { text: "Settings…"; iconName: "configure"; shortcutText: Help.shown("Ctrl+,"); onTriggered: bw.showSettings() }
        GMenuItem { text: "Help and Keyboard Shortcuts"; iconName: "help-contents"; shortcutText: Help.shown("F1"); onTriggered: bw.showHelp("") }
    }
    function editPath() { if (pathBar) pathBar.edit(); else goSheet.open() }
    function focusSearch() { if (search) { search.forceActiveFocus(); search.selectAll() } else openFilter() }
    function openFilter() {
        const p = activePane
        p.filterOpen = true
        filterFocus.restart()
    }
    Timer { id: filterFocus; interval: 30; onTriggered: { const f = bw.findFilterField(bw.activePane); if (f) f.forceActiveFocus() } }
    function findFilterField(item) {
        if (!item) return null
        for (let i = 0; i < item.children.length; i++) {
            const c = item.children[i]
            if (c.placeholderText !== undefined && c.placeholderText.indexOf("Show only") === 0) return c
            const r = findFilterField(c)
            if (r) return r
        }
        return null
    }

    // ---------- dialogs ----------
    ConflictDialog { id: conflict; app: bw.app }
    Sheet {
        id: confirmSheet
        property string actionText: "OK"
        property bool destructive: false
        property var fn: null
        buttons: [
            PushButton { text: "Cancel"; onClicked: confirmSheet.close() },
            PushButton { text: confirmSheet.actionText; primary: !confirmSheet.destructive; destructive: confirmSheet.destructive; focus: true; onClicked: { confirmSheet.close(); if (confirmSheet.fn) confirmSheet.fn() } }
        ]
    }
    BatchRename { id: renameSheet; app: bw.app; win: bw }
    VersionsSheet { id: versionsSheet; app: bw.app; win: bw }
    ShareSheet { id: shareSheet; app: bw.app; win: bw }
    Sheet {
        id: commitSheet
        property string dir: ""
        property var paths: []
        title: paths.length ? "Commit " + paths.length + " Selected Items" : "Commit All Changes"
        message: "Describe what changed. This records the changes in git (it doesn't push them)."
        onOpened: { commitMsg.text = ""; commitMsg.forceActiveFocus() }
        GField { id: commitMsg; width: parent.width; placeholderText: "What changed"; Keys.onReturnPressed: commitBtn.clicked() }
        buttons: [
            PushButton { text: "Cancel"; onClicked: commitSheet.close() },
            PushButton { id: commitBtn; text: "Commit"; primary: true; enabled: commitMsg.text !== ""; onClicked: { bw.gitAction("commit", commitSheet.dir, commitSheet.paths, commitMsg.text); commitSheet.close() } }
        ]
    }
    ConnectServer { id: connectSheet; app: bw.app; win: bw }
    // Finder's Go to Folder (when the path bar isn't in the toolbar).
    Sheet {
        id: goSheet
        title: "Go to Folder"
        onOpened: { goField.text = bw.activePane && bw.activePane.path.indexOf("/") === 0 ? bw.activePane.path : ""; goField.forceActiveFocus(); goField.selectAll() }
        GField { id: goField; width: parent.width; placeholderText: "~/Documents"; tip: "Type a folder path (~ is your home folder)"; Keys.onReturnPressed: goBtn.clicked() }
        buttons: [
            PushButton { text: "Cancel"; onClicked: goSheet.close() },
            PushButton { id: goBtn; text: "Go"; primary: true; enabled: goField.text !== ""; onClicked: { const t = goField.text.replace(/^~/, bw.app.grove.home()); goSheet.close(); bw.go(t) } }
        ]
    }
    OpenWithSheet { id: openWithSheet; app: bw.app; win: bw }
    Sheet {
        id: saveSheet
        property var pane
        title: "Save Search as a Smart Folder"
        message: "It appears in the sidebar and runs the search again whenever you open it."
        onOpened: { smartName.text = pane ? pane.searchText : ""; smartName.forceActiveFocus(); smartName.selectAll() }
        GField { id: smartName; width: parent.width; placeholderText: "Name"; Keys.onReturnPressed: saveBtn.clicked() }
        buttons: [
            PushButton { text: "Cancel"; onClicked: saveSheet.close() },
            PushButton {
                id: saveBtn
                text: "Save"; primary: true; enabled: smartName.text !== ""
                onClicked: {
                    const p = saveSheet.pane
                    const q = { name: p.searchContents ? "" : p.searchText, content: p.searchContents ? p.searchText : "", include_hidden: !!bw.app.settings.show_hidden }
                    bw.app.grove.saveSmartFolder(smartName.text, p.searchRoot, JSON.stringify(q))
                    saveSheet.close()
                    bw.toast("info", "Saved “" + smartName.text + "” in the sidebar")
                }
            }
        ]
    }
    QuickLook { id: ql; app: bw.app; win: bw }
    ViewOptions { id: viewOptions; win: bw; x: bw.width - width - 16; y: Theme.toolbarHeight + 8 }
    function toggleViewOptions() { if (viewOptions.opened) viewOptions.close(); else viewOptions.open() }
    function burnSelection() { toast("info", "Disc burning isn't ready yet.") }
    function toggleSelectionMode() { activePane.setSelectionMode(!activePane.selectionMode); activePane.forceActiveFocus() }
    function toggleStacks() { activePane.setStacks(activePane.stacks ? "" : "kind") }
    SettingsWindow { id: settingsWin; app: bw.app }
    HelpWindow { id: helpWin; app: bw.app; onChangeShortcuts: { settingsWin.page = "keyboard"; bw.showSettings() } }
    Welcome { id: welcome; app: bw.app; win: bw }

    // ---------- shortcuts ----------
    // Shortcuts come from Help (defaults) and settings.shortcuts (the user's).
    component Key: Shortcut { property string act: ""; sequences: Help.seqs(act); context: Qt.WindowShortcut; enabled: !bw.modalOpen && sequences.length > 0 }
    Key { act: "back"; onActivated: bw.activePane.goBack() }
    Key { act: "forward"; onActivated: bw.activePane.goForward() }
    Key { act: "up"; onActivated: bw.activePane.goUp() }
    Key { act: "open"; onActivated: bw.activePane.openSelected(false) }
    Key { act: "gotofolder"; onActivated: bw.editPath() }
    Key { act: "home"; onActivated: bw.go(bw.app.grove.home()) }
    Key { act: "desktop"; onActivated: bw.go(bw.app.grove.home() + "/Desktop") }
    Key { act: "documents"; onActivated: bw.go(bw.app.grove.home() + "/Documents") }
    Key { act: "downloads"; onActivated: bw.go(bw.app.grove.home() + "/Downloads") }
    Key { act: "recents"; onActivated: bw.go("recent:///") }
    Key { act: "computer"; onActivated: bw.go("/") }
    Key { act: "trash"; onActivated: bw.go("trash:///") }
    Key { act: "connect"; onActivated: bw.connectServer() }
    Key { act: "reload"; onActivated: bw.activePane.reload() }
    Key { act: "viewicons"; onActivated: bw.activePane.setView("icons") }
    Key { act: "viewlist"; onActivated: bw.activePane.setView("list") }
    Key { act: "viewcolumns"; onActivated: bw.activePane.setView("columns") }
    Key { act: "viewgallery"; onActivated: bw.activePane.setView("gallery") }
    Key { act: "zoomin"; onActivated: bw.activePane.zoom(1) }
    Key { act: "zoomout"; onActivated: bw.activePane.zoom(-1) }
    Key { act: "hidden"; onActivated: bw.toggleHidden() }
    Key { act: "filter"; onActivated: bw.openFilter() }
    Key { act: "selectionmode"; onActivated: bw.toggleSelectionMode() }
    Key { act: "viewoptions"; onActivated: bw.toggleViewOptions() }
    Key { act: "stacks"; onActivated: bw.toggleStacks() }
    Key { act: "quicklook"; onActivated: bw.activePane.quickLook() }
    Key { act: "split"; onActivated: bw.currentTab.toggleSplit() }
    Key { act: "sidebar"; onActivated: bw.sidebarShown = !bw.sidebarShown }
    Key { act: "newwindow"; onActivated: bw.app.openWindow([bw.activePane.path.indexOf("/") === 0 ? bw.activePane.path : bw.app.grove.home()]) }
    Key { act: "newtab"; onActivated: bw.newTab(bw.activePane.path.indexOf("/") === 0 ? bw.activePane.path : bw.app.grove.home(), []) }
    Key { act: "closetab"; onActivated: bw.closeTab(tabBar.currentIndex) }
    Key { act: "nexttab"; onActivated: tabBar.currentIndex = (tabBar.currentIndex + 1) % tabs.count }
    Key { act: "prevtab"; onActivated: tabBar.currentIndex = (tabBar.currentIndex + tabs.count - 1) % tabs.count }
    Key { act: "copy"; onActivated: bw.activePane.copy(false) }
    Key { act: "cut"; onActivated: bw.activePane.copy(true) }
    Key { act: "paste"; onActivated: bw.activePane.paste(false) }
    Key { act: "movehere"; onActivated: bw.activePane.paste(true) }
    Key { act: "copypath"; onActivated: bw.activePane.copyPath() }
    Key { act: "duplicate"; onActivated: bw.activePane.duplicate() }
    Key { act: "alias"; onActivated: bw.activePane.makeAlias() }
    Key { act: "rename"; onActivated: bw.activePane.rename() }
    Key { act: "batchrename"; onActivated: { const s = bw.activePane.selectedPaths(); if (s.length) bw.batchRename(s) } }
    Key { act: "newfolder"; onActivated: bw.activePane.newFolder() }
    Key { act: "newfile"; onActivated: bw.activePane.newFile("") }
    Key { act: "movetotrash"; onActivated: bw.activePane.trashSelected() }
    Key { act: "delete"; onActivated: bw.activePane.deleteSelected() }
    Key { act: "undo"; onActivated: bw.app.grove.undo() }
    Key { act: "redo"; onActivated: bw.app.grove.redo() }
    Key { act: "selectall"; onActivated: bw.activePane.model.selectAll() }
    Key { act: "invert"; onActivated: bw.activePane.model.invertSelection() }
    Key { act: "getinfo"; onActivated: { const s = bw.activePane.selectedPaths(); bw.showInfo(s.length ? s : [bw.activePane.path]) } }
    Key { act: "search"; onActivated: bw.focusSearch() }
    Key { act: "terminal"; onActivated: bw.app.grove.openTerminal(bw.activePane.path) }
    Key { act: "eject"; onActivated: bw.ejectCurrent() }
    Key { act: "settings"; onActivated: bw.showSettings() }
    Key { act: "terminalpanel"; onActivated: { bw.setPanel("terminal_panel", !bw.app.settings.terminal_panel); if (bw.app.settings.terminal_panel) bw.activePane.forceActiveFocus() } }
    Key { act: "folderspanel"; onActivated: bw.setPanel("folders_panel", !bw.app.settings.folders_panel) }
    Key { act: "infopanel"; onActivated: bw.setPanel("info_panel", !bw.app.settings.info_panel) }
    Key { act: "copyother"; onActivated: bw.toOtherPane(false) }
    Key { act: "moveother"; onActivated: bw.toOtherPane(true) }
    Key { act: "help"; onActivated: bw.showHelp("") }
    Key { act: "tagred"; onActivated: bw.activePane.toggleTag("Red") }
    Key { act: "tagorange"; onActivated: bw.activePane.toggleTag("Orange") }
    Key { act: "tagyellow"; onActivated: bw.activePane.toggleTag("Yellow") }
    Key { act: "taggreen"; onActivated: bw.activePane.toggleTag("Green") }
    Key { act: "tagblue"; onActivated: bw.activePane.toggleTag("Blue") }
    Key { act: "tagpurple"; onActivated: bw.activePane.toggleTag("Purple") }
    Key { act: "taggray"; onActivated: bw.activePane.toggleTag("Gray") }

    Component.onCompleted: {
        if (initialStates.length) for (const st of initialStates) tabs.append({ path: st.path, select: "[]", tstate: JSON.stringify(st) })
        else {
            const paths = initialPaths.length ? initialPaths : [app.grove.home()]
            for (const p of paths) tabs.append({ path: p, select: JSON.stringify(initialSelect), tstate: "" })
        }
        if (!app.settings.welcomed && app.screenshot === "") welcome.open()
        if (app.screenshot !== "") shotTimer.start()
    }

    // Test hook: --screenshot FILE grabs the window and quits.
    Timer {
        id: shotTimer
        interval: Number(bw.app.grove.env("GROVE_SHOT_DELAY") || 2500)
        onTriggered: bw.runSteps((bw.app.grove.env("GROVE_TEST") || "").split(";").filter(s => s))
    }
    // "sleep=MS" waits before the steps after it (async things to settle).
    function runSteps(steps) {
        for (let i = 0; i < steps.length; i++) {
            if (steps[i].indexOf("sleep=") === 0) {
                stepTimer.rest = steps.slice(i + 1)
                stepTimer.interval = Number(steps[i].substring(6))
                stepTimer.start()
                return
            }
            bw.testStep(steps[i])
        }
        shotTimer2.start()
    }
    Timer { id: stepTimer; property var rest: []; onTriggered: bw.runSteps(rest) }
    Timer {
        id: shotTimer2
        interval: Number(bw.app.grove.env("GROVE_SHOT_DELAY2") || 1500)
        onTriggered: { bw.app.grove.grabWindows(bw.app.screenshot); console.log("grove: screenshot " + bw.app.screenshot + " (" + bw.activePane.model.lastLoadMs() + " ms listing, " + bw.activePane.model.count + " items)"); Qt.quit() }
    }
    function testStep(s) {
        const [cmd, arg] = s.split("=")
        const p = activePane
        if (cmd === "view") p.setView(arg)
        else if (cmd === "go") p.navigate(arg)
        else if (cmd === "select") p.model.selectPaths(JSON.stringify(arg.split(",")))
        else if (cmd === "selectrow") p.model.select(Number(arg), "set")
        else if (cmd === "split") currentTab.toggleSplit()
        else if (cmd === "tab") newTab(arg, [])
        else if (cmd === "ql") quickLook(p)
        else if (cmd === "info") showInfo([arg])
        else if (cmd === "filter") { openFilter(); p.model.applyFilter(arg) }
        else if (cmd === "search") { if (search) search.text = arg; p.startSearch(arg) }
        else if (cmd === "menu") p.showItemMenu(Number(arg), 100, 100)
        else if (cmd === "bgmenu") p.showBackgroundMenu()
        else if (cmd === "actions") { if (actionButton) actionMenu.popup(actionButton, 0, actionButton.height + 4); else actionMenu.popup() }
        else if (cmd === "toolbar") setSetting("toolbar", arg ? arg.split(",") : [])
        else if (cmd === "customize") toolbarSheet.open()
        else if (cmd === "shortcut") { const i = arg.indexOf(":"); const c = Object.assign({}, app.settings.shortcuts || {}); c[arg.substring(0, i)] = arg.substring(i + 1).split("|"); setSetting("shortcuts", c) }
        else if (cmd === "record") { settingsWin.page = "keyboard"; showSettings(); settingsWin.record(arg) }
        else if (cmd === "qledit") {
            ql.editing = arg
            if (arg === "markup") ql.markupDemo()
            else { ql.trimFrom(2000, 5000) }
        }
        else if (cmd === "qlsave") ql.saveEdit(arg)
        else if (cmd === "tip") { setSetting("preview_tooltips", true); p.testTip = Number(arg) }
        else if (cmd === "recordkey") settingsWin.recordSeq(arg)
        else if (cmd === "key") { const [k, m] = arg.split(":"); app.grove.testKey(Number(k), Number(m || 0), "") }
        else if (cmd === "settings") { if (arg) settingsWin.page = arg; showSettings() }
        else if (cmd === "help") showHelp(arg || "")
        else if (cmd === "connect") connectServer()
        else if (cmd === "rename") batchRename(p.selectedPaths())
        else if (cmd === "conflict") conflict.ask(-1, { source: arg, target: arg, source_is_dir: false, target_is_dir: false, source_size: 1200, target_size: 3400, source_mtime: 1700000000, target_mtime: 1690000000 })
        else if (cmd === "zoom") p.iconSize = Number(arg)
        else if (cmd === "expand") p.model.toggleExpanded(Number(arg))
        else if (cmd === "welcome") welcome.open()
        else if (cmd === "toast") toast("info", arg)
        else if (cmd === "light") Theme.mode = "light"
        else if (cmd === "hidden") toggleHidden()
        else if (cmd === "share") share([arg])
        else if (cmd === "versions") showVersions(arg)
        else if (cmd === "panel") setPanel(arg, true)
        else if (cmd === "group") p.setGroupBy(arg)
        else if (cmd === "viewopts") viewOptions.open()
        else if (cmd === "selmode") p.setSelectionMode(true)
        else if (cmd === "tearoff") tearOffTab(Number(arg))
        else if (cmd === "movetab") { const o = app.windows.filter(w => w !== bw); if (o.length) o[0].acceptTab(bw, Number(arg), -1) }
        else if (cmd === "back") p.goBack()
        else if (cmd === "stacks") p.setStacks(arg)
        else if (cmd === "openstack") Qt.callLater(() => p.view.toggleStack(arg))
        else if (cmd === "opt") { const [k, v] = arg.split(":"); p.setViewOption(k, isNaN(Number(v)) ? (v === "true" ? true : v === "false" ? false : v) : Number(v)) }
        else if (cmd === "bg") p.setViewOption("background", arg.replace("~", "#"))
    }
}
