import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window

// Grove Archiver: an archive's contents in a window, like 7-Zip's File
// Manager in grove's look. Folders open in place (Backspace goes up); the
// columns are 7-Zip's: Size, Packed Size, Modified, Method, CRC, Encrypted,
// Attributes, sortable. Toolbar: Add, Extract, Test, Copy To, Delete, Info.
// Inside 7z, zip and tar archives you can add (drop files on the window),
// delete, rename and make folders. Double-click a file to open it.
Window {
    id: aw
    property var app
    property var archiver
    property string path: ""
    property string password: ""
    property var listing: null
    property string error: ""
    property string errorCode: ""
    property bool loading: false
    property string cwd: ""            // folder inside ("" = top)
    property var rows: []
    property var sel: ({})             // path -> true
    property int anchorRow: -1
    property int currentRow: -1
    property string sortKey: "name"
    property bool sortDesc: false
    property string filter: ""
    property bool showInfo: false
    property string busyText: ""
    readonly property var grove: app.grove
    readonly property bool writable: !!listing && listing.writable
    readonly property int selCount: Object.keys(sel).length

    width: Math.round(900 * Theme.pt)
    height: Math.round(580 * Theme.pt)
    minimumWidth: 560
    minimumHeight: 320
    color: Theme.window
    title: path ? name(path) + (cwd ? " — " + cwd : "") : "Grove Archiver"

    function name(p) { return p.substring(p.lastIndexOf("/") + 1) }
    function dirOf(p) { return p.substring(0, p.lastIndexOf("/")) || "/" }

    Component.onCompleted: if (path) load()
    function load() {
        loading = true; error = ""; errorCode = ""
        const p = path
        Async.call(grove, k => grove.arcList(k, p, aw.password), r => {
            if (p !== aw.path) return
            aw.loading = false
            if (r.error) { aw.error = r.error; aw.errorCode = r.code || ""; aw.listing = null; aw.rows = []; if (r.code === "needPassword" || r.code === "badPassword") pwField.forceActiveFocus(); return }
            aw.listing = r
            if (aw.cwd && !r.items.some(i => i.path === aw.cwd || i.path.indexOf(aw.cwd + "/") === 0)) aw.cwd = ""
            aw.rebuild()
        })
    }
    function openArchive(p) { path = p; password = ""; cwd = ""; sel = ({}); expanded = ({}); load() }

    // The rows of the folder shown: its files, and its folders (also those
    // only implied by paths) with their totals.
    // Folders opened in place with their disclosure triangles (path -> true).
    property var expanded: ({})
    // The items directly in folder `pre` ("" = top, else "dir/"), sorted,
    // with folders that only appear in paths filled in and their totals.
    function children(pre) {
        const byName = {}
        for (const it of listing.items) {
            if (pre && it.path.indexOf(pre) !== 0) continue
            const rest = it.path.substring(pre.length)
            if (!rest) continue
            const slash = rest.indexOf("/")
            const nm = slash < 0 ? rest : rest.substring(0, slash)
            let r = byName[nm]
            if (!r) {
                r = byName[nm] = { name: nm, path: pre + nm, isDir: slash >= 0, size: 0, packed: 0, mtime: 0, method: "", crc: "", encrypted: false, mode: 0, files: 0, link: "", depth: 0 }
            }
            if (slash < 0 && !it.isDir) {
                Object.assign(r, { isDir: false, size: it.size, packed: it.packed, mtime: it.mtime, method: it.method, crc: it.crc, encrypted: it.encrypted, mode: it.mode, link: it.link })
            } else {
                r.isDir = true
                if (slash < 0) { r.mtime = it.mtime; r.mode = it.mode }
                else if (!it.isDir) { r.size += it.size; r.packed += (it.packed || 0); r.files++; r.encrypted = r.encrypted || it.encrypted; r.mtime = Math.max(r.mtime, it.mtime) }
            }
        }
        return sorted(Object.values(byName))
    }
    function sorted(out) {
        const k = sortKey, d = sortDesc ? -1 : 1
        return out.sort((a, b) => {
            if (a.isDir !== b.isDir) return a.isDir ? -1 : 1
            let x = a[k], y = b[k]
            if (k === "name" || k === "method" || k === "crc") return d * String(x).localeCompare(String(y), undefined, { numeric: true, sensitivity: "base" })
            if (k === "encrypted") { x = x ? 1 : 0; y = y ? 1 : 0 }
            return d * ((x || 0) - (y || 0)) || a.name.localeCompare(b.name)
        })
    }
    // The rows shown: the folder's items, and inside every expanded folder
    // its items, indented. A filter shows every match in the folder (all
    // depths), each with its path.
    function rebuild() {
        if (!listing) { rows = []; return }
        const pre = cwd ? cwd + "/" : ""
        const f = filter.toLowerCase()
        let out = []
        if (f) {
            const seen = {}
            const walk = (p, depth) => {
                for (const r of children(p)) {
                    if (r.name.toLowerCase().indexOf(f) >= 0) { r.depth = 0; r.where = p.substring(pre.length); out.push(r) }
                    if (r.isDir && depth < 64) walk(r.path + "/", depth + 1)
                }
            }
            walk(pre, 0)
        } else {
            const walk = (p, depth) => {
                for (const r of children(p)) {
                    r.depth = depth
                    r.open = r.isDir && !!expanded[r.path]
                    out.push(r)
                    if (r.open) walk(r.path + "/", depth + 1)
                }
            }
            walk(pre, 0)
        }
        rows = out
        const keep = {}
        for (const r of out) if (sel[r.path]) keep[r.path] = true
        sel = keep
        currentRow = Math.min(currentRow, rows.length - 1)
    }
    // Open or close a folder in place; `all` (Alt-click, as in Finder) does
    // every folder inside it too.
    function toggle(path, open, all) {
        const e = Object.assign({}, expanded)
        const want = open === undefined ? !e[path] : open
        const set = p => { if (want) e[p] = true; else delete e[p] }
        set(path)
        if (all) for (const it of listing.items) {
            const parts = it.path.split("/")
            for (let i = 1; i < parts.length + (it.isDir ? 1 : 0); i++) {
                const d = parts.slice(0, i).join("/")
                if (d.indexOf(path + "/") === 0) set(d)
            }
        }
        expanded = e
        rebuild()
    }
    function expandAll(open) {
        const e = {}
        if (open && listing) for (const it of listing.items) {
            const parts = it.path.split("/")
            for (let i = 1; i < parts.length + (it.isDir ? 1 : 0); i++) e[parts.slice(0, i).join("/")] = true
        }
        expanded = e
        rebuild()
    }
    onFilterChanged: rebuild()
    function sortBy(k) { if (sortKey === k) sortDesc = !sortDesc; else { sortKey = k; sortDesc = k === "size" || k === "packed" || k === "mtime" } rebuild() }

    function enter(p) { cwd = p; sel = ({}); currentRow = -1; anchorRow = -1; filterField.text = ""; rebuild() }
    function up() { if (!cwd) return; const prev = cwd; enter(cwd.indexOf("/") < 0 ? "" : cwd.substring(0, cwd.lastIndexOf("/"))); for (let i = 0; i < rows.length; i++) if (rows[i].path === prev) { select(i, "set"); break } }

    function select(i, how) {
        if (i < 0 || i >= rows.length) return
        let s = how === "set" ? {} : Object.assign({}, sel)
        if (how === "toggle") { if (s[rows[i].path]) delete s[rows[i].path]; else s[rows[i].path] = true; anchorRow = i }
        else if (how === "range" && anchorRow >= 0) { s = {}; for (let j = Math.min(anchorRow, i); j <= Math.max(anchorRow, i); j++) s[rows[j].path] = true }
        else { s[rows[i].path] = true; anchorRow = i }
        sel = s
        currentRow = i
        list.positionViewAtIndex(i, ListView.Contain)
    }
    function keyDisclose(open, all) {
        const r = rows[currentRow]
        if (!r) return
        if (r.isDir && (open ? !r.open || all : r.open)) { toggle(r.path, open, all); return }
        if (!open && r.depth > 0) {
            const parent = r.path.substring(0, r.path.lastIndexOf("/"))
            for (let i = 0; i < rows.length; i++) if (rows[i].path === parent) { select(i, "set"); return }
        }
        if (!open && r.depth === 0) up()
    }
    function selectAll() { const s = {}; for (const r of rows) s[r.path] = true; sel = s }
    function selected() { return rows.filter(r => sel[r.path]).map(r => r.path) }
    function selectedRows() { return rows.filter(r => sel[r.path]) }

    function activate(i) {
        const r = rows[i]
        if (!r) return
        if (r.isDir) { enter(r.path); return }
        aw.busyText = "Opening “" + r.name + "”…"
        Async.call(grove, k => grove.arcMember(k, aw.path, r.path, aw.password), x => {
            aw.busyText = ""
            if (x.error) { toast(x.error); return }
            grove.openPaths(JSON.stringify([x.path]))
        })
    }

    // ---- commands
    function extractAll() { if (path) archiver.extractDialog(path, { password: password }) }
    function extractSelected() {
        if (!path) return
        const s = selected()
        if (!s.length) return extractAll()
        archiver.extractDialog(path, { only: s, strip: cwd ? cwd + "/" : "", password: password })
    }
    // 7-Zip's Copy To: the selection straight into a folder you pick.
    function copyTo() {
        const s = selected()
        if (!path || !s.length) return
        archiver.choose("Copy To", { directory: true, currentFolder: dirOf(path), acceptLabel: "Copy Here" }, ps => {
            if (!ps || !ps.length) return
            archiver.extract({ archive: aw.path, dest: ps[0], folder: "never", only: s, strip: aw.cwd ? aw.cwd + "/" : "", password: aw.password, overwrite: "ask" }, true, { reveal: true, trash: false, force: true })
        })
    }
    function test() { if (path) archiver.run("test", { archive: path, password: password }) }
    function change(args) {
        args.archive = path; args.password = password
        archiver.run("update", args, r => aw.load())
    }
    function addFiles(paths) {
        if (!paths || !paths.length) return
        if (!path) { if (paths.length === 1) openArchive(paths[0]); return }
        if (!writable) { toast(listing ? listing.format + " archives can only be read. Use Add to Archive for a new 7z or zip." : ""); return }
        change({ add: paths, addInto: cwd })
    }
    function chooseAdd() {
        if (!writable) return
        archiver.choose("Add to “" + name(path) + "”", { multiple: true, currentFolder: dirOf(path), acceptLabel: "Add" }, ps => addFiles(ps))
    }
    property var pendingDelete: []
    function askDelete() { const s = selected(); if (writable && s.length) { pendingDelete = s; confirmDel.open() } }
    function doDelete() { change({ delete: pendingDelete }); pendingDelete = [] }
    function startRename() {
        const r = selectedRows()
        if (!writable || r.length !== 1) return
        renamePop.target = r[0]
        renameField.text = r[0].name
        renamePop.open()
        renameField.forceActiveFocus()
        const dot = r[0].isDir ? -1 : r[0].name.lastIndexOf(".")
        renameField.select(0, dot > 0 ? dot : r[0].name.length)
    }
    function newFolder() {
        if (!writable) return
        let n = "untitled folder", i = 2
        const names = rows.map(r => r.name)
        while (names.indexOf(n) >= 0) n = "untitled folder " + (i++)
        change({ mkdir: [(cwd ? cwd + "/" : "") + n] })
    }
    function openOther() {
        archiver.choose("Open Archive", { currentFolder: path ? dirOf(path) : grove.home(), acceptLabel: "Open" }, ps => { if (ps && ps.length) aw.openArchive(ps[0]) })
    }
    function toast(m) { toastText.text = m; toastTimer.restart() }
    function fmtDate(t) { return t > 0 ? new Date(t * 1000).toLocaleString(Qt.locale(), "yyyy-MM-dd HH:mm") : "" }
    function modeText(m, dir) {
        if (!m) return ""
        const c = "rwxrwxrwx"
        let s = dir ? "d" : (m & 0o170000) === 0o120000 ? "l" : "-"
        for (let i = 0; i < 9; i++) s += (m & (1 << (8 - i))) ? c[i] : "-"
        return s
    }
    function sizeText(r) { return r.isDir && !r.files ? "" : app.size(r.size) }

    // ---- keys
    Shortcut { sequences: ["Ctrl+W", StandardKey.Close]; onActivated: aw.close() }
    Shortcut { sequence: "Ctrl+O"; onActivated: aw.openOther() }
    Shortcut { sequences: ["Backspace", "Alt+Up", "Ctrl+Up"]; enabled: !renamePop.opened && !filterField.activeFocus; onActivated: aw.up() }
    Shortcut { sequences: ["Return", "Enter", "Ctrl+Down"]; enabled: !renamePop.opened && !pwField.activeFocus && !filterField.activeFocus; onActivated: aw.activate(aw.currentRow) }
    Shortcut { sequence: "Ctrl+E"; onActivated: aw.extractSelected() }
    Shortcut { sequence: "Ctrl+Shift+E"; onActivated: aw.extractAll() }
    Shortcut { sequence: "F5"; onActivated: aw.copyTo() }
    Shortcut { sequence: "Ctrl+R"; onActivated: aw.load() }
    Shortcut { sequence: "Ctrl+T"; onActivated: aw.test() }
    Shortcut { sequences: ["Ctrl+I", "Alt+Return"]; onActivated: aw.showInfo = !aw.showInfo }
    Shortcut { sequences: ["Delete", "Ctrl+Backspace"]; enabled: !filterField.activeFocus && !renamePop.opened; onActivated: aw.askDelete() }
    Shortcut { sequence: "F2"; onActivated: aw.startRename() }
    Shortcut { sequence: "Ctrl+Shift+N"; onActivated: aw.newFolder() }
    Shortcut { sequences: ["Ctrl+Shift+A", "Insert"]; onActivated: aw.chooseAdd() }
    Shortcut { sequence: "Ctrl+A"; enabled: !filterField.activeFocus && !renamePop.opened; onActivated: aw.selectAll() }
    Shortcut { sequences: ["Ctrl+F", "Ctrl+L"]; onActivated: filterField.forceActiveFocus() }
    Shortcut { sequence: "F1"; onActivated: helpWin.show() }
    Shortcut { sequence: "Down"; enabled: !renamePop.opened; onActivated: aw.select(Math.min(aw.rows.length - 1, aw.currentRow + 1), "set") }
    Shortcut { sequence: "Up"; enabled: !renamePop.opened; onActivated: aw.select(Math.max(0, aw.currentRow - 1), "set") }
    Shortcut { sequence: "Shift+Down"; onActivated: aw.select(Math.min(aw.rows.length - 1, aw.currentRow + 1), "range") }
    Shortcut { sequence: "Shift+Up"; onActivated: aw.select(Math.max(0, aw.currentRow - 1), "range") }
    // Finder's list view: Right opens a folder in place, Left closes it (or
    // goes to the folder it's in); Alt opens or closes everything inside.
    Shortcut { sequences: ["Right", "Alt+Right"]; enabled: !renamePop.opened && !filterField.activeFocus; onActivated: aw.keyDisclose(true, false) }
    Shortcut { sequences: ["Left"]; enabled: !renamePop.opened && !filterField.activeFocus; onActivated: aw.keyDisclose(false, false) }
    Shortcut { sequences: ["Alt+Shift+Right"]; onActivated: aw.keyDisclose(true, true) }
    Shortcut { sequences: ["Alt+Shift+Left"]; onActivated: aw.keyDisclose(false, true) }
    Shortcut { sequence: "Ctrl+Alt+Right"; onActivated: aw.expandAll(true) }
    Shortcut { sequence: "Ctrl+Alt+Left"; onActivated: aw.expandAll(false) }

    HelpWindow { id: helpWin; app: aw.app; query: "Archiver" }

    readonly property var columns: [
        { k: "size", label: "Size", w: 90, right: true, tip: "How big the file is once extracted (folders: everything in them)" },
        { k: "packed", label: "Packed Size", w: 96, right: true, tip: "Its size inside the archive. In solid 7z archives files are packed together: the first file of each block carries the block's size" },
        { k: "mtime", label: "Modified", w: 140, tip: "When the file was last changed (as stored in the archive)" },
        { k: "method", label: "Method", w: 120, tip: "How it's compressed (and AES when encrypted)" },
        { k: "crc", label: "CRC", w: 84, tip: "The CRC-32 checksum stored for the file; Test compares it with the contents" },
        { k: "encrypted", label: "🔒", w: 30, tip: "Encrypted: needs the password to extract" },
        { k: "mode", label: "Attributes", w: 96, tip: "Unix permissions, where the archive keeps them" }]
    readonly property real fixedW: { let t = 0; for (const c of columns) t += c.w * Theme.pt; return t }
    readonly property real nameW: Math.max(200, list.width - fixedW - 12)

    Item {
        anchors.fill: parent
        transform: Translate { x: 0.001; y: 0.001 }

        // ---- toolbar
        Rectangle {
            id: bar
            width: parent.width
            height: Theme.toolbarHeight
            color: Theme.toolbar
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.separator }
            Row {
                x: 10
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4
                GButton { iconName: "go-up|go-previous"; enabled: aw.cwd !== ""; tip: "Up one folder (Backspace)"; onClicked: aw.up() }
                GButton { iconName: "document-open|folder-open"; tip: "Open another archive (Ctrl+O)"; onClicked: aw.openOther() }
                Item { width: 6; height: 1 }
                component Tool: GButton {
                    property string label: ""
                    width: Math.max(54, lbl.implicitWidth + 12)
                    height: bar.height - 8
                    iconSize: 20
                    GLabel { id: lbl; text: parent.label; font.pixelSize: Theme.caption; anchors.horizontalCenter: parent.horizontalCenter; anchors.bottom: parent.bottom; anchors.bottomMargin: 2; color: Theme.fgSecondary }
                }
                Tool { label: "Add"; iconName: "list-add|archive-insert"; enabled: aw.writable; tip: aw.writable ? "Add files to this archive (or drop them on the window) — Ctrl+Shift+A" : "This kind of archive can only be read"; onClicked: aw.chooseAdd() }
                Tool { label: "Extract"; iconName: "archive-extract|document-export"; enabled: !!aw.listing; tip: aw.selCount ? "Extract what's selected… (Ctrl+E)" : "Extract everything… (Ctrl+E)"; onClicked: aw.extractSelected() }
                Tool { label: "Test"; iconName: "dialog-ok-apply|checkmark"; enabled: !!aw.listing; tip: "Check every file against its checksum, to find damage (Ctrl+T)"; onClicked: aw.test() }
                Tool { label: "Copy To"; iconName: "edit-copy"; enabled: aw.selCount > 0; tip: "Extract what's selected straight into a folder you pick (F5)"; onClicked: aw.copyTo() }
                Tool { label: "Delete"; iconName: "edit-delete"; enabled: aw.writable && aw.selCount > 0; tip: "Remove what's selected from the archive (Delete)"; onClicked: aw.askDelete() }
                Tool { label: "Info"; iconName: "documentinfo|dialog-information"; checked: aw.showInfo; enabled: !!aw.listing; tip: "About this archive: format, method, solid, encryption, sizes, volumes (Ctrl+I)"; onClicked: aw.showInfo = !aw.showInfo }
            }
            Row {
                anchors.right: parent.right; anchors.rightMargin: 10
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                GField { id: filterField; width: 180; iconName: "edit-find|search"; clearable: true; placeholderText: "Filter"; tip: "Show only names containing this (Ctrl+F)"; onTextChanged: aw.filter = text
                         Keys.onEscapePressed: { text = ""; list.forceActiveFocus() } }
                GButton { iconName: "help-contents|help-about"; tip: "Help and shortcuts (F1)"; onClicked: helpMenu.popup() }
                GMenu {
                    id: helpMenu
                    GMenuItem { text: "Grove Archiver Help"; iconName: "help-contents"; shortcutText: "F1"; onTriggered: helpWin.show() }
                    GMenuItem { text: "Add to Archive…"; iconName: "archive-insert"; tip: "Make a new archive from files you pick"
                        onTriggered: aw.archiver.choose("Add to Archive", { multiple: true, currentFolder: aw.path ? aw.dirOf(aw.path) : aw.grove.home(), acceptLabel: "Choose" }, ps => { if (ps && ps.length) aw.archiver.compressDialog(ps) }) }
                    GMenuItem { text: "New Folder"; iconName: "folder-new"; enabled: aw.writable; shortcutText: "Ctrl+Shift+N"; onTriggered: aw.newFolder() }
                    GMenuItem { text: "Show Archive in grove"; iconName: "folder-open"; enabled: !!aw.path; onTriggered: aw.app.openOrTab([aw.dirOf(aw.path)], [aw.path]) }
                }
            }
        }

        // ---- path line
        Rectangle {
            id: crumbs
            y: bar.height
            width: parent.width; height: Math.round(26 * Theme.pt)
            color: Theme.window
            Row {
                x: 12; anchors.verticalCenter: parent.verticalCenter
                spacing: 4
                Repeater {
                    model: aw.path ? [""].concat(aw.cwd ? aw.cwd.split("/") : []) : []
                    Row {
                        required property var modelData
                        required property int index
                        spacing: 4
                        GLabel { visible: index > 0; text: "›"; color: Theme.fgTertiary }
                        GIcon { size: 16; name: index === 0 ? "package-x-generic" : "folder"; anchors.verticalCenter: parent.verticalCenter }
                        GLabel {
                            text: index === 0 ? aw.name(aw.path) : modelData
                            font.pixelSize: Theme.small
                            MouseArea { anchors.fill: parent; onClicked: aw.enter(aw.cwd.split("/").slice(0, index).join("/")) }
                        }
                    }
                }
            }
            Row {
                anchors.right: parent.right; anchors.rightMargin: 10
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                visible: !!aw.listing && !aw.filter
                GButton { text: "Expand All"; height: crumbs.height - 4; enabled: aw.rows.some(r => r.isDir && !r.open); tip: "Open every folder in place (Ctrl+Alt+Right)"; onClicked: aw.expandAll(true) }
                GButton { text: "Collapse All"; height: crumbs.height - 4; enabled: Object.keys(aw.expanded).length > 0; tip: "Close every open folder (Ctrl+Alt+Left)"; onClicked: aw.expandAll(false) }
            }
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.separator }
        }

        // ---- the list
        Item {
            id: body
            clip: true
            y: crumbs.y + crumbs.height
            width: parent.width - (aw.showInfo ? info.width : 0)
            height: parent.height - y - status.height

            Rectangle {
                id: header
                width: parent.width; height: Math.round(24 * Theme.pt)
                color: Theme.window
                visible: !!aw.listing
                Row {
                    x: 6
                    height: parent.height
                    component Head: Item {
                        property string k: ""
                        property string label: ""
                        property bool alignEnd: false
                        property string tip: ""
                        height: header.height
                        GLabel {
                            anchors.fill: parent; anchors.leftMargin: 6; anchors.rightMargin: 6
                            text: parent.label + (aw.sortKey === parent.k ? (aw.sortDesc ? "  ⌄" : "  ⌃") : "")
                            font.pixelSize: Theme.small; font.bold: aw.sortKey === parent.k
                            color: Theme.fgSecondary
                            horizontalAlignment: parent.alignEnd ? Text.AlignRight : Text.AlignLeft
                        }
                        Rectangle { anchors.right: parent.right; width: 1; height: parent.height - 8; anchors.verticalCenter: parent.verticalCenter; color: Theme.separator }
                        MouseArea { id: hm; anchors.fill: parent; hoverEnabled: true; onClicked: aw.sortBy(parent.k) }
                        GTip { visible: hm.containsMouse && parent.tip !== ""; text: parent.tip + " — click to sort" }
                    }
                    Head { k: "name"; label: "Name"; width: aw.nameW; tip: "The file or folder's name" }
                    Repeater {
                        model: aw.columns
                        Head { required property var modelData; k: modelData.k; label: modelData.label; alignEnd: !!modelData.right; tip: modelData.tip; width: modelData.w * Theme.pt }
                    }
                }
                Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.separator }
            }

            ListView {
                id: list
                y: header.visible ? header.height : 0
                width: parent.width; height: parent.height - y
                clip: true
                model: aw.rows
                focus: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: GScrollBar {}
                delegate: Rectangle {
                    id: row
                    required property var modelData
                    required property int index
                    readonly property bool picked: !!aw.sel[modelData.path]
                    width: list.width
                    height: Theme.rowHeight
                    color: picked ? (aw.active ? Theme.accent : Theme.selectedInactive) : index % 2 ? Theme.rowAlt : "transparent"
                    radius: 4
                    readonly property color ink: picked && aw.active ? Theme.accentText : Theme.fg
                    readonly property color ink2: picked && aw.active ? Theme.accentText : Theme.fgSecondary
                    Row {
                        x: 6
                        height: parent.height
                        Row {
                            width: aw.nameW
                            height: parent.height
                            spacing: 4
                            leftPadding: 4 + (row.modelData.depth || 0) * 16
                            // Disclosure triangle: open the folder in place.
                            Item {
                                width: 14; height: parent.height
                                visible: !aw.filter
                                GLabel {
                                    visible: row.modelData.isDir
                                    anchors.centerIn: parent
                                    text: "›"
                                    font.pixelSize: Theme.title
                                    color: row.ink2
                                    rotation: row.modelData.open ? 90 : 0
                                    Behavior on rotation { NumberAnimation { duration: 120 } }
                                }
                                MouseArea {
                                    id: tri
                                    anchors.fill: parent
                                    enabled: row.modelData.isDir
                                    hoverEnabled: true
                                    onClicked: (e) => aw.toggle(row.modelData.path, undefined, (e.modifiers & Qt.AltModifier) !== 0)
                                }
                                GTip { visible: tri.containsMouse && row.modelData.isDir; text: (row.modelData.open ? "Collapse" : "Expand") + " (Right/Left; Alt-click: everything inside too)" }
                            }
                            GIcon {
                                size: 16; anchors.verticalCenter: parent.verticalCenter
                                name: row.modelData.isDir ? "folder" : aw.iconFor(row.modelData.name)
                            }
                            GLabel { text: row.modelData.name + (row.modelData.link && row.modelData.link !== "→" ? "  → " + row.modelData.link : "") + (row.modelData.where ? "   — " + row.modelData.where : "")
                                     width: aw.nameW - 56 - (row.modelData.depth || 0) * 16; color: row.ink; anchors.verticalCenter: parent.verticalCenter }
                        }
                        component Cell: GLabel {
                            property real w: 80
                            property bool alignEnd: false
                            width: w * Theme.pt
                            height: row.height
                            leftPadding: 6; rightPadding: 6
                            horizontalAlignment: alignEnd ? Text.AlignRight : Text.AlignLeft
                            color: row.ink2
                            font.pixelSize: Theme.small
                            font.features: { "tnum": 1 }
                        }
                        Cell { w: 90; alignEnd: true; text: aw.sizeText(row.modelData) }
                        Cell { w: 96; alignEnd: true; text: row.modelData.packed === null || row.modelData.packed === undefined || (row.modelData.isDir && !row.modelData.packed) ? "" : aw.app.size(row.modelData.packed) }
                        Cell { w: 140; text: aw.fmtDate(row.modelData.mtime) }
                        Cell { w: 120; text: row.modelData.method }
                        Cell { w: 84; text: row.modelData.crc; font.family: Theme.mono }
                        Cell { w: 30; text: row.modelData.encrypted ? "🔒" : ""; horizontalAlignment: Text.AlignHCenter }
                        Cell { w: 96; text: aw.modeText(row.modelData.mode, row.modelData.isDir); font.family: Theme.mono }
                    }
                    MouseArea {
                        anchors.fill: parent
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        onPressed: (e) => {
                            list.forceActiveFocus()
                            if (e.button === Qt.RightButton) { if (!row.picked) aw.select(row.index, "set"); rowMenu.popup(); return }
                            aw.select(row.index, (e.modifiers & Qt.ControlModifier) ? "toggle" : (e.modifiers & Qt.ShiftModifier) ? "range" : "set")
                        }
                        onDoubleClicked: aw.activate(row.index)
                    }
                }
                MouseArea {
                    anchors.fill: parent
                    z: -1
                    acceptedButtons: Qt.LeftButton | Qt.RightButton
                    onPressed: (e) => { aw.sel = ({}); list.forceActiveFocus(); if (e.button === Qt.RightButton) bgMenu.popup() }
                }
            }

            // Nothing open yet / can't read / needs a password.
            Column {
                anchors.centerIn: parent
                width: Math.min(parent.width - 40, 380)
                spacing: 12
                visible: !aw.listing
                GIcon { size: 64; name: "package-x-generic"; anchors.horizontalCenter: parent.horizontalCenter }
                GLabel {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; elide: Text.ElideNone
                    text: aw.loading ? "Reading “" + aw.name(aw.path) + "”…"
                        : !aw.path ? "Open an archive to see what's inside, or drop one here.\n7z, zip, rar, tar (.gz .xz .bz2 .zst), iso, cab, deb, rpm and more."
                        : aw.errorCode === "needPassword" ? "“" + aw.name(aw.path) + "” is protected by a password, file names included."
                        : aw.errorCode === "badPassword" ? "That password doesn't open it. Try again:"
                        : aw.error
                    color: aw.error && aw.errorCode !== "needPassword" && aw.errorCode !== "badPassword" ? Theme.danger : Theme.fg
                }
                GField {
                    id: pwField
                    visible: aw.errorCode === "needPassword" || aw.errorCode === "badPassword"
                    width: parent.width
                    echoMode: TextInput.Password
                    placeholderText: "Password"
                    tip: "The password the archive was made with"
                    onAccepted: { aw.password = text; text = ""; aw.load() }
                }
                PushButton {
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: !aw.path && !aw.loading
                    primary: true
                    text: "Open Archive…"
                    tip: "Pick an archive (Ctrl+O)"
                    onClicked: aw.openOther()
                }
            }

            DropArea {
                anchors.fill: parent
                onEntered: (d) => { d.accepted = d.hasUrls; dropGlow.visible = d.hasUrls }
                onExited: dropGlow.visible = false
                onDropped: (d) => {
                    dropGlow.visible = false
                    const ps = d.urls.map(u => decodeURIComponent(String(u).replace(/^file:\/\//, "")))
                    aw.addFiles(ps)
                }
                Rectangle { id: dropGlow; visible: false; anchors.fill: parent; anchors.margins: 4; radius: 8; color: Theme.dropTarget; border.width: 2; border.color: Theme.accent
                    GLabel { anchors.centerIn: parent; text: aw.path ? (aw.writable ? "Add to “" + (aw.cwd || aw.name(aw.path)) + "”" : "This archive can only be read") : "Open it"; font.bold: true } }
            }
        }

        // ---- info panel
        Rectangle {
            id: info
            visible: aw.showInfo && !!aw.listing
            x: parent.width - width
            y: body.y
            width: Math.round(270 * Theme.pt)
            height: body.height
            color: Theme.sidebar
            Rectangle { width: 1; height: parent.height; color: Theme.separator }
            readonly property var lst: aw.listing || {}
            readonly property real total: { let t = 0; for (const i of (lst.items || [])) t += i.size; return t }
            readonly property int nfiles: (lst.items || []).filter(i => !i.isDir).length
            Flickable {
                anchors.fill: parent; anchors.margins: 14
                contentHeight: facts.height
                clip: true
                Column {
                    id: facts
                    width: parent.width
                    spacing: 6
                    component F: Column {
                        property string k: ""
                        property string v: ""
                        width: facts.width
                        visible: v !== ""
                        GLabel { text: parent.k; font.pixelSize: Theme.caption; color: Theme.fgSecondary }
                        GLabel { text: parent.v; width: parent.width; wrapMode: Text.WrapAnywhere; elide: Text.ElideNone; font.pixelSize: Theme.small }
                    }
                    GLabel { text: aw.name(aw.path); font.bold: true; width: parent.width; wrapMode: Text.WrapAnywhere; elide: Text.ElideNone }
                    F { k: "Kind"; v: (info.lst.format || "") + " archive" + (info.lst.writable ? "" : " (read only)") }
                    F { k: "Method"; v: info.lst.method || "" }
                    F { k: "Solid"; v: info.lst.format === "7z" ? (info.lst.solid ? "Yes, " + info.lst.blocks + (info.lst.blocks === 1 ? " block" : " blocks") : "No") : "" }
                    F { k: "Encrypted"; v: info.lst.headerEncrypted ? "Yes, file names too" : info.lst.encrypted ? "Yes (names visible)" : "No" }
                    F { k: "Contents"; v: info.nfiles + (info.nfiles === 1 ? " file, " : " files, ") + aw.app.size(info.total) }
                    F { k: "Size on disk"; v: aw.app.size(info.lst.physical || 0) + (info.total > 0 ? " — " + Math.round((info.lst.physical || 0) / info.total * 100) + "% of the contents" : "") }
                    F { k: "Volumes"; v: (info.lst.volumes || []).length > 1 ? (info.lst.volumes.length + " parts: " + info.lst.volumes.map(aw.name).join(", ")) : "" }
                    F { k: "Comment"; v: info.lst.comment || "" }
                    F { k: "Where"; v: aw.dirOf(aw.path) }
                    F { k: "Selected"; v: aw.selCount ? aw.selCount + " items, " + aw.app.size(aw.selectedRows().reduce((t, r) => t + r.size, 0)) : "" }
                }
            }
        }

        // ---- status bar
        Rectangle {
            id: status
            y: parent.height - height
            width: parent.width; height: Math.round(24 * Theme.pt)
            color: Theme.window
            Rectangle { width: parent.width; height: 1; color: Theme.separator }
            GLabel {
                id: busy
                x: 12; anchors.verticalCenter: parent.verticalCenter
                font.pixelSize: Theme.small; color: Theme.fgSecondary
                width: parent.width - 24
                property string text2: !aw.listing ? "" : (aw.selCount ? aw.selCount + " of " : "") + aw.rows.length + (aw.rows.length === 1 ? " item" : " items")
                    + (aw.selCount ? " selected, " + aw.app.size(aw.selectedRows().reduce((t, r) => t + r.size, 0)) : "")
                    + "   ·   " + aw.listing.format + (aw.listing.method ? " " + aw.listing.method : "") + (aw.listing.solid ? ", solid" : "") + (aw.listing.encrypted ? ", encrypted" : "")
                    + (aw.listing.writable ? "" : ", read only")
                text: aw.busyText || text2
            }
        }

        // ---- toast
        Rectangle {
            visible: toastText.text !== ""
            anchors.horizontalCenter: parent.horizontalCenter
            y: parent.height - status.height - height - 14
            width: Math.min(parent.width - 40, toastText.implicitWidth + 28); height: toastText.implicitHeight + 14
            radius: height / 2
            color: Theme.glass
            border.color: Theme.glassEdge
            GLabel { id: toastText; anchors.centerIn: parent; width: Math.min(implicitWidth, aw.width - 68); wrapMode: Text.Wrap; elide: Text.ElideNone }
            Timer { id: toastTimer; interval: 5000; onTriggered: toastText.text = "" }
        }
    }

    // Icons by name (freedesktop mime-ish names; grove's icon theme fills in).
    function iconFor(n) {
        const e = n.substring(n.lastIndexOf(".") + 1).toLowerCase()
        if (/^(png|jpe?g|gif|webp|bmp|tiff?|svg|heic|avif)$/.test(e)) return "image-x-generic"
        if (/^(mp4|mkv|webm|mov|avi|m4v)$/.test(e)) return "video-x-generic"
        if (/^(mp3|flac|ogg|opus|wav|m4a|aac)$/.test(e)) return "audio-x-generic"
        if (/^(zip|7z|rar|tar|gz|xz|bz2|zst|tgz|iso|cab|deb|rpm)$/.test(e)) return "package-x-generic"
        if (e === "pdf") return "application-pdf"
        if (/^(txt|md|log|ini|conf|cfg|json|xml|yaml|yml|toml|csv)$/.test(e)) return "text-x-generic"
        if (/^(c|h|cpp|rs|py|js|ts|qml|java|go|sh|html|css)$/.test(e)) return "text-x-script|text-x-generic"
        if (/^(exe|msi|dll|so|bin|appimage)$/.test(e)) return "application-x-executable"
        return "text-x-generic|unknown"
    }

    GMenu {
        id: rowMenu
        GMenuItem { text: "Open"; iconName: "document-open"; shortcutText: "Return"; onTriggered: aw.activate(aw.currentRow) }
        GMenuItem { text: (aw.rows[aw.currentRow] || {}).open ? "Collapse" : "Expand"; iconName: "view-list-tree"; visible: !!(aw.rows[aw.currentRow] || {}).isDir; height: visible ? implicitHeight : 0
                    shortcutText: (aw.rows[aw.currentRow] || {}).open ? "Left" : "Right"; tip: "Show or hide what's in this folder, in place"; onTriggered: aw.keyDisclose(!(aw.rows[aw.currentRow] || {}).open, false) }
        GMenuItem { text: "Expand Everything Inside"; iconName: "view-list-tree"; visible: !!(aw.rows[aw.currentRow] || {}).isDir; height: visible ? implicitHeight : 0
                    shortcutText: "Alt+Shift+Right"; onTriggered: aw.keyDisclose(true, true) }
        GMenuItem { text: "Extract…"; iconName: "archive-extract"; shortcutText: "Ctrl+E"; tip: "Extract what's selected, with options"; onTriggered: aw.extractSelected() }
        GMenuItem { text: "Copy To…"; iconName: "edit-copy"; shortcutText: "F5"; tip: "Straight into a folder you pick"; onTriggered: aw.copyTo() }
        GMenuSeparator {}
        GMenuItem { text: "Rename"; iconName: "edit-rename"; shortcutText: "F2"; enabled: aw.writable && aw.selCount === 1; onTriggered: aw.startRename() }
        GMenuItem { text: "Delete from Archive"; iconName: "edit-delete"; shortcutText: "Delete"; enabled: aw.writable; onTriggered: aw.askDelete() }
    }
    GMenu {
        id: bgMenu
        GMenuItem { text: "Add Files…"; iconName: "list-add"; enabled: aw.writable; shortcutText: "Ctrl+Shift+A"; onTriggered: aw.chooseAdd() }
        GMenuItem { text: "New Folder"; iconName: "folder-new"; enabled: aw.writable; shortcutText: "Ctrl+Shift+N"; onTriggered: aw.newFolder() }
        GMenuItem { text: "Extract All…"; iconName: "archive-extract"; enabled: !!aw.listing; shortcutText: "Ctrl+Shift+E"; onTriggered: aw.extractAll() }
        GMenuItem { text: "Test Archive"; iconName: "dialog-ok-apply"; enabled: !!aw.listing; shortcutText: "Ctrl+T"; onTriggered: aw.test() }
    }

    Popup {
        id: renamePop
        property var target: null
        anchors.centerIn: parent
        width: 340
        padding: 16; topPadding: 16.001; leftPadding: 16.001
        modal: true
        popupType: Popup.Item
        background: GlassPanel { radius: Theme.glassRadius; tint: Theme.glassMenu }
        Column {
            spacing: 10
            width: parent.width
            GLabel { text: "Rename “" + (renamePop.target ? renamePop.target.name : "") + "”"; font.bold: true; width: parent.width }
            GField { id: renameField; width: parent.width; tip: "The new name"
                onAccepted: { const t = renamePop.target; renamePop.close(); if (t && text && text !== t.name) aw.change({ rename: [[t.path, text]] }) }
                Keys.onEscapePressed: renamePop.close() }
            Row {
                anchors.right: parent.right; spacing: 8
                PushButton { text: "Cancel"; tip: "Keep the old name"; onClicked: renamePop.close() }
                PushButton { text: "Rename"; primary: true; tip: "Rename it inside the archive"; onClicked: renameField.accepted() }
            }
        }
    }
    Popup {
        id: confirmDel
        anchors.centerIn: parent
        width: 360
        padding: 16; topPadding: 16.001; leftPadding: 16.001
        modal: true
        popupType: Popup.Item
        background: GlassPanel { radius: Theme.glassRadius; tint: Theme.glassMenu }
        Column {
            spacing: 10
            width: parent.width
            GLabel { width: parent.width; wrapMode: Text.Wrap; elide: Text.ElideNone; font.bold: true
                text: aw.pendingDelete.length === 1 ? "Delete “" + aw.name(aw.pendingDelete[0]) + "” from the archive?" : "Delete " + aw.pendingDelete.length + " items from the archive?" }
            GLabel { width: parent.width; wrapMode: Text.Wrap; elide: Text.ElideNone; font.pixelSize: Theme.small; color: Theme.fgSecondary
                text: "They're removed from the archive only (not put in the Trash). This can't be undone." }
            Row {
                anchors.right: parent.right; spacing: 8
                PushButton { text: "Cancel"; tip: "Keep them"; onClicked: confirmDel.close() }
                PushButton { text: "Delete"; destructive: true; tip: "Remove them from the archive"; onClicked: { confirmDel.close(); aw.doDelete() } }
            }
        }
    }

    // Tests: GROVE_ARC_TEST "pw=x;enter=docs;select=a.md;info;help".
    Timer {
        interval: 900
        running: (aw.app.grove.env("GROVE_ARC_TEST") || "") !== "" && aw.path !== ""
        onTriggered: {
            for (const st of aw.app.grove.env("GROVE_ARC_TEST").split(";")) {
                const [k, v] = st.split("=")
                if (k === "enter") aw.enter(v)
                else if (k === "expand") aw.toggle(v, true, false)
                else if (k === "expandall") aw.expandAll(true)
                else if (k === "select") { for (let i = 0; i < aw.rows.length; i++) if (aw.rows[i].name === v) aw.select(i, "toggle") }
                else if (k === "info") aw.showInfo = true
                else if (k === "menu") rowMenu.popup(200, 160)
            }
        }
    }
    Timer {
        interval: 300
        running: aw.errorCode === "needPassword" && (aw.app.grove.env("GROVE_ARC_TEST") || "").indexOf("pw=") >= 0
        onTriggered: { const m = aw.app.grove.env("GROVE_ARC_TEST").match(/pw=([^;]*)/); if (m) { aw.password = m[1]; aw.load() } }
    }
}
