import QtQuick
import QtQuick.Controls.Basic

// Open With › Other…: every installed app, searchable; optionally make it
// the default for this kind of file (written to ~/.config/mimeapps.list,
// so every app agrees).
Sheet {
    id: ow
    property var app
    property var win
    property string path: ""
    property string mime: ""
    property string kind: ""
    property var apps: []
    property string chosen: ""
    title: "Open “" + path.substring(path.lastIndexOf("/") + 1) + "” with…"
    width: Math.min(parent ? parent.width - 40 : 520, Math.round(480 * Theme.pt))
    function start(p) {
        path = p; chosen = ""; filter.text = ""; always.checked = false
        Async.call(app.grove, k => app.grove.appsFor(k, p), r => { ow.mime = r.mime; ow.kind = r.kind })
        Async.call(app.grove, k => app.grove.allApps(k), r => ow.apps = r)
        open()
    }
    GField { id: filter; width: parent.width; iconName: "edit-find"; placeholderText: "Find an app"; clearable: true }
    Rectangle {
        width: parent.width; height: 260; radius: 8; color: Theme.field
        ListView {
            id: list
            anchors.fill: parent; anchors.margins: 4
            clip: true
            model: ow.apps.filter(a => filter.text === "" || a.name.toLowerCase().indexOf(filter.text.toLowerCase()) >= 0)
            ScrollBar.vertical: ScrollBar {}
            delegate: Rectangle {
                required property var modelData
                width: list.width; height: Theme.rowHeight + 4
                radius: 6
                color: ow.chosen === modelData.id ? Theme.accent : am.containsMouse ? Theme.hover : "transparent"
                GIcon { x: 6; anchors.verticalCenter: parent.verticalCenter; size: 20; name: modelData.icon || "application-x-executable" }
                GLabel { x: 34; width: parent.width - 40; anchors.verticalCenter: parent.verticalCenter; text: modelData.name; color: ow.chosen === modelData.id ? Theme.accentText : Theme.fg }
                MouseArea { id: am; anchors.fill: parent; hoverEnabled: true; onClicked: ow.chosen = modelData.id; onDoubleClicked: openBtn.clicked() }
                GTip { visible: am.containsMouse && modelData.comment !== ""; text: modelData.comment }
            }
        }
    }
    GCheck { id: always; text: "Always open " + (ow.kind || "files like this") + " with this app"; tip: "Make it the default for " + ow.mime }
    buttons: [
        PushButton { text: "Cancel"; onClicked: ow.close() },
        PushButton {
            id: openBtn
            text: "Open"; primary: true; enabled: ow.chosen !== ""
            onClicked: {
                if (always.checked) Async.call(ow.app.grove, k => ow.app.grove.setDefaultApp(k, ow.mime, ow.chosen), r => { if (r.error) ow.win.toast("error", r.error) })
                ow.app.grove.openWith(JSON.stringify([ow.path]), ow.chosen)
                ow.close()
            }
        }
    ]
}
