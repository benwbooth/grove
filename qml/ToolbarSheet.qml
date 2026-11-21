import QtQuick
import QtQuick.Controls.Basic

// Finder's "Customize Toolbar…" sheet: drag items from the palette into
// the toolbar row, drag them along it to reorder, or out of it to remove
// them. Space and Flexible Space push items apart. Saved in settings.
Sheet {
    id: ts
    property var win
    readonly property var current: win ? win.toolbarItems : []
    title: "Customize Toolbar"
    message: "Drag your favourite items into the toolbar row. Drag them along it to reorder, or out of it to remove them."
    width: Math.min(parent ? parent.width - 40 : 900, Math.round(820 * Theme.pt))

    function set(list) { win.setSetting("toolbar", list) }
    function insertAt(kind, at) {
        const l = current.slice()
        if (kind !== "space" && kind !== "flexspace" && l.indexOf(kind) >= 0) l.splice(l.indexOf(kind), 1)
        l.splice(Math.max(0, Math.min(at, l.length)), 0, kind)
        set(l)
    }
    function moveTo(from, at) {
        const l = current.slice()
        const k = l.splice(from, 1)[0]
        l.splice(at > from ? at - 1 : at, 0, k)
        set(l)
    }
    function removeAt(i) { const l = current.slice(); l.splice(i, 1); set(l) }

    // --- dragging (inside the sheet) ---
    property string dragKind: ""
    property int dragFrom: -1           // index in the toolbar row, -1: from the palette
    property int dropAt: -1
    function chipW(kind) { return kind === "flexspace" ? 110 : kind === "space" ? 44 : kind === "path" ? 150 : kind === "search" ? 120 : 64 }
    function indexAt(xInRow) {
        let x = 0
        for (let i = 0; i < current.length; i++) {
            const w = chipW(current[i]) + rowFlow.spacing
            if (xInRow < x + w / 2) return i
            x += w
        }
        return current.length
    }
    function dragMoved(item, mx, my) {
        const p = item.mapToItem(ghost.parent, mx, my)
        ghost.x = p.x - ghost.width / 2
        ghost.y = p.y - ghost.height / 2
        const r = item.mapToItem(rowBox, mx, my)
        dropAt = r.x >= 0 && r.y >= -20 && r.x <= rowBox.width && r.y <= rowBox.height + 20 ? indexAt(r.x - rowFlow.x) : -1
    }
    function dragEnded() {
        if (dragKind === "") return
        if (dropAt >= 0) {
            if (dragFrom >= 0) moveTo(dragFrom, dropAt)
            else insertAt(dragKind, dropAt)
        } else if (dragFrom >= 0) removeAt(dragFrom)
        dragKind = ""; dragFrom = -1; dropAt = -1
    }

    component Chip: Item {
        id: chip
        property string kind: ""
        property int fromIndex: -1
        readonly property var info: ts.win ? ts.win.toolbarCatalog[kind] || { label: kind, icon: "" } : { label: kind, icon: "" }
        width: ts.chipW(kind)
        height: 58
        opacity: ts.dragKind !== "" && ts.dragFrom === fromIndex && fromIndex >= 0 ? 0.3 : 1
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            y: 2
            width: parent.width - 4
            height: 30
            radius: chip.kind === "path" || chip.kind === "search" ? height / 2 : Theme.radius
            color: chip.kind === "space" || chip.kind === "flexspace" ? "transparent" : (cm.containsMouse ? Theme.pressed : Theme.field)
            border.width: chip.kind === "space" || chip.kind === "flexspace" ? 1 : 0
            border.color: Theme.fgTertiary
            GIcon {
                anchors.centerIn: parent
                visible: chip.info.icon !== ""
                size: 16
                name: Theme.glyph(chip.info.icon)
                tint: Theme.fg
            }
            GLabel {
                visible: chip.kind === "flexspace"
                anchors.centerIn: parent
                text: "⟷"
                color: Theme.fgSecondary
            }
        }
        GLabel {
            anchors.horizontalCenter: parent.horizontalCenter
            y: 36
            width: parent.width + 8
            horizontalAlignment: Text.AlignHCenter
            text: chip.info.label
            font.pixelSize: Theme.caption
            color: Theme.fgSecondary
            elide: Text.ElideRight
        }
        MouseArea {
            id: cm
            anchors.fill: parent
            hoverEnabled: true
            preventStealing: true
            onPressed: (m) => { ts.dragKind = chip.kind; ts.dragFrom = chip.fromIndex; ts.dragMoved(chip, m.x, m.y) }
            onPositionChanged: (m) => { if (pressed) ts.dragMoved(chip, m.x, m.y) }
            onReleased: ts.dragEnded()
            onCanceled: { ts.dragKind = ""; ts.dragFrom = -1; ts.dropAt = -1 }
        }
        GTip { visible: cm.containsMouse && !cm.pressed; text: chip.info.tip || chip.info.label }
    }

    // The toolbar as it is now.
    Rectangle {
        id: rowBox
        width: parent.width
        height: 76
        radius: 10
        color: Theme.window
        border.width: ts.dropAt >= 0 ? 2 : 1
        border.color: ts.dropAt >= 0 ? Theme.accent : Theme.separator
        Flow {
            id: rowFlow
            x: 10; y: 9
            width: parent.width - 20
            spacing: 4
            Repeater {
                model: ts.current
                Chip { required property string modelData; required property int index; kind: modelData; fromIndex: index }
            }
        }
        // Where the dragged item goes.
        Rectangle {
            visible: ts.dropAt >= 0
            x: {
                let x = rowFlow.x
                for (let i = 0; i < Math.min(ts.dropAt, ts.current.length); i++) x += ts.chipW(ts.current[i]) + rowFlow.spacing
                return x - 3
            }
            y: 8; width: 3; height: 34; radius: 1.5
            color: Theme.accent
        }
    }
    GLabel { text: "Drag items into the toolbar:"; color: Theme.fgSecondary; font.pixelSize: Theme.small }
    Flow {
        width: parent.width
        spacing: 6
        Repeater {
            model: ts.win ? ts.win.toolbarAll : []
            Chip { required property string modelData; kind: modelData }
        }
    }
    GLabel { text: "…or put back the default set:"; color: Theme.fgSecondary; font.pixelSize: Theme.small }
    Rectangle {
        width: parent.width
        height: 66
        radius: 10
        color: dm.containsMouse ? Theme.hover : "transparent"
        border.width: 1
        border.color: Theme.separator
        Flow {
            x: 10; y: 4
            width: parent.width - 20
            spacing: 4
            enabled: false
            Repeater {
                model: ts.win ? ts.win.toolbarDefaults : []
                Chip { required property string modelData; kind: modelData }
            }
        }
        MouseArea { id: dm; anchors.fill: parent; hoverEnabled: true; onClicked: ts.set([]) }
        GTip { visible: dm.containsMouse; text: "Click to restore the default toolbar" }
    }

    // What follows the pointer while dragging.
    Rectangle {
        id: ghost
        parent: ts.parent
        visible: ts.dragKind !== ""
        z: 1000
        width: 48; height: 32; radius: Theme.radius
        color: Theme.accent
        opacity: 0.85
        GIcon {
            anchors.centerIn: parent
            size: 16
            name: ts.win && ts.dragKind && ts.win.toolbarCatalog[ts.dragKind] ? Theme.glyph(ts.win.toolbarCatalog[ts.dragKind].icon) : ""
            tint: Theme.accentText
        }
    }

    buttons: [
        PushButton { text: "Restore Defaults"; tip: "Put back grove's standard toolbar"; onClicked: ts.set([]) },
        PushButton { text: "Done"; primary: true; tip: "Close (changes are already saved)"; onClicked: ts.close() }
    ]
}
