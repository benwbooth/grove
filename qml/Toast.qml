import QtQuick

// A short message at the bottom of the window (plain language). Errors
// stay longer and can be clicked away.
Item {
    id: t
    width: Math.min(box.implicitWidth, parent ? parent.width - 80 : 600)
    height: box.height
    opacity: 0
    visible: opacity > 0
    z: 50
    property string level: "info"
    function show(lv, msg) {
        level = lv
        label.text = msg
        fadeIn.restart()
        hide.interval = lv === "error" ? 9000 : 3000
        hide.restart()
    }
    NumberAnimation { id: fadeIn; target: t; property: "opacity"; to: 1; duration: 150 }
    NumberAnimation { id: fadeOut; target: t; property: "opacity"; to: 0; duration: 250 }
    Timer { id: hide; onTriggered: fadeOut.restart() }
    GlassPanel {
        id: box
        width: t.width
        height: label.implicitHeight + 20
        implicitWidth: label.implicitWidth + 50
        tint: t.level === "error" ? (Theme.dark ? Qt.rgba(0.35, 0.12, 0.12, 0.96) : Qt.rgba(1, 0.92, 0.92, 0.98)) : Theme.glassMenu
        radius: 14
        GIcon {
            x: 12
            anchors.verticalCenter: parent.verticalCenter
            size: 16
            name: t.level === "error" ? "dialog-error" : "dialog-information"
        }
        GLabel {
            id: label
            x: 36
            width: Math.min(implicitWidth, 560)
            anchors.verticalCenter: parent.verticalCenter
            wrapMode: Text.Wrap
            elide: Text.ElideNone
        }
        MouseArea { anchors.fill: parent; onClicked: fadeOut.restart() }
    }
}
