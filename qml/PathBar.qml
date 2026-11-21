import QtQuick
import QtQuick.Controls.Basic

// The path: clickable folder segments (drop files on one to move them
// there); click the empty part or press Ctrl+L to type a path, with
// completion of folder names (Tab or the list).
Item {
    id: pb
    property var pane
    property var grove: pane ? pane.grove : null
    property bool editing: false
    implicitHeight: Theme.controlHeight

    function segments() {
        if (!pane || !grove) return []
        const p = pane.path
        if (p.indexOf("/") !== 0) return [{ name: pane.title, path: p, icon: p.indexOf("trash:") === 0 ? "user-trash" : p.indexOf("recent:") === 0 ? "document-open-recent" : "edit-find" }]
        const home = grove.home()
        const out = []
        let base = "/"
        let rest = p
        if (p === home || p.indexOf(home + "/") === 0) {
            out.push({ name: grove.env("USER") || "Home", path: home, icon: "user-home" })
            base = home
            rest = p.substring(home.length)
        } else {
            out.push({ name: "Computer", path: "/", icon: "computer" })
            rest = p.substring(1)
        }
        let acc = base === "/" ? "" : base
        for (const s of rest.split("/")) {
            if (!s) continue
            acc += "/" + s
            out.push({ name: s, path: acc, icon: "" })
        }
        return out
    }

    function edit() {
        editing = true
        field.text = pane.path.indexOf("/") === 0 ? pane.path : grove.home()
        field.forceActiveFocus()
        field.selectAll()
    }
    function finish(go) {
        if (go) {
            let t = field.text.trim()
            if (t.indexOf("~") === 0) t = grove.home() + t.substring(1)
            if (t.indexOf("file://") === 0) t = decodeURIComponent(t.substring(7))
            if (t) pane.navigate(t)
        }
        editing = false
        completions.close()
        pane.forceActiveFocus()
    }

    Rectangle {
        anchors.fill: parent
        radius: height / 2
        color: Theme.field
        border.width: pb.editing ? 2 : 0
        border.color: Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.7)
    }

    // Segments.
    ListView {
        id: crumbs
        visible: !pb.editing
        anchors.fill: parent
        anchors.leftMargin: 6
        anchors.rightMargin: 6
        orientation: ListView.Horizontal
        interactive: false
        clip: true
        model: pb.pane ? pb.segments() : []
        onCountChanged: positionViewAtEnd()
        onWidthChanged: positionViewAtEnd()
        delegate: Item {
            id: seg
            required property var modelData
            required property int index
            readonly property bool last: index === crumbs.count - 1
            height: crumbs.height
            width: segRow.implicitWidth + 12
            Rectangle {
                anchors.fill: parent
                anchors.topMargin: 3; anchors.bottomMargin: 3
                radius: height / 2
                color: segDrop.containsDrag ? Theme.dropTarget : segMouse.containsMouse ? Theme.hover : "transparent"
            }
            Row {
                id: segRow
                anchors.centerIn: parent
                spacing: 4
                GIcon { visible: seg.modelData.icon !== ""; name: seg.modelData.icon; size: 15; anchors.verticalCenter: parent.verticalCenter }
                GLabel {
                    text: seg.modelData.name
                    font.bold: seg.last
                    color: seg.last ? Theme.fg : Theme.fgSecondary
                    anchors.verticalCenter: parent.verticalCenter
                }
                GLabel { visible: !seg.last; text: "›"; color: Theme.fgTertiary; anchors.verticalCenter: parent.verticalCenter }
            }
            MouseArea {
                id: segMouse
                anchors.fill: parent
                hoverEnabled: true
                acceptedButtons: Qt.LeftButton | Qt.MiddleButton
                onClicked: (m) => {
                    if (m.button === Qt.MiddleButton) pb.pane.win.newTab(seg.modelData.path, [])
                    else if (seg.last) pb.edit()
                    else pb.pane.navigate(seg.modelData.path, { select: [] })
                }
            }
            DropArea {
                id: segDrop
                anchors.fill: parent
                keys: ["text/uri-list"]
                onDropped: (d) => pb.pane.dropInto(seg.modelData.path, d)
            }
            GTip { visible: segMouse.containsMouse; text: seg.last ? "Click to type a path (Ctrl+L)" : "Go to " + seg.modelData.path + " (middle-click: new tab). Drop files here to move them." }
        }
        MouseArea {
            anchors.fill: parent
            z: -1
            onClicked: pb.edit()
        }
    }

    TextField {
        id: field
        visible: pb.editing
        anchors.fill: parent
        leftPadding: 12
        rightPadding: 12
        font.family: Theme.family
        font.pixelSize: Theme.body
        renderType: Text.NativeRendering
        color: Theme.fg
        selectionColor: Theme.accent
        selectedTextColor: Theme.accentText
        verticalAlignment: TextInput.AlignVCenter
        background: Item {}
        placeholderText: "Type a folder path, e.g. ~/Documents"
        Keys.onReturnPressed: {
            if (completions.opened && list.currentIndex >= 0) { text = list.model[list.currentIndex] + "/"; completions.close() }
            else pb.finish(true)
        }
        Keys.onEscapePressed: pb.finish(false)
        Keys.onTabPressed: {
            if (list.count > 0) text = list.model[Math.max(0, list.currentIndex)] + "/"
        }
        Keys.onDownPressed: if (completions.opened) list.currentIndex = Math.min(list.count - 1, list.currentIndex + 1)
        Keys.onUpPressed: if (completions.opened) list.currentIndex = Math.max(-1, list.currentIndex - 1)
        onActiveFocusChanged: if (!activeFocus && pb.editing && !completions.opened) pb.finish(false)
        onTextEdited: {
            const t = text
            Async.call(pb.grove, k => pb.grove.completePath(k, t), r => {
                if (field.text !== t) return
                list.model = r
                list.currentIndex = -1
                if (r.length && pb.editing) completions.open(); else completions.close()
            })
        }
    }

    Popup {
        id: completions
        y: pb.height + 4
        width: pb.width
        height: Math.min(list.contentHeight + 12, 300)
        padding: 6
        topPadding: 6.001
        leftPadding: 6.001
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
        background: GlassPanel { tint: Theme.glassMenu }
        contentItem: ListView {
            id: list
            clip: true
            model: []
            delegate: Rectangle {
                required property var modelData
                required property int index
                width: list.width
                height: Theme.menuRow
                radius: 6
                color: index === list.currentIndex ? Theme.accent : cm.containsMouse ? Theme.hover : "transparent"
                GLabel {
                    x: 8
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - 16
                    text: modelData
                    elide: Text.ElideMiddle
                    color: index === list.currentIndex ? Theme.accentText : Theme.fg
                }
                MouseArea { id: cm; anchors.fill: parent; hoverEnabled: true; onClicked: { field.text = modelData + "/"; field.forceActiveFocus(); completions.close() } }
            }
        }
    }
}
