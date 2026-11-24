import QtQuick

// Disk usage mode: a small spinner while an item's size is worked out.
Item {
    id: sp
    property bool running: false
    property color color: Theme.fgSecondary
    implicitWidth: 14
    implicitHeight: 14
    visible: running
    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: "transparent"
        border.width: 2
        border.color: Qt.rgba(sp.color.r, sp.color.g, sp.color.b, 0.25)
    }
    // The moving arc: a quarter of the ring.
    Item {
        clip: true
        Item {
            x: -sp.width / 2
            width: sp.width; height: sp.height
            Rectangle {
                width: sp.width; height: sp.height
                radius: width / 2
                color: "transparent"
                border.width: 2
                border.color: sp.color
            }
        }
        // (A quarter shows: the clip box is the top-right quarter.)
        width: sp.width / 2; height: sp.height / 2
        x: sp.width / 2
    }
    RotationAnimation on rotation {
        running: sp.running && sp.visible
        from: 0; to: 360
        duration: 900
        loops: Animation.Infinite
    }
}
