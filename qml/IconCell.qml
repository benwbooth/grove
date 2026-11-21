import QtQuick
import QtQuick.Controls.Basic

// One item in icon view: icon or thumbnail, name (under it or to its
// right, Finder's label position), tag dots, badges, an optional info line,
// a checkbox in selection mode. Shared by the plain grid and the grouped /
// stacked layout.
Item {
    id: cell
    property var grid              // IconGrid: sizes and options
    property var pane: grid.pane
    property int index: -1
    property string name: ""
    property string path: ""
    property bool isDir: false
    property bool isLink: false
    property bool selected: false
    property string iconName: ""
    property bool thumb: false
    property real mtime: 0
    property string tags: ""
    property string tagColors: ""
    property string git: ""
    property bool cut: false
    property bool hidden: false
    property string info: ""
    readonly property bool renaming: pane.renamingRow === index
    readonly property bool atRight: grid.labelRight
    readonly property int icon: grid.icon
    readonly property bool hovered: mouse.containsMouse

    Rectangle {
        id: iconBg
        x: cell.atRight ? 4 : Math.round((cell.width - width) / 2)
        y: cell.atRight ? Math.round((cell.height - height) / 2) : 4
        width: cell.icon + 14
        height: cell.icon + 10
        radius: Math.round(8 * Theme.pt)
        color: cell.selected ? Theme.selectedInactive : (drop.hovering ? Theme.dropTarget : (mouse.containsMouse ? Theme.hover : "transparent"))
    }
    GIcon {
        id: ic
        anchors.centerIn: iconBg
        size: cell.icon
        name: cell.iconName
        visible: th.status !== Image.Ready || th.implicitWidth <= 1
        opacity: cell.cut || cell.hidden ? 0.5 : 1
    }
    Image {
        id: th
        anchors.centerIn: iconBg
        width: cell.icon
        height: cell.icon
        fillMode: Image.PreserveAspectFit
        asynchronous: true
        cache: true
        smooth: true
        sourceSize.width: Math.ceil(cell.icon * Screen.devicePixelRatio)
        sourceSize.height: Math.ceil(cell.icon * Screen.devicePixelRatio)
        source: cell.thumb && cell.grid.thumbs && cell.icon >= 32 ? "image://thumb/" + encodeURIComponent(cell.path) + "?m=" + cell.mtime : ""
        opacity: cell.cut || cell.hidden ? 0.5 : 1
    }
    // Alias arrow.
    Rectangle {
        visible: cell.isLink
        x: iconBg.x + 6; y: iconBg.y + iconBg.height - 18
        width: 14; height: 14; radius: 3
        color: "white"
        border.color: Qt.rgba(0, 0, 0, 0.3)
        GLabel { anchors.centerIn: parent; text: "↗"; color: "black"; font.pixelSize: 10 }
    }
    GitBadge { status: cell.git; x: iconBg.x + iconBg.width - width - 2; y: iconBg.y + 2 }
    // Selection mode (Dolphin): a checkbox on every item.
    Rectangle {
        id: check
        visible: cell.pane.selectionMode
        x: iconBg.x + 2; y: iconBg.y + 2
        width: 18; height: 18; radius: 9
        color: cell.selected ? Theme.accent : Qt.rgba(Theme.content.r, Theme.content.g, Theme.content.b, 0.85)
        border.width: cell.selected ? 0 : 1.5
        border.color: Theme.fgTertiary
        GLabel { anchors.centerIn: parent; visible: cell.selected; text: "✓"; color: Theme.accentText; font.pixelSize: 12; font.bold: true }
        MouseArea {
            id: checkMouse
            anchors.fill: parent
            anchors.margins: -3
            hoverEnabled: true
            z: 3
            onClicked: { cell.pane.activated(); cell.pane.forceActiveFocus(); cell.pane.model.select(cell.index, "toggle") }
        }
        GTip { visible: checkMouse.containsMouse; text: cell.selected ? "Deselect this item (Space)" : "Select this item (Space)" }
        z: 3
    }

    Rectangle {
        id: labelBg
        visible: !cell.renaming
        x: cell.atRight ? iconBg.x + iconBg.width + 4 : Math.round((cell.width - width) / 2)
        y: cell.atRight ? Math.round((cell.height - height) / 2) : iconBg.y + iconBg.height + 3
        width: cell.atRight ? Math.min(labelRow.implicitWidth + 10, cell.width - x - 2) : Math.min(label.implicitWidth, cell.width - 6) + 10 + (dots.visible ? dots.width + 4 : 0)
        height: labelCol.height + 2
        radius: 5
        color: cell.selected ? Theme.accent : "transparent"
        Column {
            id: labelCol
            x: cell.atRight ? 5 : Math.round((parent.width - width) / 2)
            y: 1
            Row {
                id: labelRow
                spacing: 4
                anchors.horizontalCenter: cell.atRight ? undefined : parent.horizontalCenter
                TagDots { id: dots; tags: cell.tags; colors: cell.tagColors; anchors.verticalCenter: parent.verticalCenter }
                GLabel {
                    id: label
                    width: Math.min(implicitWidth, (cell.atRight ? cell.width - iconBg.width - 24 : cell.width - 16) - (dots.visible ? dots.width + 4 : 0))
                    text: cell.name
                    font.pixelSize: cell.grid.textPx
                    horizontalAlignment: cell.atRight ? Text.AlignLeft : Text.AlignHCenter
                    wrapMode: Text.WrapAnywhere
                    maximumLineCount: cell.selected ? 4 : cell.grid.labelLines
                    elide: Text.ElideRight
                    color: cell.selected ? Theme.accentText : cell.grid.textColor
                    opacity: cell.cut || cell.hidden ? 0.6 : 1
                }
            }
            GLabel {
                visible: cell.grid.itemInfo && cell.info !== ""
                anchors.horizontalCenter: cell.atRight ? undefined : parent.horizontalCenter
                text: cell.info
                font.pixelSize: Math.max(9, cell.grid.textPx - 2)
                color: cell.selected ? Theme.accentText : Theme.accent
            }
        }
    }
    Loader {
        active: cell.renaming
        x: cell.atRight ? labelBg.x : 3
        y: labelBg.y - 1
        width: cell.atRight ? cell.width - labelBg.x - 2 : cell.width - 6
        sourceComponent: RenameField { pane: cell.pane; row: cell.index; original: cell.name; isDir: cell.isDir }
    }

    ItemMouse {
        id: mouse
        x: Math.min(iconBg.x, labelBg.x)
        y: Math.min(iconBg.y, labelBg.y)
        width: Math.max(iconBg.x + iconBg.width, labelBg.x + labelBg.width) - x
        height: Math.max(iconBg.y + iconBg.height, labelBg.y + labelBg.height) - y
        pane: cell.pane
        row: cell.index
        iconUrl: th.status === Image.Ready ? th.source : Theme.iconUrl(cell.iconName)
        enabled: !cell.renaming
    }
    FolderDrop {
        id: drop
        anchors.fill: mouse
        pane: cell.pane
        path: cell.path
        enabledFor: cell.isDir && !cell.pane.special
    }
    PreviewTip {
        hovering: mouse.containsMouse && !mouse.pressed && !cell.renaming
        pane: cell.pane
        row: cell.index
        path: cell.path
        name: cell.name
        thumb: cell.thumb
        mtime: cell.mtime
        truncated: label.truncated
    }
}
