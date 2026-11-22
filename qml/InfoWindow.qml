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
            if (!r.isDir && r.isArchive) Async.call(app.grove, k => app.grove.archiveSizes(k, iw.path), a => { if (!a.error) iw.arch = a })
        })
    }
    property string sizeKey: ""
    property var arch: null
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
    property var sums: ({})
    function computeSum(a, then) {
        const s0 = Object.assign({}, sums); s0[a] = "…"; sums = s0
        Async.call(app.grove, k => app.grove.checksum(k, iw.path, a), r => {
            const s = Object.assign({}, iw.sums)
            s[a] = r.error ? "" : r.sum
            iw.sums = s
            if (r.error) iw.error = r.error
            if (then) then()
        })
    }
    function verify() {
        const t = expected.text.trim().toLowerCase()
        if (!t) { verdict.text = ""; return }
        const a = t.length === 32 ? "md5" : t.length === 40 ? "sha1" : t.length === 64 ? "sha256" : t.length === 128 ? "sha512" : ""
        if (!a) { verdict.text = "That doesn't look like an MD5, SHA-1, SHA-256 or SHA-512 checksum."; verdict.color = Theme.fgSecondary; return }
        const check = () => {
            const ok = iw.sums[a] === t
            verdict.text = ok ? "✓ Matches (" + a.toUpperCase() + "): the file is intact." : "✗ Doesn't match (" + a.toUpperCase() + "): the file differs from the one the checksum is for."
            verdict.color = ok ? Theme.good : Theme.danger
        }
        if (sums[a] && sums[a] !== "…") check(); else { verdict.text = "Calculating " + a.toUpperCase() + "…"; computeSum(a, check) }
    }
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

    KineticWheel { flickable: gsv2.contentItem; visible: gsv2.visible }
    ScrollView {
        id: gsv2
        ScrollBar.vertical: GScrollBar { parent: gsv2; x: gsv2.width - width; y: gsv2.topPadding; height: gsv2.availableHeight }
        ScrollBar.horizontal: GScrollBar { parent: gsv2; x: gsv2.leftPadding; y: gsv2.height - height; width: gsv2.availableWidth }
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
                Fact { k: "Uncompressed"; visible: !!iw.arch
                       v: !iw.arch ? "" : iw.arch.uncompressedText + (iw.arch.ratio && iw.arch.ratio < 1 ? " — packed to " + Math.round(iw.arch.ratio * 100) + "%" : "") + (iw.arch.files > 1 ? ", " + iw.arch.files + " files" : "") }
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
                GCombo {
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
                title: "Rating"
                Row {
                    spacing: 4
                    Repeater {
                        model: 5
                        GLabel {
                            required property int index
                            text: (iw.info.rating || 0) >= (index + 1) * 2 ? "★" : "☆"
                            color: Theme.accent
                            font.pixelSize: Theme.large
                            MouseArea {
                                anchors.fill: parent
                                onClicked: {
                                    const r = (iw.info.rating || 0) === (parent.index + 1) * 2 ? 0 : (parent.index + 1) * 2
                                    Async.call(iw.app.grove, k => iw.app.grove.setRating(k, JSON.stringify([iw.path]), r), x => iw.load())
                                }
                            }
                        }
                    }
                }
            }

            Section {
                title: "Locked"
                GCheck {
                    text: "Locked (nobody can change it, including you, until unlocked)"
                    width: parent.width - 14
                    checked: iw.info.mode !== undefined && (iw.info.mode & 0o222) === 0
                    tip: "Finder's Locked: removes write permission for everyone. Untick to allow changes again."
                    onToggled: {
                        const m = checked ? (iw.info.mode & ~0o222) : (iw.info.mode | 0o200)
                        Async.call(iw.app.grove, k => iw.app.grove.setMode(k, iw.path, m, false), r => { if (r.error) iw.error = r.error; iw.load() })
                    }
                }
            }

            Section {
                title: "Folder Icon"
                visible: !!iw.info.isDir
                open: false
                GLabel { width: parent.width - 14; text: "A theme icon name (e.g. folder-music) or a picture's path. Saved in the folder's .directory file, so Dolphin shows it too."; wrapMode: Text.Wrap; elide: Text.ElideNone; font.pixelSize: Theme.small; color: Theme.fgSecondary }
                Row {
                    spacing: 8
                    GField { id: iconField; width: 180; text: (iw.info.customIcon || "").split("|")[0]; placeholderText: "folder-music" }
                    PushButton { text: "Set"; onClicked: Async.call(iw.app.grove, k => iw.app.grove.setFolderIcon(k, iw.path, iconField.text.trim()), r => { if (r.error) iw.error = r.error; iw.load() }) }
                    PushButton { text: "Reset"; onClicked: { iconField.text = ""; Async.call(iw.app.grove, k => iw.app.grove.setFolderIcon(k, iw.path, ""), r => iw.load()) } }
                }
            }

            Section {
                title: "Checksums"
                visible: !iw.info.isDir
                open: false
                GLabel { width: parent.width - 14; text: "Check a download: paste the checksum the website gives to compare."; wrapMode: Text.Wrap; elide: Text.ElideNone; font.pixelSize: Theme.small; color: Theme.fgSecondary }
                GField { id: expected; width: parent.width - 14; placeholderText: "Paste a checksum to verify"; onTextChanged: iw.verify() }
                GLabel { id: verdict; width: parent.width - 14; visible: text !== ""; wrapMode: Text.Wrap; elide: Text.ElideNone }
                Repeater {
                    model: ["md5", "sha1", "sha256", "sha512"]
                    Column {
                        required property string modelData
                        width: parent.width - 14
                        Row {
                            spacing: 8
                            GLabel { text: parent.parent.modelData.toUpperCase(); width: 60; font.pixelSize: Theme.small; color: Theme.fgSecondary; anchors.verticalCenter: parent.verticalCenter }
                            GButton {
                                text: iw.sums[parent.parent.modelData] ? "Copy" : "Calculate"
                                flat: false; capsule: true
                                onClicked: {
                                    const a = parent.parent.modelData
                                    if (iw.sums[a]) { iw.app.grove.copyText(iw.sums[a]); return }
                                    iw.computeSum(a)
                                }
                            }
                        }
                        TextEdit {
                            width: parent.width
                            visible: !!iw.sums[parent.modelData]
                            readOnly: true; selectByMouse: true
                            wrapMode: TextEdit.WrapAnywhere
                            text: iw.sums[parent.modelData] || ""
                            font.family: Theme.mono; font.pixelSize: Theme.caption
                            renderType: Text.NativeRendering
                            color: Theme.fg
                        }
                    }
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
