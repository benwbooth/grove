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
    readonly property bool one: sel.length === 1
    readonly property bool trash: pane.inTrash
    readonly property bool isImage: (target.mime || "").indexOf("image/") === 0
    readonly property bool isArchive: pane.archiveLike(target)

    GMenuItem { text: m.target.isDir ? "Open" : "Open"; iconName: "document-open"; shortcutText: "Return"; enabled: !m.trash; onTriggered: m.pane.openSelected(false) }
    GMenuItem { text: "Open in New Tab"; iconName: "tab-new"; visible: !!m.target.isDir; height: visible ? implicitHeight : 0; shortcutText: "Ctrl+Return"; onTriggered: m.pane.openSelected(true) }
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
        GMenuItem { text: "Other…"; tip: "Choose any installed app, and optionally make it the default"; onTriggered: m.pane.win.openWithDialog(m.target.path) }
    }
    GMenuItem { text: "Quick Look"; iconName: "quickview"; shortcutText: "Space"; onTriggered: m.pane.quickLook() }
    GMenuItem { text: "Get Info"; iconName: "documentinfo"; shortcutText: "Alt+Return"; onTriggered: m.pane.win.showInfo(m.sel) }
    GMenuSeparator {}
    GMenuItem { text: "Put Back"; iconName: "edit-undo"; visible: m.trash; height: visible ? implicitHeight : 0; onTriggered: m.pane.putBack() }
    GMenuItem { text: m.one ? "Rename…" : "Rename " + m.sel.length + " Items…"; iconName: "edit-rename"; shortcutText: "F2"; enabled: !m.pane.special; onTriggered: m.pane.rename() }
    GMenuItem { text: "Duplicate"; iconName: "edit-copy"; shortcutText: "Ctrl+D"; enabled: !m.pane.special; onTriggered: m.pane.duplicate() }
    GMenuItem { text: "Make Alias"; iconName: "insert-link"; shortcutText: "Ctrl+Alt+A"; enabled: !m.pane.special; onTriggered: m.pane.makeAlias() }
    GMenuItem { text: m.one ? "Compress “" + m.target.name + "”" : "Compress " + m.sel.length + " Items"; iconName: "archive-insert"; enabled: !m.pane.special; tip: "Make a .zip next to the items"; onTriggered: m.pane.compress("zip") }
    GMenuItem { text: "Extract Here"; iconName: "archive-extract"; visible: m.isArchive; height: visible ? implicitHeight : 0; onTriggered: m.pane.extract() }
    GMenuSeparator {}
    GMenuItem { text: "Copy"; iconName: "edit-copy"; shortcutText: "Ctrl+C"; onTriggered: m.pane.copy(false) }
    GMenuItem { text: "Cut"; iconName: "edit-cut"; shortcutText: "Ctrl+X"; enabled: !m.pane.special || m.trash; onTriggered: m.pane.copy(true) }
    GMenuItem { text: "Copy Path"; iconName: "edit-copy-path"; shortcutText: "Ctrl+Alt+C"; onTriggered: m.pane.copyPath() }
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
    GMenuSeparator {}
    GMenu {
        id: quick
        title: "Quick Actions"
        GMenuItem { text: "Rotate Left"; iconName: "object-rotate-left"; enabled: m.isImage; onTriggered: m.pane.win.quickAction("rotate-left", m.sel, "") }
        GMenuItem { text: "Rotate Right"; iconName: "object-rotate-right"; enabled: m.isImage; onTriggered: m.pane.win.quickAction("rotate-right", m.sel, "") }
        GMenu {
            title: "Convert Image"
            enabled: m.isImage
            GMenuItem { text: "to PNG"; onTriggered: m.pane.win.quickAction("convert", m.sel, "png") }
            GMenuItem { text: "to JPEG"; onTriggered: m.pane.win.quickAction("convert", m.sel, "jpg") }
            GMenuItem { text: "to WebP"; onTriggered: m.pane.win.quickAction("convert", m.sel, "webp") }
        }
        GMenuItem { text: "Compress as .tar.zst"; iconName: "archive-insert"; onTriggered: m.pane.compress("tar.zst") }
        GMenuItem { text: "Open in Terminal"; iconName: "utilities-terminal"; enabled: !!m.target.isDir; onTriggered: m.pane.grove.openTerminal(m.target.path) }
        GMenuItem { text: "Open as Administrator"; iconName: "dialog-password"; tip: "Opens a grove window with administrator rights (asks for your password)"; onTriggered: m.pane.win.quickAction("open-as-root", m.target.isDir ? [m.target.path] : [m.pane.path], "") }
        GMenuItem { text: "Add to Favourites"; iconName: "bookmark-new"; enabled: !!m.target.isDir; onTriggered: m.pane.grove.addFavourite(m.target.path, -1) }
        GMenuItem { text: "Batch Rename…"; iconName: "edit-rename"; shortcutText: "Shift+F2"; onTriggered: m.pane.win.batchRename(m.sel) }
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
    GMenuItem { text: "Delete Permanently…"; iconName: "edit-delete"; visible: !m.trash; height: visible ? implicitHeight : 0; shortcutText: "Shift+Delete"; enabled: !m.pane.special; onTriggered: m.pane.deleteSelected() }
}
