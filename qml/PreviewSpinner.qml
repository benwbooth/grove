import QtQuick

// A small macOS-style spinner for a preview that's still being made. It
// waits a moment before showing, so cached previews never flash it.
// `image`: the Image whose loading it stands for (or set `pending`).
Item {
    id: sp
    property var image: null
    property bool pending: image ? image.status === Image.Loading : false
    property int size: 16
    property int delay: 250
    property color color: Theme.fgSecondary
    width: size
    height: size
    visible: shown
    property bool shown: false
    onPendingChanged: { if (pending) wait.restart(); else { wait.stop(); shown = false } }
    Component.onCompleted: if (pending) wait.start()
    Timer { id: wait; interval: sp.delay; onTriggered: sp.shown = sp.pending }

    // A soft disc behind it, so it reads over any picture.
    property bool backdrop: true
    Rectangle {
        visible: sp.backdrop
        anchors.centerIn: parent
        width: sp.size + 6; height: width; radius: width / 2
        color: Theme.glass
        border.width: 1
        border.color: Theme.glassEdge
    }
    Item {
        id: wheel
        anchors.fill: parent
        Repeater {
            model: 8
            Rectangle {
                required property int index
                x: (sp.size - width) / 2
                y: 0
                width: Math.max(1.5, sp.size * 0.12)
                height: sp.size * 0.3
                radius: width / 2
                color: sp.color
                opacity: 0.2 + 0.8 * (index / 7)
                transform: Rotation { origin.x: width / 2; origin.y: sp.size / 2; angle: index * 45 }
            }
        }
        RotationAnimator on rotation {
            running: sp.shown && !Theme.reduceMotion
            from: 0; to: 360
            duration: 900
            loops: Animation.Infinite
        }
    }
}
