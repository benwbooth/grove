import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window

// Get Info: one window per item. Size (folders counted in the background),
// dates, where it is, tags, comment, which app opens it (and changing the
// default), permissions and owner (asks for the administrator password
// through polkit when the item isn't yours).
Window {
    id: iw
    property var app
    property string path: ""
    property var info: ({})
    property real folderBytes: -1
    property real folderItems: -1
    property bool counting: false
    property var apps: []
    property string error: ""
    width: Math.round(330 * Theme.pt)
    height: Math.min(Screen.height - 80, Math.round(640 * Theme.pt))
    minimumWidth: 300
    color: Theme.window
    title: (info.name || "") + " Info"

    function load() {
        Async.call(app.grove, k => app.grove.info(k, iw.path), r => {
            if (r.error) { iw.error = r.error; return }
            iw.info = r
            comment.text = r.comment || ""
            tagsField.text = (r.tags || []).join(", ")
            if (r.isDir && !iw.counting) {
                iw.counting = true
                sizeKey = Async.call(app.grove, k => { iw.sizeKey = k; app.grove.folderSize(k, iw.path) }, null)
            }
            if (!r.isDir) Async.call(app.grove, k => app.grove.appsFor(k, iw.path), a => iw.apps = a.apps || [])
        })
    }
    property string sizeKey: ""
    Connections {
        target: iw.app.grove
        function onSizeReady(key, bytes, items, done) {
            if (key !== iw.sizeKey) return
            iw.folderBytes = bytes; iw.folderItems = items
            if (done) iw.counting = false
        }
    }
    Component.onCompleted: load()

    function fmt(t) { return t ? new Date(t * 1000).toLocaleString(Qt.locale(), "dddd, d MMMM yyyy 'at' h:mm AP") : "--" }
    function setMode(bit, on) {
        const m = on ? (info.mode | bit) : (info.mode & ~bit)
        Async.call(app.grove, k => app.grove.setMode(k, iw.path, m, false), r => { if (r.error) iw.error = r.error; iw.load() })
    }

    component Section: Column {
        id: sec
        property string title: ""
        property bool open: true
        default property alias body: inner.data
        width: parent ? parent.width : 0
        spacing: 6
        Item {
            width: parent.width; height: 24
            GLabel { text: (sec.open ? "⌄  " : "›  ") + sec.title; font.bold: true }
            MouseArea { anchors.fill: parent; onClicked: sec.open = !sec.open }
        }
        Column { id: inner; visible: sec.open; width: parent.width; spacing: 6; leftPadding: 14 }
        Rectangle { width: parent.width; height: 1; color: Theme.separator }
    }
    component Fact: Row {
        property string k: ""
        property string v: ""
        spacing: 8
        width: parent ? parent.width - 14 : 0
        GLabel { text: parent.k; width: 90; horizontalAlignment: Text.AlignRight; color: Theme.fgSecondary; font.pixelSize: Theme.small }
        GLabel { text: parent.v; width: parent.width - 98; wrapMode: Text.WrapAnywhere; elide: Text.ElideNone; font.pixelSize: Theme.small }
    }

    ScrollView {
        anchors.fill: parent
        transform: Translate { x: 0.001; y: 0.001 }
        contentWidth: availableWidth
        Column {
            width: iw.width - 32
            x: 16
            topPadding: 16
            bottomPadding: 16
            spacing: 10

            Row {
                spacing: 12
                width: parent.width
                GIcon { size: 56; name: iw.info.icon || "unknown" }
                Column {
                    width: parent.width - 70
                    anchors.verticalCenter: parent.verticalCenter
                    GLabel { width: parent.width; text: iw.info.name || ""; font.bold: true; font.pixelSize: Theme.title; wrapMode: Text.WrapAnywhere; elide: Text.ElideNone }
                    GLabel {
                        text: iw.info.isDir ? (iw.folderBytes >= 0 ? iw.app.size(iw.folderBytes) + (iw.counting ? "…" : "") : "Counting…") : (iw.info.sizeText || "")
                        color: Theme.fgSecondary
                    }
                    GLabel { text: "Modified " + iw.fmt(iw.info.mtime); color: Theme.fgSecondary; font.pixelSize: Theme.small }
                }
            }
            GLabel { visible: iw.error !== ""; width: parent.width; text: iw.error; color: Theme.danger; wrapMode: Text.Wrap; elide: Text.ElideNone }

            Section {
                title: "Tags"
                GField {
                    id: tagsField
                    width: parent.width - 14
                    placeholderText: "Add tags, separated by commas"
                    tip: "Tags are saved on the file (user.xdg.tags) and shared with Dolphin"
                    onEditingFinished: {
                        const t = text.split(",").map(s => s.trim()).filter(s => s)
                        Async.call(iw.app.grove, k => iw.app.grove.setTags(k, iw.path, JSON.stringify(t)), r => { if (r.error) iw.error = r.error })
                    }
                }
                Row {
                    spacing: 6
                    Repeater {
                        model: ["Red", "Orange", "Yellow", "Green", "Blue", "Purple", "Gray"]
                        Rectangle {
                            required property string modelData
                            width: 16; height: 16; radius: 8
                            color: Theme.tagColor(modelData)
                            border.width: (iw.info.tags || []).indexOf(modelData) >= 0 ? 2 : 0
                            border.color: Theme.fg
                            MouseArea {
                                id: tma
                                anchors.fill: parent
                                hoverEnabled: true
                                onClicked: {
                                    const cur = (iw.info.tags || []).slice()
                                    const i = cur.indexOf(parent.modelData)
                                    if (i >= 0) cur.splice(i, 1); else cur.push(parent.modelData)
                                    Async.call(iw.app.grove, k => iw.app.grove.setTags(k, iw.path, JSON.stringify(cur)), r => { if (r.error) iw.error = r.error; iw.load() })
                                }
                            }
                            GTip { visible: tma.containsMouse; text: "Tag or untag “" + parent.modelData + "”" }
                        }
                    }
                }
            }

            Section {
                title: "General"
                Fact { k: "Kind"; v: iw.info.kind || "" }
                Fact { k: "Size"; v: iw.info.isDir ? (iw.folderBytes >= 0 ? iw.app.size(iw.folderBytes) + " on disk, " + iw.folderItems + " items" + (iw.counting ? " (counting…)" : "") : "Counting…") : (iw.info.size || 0).toLocaleString(Qt.locale(), "f", 0) + " bytes (" + (iw.info.sizeText || "") + ")" }
                Fact { k: "Where"; v: iw.info.parent || "" }
                Fact { k: "Created"; v: iw.fmt(iw.info.btime) }
                Fact { k: "Modified"; v: iw.fmt(iw.info.mtime) }
                Fact { k: "Opened"; v: iw.fmt(iw.info.atime) }
                Fact { k: "Points to"; v: iw.info.target || ""; visible: !!iw.info.isLink }
                Fact { k: "Disk"; v: iw.info.mount ? iw.info.mount + " — " + iw.app.size(iw.info.diskFree || 0) + " free of " + iw.app.size(iw.info.diskTotal || 0) : "" }
            }

            Section {
                title: "Comments"
                TextArea {
                    id: comment
                    width: parent.width - 14
                    height: 70
                    wrapMode: TextEdit.Wrap
                    font.family: Theme.family
                    font.pixelSize: Theme.body
                    renderType: Text.NativeRendering
                    color: Theme.fg
                    placeholderText: "A note about this item (saved on the file, shared with Dolphin)"
                    placeholderTextColor: Theme.fgTertiary
                    background: Rectangle { radius: 6; color: Theme.field }
                    onEditingFinished: Async.call(iw.app.grove, k => iw.app.grove.setComment(k, iw.path, text), r => { if (r.error) iw.error = r.error })
                }
            }

            Section {
                title: "Open With"
                visible: !iw.info.isDir
                ComboBox {
                    id: appBox
                    width: parent.width - 14
                    model: iw.apps.map(a => a.name + (a.isDefault ? " (default)" : ""))
                    font.family: Theme.family
                    font.pixelSize: Theme.body
                    currentIndex: Math.max(0, iw.apps.findIndex(a => a.isDefault))
                }
                Row {
                    spacing: 8
                    PushButton {
                        text: "Change All…"
                        tip: "Make this app the default for every " + (iw.info.kind || "file of this kind")
                        enabled: iw.apps.length > 0
                        onClicked: {
                            const a = iw.apps[appBox.currentIndex]
                            Async.call(iw.app.grove, k => iw.app.grove.setDefaultApp(k, iw.info.mime, a.id), r => {
                                if (r.error) iw.error = r.error
                                else Async.call(iw.app.grove, k => iw.app.grove.appsFor(k, iw.path), x => iw.apps = x.apps || [])
                            })
                        }
                    }
                    GLabel { text: "Use this app for all " + (iw.info.kind || "") + " files."; color: Theme.fgSecondary; font.pixelSize: Theme.small; anchors.verticalCenter: parent.verticalCenter; width: 140; wrapMode: Text.Wrap; elide: Text.ElideNone }
                }
            }

            Section {
                title: "Sharing & Permissions"
                GLabel {
                    width: parent.width - 14
                    wrapMode: Text.Wrap; elide: Text.ElideNone
                    font.pixelSize: Theme.small
                    color: Theme.fgSecondary
                    text: iw.info.mine ? "You own this item and can change what others may do with it." : "This item belongs to “" + (iw.info.owner || "") + "”. Changing it asks for the administrator password."
                }
                Column {
                    spacing: 4
                    Row {
                        spacing: 0
                        GLabel { text: ""; width: 140 }
                        GLabel { text: "Read"; width: 50; font.pixelSize: Theme.small; color: Theme.fgSecondary }
                        GLabel { text: "Write"; width: 50; font.pixelSize: Theme.small; color: Theme.fgSecondary }
                        GLabel { text: iw.info.isDir ? "Open" : "Run"; width: 50; font.pixelSize: Theme.small; color: Theme.fgSecondary }
                    }
                    Repeater {
                        model: [
                            { who: (iw.info.owner || "owner") + " (owner)", shift: 6 },
                            { who: (iw.info.group || "group") + " (group)", shift: 3 },
                            { who: "Everyone else", shift: 0 }
                        ]
                        delegate: Row {
                            id: permRow
                            required property var modelData
                            spacing: 0
                            GLabel { text: permRow.modelData.who; width: 140; font.pixelSize: Theme.small; height: 22 }
                            Repeater {
                                model: [4, 2, 1]
                                delegate: Item {
                                    required property int modelData
                                    width: 50; height: 22
                                    GCheck {
                                        anchors.verticalCenter: parent.verticalCenter
                                        checked: (((iw.info.mode || 0) >> permRow.modelData.shift) & parent.modelData) !== 0
                                        onToggled: iw.setMode(parent.modelData << permRow.modelData.shift, checked)
                                        tip: "Allow " + permRow.modelData.who + " to " + ({ 4: "read", 2: "change", 1: iw.info.isDir ? "open" : "run" })[parent.modelData] + " it"
                                    }
                                }
                            }
                        }
                    }
                }
                GLabel { text: "Permissions: " + (iw.info.rwx || "") + "  (" + (iw.info.mode !== undefined ? iw.info.mode.toString(8) : "") + ")"; font.family: Theme.mono; font.pixelSize: Theme.small; color: Theme.fgSecondary }
                Row {
                    spacing: 8
                    GField { id: ownerField; width: 100; placeholderText: iw.info.owner || "owner"; tip: "New owner (user name)" }
                    GField { id: groupField; width: 100; placeholderText: iw.info.group || "group"; tip: "New group" }
                    PushButton {
                        text: "Change Owner"
                        tip: "Needs the administrator password unless you're only changing the group to one of yours"
                        onClicked: Async.call(iw.app.grove, k => iw.app.grove.setOwner(k, iw.path, ownerField.text || iw.info.owner, groupField.text || iw.info.group, false), r => { if (r.error) iw.error = r.error; iw.load() })
                    }
                }
            }
        }
    }
}
