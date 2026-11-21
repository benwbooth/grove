import QtQuick
import QtQuick.Controls.Basic
import Grove

// One browsing pane: a folder, its history, its view (icons, list,
// columns, gallery), selection, the filter bar and search. A tab holds one
// pane, or two side by side (split view, F3).
FocusScope {
    id: pane

    property var win
    property var app: win ? win.app : null
    property var grove: app ? app.grove : null
    property alias model: fm
    property string path: ""
    property string title: titleFor(path)
    property string viewMode: "icons"
    property int iconSize: 64
    property var backStack: []
    property var fwdStack: []
    property bool active: false
    property bool filterOpen: false
    property bool searching: false
    property string searchText: ""
    property bool searchContents: app && app.settings ? !!app.settings.search_contents : false
    property string searchRoot: ""
    property string folderBeforeSearch: ""
    property int renamingRow: -1
    property var pendingSelect: []
    property string typeAhead: ""
    property bool dropHover: false
    property var gitMap: ({})
    readonly property bool inTrash: path.indexOf("trash:") === 0
    readonly property bool special: path.indexOf(":///") > 0 || inTrash || path.indexOf("recent:") === 0
    readonly property bool inArchive: path.indexOf("archive://") === 0
    property Item view: viewLoader.item

    signal activated()

    FolderModel {
        id: fm
        onLoaded: {
            if (pane.pendingSelect.length) {
                fm.selectPaths(JSON.stringify(pane.pendingSelect))
                if (pane.view && pane.view.positionAt) pane.view.positionAt(fm.currentRow)
                pane.pendingSelect = []
            }
            if (app && app.settings.show_git_status && !pane.special && !pane.searching) {
                Async.call(grove, k => grove.gitStatus(k, pane.path), r => fm.setGitStatus(JSON.stringify(r || {})))
            }
        }
    }

    function titleFor(p) {
        if (!p) return ""
        if (p.indexOf("trash:") === 0) return "Trash"
        if (p.indexOf("recent:") === 0) return "Recents"
        if (p.indexOf("search:") === 0) return "Searching “" + searchText + "”"
        if (p.indexOf("archive://") === 0) {
            const h = p.indexOf("#")
            const a = p.substring(10, h < 0 ? p.length : h)
            const inner = h < 0 ? "" : p.substring(h + 1)
            return inner ? inner.substring(inner.lastIndexOf("/") + 1) : a.substring(a.lastIndexOf("/") + 1)
        }
        if (grove && p === grove.home()) return grove.env("USER") || "Home"
        if (p === "/") return "Computer"
        return p.substring(p.lastIndexOf("/") + 1)
    }

    // ---------- navigation ----------
    function navigate(p, opts) {
        opts = opts || {}
        if (!p) return
        if (p.length > 1 && p.endsWith("/") && p.indexOf(":///") < 0) p = p.substring(0, p.length - 1)
        if (searching && !opts.keepSearch) endSearch(false)
        if (path && path !== p && !opts.noHistory) {
            backStack = backStack.concat([path])
            fwdStack = []
        }
        rememberView()
        path = p
        filterOpen = false
        fm.applyFilter("")
        renamingRow = -1
        applyRememberedView(p)
        fm.applyShowHidden(app ? !!app.settings.show_hidden : false)
        fm.applyFoldersFirst(app ? app.settings.folders_first !== false : true)
        pendingSelect = opts.select || []
        fm.loadPath(p)
        gitMap = {}
    }
    function goBack() {
        if (!backStack.length) return
        const p = backStack[backStack.length - 1]
        backStack = backStack.slice(0, -1)
        fwdStack = fwdStack.concat([path])
        const from = path
        navigate(p, { noHistory: true, select: [from] })
    }
    function goForward() {
        if (!fwdStack.length) return
        const p = fwdStack[fwdStack.length - 1]
        fwdStack = fwdStack.slice(0, -1)
        backStack = backStack.concat([path])
        navigate(p, { noHistory: true })
    }
    function parentOf(p) {
        if (p.indexOf("archive://") === 0) {
            const h = p.indexOf("#")
            if (h < 0) return p.substring(10, p.lastIndexOf("/")) || "/"
            const inner = p.substring(h + 1)
            const i = inner.lastIndexOf("/")
            return i < 0 ? p.substring(0, h) : p.substring(0, h + 1) + inner.substring(0, i)
        }
        if (p === "/" || p.indexOf("/") !== 0) return ""
        const i = p.lastIndexOf("/")
        return i <= 0 ? "/" : p.substring(0, i)
    }
    function goUp() {
        const up = parentOf(path)
        if (up) navigate(up, { select: [path] })
    }
    function reload() { pendingSelect = JSON.parse(fm.selectedPaths()); fm.reload() }

    // ---------- views ----------
    function rememberView() {
        if (!path || special || !app || !app.settings.per_folder_views) return
        grove.setFolderView(path, JSON.stringify({ view: viewMode, sort: fm.sortKey, descending: fm.descending, icon_size: iconSize }))
    }
    function applyRememberedView(p) {
        let v = {}
        try { v = JSON.parse(grove.folderView(p)) } catch (e) {}
        const s = app ? app.settings : {}
        viewMode = v.view || (pane.searching ? "list" : (s.default_view || "icons"))
        iconSize = v.icon_size || s.icon_size || 64
        fm.applySort(v.sort || "name", !!v.descending)
    }
    function setView(mode) {
        viewMode = mode
        rememberView()
        pane.forceActiveFocus()
    }
    function zoom(d) {
        const sizes = [16, 24, 32, 48, 64, 96, 128, 192, 256]
        let i = sizes.indexOf(iconSize)
        if (i < 0) i = 4
        i = Math.max(0, Math.min(sizes.length - 1, i + d))
        iconSize = sizes[i]
        rememberView()
    }
    function sortBy(key) {
        if (fm.sortKey === key) fm.applySort(key, !fm.descending)
        else fm.applySort(key, key === "modified" || key === "size" || key === "created")
        rememberView()
    }

    // ---------- selection & opening ----------
    function selectedPaths() { return JSON.parse(fm.selectedPaths()) }
    function itemAt(row) { return JSON.parse(fm.get(row)) }
    function currentItem() { return fm.currentRow >= 0 ? itemAt(fm.currentRow) : null }

    function pressItem(row, modifiers) {
        pane.activated()
        pane.forceActiveFocus()
        if (renamingRow >= 0 && renamingRow !== row) renamingRow = -1
        if (modifiers & Qt.ShiftModifier) fm.select(row, (modifiers & Qt.ControlModifier) ? "extend" : "range")
        else if (modifiers & Qt.ControlModifier) fm.select(row, "toggle")
        else if (!fm.isSelected(row)) fm.select(row, "set")
        else fm.currentRow = row
    }
    function releaseItem(row, modifiers) {
        // Click on one of several selected items (no drag): only it.
        if (!(modifiers & (Qt.ShiftModifier | Qt.ControlModifier)) && fm.selectionCount > 1) fm.select(row, "set")
    }
    function openRow(row, inNewTab) {
        const it = itemAt(row)
        if (!it.path) return
        if (win.chooserMode) { win.chooserOpen(it.path, it.isDir); return }
        if (it.isDir) {
            if (inNewTab) win.newTab(it.path, [])
            else navigate(it.path)
            return
        }
        if (it.key && inTrash) { win.toast("info", "Items in the Trash can't be opened. Put it back first (right-click › Put Back)."); return }
        if (archiveLike(it)) { navigate("archive://" + it.path + "#"); return }
        grove.openPaths(JSON.stringify([it.path]))
    }
    function archiveLike(it) {
        const n = (it.name || "").toLowerCase()
        return [".zip", ".tar", ".tar.gz", ".tgz", ".tar.xz", ".tar.zst", ".tar.bz2", ".7z", ".rar", ".jar"].some(e => n.endsWith(e))
            && app && app.settings.browse_archives !== false
    }
    function openSelected(inNewTab) {
        const sel = selectedPaths()
        if (!sel.length) return
        const files = []
        for (const p of sel) {
            const row = fm.rowOf(p)
            const it = itemAt(row)
            if (it.isDir) {
                if (sel.length === 1 && !inNewTab) { navigate(it.path); return }
                win.newTab(it.path, [])
            } else files.push(p)
        }
        if (files.length) grove.openPaths(JSON.stringify(files))
    }

    // ---------- search & filter ----------
    function startSearch(text) {
        searchText = text
        if (!text) { endSearch(true); return }
        if (!searching) {
            folderBeforeSearch = path
            searchRoot = special ? grove.home() : path
            searching = true
        }
        searchTimer.restart()
    }
    function runSearch() {
        const q = { name: searchContents ? "" : searchText, content: searchContents ? searchText : "",
                    include_hidden: app ? !!app.settings.show_hidden : false,
                    respect_ignore: app ? !!app.settings.search_respect_ignore : false }
        if (!searchContents && searchText.indexOf(" ") < 0 && searchText.indexOf("*") < 0) q.name = searchText
        viewMode = viewMode === "columns" ? "list" : viewMode
        if (path.indexOf("search:") !== 0) backStack = backStack.concat([path])
        fm.startSearch(searchRoot, JSON.stringify(q))
        path = fm.path
    }
    function endSearch(goBackToFolder) {
        if (!searching) return
        searching = false
        searchText = ""
        fm.stop()
        if (goBackToFolder && folderBeforeSearch) {
            backStack = backStack.filter(x => x !== folderBeforeSearch)
            navigate(folderBeforeSearch, { noHistory: true })
        }
    }
    function runSmartFolder(sf) {
        folderBeforeSearch = path || grove.home()
        searching = true
        searchText = sf.query.name || sf.query.content || ""
        searchRoot = sf.root
        viewMode = "list"
        fm.startSearch(sf.root, JSON.stringify(sf.query))
        path = fm.path
    }
    function showTag(tag) {
        folderBeforeSearch = path || grove.home()
        searching = true
        searchText = "tag: " + tag
        searchRoot = grove.home()
        fm.startSearch(grove.home(), JSON.stringify({ name: "", tag: tag }))
        path = fm.path
    }
    Timer { id: searchTimer; interval: 250; onTriggered: pane.runSearch() }

    // ---------- file actions ----------
    function newFolder() {
        if (special) return
        Async.call(grove, k => grove.newFolder(k, pane.path, ""), r => {
            if (r.error) { win.toast("error", r.error); return }
            pane.pendingSelect = [r.path]
            pane.renameWhenLoaded = r.path
            selectWhenThere(r.path, true)
        })
    }
    function newFile(name) {
        if (special) return
        Async.call(grove, k => grove.newFile(k, pane.path, name || ""), r => {
            if (r.error) { win.toast("error", r.error); return }
            selectWhenThere(r.path, true)
        })
    }
    property string renameWhenLoaded: ""
    // The watcher adds new items shortly; select (and rename) once there.
    function selectWhenThere(p, rename) {
        waitFor.target = p
        waitFor.rename = rename
        waitFor.tries = 0
        waitFor.start()
    }
    Timer {
        id: waitFor
        property string target: ""
        property bool rename: false
        property int tries: 0
        interval: 60
        repeat: true
        onTriggered: {
            const row = fm.rowOf(target)
            if (row >= 0) {
                stop()
                fm.selectPaths(JSON.stringify([target]))
                if (pane.view && pane.view.positionAt) pane.view.positionAt(row)
                if (rename) pane.renamingRow = row
            } else if (++tries > 50) stop()
        }
    }
    function rename() {
        const sel = selectedPaths()
        if (sel.length > 1) { win.batchRename(sel); return }
        if (fm.currentRow >= 0 && !special) renamingRow = fm.currentRow
    }
    function commitRename(row, newName) {
        const it = itemAt(row)
        renamingRow = -1
        pane.forceActiveFocus()
        if (!it.path || newName === it.name || !newName) return
        Async.call(grove, k => grove.renamePath(k, it.path, newName), r => {
            if (r.error) win.toast("error", r.error)
            else selectWhenThere(r.path, false)
        })
    }
    function trashSelected() {
        const sel = selectedPaths()
        if (!sel.length) return
        if (inTrash) { win.confirmDelete(sel); return }
        const go = () => Async.call(grove, k => grove.trashPaths(k, JSON.stringify(sel)), r => {
            if (r.errors && r.errors.length) win.toast("error", r.errors.join("\n"))
            app.refreshTrash()
        })
        if (app.settings.confirm_trash) win.confirm("Move " + (sel.length === 1 ? "this item" : sel.length + " items") + " to the Trash?", "You can put items back from the Trash later.", "Move to Trash", go)
        else go()
    }
    function deleteSelected() { const sel = selectedPaths(); if (sel.length) win.confirmDelete(sel) }
    function putBack() {
        const keys = []
        for (const p of selectedPaths()) keys.push(itemAt(fm.rowOf(p)).key)
        Async.call(grove, k => grove.restoreTrash(k, JSON.stringify(keys)), r => {
            if (r.errors && r.errors.length) win.toast("error", r.errors.join("\n"))
            app.refreshTrash()
            fm.reload()
        })
    }
    function copy(cut) {
        const sel = selectedPaths()
        if (!sel.length) return
        grove.copyToClipboard(JSON.stringify(sel), cut)
        fm.setCutPaths(JSON.stringify(cut ? sel : []))
        win.toast("info", (cut ? "Cut " : "Copied ") + (sel.length === 1 ? "1 item" : sel.length + " items") + (cut ? ". Paste to move." : ""))
    }
    function paste(forceMove) {
        if (special) return
        if (forceMove) {
            const c = JSON.parse(grove.clipboard())
            if (c.paths.length) grove.runJob("move", JSON.stringify(c.paths), path)
            return
        }
        if (grove.pasteInto(path) < 0) win.toast("info", "There's nothing to paste. Copy some files first.")
        fm.setCutPaths("[]")
    }
    function duplicate() { const sel = selectedPaths(); if (sel.length && !special) grove.runJob("duplicate", JSON.stringify(sel), path) }
    function makeAlias() { const sel = selectedPaths(); if (sel.length && !special) grove.runJob("link", JSON.stringify(sel.map(x => x)), path) }
    function copyPath() {
        const sel = selectedPaths()
        const t = sel.length ? sel.join("\n") : path
        grove.copyText(t)
        win.toast("info", "Copied the path" + (sel.length > 1 ? "s" : ""))
    }
    function toggleTag(tag) {
        const sel = selectedPaths()
        if (!sel.length) return
        Async.call(grove, k => grove.toggleTag(k, JSON.stringify(sel), tag), r => {
            if (r.error) win.toast("error", r.error)
            fm.refreshTags(JSON.stringify(sel))
        })
    }
    function quickLook() { win.quickLook(pane) }
    function compress(fmt) { const sel = selectedPaths(); if (sel.length) grove.compress(JSON.stringify(sel), fmt || "zip") }
    function extract() { for (const p of selectedPaths()) grove.extract(p, "") }

    // Type-to-select.
    Timer { id: typeReset; interval: 900; onTriggered: pane.typeAhead = "" }
    function typeSelect(ch) {
        typeAhead += ch
        typeReset.restart()
        const r = fm.findPrefix(typeAhead, typeAhead.length === 1 ? fm.currentRow + 1 : Math.max(0, fm.currentRow))
        if (r >= 0) { fm.select(r, "set"); if (view && view.positionAt) view.positionAt(r) }
    }

    // ---------- keys (view focused) ----------
    Keys.onPressed: (e) => {
        if (renamingRow >= 0) return
        const ctrl = e.modifiers & Qt.ControlModifier
        const alt = e.modifiers & Qt.AltModifier
        const shift = e.modifiers & Qt.ShiftModifier
        if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) {
            if (alt) win.showInfo(selectedPaths().length ? selectedPaths() : [path])
            else openSelected(ctrl)
            e.accepted = true
        } else if (e.key === Qt.Key_Space && !ctrl) { quickLook(); e.accepted = true }
        else if (e.key === Qt.Key_Backspace && !ctrl) { goBack(); e.accepted = true }
        else if (e.key === Qt.Key_Escape) {
            if (filterOpen) { filterOpen = false; fm.applyFilter("") }
            else if (searching) endSearch(true)
            else fm.clearSelection()
            e.accepted = true
        } else if (e.key === Qt.Key_Tab && !ctrl && win.currentTab && win.currentTab.split) {
            win.currentTab.switchPane(); e.accepted = true
        } else if (view && view.handleKey && view.handleKey(e)) {
            e.accepted = true
        } else if (!ctrl && !alt && e.text.length === 1 && e.text >= " " && e.key !== Qt.Key_Space) {
            typeSelect(e.text)
            e.accepted = true
        }
    }

    // Move the current item by `d` rows (keyboard), with Shift extending.
    function step(d, shift) {
        const n = fm.count
        if (!n) return
        let r = fm.currentRow < 0 ? (d > 0 ? 0 : n - 1) : Math.max(0, Math.min(n - 1, fm.currentRow + d))
        fm.select(r, shift ? "range" : "set")
        if (view && view.positionAt) view.positionAt(r)
        win.quickLookFollow(pane)
    }

    // ---------- drag & drop ----------
    function dropAction(drop) {
        if (drop.proposedAction === Qt.CopyAction) return "copy"
        if (drop.proposedAction === Qt.LinkAction) return "link"
        return "auto"
    }
    function dropInto(dest, drop) {
        const paths = []
        for (const u of drop.urls) {
            const s = u.toString()
            if (s.indexOf("file://") === 0) paths.push(decodeURIComponent(s.substring(7)))
        }
        if (!paths.length) return false
        if (dest.indexOf(":///") > 0 || dest.indexOf("trash:") === 0) {
            if (dest.indexOf("trash:") === 0) {
                Async.call(grove, k => grove.trashPaths(k, JSON.stringify(paths)), r => { if (r.errors.length) win.toast("error", r.errors.join("\n")); app.refreshTrash() })
                return true
            }
            return false
        }
        if (paths.every(p => p.substring(0, p.lastIndexOf("/")) === dest) && dropAction(drop) === "auto") return false
        grove.runJob(dropAction(drop), JSON.stringify(paths), dest)
        drop.acceptProposedAction()
        return true
    }
    function dragUris() {
        return selectedPaths().map(p => "file://" + encodeURI(p).replace(/#/g, "%23").replace(/\?/g, "%3F")).join("\r\n")
    }

    DropArea {
        anchors.fill: parent
        keys: ["text/uri-list"]
        onEntered: (drag) => { pane.dropHover = !pane.special || pane.inTrash }
        onExited: pane.dropHover = false
        onDropped: (drop) => { pane.dropHover = false; pane.dropInto(pane.path, drop) }
    }

    Rectangle {
        anchors.fill: parent
        color: Theme.content
    }

    Column {
        id: topBars
        width: parent.width
        z: 2
        // Filter bar (Ctrl+I, like Dolphin's).
        Rectangle {
            width: parent.width
            height: pane.filterOpen ? Theme.controlHeight + 12 : 0
            visible: height > 0
            color: Theme.content
            clip: true
            Behavior on height { NumberAnimation { duration: 120 } }
            Row {
                anchors.verticalCenter: parent.verticalCenter
                x: 10
                spacing: 8
                GLabel { text: "Filter:"; color: Theme.fgSecondary; anchors.verticalCenter: parent.verticalCenter }
                GField {
                    id: filterField
                    width: Math.min(pane.width - 120, 360)
                    iconName: "view-filter"
                    clearable: true
                    placeholderText: "Show only items whose name contains…"
                    tip: "Type to show only the items in this folder whose names contain these words (Ctrl+I). Esc closes."
                    onTextChanged: fm.applyFilter(text)
                    Keys.onEscapePressed: { pane.filterOpen = false; text = ""; pane.forceActiveFocus() }
                    Keys.onDownPressed: pane.forceActiveFocus()
                    Keys.onReturnPressed: { pane.forceActiveFocus(); if (fm.count) fm.select(0, "set") }
                }
            }
            GButton {
                anchors.right: parent.right; anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                iconName: "window-close"; iconSize: 12
                tip: "Close the filter bar (Esc)"
                onClicked: { pane.filterOpen = false; filterField.text = ""; pane.forceActiveFocus() }
            }
        }
        // Search scope bar.
        Rectangle {
            width: parent.width
            height: pane.searching ? Theme.controlHeight + 10 : 0
            visible: height > 0
            color: Theme.content
            clip: true
            Row {
                x: 10
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                GLabel { text: "Search:"; color: Theme.fgSecondary; anchors.verticalCenter: parent.verticalCenter }
                GButton {
                    text: "Home"; checked: pane.grove && pane.searchRoot === pane.grove.home(); flat: false; capsule: true
                    tip: "Search everything in your home folder"
                    onClicked: { pane.searchRoot = pane.grove.home(); pane.runSearch() }
                }
                GButton {
                    text: "“" + pane.titleFor(pane.folderBeforeSearch) + "”"
                    visible: pane.grove && pane.folderBeforeSearch !== pane.grove.home() && pane.folderBeforeSearch.indexOf("/") === 0
                    checked: pane.searchRoot === pane.folderBeforeSearch; flat: false; capsule: true
                    tip: "Search only in the folder you were in"
                    onClicked: { pane.searchRoot = pane.folderBeforeSearch; pane.runSearch() }
                }
                GButton {
                    text: "Computer"; checked: pane.searchRoot === "/"; flat: false; capsule: true
                    tip: "Search the whole computer (slower)"
                    onClicked: { pane.searchRoot = "/"; pane.runSearch() }
                }
                Rectangle { width: 1; height: 16; color: Theme.separator; anchors.verticalCenter: parent.verticalCenter }
                GButton {
                    text: "Names"; checked: !pane.searchContents; flat: false; capsule: true
                    tip: "Find items whose name contains the search text"
                    onClicked: { pane.searchContents = false; pane.runSearch() }
                }
                GButton {
                    text: "Contents"; checked: pane.searchContents; flat: false; capsule: true
                    tip: "Find files that contain the search text inside them"
                    onClicked: { pane.searchContents = true; pane.runSearch() }
                }
                GLabel {
                    anchors.verticalCenter: parent.verticalCenter
                    text: fm.loading ? "  Searching… " + fm.count + " found" : "  " + fm.count + " found"
                    color: Theme.fgSecondary
                }
            }
            GButton {
                anchors.right: parent.right; anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                text: "Save"; flat: false; capsule: true
                tip: "Save this search as a Smart Folder in the sidebar"
                onClicked: pane.win.saveSmartFolder(pane)
            }
        }
    }

    Loader {
        id: viewLoader
        anchors.top: topBars.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        focus: true
        sourceComponent: pane.viewMode === "list" ? listComp : pane.viewMode === "columns" ? columnComp : pane.viewMode === "gallery" ? galleryComp : iconComp
    }
    Component { id: iconComp; IconGrid { pane: viewLoader.parent } }
    Component { id: listComp; DetailList { pane: viewLoader.parent } }
    Component { id: columnComp; ColumnBrowser { pane: viewLoader.parent } }
    Component { id: galleryComp; Gallery { pane: viewLoader.parent } }

    // Empty / error / loading states.
    Column {
        anchors.centerIn: viewLoader
        spacing: 8
        visible: fm.count === 0 && pane.viewMode !== "columns"
        GLabel {
            anchors.horizontalCenter: parent.horizontalCenter
            text: fm.error !== "" ? fm.error : fm.loading ? "" : pane.searching ? "No results" : fm.filter !== "" ? "Nothing matches “" + fm.filter + "”" : pane.inTrash ? "The Trash is empty" : "This folder is empty"
            color: fm.error !== "" ? Theme.danger : Theme.fgSecondary
            font.pixelSize: Theme.title
            width: Math.min(implicitWidth, pane.width - 60)
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
        }
    }
    BusyDots {
        anchors.top: viewLoader.top
        anchors.right: viewLoader.right
        anchors.margins: 10
        running: fm.loading
    }

    Rectangle {
        anchors.fill: parent
        color: "transparent"
        border.width: 2
        border.color: Theme.accent
        radius: 4
        visible: pane.dropHover
    }

    // ---------- context menus ----------
    function showItemMenu(row, x, y) {
        if (!fm.isSelected(row)) fm.select(row, "set")
        itemMenu.target = itemAt(row)
        itemMenu.sel = selectedPaths()
        itemMenu.apps = []
        itemMenu.services = []
        if (!itemMenu.target.isDir && itemMenu.sel.length === 1)
            Async.call(grove, k => grove.appsFor(k, itemMenu.target.path), r => itemMenu.apps = r.apps || [])
        Async.call(grove, k => grove.serviceMenus(k, JSON.stringify(itemMenu.sel)), r => itemMenu.services = r || [])
        itemMenu.popup()
    }
    function showBackgroundMenu() {
        fm.clearSelection()
        Async.call(grove, k => grove.templates(k), r => bgMenu.templates = r || [])
        bgMenu.popup()
    }

    ItemMenu { id: itemMenu; pane: itemMenu.parent }

    GMenu {
        id: bgMenu
        property var templates: []
        GMenuItem { text: "New Folder"; iconName: "folder-new"; shortcutText: "Ctrl+Shift+N"; enabled: !pane.special; onTriggered: pane.newFolder() }
        GMenu {
            id: newFileMenu
            title: "New File"
            enabled: !pane.special
            GMenuItem { text: "Empty Text File"; iconName: "text-plain"; shortcutText: "Ctrl+Alt+N"; onTriggered: pane.newFile("") }
            Instantiator {
                model: bgMenu.templates
                delegate: GMenuItem { required property var modelData; text: modelData; iconName: "text-x-generic"; onTriggered: pane.newFile(modelData) }
                onObjectAdded: (i, o) => newFileMenu.insertItem(i + 1, o)
                onObjectRemoved: (i, o) => newFileMenu.removeItem(o)
            }
        }
        GMenuItem { text: "Paste"; iconName: "edit-paste"; shortcutText: "Ctrl+V"; enabled: !pane.special; onTriggered: pane.paste(false) }
        GMenuSeparator {}
        GMenuItem { text: "Get Info"; iconName: "documentinfo"; shortcutText: "Alt+Return"; enabled: !pane.special; onTriggered: pane.win.showInfo([pane.path]) }
        GMenuItem { text: "Open in Terminal"; iconName: "utilities-terminal"; shortcutText: "Ctrl+Alt+T"; enabled: !pane.special; onTriggered: pane.grove.openTerminal(pane.path) }
        GMenuItem { text: "Copy Path"; iconName: "edit-copy-path"; shortcutText: "Ctrl+Alt+C"; onTriggered: pane.copyPath() }
        GMenuItem { text: "Add to Favourites"; iconName: "bookmark-new"; enabled: !pane.special; onTriggered: pane.grove.addFavourite(pane.path, -1) }
        GMenuSeparator {}
        GMenu {
            title: "View as"
            GMenuItem { text: "Icons"; checkable: true; checked: pane.viewMode === "icons"; shortcutText: "Ctrl+1"; onTriggered: pane.setView("icons") }
            GMenuItem { text: "List"; checkable: true; checked: pane.viewMode === "list"; shortcutText: "Ctrl+2"; onTriggered: pane.setView("list") }
            GMenuItem { text: "Columns"; checkable: true; checked: pane.viewMode === "columns"; shortcutText: "Ctrl+3"; onTriggered: pane.setView("columns") }
            GMenuItem { text: "Gallery"; checkable: true; checked: pane.viewMode === "gallery"; shortcutText: "Ctrl+4"; onTriggered: pane.setView("gallery") }
        }
        GMenu {
            title: "Sort by"
            GMenuItem { text: "Name"; checkable: true; checked: fm.sortKey === "name"; onTriggered: pane.sortBy("name") }
            GMenuItem { text: "Kind"; checkable: true; checked: fm.sortKey === "kind"; onTriggered: pane.sortBy("kind") }
            GMenuItem { text: "Date Modified"; checkable: true; checked: fm.sortKey === "modified"; onTriggered: pane.sortBy("modified") }
            GMenuItem { text: "Date Created"; checkable: true; checked: fm.sortKey === "created"; onTriggered: pane.sortBy("created") }
            GMenuItem { text: "Size"; checkable: true; checked: fm.sortKey === "size"; onTriggered: pane.sortBy("size") }
            GMenuSeparator {}
            GMenuItem { text: "Descending"; checkable: true; checked: fm.descending; onTriggered: { fm.applySort(fm.sortKey, !fm.descending); pane.rememberView() } }
            GMenuItem { text: "Folders First"; checkable: true; checked: fm.foldersFirst; onTriggered: pane.win.setSetting("folders_first", !fm.foldersFirst) }
        }
        GMenuItem { text: "Show Hidden Files"; checkable: true; checked: fm.showHidden; shortcutText: "Ctrl+H"; onTriggered: pane.win.toggleHidden() }
        GMenuSeparator { visible: pane.inTrash }
        GMenuItem { text: "Empty Trash…"; iconName: "trash-empty"; visible: pane.inTrash; height: visible ? implicitHeight : 0; onTriggered: pane.win.emptyTrash() }
    }

    // Selected item paths for drags (cached per drag start).
    property string dragData: ""
}
