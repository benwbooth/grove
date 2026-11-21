import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window

// grove as every app's open/save dialog (xdg-desktop-portal FileChooser):
// the same sidebar, views and search as a grove window, plus a name field
// (saving), file type filters and Open/Save.
ApplicationWindow {
    id: fc
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
    width: 960
    height: 620
    visible: false
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
    function confirm(t, m, a, fn) { fn() }
    function confirmDelete(p) {}
    function emptyTrash() {}
    function selectAfterJob() {}

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
        app.grove.chooserDone(request.id, true, JSON.stringify(uris), "{}")
        close()
    }
    function chooserOpen(path, isDir) {
        if (isDir) go(path)
        else accept([path])
    }
    onClosing: if (!answered) app.grove.chooserDone(request.id, false, "[]", "{}")

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
                Item {
                    width: parent.width - 20
                    height: Theme.controlHeight + 4
                    Row {
                        spacing: 8
                        anchors.verticalCenter: parent.verticalCenter
                        ComboBox {
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
    Component.onCompleted: {
        let start = request.currentFolder || ""
        if (request.currentFile) {
            const f = request.currentFile
            start = f.substring(0, f.lastIndexOf("/"))
            nameField.text = f.substring(f.lastIndexOf("/") + 1)
        }
        if (request.currentName) nameField.text = request.currentName
        chooserPane.navigate(start || app.grove.home())
        const fi = (request.filters || []).findIndex(f => f.name === request.currentFilter)
        if (fi >= 0) filterBox.currentIndex = fi
        else if ((request.filters || []).length) chooserPane.model.applyPatterns(JSON.stringify(request.filters[0].patterns))
        if (nameField.visible) { nameField.forceActiveFocus(); const d = nameField.text.lastIndexOf("."); nameField.select(0, d > 0 ? d : nameField.text.length) }
    }
}
