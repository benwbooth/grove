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
    property string cloud: ""
    property bool cut: false
    property bool hidden: false
    property string info: ""
    // A video's length or an archive's item count, on the icon.
    property string badge: ""
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
        border.width: cell.pane.selectionMode && cell.pane.model.currentRow === cell.index ? 2 : 0
        border.color: Theme.accent
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
    // Still being made: a small spinner in the corner (the icon stands in).
    PreviewSpinner {
        image: th
        size: Math.max(10, Math.round(cell.icon * 0.2))
        x: iconBg.x + iconBg.width - width - 7
        y: iconBg.y + iconBg.height - height - 6
        z: 2
    }
    Rectangle {
        id: badgeBox
        visible: cell.badge !== "" && cell.icon >= 48
        z: 2
        readonly property real imgW: th.status === Image.Ready ? Math.min(th.paintedWidth, cell.icon) : cell.icon * 0.8
        readonly property real imgH: th.status === Image.Ready ? Math.min(th.paintedHeight, cell.icon) : cell.icon * 0.8
        x: Math.round(iconBg.x + (iconBg.width + imgW) / 2 - width - 3)
        y: Math.round(iconBg.y + (iconBg.height + imgH) / 2 - height - 3)
        width: badgeText.implicitWidth + 8
        height: badgeText.implicitHeight + 2
        radius: height / 2
        color: Qt.rgba(0, 0, 0, 0.62)
        GLabel {
            id: badgeText
            anchors.centerIn: parent
            text: cell.badge
            color: "white"
            font.pixelSize: Math.max(9, Math.round(cell.grid.textPx * 0.8))
            font.weight: Font.DemiBold
        }
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
    // Google Drive (canopy): bottom left, beside the alias arrow.
    CloudBadge { status: cell.cloud; x: iconBg.x + (cell.isLink ? 22 : 2); y: iconBg.y + iconBg.height - height - 2 }
    // Selection mode (Dolphin): a checkbox on every item.
    SelectCheck {
        visible: cell.pane.selectionMode
        x: iconBg.x + 2; y: iconBg.y + 2
        z: 3
        pane: cell.pane
        row: cell.index
        checked: cell.selected
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
                visible: (cell.grid.itemInfo || (cell.grid.pane && cell.grid.pane.usageOn)) && cell.info !== ""
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
