import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window

// grove as every app's open/save dialog (xdg-desktop-portal FileChooser):
// the same sidebar, views and search as a grove window, plus a name field
// (saving), file type filters and Open/Save.
ApplicationWindow {
    id: fc
    // Its own app to the window manager: an Open/Save window keeps its own
    // size and place, and grove's own window comes back where it was (with
    // grove's id, a chooser closed last became grove's remembered place, and
    // one open while grove started had grove opened in the middle).
    property string groveAppId: "grove-chooser"
    property var app
    property var request: ({})
    readonly property bool saving: request.mode === "save" || request.mode === "savefiles"
    readonly property bool dirMode: !!request.directory || request.mode === "savefiles"
    readonly property bool clientDecorations: false
    readonly property bool chooserMode: true
    property var activePane: chooserPane
    property var currentTab: null
    property bool modalOpen: false
    property bool answered: false
    // The app's extra controls (portal "choices": a checkbox, or a
    // drop-down when it has options), by id; answered with the file.
    property var choiceValues: ({})
    function setChoice(id, v) { const c = Object.assign({}, choiceValues); c[id] = v; choiceValues = c }
    function answerExtra() {
        const f = (request.filters || [])[filterBox.currentIndex]
        return JSON.stringify({ choices: choiceValues, filter: f ? f.name : "" })
    }
    // Used inside grove (a picture for View Options, a disc image...):
    // called with the chosen paths instead of answering the portal.
    property var onChosen: null
    // Its own size, every time (the last one you left it at): the window
    // manager otherwise gave it the size it remembers for grove's windows
    // (a browser window's, almost the screen's height).
    readonly property var wantSize: app && app.settings.chooser_size && app.settings.chooser_size.length === 2 ? app.settings.chooser_size : [960, 620]
    width: wantSize[0]
    height: wantSize[1]
    visible: false
    property bool sized: false
    onVisibleChanged: if (visible && !sized) sizeTimer.restart()
    // (After the window manager's first configure, which sets its size.)
    Timer {
        id: sizeTimer
        interval: 60
        onTriggered: {
            fc.sized = true
            const w = Math.min(fc.wantSize[0], Math.round(fc.screen.desktopAvailableWidth * 0.95))
            const h = Math.min(fc.wantSize[1], Math.round(fc.screen.desktopAvailableHeight * 0.9))
            if (fc.width !== w || fc.height !== h) { fc.width = w; fc.height = h }
        }
    }
    function rememberSize() {
        if (!fc.sized || !app) return
        const s = [Math.round(fc.width), Math.round(fc.height)]
        const old = app.settings.chooser_size || []
        if (old[0] !== s[0] || old[1] !== s[1]) app.grove.setSetting("chooser_size", JSON.stringify(s))
    }
    color: Theme.window
    title: request.title || (saving ? "Save" : "Open")

    // The browser-window functions the chooserPane and sidebar call.
    function go(p) { chooserPane.navigate(p); chooserPane.forceActiveFocus() }
    function newTab(p) { go(p) }
    function toast(level, msg) { toastBox.show(level, msg) }
    function showInfo(paths) {}
    function quickLook(p) {}
    function quickLookFollow(p) {}
    function batchRename(p) {}
    function saveSmartFolder(p) {}
    function openWithDialog(p) {}
    function quickAction() {}
    function connectServer() { toast("info", "Connect to the server in a Grove window first; it then appears here.") }
    function connectSaved(s) { connectServer() }
    function eject(path, name) { Async.call(app.grove, k => app.grove.eject(k, path), r => { if (r.error) toast("error", r.error) }) }
    function showHelp() {}
    function setSetting(n, v) { app.grove.setSetting(n, JSON.stringify(v)) }
    function toggleHidden() { const v = !app.settings.show_hidden; setSetting("show_hidden", v); chooserPane.model.applyShowHidden(v) }
    // Ctrl+H (or your own keys for it) shows and hides hidden files here too.
    Shortcut { sequences: Help.seqs("hidden"); context: Qt.WindowShortcut; enabled: !fc.modalOpen; onActivated: fc.toggleHidden() }
    // Ctrl+L, Ctrl+Shift+G, F6 (or your own keys for Go to Folder): type a
    // location in the path bar, as in a Grove window.
    Shortcut { sequences: Help.seqs("gotofolder"); context: Qt.WindowShortcut; enabled: !fc.modalOpen; onActivated: chooserPath.edit() }
    function confirm(t, m, a, fn) { fn() }
    function confirmDelete(p) {}
    function emptyTrash() {}
    function selectAfterJob() {}
    function toggleViewOptions() {}
    function burn(p) {}

    function accept(paths) {
        let uris = []
        if (saving && !dirMode) {
            if (!nameField.text) { toast("error", "Type a name for the file."); return }
            uris = ["file://" + encodeURI(chooserPane.path + "/" + nameField.text)]
        } else if (dirMode) {
            const sel = paths || chooserPane.selectedPaths()
            uris = (sel.length ? sel : [chooserPane.path]).map(p => "file://" + encodeURI(p))
        } else {
            const sel = paths || chooserPane.selectedPaths()
            if (!sel.length) return
            uris = sel.map(p => "file://" + encodeURI(p))
        }
        answered = true
        if (onChosen) onChosen(uris.map(u => decodeURIComponent(u.substring(7))))
        else app.grove.chooserDone(request.id, true, JSON.stringify(uris), answerExtra())
        close()
    }
    function chooserOpen(path, isDir) {
        if (isDir) go(path)
        else accept([path])
    }
    onClosing: { rememberSize(); if (!answered && !onChosen) app.grove.chooserDone(request.id, false, "[]", "{}") }

    Item {
        anchors.fill: parent
        transform: Translate { x: 0.001; y: 0.001 }
        Sidebar {
            id: sidebar
            win: fc
            x: 8; y: 8
            width: Math.round(180 * Theme.pt)
            height: parent.height - 16
            currentPath: chooserPane.path
        }
        Item {
            x: sidebar.width + 16
            width: parent.width - x
            height: parent.height
            Row {
                id: tools
                x: 0; y: 10
                spacing: 8
                GButton { iconName: "go-previous"; enabled: chooserPane.backStack.length > 0; tip: "Back"; onClicked: chooserPane.goBack() }
                GButton { iconName: "go-next"; enabled: chooserPane.fwdStack.length > 0; tip: "Forward"; onClicked: chooserPane.goForward() }
                GButton { iconName: "go-up"; tip: "Enclosing folder"; onClicked: chooserPane.goUp() }
            }
            PathBar {
                id: chooserPath
                pane: chooserPane
                anchors.left: tools.right; anchors.leftMargin: 10
                anchors.right: views.left; anchors.rightMargin: 10
                y: 10
            }
            Segmented {
                id: views
                anchors.right: parent.right; anchors.rightMargin: 10
                y: 10
                items: [
                    { icon: "view-list-icons", tip: "Icons", value: "icons" },
                    { icon: "view-list-details", tip: "List", value: "list" },
                    { icon: "view-file-columns|view-split-left-right", tip: "Columns", value: "columns" }
                ]
                current: chooserPane.viewMode
                onPicked: (v) => chooserPane.setView(v)
            }
            FolderPane {
                id: chooserPane
                win: fc
                active: true
                anchors.top: tools.bottom; anchors.topMargin: 10
                anchors.bottom: bottomBar.top
                width: parent.width
                focus: true
            }
            Column {
                id: bottomBar
                anchors.bottom: parent.bottom
                width: parent.width - 10
                padding: 10
                spacing: 8
                Row {
                    visible: fc.saving && !fc.dirMode
                    spacing: 8
                    GLabel { text: "Save as:"; anchors.verticalCenter: parent.verticalCenter }
                    GField { id: nameField; width: 320; Keys.onReturnPressed: fc.accept() }
                }
                Flow {
                    visible: (fc.request.choices || []).length > 0
                    width: parent.width - 20
                    spacing: 14
                    Repeater {
                        model: fc.request.choices || []
                        Item {
                            id: choice
                            required property var modelData
                            readonly property bool isCheck: !(modelData.options || []).length
                            width: isCheck ? check.implicitWidth : comboRow.implicitWidth
                            height: Theme.controlHeight + 4
                            GCheck {
                                id: check
                                visible: choice.isCheck
                                anchors.verticalCenter: parent.verticalCenter
                                text: choice.modelData.label
                                checked: fc.choiceValues[choice.modelData.id] === "true"
                                onToggled: fc.setChoice(choice.modelData.id, checked ? "true" : "false")
                            }
                            Row {
                                id: comboRow
                                visible: !choice.isCheck
                                spacing: 8
                                anchors.verticalCenter: parent.verticalCenter
                                GLabel { visible: text !== ""; text: choice.modelData.label + ":"; anchors.verticalCenter: parent.verticalCenter }
                                GCombo {
                                    width: 200
                                    font.family: Theme.family
                                    font.pixelSize: Theme.body
                                    model: (choice.modelData.options || []).map(o => o.label)
                                    currentIndex: Math.max(0, (choice.modelData.options || []).findIndex(o => o.id === fc.choiceValues[choice.modelData.id]))
                                    onActivated: (i) => fc.setChoice(choice.modelData.id, choice.modelData.options[i].id)
                                }
                            }
                        }
                    }
                }
                Item {
                    width: parent.width - 20
                    height: Theme.controlHeight + 4
                    Row {
                        spacing: 8
                        anchors.verticalCenter: parent.verticalCenter
                        GCombo {
                            id: filterBox
                            visible: (fc.request.filters || []).length > 0
                            width: 220
                            font.family: Theme.family
                            font.pixelSize: Theme.body
                            model: (fc.request.filters || []).map(f => f.name)
                            onCurrentIndexChanged: if (fc.request.filters && fc.request.filters[currentIndex]) chooserPane.model.applyPatterns(JSON.stringify(fc.request.filters[currentIndex].patterns))
                        }
                        GButton { visible: fc.saving; text: "New Folder"; flat: false; capsule: true; iconName: "folder-new"; tip: "Make a folder here"; onClicked: chooserPane.newFolder() }
                    }
                    Row {
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 10
                        PushButton { text: "Cancel"; onClicked: fc.close() }
                        PushButton {
                            text: fc.request.acceptLabel || (fc.saving ? "Save" : fc.dirMode ? "Choose" : "Open")
                            primary: true
                            enabled: fc.saving || fc.dirMode || chooserPane.model.selectionCount > 0
                            onClicked: fc.accept()
                        }
                    }
                }
            }
        }
        Toast { id: toastBox; anchors.horizontalCenter: parent.horizontalCenter; anchors.bottom: parent.bottom; anchors.bottomMargin: 70 }
    }
    // Tests: GROVE_CHOOSER_TEST='{"choices": {id: value}, "filter": index,
    // "name": "x.odt"}' fills those in and presses Open/Save after a moment.
    Timer {
        readonly property string spec: fc.app ? fc.app.grove.env("GROVE_CHOOSER_TEST") : ""
        running: spec !== "" && !fc.onChosen
        interval: 1500
        onTriggered: {
            const t = JSON.parse(spec)
            if (t.filter !== undefined) filterBox.currentIndex = t.filter
            if (t.name) nameField.text = t.name
            for (const k in (t.choices || {})) fc.setChoice(k, t.choices[k])
            fc.accept()
        }
    }
    Timer {
        running: fc.app.screenshot !== "" && !fc.onChosen
        interval: 2500
        onTriggered: { fc.app.grove.grabWindows(fc.app.screenshot); fc.app.grove.chooserDone(fc.request.id, false, "[]", "{}"); fc.answered = true; Qt.quit() }
    }
    Component.onCompleted: {
        let start = request.currentFolder || ""
        if (request.currentFile) {
            const f = request.currentFile
            start = f.substring(0, f.lastIndexOf("/"))
            nameField.text = f.substring(f.lastIndexOf("/") + 1)
        }
        if (request.currentName) nameField.text = request.currentName
        const cv = {}
        for (const c of request.choices || [])
            cv[c.id] = c.default || ((c.options || []).length ? c.options[0].id : "false")
        choiceValues = cv
        chooserPane.navigate(start || app.grove.home())
        const fi = (request.filters || []).findIndex(f => f.name === request.currentFilter)
        if (fi >= 0) filterBox.currentIndex = fi
        else if ((request.filters || []).length) chooserPane.model.applyPatterns(JSON.stringify(request.filters[0].patterns))
        if (nameField.visible) { nameField.forceActiveFocus(); const d = nameField.text.lastIndexOf("."); nameField.select(0, d > 0 ? d : nameField.text.length) }
    }

    // Resize from any edge: the window manager draws no borders of its own.
    Repeater {
        model: fc.visibility !== Window.Maximized && fc.visibility !== Window.FullScreen ? [
            { e: Qt.LeftEdge, x: 0, y: 6, w: 5, h: -12, c: Qt.SizeHorCursor }, { e: Qt.RightEdge, x: -5, y: 6, w: 5, h: -12, c: Qt.SizeHorCursor },
            { e: Qt.BottomEdge, x: 6, y: -5, w: -12, h: 5, c: Qt.SizeVerCursor }, { e: Qt.TopEdge, x: 6, y: 0, w: -12, h: 4, c: Qt.SizeVerCursor },
            { e: Qt.BottomEdge | Qt.RightEdge, x: -10, y: -10, w: 10, h: 10, c: Qt.SizeFDiagCursor }, { e: Qt.BottomEdge | Qt.LeftEdge, x: 0, y: -10, w: 10, h: 10, c: Qt.SizeBDiagCursor },
            { e: Qt.TopEdge | Qt.RightEdge, x: -10, y: 0, w: 10, h: 10, c: Qt.SizeBDiagCursor }, { e: Qt.TopEdge | Qt.LeftEdge, x: 0, y: 0, w: 10, h: 10, c: Qt.SizeFDiagCursor } ] : []
        MouseArea {
            required property var modelData
            parent: fc.contentItem
            z: 1000
            x: modelData.x < 0 ? fc.contentItem.width + modelData.x : modelData.x
            y: modelData.y < 0 ? fc.contentItem.height + modelData.y : modelData.y
            width: modelData.w <= 0 ? fc.contentItem.width + modelData.w : modelData.w
            height: modelData.h <= 0 ? fc.contentItem.height + modelData.h : modelData.h
            cursorShape: modelData.c
            onPressed: fc.startSystemResize(modelData.e)
        }
    }
}
