import QtQuick
import QtQuick.Controls.Basic

MenuItem {
    id: mi
    property string shortcutText: ""
    property string iconName: ""
    property color swatch: "transparent"
    property string tip: ""
    implicitHeight: Theme.menuRow
    implicitWidth: Math.max(Math.round(200 * Theme.pt), lbl.implicitWidth + sc.implicitWidth + 70)
    hoverEnabled: true
    background: Rectangle {
        radius: Math.round(8 * Theme.pt)
        color: mi.highlighted && mi.enabled ? Theme.accent : "transparent"
    }
    indicator: Item {}
    arrow: GLabel {
        visible: mi.subMenu !== null
        x: mi.width - width - 8
        anchors.verticalCenter: parent.verticalCenter
        text: "›"
        font.pixelSize: Theme.title
        color: mi.highlighted ? Theme.accentText : Theme.fgSecondary
    }
    contentItem: Item {
        implicitWidth: lbl.implicitWidth + sc.implicitWidth + 60
        // On state of a toggle with an icon: a soft accent square behind it.
        Rectangle {
            visible: mi.checkable && mi.checked && mi.iconName !== "" && mi.swatch.a === 0
            x: -1
            width: 22; height: 22; radius: 5
            anchors.verticalCenter: parent.verticalCenter
            color: mi.highlighted ? Qt.rgba(1, 1, 1, 0.25) : Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.28)
        }
        GLabel {
            visible: mi.checkable && mi.checked && mi.iconName === ""
            x: 4
            anchors.verticalCenter: parent.verticalCenter
            text: "✓"
            color: mi.highlighted ? Theme.accentText : Theme.fg
        }
        Rectangle {
            visible: mi.swatch.a > 0
            x: 4
            anchors.verticalCenter: parent.verticalCenter
            width: 11; height: 11; radius: 6
            color: mi.swatch
            border.width: 1
            border.color: Qt.rgba(0, 0, 0, 0.2)
        }
        GIcon {
            id: rowIcon
            visible: mi.iconName !== "" && mi.swatch.a === 0
            name: mi.iconName
            size: 16
            crisp: true
            x: 2 + (16 - width) / 2
            anchors.verticalCenter: parent.verticalCenter
            // Placed again whenever the menu opens or moves (it opens at a
            // new spot, then is moved to stay inside the window).
            Connections {
                target: mi.menu
                function onOpened() { Qt.callLater(rowIcon.snap) }
                function onXChanged() { Qt.callLater(rowIcon.snap) }
                function onYChanged() { Qt.callLater(rowIcon.snap) }
            }
        }
        GLabel {
            id: lbl
            x: 24
            width: parent.width - x - sc.width - 8
            anchors.verticalCenter: parent.verticalCenter
            text: mi.text
            color: !mi.enabled ? Theme.fgDisabled : mi.highlighted ? Theme.accentText : Theme.fg
        }
        GLabel {
            id: sc
            anchors.right: parent.right
            anchors.rightMargin: mi.subMenu ? 18 : 4
            anchors.verticalCenter: parent.verticalCenter
            text: mi.shortcutText
            color: mi.highlighted ? Theme.accentText : Theme.fgSecondary
        }
    }
    GTip { visible: mi.hovered && mi.tip !== ""; text: mi.tip }
}
