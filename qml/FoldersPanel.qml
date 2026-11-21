import QtQuick
import QtQuick.Controls.Basic
import Grove

// Dolphin's Folders panel (F7): the folder tree from the top of the
// computer; click to go there, the triangles open folders in place.
Item {
    id: fp
    property var win
    property string currentPath: win.activePane ? win.activePane.path : ""
    FolderModel {
        id: tree
        Component.onCompleted: { applyDirsOnly(true); applyShowHidden(false); applySort("name", false); loadPath("/") }
    }
    GlassPanel { anchors.fill: parent; tint: Theme.sidebar }
    GLabel { x: 12; y: 10; text: "Folders"; font.pixelSize: Theme.small; font.bold: true; color: Theme.fgTertiary }
    ListView {
        id: lv
        anchors.fill: parent
        anchors.topMargin: 30
        anchors.bottomMargin: 8
        clip: true
        model: tree
        reuseItems: true
        ScrollBar.vertical: GScrollBar {}
        delegate: Item {
            id: r
            required property int index
            required property string name
            required property string path
            required property bool expanded
            required property int depth
            required property string iconName
            width: lv.width
            height: Theme.sidebarRow - 4
            Rectangle {
                anchors.fill: parent; anchors.leftMargin: 6; anchors.rightMargin: 6
                radius: 6
                color: r.path === fp.currentPath ? Theme.selectedInactive : ma.containsMouse ? Theme.hover : "transparent"
            }
            GLabel {
                x: 10 + r.depth * 14
                width: 14
                anchors.verticalCenter: parent.verticalCenter
                text: "›"
                rotation: r.expanded ? 90 : 0
                color: Theme.fgSecondary
                MouseArea { anchors.fill: parent; anchors.margins: -4; onClicked: tree.toggleExpanded(r.index) }
            }
            GIcon { id: ic; x: 26 + r.depth * 14; anchors.verticalCenter: parent.verticalCenter; size: 16; name: r.iconName }
            GLabel { anchors.left: ic.right; anchors.leftMargin: 6; anchors.right: parent.right; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter; text: r.name; font.pixelSize: Theme.small }
            MouseArea {
                id: ma
                anchors.fill: parent
                anchors.leftMargin: 24 + r.depth * 14
                hoverEnabled: true
                acceptedButtons: Qt.LeftButton | Qt.MiddleButton
                onClicked: (m) => m.button === Qt.MiddleButton ? fp.win.newTab(r.path, []) : fp.win.go(r.path)
                onDoubleClicked: tree.toggleExpanded(r.index)
            }
            FolderDrop { anchors.fill: parent; pane: fp.win.activePane; path: r.path }
            GTip { visible: ma.containsMouse; text: r.path + " — click to open, double-click or the triangle to show its folders" }
        }
    }
}
