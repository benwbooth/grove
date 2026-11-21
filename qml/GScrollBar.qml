import QtQuick
import QtQuick.Controls.Basic

// macOS overlay scroll bar: a thin rounded thumb over the content that
// shows while scrolling and fades out ~1 s after it stops; hovering it
// widens the thumb and shows a faint track, and it can be dragged or
// clicked like any scroll bar. Attach as `ScrollBar.vertical: GScrollBar {}`.
ScrollBar {
    id: sb
    // Shown while the view scrolls (content moves) or the bar is in use.
    property bool recent: false
    readonly property bool shown: size < 1 && (recent || hovered || pressed)

    policy: ScrollBar.AsNeeded
    minimumSize: 0.04
    padding: 2
    hoverEnabled: true
    implicitWidth: vertical ? (hovered || pressed ? 15 : 11) : 100
    implicitHeight: vertical ? 100 : (hovered || pressed ? 15 : 11)
    Behavior on implicitWidth { NumberAnimation { duration: 120; easing.type: Easing.OutCubic } }
    Behavior on implicitHeight { NumberAnimation { duration: 120; easing.type: Easing.OutCubic } }

    opacity: shown ? 1 : 0
    // Appear at once, fade out slowly (macOS: ~0.3 s fade after ~1 s idle).
    Behavior on opacity { NumberAnimation { duration: sb.shown ? 80 : 320 } }
    visible: size < 1

    Timer { id: idle; interval: 1000; onTriggered: sb.recent = false }
    function poke() { recent = true; idle.restart() }
    onPositionChanged: if (!pressed) poke()
    onPressedChanged: poke()

    background: Rectangle {
        radius: Math.min(width, height) / 2
        color: Theme.dark ? Qt.rgba(1, 1, 1, 0.06) : Qt.rgba(0, 0, 0, 0.04)
        border.color: Theme.dark ? Qt.rgba(1, 1, 1, 0.08) : Qt.rgba(0, 0, 0, 0.06)
        border.width: 1
        opacity: sb.hovered || sb.pressed ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: 120 } }
    }
    contentItem: Rectangle {
        implicitWidth: 7
        implicitHeight: 7
        radius: Math.min(width, height) / 2
        color: Theme.dark ? Qt.rgba(1, 1, 1, sb.pressed ? 0.62 : 0.45)
                          : Qt.rgba(0, 0, 0, sb.pressed ? 0.55 : 0.4)
    }
}
