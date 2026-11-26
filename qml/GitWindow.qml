import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window

// Git Changes for one repository: the branch (switch, make one), how far
// it is ahead of or behind its upstream (Fetch, Pull, Push), and its files
// by state: Staged, Changes (not staged), Untracked, Conflicts, and Ignored
// when asked. Click a file to see its diff (the delta-style view: unified
// or side by side); stage, unstage or discard it from its row or by
// right-clicking; write a message and commit what's staged. libgit2 does
// the work (src/gitrepo.rs), never on the UI thread.
Window {
    id: gw
    property var app
    property var win              // the browser window (Show in Folder)
    property string dir: ""
    property var ov: ({})
    property bool loading: false
    property bool busy: false
    property bool showIgnored: false
    // The file shown: its path, its list, and the diff file made for it.
    property string selRel: ""
    property string selWhich: ""
    property string diffPath: ""
    property string diffNote: ""
    readonly property string root: ov.root || ""
    readonly property string repoName: root.substring(root.lastIndexOf("/") + 1)
    readonly property int stagedCount: (ov.staged || []).length

    width: Math.round(1180 * Theme.pt)
    height: Math.round(780 * Theme.pt)
    minimumWidth: 640
    minimumHeight: 420
    color: Theme.window
    title: "Changes — " + (repoName || "git") + (ov.branch ? " (" + ov.branch + ")" : "")

    Component.onCompleted: refresh()
    onActiveChanged: if (active) refresh()
    // Changes made elsewhere (an editor, a terminal) show up on their own.
    Timer { interval: 4000; repeat: true; running: gw.active && gw.visible && !gw.busy; onTriggered: gw.refresh() }

    function refresh() {
        if (!dir || loading) return
        loading = true
        Async.call(app.grove, k => app.grove.gitOverview(k, dir, showIgnored), r => {
            loading = false
            const before = JSON.stringify(gw.ov)
            gw.ov = r || {}
            // The shown file: its diff again if anything changed, else
            // the first file if it's gone from its list.
            if (selRel && !listOf(selWhich).some(f => f.path === selRel)) {
                const other = ["staged", "changed", "untracked", "conflicts"].find(w => listOf(w).some(f => f.path === selRel))
                if (other) selWhich = other; else { selRel = ""; diffPath = ""; diffNote = "" }
            }
            if (!selRel) { const f = firstFile(); if (f) showFile(f.path, f.which); return }
            if (JSON.stringify(gw.ov) !== before) loadDiff()
        })
    }
    onShowIgnoredChanged: refresh()
    function listOf(w) { return (ov[w] || []) }
    function firstFile() {
        for (const w of ["conflicts", "changed", "staged", "untracked"]) { const l = listOf(w); if (l.length) return { path: l[0].path, which: w } }
        return null
    }
    function showFile(rel, which) { selRel = rel; selWhich = which; loadDiff() }
    function loadDiff() {
        if (!selRel || selWhich === "ignored") { diffPath = ""; diffNote = selWhich === "ignored" ? "Ignored by .gitignore: git doesn't track it." : ""; return }
        const rel = selRel, w = selWhich
        Async.call(app.grove, k => app.grove.gitFileDiff(k, root || dir, rel, w), r => {
            if (rel !== gw.selRel || w !== gw.selWhich) return
            diffNote = r.error ? r.error : r.empty ? "No changes to show (a binary file, or only its mode changed)." : ""
            diffPath = r.path || ""
        })
    }

    function act(action, rels, arg, done) {
        busy = true
        Async.call(app.grove, k => app.grove.gitChange(k, root || dir, action, JSON.stringify(rels || []), arg || ""), r => {
            busy = false
            if (r.error) toast.show("error", r.error)
            else if (r.output && action !== "stage" && action !== "unstage") toast.show("info", r.output)
            if (done) done(r)
            refresh()
            if (win) win.forEachPane(p => { if (root && p.path.indexOf(root) === 0) p.reload() })
        })
    }
    // Discarding asks first: what's not staged is lost.
    function askDiscard(rels) { discardSheet.rels = rels; discardSheet.open() }

    // Tests (GROVE_TEST gitwin=...): "dump", "stage:a.txt", "commit:msg"...
    function testAction(a) {
        const [c, ...rest] = a.split(":"), v = rest.join(":")
        if (c === "stage") act("stage", [v])
        else if (c === "unstage") act("unstage", [v])
        else if (c === "show") { const w = ["changed", "staged", "untracked", "conflicts"].find(x => listOf(x).some(f => f.path === v)); if (w) showFile(v, w) }
        else if (c === "commit") { msg.text = v; commit() }
        else if (c === "switch") act("switch", [], v)
        else if (c === "branchmenu") branchMenu.popup(branchBtn, 0, branchBtn.height)
        else if (c === "ignored") showIgnored = true
        console.warn("grove test git", a, JSON.stringify({ branch: ov.branch, staged: listOf("staged").map(f => f.code + " " + f.path), changed: listOf("changed").map(f => f.code + " " + f.path), untracked: listOf("untracked").map(f => f.path), ignored: listOf("ignored").map(f => f.path), sel: selRel, which: selWhich, diff: diffPath !== "", note: diffNote }))
    }

    function commit() {
        if (msg.text.trim() === "") { toast.show("error", "Write a message saying what the change does."); msg.forceActiveFocus(); return }
        act("commit", [], JSON.stringify({ message: msg.text, amend: amend.checked }), r => { if (r.ok) { msg.text = ""; amend.checked = false } })
    }

    Shortcut { sequence: "Ctrl+Return"; onActivated: gw.commit() }
    Shortcut { sequence: "F5"; onActivated: gw.refresh() }
    Shortcut { sequence: "Ctrl+W"; onActivated: gw.close() }

    Item {
        anchors.fill: parent
        transform: Translate { x: 0.001; y: 0.001 }

        // ---- toolbar: branch, upstream, Fetch / Pull / Push
        Rectangle {
            id: bar
            width: parent.width
            height: Math.round(46 * Theme.pt)
            color: Theme.toolbar
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.separator }
            Row {
                x: 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 10
                PushButton {
                    id: branchBtn
                    anchors.verticalCenter: parent.verticalCenter
                    text: "⎇  " + (gw.ov.branch || "…") + (gw.ov.detached ? " (detached)" : "") + "  ▾"
                    tip: "The branch you're on: click to switch to another or make a new one"
                    onClicked: branchMenu.popup(branchBtn, 0, branchBtn.height + 4)
                }
                GLabel {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: !!gw.ov.upstream
                    text: (gw.ov.ahead ? "↑" + gw.ov.ahead + "  " : "") + (gw.ov.behind ? "↓" + gw.ov.behind + "  " : "") + (gw.ov.ahead || gw.ov.behind ? "" : "Up to date with ") + (gw.ov.upstream || "")
                    color: Theme.fgSecondary
                    font.pixelSize: Theme.small
                    GTip { visible: upHover.hovered; text: "↑ commits here not on " + gw.ov.upstream + " yet (Push sends them); ↓ commits there not here yet (Pull brings them). Fetch to check." }
                    HoverHandler { id: upHover }
                }
                GLabel {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: !gw.ov.upstream && !!gw.ov.branch && !gw.ov.detached
                    text: "Not on any remote yet"
                    color: Theme.fgSecondary
                    font.pixelSize: Theme.small
                }
            }
            Row {
                anchors.right: parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                BusyDots { visible: gw.busy; anchors.verticalCenter: parent.verticalCenter }
                GCheck { text: "Show Ignored"; checked: gw.showIgnored; onToggled: gw.showIgnored = checked; tip: "Also list what .gitignore leaves out (build output and the like)"; anchors.verticalCenter: parent.verticalCenter }
                GButton { iconName: "view-refresh"; tip: "Refresh (F5)"; onClicked: gw.refresh(); anchors.verticalCenter: parent.verticalCenter }
                PushButton { text: "Fetch"; enabled: !gw.busy; tip: "See what's new on the remotes, without changing your files"; onClicked: gw.act("fetch", [], ""); anchors.verticalCenter: parent.verticalCenter }
                PushButton { text: "Pull" + (gw.ov.behind ? " ↓" + gw.ov.behind : ""); enabled: !gw.busy && !!gw.ov.upstream; tip: "Bring in the commits from " + (gw.ov.upstream || "the remote") + " (fast-forward only)"; onClicked: gw.act("pull", [], ""); anchors.verticalCenter: parent.verticalCenter }
                PushButton { text: "Push" + (gw.ov.ahead ? " ↑" + gw.ov.ahead : ""); enabled: !gw.busy && !!gw.ov.branch && !gw.ov.detached; tip: "Send your commits to " + (gw.ov.upstream || "the remote"); onClicked: gw.act("push", [], ""); anchors.verticalCenter: parent.verticalCenter }
            }
        }

        // A merge or rebase in progress.
        Rectangle {
            id: stateBar
            anchors.top: bar.bottom
            width: parent.width
            height: visible ? 30 : 0
            visible: !!gw.ov.state || !!gw.ov.error
            color: Qt.rgba(1, 0.62, 0.04, Theme.dark ? 0.16 : 0.14)
            GLabel { x: 12; anchors.verticalCenter: parent.verticalCenter; text: gw.ov.error || (gw.ov.state + " Finish or abort it in a terminal; resolve conflicts, then stage the files.") }
        }

        // ---- the files (left) and the diff (right)
        SplitView {
            anchors.top: stateBar.bottom
            anchors.bottom: parent.bottom
            width: parent.width
            orientation: Qt.Horizontal
            handle: Rectangle { implicitWidth: 1; color: Theme.separator }

            Item {
                id: side
                SplitView.preferredWidth: Math.round(360 * Theme.pt)
                SplitView.minimumWidth: 240

                ListView {
                    id: files
                    anchors.top: parent.top
                    anchors.bottom: commitBox.top
                    width: parent.width
                    clip: true
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: GScrollBar {}
                    readonly property var sections: [
                        { which: "conflicts", title: "Conflicts", tip: "Changed on both sides of a merge: fix the file, then stage it" },
                        { which: "staged", title: "Staged Changes", tip: "What the next commit will hold" },
                        { which: "changed", title: "Changes", tip: "Changed since staged or committed, not staged yet" },
                        { which: "untracked", title: "Untracked", tip: "New files git isn't tracking yet" },
                        { which: "ignored", title: "Ignored", tip: "Left out by .gitignore" }
                    ]
                    model: {
                        const out = []
                        for (const s of sections) {
                            const l = gw.listOf(s.which)
                            if (!l.length) continue
                            out.push({ header: true, which: s.which, title: s.title, tip: s.tip, count: l.length })
                            for (const f of l) out.push({ header: false, which: s.which, path: f.path, code: f.code, old: f.old || "" })
                        }
                        return out
                    }
                    delegate: Item {
                        id: fr
                        required property var modelData
                        required property int index
                        readonly property var d: modelData
                        readonly property bool selected: !d.header && d.path === gw.selRel && d.which === gw.selWhich
                        readonly property string name: d.header ? "" : d.path.replace(/\/$/, "").split("/").pop() + (d.path.endsWith("/") ? "/" : "")
                        readonly property string folder: d.header ? "" : d.path.replace(/\/$/, "").split("/").slice(0, -1).join("/")
                        width: files.width
                        height: d.header ? Math.round(32 * Theme.pt) : Math.round(26 * Theme.pt)

                        // Section header with its all-at-once buttons.
                        Item {
                            visible: fr.d.header
                            anchors.fill: parent
                            GLabel {
                                x: 12
                                anchors.bottom: parent.bottom; anchors.bottomMargin: 6
                                text: fr.d.title + "  " + fr.d.count
                                font.bold: true
                                font.pixelSize: Theme.small
                                color: Theme.fgSecondary
                                HoverHandler { id: hh }
                                GTip { visible: hh.hovered; text: fr.d.tip || "" }
                            }
                            Row {
                                anchors.right: parent.right; anchors.rightMargin: 8
                                anchors.bottom: parent.bottom; anchors.bottomMargin: 2
                                GButton { visible: fr.d.which === "staged"; iconName: "list-remove"; iconSize: 14; tip: "Unstage everything"; onClicked: gw.act("unstage", gw.listOf("staged").map(f => f.path)) }
                                GButton { visible: fr.d.which === "changed" || fr.d.which === "untracked" || fr.d.which === "conflicts"; iconName: "list-add"; iconSize: 14; tip: "Stage all of these"; onClicked: gw.act("stage", gw.listOf(fr.d.which).map(f => f.path)) }
                            }
                        }

                        // A file.
                        Rectangle {
                            visible: !fr.d.header
                            anchors.fill: parent
                            anchors.leftMargin: 6; anchors.rightMargin: 6
                            radius: 5
                            color: fr.selected ? Theme.accent : rowHover.hovered ? Theme.hover : "transparent"
                            HoverHandler { id: rowHover }
                            GitBadge {
                                id: badge
                                x: 6
                                anchors.verticalCenter: parent.verticalCenter
                                status: fr.d.header ? "" : (fr.d.code === "?" ? "?" : fr.d.code === "U" ? "U" : fr.d.code === "I" ? "I" : fr.d.code === "D" ? "D" : fr.d.code === "A" ? "A" : fr.d.code === "R" ? "R" : "M")
                            }
                            GLabel {
                                id: nameT
                                anchors.left: badge.right; anchors.leftMargin: 8
                                anchors.verticalCenter: parent.verticalCenter
                                width: Math.min(implicitWidth, parent.width - x - 8 - (rowButtons.visible ? rowButtons.width : 0))
                                text: fr.name
                                color: fr.selected ? Theme.accentText : Theme.fg
                                font.strikeout: fr.d.code === "D"
                            }
                            GLabel {
                                anchors.left: nameT.right; anchors.leftMargin: 6
                                anchors.verticalCenter: parent.verticalCenter
                                width: Math.max(0, parent.width - x - 8 - (rowButtons.visible ? rowButtons.width : 0))
                                text: (fr.d.old ? "← " + fr.d.old + "  " : "") + fr.folder
                                elide: Text.ElideLeft
                                font.pixelSize: Theme.small
                                color: fr.selected ? Theme.accentText : Theme.fgSecondary
                            }
                            Row {
                                id: rowButtons
                                visible: rowHover.hovered || fr.selected
                                anchors.right: parent.right; anchors.rightMargin: 2
                                anchors.verticalCenter: parent.verticalCenter
                                GButton { visible: fr.d.which === "changed" || fr.d.which === "untracked"; iconName: "edit-undo"; iconSize: 14; tint: fr.selected ? Theme.accentText : Theme.fg; tip: fr.d.which === "untracked" ? "Discard: move this new file to the Trash" : "Discard this change (back to how it's staged or committed)"; onClicked: gw.askDiscard([fr.d.path]) }
                                GButton { visible: fr.d.which === "staged"; iconName: "list-remove"; iconSize: 14; tint: fr.selected ? Theme.accentText : Theme.fg; tip: "Unstage (keep the change, leave it out of the next commit)"; onClicked: gw.act("unstage", [fr.d.path]) }
                                GButton { visible: fr.d.which !== "staged" && fr.d.which !== "ignored"; iconName: "list-add"; iconSize: 14; tint: fr.selected ? Theme.accentText : Theme.fg; tip: "Stage (put it in the next commit)"; onClicked: gw.act("stage", [fr.d.path]) }
                            }
                            TapHandler {
                                acceptedButtons: Qt.LeftButton | Qt.RightButton
                                onTapped: (pt, button) => {
                                    gw.showFile(fr.d.path, fr.d.which)
                                    if (button === Qt.RightButton) { fileMenu.d = fr.d; fileMenu.popup() }
                                }
                                onDoubleTapped: app.grove.openPaths(JSON.stringify([gw.root + "/" + fr.d.path]))
                            }
                        }
                    }
                }

                GLabel {
                    anchors.centerIn: files
                    visible: files.count === 0 && !gw.loading && !gw.ov.error && !!gw.ov.root
                    text: "No changes: everything's committed."
                    color: Theme.fgSecondary
                }

                // ---- commit
                Rectangle {
                    id: commitBox
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: commitCol.height + 20
                    color: Theme.toolbar
                    Rectangle { width: parent.width; height: 1; color: Theme.separator }
                    Column {
                        id: commitCol
                        x: 10; y: 10
                        width: parent.width - 20
                        spacing: 8
                        Rectangle {
                            width: parent.width
                            height: Math.round(78 * Theme.pt)
                            radius: 6
                            color: Theme.field
                            border.width: msg.activeFocus ? 2 : 0
                            border.color: Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.6)
                            ScrollView {
                                anchors.fill: parent
                                anchors.margins: 2
                                TextArea {
                                    id: msg
                                    placeholderText: "Commit message (Ctrl+Enter commits)"
                                    placeholderTextColor: Theme.fgSecondary
                                    wrapMode: TextEdit.Wrap
                                    color: Theme.fg
                                    selectionColor: Theme.accent
                                    selectedTextColor: Theme.accentText
                                    renderType: Text.NativeRendering
                                    background: null
                                    font.pixelSize: Theme.body
                                }
                            }
                        }
                        Item {
                            width: parent.width
                            height: commitBtn.height
                            GCheck {
                                id: amend
                                anchors.verticalCenter: parent.verticalCenter
                                text: "Amend last commit"
                                tip: "Replace the last commit with this one (what's staged added to it, and this message)"
                                onToggled: if (checked && msg.text === "") gw.act("lastmessage", [], "", r => { if (r.message) msg.text = r.message.trim() })
                            }
                            PushButton {
                                id: commitBtn
                                anchors.right: parent.right
                                primary: true
                                enabled: !gw.busy && (gw.stagedCount > 0 || amend.checked)
                                text: amend.checked ? "Amend" : "Commit " + gw.stagedCount + (gw.stagedCount === 1 ? " File" : " Files")
                                tip: gw.stagedCount === 0 && !amend.checked ? "Stage the changes to commit first (the + on a file)" : "Commit what's staged, with this message (Ctrl+Enter)"
                                onClicked: gw.commit()
                            }
                        }
                    }
                }
            }

            Item {
                SplitView.fillWidth: true
                DocView {
                    id: diffView
                    anchors.fill: parent
                    visible: gw.diffPath !== ""
                    active: visible
                    mode: "code"
                    path: gw.diffPath
                }
                GLabel {
                    anchors.centerIn: parent
                    width: Math.min(implicitWidth, parent.width - 40)
                    visible: gw.diffPath === ""
                    wrapMode: Text.Wrap
                    horizontalAlignment: Text.AlignHCenter
                    text: gw.diffNote || (gw.selRel ? "" : "Pick a file to see its changes.")
                    color: Theme.fgSecondary
                }
            }
        }
    }

    // ---- menus and sheets
    GMenu {
        id: branchMenu
        Instantiator {
            model: gw.ov.branches || []
            delegate: GMenuItem {
                required property var modelData
                text: modelData.name
                checkable: true
                checked: !!modelData.current
                tip: modelData.summary ? "Last commit: " + modelData.summary : ""
                enabled: !modelData.current && !gw.busy
                onTriggered: gw.act("switch", [], modelData.name)
            }
            onObjectAdded: (i, o) => branchMenu.insertItem(i, o)
            onObjectRemoved: (i, o) => branchMenu.removeItem(o)
        }
        GMenuSeparator {}
        GMenu {
            id: remoteMenu
            title: "Remote Branches"
            enabled: (gw.ov.remotes || []).length > 0
            Instantiator {
                model: (gw.ov.remotes || []).slice(0, 60)
                delegate: GMenuItem {
                    required property var modelData
                    text: modelData.name
                    tip: "Switch to a local branch following " + modelData.name + (modelData.summary ? " (last commit: " + modelData.summary + ")" : "")
                    onTriggered: gw.act("switch", [], modelData.name)
                }
                onObjectAdded: (i, o) => remoteMenu.insertItem(i, o)
                onObjectRemoved: (i, o) => remoteMenu.removeItem(o)
            }
        }
        GMenuItem { text: "New Branch…"; iconName: "list-add"; tip: "Make a branch from where you are and switch to it (your changes come along)"; onTriggered: { newBranchName.text = ""; newBranchSheet.open() } }
    }

    GMenu {
        id: fileMenu
        property var d: ({})
        GMenuItem { visible: fileMenu.d.which !== "staged" && fileMenu.d.which !== "ignored"; text: "Stage"; iconName: "list-add"; onTriggered: gw.act("stage", [fileMenu.d.path]) }
        GMenuItem { visible: fileMenu.d.which === "staged"; text: "Unstage"; iconName: "list-remove"; onTriggered: gw.act("unstage", [fileMenu.d.path]) }
        GMenuItem { visible: fileMenu.d.which === "changed" || fileMenu.d.which === "untracked"; text: fileMenu.d.which === "untracked" ? "Move to Trash…" : "Discard Changes…"; iconName: "edit-undo"; onTriggered: gw.askDiscard([fileMenu.d.path]) }
        GMenuSeparator {}
        GMenuItem { text: "Open"; iconName: "document-open"; enabled: fileMenu.d.code !== "D"; onTriggered: app.grove.openPaths(JSON.stringify([gw.root + "/" + fileMenu.d.path])) }
        GMenuItem {
            text: "Show in Folder"; iconName: "folder-open"
            enabled: !!gw.win
            onTriggered: {
                const p = gw.root + "/" + (fileMenu.d.path || "").replace(/\/$/, "")
                gw.win.activePane.navigate(p.substring(0, p.lastIndexOf("/")), { select: [p] })
                gw.win.raise(); gw.win.requestActivate()
            }
        }
        GMenuItem { text: "Copy Path"; iconName: "edit-copy"; onTriggered: app.grove.copyText(gw.root + "/" + fileMenu.d.path) }
    }

    Sheet {
        id: discardSheet
        property var rels: []
        readonly property bool untracked: rels.length > 0 && rels.every(r => gw.listOf("untracked").some(f => f.path === r))
        title: untracked ? "Move “" + (rels[0] || "") + "” to the Trash?" : "Discard your changes to “" + (rels[0] || "") + "”?"
        message: untracked ? "It's a new file git doesn't track; it goes to the Trash, so you can get it back from there."
                           : "The file goes back to how it's staged (or last committed). Changes that aren't staged are lost; this can't be undone."
        buttons: [
            PushButton { text: "Cancel"; onClicked: discardSheet.close() },
            PushButton { text: discardSheet.untracked ? "Move to Trash" : "Discard"; destructive: true; primary: true; onClicked: { gw.act("discard", discardSheet.rels); discardSheet.close() } }
        ]
    }

    Sheet {
        id: newBranchSheet
        title: "New Branch"
        message: "Made from " + (gw.ov.branch || "here") + " as it is now; you switch to it, and your uncommitted changes come along."
        onOpened: newBranchName.forceActiveFocus()
        GField {
            id: newBranchName
            width: parent ? parent.width : 300
            placeholderText: "Name (like fix-login)"
            tip: "No spaces; letters, digits, - _ / and ."
            onAccepted: makeBranch.clicked()
        }
        buttons: [
            PushButton { text: "Cancel"; onClicked: newBranchSheet.close() },
            PushButton { id: makeBranch; text: "Create and Switch"; primary: true; enabled: newBranchName.text.trim() !== ""; onClicked: { gw.act("newbranch", [], newBranchName.text.trim()); newBranchSheet.close() } }
        ]
    }

    Toast { id: toast; anchors.bottom: parent.bottom; anchors.bottomMargin: 20; anchors.horizontalCenter: parent.horizontalCenter }
}
