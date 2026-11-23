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
    property bool inGit: false
    property string groupBy: ""
    // Finder's View Options (Ctrl+J), remembered per folder.
    property int gridSpacing: 0          // 1–10, 0 = default (4)
    property int textSize: 0             // points, 0 = default (13)
    property string labelPos: "bottom"   // bottom, right
    property string background: ""       // "", "color:#rrggbb", "picture:/path"
    property bool itemInfo: false
    // Finder's "Always open in … view": this folder opens in its saved view;
    // otherwise browsing keeps the view you're in.
    property bool alwaysOpenIn: false
    property var previews: null          // per folder: null follows Settings
    readonly property bool previewsOn: previews === null || previews === undefined ? (!app || app.settings.show_thumbnails !== false) : !!previews
    property string stacks: ""           // "", kind, date (icon view)
    // Dolphin's selection mode: checkboxes, clicks toggle.
    property bool selectionMode: false
    property int testTip: -1             // test hook: show this row's tooltip
    function setGroupBy(g) {
        groupBy = g
        if (g) stacks = ""
        fm.applyGroupBy(modelGrouping())
        rememberView()
    }
    function setStacks(by) {
        stacks = by
        if (by) { groupBy = ""; if (viewMode !== "icons") setView("icons") }
        fm.applyGroupBy(modelGrouping())
        rememberView()
    }
    // Stacks only exist in icon view; other views show the groups.
    function modelGrouping() { return stacks && viewMode === "icons" ? "stack" + stacks : groupBy }
    onViewModeChanged: if (fm.grouping() !== modelGrouping()) fm.applyGroupBy(modelGrouping())
    function setSelectionMode(on) {
        selectionMode = on
        if (win && win.toast && on) win.toast("info", "Selection mode: click items or press Space to tick them. Esc leaves.")
    }
    function setViewOption(name, value) { pane[name] = value; rememberView() }
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
                Async.call(grove, k => grove.gitStatus(k, pane.path), r => { pane.gitMap = r || {}; pane.inGit = !!r && r.__repo === "1"; fm.setGitStatus(JSON.stringify(r || {})) })
            }
        }
    }

    function titleFor(p) {
        if (!p) return ""
        if (p.indexOf("trash:") === 0) return "Trash"
        if (p.indexOf("recent:") === 0) return "Recents"
        if (p.indexOf("computer:") === 0) return "Computer"
        if (p.indexOf("search:") === 0) return searchText ? "Searching “" + searchText + "”" : searchTag ? "Tag: " + searchTag : "Search"
        if (p.indexOf("archive://") === 0) {
            const h = p.indexOf("#")
            const a = p.substring(10, h < 0 ? p.length : h)
            const inner = h < 0 ? "" : p.substring(h + 1)
            return inner ? inner.substring(inner.lastIndexOf("/") + 1) : a.substring(a.lastIndexOf("/") + 1)
        }
        if (grove && p === grove.home()) return grove.env("USER") || "Home"
        if (p === "/") return grove ? grove.osName() : "/"
        return p.substring(p.lastIndexOf("/") + 1)
    }

    // ---------- navigation ----------
    function navigate(p, opts) {
        opts = opts || {}
        if (!p) return
        // A disk in Computer that isn't mounted: mount it, then open it.
        if (p.indexOf("mount:") === 0) {
            Async.call(grove, k => grove.mountDevice(k, p.substring(6)), r => {
                if (r.error) win.toast("error", r.error); else { pane.navigate(r.path); grove.placesRevision = grove.placesRevision + 1 }
            })
            return
        }
        if (p.length > 1 && p.endsWith("/") && p.indexOf(":///") < 0) p = p.substring(0, p.length - 1)
        if (searching && !opts.keepSearch) endSearch(false)
        if (path && path !== p && !opts.noHistory) {
            backStack = backStack.concat([path])
            fwdStack = []
        }
        rememberView()
        const hadPath = path !== ""
        path = p
        filterOpen = false
        fm.applyFilter("")
        renamingRow = -1
        // As in Finder, browsing keeps the view you're in; a folder set to
        // "Always open in … view" switches to its own. The first folder of a
        // new window or tab opens in its saved view.
        applyRememberedView(p, !!opts.firstLoad || !hadPath)
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
            if (!inner) { const f = p.substring(10, h); return f.substring(0, f.lastIndexOf("/")) || "/" }
            const i = inner.lastIndexOf("/")
            return p.substring(0, h + 1) + (i < 0 ? "" : inner.substring(0, i))
        }
        // The system disk's parent is Computer, as in Finder.
        if (p === "/") return "computer:///"
        if (p.indexOf("/") !== 0) return ""
        const i = p.lastIndexOf("/")
        return i <= 0 ? "/" : p.substring(0, i)
    }
    function goUp() {
        const up = parentOf(path)
        // Leaving an archive selects the archive file.
        const sel = path.indexOf("archive://") === 0 && path.endsWith("#") ? path.substring(10, path.length - 1) : path
        if (up) navigate(up, { select: [sel] })
    }
    function reload() { pendingSelect = JSON.parse(fm.selectedPaths()); fm.reload() }

    // ---------- views ----------
    function rememberView() {
        if (!path || special || !app || !app.settings.per_folder_views) return
        grove.setFolderView(path, JSON.stringify(viewState()))
    }
    function viewState() {
        const v = { view: viewMode, sort: fm.sortKey, descending: fm.descending, icon_size: iconSize, group: groupBy,
                    grid_spacing: gridSpacing, text_size: textSize, label_pos: labelPos === "bottom" ? "" : labelPos,
                    background: background, item_info: itemInfo, stacks: stacks, always: alwaysOpenIn }
        if (previews !== null && previews !== undefined) v.previews = previews
        return v
    }
    // Finder's "Use as Defaults": these options for every folder without its own.
    function useAsDefaults() {
        const v = viewState()
        delete v.view
        win.setSetting("view_defaults", v)
        win.toast("info", "Folders without their own view options now use these")
    }
    function applyRememberedView(p, opening) {
        let v = {}
        try { v = JSON.parse(grove.folderView(p)) } catch (e) {}
        applyViewState(v, opening)
    }
    function applyViewState(v, opening) {
        const s = app ? app.settings : {}
        const d = (s.view_defaults && typeof s.view_defaults === "object") ? s.view_defaults : {}
        const has = (k) => v.view !== undefined && v[k] !== undefined
        const pick = (k, dflt) => has(k) ? v[k] : (d[k] !== undefined ? d[k] : dflt)
        alwaysOpenIn = !!v.always
        if (opening === undefined || opening || alwaysOpenIn)
            viewMode = v.view || (pane.searching ? "list" : (s.default_view || "icons"))
        iconSize = v.icon_size || d.icon_size || s.icon_size || 64
        groupBy = pick("group", "")
        gridSpacing = pick("grid_spacing", 0)
        textSize = pick("text_size", 0)
        labelPos = pick("label_pos", "") || "bottom"
        background = pick("background", "")
        itemInfo = !!pick("item_info", false)
        previews = v.previews !== undefined ? v.previews : (d.previews !== undefined ? d.previews : null)
        stacks = pick("stacks", "")
        fm.applyGroupBy(modelGrouping())
        fm.applySort(v.sort || d.sort || "name", v.sort ? !!v.descending : !!d.descending)
    }
    // Everything a tab carries to another window: folder, history, view.
    function tabState() {
        return { path: searching ? folderBeforeSearch : path, back: backStack, fwd: fwdStack, view: viewState(), selected: selectedPaths() }
    }
    function restoreTabState(t) {
        navigate(t.path, { noHistory: true, select: t.selected || [] })
        backStack = t.back || []
        fwdStack = t.fwd || []
        if (t.view) applyViewState(t.view)
    }
    function viewLoaderItem() { return viewLoader.item }
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
        else fm.applySort(key, ["modified", "size", "created", "added", "accessed"].indexOf(key) >= 0)
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
        if (selectionMode) { fm.select(row, (modifiers & Qt.ShiftModifier) ? "extend" : "toggle"); return }
        if (modifiers & Qt.ShiftModifier) fm.select(row, (modifiers & Qt.ControlModifier) ? "extend" : "range")
        else if (modifiers & Qt.ControlModifier) fm.select(row, "toggle")
        else if (!fm.isSelected(row)) fm.select(row, "set")
        else fm.currentRow = row
    }
    function releaseItem(row, modifiers) {
        // Click on one of several selected items (no drag): only it.
        if (!selectionMode && !(modifiers & (Qt.ShiftModifier | Qt.ControlModifier)) && fm.selectionCount > 1) fm.select(row, "set")
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
        if (archiveLike(it)) {
            if (inNewTab) win.newTab(browsePath(it), [])
            else navigate(browsePath(it))
            return
        }
        grove.openPaths(JSON.stringify([it.path]))
    }
    // Where an item opens as a folder: a folder's own path, an archive's
    // archive:// listing (archives browse like folders), else "".
    function browsePath(it) {
        if (!it) return ""
        if (it.path && it.path.indexOf("mount:") === 0) return it.path
        if (it.isDir) return it.path
        if (archiveLike(it) && it.path.indexOf("/") === 0) return "archive://" + it.path + "#"
        // An archive inside an archive: its own path lists it (nested).
        if (archiveLike(it) && it.path.indexOf("archive://") === 0) return it.path
        return ""
    }
    // Anything bsdtar can open (grove's list: zip, tar.*, 7z, rar, iso, cab,
    // deb/rpm, cbz/cbr, jar/apk...), decided by the model from type and name.
    function archiveLike(it) {
        return !!it && !!it.isArchive && app && app.settings.browse_archives !== false
    }
    function openSelected(inNewTab) {
        const sel = selectedPaths()
        if (!sel.length) return
        const files = []
        for (const p of sel) {
            const row = fm.rowOf(p)
            const it = itemAt(row)
            const dir = browsePath(it)
            if (dir) {
                if (sel.length === 1 && !inNewTab) { navigate(dir); return }
                win.newTab(dir, [])
            } else files.push(p)
        }
        if (files.length) grove.openPaths(JSON.stringify(files))
    }

    // ---------- search & filter ----------
    // Finder's search criteria: chips in the search bar (kind, date, tag).
    property string searchKind: ""       // "", folder, image, video, audio, document, archive
    property int searchDays: 0           // modified within (0: any time)
    property string searchTag: ""
    readonly property bool searchFiltered: searchKind !== "" || searchDays > 0 || searchTag !== ""
    function startSearch(text) {
        searchText = text
        if (!text && !searchFiltered) { endSearch(true); return }
        if (!searching) {
            folderBeforeSearch = path
            searchRoot = special ? grove.home() : path
            searching = true
        }
        searchTimer.restart()
    }
    function setSearchFilter(name, value) {
        pane[name] = value
        if (searching) runSearch()
        else if (searchFiltered) startSearch(searchText)
    }
    function searchQuery() {
        const q = { name: searchContents ? "" : searchText, content: searchContents ? searchText : "",
                    include_hidden: app ? !!app.settings.show_hidden : false,
                    respect_ignore: app ? !!app.settings.search_respect_ignore : false,
                    kind: searchKind, days: searchDays, tag: searchTag }
        return q
    }
    function runSearch() {
        const q = searchQuery()
        viewMode = viewMode === "columns" ? "list" : viewMode
        if (path.indexOf("search:") !== 0) backStack = backStack.concat([path])
        fm.startSearch(searchRoot, JSON.stringify(q))
        path = fm.path
    }
    function endSearch(goBackToFolder) {
        if (!searching) return
        searching = false
        searchText = ""
        searchKind = ""; searchDays = 0; searchTag = ""
        fm.stop()
        if (goBackToFolder && folderBeforeSearch) {
            backStack = backStack.filter(x => x !== folderBeforeSearch)
            navigate(folderBeforeSearch, { noHistory: true })
        }
    }
    function runSmartFolder(sf) {
        folderBeforeSearch = path || grove.home()
        searching = true
        const q = sf.query || {}
        searchContents = !q.name && !!q.content
        searchText = q.name || q.content || ""
        searchKind = q.kind || ""; searchDays = q.days || 0; searchTag = q.tag || ""
        searchRoot = sf.root
        viewMode = "list"
        fm.startSearch(sf.root, JSON.stringify(q))
        path = fm.path
    }
    function showTag(tag) {
        folderBeforeSearch = path || grove.home()
        searching = true
        searchText = ""
        searchKind = ""; searchDays = 0; searchTag = tag
        searchRoot = grove.home()
        fm.startSearch(grove.home(), JSON.stringify(searchQuery()))
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
    function setRating(r, done) {
        const sel = selectedPaths()
        if (!sel.length) return
        Async.call(grove, k => grove.setRating(k, JSON.stringify(sel), r), res => {
            if (res.errors && res.errors.length) win.toast("error", res.errors.join("\n"))
            fm.refreshRatings(JSON.stringify(sel))
            if (done) done()
        })
    }
    function showEnclosing() {
        const it = currentItem()
        if (!it) return
        const dir = it.path.substring(0, it.path.lastIndexOf("/")) || "/"
        endSearch(false)
        navigate(dir, { select: [it.path] })
    }
    function compress(fmt) { const sel = selectedPaths(); if (sel.length) grove.compress(JSON.stringify(sel), fmt || "zip") }
    function extract() { for (const p of selectedPaths()) grove.extract(p, "") }
    // Extract into a folder you pick (a subfolder named after the archive
    // when it holds more than one item, as Extract Here does).
    function extractTo() {
        const sel = selectedPaths().filter(x => x.indexOf("/") === 0)
        if (!sel.length) return
        win.chooseFiles("Extract To", { directory: true, currentFolder: path, acceptLabel: "Extract Here" }, ps => {
            if (ps && ps.length) for (const p of sel) grove.extract(p, ps[0])
        })
    }

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
        } else if (e.key === Qt.Key_Space && !ctrl && selectionMode) {
            if (fm.currentRow >= 0) fm.select(fm.currentRow, "toggle")
            e.accepted = true
        } else if (e.key === Qt.Key_Space && !ctrl) { quickLook(); e.accepted = true }
        else if (e.key === Qt.Key_Backspace && !ctrl) { goBack(); e.accepted = true }
        else if (e.key === Qt.Key_Escape) {
            if (selectionMode) setSelectionMode(false)
            else if (filterOpen) { filterOpen = false; fm.applyFilter("") }
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
        if (selectionMode && !shift) fm.currentRow = r
        else fm.select(r, shift ? "range" : "set")
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
                Rectangle { width: 1; height: 16; color: Theme.separator; anchors.verticalCenter: parent.verticalCenter }
                SearchChip {
                    label: "Kind"
                    value: pane.searchKind
                    choices: [{ v: "folder", t: "Folders" }, { v: "image", t: "Images" }, { v: "video", t: "Movies" }, { v: "audio", t: "Music" }, { v: "document", t: "Documents" }, { v: "archive", t: "Archives" }]
                    tip: "Only items of one kind"
                    onPicked: (v) => pane.setSearchFilter("searchKind", v)
                }
                SearchChip {
                    label: "Date"
                    value: pane.searchDays ? "" + pane.searchDays : ""
                    choices: [{ v: "1", t: "Today" }, { v: "7", t: "Past 7 Days" }, { v: "30", t: "Past 30 Days" }, { v: "365", t: "Past Year" }]
                    tip: "Only items modified recently"
                    onPicked: (v) => pane.setSearchFilter("searchDays", Number(v || 0))
                }
                SearchChip {
                    id: tagChip
                    label: "Tag"
                    value: pane.searchTag
                    choices: []
                    tip: "Only items with a tag"
                    onAboutToOpen: Async.call(pane.grove, k => pane.grove.knownTags(k), r => tagChip.choices = (r || []).map(t => ({ v: t.name, t: t.name, color: t.color })))
                    onPicked: (v) => pane.setSearchFilter("searchTag", v)
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
        anchors.bottom: listPreview.active ? listPreview.top : selectBar.top
        focus: true
        sourceComponent: pane.viewMode === "list" ? listComp : pane.viewMode === "columns" ? columnComp : pane.viewMode === "gallery" ? galleryComp : iconComp
    }
    // The list and grid views' preview pane (Settings: list_preview, its height).
    Loader {
        id: listPreview
        active: (pane.viewMode === "list" || pane.viewMode === "icons") && !!pane.app && pane.app.settings.list_preview !== false && !(pane.win && pane.win.chooserMode)
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: selectBar.top
        // implicitHeight, not height: a Loader writes its own height back into
        // the item, which replaced the pane's binding (drags did nothing).
        height: item ? item.implicitHeight : 0
        // (Not "pane: pane" inside the component: that binds to itself.)
        sourceComponent: BottomPreview {}
        onLoaded: item.pane = pane
    }
    // Tests (GROVE_TEST previewdrag=H): as if its edge were dragged to H.
    function previewDrag(h) { if (listPreview.item) listPreview.item.dragH = h }
    // Dolphin's selection mode bar: what's ticked and what to do with it.
    Rectangle {
        id: selectBar
        anchors.bottom: parent.bottom
        width: parent.width
        height: pane.selectionMode ? Theme.controlHeight + 14 : 0
        visible: height > 0
        clip: true
        color: Theme.content
        Behavior on height { NumberAnimation { duration: 120 } }
        Rectangle { width: parent.width; height: 1; color: Theme.separator }
        Row {
            x: 10
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6
            GLabel {
                anchors.verticalCenter: parent.verticalCenter
                text: fm.selectionCount === 0 ? "Tick items to select them" : fm.selectionCount + " selected"
                color: Theme.fgSecondary
                width: Math.max(implicitWidth, 120)
            }
            GButton { text: "Select All"; flat: false; capsule: true; tip: Help.tip("Tick every item", "Select all"); onClicked: fm.selectAll() }
            GButton { text: "Clear"; flat: false; capsule: true; enabled: fm.selectionCount > 0; tip: "Untick everything"; onClicked: fm.clearSelection() }
            GButton { iconName: "edit-copy"; enabled: fm.selectionCount > 0; tip: Help.tip("Copy the ticked items", "Copy"); onClicked: pane.copy(false) }
            GButton { iconName: "edit-cut"; enabled: fm.selectionCount > 0 && !pane.special; tip: Help.tip("Cut the ticked items (paste moves them)", "Cut (move on paste)"); onClicked: pane.copy(true) }
            GButton { iconName: "edit-rename"; enabled: fm.selectionCount > 0 && !pane.special; tip: Help.tip("Rename the ticked items", "Rename"); onClicked: pane.rename() }
            GButton { iconName: "user-trash"; enabled: fm.selectionCount > 0 && (!pane.special || pane.inTrash); tip: Help.tip(pane.inTrash ? "Delete the ticked items permanently" : "Move the ticked items to the Trash", "Move to Trash"); onClicked: pane.trashSelected() }
            GButton { iconName: "document-share|emblem-shared"; enabled: fm.selectionCount > 0 && !pane.special; tip: "Share the ticked items with a nearby device"; onClicked: pane.win.share(pane.selectedPaths()) }
        }
        GButton {
            anchors.right: parent.right; anchors.rightMargin: 10
            anchors.verticalCenter: parent.verticalCenter
            text: "Done"; flat: false; capsule: true
            tip: Help.tip("Leave selection mode (Esc)", "Selection mode")
            onClicked: { pane.setSelectionMode(false); pane.forceActiveFocus() }
        }
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
        GMenuItem { text: "New Folder"; iconName: "folder-new"; shortcutText: Help.shown("Ctrl+Shift+N"); enabled: !pane.special; onTriggered: pane.newFolder() }
        GMenu {
            id: newFileMenu
            title: "New File"
            enabled: !pane.special
            GMenuItem { text: "Empty Text File"; iconName: "text-plain"; shortcutText: Help.shown("Ctrl+Alt+N"); onTriggered: pane.newFile("") }
            Instantiator {
                model: bgMenu.templates
                delegate: GMenuItem { required property var modelData; text: modelData; iconName: "text-x-generic"; onTriggered: pane.newFile(modelData) }
                onObjectAdded: (i, o) => newFileMenu.insertItem(i + 1, o)
                onObjectRemoved: (i, o) => newFileMenu.removeItem(o)
            }
        }
        GMenuItem { text: "Paste"; iconName: "edit-paste"; shortcutText: Help.shown("Ctrl+V"); enabled: !pane.special; onTriggered: pane.paste(false) }
        GMenuSeparator {}
        GMenuItem { text: "Get Info"; iconName: "documentinfo"; shortcutText: Help.shown("Alt+Return"); enabled: !pane.special; onTriggered: pane.win.showInfo([pane.path]) }
        GMenuItem { text: "Open in Terminal"; iconName: "utilities-terminal"; shortcutText: Help.shown("Ctrl+Alt+T"); enabled: !pane.special; onTriggered: pane.grove.openTerminal(pane.path) }
        GMenuItem { text: "Copy Path"; iconName: "edit-copy-path"; shortcutText: Help.shown("Ctrl+Alt+C"); onTriggered: pane.copyPath() }
        GMenuItem { text: "Add to Favourites"; iconName: "bookmark-new"; enabled: !pane.special; onTriggered: pane.grove.addFavourite(pane.path, -1) }
        GMenuItem { text: "Burn “" + pane.title + "” to Disc…"; iconName: "media-optical-burn|media-optical"; enabled: !pane.special; tip: "Put this folder's contents on a CD, DVD or Blu-ray (or a disc image file)"; onTriggered: pane.win.burn([pane.path]) }
        GMenuItem { text: "Browse Previous Versions…"; iconName: "document-open-recent"; enabled: !pane.special; tip: "Earlier copies of this folder from the hourly snapshots"; onTriggered: pane.win.showVersions(pane.path) }
        GMenu {
            id: gitBgMenu
            title: "Git"
            enabled: pane.inGit
            GMenuItem { text: "Pull"; iconName: "vcs-pull|go-down"; onTriggered: pane.win.gitAction("pull", pane.path, [], "") }
            GMenuItem { text: "Push"; iconName: "vcs-push|go-up"; onTriggered: pane.win.gitAction("push", pane.path, [], "") }
            GMenuItem { text: "Commit All Changes…"; iconName: "vcs-commit|document-save"; onTriggered: pane.win.askCommit(pane.path, []) }
            GMenuItem { text: "Show Changes"; iconName: "vcs-diff|document-preview"; onTriggered: pane.win.gitAction("diff", pane.path, [], "") }
            GMenuItem { text: "Show History"; iconName: "vcs-log|view-history"; onTriggered: pane.win.gitAction("log", pane.path, [], "") }
        }
        GMenuSeparator {}
        GMenu {
            title: "View as"
            GMenuItem { text: "Icons"; iconName: "view-grid|view-list-icons"; checkable: true; checked: pane.viewMode === "icons"; shortcutText: Help.shown("Ctrl+1"); onTriggered: pane.setView("icons") }
            GMenuItem { text: "List"; iconName: "view-list|view-list-details"; checkable: true; checked: pane.viewMode === "list"; shortcutText: Help.shown("Ctrl+2"); onTriggered: pane.setView("list") }
            GMenuItem { text: "Columns"; iconName: "view-column|view-file-columns"; checkable: true; checked: pane.viewMode === "columns"; shortcutText: Help.shown("Ctrl+3"); onTriggered: pane.setView("columns") }
            GMenuItem { text: "Gallery"; iconName: "view-paged|view-preview"; checkable: true; checked: pane.viewMode === "gallery"; shortcutText: Help.shown("Ctrl+4"); onTriggered: pane.setView("gallery") }
        }
        GMenu {
            title: "Sort by"
            GMenuItem { text: "Name"; iconName: "view-sort-ascending-name|view-sort-ascending|view-sort"; checkable: true; checked: fm.sortKey === "name"; onTriggered: pane.sortBy("name") }
            GMenuItem { text: "Kind"; iconName: "view-categories|folder-documents|view-list-tree"; checkable: true; checked: fm.sortKey === "kind"; onTriggered: pane.sortBy("kind") }
            GMenuItem { text: "Date Modified"; iconName: "document-edit|x-office-calendar"; checkable: true; checked: fm.sortKey === "modified"; onTriggered: pane.sortBy("modified") }
            GMenuItem { text: "Date Created"; iconName: "document-new|x-office-calendar"; checkable: true; checked: fm.sortKey === "created"; onTriggered: pane.sortBy("created") }
            GMenuItem { text: "Date Added"; iconName: "list-add"; checkable: true; checked: fm.sortKey === "added"; onTriggered: pane.sortBy("added") }
            GMenuItem { text: "Last Opened"; iconName: "document-open-recent"; checkable: true; checked: fm.sortKey === "accessed"; onTriggered: pane.sortBy("accessed") }
            GMenuItem { text: "Size"; iconName: "drive-harddisk|view-sort"; checkable: true; checked: fm.sortKey === "size"; onTriggered: pane.sortBy("size") }
            GMenuSeparator {}
            GMenuItem { text: "Descending"; iconName: "view-sort-descending|go-down"; checkable: true; checked: fm.descending; onTriggered: { fm.applySort(fm.sortKey, !fm.descending); pane.rememberView() } }
            GMenuItem { text: "Folders First"; iconName: "folder"; checkable: true; checked: fm.foldersFirst; onTriggered: pane.win.setSetting("folders_first", !fm.foldersFirst) }
        }
        GMenu {
            title: "Group by"
            GMenuItem { text: "None"; iconName: "edit-clear"; checkable: true; checked: pane.groupBy === ""; onTriggered: pane.setGroupBy("") }
            GMenuItem { text: "Kind"; iconName: "view-categories|folder-documents|view-list-tree"; checkable: true; checked: pane.groupBy === "kind"; onTriggered: pane.setGroupBy("kind") }
            GMenuItem { text: "Date Modified"; iconName: "document-edit|x-office-calendar"; checkable: true; checked: pane.groupBy === "date"; onTriggered: pane.setGroupBy("date") }
            GMenuItem { text: "Size"; iconName: "drive-harddisk|view-sort"; checkable: true; checked: pane.groupBy === "size"; onTriggered: pane.setGroupBy("size") }
            GMenuItem { text: "Name"; iconName: "view-sort-ascending-name|view-sort-ascending|view-sort"; checkable: true; checked: pane.groupBy === "name"; onTriggered: pane.setGroupBy("name") }
            GMenuItem { text: "Tags"; iconName: "tag|bookmarks|starred"; checkable: true; checked: pane.groupBy === "tags"; onTriggered: pane.setGroupBy("tags") }
        }
        GMenuItem { text: "Use Stacks"; iconName: "view-list-tree|view-grid"; checkable: true; checked: pane.stacks !== ""; shortcutText: Help.shown("Ctrl+Alt+0"); tip: "Pile files of one kind into stacks (icon view)"; onTriggered: pane.setStacks(pane.stacks ? "" : "kind") }
        GMenuItem { text: "Show View Options"; iconName: "configure"; shortcutText: Help.shown("Ctrl+J"); tip: "Icon size, spacing, text size, labels, background for this folder"; onTriggered: pane.win.toggleViewOptions() }
        GMenuItem { text: "Show Hidden Files"; iconName: "view-hidden|view-visible"; checkable: true; checked: fm.showHidden; shortcutText: Help.shown("Ctrl+H"); onTriggered: pane.win.toggleHidden() }
        GMenuSeparator { visible: pane.inTrash }
        GMenuItem { text: "Empty Trash…"; iconName: "trash-empty"; visible: pane.inTrash; height: visible ? implicitHeight : 0; onTriggered: pane.win.emptyTrash() }
    }

    // Selected item paths for drags (cached per drag start).
    property string dragData: ""
}
