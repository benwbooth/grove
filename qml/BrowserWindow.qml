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
    readonly property bool clientDecorations: app.settings.client_decorations !== false
    readonly property bool chooserMode: false
    property bool sidebarShown: true
    property int sidebarWidth: app.settings.sidebar_width > 0 ? app.settings.sidebar_width : Math.round(200 * Theme.pt)
    property var currentTab: tabRepeater.count > 0 ? tabRepeater.itemAt(tabBar.currentIndex) : null
    property var activePane: currentTab ? currentTab.activePane : null
    property bool modalOpen: commitSheet.opened || versionsSheet.opened || conflict.opened || confirmSheet.opened || renameSheet.opened || connectSheet.opened || saveSheet.opened || openWithSheet.opened

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
        tabs.append({ path: path || app.grove.home(), select: JSON.stringify(select || []) })
        tabBar.currentIndex = tabs.count - 1
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
                    onPressed: bw.startSystemMove()
                    onDoubleClicked: bw.visibility === Window.Maximized ? bw.showNormal() : bw.showMaximized()
                }
                Row {
                    id: leftTools
                    x: bw.sidebarShown ? 4 : (bw.clientDecorations ? 12 : 8)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8
                    GButton {
                        visible: !bw.sidebarShown && bw.clientDecorations
                        iconName: "window-close"; iconSize: 12
                        tip: "Close window"
                        onClicked: bw.close()
                    }
                    Item {
                        width: navGroup.implicitWidth + 4
                        height: Theme.controlHeight
                        GlassPanel { anchors.fill: parent; radius: height / 2; shadow: false; tint: Theme.field }
                        Row {
                            id: navGroup
                            anchors.centerIn: parent
                            GButton { iconName: "go-previous"; capsule: true; enabled: bw.activePane && bw.activePane.backStack.length > 0; tip: Help.tip("Back"); onClicked: bw.activePane.goBack() }
                            GButton { iconName: "go-next"; capsule: true; enabled: bw.activePane && bw.activePane.fwdStack.length > 0; tip: Help.tip("Forward"); onClicked: bw.activePane.goForward() }
                            GButton { iconName: "go-up"; capsule: true; enabled: bw.activePane && bw.activePane.parentOf(bw.activePane.path) !== ""; tip: Help.tip("Enclosing folder"); onClicked: bw.activePane.goUp() }
                        }
                    }
                    GButton {
                        iconName: "sidebar-show-symbolic|view-split-left-right|sidebar-expand-left"
                        tip: Help.tip("Show or hide the sidebar", "Sidebar")
                        onClicked: bw.sidebarShown = !bw.sidebarShown
                    }
                }
                PathBar {
                    id: pathBar
                    pane: bw.activePane
                    anchors.left: leftTools.right
                    anchors.leftMargin: 10
                    anchors.right: rightTools.left
                    anchors.rightMargin: 10
                    anchors.verticalCenter: parent.verticalCenter
                }
                Row {
                    id: rightTools
                    anchors.right: parent.right
                    anchors.rightMargin: 10
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8
                    Segmented {
                        items: [
                            { icon: "view-list-icons", tip: Help.tip("View as icons", "Icons"), value: "icons" },
                            { icon: "view-list-details", tip: Help.tip("View as a list (folders expand in place)", "List"), value: "list" },
                            { icon: "view-file-columns|view-split-left-right", tip: Help.tip("View in columns", "Columns"), value: "columns" },
                            { icon: "view-preview|view-list-images", tip: Help.tip("View as a gallery", "Gallery"), value: "gallery" }
                        ]
                        current: bw.activePane ? bw.activePane.viewMode : ""
                        onPicked: (v) => bw.activePane.setView(v)
                    }
                    GButton {
                        iconName: "view-split-left-right"
                        checked: bw.currentTab && bw.currentTab.split
                        tip: Help.tip("Split view: two folders side by side", "Split view")
                        onClicked: bw.currentTab.toggleSplit()
                    }
                    GButton {
                        id: actionButton
                        iconName: "application-menu|open-menu-symbolic|overflow-menu"
                        tip: "More actions: new folder, sort, show hidden files, settings, help"
                        onClicked: actionMenu.popup(actionButton, 0, actionButton.height + 4)
                    }
                    JobsButton { app: bw.app; win: bw }
                    GField {
                        id: search
                        width: Math.round(190 * Theme.pt)
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
                            function onSearchingChanged() { if (!bw.activePane.searching && search.text !== "") search.text = "" }
                        }
                    }
                }
            }

            // ---------- tabs ----------
            TabStrip {
                id: tabBar
                win: bw
                model: tabs
                anchors.top: toolbar.bottom
                width: parent.width
                height: tabs.count > 1 ? Math.round(30 * Theme.pt) : 0
                visible: tabs.count > 1
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
                        required property int index
                        win: bw
                        initialPath: path
                        initialSelect: JSON.parse(select)
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

        Toast { id: toastBox; anchors.horizontalCenter: main.horizontalCenter; anchors.bottom: parent.bottom; anchors.bottomMargin: 40 }
    }

    // ---------- menus ----------
    GMenu {
        id: actionMenu
        GMenuItem { text: "New Window"; iconName: "window-new"; shortcutText: "Ctrl+N"; onTriggered: bw.app.openWindow([bw.activePane.path.indexOf("/") === 0 ? bw.activePane.path : bw.app.grove.home()]) }
        GMenuItem { text: "New Tab"; iconName: "tab-new"; shortcutText: "Ctrl+T"; onTriggered: bw.newTab(bw.activePane.path.indexOf("/") === 0 ? bw.activePane.path : bw.app.grove.home(), []) }
        GMenuItem { text: "New Folder"; iconName: "folder-new"; shortcutText: "Ctrl+Shift+N"; onTriggered: bw.activePane.newFolder() }
        GMenuSeparator {}
        GMenuItem { text: bw.app.grove.undoLabel ? "Undo " + bw.app.grove.undoLabel : "Undo"; iconName: "edit-undo"; shortcutText: "Ctrl+Z"; enabled: bw.app.grove.undoLabel !== ""; onTriggered: bw.app.grove.undo() }
        GMenuItem { text: bw.app.grove.redoLabel ? "Redo " + bw.app.grove.redoLabel : "Redo"; iconName: "edit-redo"; shortcutText: "Ctrl+Shift+Z"; enabled: bw.app.grove.redoLabel !== ""; onTriggered: bw.app.grove.redo() }
        GMenuSeparator {}
        GMenuItem { text: "Filter This Folder"; iconName: "view-filter"; shortcutText: "Ctrl+I"; onTriggered: bw.openFilter() }
        GMenuItem { text: "Show Hidden Files"; checkable: true; checked: !!bw.app.settings.show_hidden; shortcutText: "Ctrl+H"; onTriggered: bw.toggleHidden() }
        GMenuItem { text: "Information Panel"; checkable: true; checked: !!bw.app.settings.info_panel; shortcutText: "F11"; onTriggered: bw.setPanel("info_panel", !bw.app.settings.info_panel) }
        GMenuItem { text: "Terminal Panel"; checkable: true; checked: !!bw.app.settings.terminal_panel; shortcutText: "F4"; onTriggered: bw.setPanel("terminal_panel", !bw.app.settings.terminal_panel) }
        GMenuItem { text: "Folders Panel"; checkable: true; checked: !!bw.app.settings.folders_panel; shortcutText: "F7"; onTriggered: bw.setPanel("folders_panel", !bw.app.settings.folders_panel) }
        GMenuItem { text: "Split View"; checkable: true; checked: bw.currentTab && bw.currentTab.split; shortcutText: "F3"; onTriggered: bw.currentTab.toggleSplit() }
        GMenuItem { text: "Open in Terminal"; iconName: "utilities-terminal"; shortcutText: "Ctrl+Alt+T"; onTriggered: bw.app.grove.openTerminal(bw.activePane.path) }
        GMenuItem { text: "Connect to Server…"; iconName: "network-connect"; shortcutText: "Ctrl+K"; onTriggered: bw.connectServer() }
        GMenuSeparator {}
        GMenuItem { text: "Empty Trash…"; iconName: "trash-empty"; enabled: bw.app.trashCount > 0; onTriggered: bw.emptyTrash() }
        GMenuSeparator {}
        GMenuItem { text: "Settings…"; iconName: "configure"; shortcutText: "Ctrl+,"; onTriggered: bw.showSettings() }
        GMenuItem { text: "Help and Keyboard Shortcuts"; iconName: "help-contents"; shortcutText: "F1"; onTriggered: bw.showHelp("") }
    }
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
    SettingsWindow { id: settingsWin; app: bw.app }
    HelpWindow { id: helpWin; app: bw.app }
    Welcome { id: welcome; app: bw.app; win: bw }

    // ---------- shortcuts ----------
    component Key: Shortcut { context: Qt.WindowShortcut; enabled: !bw.modalOpen }
    Key { sequences: ["Alt+Left", "Ctrl+["]; onActivated: bw.activePane.goBack() }
    Key { sequences: ["Alt+Right", "Ctrl+]"]; onActivated: bw.activePane.goForward() }
    Key { sequences: ["Alt+Up", "Ctrl+Up"]; onActivated: bw.activePane.goUp() }
    Key { sequences: ["Ctrl+Down"]; onActivated: bw.activePane.openSelected(false) }
    Key { sequences: ["Ctrl+L", "Ctrl+Shift+G", "F6"]; onActivated: pathBar.edit() }
    Key { sequences: ["Ctrl+Shift+H", "Alt+Home"]; onActivated: bw.go(bw.app.grove.home()) }
    Key { sequences: ["Ctrl+Shift+D"]; onActivated: bw.go(bw.app.grove.home() + "/Desktop") }
    Key { sequences: ["Ctrl+Shift+O"]; onActivated: bw.go(bw.app.grove.home() + "/Documents") }
    Key { sequences: ["Ctrl+Alt+L"]; onActivated: bw.go(bw.app.grove.home() + "/Downloads") }
    Key { sequences: ["Ctrl+Shift+F"]; onActivated: bw.go("recent:///") }
    Key { sequences: ["Ctrl+Shift+C"]; onActivated: bw.go("/") }
    Key { sequences: ["Ctrl+Shift+T"]; onActivated: bw.go("trash:///") }
    Key { sequences: ["Ctrl+K"]; onActivated: bw.connectServer() }
    Key { sequences: ["F5", "Ctrl+R"]; onActivated: bw.activePane.reload() }
    Key { sequences: ["Ctrl+1"]; onActivated: bw.activePane.setView("icons") }
    Key { sequences: ["Ctrl+2"]; onActivated: bw.activePane.setView("list") }
    Key { sequences: ["Ctrl+3"]; onActivated: bw.activePane.setView("columns") }
    Key { sequences: ["Ctrl+4"]; onActivated: bw.activePane.setView("gallery") }
    Key { sequences: ["Ctrl++", "Ctrl+="]; onActivated: bw.activePane.zoom(1) }
    Key { sequences: ["Ctrl+-"]; onActivated: bw.activePane.zoom(-1) }
    Key { sequences: ["Ctrl+H", "Ctrl+Shift+."]; onActivated: bw.toggleHidden() }
    Key { sequences: ["Ctrl+I"]; onActivated: bw.openFilter() }
    Key { sequences: ["Ctrl+Y"]; onActivated: bw.activePane.quickLook() }
    Key { sequences: ["F3"]; onActivated: bw.currentTab.toggleSplit() }
    Key { sequences: ["Ctrl+Alt+S", "F9"]; onActivated: bw.sidebarShown = !bw.sidebarShown }
    Key { sequences: ["Ctrl+N"]; onActivated: bw.app.openWindow([bw.activePane.path.indexOf("/") === 0 ? bw.activePane.path : bw.app.grove.home()]) }
    Key { sequences: ["Ctrl+T"]; onActivated: bw.newTab(bw.activePane.path.indexOf("/") === 0 ? bw.activePane.path : bw.app.grove.home(), []) }
    Key { sequences: ["Ctrl+W"]; onActivated: bw.closeTab(tabBar.currentIndex) }
    Key { sequences: ["Ctrl+Tab"]; onActivated: tabBar.currentIndex = (tabBar.currentIndex + 1) % tabs.count }
    Key { sequences: ["Ctrl+Shift+Tab", "Ctrl+Backtab"]; onActivated: tabBar.currentIndex = (tabBar.currentIndex + tabs.count - 1) % tabs.count }
    Key { sequences: ["Ctrl+C"]; onActivated: bw.activePane.copy(false) }
    Key { sequences: ["Ctrl+X"]; onActivated: bw.activePane.copy(true) }
    Key { sequences: ["Ctrl+V"]; onActivated: bw.activePane.paste(false) }
    Key { sequences: ["Ctrl+Alt+V"]; onActivated: bw.activePane.paste(true) }
    Key { sequences: ["Ctrl+Alt+C"]; onActivated: bw.activePane.copyPath() }
    Key { sequences: ["Ctrl+D"]; onActivated: bw.activePane.duplicate() }
    Key { sequences: ["Ctrl+Alt+A"]; onActivated: bw.activePane.makeAlias() }
    Key { sequences: ["F2"]; onActivated: bw.activePane.rename() }
    Key { sequences: ["Shift+F2"]; onActivated: { const s = bw.activePane.selectedPaths(); if (s.length) bw.batchRename(s) } }
    Key { sequences: ["Ctrl+Shift+N", "F10"]; onActivated: bw.activePane.newFolder() }
    Key { sequences: ["Ctrl+Alt+N"]; onActivated: bw.activePane.newFile("") }
    Key { sequences: ["Delete", "Ctrl+Backspace"]; onActivated: bw.activePane.trashSelected() }
    Key { sequences: ["Shift+Delete"]; onActivated: bw.activePane.deleteSelected() }
    Key { sequences: ["Ctrl+Z"]; onActivated: bw.app.grove.undo() }
    Key { sequences: ["Ctrl+Shift+Z"]; onActivated: bw.app.grove.redo() }
    Key { sequences: ["Ctrl+A"]; onActivated: bw.activePane.model.selectAll() }
    Key { sequences: ["Ctrl+Shift+A"]; onActivated: bw.activePane.model.invertSelection() }
    Key { sequences: ["Alt+Return", "Ctrl+Shift+I"]; onActivated: { const s = bw.activePane.selectedPaths(); bw.showInfo(s.length ? s : [bw.activePane.path]) } }
    Key { sequences: ["Ctrl+F"]; onActivated: { search.forceActiveFocus(); search.selectAll() } }
    Key { sequences: ["Ctrl+Alt+T"]; onActivated: bw.app.grove.openTerminal(bw.activePane.path) }
    Key { sequences: ["Ctrl+E"]; onActivated: { const p = bw.activePane.path; for (const l of (sidebar.places.locations || []).concat(sidebar.places.network || [])) if (l.ejectable && (p === l.path || p.indexOf(l.path + "/") === 0)) bw.eject(l.path, l.name) } }
    Key { sequences: ["Ctrl+,"]; onActivated: bw.showSettings() }
    Key { sequences: ["F4"]; onActivated: { bw.setPanel("terminal_panel", !bw.app.settings.terminal_panel); if (bw.app.settings.terminal_panel) bw.activePane.forceActiveFocus() } }
    Key { sequences: ["F7"]; onActivated: bw.setPanel("folders_panel", !bw.app.settings.folders_panel) }
    Key { sequences: ["F11", "Ctrl+Shift+P"]; onActivated: bw.setPanel("info_panel", !bw.app.settings.info_panel) }
    Key { sequences: ["Shift+F5"]; onActivated: bw.toOtherPane(false) }
    Key { sequences: ["Shift+F6"]; onActivated: bw.toOtherPane(true) }
    Key { sequences: ["F1", "Ctrl+?", "Ctrl+Shift+/"]; onActivated: bw.showHelp("") }
    Key { sequences: ["Ctrl+Alt+1"]; onActivated: bw.activePane.toggleTag("Red") }
    Key { sequences: ["Ctrl+Alt+2"]; onActivated: bw.activePane.toggleTag("Orange") }
    Key { sequences: ["Ctrl+Alt+3"]; onActivated: bw.activePane.toggleTag("Yellow") }
    Key { sequences: ["Ctrl+Alt+4"]; onActivated: bw.activePane.toggleTag("Green") }
    Key { sequences: ["Ctrl+Alt+5"]; onActivated: bw.activePane.toggleTag("Blue") }
    Key { sequences: ["Ctrl+Alt+6"]; onActivated: bw.activePane.toggleTag("Purple") }
    Key { sequences: ["Ctrl+Alt+7"]; onActivated: bw.activePane.toggleTag("Gray") }

    Component.onCompleted: {
        const paths = initialPaths.length ? initialPaths : [app.grove.home()]
        for (const p of paths) tabs.append({ path: p, select: JSON.stringify(initialSelect) })
        if (!app.settings.welcomed && app.screenshot === "") welcome.open()
        if (app.screenshot !== "") shotTimer.start()
    }

    // Test hook: --screenshot FILE grabs the window and quits.
    Timer {
        id: shotTimer
        interval: Number(bw.app.grove.env("GROVE_SHOT_DELAY") || 2500)
        onTriggered: {
            const steps = (bw.app.grove.env("GROVE_TEST") || "").split(";").filter(s => s)
            for (const s of steps) bw.testStep(s)
            shotTimer2.start()
        }
    }
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
        else if (cmd === "search") { search.text = arg; p.startSearch(arg) }
        else if (cmd === "menu") p.showItemMenu(Number(arg), 100, 100)
        else if (cmd === "bgmenu") p.showBackgroundMenu()
        else if (cmd === "actions") actionMenu.popup(actionButton, 0, actionButton.height + 4)
        else if (cmd === "settings") showSettings()
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
    }
}
