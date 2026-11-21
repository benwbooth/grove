import QtQuick

// A small "working" indicator.
Row {
    id: b
    property bool running: false
    spacing: 4
    visible: running
    Repeater {
        model: 3
        Rectangle {
            required property int index
            width: 6; height: 6; radius: 3
            color: Theme.fgSecondary
            SequentialAnimation on opacity {
                running: b.running
                loops: Animation.Infinite
                PauseAnimation { duration: index * 150 }
                NumberAnimation { from: 0.2; to: 1; duration: 300 }
                NumberAnimation { from: 1; to: 0.2; duration: 300 }
                PauseAnimation { duration: (2 - index) * 150 }
            }
        }
    }
}
