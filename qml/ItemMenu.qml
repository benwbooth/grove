import QtQuick
import QtQuick.Controls.Basic

// The right-click menu for selected items.
GMenu {
    id: m
    property var pane
    property var target: ({})
    property var sel: []
    property var apps: []
    property var services: []
    // A program or script (executable, #!, .sh, .py...): Run in Terminal.
    property bool runnable: false
    readonly property bool one: sel.length === 1
    // A long name in a menu item: its start and end, "…" between.
    function shortName(n) { n = n || ""; return n.length <= 32 ? n : n.substring(0, 20) + "…" + n.substring(n.length - 10) }
    readonly property bool trash: pane.inTrash
    readonly property bool isImage: (target.mime || "").indexOf("image/") === 0
    readonly property bool isArchive: pane.archiveLike(target)
    readonly property bool isDiskImage: /\.(iso|img|dmg)$/i.test(target.name || "")
    readonly property bool split: pane.win.currentTab && pane.win.currentTab.split
    // Google Drive (canopy): the synced folder holding the item, its badge,
    // and whether it's starred (asked for when the menu opens).
    readonly property string cloudRoot: m.one && (target.path || "").indexOf("/") === 0 ? pane.cloudRootOf(target.path) : ""
    readonly property bool inCloud: cloudRoot !== "" && target.path !== cloudRoot
    readonly property string cloudCode: (target.cloud || "").split("/")[0]
    property var cloudItem: null
    readonly property bool canopyUp: !!pane.grove && pane.grove.canopyAvailable
    function driveCall(action, arg, fn) {
        const p = m.target.path
        Async.call(m.pane.grove, k => m.pane.grove.cloudAction(k, action, p, arg || ""), r => {
            if (r && r.error) m.pane.win.toast("error", r.error); else if (fn) fn(r || {})
        })
    }
    function copyDriveLink() { driveCall("link", "", r => { if (r.url) { m.pane.grove.copyText(r.url); m.pane.win.toast("info", "Link to “" + (r.name || m.target.name) + "” copied.") } }) }
    function viewOnDrive() { driveCall("link", "", r => { if (r.url) m.pane.grove.openUrl(r.url) }) }
    function setStar(on) { driveCall("star", on ? "1" : "0", () => m.pane.win.toast("info", (on ? "Starred “" : "Unstarred “") + m.target.name + "” in Google Drive.")) }
    function unsync() {
        const name = m.target.name, pane = m.pane, p = m.target.path
        pane.win.confirm("Remove “" + name + "” from this computer?",
                         "It stays in Google Drive and stops syncing here. The copy on this computer goes to the Trash, so you can put it back.",
                         "Remove", () => Async.call(pane.grove, k => pane.grove.cloudAction(k, "unsync", p, ""), r => {
                             if (r && r.error) pane.win.toast("error", r.error)
                             else { pane.win.toast("info", "“" + name + "” is only in Google Drive now."); pane.cloudBadges() }
                         }), true)
    }
    onAboutToShow: {
        cloudItem = null
        if (inCloud) driveCall("file_status", "", r => m.cloudItem = r.item || null)
        // The Google Drive submenu only shows inside a synced folder.
        for (let i = 0; i < count; i++) {
            const it = itemAt(i)
            if (it && it.subMenu === driveMenu) { it.visible = m.inCloud; it.height = m.inCloud ? it.implicitHeight : 0 }
        }
    }

    GMenuItem {
        text: m.one ? "Restore “" + m.target.name + "”" : "Restore " + m.sel.length + " Items"
        iconName: "edit-undo|document-revert"
        visible: !!m.pane.snapInfo
        height: visible ? implicitHeight : 0
        tip: "Copy back to where it is today (" + (m.pane.snapInfo ? m.pane.snapInfo.live : "") + "); if it's still there you choose to replace it or keep both"
        onTriggered: m.pane.restoreFromSnapshot(m.sel)
    }
    GMenuItem { text: m.target.isDir ? "Open" : "Open"; iconName: "document-open"; shortcutText: Help.shown("Return"); enabled: !m.trash; onTriggered: m.pane.openSelected(false) }
    GMenuItem {
        text: "Run in Terminal"
        iconName: "utilities-terminal"
        visible: m.runnable && m.one && !m.trash
        height: visible ? implicitHeight : 0
        tip: "Run “" + (m.target.name || "") + "” in a terminal (a new tab of the open Tessera window), in its folder; the tab stays open afterwards"
        onTriggered: m.pane.grove.runInTerminal(m.target.path)
    }
    GMenuItem { text: "Open in New Tab"; iconName: "tab-new"; visible: !!m.target.isDir; height: visible ? implicitHeight : 0; shortcutText: Help.shown("Ctrl+Return"); onTriggered: m.pane.openSelected(true) }
    GMenu {
        id: openWith
        title: "Open With"
        enabled: m.one && !m.target.isDir && !m.trash
        Instantiator {
            model: m.apps
            delegate: GMenuItem {
                required property var modelData
                text: modelData.name + (modelData.isDefault ? "  (default)" : "")
                iconName: modelData.icon
                onTriggered: m.pane.grove.openWith(JSON.stringify(m.sel), modelData.id)
            }
            onObjectAdded: (i, o) => openWith.insertItem(i, o)
            onObjectRemoved: (i, o) => openWith.removeItem(o)
        }
        GMenuSeparator {}
        GMenuItem { text: "Other Application…"; iconName: "system-run|application-x-executable"; tip: "Choose any installed app (all of them, with a search), and optionally make it the default"; onTriggered: m.pane.win.openWithDialog(m.target.path) }
    }
    GMenuItem { text: "Quick Look"; iconName: "quickview"; shortcutText: Help.shown("Space"); onTriggered: m.pane.quickLook() }
    GMenuItem { text: "Get Info"; iconName: "documentinfo"; shortcutText: Help.shown("Alt+Return"); onTriggered: m.pane.win.showInfo(m.sel) }
    GMenuItem { text: "Analyze Disk Usage"; iconName: "filelight|baobab|drive-harddisk"; visible: m.one && !!m.target.isDir && (m.target.path || "").indexOf("/") === 0; height: visible ? implicitHeight : 0; tip: "What takes the room in this folder"; onTriggered: m.pane.win.showDiskUsage(m.target.path) }
    GMenuItem { text: "Show Enclosing Folder"; iconName: "folder-open"; visible: m.pane.searching || m.pane.path.indexOf("recent:") === 0; height: visible ? implicitHeight : 0; onTriggered: m.pane.showEnclosing() }
    GMenuSeparator {}
    GMenuItem { text: "Put Back"; iconName: "edit-undo"; visible: m.trash; height: visible ? implicitHeight : 0; onTriggered: m.pane.putBack() }
    GMenuItem { text: m.one ? "Rename…" : "Rename " + m.sel.length + " Items…"; iconName: "edit-rename"; shortcutText: Help.shown("F2"); enabled: !m.pane.special; onTriggered: m.pane.rename() }
    GMenuItem { text: "Duplicate"; iconName: "edit-copy"; shortcutText: Help.shown("Ctrl+D"); enabled: !m.pane.special; onTriggered: m.pane.duplicate() }
    GMenuItem { text: "Make Alias"; iconName: "insert-link"; shortcutText: Help.shown("Ctrl+Alt+A"); enabled: !m.pane.special; onTriggered: m.pane.makeAlias() }
    GMenuItem { text: "Extract Here"; iconName: "archive-extract"; visible: m.isArchive; height: visible ? implicitHeight : 0
        tip: "Into a folder named after the archive if it holds more than one item; else right here"; onTriggered: m.pane.extract() }
    GMenuItem { text: "Extract To…"; iconName: "archive-extract|folder-open"; visible: m.isArchive; height: visible ? implicitHeight : 0
        tip: "Pick where to extract it (a folder named after the archive is made there if it holds more than one item)"; onTriggered: m.pane.extractTo() }
    GMenu {
        // 7-Zip's context menu, from Grove Archiver.
        id: arcMenu
        title: "Archive"
        enabled: !m.pane.special && (m.target.path || "").indexOf("/") === 0
        readonly property var ar: m.pane.win.app.archiver
        readonly property string stem: m.one ? (m.target.name || "") : "Archive"
        GMenuItem { text: "Open in Grove Archiver"; iconName: "package-x-generic"; enabled: m.isArchive && m.one; tip: "See what's inside with 7-Zip's columns (packed size, CRC, method), add, delete, rename, test"; onTriggered: arcMenu.ar.openWindow(m.target.path) }
        GMenuItem { text: "Extract Files…"; iconName: "archive-extract"; enabled: m.isArchive; tip: "Where to, folder or not, files already there, password"; onTriggered: { for (const p of m.sel) arcMenu.ar.extractDialog(p, {}) } }
        GMenuItem { text: "Extract to “" + (m.one ? m.target.name.replace(/(\.tar)?\.[^.]+$/, "") : "…") + "/”"; iconName: "archive-extract|folder"; enabled: m.isArchive; tip: "Into a new folder named after the archive"; onTriggered: { for (const p of m.sel) arcMenu.ar.extractToFolder(p) } }
        GMenuItem { text: "Test Archive"; iconName: "dialog-ok-apply"; enabled: m.isArchive; tip: "Check every file against its checksum, to find damage"; onTriggered: { for (const p of m.sel) arcMenu.ar.test(p, "") } }
        GMenuSeparator {}
        GMenuItem { text: "Add to Archive…"; iconName: "archive-insert"; tip: "Format, compression level, password, split into volumes"; onTriggered: arcMenu.ar.compressDialog(m.sel) }
        GMenuItem { text: "Add to “" + arcMenu.stem + ".7z”"; iconName: "archive-insert"; tip: "Right away, with the default settings"; onTriggered: arcMenu.ar.quickCompress(m.sel, "7z") }
        GMenuItem { text: "Add to “" + arcMenu.stem + ".zip”"; iconName: "archive-insert"; tip: "Right away; opens on any computer"; onTriggered: arcMenu.ar.quickCompress(m.sel, "zip") }
    }
    GMenuItem { text: "Open Disk Image"; iconName: "media-optical"; visible: m.isDiskImage && m.one; height: visible ? implicitHeight : 0; tip: "Attach the image (ISO, IMG) read-only and open it like a disk"; onTriggered: m.pane.win.mountImage(m.target.path) }
    GMenuItem { text: "Compare"; iconName: "kompare|document-compare|view-split-left-right"; visible: m.sel.length === 2; height: visible ? implicitHeight : 0; tip: "Show the differences between the two items"; onTriggered: m.pane.win.compareFiles(m.sel[0], m.sel[1]) }
    GMenuItem { text: "Copy to Other Side"; iconName: "edit-copy"; visible: m.split; height: visible ? implicitHeight : 0; shortcutText: Help.shown("Shift+F5"); onTriggered: m.pane.win.toOtherPane(false) }
    GMenuItem { text: "Move to Other Side"; iconName: "go-next"; visible: m.split; height: visible ? implicitHeight : 0; shortcutText: Help.shown("Shift+F6"); onTriggered: m.pane.win.toOtherPane(true) }
    GMenuItem { text: "Take Snapshot…"; iconName: "camera-photo"; visible: m.one && !!m.target.subvol; height: visible ? implicitHeight : 0; tip: "An instant copy of this btrfs " + (m.target.subvol || "").toLowerCase() + " as it is now (read-only by default)"; onTriggered: m.pane.win.snapshotSubvolume(m.target.path) }
    GMenuItem { text: "Restore from This Snapshot…"; iconName: "document-revert"; visible: m.one && (m.target.subvol || "").indexOf("Snapshot") >= 0 && m.target.subvol !== "Snapper Snapshot"; height: visible ? implicitHeight : 0; tip: "Put the subvolume this snapshot was taken of back the way it was then (what's in it now is kept aside)"; onTriggered: m.pane.win.restoreSubvolume(m.target.path) }
    GMenuItem { text: "Browse Previous Versions…"; iconName: "document-open-recent"; enabled: m.one && !m.pane.special; tip: "Earlier copies from the hourly snapshots; show or restore them"; onTriggered: m.pane.win.showVersions(m.target.path) }
    GMenuItem { text: m.one ? "Burn “" + m.shortName(m.target.name) + "” to Disc…" : "Burn " + m.sel.length + " Items to Disc…"; iconName: "media-optical-burn|media-optical"; enabled: !m.pane.special; visible: !!m.pane.app && m.pane.app.hasBurner; height: visible ? implicitHeight : 0; tip: "Put these on a CD, DVD or Blu-ray (or a disc image file)"; onTriggered: m.pane.win.burn(m.sel) }
    GMenuItem { text: "Share with Nearby Device…"; iconName: "document-share|send-to|network-wireless"; enabled: !m.pane.special && !m.target.isDir; tip: "Send to a phone or computer running LocalSend (like AirDrop)"; onTriggered: m.pane.win.share(m.sel) }
    GMenuSeparator {}
    GMenuItem { text: "Copy"; iconName: "edit-copy"; shortcutText: Help.shown("Ctrl+C"); onTriggered: m.pane.copy(false) }
    GMenuItem { text: "Cut"; iconName: "edit-cut"; shortcutText: Help.shown("Ctrl+X"); enabled: !m.pane.special || m.trash; onTriggered: m.pane.copy(true) }
    GMenuItem { text: "Copy Path"; iconName: "edit-copy-path"; shortcutText: Help.shown("Ctrl+Alt+C"); onTriggered: m.pane.copyPath() }
    GMenuSeparator {}
    // Tags: Finder's colour dots plus "Tags…".
    MenuItem {
        id: tagRow
        implicitHeight: Theme.menuRow + 4
        enabled: !m.pane.special
        background: Item {}
        contentItem: Row {
            spacing: 6
            leftPadding: 22
            Repeater {
                model: ["Red", "Orange", "Yellow", "Green", "Blue", "Purple", "Gray"]
                Rectangle {
                    required property string modelData
                    required property int index
                    width: 16; height: 16; radius: 8
                    anchors.verticalCenter: parent.verticalCenter
                    color: Theme.tagColor(modelData)
                    border.width: tm.containsMouse ? 2 : 0
                    border.color: Theme.fg
                    MouseArea {
                        id: tm
                        anchors.fill: parent
                        hoverEnabled: true
                        onClicked: { m.pane.toggleTag(parent.modelData); m.close() }
                    }
                    GTip { visible: tm.containsMouse; text: "Tag “" + parent.modelData + "” (Ctrl+Alt+" + (parent.index + 1) + ")" }
                }
            }
        }
    }
    GMenuItem { text: "Tags…"; iconName: "tag"; enabled: !m.pane.special; onTriggered: m.pane.win.showInfo(m.sel) }
    MenuItem {
        implicitHeight: Theme.menuRow
        enabled: !m.pane.special
        background: Item {}
        contentItem: Row {
            leftPadding: 24
            spacing: 2
            GLabel { text: "Rating  "; color: Theme.fgSecondary; anchors.verticalCenter: parent.verticalCenter }
            Repeater {
                model: 5
                GLabel {
                    required property int index
                    anchors.verticalCenter: parent.verticalCenter
                    text: (m.target.rating || 0) >= (index + 1) * 2 ? "★" : "☆"
                    color: Theme.accent
                    font.pixelSize: Theme.title
                    MouseArea { anchors.fill: parent; onClicked: { m.pane.setRating((m.target.rating || 0) === (parent.index + 1) * 2 ? 0 : (parent.index + 1) * 2); m.close() } }
                }
            }
        }
    }
    GMenuSeparator {}
    GMenu {
        id: quick
        title: "Quick Actions"
        GMenuItem { text: "Rotate Left"; iconName: "object-rotate-left"; enabled: m.isImage; onTriggered: m.pane.win.quickAction("rotate-left", m.sel, "") }
        GMenuItem { text: "Rotate Right"; iconName: "object-rotate-right"; enabled: m.isImage; onTriggered: m.pane.win.quickAction("rotate-right", m.sel, "") }
        GMenu {
            title: "Convert Image"
            enabled: m.isImage
            GMenuItem { text: "to PNG"; iconName: "image-x-generic"; onTriggered: m.pane.win.quickAction("convert", m.sel, "png") }
            GMenuItem { text: "to JPEG"; iconName: "image-x-generic"; onTriggered: m.pane.win.quickAction("convert", m.sel, "jpg") }
            GMenuItem { text: "to WebP"; iconName: "image-x-generic"; onTriggered: m.pane.win.quickAction("convert", m.sel, "webp") }
        }
        GMenuItem { text: "Open in Terminal"; iconName: "utilities-terminal"; enabled: !!m.target.isDir; onTriggered: m.pane.grove.openTerminal(m.target.path) }
        GMenuItem { text: "Open as Administrator"; iconName: "dialog-password"; tip: "Opens a grove window with administrator rights (asks for your password)"; onTriggered: m.pane.win.quickAction("open-as-root", m.target.isDir ? [m.target.path] : [m.pane.path], "") }
        GMenuItem { text: "Add to Favourites"; iconName: "bookmark-new"; enabled: !!m.target.isDir; onTriggered: m.pane.grove.addFavourite(m.target.path, -1) }
        GMenuItem { text: "Batch Rename…"; iconName: "edit-rename"; shortcutText: Help.shown("Shift+F2"); onTriggered: m.pane.win.batchRename(m.sel) }
    }
    GMenu {
        id: gitMenu
        title: "Git"
        enabled: m.pane.inGit
        GMenuItem { text: "Add to Git"; iconName: "vcs-add|list-add"; onTriggered: m.pane.win.gitAction("add", m.pane.path, m.sel, "") }
        GMenuItem { text: "Commit Selected…"; iconName: "vcs-commit|document-save"; onTriggered: m.pane.win.askCommit(m.pane.path, m.sel) }
        GMenuItem { text: "Show Changes"; iconName: "vcs-diff|document-preview"; onTriggered: m.pane.win.gitAction("diff", m.pane.path, m.sel, "") }
        GMenuItem { text: "Show History"; iconName: "vcs-log|view-history"; onTriggered: m.pane.win.gitAction("log", m.pane.path, m.sel, "") }
        GMenuSeparator {}
        GMenuItem { text: "Pull"; iconName: "vcs-pull|go-down"; onTriggered: m.pane.win.gitAction("pull", m.pane.path, [], "") }
        GMenuItem { text: "Push"; iconName: "vcs-push|go-up"; onTriggered: m.pane.win.gitAction("push", m.pane.path, [], "") }
    }
    GMenu {
        id: driveMenu
        title: "Google Drive"
        GMenuItem { text: "Copy Link"; iconName: "edit-copy|insert-link"; tip: "Put this item's Google Drive address on the clipboard"; onTriggered: m.copyDriveLink() }
        GMenuItem { text: "View on Google Drive"; iconName: "internet-web-browser|applications-internet"; tip: "Open this item's page on drive.google.com in your browser"; onTriggered: m.viewOnDrive() }
        GMenuItem { text: "Share…"; iconName: "document-share|send-to"; tip: "Choose who can see or edit it (opens canopy's Share sheet)"; onTriggered: m.driveCall("share", "") }
        GMenuItem {
            readonly property bool starred: !!m.cloudItem && !!m.cloudItem.starred
            text: starred ? "Unstar" : "Star"
            iconName: starred ? "starred-symbolic|rating" : "non-starred-symbolic|rating-unrated|rating"
            enabled: !!m.cloudItem
            tip: starred ? "Take it out of Starred in Google Drive" : "Add it to Starred in Google Drive, to find it quickly"
            onTriggered: m.setStar(!starred)
        }
        GMenuSeparator {}
        GMenuItem {
            text: "Remove from This Computer…"
            iconName: "edit-delete|user-trash"
            enabled: m.cloudCode === "S"
            tip: m.cloudCode === "S" ? "Stop syncing it here: it stays in Google Drive, the copy on this computer goes to the Trash (asks first)"
                                     : "Only for items that are fully synced"
            onTriggered: m.unsync()
        }
    }
    GMenuItem {
        text: "Sync with Google Drive…"
        iconName: "folder-cloud|folder-sync|folder-remote"
        visible: m.one && !!m.target.isDir && (m.target.path || "").indexOf("/") === 0 && m.cloudRoot === "" && !m.target.cloud && m.canopyUp
        height: visible ? implicitHeight : 0
        tip: "Keep this folder in Google Drive and in step with it (opens canopy)"
        onTriggered: m.driveCall("add-folder", "")
    }
    GMenu {
        id: svc
        title: "Actions"
        enabled: m.services.length > 0
        Instantiator {
            model: m.services
            delegate: GMenuItem {
                required property var modelData
                text: (modelData.submenu ? modelData.submenu + " › " : "") + modelData.name
                iconName: modelData.icon
                onTriggered: m.pane.grove.runServiceMenu(modelData.exec, JSON.stringify(m.sel))
            }
            onObjectAdded: (i, o) => svc.insertItem(i, o)
            onObjectRemoved: (i, o) => svc.removeItem(o)
        }
    }
    GMenuSeparator {}
    GMenuItem { text: m.trash ? "Delete Immediately…" : "Move to Trash"; iconName: m.trash ? "edit-delete" : "user-trash"; shortcutText: m.trash ? "Delete" : "Delete"; enabled: !m.pane.special || m.trash; onTriggered: m.pane.trashSelected() }
    GMenuItem {
        text: "Delete " + (m.target.subvol || "").replace("Read-only ", "") + "…"
        iconName: "edit-delete"
        visible: m.one && !!m.target.subvol && !m.trash
        height: visible ? implicitHeight : 0
        enabled: m.target.subvol !== "Snapper Snapshot"
        tip: m.target.subvol === "Snapper Snapshot" ? "Snapper made this snapshot and removes it itself; Grove doesn't delete it" : "Delete this btrfs " + (m.target.subvol || "").toLowerCase() + " and everything in it at once (asks first)"
        onTriggered: m.pane.win.deleteSubvolume(m.target.path)
    }
    GMenuItem { text: "Delete Permanently…"; iconName: "edit-delete"; visible: !m.trash; height: visible ? implicitHeight : 0; shortcutText: Help.shown("Shift+Delete"); enabled: !m.pane.special; onTriggered: m.pane.deleteSelected() }
}
