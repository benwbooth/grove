import QtQuick
import QtCore
import QtQuick.Controls.Basic
import QtQuick.Window
import QtQuick.Layouts
import GlobalMenu

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
    property bool modalOpen: burnSheet.opened || toolbarSheet.opened || goSheet.opened || shareSheet.opened || commitSheet.opened || versionsSheet.opened || conflict.opened || confirmSheet.opened || renameSheet.opened || connectSheet.opened || cloudSheet.opened || saveSheet.opened || openWithSheet.opened || aboutSheet.opened

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
    // Get Info and Disk Usage windows open now (for the session).
    property var infoWins: []
    property var duWins: []
    function showInfo(paths) {
        for (const p of paths.slice(0, 10)) {
            const w = infoComp.createObject(null, { app: app, path: p })
            infoWins = infoWins.concat([w])
            w.show()
        }
    }
    // Everything about this window for the session: its tabs (folders,
    // scroll, open folders, viewed files), size, the tab in front, the side
    // panel's file, Quick Look, and its Get Info and Disk Usage windows.
    function windowState() {
        infoWins = infoWins.filter(w => w && w.visible)
        duWins = duWins.filter(w => w && w.visible)
        const side = infoPanel.visible && infoPanel.viewerItem ? infoPanel.viewerItem.viewState() : null
        return {
            states: tabStates(),
            tab: tabBar.currentIndex,
            w: Math.round(bw.width), h: Math.round(bw.height),
            max: bw.visibility === Window.Maximized, full: bw.visibility === Window.FullScreen,
            side: side,
            ql: ql.viewState(),
            info: infoWins.map(w => w.path),
            du: duWins.map(w => w.path)
        }
    }
    // Back as it was: called once the tabs are made.
    function restoreWindow(s) {
        if (s.w > 200 && s.h > 150) { bw.width = s.w; bw.height = s.h }
        if (s.max) bw.showMaximized()
        else if (s.full) bw.showFullScreen()
        if (s.tab >= 0 && s.tab < tabs.count) tabBar.currentIndex = s.tab
        if (s.info && s.info.length) showInfo(s.info)
        for (const p of (s.du || [])) showDiskUsage(p)
        if (s.side || s.ql) { restoreExtras = { side: s.side, ql: s.ql, tries: 0 }; extrasTimer.start() }
    }
    property var restoreExtras: null
    Timer {
        id: extrasTimer
        interval: 400
        onTriggered: {
            const r = bw.restoreExtras
            const p = bw.activePane
            if (!r || !p) return
            if (p.model.loading && r.tries++ < 20) { start(); return }
            if (r.side && infoPanel.viewerItem) infoPanel.viewerItem.restoreView(r.side)
            if (r.ql) {
                const row = p.model.rowOf(r.ql.path)
                if (row >= 0) {
                    p.model.select(row, "set")
                    ql.pane = p
                    ql.showItem(p.itemAt(row))
                    ql.restoreView(r.ql)
                }
            }
            bw.restoreExtras = null
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
    // With hearth's accounts service, cloud drives and Google sign-in are
    // set up in hearth's Settings > Accounts; grove's own sheets are the
    // fallback when it isn't there (or can't open Settings).
    function addCloudDrive() { connectSheet.close(); if (app.accountsService) openAccounts("add", () => cloudSheet.start()); else cloudSheet.start() }
    // Sign in with Google (Google Docs/Sheets/Slides previews), with its setup.
    function googleSignIn() { if (app.accountsService) openAccounts("setup:google", () => googleSheet.start()); else googleSheet.start() }
    function googleSheetStart() { googleSheet.start() }
    // account: an account id, or one of Settings' pages: "add", "server"
    // (add a file server), "setup:google".
    function openAccounts(account, fallback) {
        Async.call(app.grove, k => app.grove.accountsOpenSettings(k, account || ""), r => { if (!r.ok && fallback) fallback() })
    }
    // A Google account whose sign-in ended: the accounts service signs in
    // again in the browser (previews try again by themselves afterwards).
    function accountSignInAgain(account) {
        Async.call(app.grove, k => app.grove.accountsSignIn(k, "google", account || ""), r => {
            if (r.error) toast("error", r.error)
            else toast("info", "Sign in to Google in your browser…")
        })
    }
    function connectSaved(s) {
        toast("info", "Connecting to “" + s.name + "”…")
        Async.call(app.grove, k => app.grove.connectServer(k, JSON.stringify({ name: s.name, kind: s.kind, host: s.host, user: s.user, path: s.path, port: s.port || 0, account: s.account || "", remember: true })), r => {
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
    // List and icon views show the preview under the files; the others at the side.
    // (Settings: info_bottom; the panel's own button or View > Information Panel Below moves it.)
    readonly property bool bottomPreviewView: !!activePane && (activePane.viewMode === "list" || activePane.viewMode === "icons") && !chooserMode && app.settings.info_bottom !== false
    readonly property bool infoShown: bottomPreviewView ? app.settings.list_preview !== false : !!app.settings.info_panel
    // Move the panel under the files or to the side, showing it there.
    function placeInfo(bottom) {
        setSetting("info_bottom", bottom)
        if (bottom) setSetting("list_preview", true)
        else setSetting("info_panel", true)
    }
    function toggleInfo() {
        if (activePane && activePane.viewMode === "columns") return
        if (bottomPreviewView) setSetting("list_preview", app.settings.list_preview === false)
        else setPanel("info_panel", !app.settings.info_panel)
    }
    function askCommit(dir, paths) { commitSheet.dir = dir; commitSheet.paths = paths; commitSheet.open() }

    function showSettings() { settingsWin.show(); settingsWin.raise(); settingsWin.requestActivate() }
    function showHelp(search) { helpWin.query = search || ""; helpWin.show(); helpWin.raise(); helpWin.requestActivate() }

    Component { id: infoComp; InfoWindow {} }
    Component { id: duComp; DiskUsageWindow {} }
    // Disk Usage for a folder (default: the selected folder, else this one).
    function showDiskUsage(path) {
        let p = path
        if (!p) {
            const it = activePane.currentItem ? activePane.currentItem() : null
            p = it && it.isDir && activePane.selectedPaths().length === 1 ? it.path : activePane.path
        }
        if (!p || p.indexOf("/") !== 0) p = StandardPaths.writableLocation(StandardPaths.HomeLocation).toString().replace("file://", "")
        const w = duComp.createObject(null, { app: app, win: bw, path: p })
        duWins = duWins.concat([w])
        w.show(); w.raise(); w.requestActivate()
    }
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
            // Below the toolbar, which spans the whole window.
            x: 8; y: Theme.toolbarHeight
            width: bw.sidebarWidth
            height: parent.height - y - 8
            currentPath: bw.activePane ? bw.activePane.path : ""
        }
        FoldersPanel {
            id: foldersPanel
            win: bw
            visible: !!bw.app.settings.folders_panel
            x: bw.sidebarShown ? sidebar.x + sidebar.width + 8 : 8
            y: Theme.toolbarHeight
            width: Math.round(220 * Theme.pt)
            height: parent.height - y - 8
        }
        MouseArea {
            visible: bw.sidebarShown
            x: sidebar.x + sidebar.width
            y: sidebar.y
            width: 8
            height: sidebar.height
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
            // Spans the whole window (out over the sidebar column) so the
            // toolbar has room; the sidebar sits below it.
            Item {
                id: toolbar
                x: -main.x
                width: root.width
                height: Theme.toolbarHeight
                MouseArea {
                    anchors.fill: parent
                    acceptedButtons: Qt.LeftButton | Qt.RightButton
                    onPressed: (m) => { if (m.button === Qt.RightButton) toolbarMenu.popup(); else bw.startSystemMove() }
                    onDoubleClicked: bw.visibility === Window.Maximized ? bw.showNormal() : bw.showMaximized()
                }
                // Window buttons, Breeze style, when grove draws its own title bar.
                WindowControls {
                    id: winControls
                    window: bw
                    visible: bw.clientDecorations
                    anchors.right: parent.right
                    anchors.rightMargin: 10
                    anchors.verticalCenter: parent.verticalCenter
                }
                // The items from Customize Toolbar (settings "toolbar").
                RowLayout {
                    id: toolRow
                    anchors.left: parent.left
                    anchors.leftMargin: 12
                    anchors.right: winControls.visible ? winControls.left : parent.right
                    anchors.rightMargin: winControls.visible ? 12 : 10
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8
                    // Never draw under the window buttons, however narrow the window.
                    clip: true
                    Repeater {
                        model: bw.toolbarItems
                        Loader {
                            required property string modelData
                            readonly property bool flexible: modelData === "path" || modelData === "flexspace"
                            // The search field shrinks before anything else drops out.
                            readonly property bool shrinks: modelData === "search"
                            Layout.fillWidth: flexible || shrinks
                            Layout.minimumWidth: modelData === "path" ? 80 : shrinks ? 90 : 0
                            Layout.maximumWidth: shrinks && item ? item.implicitWidth : Number.POSITIVE_INFINITY
                            Layout.preferredWidth: modelData === "space" ? 16 : flexible ? 10 : (item ? item.implicitWidth : 0)
                            Layout.preferredHeight: Theme.controlHeight
                            // As the toolbar narrows, the least-used items step aside
                            // (they stay in the action menu and the keyboard).
                            visible: bw.toolbarFits(modelData, toolRow.width)
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
                // Not in column view: its own preview column shows the same.
                visible: !!bw.app.settings.info_panel && !bw.bottomPreviewView && !(bw.activePane && bw.activePane.viewMode === "columns")
                anchors.top: tabBar.bottom
                anchors.bottom: statusBar.top
                anchors.right: parent.right
                // Drag the left edge to resize (remembered: info_panel_width).
                property real dragW: 0
                readonly property real savedW: bw.app.settings.info_panel_width > 0 ? bw.app.settings.info_panel_width : Math.round(260 * Theme.pt)
                onSavedWChanged: dragW = 0
                width: dragW > 0 ? dragW : savedW
                property var item: null
                readonly property var viewerItem: sidePreview.viewer
                // Shown again (moved here from under the files): the selection now.
                onVisibleChanged: if (visible) refresh()
                Rectangle { width: 1; height: parent.height; color: infoEdge.containsMouse || infoEdge.pressed ? Theme.accent : Theme.separator }
                MouseArea {
                    id: infoEdge
                    z: 5
                    x: -3; width: 7; height: parent.height
                    hoverEnabled: true
                    cursorShape: Qt.SizeHorCursor
                    property real startX: 0
                    property real startW: 0
                    onPressed: (m) => { startX = mapToItem(null, m.x, 0).x; startW = infoPanel.width }
                    onPositionChanged: (m) => {
                        if (!pressed) return
                        const x = mapToItem(null, m.x, 0).x
                        infoPanel.dragW = Math.max(200, Math.min(bw.width * 0.6, startW - (x - startX)))
                    }
                    onReleased: if (infoPanel.dragW > 0) bw.setSetting("info_panel_width", Math.round(infoPanel.dragW))
                    onDoubleClicked: { infoPanel.dragW = 0; bw.setSetting("info_panel_width", 0) }
                    GTip { visible: infoEdge.containsMouse && !infoEdge.pressed; text: "Drag to resize the panel; double-click for its usual width" }
                }
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
                // Move it under the files (list and icon views).
                GButton {
                    z: 3
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.margins: 10
                    visible: !!bw.activePane && (bw.activePane.viewMode === "list" || bw.activePane.viewMode === "icons")
                    iconName: "view-split-top-bottom"
                    iconSize: 14
                    tip: "Show the Information panel under the files instead (View > Information Panel Below Files)"
                    onClicked: bw.placeInfo(true)
                }
                PreviewPanel {
                    id: sidePreview
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
                // Dolphin's status bar: the summary, and the free space (Finder
                // puts "… available" there too).
                property string freeText: ""
                function refreshFree() {
                    const p = bw.activePane ? bw.activePane.path : ""
                    if (p.indexOf("/") !== 0) { freeText = ""; return }
                    Async.call(bw.app.grove, k => bw.app.grove.freeSpace(k, p), r => {
                        if (!bw.activePane || bw.activePane.path !== p) return
                        statusBar.freeText = r.total > 0 ? bw.app.size(r.free) + " available" : ""
                    })
                }
                Connections { target: bw; function onActivePaneChanged() { statusBar.refreshFree() } }
                Connections { target: bw.activePane ? bw.activePane.model : null; function onLoaded() { statusBar.refreshFree() } }
                Connections { target: bw.app.grove; function onFoldersTouched() { freeTimer.restart() } }
                Timer { id: freeTimer; interval: 800; onTriggered: statusBar.refreshFree() }
                GLabel {
                    x: 12
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - zoomRow.width - 40
                    text: (bw.activePane ? bw.activePane.model.status : "") + (statusBar.freeText ? ", " + statusBar.freeText : "")
                    font.pixelSize: Theme.small
                    color: Theme.fgSecondary
                    elide: Text.ElideRight
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
    readonly property var toolbarDefaults: ["nav", "sidebar", "path", "view", "sort", "split", "selection", "action", "jobs", "search"]
    readonly property var toolbarItems: app.settings.toolbar && app.settings.toolbar.length ? app.settings.toolbar : toolbarDefaults
    readonly property var toolbarAll: ["nav", "sidebar", "path", "view", "sort", "split", "selection", "action", "jobs", "search", "newfolder", "trash", "info", "share", "quicklook", "terminal", "hidden", "viewopts", "connect", "eject", "burn", "space", "flexspace"]
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
        sort: { label: "Sort", icon: "view-sort|view-sort-ascending", tip: "Sort and group the items (in every view)" },
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
    // Width below which a toolbar item hides (0: always shown).
    function toolbarFits(k, w) {
        const minW = { split: 760, selection: 700, viewopts: 680, hidden: 680, terminal: 680, connect: 680, burn: 680,
                       eject: 680, share: 640, info: 640, quicklook: 640, newfolder: 620, trash: 620, jobs: 0,
                       sidebar: 560, view: 520, space: 600, flexspace: 0 }[k] || 0
        return w >= minW
    }
    function toolComponent(k) {
        return ({ nav: tcNav, sidebar: tcSidebar, path: tcPath, view: tcView, split: tcSplit, selection: tcSelection, action: tcAction,
                  jobs: tcJobs, search: tcSearch, newfolder: tcNewFolder, trash: tcTrash, info: tcInfo, share: tcShare, quicklook: tcQuickLook,
                  terminal: tcTerminal, hidden: tcHidden, viewopts: tcViewOpts, sort: tcSort, connect: tcConnect, eject: tcEject, burn: tcBurn,
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
            tip: "Menus: Grove, File, Edit, View, Go, Window, Help (also in the menu bar at the top of the screen)"
            // A second click closes it: the press outside already closed the
            // menu, so don't open it again right away.
            onClicked: bw.toggleAppMenu(ab)
            Component.onCompleted: bw.actionButton = this
            Component.onDestruction: if (bw.actionButton === this) bw.actionButton = null
        }
    }
    // Sort and group, in every view (Finder's toolbar Group button).
    Component {
        id: tcSort
        GButton {
            id: sortBtn
            iconName: "view-sort|view-sort-ascending"
            tip: Help.tip("Sort and group the items in this folder", "Sort By")
            onClicked: sortMenu.visible ? sortMenu.close() : sortMenu.popup(sortBtn, 0, sortBtn.height + 4)
            GMenu {
                id: sortMenu
                readonly property var p: bw.activePane
                readonly property var m: p ? p.model : null
                GMenuItem { text: "Name"; iconName: "view-sort-ascending-name|view-sort-ascending|view-sort"; checkable: true; checked: !!sortMenu.m && sortMenu.m.sortKey === "name"; onTriggered: sortMenu.p.sortBy("name") }
                GMenuItem { text: "Kind"; iconName: "view-categories|folder-documents|view-list-tree"; checkable: true; checked: !!sortMenu.m && sortMenu.m.sortKey === "kind"; onTriggered: sortMenu.p.sortBy("kind") }
                GMenuItem { text: "Date Modified"; iconName: "document-edit|x-office-calendar"; checkable: true; checked: !!sortMenu.m && sortMenu.m.sortKey === "modified"; onTriggered: sortMenu.p.sortBy("modified") }
                GMenuItem { text: "Date Created"; iconName: "document-new|x-office-calendar"; checkable: true; checked: !!sortMenu.m && sortMenu.m.sortKey === "created"; onTriggered: sortMenu.p.sortBy("created") }
                GMenuItem { text: "Date Added"; iconName: "list-add"; checkable: true; checked: !!sortMenu.m && sortMenu.m.sortKey === "added"; onTriggered: sortMenu.p.sortBy("added") }
                GMenuItem { text: "Last Opened"; iconName: "document-open-recent"; checkable: true; checked: !!sortMenu.m && sortMenu.m.sortKey === "accessed"; onTriggered: sortMenu.p.sortBy("accessed") }
                GMenuItem { text: "Size"; iconName: "drive-harddisk|view-sort"; checkable: true; checked: !!sortMenu.m && sortMenu.m.sortKey === "size"; onTriggered: sortMenu.p.sortBy("size") }
                GMenuSeparator {}
                GMenuItem { text: "Descending"; iconName: "view-sort-descending|go-down"; checkable: true; checked: !!sortMenu.m && sortMenu.m.descending; onTriggered: { sortMenu.m.applySort(sortMenu.m.sortKey, !sortMenu.m.descending); sortMenu.p.rememberView() } }
                GMenuItem { text: "Folders First"; iconName: "folder"; checkable: true; checked: !!sortMenu.m && sortMenu.m.foldersFirst; onTriggered: bw.setSetting("folders_first", !sortMenu.m.foldersFirst) }
                GMenuSeparator {}
                GMenu {
                    title: "Group By"
                    GMenuItem { text: "None"; checkable: true; checked: !!sortMenu.p && sortMenu.p.groupBy === ""; onTriggered: sortMenu.p.setGroupBy("") }
                    GMenuItem { text: "Kind"; checkable: true; checked: !!sortMenu.p && sortMenu.p.groupBy === "kind"; onTriggered: sortMenu.p.setGroupBy("kind") }
                    GMenuItem { text: "Date Modified"; checkable: true; checked: !!sortMenu.p && sortMenu.p.groupBy === "date"; onTriggered: sortMenu.p.setGroupBy("date") }
                    GMenuItem { text: "Size"; checkable: true; checked: !!sortMenu.p && sortMenu.p.groupBy === "size"; onTriggered: sortMenu.p.setGroupBy("size") }
                    GMenuItem { text: "Name"; checkable: true; checked: !!sortMenu.p && sortMenu.p.groupBy === "name"; onTriggered: sortMenu.p.setGroupBy("name") }
                    GMenuItem { text: "Tags"; checkable: true; checked: !!sortMenu.p && sortMenu.p.groupBy === "tags"; onTriggered: sortMenu.p.setGroupBy("tags") }
                }
            }
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
            onTextChanged: if (text === "" && bw.activePane && bw.activePane.searching && !bw.activePane.searchFiltered) bw.activePane.endSearch(true)
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
        GMenuItem { text: "Restore Default Toolbar"; iconName: "edit-undo|document-revert"; tip: "Put back grove's standard toolbar"; onTriggered: bw.setSetting("toolbar", []) }
    }
    ToolbarSheet { id: toolbarSheet; win: bw }
    BurnSheet { id: burnSheet; app: bw.app; win: bw }

    // ---------- menus ----------
    // Defined once: exported to the desktop's global menu bar (GlobalMenuBar:
    // hearth's top bar, Plasma's Global Menu) and shown by the toolbar's
    // action button (AppMenu). Shortcuts are only shown there: the Key
    // shortcuts below (and the view's Keys.onPressed) do the work.
    function menuKey(act) { return Help.seqs(act)[0] || "" }
    readonly property var menuSel: activePane ? activePane.model.selectionCount : 0
    // The one selected item (for Open With, Extract), else null.
    readonly property var menuTarget: {
        const p = activePane
        if (!p || p.model.selectionCount !== 1 || p.model.currentRow < 0) return null
        return p.itemAt(p.model.currentRow)
    }
    property var menuApps: []
    onMenuTargetChanged: appsTimer.restart()
    Timer {
        id: appsTimer
        interval: 150
        onTriggered: {
            const t = bw.menuTarget
            if (!t || t.isDir || !t.path || bw.activePane.inTrash) { bw.menuApps = []; return }
            const path = t.path
            Async.call(bw.app.grove, k => bw.app.grove.appsFor(k, path), r => { if (bw.menuTarget && bw.menuTarget.path === path) bw.menuApps = r.apps || [] })
        }
    }
    readonly property var menuModel: {
        const p = activePane
        const ok = !!p
        const special = ok && p.special
        const trash = ok && p.inTrash
        const sel = menuSel
        const t = menuTarget
        const view = ok ? p.viewMode : ""
        const sortKey = ok ? p.model.sortKey : ""
        const group = ok ? p.groupBy : ""
        const split = !!(currentTab && currentTab.split)
        const st = app.settings
        const k = menuKey
        // Icons as in the view's Sort By / Group By context menu.
        const keyIcon = { name: "view-sort-ascending-name", kind: "folder-documents", modified: "document-edit", date: "document-edit",
                          created: "x-office-calendar", added: "list-add", accessed: "document-open-recent", size: "drive-harddisk",
                          tags: "tag", "": "edit-clear" }
        const sortItem = (id, text) => ({ id: "sort." + id, icon: keyIcon[id], text: text, checkable: true, checked: sortKey === id, enabled: ok })
        const groupItem = (id, text) => ({ id: "group." + id, icon: keyIcon[id], text: text, checkable: true, checked: group === id, enabled: ok })
        const tagItem = (n, i) => ({ id: "tag." + n, icon: "tag", text: n, shortcut: k("tag" + n.toLowerCase()), enabled: sel > 0 && !special })
        // The app's own icon (a theme name; a file path can't be shown by name).
        const openWith = menuApps.map((a, i) => ({ id: "openwith." + i, icon: a.icon && a.icon.indexOf("/") < 0 ? a.icon : "system-run", text: a.name + (a.isDefault ? "  (default)" : "") }))
        if (openWith.length) openWith.push({ separator: true })
        openWith.push({ id: "openwith.other", icon: "application-x-executable", text: "Other…", enabled: !!t && !t.isDir && !trash })
        return [
            { title: "Grove", icon: "system-file-manager", items: [
                { id: "about", icon: "help-about", text: "About Grove" },
                { separator: true },
                { id: "settings", icon: "configure", text: "Settings…", shortcut: k("settings") },
                { id: "customizetoolbar", icon: "configure-toolbars", text: "Customize Toolbar…" },
                { separator: true },
                { id: "emptytrash", icon: "trash-empty", text: "Empty Trash…", enabled: app.trashCount > 0 },
                { separator: true },
                { id: "quit", icon: "application-exit", text: "Quit Grove" }
            ] },
            { title: "File", icon: "folder", items: [
                { id: "newwindow", icon: "window-new", text: "New Window", shortcut: k("newwindow") },
                { id: "newtab", icon: "tab-new", text: "New Tab", shortcut: k("newtab") },
                { id: "newfolder", icon: "folder-new", text: "New Folder", shortcut: k("newfolder"), enabled: ok && !special },
                { id: "newfile", icon: "document-new", text: "New File", shortcut: k("newfile"), enabled: ok && !special },
                { separator: true },
                { id: "open", icon: "document-open", text: "Open", shortcut: k("open"), enabled: sel > 0 && !trash },
                { id: "opentab", icon: "tab-new-background", text: "Open in New Tab", shortcut: "Ctrl+Return", enabled: !!t && !!t.isDir && !trash },
                { id: "openwith", icon: "preferences-desktop-filetype-association", text: "Open With", enabled: !!t && !t.isDir && !trash, items: openWith },
                { id: "quicklook", icon: "quickview", text: "Quick Look", shortcut: k("quicklook"), enabled: ok && p.model.currentRow >= 0 },
                { id: "getinfo", icon: "documentinfo", text: "Get Info", shortcut: k("getinfo"), enabled: ok },
                { separator: true },
                { id: "rename", icon: "edit-rename", text: sel > 1 ? "Rename " + sel + " Items…" : "Rename…", shortcut: k("rename"), enabled: sel > 0 && !special },
                { id: "duplicate", icon: "edit-copy", text: "Duplicate", shortcut: k("duplicate"), enabled: sel > 0 && !special },
                { id: "alias", icon: "insert-link", text: "Make Alias", shortcut: k("alias"), enabled: sel > 0 && !special },
                { id: "compress", icon: "archive-insert", text: sel > 1 ? "Compress " + sel + " Items" : t ? "Compress “" + t.name + "”" : "Compress", enabled: sel > 0 && !special },
                { id: "extract", icon: "archive-extract", text: "Extract Here", enabled: !!t && p.archiveLike(t) && !special },
                { id: "extractto", icon: "folder-open", text: "Extract To…", enabled: !!t && p.archiveLike(t) && !special },
                { id: "burn", icon: "tools-media-optical-burn", text: "Burn to Disc…", enabled: ok && (sel > 0 || !special) },
                { id: "share", icon: "document-share", text: "Share with Nearby Device…", enabled: sel > 0 && !special && !(t && t.isDir) },
                { separator: true },
                { id: "putback", icon: "edit-undo", text: "Put Back", enabled: trash && sel > 0 },
                { id: "movetotrash", icon: "user-trash", text: trash ? "Delete Immediately…" : "Move to Trash", shortcut: k("movetotrash"), enabled: sel > 0 && (!special || trash) },
                { id: "delete", icon: "edit-delete", text: "Delete Permanently…", shortcut: k("delete"), enabled: sel > 0 && !special },
                { separator: true },
                { id: "eject", icon: "media-eject", text: "Eject", shortcut: k("eject"), enabled: ok },
                { id: "closetab", icon: "tab-close", text: "Close Tab", shortcut: tabs.count > 1 ? k("closetab") : "", enabled: tabs.count > 1 },
                { id: "closewindow", icon: "window-close", text: "Close Window", shortcut: tabs.count > 1 ? "" : k("closetab") }
            ] },
            { title: "Edit", icon: "document-edit", items: [
                { id: "undo", icon: "edit-undo", text: app.grove.undoLabel ? "Undo " + app.grove.undoLabel : "Undo", shortcut: k("undo"), enabled: app.grove.undoLabel !== "" },
                { id: "redo", icon: "edit-redo", text: app.grove.redoLabel ? "Redo " + app.grove.redoLabel : "Redo", shortcut: k("redo"), enabled: app.grove.redoLabel !== "" },
                { separator: true },
                { id: "cut", icon: "edit-cut", text: "Cut", shortcut: k("cut"), enabled: sel > 0 && (!special || trash) },
                { id: "copy", icon: "edit-copy", text: "Copy", shortcut: k("copy"), enabled: sel > 0 },
                { id: "paste", icon: "edit-paste", text: "Paste", shortcut: k("paste"), enabled: ok && !special },
                { id: "movehere", icon: "transform-move", text: "Move Here", shortcut: k("movehere"), enabled: ok && !special },
                { id: "copypath", icon: "edit-copy-path", text: "Copy Path", shortcut: k("copypath"), enabled: ok },
                { id: "copyother", icon: "go-next", text: "Copy to Other Side", shortcut: k("copyother"), enabled: split && sel > 0 },
                { id: "moveother", icon: "go-next-view", text: "Move to Other Side", shortcut: k("moveother"), enabled: split && sel > 0 },
                { separator: true },
                { id: "selectall", icon: "edit-select-all", text: "Select All", shortcut: k("selectall"), enabled: ok },
                { id: "invert", icon: "edit-select-invert", text: "Invert Selection", shortcut: k("invert"), enabled: ok },
                { id: "selectionmode", icon: "checkbox", text: "Selection Mode", shortcut: k("selectionmode"), checkable: true, checked: ok && p.selectionMode, enabled: ok },
                { separator: true },
                { id: "batchrename", icon: "edit-rename", text: "Batch Rename…", shortcut: k("batchrename"), enabled: sel > 0 && !special },
                { id: "tags", icon: "tag", text: "Tags", enabled: sel > 0 && !special, items: ["Red", "Orange", "Yellow", "Green", "Blue", "Purple", "Gray"].map(tagItem) },
                { separator: true },
                { id: "search", icon: "edit-find", text: "Find…", shortcut: k("search"), enabled: ok },
                { id: "filter", icon: "view-filter", text: "Filter This Folder", shortcut: k("filter"), enabled: ok }
            ] },
            { title: "View", icon: "view-visible", items: [
                { id: "viewicons", icon: "view-list-icons", text: "As Icons", shortcut: k("viewicons"), checkable: true, checked: view === "icons", enabled: ok },
                { id: "viewlist", icon: "view-list-details", text: "As List", shortcut: k("viewlist"), checkable: true, checked: view === "list", enabled: ok },
                { id: "viewcolumns", icon: "view-file-columns", text: "As Columns", shortcut: k("viewcolumns"), checkable: true, checked: view === "columns", enabled: ok },
                { id: "viewgallery", icon: "view-preview", text: "As Gallery", shortcut: k("viewgallery"), checkable: true, checked: view === "gallery", enabled: ok },
                { separator: true },
                { id: "stacks", icon: "view-list-tree", text: "Use Stacks", shortcut: k("stacks"), checkable: true, checked: ok && p.stacks !== "", enabled: ok },
                { id: "sortby", icon: "view-sort", text: "Sort By", enabled: ok, items: [
                    sortItem("name", "Name"), sortItem("kind", "Kind"), sortItem("modified", "Date Modified"),
                    sortItem("created", "Date Created"), sortItem("added", "Date Added"), sortItem("accessed", "Last Opened"),
                    sortItem("size", "Size"),
                    { separator: true },
                    { id: "sort.descending", icon: "view-sort-descending", text: "Descending", checkable: true, checked: ok && p.model.descending, enabled: ok },
                    { id: "sort.foldersfirst", icon: "folder", text: "Folders First", checkable: true, checked: ok && p.model.foldersFirst, enabled: ok }
                ] },
                { id: "groupby", icon: "view-group", text: "Group By", enabled: ok, items: [
                    groupItem("", "None"), groupItem("kind", "Kind"), groupItem("date", "Date Modified"),
                    groupItem("size", "Size"), groupItem("name", "Name"), groupItem("tags", "Tags")
                ] },
                { separator: true },
                { id: "sidebar", icon: "sidebar-expand-left", text: "Sidebar", shortcut: k("sidebar"), checkable: true, checked: sidebarShown },
                { id: "folderspanel", icon: "view-list-tree", text: "Folders Panel", shortcut: k("folderspanel"), checkable: true, checked: !!st.folders_panel },
                { id: "infopanel", icon: "dialog-information", text: "Information Panel", shortcut: k("infopanel"), checkable: true, checked: infoShown, enabled: !(activePane && activePane.viewMode === "columns") },
                { id: "infobottom", icon: "view-split-top-bottom", text: "Information Panel Below Files", checkable: true, checked: st.info_bottom !== false },
                { id: "terminalpanel", icon: "utilities-terminal", text: "Terminal Panel", shortcut: k("terminalpanel"), checkable: true, checked: !!st.terminal_panel },
                { id: "split", icon: "view-split-left-right", text: "Split View", shortcut: k("split"), checkable: true, checked: split, enabled: !!currentTab },
                { id: "statusbar", icon: "kt-show-statusbar", text: "Status Bar", checkable: true, checked: st.show_status_bar !== false },
                { separator: true },
                { id: "hidden", icon: "view-hidden", text: "Show Hidden Files", shortcut: k("hidden"), checkable: true, checked: !!st.show_hidden },
                { id: "zoomin", icon: "zoom-in", text: "Bigger Icons", shortcut: k("zoomin"), enabled: ok },
                { id: "zoomout", icon: "zoom-out", text: "Smaller Icons", shortcut: k("zoomout"), enabled: ok },
                { id: "viewoptions", icon: "configure", text: "Show View Options", shortcut: k("viewoptions"), enabled: ok },
                { separator: true },
                { id: "reload", icon: "view-refresh", text: "Reload", shortcut: k("reload"), enabled: ok }
            ] },
            { title: "Go", icon: "go-jump", items: [
                { id: "back", icon: "go-previous", text: "Back", shortcut: k("back"), enabled: ok && p.backStack.length > 0 },
                { id: "forward", icon: "go-next", text: "Forward", shortcut: k("forward"), enabled: ok && p.fwdStack.length > 0 },
                { id: "up", icon: "go-up", text: "Enclosing Folder", shortcut: k("up"), enabled: ok && p.parentOf(p.path) !== "" },
                { separator: true },
                { id: "recents", icon: "document-open-recent", text: "Recents", shortcut: k("recents") },
                { id: "documents", icon: "folder-documents", text: "Documents", shortcut: k("documents") },
                { id: "desktop", icon: "user-desktop", text: "Desktop", shortcut: k("desktop") },
                { id: "downloads", icon: "folder-download", text: "Downloads", shortcut: k("downloads") },
                { id: "home", icon: "user-home", text: "Home", shortcut: k("home") },
                { id: "computer", icon: "computer", text: "Computer", shortcut: k("computer") },
                { id: "trash", icon: "user-trash", text: "Trash", shortcut: k("trash") },
                { separator: true },
                { id: "gotofolder", icon: "go-jump", text: "Go to Folder…", shortcut: k("gotofolder") },
                { id: "connect", icon: "network-connect", text: "Connect to Server…", shortcut: k("connect") },
                { id: "cloud", icon: app.accountsService ? "preferences-system-users|folder-cloud" : "folder-cloud", text: app.accountsService ? "Open Settings › Accounts…" : "Add Cloud Drive…" },
                { separator: true },
                { id: "terminal", icon: "utilities-terminal", text: "Open in Terminal", shortcut: k("terminal"), enabled: ok },
                { id: "diskusage", icon: "filelight", text: "Disk Usage…", shortcut: k("diskusage") }
            ] },
            { title: "Window", icon: "window-duplicate", items: [
                { id: "minimize", icon: "window-minimize", text: "Minimize" },
                { id: "zoom", icon: "window-maximize", text: "Zoom" },
                { separator: true },
                { id: "prevtab", icon: "go-previous-view", text: "Show Previous Tab", shortcut: k("prevtab"), enabled: tabs.count > 1 },
                { id: "nexttab", icon: "go-next-view", text: "Show Next Tab", shortcut: k("nexttab"), enabled: tabs.count > 1 },
                { id: "tearoff", icon: "tab-detach", text: "Move Tab to New Window", enabled: tabs.count > 1 }
            ] },
            { title: "Help", icon: "help-browser", items: [
                { id: "help", icon: "help-contents", text: "Grove Help and Keyboard Shortcuts", shortcut: k("help") }
            ] }
        ]
    }
    function homeOr(p) { return p && p.indexOf("/") === 0 ? p : app.grove.home() }
    function menuAction(id) {
        const p = activePane
        const home = app.grove.home()
        const dot = id.indexOf(".")
        if (dot > 0) {
            const kind = id.substring(0, dot), arg = id.substring(dot + 1)
            if (!p) return
            if (kind === "sort") {
                if (arg === "descending") { p.model.applySort(p.model.sortKey, !p.model.descending); p.rememberView() }
                else if (arg === "foldersfirst") setSetting("folders_first", !p.model.foldersFirst)
                else p.sortBy(arg)
            } else if (kind === "group") p.setGroupBy(arg)
            else if (kind === "tag") p.toggleTag(arg)
            else if (kind === "openwith") {
                const t = menuTarget
                if (!t) return
                if (arg === "other") openWithDialog(t.path)
                else if (menuApps[Number(arg)]) app.grove.openWith(JSON.stringify(p.selectedPaths()), menuApps[Number(arg)].id)
            }
            return
        }
        switch (id) {
        case "about": aboutSheet.open(); break
        case "settings": showSettings(); break
        case "customizetoolbar": toolbarSheet.open(); break
        case "emptytrash": emptyTrash(); break
        case "quit": app.saveSession(); Qt.quit(); break
        case "newwindow": app.openWindow([homeOr(p && p.path)]); break
        case "newtab": newTab(homeOr(p && p.path), []); break
        case "help": showHelp(""); break
        case "minimize": bw.showMinimized(); break
        case "zoom": bw.visibility === Window.Maximized ? bw.showNormal() : bw.showMaximized(); break
        case "prevtab": tabBar.currentIndex = (tabBar.currentIndex + tabs.count - 1) % tabs.count; break
        case "nexttab": tabBar.currentIndex = (tabBar.currentIndex + 1) % tabs.count; break
        case "tearoff": tearOffTab(tabBar.currentIndex); break
        case "closetab": closeTab(tabBar.currentIndex); break
        case "closewindow": bw.close(); break
        case "sidebar": sidebarShown = !sidebarShown; break
        case "folderspanel": setPanel("folders_panel", !app.settings.folders_panel); break
        case "infopanel": toggleInfo(); break
        case "infobottom": placeInfo(app.settings.info_bottom === false); break
        case "terminalpanel": setPanel("terminal_panel", !app.settings.terminal_panel); break
        case "statusbar": setSetting("show_status_bar", app.settings.show_status_bar === false); break
        case "hidden": toggleHidden(); break
        case "recents": go("recent:///"); break
        case "documents": go(home + "/Documents"); break
        case "desktop": go(home + "/Desktop"); break
        case "downloads": go(home + "/Downloads"); break
        case "home": go(home); break
        case "computer": go("computer:///"); break
        case "trash": go("trash:///"); break
        case "gotofolder": editPath(); break
        case "connect": connectServer(); break
        case "cloud": addCloudDrive(); break
        case "diskusage": showDiskUsage(); break
        case "undo": app.grove.undo(); break
        case "redo": app.grove.redo(); break
        default:
            if (!p) return
            switch (id) {
            case "newfolder": p.newFolder(); break
            case "newfile": p.newFile(""); break
            case "open": p.openSelected(false); break
            case "opentab": p.openSelected(true); break
            case "quicklook": p.quickLook(); break
            case "getinfo": { const s = p.selectedPaths(); showInfo(s.length ? s : [p.path]) } break
            case "rename": p.rename(); break
            case "duplicate": p.duplicate(); break
            case "alias": p.makeAlias(); break
            case "compress": p.compress("zip"); break
            case "extract": p.extract(); break
            case "extractto": p.extractTo(); break
            case "burn": burnSelection(); break
            case "share": share(p.selectedPaths()); break
            case "putback": p.putBack(); break
            case "movetotrash": p.trashSelected(); break
            case "delete": p.deleteSelected(); break
            case "eject": ejectCurrent(); break
            case "cut": p.copy(true); break
            case "copy": p.copy(false); break
            case "paste": p.paste(false); break
            case "movehere": p.paste(true); break
            case "copypath": p.copyPath(); break
            case "copyother": toOtherPane(false); break
            case "moveother": toOtherPane(true); break
            case "selectall": p.model.selectAll(); break
            case "invert": p.model.invertSelection(); break
            case "selectionmode": toggleSelectionMode(); break
            case "batchrename": { const s = p.selectedPaths(); if (s.length) batchRename(s) } break
            case "search": focusSearch(); break
            case "filter": openFilter(); break
            case "viewicons": p.setView("icons"); break
            case "viewlist": p.setView("list"); break
            case "viewcolumns": p.setView("columns"); break
            case "viewgallery": p.setView("gallery"); break
            case "stacks": toggleStacks(); break
            case "split": currentTab.toggleSplit(); break
            case "zoomin": p.zoom(1); break
            case "zoomout": p.zoom(-1); break
            case "viewoptions": toggleViewOptions(); break
            case "reload": p.reload(); break
            case "back": p.goBack(); break
            case "forward": p.goForward(); break
            case "up": p.goUp(); break
            case "terminal": app.grove.openTerminal(p.path); break
            }
        }
    }
    GlobalMenuBar { id: globalMenu; window: bw; model: bw.menuModel; onTriggered: id => bw.menuAction(id) }
    onVisibleChanged: if (visible) Qt.callLater(globalMenu.attach)
    // The same menus in the window, from the toolbar's action button.
    AppMenu {
        id: appMenu
        model: bw.menuModel
        onTriggered: id => bw.menuAction(id)
        accent: Theme.accent
        dark: Theme.dark
        // In the window: a separate popup window gets a different scale
        // from arbor at 4/3 and loses pixels.
        popupType: Popup.Item
        property real closedAt: 0
        onClosed: closedAt = Date.now()
    }
    function toggleAppMenu(button) {
        if (appMenu.visible || Date.now() - appMenu.closedAt < 250) appMenu.close()
        else appMenu.openBelow(button)
    }
    Sheet {
        id: aboutSheet
        title: "Grove"
        message: "Version " + Qt.application.version + "\nA Finder-style file manager with Dolphin's power."
        GLabel {
            width: parent.width
            text: "github.com/benwbooth/grove"
            color: Theme.accent
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: Qt.openUrlExternally("https://github.com/benwbooth/grove") }
        }
        buttons: [ PushButton { text: "OK"; primary: true; focus: true; onClicked: aboutSheet.close() } ]
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
    CloudDriveSheet { id: cloudSheet; app: bw.app; win: bw }
    GoogleSheet { id: googleSheet; app: bw.app; win: bw }
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
        onOpened: { smartName.text = pane ? (pane.searchText || pane.searchTag || "Smart Folder") : ""; smartName.forceActiveFocus(); smartName.selectAll() }
        // What it will look for (the search text and the chips).
        GLabel {
            width: parent.width
            wrapMode: Text.Wrap; elide: Text.ElideNone
            color: Theme.fgSecondary
            font.pixelSize: Theme.small
            text: {
                const p = saveSheet.pane
                if (!p) return ""
                const kinds = { folder: "folders", image: "images", video: "movies", audio: "music", document: "documents", archive: "archives" }
                const parts = []
                if (p.searchText) parts.push((p.searchContents ? "containing “" : "named “") + p.searchText + "”")
                if (p.searchKind) parts.push("only " + kinds[p.searchKind])
                if (p.searchDays) parts.push("modified in the last " + (p.searchDays === 1 ? "day" : p.searchDays + " days"))
                if (p.searchTag) parts.push("tagged " + p.searchTag)
                return "Finds items " + (parts.join(", ") || "of any kind") + " in “" + p.titleFor(p.searchRoot) + "”."
            }
        }
        GField { id: smartName; width: parent.width; placeholderText: "Name"; Keys.onReturnPressed: saveBtn.clicked() }
        buttons: [
            PushButton { text: "Cancel"; onClicked: saveSheet.close() },
            PushButton {
                id: saveBtn
                text: "Save"; primary: true; enabled: smartName.text !== ""
                onClicked: {
                    const p = saveSheet.pane
                    const q = p.searchQuery()
                    delete q.respect_ignore
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
    function burn(paths) { if (paths.length) burnSheet.start(paths) }
    function burnSelection() {
        const s = activePane.selectedPaths()
        if (s.length) burn(s)
        else if (!activePane.special) burn([activePane.path])
        else toast("info", "Select something to burn first.")
    }
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
    Key { act: "computer"; onActivated: bw.go("computer:///") }
    Key { act: "trash"; onActivated: bw.go("trash:///") }
    Key { act: "connect"; onActivated: bw.connectServer() }
    Key { act: "diskusage"; onActivated: bw.showDiskUsage() }
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
    Key { act: "infopanel"; onActivated: bw.toggleInfo() }
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
    Timer {
        id: frameGrab
        property int left: 0
        property int n: 0
        interval: 16; repeat: true
        onTriggered: {
            if (left <= 0) { stop(); return }
            left--
            bw.app.grove.grabWindows(bw.app.screenshot.replace(".png", "-f" + ("0" + n).slice(-2) + ".png"))
            n++
        }
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
        else if (cmd === "size") { const [w, h] = arg.split("x").map(Number); bw.width = w; if (h) bw.height = h }
        else if (cmd === "wheel") { const v = p.viewLoaderItem(); if (v && v.testWheel) v.testWheel(Number(arg)) }
        else if (cmd === "pxwheel") { const v = p.viewLoaderItem(); if (v && v.testPixelWheel) v.testPixelWheel(Number(arg)) }
        else if (cmd === "swipe") { const v = p.viewLoaderItem(); if (v && v.testSwipe) v.testSwipe(Number(arg)) }
        else if (cmd === "du") bw.showDiskUsage(arg || p.path)
        else if (cmd === "cloud") bw.addCloudDrive()
        else if (cmd === "cloudpick") { const f = cloudSheet.providers.find(x => x.name === arg); if (f) cloudSheet.pick(f) }
        else if (cmd === "cloudnext") cloudSheet.page === "name" ? cloudSheet.begin() : cloudSheet.send(arg || "")
        else if (cmd === "cloudquit") cloudSheet.abandon()
        else if (cmd === "clouddefaults") cloudSheet.send(cloudSheet.answer, { defaults: true })
        else if (cmd === "cloudwait") { const f = cloudSheet.providers.find(x => x.name === arg); if (f) { cloudSheet.provider = f; cloudSheet.page = "ask"; cloudSheet.busy = true; cloudSheet.waitingLong = true } }
        else if (cmd === "clouddump") console.log("grove: cloud " + JSON.stringify({ page: cloudSheet.page, busy: cloudSheet.busy, error: cloudSheet.error, optName: cloudSheet.opt.Name, help0: cloudSheet.helpLines[0], guide: cloudSheet.guide, ex: cloudSheet.examples.length, step: cloudSheet.step }).slice(0, 600))
        else if (cmd === "openrow") { console.warn("grove test item", p.model.get(Number(arg))); p.openRow(Number(arg), false) }
        else if (cmd === "dumpx") { const v = p.viewLoaderItem(); console.warn("grove test x", arg || "", v && v.stripX !== undefined ? v.stripX.toFixed(1) : "?") }
        else if (cmd === "media") console.warn("grove test media", arg || "", ql.mediaState())
        // frames=N: grab the window every 16 ms, N times (shot-f00.png...),
        // to see every intermediate state of what the next steps do.
        else if (cmd === "frames") { frameGrab.left = Number(arg) || 30; frameGrab.n = 0; frameGrab.start() }
        else if (cmd === "dumppath") console.warn("grove test path", p.path, p.viewMode)
        else if (cmd === "select") p.model.selectPaths(JSON.stringify(arg.split(",")))
        else if (cmd === "previewdrag") p.previewDrag(Number(arg))
        else if (cmd === "extract") p.extract()
        else if (cmd === "viewscroll") { const v = p.viewLoaderItem(); if (v && v.restoreScroll) v.restoreScroll({ y: Number(arg), x: Number(arg) }) }
        else if (cmd === "previewscroll") { const pv = p.previewViewer(); if (pv && pv.item) pv.restoreView({ path: pv.item.path, doc: { y: Number(arg) }, pdf: { y: Number(arg) } }) }
        else if (cmd === "savesession") app.saveSession()
        else if (cmd === "googlesignedin") app.googleSignIns++
        // The open dialog as the portal shows it (no answer goes out: no id).
        else if (cmd === "chooser") { const c = app.openChooser({ mode: "open", title: "Open file", id: "" }); Qt.callLater(() => console.log("grove: chooser " + c.width + "x" + c.height)) }
        else if (cmd === "forget") Async.call(app.grove, k => app.grove.forgetServer(k, arg), r => console.warn("grove test forget", JSON.stringify(r)))
        else if (cmd === "accountsui") openAccounts(arg || "", () => console.warn("grove test accounts fallback"))
        else if (cmd === "accountsdump") console.warn("grove test accounts", app.accountsService, JSON.stringify((sidebar.places.servers || []).map(s => [s.name, s.kind, s.account || ""])))
        else if (cmd === "google") { googleSheet.start(); if (arg) googleSheet.step = Number(arg) }
        else if (cmd === "previewzoom") { const pv = p.previewViewer(); if (pv) pv.testZoom(Number(arg)) }
        else if (cmd === "infodump") Async.call(app.grove, k => app.grove.info(k, arg), r => console.warn("grove test info", JSON.stringify(r)))
        else if (cmd === "sort") { const [k, d] = arg.split(","); p.model.applySort(k, d === "desc"); p.rememberView() }
        else if (cmd === "dumpstate") console.warn("grove test state", JSON.stringify(bw.windowState()))
        else if (cmd === "makeiso") app.grove.burn(JSON.stringify({ paths: [p.path], iso: arg }))
        else if (cmd === "compress") p.compress(arg || "zip")
        else if (cmd === "colscroll") { const v = p.viewLoaderItem(); if (v && v.testScroll) v.testScroll(Number(arg)) }
        else if (cmd === "dumpcols") { const v = p.viewLoaderItem(); if (v && v.testDump) v.testDump() }
        else if (cmd === "placeinfo") { bw.placeInfo(arg === "bottom"); console.warn("grove test placeinfo", arg, JSON.stringify({ b: app.settings.info_bottom, p: app.settings.info_panel, lp: app.settings.list_preview, view: bottomPreviewView })) }
        else if (cmd === "expandrow") p.model.toggleExpanded(Number(arg))
        else if (cmd === "selectrow") p.model.select(Number(arg), "set")
        else if (cmd === "split") currentTab.toggleSplit()
        else if (cmd === "tab") newTab(arg, [])
        else if (cmd === "ql") quickLook(p)
        // A viewer's test hook (DocView, DataTable): "qlview=end", "qlview=sort:1:1"...
        else if (cmd === "qlview") {
            const find = (it) => { if (!it || !it.visible) return null; if (typeof it.testAction === "function") return it; for (let i = 0; i < it.children.length; i++) { const f = find(it.children[i]); if (f) return f } return null }
            const v = find(ql.visible ? ql.contentItem : bw.contentItem)
            if (v) v.testAction(arg); else console.warn("grove test qlview: no viewer")
        }
        else if (cmd === "info") showInfo([arg])
        else if (cmd === "filter") { openFilter(); p.model.applyFilter(arg) }
        else if (cmd === "search") { if (search) search.text = arg; p.startSearch(arg) }
        else if (cmd === "skind") p.setSearchFilter("searchKind", arg)
        else if (cmd === "sdays") p.setSearchFilter("searchDays", Number(arg))
        else if (cmd === "stag") p.setSearchFilter("searchTag", arg)
        else if (cmd === "savesearch") saveSmartFolder(p)
        else if (cmd === "menu") p.showItemMenu(Number(arg), 100, 100)
        else if (cmd === "bgmenu") p.showBackgroundMenu()
        else if (cmd === "actions") { if (actionButton) appMenu.openBelow(actionButton); else appMenu.openAt(root, root.width - appMenu.implicitWidth - 12, Theme.toolbarHeight) }
        // "appmenu=View/Sort By": the action button's menus, those submenus open.
        else if (cmd === "appmenu") {
            if (!appMenu.visible) testStep("actions")
            let m = appMenu
            for (const title of (arg || "").split("/").filter(t => t)) {
                for (let i = 0; i < m.count; i++) {
                    const it = m.itemAt(i)
                    if (it && it.subMenu && it.text === title) { m.currentIndex = i; it.subMenu.popup(it, it.width - 3, -Theme.menuPad); m = it.subMenu; break }
                }
            }
        }
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
        else if (cmd === "burn") burn(arg ? arg.split(",") : [p.path])
        else if (cmd === "burngo") burnSheet.goImage()
        else if (cmd === "hideplace") setSetting("hidden_places", arg.split(","))
        else if (cmd === "showhiddenplaces") sidebar.showHiddenPlaces = true
        else if (cmd === "columns") setSetting("list_columns", arg.split(","))
        else if (cmd === "tip") { setSetting("preview_tooltips", true); p.testTip = Number(arg) }
        else if (cmd === "recordkey") settingsWin.recordSeq(arg)
        else if (cmd === "pad") { const v = arg.split(",").map(Number); app.grove.testTouchpad(v[0], v[1], v[2], v[3], v[4], v[5], v[6] || 0) }
        else if (cmd === "realwheelx") { const [x, y, a] = arg.split(",").map(Number); app.grove.testWheelX(x, y, a) }
        else if (cmd === "realwheel") { const [x, y, a, px] = arg.split(",").map(Number); app.grove.testWheel(x, y, a, px || 0) }
        else if (cmd === "key") { const [k, m] = arg.split(":"); app.grove.testKey(Number(k), Number(m || 0), "") }
        else if (cmd === "settings") { if (arg) settingsWin.page = arg; showSettings() }
        else if (cmd === "help") showHelp(arg || "")
        else if (cmd === "connect") connectServer()
        else if (cmd === "connectkind") connectSheet.kind = arg
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
