import QtQuick
import QtQuick.Controls.Basic

// The sidebar (a floating glass pane, as in macOS 26): Favourites (drag
// folders in to add, drag to reorder, right-click to remove), Recents,
// Locations (home, computer, disks with capacity and eject), Network
// (saved servers, connected shares, Connect to Server…), Tags, Smart Folders.
Item {
    id: sb
    property var win
    property var app: win.app
    property var places: ({})
    property string currentPath: ""
    property var collapsed: app.settings.collapsed_sections || []
    // Dolphin's hidden places: entries the user hid (right-click › Hide),
    // shown dimmed again with "Show Hidden Entries".
    readonly property var hiddenPlaces: app.settings.hidden_places || []
    property bool showHiddenPlaces: false
    function setHidden(key, hide) {
        const h = hiddenPlaces.filter(k => k !== key)
        if (hide) h.push(key)
        app.grove.setSetting("hidden_places", JSON.stringify(h))
        if (hide) win.toast("info", "Hidden from the sidebar. Right-click a section heading › Show Hidden Entries brings it back.")
    }

    function reload() { Async.call(app.grove, k => app.grove.places(k), r => sb.places = r) }
    Component.onCompleted: reload()
    Connections {
        target: sb.app.grove
        function onPlacesRevisionChanged() { sb.reload() }
    }
    Timer { interval: 15000; running: true; repeat: true; onTriggered: sb.reload() }

    function toggleSection(name) {
        let c = collapsed.slice()
        const i = c.indexOf(name)
        if (i >= 0) c.splice(i, 1); else c.push(name)
        app.grove.setSetting("collapsed_sections", JSON.stringify(c))
    }

    GlassPanel { anchors.fill: parent; tint: Theme.sidebar }
    // Wheel on the sidebar itself (its list's own scrolling is off).
    Kinetic { id: sbKinetic; target: flick; notch: 60 }
    WheelHandler { acceptedDevices: PointerDevice.AllDevices; acceptedModifiers: Qt.NoModifier; onWheel: (e) => sbKinetic.wheel(e) }

    // Drag the window by the sidebar's top strip.
    MouseArea {
        anchors.left: parent.left; anchors.right: parent.right
        anchors.top: parent.top
        height: 10
        z: -1
        onPressed: sb.win.startSystemMove()
        onDoubleClicked: sb.win.visibility === Window.Maximized ? sb.win.showNormal() : sb.win.showMaximized()
    }

    component Section: Column {
        id: sec
        property string name: ""
        property string tip: ""
        readonly property bool open: sb.collapsed.indexOf(name) < 0
        width: parent ? parent.width : 0
        Item {
            width: parent.width
            height: Math.round(26 * Theme.pt)
            GLabel {
                x: 12
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 4
                text: sec.name
                font.pixelSize: Theme.small
                font.bold: true
                color: Theme.fgTertiary
            }
            GLabel {
                anchors.right: parent.right; anchors.rightMargin: 12
                anchors.bottom: parent.bottom; anchors.bottomMargin: 4
                text: sec.open ? "⌄" : "›"
                color: Theme.fgTertiary
                visible: hm.containsMouse
            }
            MouseArea {
                id: hm
                anchors.fill: parent
                hoverEnabled: true
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                onClicked: (m) => { if (m.button === Qt.RightButton) sectionMenu.popup(); else sb.toggleSection(sec.name) }
            }
            GTip { visible: hm.containsMouse && sec.tip !== ""; text: sec.tip + (sec.open ? " Click to hide." : " Click to show.") + " Right-click to show hidden entries." }
        }
    }

    component Place: Item {
        id: pl
        property string label: ""
        property string icon: "folder"
        property string path: ""
        property string tip: ""
        property color dot: "transparent"
        property bool ejectable: false
        property real total: 0
        property real free: 0
        property bool dim: false
        property bool dropTarget: false
        property int favIndex: -1
        // A local disk (mounted or not): its device, for Format… and Disk Utility.
        property string device: ""
        readonly property bool isDisk: device.indexOf("/dev/") === 0
        // Identifies it for hiding (a path, or "tag:Red", "smart:Name"...).
        property string key: path
        property bool shown: true
        readonly property bool isHidden: key !== "" && sb.hiddenPlaces.indexOf(key) >= 0
        visible: shown && (!isHidden || sb.showHiddenPlaces)
        opacity: isHidden ? 0.5 : 1
        signal activated()
        signal ejected()
        signal menu()
        // Places without a menu of their own get the general one.
        onMenu: if (!customMenu) placeMenu.popup()
        property bool customMenu: false
        width: parent ? parent.width : 0
        height: Theme.sidebarRow + (total > 0 ? 6 : 0)
        readonly property bool current: path !== "" && path === sb.currentPath
        Rectangle {
            anchors.fill: parent
            anchors.leftMargin: 8; anchors.rightMargin: 8
            radius: Math.round(7 * Theme.pt)
            color: pl.current ? Theme.selectedInactive : drop.containsDrag ? Theme.dropTarget : ma.containsMouse ? Theme.hover : "transparent"
        }
        Rectangle {
            visible: pl.dot.a > 0
            x: 20; anchors.verticalCenter: parent.verticalCenter
            width: 11; height: 11; radius: 6
            color: pl.dot
        }
        GIcon {
            visible: pl.dot.a === 0
            x: 16
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: pl.total > 0 ? -3 : 0
            size: 18
            // Finder: monochrome glyphs in the accent colour.
            name: pl.icon ? pl.icon.split("|").map(n => n + "-symbolic").join("|") + "|" + pl.icon : ""
            tint: Theme.accent
            opacity: pl.dim ? 0.5 : 1
        }
        GLabel {
            x: 42
            width: parent.width - x - (pl.ejectable ? 34 : 14)
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: pl.total > 0 ? -3 : 0
            text: pl.label
            opacity: pl.dim ? 0.55 : 1
        }
        // Capacity bar.
        Rectangle {
            visible: pl.total > 0
            x: 42; width: parent.width - x - (pl.ejectable ? 34 : 14)
            y: parent.height - 7
            height: 3; radius: 1.5
            color: Theme.separator
            Rectangle {
                width: parent.width * Math.min(1, (pl.total - pl.free) / Math.max(1, pl.total))
                height: parent.height; radius: 1.5
                color: (pl.free / Math.max(1, pl.total)) < 0.1 ? Theme.danger : Theme.fgSecondary
            }
        }
        MouseArea {
            id: ma
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
            drag.target: pl.favIndex >= 0 ? dragProxy : null
            onClicked: (m) => {
                if (m.button === Qt.RightButton) { placeMenu.target = pl; pl.menu() }
                else if (m.button === Qt.MiddleButton && pl.path) sb.win.newTab(pl.path, [])
                else pl.activated()
            }
            onReleased: {
                if (dragProxy.Drag.active) dragProxy.Drag.drop()
                dragProxy.x = 0; dragProxy.y = 0
            }
        }
        Item {
            id: dragProxy
            width: pl.width; height: pl.height
            Drag.active: ma.drag.active
            Drag.keys: ["grove/favourite"]
            Drag.hotSpot.y: height / 2
            property int favIndex: pl.favIndex
        }
        GButton {
            visible: pl.ejectable
            anchors.right: parent.right; anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            width: 22; height: 22
            iconName: "media-eject"
            iconSize: 14
            tip: "Eject “" + pl.label + "” (Ctrl+E) — safe to unplug afterwards" + (pl.isDisk ? ". Right-click for Format… and Disk Utility." : "")
            onClicked: pl.ejected()
            // Right-click: the drive's menu (Eject, Format…, Disk Utility).
            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.RightButton
                enabled: pl.isDisk
                onClicked: { placeMenu.target = pl; ejectMenu.popup() }
            }
        }
        DropArea {
            id: drop
            anchors.fill: parent
            keys: pl.dropTarget ? ["text/uri-list", "grove/favourite"] : ["grove/favourite"]
            onDropped: (d) => {
                if (d.keys.indexOf("grove/favourite") >= 0 && d.source && d.source.favIndex >= 0) {
                    if (pl.favIndex >= 0) sb.app.grove.moveFavourite(d.source.favIndex, pl.favIndex)
                    return
                }
                if (pl.path && d.hasUrls) sb.win.activePane.dropInto(pl.path, d)
            }
        }
        GTip { visible: ma.containsMouse && pl.tip !== ""; text: pl.tip }
    }

    Flickable {
        id: flick
        anchors.fill: parent
        anchors.topMargin: 10
        anchors.bottomMargin: 8
        contentHeight: col.height
        clip: true
        interactive: false
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: GScrollBar {}
        Column {
            id: col
            width: flick.width

            Section { name: "Favourites"; tip: "Folders you use often. Drag a folder here to add it, drag to reorder, right-click to remove." }
            Column {
                width: parent.width
                visible: sb.collapsed.indexOf("Favourites") < 0
                Place {
                    label: "Recents"; icon: "document-open-recent"; path: "recent:///"
                    tip: "Files you opened recently, in any app (Ctrl+Shift+F)"
                    onActivated: sb.win.go("recent:///")
                }
                Repeater {
                    model: sb.places.favourites || []
                    Place {
                        required property var modelData
                        required property int index
                        label: modelData.name; icon: modelData.icon; path: modelData.path
                        dim: !modelData.exists
                        favIndex: index
                        dropTarget: true
                        tip: modelData.path
                        onActivated: sb.win.go(modelData.path)
                        customMenu: true
                        onMenu: favMenu.openFor(modelData.path)
                    }
                }
                // Drop a folder here to add it.
                Item {
                    width: parent.width
                    height: addDrop.containsDrag ? Theme.sidebarRow : 6
                    Rectangle {
                        anchors.fill: parent; anchors.leftMargin: 12; anchors.rightMargin: 12
                        visible: addDrop.containsDrag
                        radius: 6
                        color: Theme.dropTarget
                        GLabel { anchors.centerIn: parent; text: "Add to Favourites"; font.pixelSize: Theme.small }
                    }
                    DropArea {
                        id: addDrop
                        anchors.fill: parent
                        anchors.topMargin: -6
                        keys: ["text/uri-list"]
                        onDropped: (d) => {
                            for (const u of d.urls) {
                                const s = u.toString()
                                if (s.indexOf("file://") === 0) sb.app.grove.addFavourite(decodeURIComponent(s.substring(7)), -1)
                            }
                        }
                    }
                }
            }

            Section { name: "Locations"; tip: "Your computer and disks. Disks not opened yet are dimmed: click to open (mount) them." }
            Column {
                width: parent.width
                visible: sb.collapsed.indexOf("Locations") < 0
                Place {
                    label: sb.app.grove.env("USER") || "Home"; icon: "user-home"; path: sb.places.home || ""
                    tip: "Your home folder (Ctrl+Shift+H)"; dropTarget: true
                    // Finder lists home once, under Favourites.
                    shown: !(sb.places.favourites || []).some(f => f.path === sb.places.home)
                    onActivated: sb.win.go(sb.places.home)
                }
                Repeater {
                    model: sb.places.locations || []
                    Place {
                        required property var modelData
                        label: modelData.name; icon: modelData.icon; path: modelData.path
                        total: modelData.total || 0; free: modelData.free || 0
                        ejectable: !!modelData.ejectable
                        device: modelData.device || ""
                        dim: modelData.kind === "unmounted" || modelData.kind === "mtp"
                        dropTarget: modelData.kind !== "unmounted" && modelData.kind !== "mtp"
                        tip: modelData.kind === "unmounted" ? "Click to open this disk"
                           : modelData.kind === "mtp" ? "“" + modelData.name + "” is plugged in (USB). Click to open it. If it doesn't open, unlock it and allow file transfer on it, then click again."
                           : modelData.total ? modelData.path + " — " + sb.app.size(modelData.free) + " free of " + sb.app.size(modelData.total) : modelData.path
                        onActivated: {
                            if (modelData.kind === "mtp") {
                                sb.win.toast("info", "Opening “" + modelData.name + "”…")
                                Async.call(sb.app.grove, k => sb.app.grove.connectServer(k, JSON.stringify({ name: modelData.name, kind: "mtp", device: modelData.device, remember: false })), r => {
                                    if (r.error) sb.win.toast("error", r.error); else { sb.reload(); sb.win.go(r.path) }
                                })
                            } else if (modelData.kind === "unmounted") {
                                Async.call(sb.app.grove, k => sb.app.grove.mountDevice(k, modelData.device), r => {
                                    if (r.error) sb.win.toast("error", r.error); else { sb.reload(); sb.win.go(r.path) }
                                })
                            } else sb.win.go(modelData.path)
                        }
                        onEjected: sb.win.eject(modelData.path, modelData.name)
                    }
                }
                Place {
                    label: "Trash"; icon: sb.app.trashCount > 0 ? "user-trash-full" : "user-trash"; path: "trash:///"
                    tip: "Items you deleted. Drag items here to move them to the Trash; right-click to empty it."
                    dropTarget: true
                    onActivated: sb.win.go("trash:///")
                    customMenu: true
                    onMenu: trashMenu.popup()
                }
            }

            Section { name: "Network"; tip: "Servers and shares: SFTP, Windows (SMB), WebDAV, cloud drives (rclone) and phones." }
            Column {
                width: parent.width
                visible: sb.collapsed.indexOf("Network") < 0
                Repeater {
                    model: sb.places.network || []
                    Place {
                        required property var modelData
                        label: modelData.name; icon: modelData.icon; path: modelData.path
                        ejectable: true; dropTarget: true
                        tip: "Connected: " + modelData.path + " (" + modelData.fs + ")"
                        onActivated: sb.win.go(modelData.path)
                        onEjected: sb.win.eject(modelData.path, modelData.name)
                    }
                }
                Repeater {
                    model: (sb.places.servers || []).filter(s => !s.mounted)
                    Place {
                        required property var modelData
                        label: modelData.name; icon: modelData.icon
                        dim: true
                        tip: modelData.account ? "Saved server (Settings › Accounts) — click to connect" : "Saved server — click to connect"
                        onActivated: sb.win.connectSaved(modelData)
                        key: "server:" + modelData.name
                        customMenu: true
                        onMenu: serverMenu.openFor(modelData.name, modelData.account || "")
                    }
                }
                Place {
                    key: "connect"
                    label: "Connect to Server…"; icon: "network-connect|network-workgroup"
                    tip: "Open an SFTP, Windows (SMB) or WebDAV share, a cloud drive or a phone (Ctrl+K)"
                    onActivated: sb.win.connectServer()
                }
            }

            Section { name: "Tags"; tip: "Show everything with a tag. Tags are shared with Dolphin (user.xdg.tags)."; visible: (sb.places.tags || []).length > 0 }
            Column {
                width: parent.width
                visible: sb.collapsed.indexOf("Tags") < 0
                Repeater {
                    model: sb.places.tags || []
                    Place {
                        required property var modelData
                        label: modelData.name
                        dot: modelData.color !== "" ? modelData.color : "transparent"
                        icon: "tag"
                        key: "tag:" + modelData.name
                        tip: "Everything in your home folder tagged “" + modelData.name + "”"
                        onActivated: sb.win.activePane.showTag(modelData.name)
                    }
                }
            }

            Section { name: "Smart Folders"; tip: "Saved searches. Search, then click Save in the search bar."; visible: (sb.places.smart || []).length > 0 }
            Column {
                width: parent.width
                visible: sb.collapsed.indexOf("Smart Folders") < 0
                Repeater {
                    model: sb.places.smart || []
                    Place {
                        required property var modelData
                        label: modelData.name
                        icon: "folder-saved-search|edit-find"
                        tip: "Saved search in " + modelData.root
                        onActivated: sb.win.activePane.runSmartFolder(modelData)
                        key: "smart:" + modelData.name
                        customMenu: true
                        onMenu: smartMenu.openFor(modelData.name)
                    }
                }
            }
        }
    }

    GMenu {
        id: favMenu
        property string path: ""
        function openFor(p) { path = p; popup() }
        GMenuItem { text: "Open in New Tab"; iconName: "tab-new"; onTriggered: sb.win.newTab(favMenu.path, []) }
        GMenuItem { text: "Get Info"; iconName: "documentinfo"; onTriggered: sb.win.showInfo([favMenu.path]) }
        GMenuItem { text: "Remove from Sidebar"; iconName: "list-remove"; onTriggered: sb.app.grove.removeFavourite(favMenu.path) }
        HideItem {}
    }
    // Hide / show the entry right-clicked (Dolphin's "Hide").
    component HideItem: GMenuItem {
        readonly property var t: placeMenu.target
        text: t && t.isHidden ? "Show “" + t.label + "”" : "Hide " + (t ? "“" + t.label + "”" : "")
        iconName: t && t.isHidden ? "view-visible|visibility" : "view-hidden|hint"
        enabled: !!t && t.key !== ""
        tip: "Hidden entries come back with Show Hidden Entries (right-click a section heading)"
        onTriggered: sb.setHidden(t.key, !t.isHidden)
    }
    GMenu {
        id: placeMenu
        property var target: null
        GMenuItem { text: "Open in New Tab"; iconName: "tab-new"; enabled: !!placeMenu.target && placeMenu.target.path !== ""; onTriggered: sb.win.newTab(placeMenu.target.path, []) }
        GMenuItem { text: "Get Info"; iconName: "documentinfo"; enabled: !!placeMenu.target && placeMenu.target.path.indexOf("/") === 0; onTriggered: sb.win.showInfo([placeMenu.target.path]) }
        GMenuSeparator { visible: !!placeMenu.target && placeMenu.target.isDisk; height: visible ? implicitHeight : 0 }
        GMenuItem { text: "Format…"; iconName: "drive-removable-media|media-flash"; visible: !!placeMenu.target && placeMenu.target.isDisk; height: visible ? implicitHeight : 0
            tip: "Erase this drive and give it a new name and format (exFAT, FAT32, NTFS, ext4, Btrfs)"
            onTriggered: sb.win.formatDrive(placeMenu.target.device) }
        GMenuItem { text: "Open in Disk Utility"; iconName: "drive-harddisk|partitionmanager"; visible: !!placeMenu.target && placeMenu.target.isDisk; height: visible ? implicitHeight : 0
            tip: "See and change this disk's partitions in Grove Disk Utility"
            onTriggered: sb.app.openDiskUtility("") }
        HideItem {}
        GMenuSeparator {}
        GMenuItem { text: "Show Hidden Entries"; iconName: "view-visible|view-reveal"; checkable: true; checked: sb.showHiddenPlaces; enabled: sb.hiddenPlaces.length > 0; onTriggered: sb.showHiddenPlaces = !sb.showHiddenPlaces }
    }
    GMenu {
        id: ejectMenu
        GMenuItem { text: "Eject"; iconName: "media-eject"; shortcutText: "Ctrl+E"; tip: "Close the drive so it's safe to unplug"; onTriggered: placeMenu.target.ejected() }
        GMenuItem { text: "Format…"; iconName: "drive-removable-media|media-flash"; tip: "Erase this drive and give it a new name and format (exFAT, FAT32, NTFS, ext4, Btrfs)"; onTriggered: sb.win.formatDrive(placeMenu.target.device) }
        GMenuItem { text: "Open in Disk Utility"; iconName: "drive-harddisk|partitionmanager"; tip: "See and change this disk's partitions in Grove Disk Utility"; onTriggered: sb.app.openDiskUtility("") }
    }
    GMenu {
        id: sectionMenu
        GMenuItem { text: "Show Hidden Entries"; iconName: "view-visible|view-reveal"; checkable: true; checked: sb.showHiddenPlaces; enabled: sb.hiddenPlaces.length > 0; tip: "Entries you hid show dimmed, so you can unhide them (right-click › Show)"; onTriggered: sb.showHiddenPlaces = !sb.showHiddenPlaces }
        GMenuItem { text: "Unhide All"; iconName: "view-reveal|view-visible"; enabled: sb.hiddenPlaces.length > 0; tip: "Show every entry you hid"; onTriggered: sb.app.grove.setSetting("hidden_places", "[]") }
    }
    GMenu {
        id: trashMenu
        GMenuItem { text: "Open"; iconName: "document-open|folder-open"; onTriggered: sb.win.go("trash:///") }
        GMenuItem { text: "Empty Trash…"; iconName: "trash-empty"; enabled: sb.app.trashCount > 0; onTriggered: sb.win.emptyTrash() }
        HideItem {}
    }
    GMenu {
        id: serverMenu
        property string name: ""
        // Its account in hearth's Settings > Accounts ("" for grove's own).
        property string account: ""
        function openFor(n, a) { name = n; account = a || ""; popup() }
        GMenuItem { text: "Edit in Settings › Accounts…"; iconName: "document-edit|preferences-system-users"; visible: serverMenu.account !== ""; height: visible ? implicitHeight : 0
            tip: "Change its address, user or password in hearth's Settings, where your accounts are kept for all your apps"
            onTriggered: sb.win.openAccounts(serverMenu.account, () => sb.win.toast("error", "hearth's Settings couldn't be opened.")) }
        GMenuItem { text: "Forget This Server"; iconName: "list-remove"; tip: serverMenu.account ? "Remove it from Settings › Accounts (for all your apps) and from the sidebar" : "Remove it from the sidebar and its password from the keyring"
            onTriggered: Async.call(sb.app.grove, k => sb.app.grove.forgetServer(k, serverMenu.name), r => { if (r.error) sb.win.toast("error", r.error); sb.reload() }) }
        HideItem {}
    }
    GMenu {
        id: smartMenu
        property string name: ""
        function openFor(n) { name = n; popup() }
        GMenuItem { text: "Remove Smart Folder"; iconName: "list-remove"; onTriggered: sb.app.grove.removeSmartFolder(smartMenu.name) }
        HideItem {}
    }
}
