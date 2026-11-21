import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window
import QtQuick.Layouts

// A grove window: the glass sidebar, the toolbar, tabs (each one or two
// panes), the status bar with the zoom slider, and the dialogs.
ApplicationWindow {
    id: win
    property var app
    property var initialPaths: []
    property var initialSelect: []
    readonly property bool clientDecorations: app.settings.client_decorations !== false
    readonly property bool chooserMode: false
    property bool sidebarShown: true
    property int sidebarWidth: app.settings.sidebar_width > 0 ? app.settings.sidebar_width : Math.round(200 * Theme.pt)
    property var currentTab: tabRepeater.count > 0 ? tabRepeater.itemAt(tabBar.currentIndex) : null
    property var activePane: currentTab ? currentTab.activePane : null
    property bool modalOpen: conflict.opened || confirmSheet.opened || renameSheet.opened || connectSheet.opened || saveSheet.opened || openWithSheet.opened

    width: 1180
    height: 720
    minimumWidth: 560
    minimumHeight: 360
    visible: false
    color: Theme.window
    flags: clientDecorations ? (Qt.Window | Qt.FramelessWindowHint) : Qt.Window
    title: activePane ? activePane.title + " — Grove" : "Grove"

    onActiveChanged: if (active) app.activeWindow = win

    ListModel { id: tabs }

    function newTab(path, select) {
        tabs.append({ path: path || app.grove.home(), select: JSON.stringify(select || []) })
        tabBar.currentIndex = tabs.count - 1
    }
    function closeTab(i) {
        if (tabs.count <= 1) { win.close(); return }
        const t = tabRepeater.itemAt(i)
        if (t) app.closedTabs.push(t.activePane.path)
        tabs.remove(i)
        tabBar.currentIndex = Math.min(tabBar.currentIndex, tabs.count - 1)
    }
    function tabAt(i) { return tabRepeater.itemAt(i) }
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
    function showSettings() { settingsWin.show(); settingsWin.raise(); settingsWin.requestActivate() }
    function showHelp(search) { helpWin.query = search || ""; helpWin.show(); helpWin.raise(); helpWin.requestActivate() }

    Component { id: infoComp; InfoWindow {} }

    // ---------- layout ----------
    Item {
        id: root
        anchors.fill: parent
        // Text on exact half pixels smears at 4/3 scale.
        transform: Translate { x: 0.001; y: 0.001 }

        Sidebar {
            id: sidebar
            win: win
            visible: win.sidebarShown
            x: 8; y: 8
            width: win.sidebarWidth
            height: parent.height - 16
            currentPath: win.activePane ? win.activePane.path : ""
        }
        MouseArea {
            visible: win.sidebarShown
            x: sidebar.x + sidebar.width
            width: 8
            height: parent.height
            cursorShape: Qt.SplitHCursor
            property real sx: 0
            property real sw: 0
            onPressed: (m) => { sx = mapToItem(root, m.x, 0).x; sw = win.sidebarWidth }
            onPositionChanged: (m) => { if (pressed) win.sidebarWidth = Math.max(150, Math.min(400, sw + mapToItem(root, m.x, 0).x - sx)) }
            onReleased: win.setSetting("sidebar_width", Math.round(win.sidebarWidth))
        }

        Item {
            id: main
            x: win.sidebarShown ? sidebar.x + sidebar.width + 8 : 0
            width: parent.width - x
            height: parent.height

            // ---------- toolbar ----------
            Item {
                id: toolbar
                width: parent.width
                height: Theme.toolbarHeight
                MouseArea {
                    anchors.fill: parent
                    onPressed: win.startSystemMove()
                    onDoubleClicked: win.visibility === Window.Maximized ? win.showNormal() : win.showMaximized()
                }
                Row {
                    id: leftTools
                    x: win.sidebarShown ? 4 : (win.clientDecorations ? 12 : 8)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8
                    GButton {
                        visible: !win.sidebarShown && win.clientDecorations
                        iconName: "window-close"; iconSize: 12
                        tip: "Close window"
                        onClicked: win.close()
                    }
                    Item {
                        width: navGroup.implicitWidth + 4
                        height: Theme.controlHeight
                        GlassPanel { anchors.fill: parent; radius: height / 2; shadow: false; tint: Theme.field }
                        Row {
                            id: navGroup
                            anchors.centerIn: parent
                            GButton { iconName: "go-previous"; capsule: true; enabled: win.activePane && win.activePane.backStack.length > 0; tip: Help.tip("Back"); onClicked: win.activePane.goBack() }
                            GButton { iconName: "go-next"; capsule: true; enabled: win.activePane && win.activePane.fwdStack.length > 0; tip: Help.tip("Forward"); onClicked: win.activePane.goForward() }
                            GButton { iconName: "go-up"; capsule: true; enabled: win.activePane && win.activePane.parentOf(win.activePane.path) !== ""; tip: Help.tip("Enclosing folder"); onClicked: win.activePane.goUp() }
                        }
                    }
                    GButton {
                        iconName: "sidebar-show-symbolic|view-split-left-right|sidebar-expand-left"
                        tip: Help.tip("Show or hide the sidebar", "Sidebar")
                        onClicked: win.sidebarShown = !win.sidebarShown
                    }
                }
                PathBar {
                    id: pathBar
                    pane: win.activePane
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
                        current: win.activePane ? win.activePane.viewMode : ""
                        onPicked: (v) => win.activePane.setView(v)
                    }
                    GButton {
                        iconName: "view-split-left-right"
                        checked: win.currentTab && win.currentTab.split
                        tip: Help.tip("Split view: two folders side by side", "Split view")
                        onClicked: win.currentTab.toggleSplit()
                    }
                    GButton {
                        id: actionButton
                        iconName: "application-menu|open-menu-symbolic|overflow-menu"
                        tip: "More actions: new folder, sort, show hidden files, settings, help"
                        onClicked: actionMenu.popup(actionButton, 0, actionButton.height + 4)
                    }
                    JobsButton { app: win.app; win: win }
                    GField {
                        id: search
                        width: Math.round(190 * Theme.pt)
                        iconName: "edit-find"
                        clearable: true
                        placeholderText: "Search"
                        tip: Help.tip("Search names below this folder (choose Contents in the bar to search inside files)", "Search")
                        onTextEdited: win.activePane.startSearch(text)
                        onTextChanged: if (text === "" && win.activePane && win.activePane.searching) win.activePane.endSearch(true)
                        Keys.onEscapePressed: { text = ""; win.activePane.forceActiveFocus() }
                        Keys.onDownPressed: win.activePane.forceActiveFocus()
                        Keys.onReturnPressed: win.activePane.forceActiveFocus()
                        Connections {
                            target: win.activePane
                            function onSearchingChanged() { if (!win.activePane.searching && search.text !== "") search.text = "" }
                        }
                    }
                }
            }

            // ---------- tabs ----------
            TabStrip {
                id: tabBar
                win: win
                model: tabs
                anchors.top: toolbar.bottom
                width: parent.width
                height: tabs.count > 1 ? Math.round(30 * Theme.pt) : 0
                visible: tabs.count > 1
            }

            StackLayout {
                id: stack
                anchors.top: tabBar.bottom
                anchors.bottom: statusBar.top
                width: parent.width
                currentIndex: tabBar.currentIndex
                Repeater {
                    id: tabRepeater
                    model: tabs
                    TabPage {
                        required property string path
                        required property string select
                        required property int index
                        win: win
                        initialPath: path
                        initialSelect: JSON.parse(select)
                    }
                }
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
                    text: win.activePane ? win.activePane.model.status : ""
                    font.pixelSize: Theme.small
                    color: Theme.fgSecondary
                }
                Row {
                    id: zoomRow
                    anchors.right: parent.right
                    anchors.rightMargin: 14
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 6
                    visible: win.activePane && win.activePane.viewMode === "icons"
                    GIcon { name: "zoom-out"; size: 12; anchors.verticalCenter: parent.verticalCenter; opacity: 0.6 }
                    Slider {
                        id: zoom
                        width: 110
                        height: 18
                        from: 0; to: 8; stepSize: 1
                        snapMode: Slider.SnapAlways
                        readonly property var sizes: [16, 24, 32, 48, 64, 96, 128, 192, 256]
                        value: win.activePane ? Math.max(0, sizes.indexOf(win.activePane.iconSize)) : 4
                        onMoved: { win.activePane.iconSize = sizes[value]; win.activePane.rememberView() }
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
            model: win.clientDecorations && win.visibility !== Window.Maximized ? [
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
                onPressed: win.startSystemResize(modelData.e)
            }
        }

        Toast { id: toastBox; anchors.horizontalCenter: main.horizontalCenter; anchors.bottom: parent.bottom; anchors.bottomMargin: 40 }
    }

    // ---------- menus ----------
    GMenu {
        id: actionMenu
        GMenuItem { text: "New Window"; iconName: "window-new"; shortcutText: "Ctrl+N"; onTriggered: win.app.openWindow([win.activePane.path.indexOf("/") === 0 ? win.activePane.path : win.app.grove.home()]) }
        GMenuItem { text: "New Tab"; iconName: "tab-new"; shortcutText: "Ctrl+T"; onTriggered: win.newTab(win.activePane.path.indexOf("/") === 0 ? win.activePane.path : win.app.grove.home(), []) }
        GMenuItem { text: "New Folder"; iconName: "folder-new"; shortcutText: "Ctrl+Shift+N"; onTriggered: win.activePane.newFolder() }
        GMenuSeparator {}
        GMenuItem { text: win.app.grove.undoLabel ? "Undo " + win.app.grove.undoLabel : "Undo"; iconName: "edit-undo"; shortcutText: "Ctrl+Z"; enabled: win.app.grove.undoLabel !== ""; onTriggered: win.app.grove.undo() }
        GMenuItem { text: win.app.grove.redoLabel ? "Redo " + win.app.grove.redoLabel : "Redo"; iconName: "edit-redo"; shortcutText: "Ctrl+Shift+Z"; enabled: win.app.grove.redoLabel !== ""; onTriggered: win.app.grove.redo() }
        GMenuSeparator {}
        GMenuItem { text: "Filter This Folder"; iconName: "view-filter"; shortcutText: "Ctrl+I"; onTriggered: win.openFilter() }
        GMenuItem { text: "Show Hidden Files"; checkable: true; checked: !!win.app.settings.show_hidden; shortcutText: "Ctrl+H"; onTriggered: win.toggleHidden() }
        GMenuItem { text: "Split View"; checkable: true; checked: win.currentTab && win.currentTab.split; shortcutText: "F3"; onTriggered: win.currentTab.toggleSplit() }
        GMenuItem { text: "Open in Terminal"; iconName: "utilities-terminal"; shortcutText: "Ctrl+Alt+T"; onTriggered: win.app.grove.openTerminal(win.activePane.path) }
        GMenuItem { text: "Connect to Server…"; iconName: "network-connect"; shortcutText: "Ctrl+K"; onTriggered: win.connectServer() }
        GMenuSeparator {}
        GMenuItem { text: "Empty Trash…"; iconName: "trash-empty"; enabled: win.app.trashCount > 0; onTriggered: win.emptyTrash() }
        GMenuSeparator {}
        GMenuItem { text: "Settings…"; iconName: "configure"; shortcutText: "Ctrl+,"; onTriggered: win.showSettings() }
        GMenuItem { text: "Help and Keyboard Shortcuts"; iconName: "help-contents"; shortcutText: "F1"; onTriggered: win.showHelp("") }
    }
    function openFilter() {
        const p = activePane
        p.filterOpen = true
        filterFocus.restart()
    }
    Timer { id: filterFocus; interval: 30; onTriggered: { const f = win.findFilterField(win.activePane); if (f) f.forceActiveFocus() } }
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
    ConflictDialog { id: conflict; app: win.app }
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
    BatchRename { id: renameSheet; app: win.app; win: win }
    ConnectServer { id: connectSheet; app: win.app; win: win }
    OpenWithSheet { id: openWithSheet; app: win.app; win: win }
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
                    const q = { name: p.searchContents ? "" : p.searchText, content: p.searchContents ? p.searchText : "", include_hidden: !!win.app.settings.show_hidden }
                    win.app.grove.saveSmartFolder(smartName.text, p.searchRoot, JSON.stringify(q))
                    saveSheet.close()
                    win.toast("info", "Saved “" + smartName.text + "” in the sidebar")
                }
            }
        ]
    }
    QuickLook { id: ql; app: win.app; win: win }
    SettingsWindow { id: settingsWin; app: win.app }
    HelpWindow { id: helpWin; app: win.app }
    Welcome { id: welcome; app: win.app; win: win }

    // ---------- shortcuts ----------
    component Key: Shortcut { context: Qt.WindowShortcut; enabled: !win.modalOpen }
    Key { sequences: ["Alt+Left", "Ctrl+["]; onActivated: win.activePane.goBack() }
    Key { sequences: ["Alt+Right", "Ctrl+]"]; onActivated: win.activePane.goForward() }
    Key { sequences: ["Alt+Up", "Ctrl+Up"]; onActivated: win.activePane.goUp() }
    Key { sequences: ["Ctrl+Down"]; onActivated: win.activePane.openSelected(false) }
    Key { sequences: ["Ctrl+L", "Ctrl+Shift+G", "F6"]; onActivated: pathBar.edit() }
    Key { sequences: ["Ctrl+Shift+H", "Alt+Home"]; onActivated: win.go(win.app.grove.home()) }
    Key { sequences: ["Ctrl+Shift+D"]; onActivated: win.go(win.app.grove.home() + "/Desktop") }
    Key { sequences: ["Ctrl+Shift+O"]; onActivated: win.go(win.app.grove.home() + "/Documents") }
    Key { sequences: ["Ctrl+Alt+L"]; onActivated: win.go(win.app.grove.home() + "/Downloads") }
    Key { sequences: ["Ctrl+Shift+F"]; onActivated: win.go("recent:///") }
    Key { sequences: ["Ctrl+Shift+C"]; onActivated: win.go("/") }
    Key { sequences: ["Ctrl+Shift+T"]; onActivated: win.go("trash:///") }
    Key { sequences: ["Ctrl+K"]; onActivated: win.connectServer() }
    Key { sequences: ["F5", "Ctrl+R"]; onActivated: win.activePane.reload() }
    Key { sequences: ["Ctrl+1"]; onActivated: win.activePane.setView("icons") }
    Key { sequences: ["Ctrl+2"]; onActivated: win.activePane.setView("list") }
    Key { sequences: ["Ctrl+3"]; onActivated: win.activePane.setView("columns") }
    Key { sequences: ["Ctrl+4"]; onActivated: win.activePane.setView("gallery") }
    Key { sequences: ["Ctrl++", "Ctrl+="]; onActivated: win.activePane.zoom(1) }
    Key { sequences: ["Ctrl+-"]; onActivated: win.activePane.zoom(-1) }
    Key { sequences: ["Ctrl+H", "Ctrl+Shift+."]; onActivated: win.toggleHidden() }
    Key { sequences: ["Ctrl+I"]; onActivated: win.openFilter() }
    Key { sequences: ["Ctrl+Y"]; onActivated: win.activePane.quickLook() }
    Key { sequences: ["F3"]; onActivated: win.currentTab.toggleSplit() }
    Key { sequences: ["Ctrl+Alt+S", "F9"]; onActivated: win.sidebarShown = !win.sidebarShown }
    Key { sequences: ["Ctrl+N"]; onActivated: win.app.openWindow([win.activePane.path.indexOf("/") === 0 ? win.activePane.path : win.app.grove.home()]) }
    Key { sequences: ["Ctrl+T"]; onActivated: win.newTab(win.activePane.path.indexOf("/") === 0 ? win.activePane.path : win.app.grove.home(), []) }
    Key { sequences: ["Ctrl+W"]; onActivated: win.closeTab(tabBar.currentIndex) }
    Key { sequences: ["Ctrl+Tab"]; onActivated: tabBar.currentIndex = (tabBar.currentIndex + 1) % tabs.count }
    Key { sequences: ["Ctrl+Shift+Tab", "Ctrl+Backtab"]; onActivated: tabBar.currentIndex = (tabBar.currentIndex + tabs.count - 1) % tabs.count }
    Key { sequences: ["Ctrl+C"]; onActivated: win.activePane.copy(false) }
    Key { sequences: ["Ctrl+X"]; onActivated: win.activePane.copy(true) }
    Key { sequences: ["Ctrl+V"]; onActivated: win.activePane.paste(false) }
    Key { sequences: ["Ctrl+Alt+V"]; onActivated: win.activePane.paste(true) }
    Key { sequences: ["Ctrl+Alt+C"]; onActivated: win.activePane.copyPath() }
    Key { sequences: ["Ctrl+D"]; onActivated: win.activePane.duplicate() }
    Key { sequences: ["Ctrl+Alt+A"]; onActivated: win.activePane.makeAlias() }
    Key { sequences: ["F2"]; onActivated: win.activePane.rename() }
    Key { sequences: ["Shift+F2"]; onActivated: { const s = win.activePane.selectedPaths(); if (s.length) win.batchRename(s) } }
    Key { sequences: ["Ctrl+Shift+N", "F10"]; onActivated: win.activePane.newFolder() }
    Key { sequences: ["Ctrl+Alt+N"]; onActivated: win.activePane.newFile("") }
    Key { sequences: ["Delete", "Ctrl+Backspace"]; onActivated: win.activePane.trashSelected() }
    Key { sequences: ["Shift+Delete"]; onActivated: win.activePane.deleteSelected() }
    Key { sequences: ["Ctrl+Z"]; onActivated: win.app.grove.undo() }
    Key { sequences: ["Ctrl+Shift+Z"]; onActivated: win.app.grove.redo() }
    Key { sequences: ["Ctrl+A"]; onActivated: win.activePane.model.selectAll() }
    Key { sequences: ["Ctrl+Shift+A"]; onActivated: win.activePane.model.invertSelection() }
    Key { sequences: ["Alt+Return", "Ctrl+Shift+I"]; onActivated: { const s = win.activePane.selectedPaths(); win.showInfo(s.length ? s : [win.activePane.path]) } }
    Key { sequences: ["Ctrl+F"]; onActivated: { search.forceActiveFocus(); search.selectAll() } }
    Key { sequences: ["Ctrl+Alt+T"]; onActivated: win.app.grove.openTerminal(win.activePane.path) }
    Key { sequences: ["Ctrl+E"]; onActivated: { const p = win.activePane.path; for (const l of (sidebar.places.locations || []).concat(sidebar.places.network || [])) if (l.ejectable && (p === l.path || p.indexOf(l.path + "/") === 0)) win.eject(l.path, l.name) } }
    Key { sequences: ["Ctrl+,"]; onActivated: win.showSettings() }
    Key { sequences: ["F1", "Ctrl+?", "Ctrl+Shift+/"]; onActivated: win.showHelp("") }
    Key { sequences: ["Ctrl+Alt+1"]; onActivated: win.activePane.toggleTag("Red") }
    Key { sequences: ["Ctrl+Alt+2"]; onActivated: win.activePane.toggleTag("Orange") }
    Key { sequences: ["Ctrl+Alt+3"]; onActivated: win.activePane.toggleTag("Yellow") }
    Key { sequences: ["Ctrl+Alt+4"]; onActivated: win.activePane.toggleTag("Green") }
    Key { sequences: ["Ctrl+Alt+5"]; onActivated: win.activePane.toggleTag("Blue") }
    Key { sequences: ["Ctrl+Alt+6"]; onActivated: win.activePane.toggleTag("Purple") }
    Key { sequences: ["Ctrl+Alt+7"]; onActivated: win.activePane.toggleTag("Gray") }

    Component.onCompleted: {
        const paths = initialPaths.length ? initialPaths : [app.grove.home()]
        for (const p of paths) tabs.append({ path: p, select: JSON.stringify(initialSelect) })
        if (!app.settings.welcomed && app.screenshot === "") welcome.open()
        if (app.screenshot !== "") shotTimer.start()
    }

    // Test hook: --screenshot FILE grabs the window and quits.
    Timer {
        id: shotTimer
        interval: Number(win.app.grove.env("GROVE_SHOT_DELAY") || 2500)
        onTriggered: {
            const steps = (win.app.grove.env("GROVE_TEST") || "").split(";").filter(s => s)
            for (const s of steps) win.testStep(s)
            shotTimer2.start()
        }
    }
    Timer {
        id: shotTimer2
        interval: Number(win.app.grove.env("GROVE_SHOT_DELAY2") || 1500)
        onTriggered: root.grabToImage(r => { r.saveToFile(win.app.screenshot); console.log("grove: screenshot " + win.app.screenshot + " (" + win.activePane.model.lastLoadMs() + " ms listing, " + win.activePane.model.count + " items)"); Qt.quit() })
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
