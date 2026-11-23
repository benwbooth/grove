import QtQuick

// Open and Open With… for the file a preview shows (the pane under the
// files, the side panel, the column view's preview column).
Row {
    id: ob
    property var pane
    property var item: null
    spacing: 6
    visible: !!item && !item.isDir
    property var apps: []
    PushButton {
        text: "Open"
        tip: "Open it with its default app (Return or double-click)"
        onClicked: ob.pane.grove.openPaths(JSON.stringify([ob.item.path]))
    }
    PushButton {
        id: withBtn
        text: "Open With…"
        tip: "Choose the app to open it with"
        onClicked: {
            const path = ob.item.path
            Async.call(ob.pane.grove, k => ob.pane.grove.appsFor(k, path), r => {
                ob.apps = r.apps || []
                menu.popup(withBtn, 0, withBtn.height + 4)
            })
        }
    }
    GMenu {
        id: menu
        Instantiator {
            model: ob.apps
            delegate: GMenuItem {
                required property var modelData
                text: modelData.name + (modelData.isDefault ? "  (default)" : "")
                iconName: modelData.icon
                onTriggered: ob.pane.grove.openWith(JSON.stringify([ob.item.path]), modelData.id)
            }
            onObjectAdded: (i, o) => menu.insertItem(i, o)
            onObjectRemoved: (i, o) => menu.removeItem(o)
        }
        GMenuSeparator {}
        GMenuItem { text: "Other Application…"; iconName: "system-run|application-x-executable"; tip: "Choose any installed app, and optionally make it the default"; onTriggered: ob.pane.win.openWithDialog(ob.item.path) }
    }
}
