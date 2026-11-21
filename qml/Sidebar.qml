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

    // Traffic lights (grove draws its own title bar).
    Row {
        id: lights
        visible: sb.win.clientDecorations
        x: 18
        y: 18
        spacing: 8
        Repeater {
            model: [
                { c: "#ff5f57", tip: "Close this window (Ctrl+W closes a tab)", act: "close" },
                { c: "#febc2e", tip: "Minimize", act: "min" },
                { c: "#28c840", tip: "Zoom (fill the screen / restore)", act: "max" }
            ]
            Rectangle {
                required property var modelData
                width: 13; height: 13; radius: 7
                color: lm.containsMouse || sb.win.active ? modelData.c : Theme.fgTertiary
                border.width: 0.5
                border.color: Qt.rgba(0, 0, 0, 0.25)
                MouseArea {
                    id: lm
                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: {
                        if (parent.modelData.act === "close") sb.win.close()
                        else if (parent.modelData.act === "min") sb.win.showMinimized()
                        else sb.win.visibility === Window.Maximized ? sb.win.showNormal() : sb.win.showMaximized()
                    }
                }
                GTip { visible: lm.containsMouse; text: parent.modelData.tip }
            }
        }
    }
    // Drag the window by the sidebar's top strip.
    MouseArea {
        anchors.left: parent.left; anchors.right: parent.right
        anchors.top: parent.top
        height: lights.y + lights.height + 10
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
            MouseArea { id: hm; anchors.fill: parent; hoverEnabled: true; onClicked: sb.toggleSection(sec.name) }
            GTip { visible: hm.containsMouse && sec.tip !== ""; text: sec.tip + (sec.open ? " Click to hide." : " Click to show.") }
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
        signal activated()
        signal ejected()
        signal menu()
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
            name: pl.icon
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
                if (m.button === Qt.RightButton) pl.menu()
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
            tip: "Eject “" + pl.label + "” (Ctrl+E) — safe to unplug afterwards"
            onClicked: pl.ejected()
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
        anchors.topMargin: sb.win.clientDecorations ? lights.y + lights.height + 10 : 10
        anchors.bottomMargin: 8
        contentHeight: col.height
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
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
                    onActivated: sb.win.go(sb.places.home)
                }
                Repeater {
                    model: sb.places.locations || []
                    Place {
                        required property var modelData
                        label: modelData.name; icon: modelData.icon; path: modelData.path
                        total: modelData.total || 0; free: modelData.free || 0
                        ejectable: !!modelData.ejectable
                        dim: modelData.kind === "unmounted"
                        dropTarget: modelData.kind !== "unmounted"
                        tip: modelData.kind === "unmounted" ? "Click to open this disk" : modelData.total ? modelData.path + " — " + sb.app.size(modelData.free) + " free of " + sb.app.size(modelData.total) : modelData.path
                        onActivated: {
                            if (modelData.kind === "unmounted") {
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
                        tip: "Saved server — click to connect"
                        onActivated: sb.win.connectSaved(modelData)
                        onMenu: serverMenu.openFor(modelData.name)
                    }
                }
                Place {
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
    }
    GMenu {
        id: trashMenu
        GMenuItem { text: "Open"; onTriggered: sb.win.go("trash:///") }
        GMenuItem { text: "Empty Trash…"; iconName: "trash-empty"; enabled: sb.app.trashCount > 0; onTriggered: sb.win.emptyTrash() }
    }
    GMenu {
        id: serverMenu
        property string name: ""
        function openFor(n) { name = n; popup() }
        GMenuItem { text: "Forget This Server"; iconName: "list-remove"; onTriggered: Async.call(sb.app.grove, k => sb.app.grove.forgetServer(k, serverMenu.name), r => sb.reload()) }
    }
    GMenu {
        id: smartMenu
        property string name: ""
        function openFor(n) { name = n; popup() }
        GMenuItem { text: "Remove Smart Folder"; iconName: "list-remove"; onTriggered: sb.app.grove.removeSmartFolder(smartMenu.name) }
    }
}
