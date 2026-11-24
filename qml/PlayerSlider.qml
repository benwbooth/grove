import QtQuick

// Grove Player's thin white slider (QuickTime's scrubber and volume): a
// track, what's played in white, what's buffered in grey, chapter ticks,
// a knob under the pointer, the time under the pointer as a tooltip.
//     PlayerSlider { value: pos / len; onMoved: v => seek(v * len); hoverText: f => fmt(f * len) }
Item {
    id: s
    property real value: 0
    // Buffered up to (streams), 0..1.
    property real extra: 0
    property var chapters: []
    property string tip: ""
    // The tooltip for a point (0..1) under the pointer; null: `tip`.
    property var hoverText: null
    signal moved(real v)
    implicitHeight: 16
    readonly property real shown: area.pressed ? dragValue : value
    property real dragValue: 0

    Rectangle { id: track; anchors.verticalCenter: parent.verticalCenter; width: parent.width; height: area.containsMouse || area.pressed ? 6 : 4; radius: height / 2; color: Qt.rgba(1, 1, 1, 0.25)
        Behavior on height { NumberAnimation { duration: 100 } } }
    Rectangle { anchors.verticalCenter: parent.verticalCenter; width: parent.width * Math.max(0, Math.min(1, s.extra)); height: track.height; radius: track.radius; color: Qt.rgba(1, 1, 1, 0.22); visible: s.extra > s.shown }
    Rectangle { anchors.verticalCenter: parent.verticalCenter; width: parent.width * Math.max(0, Math.min(1, s.shown)); height: track.height; radius: track.radius; color: "white" }
    Repeater {
        model: s.chapters
        Rectangle { required property var modelData; x: Math.round(modelData * s.width) - 1; anchors.verticalCenter: parent.verticalCenter; width: 2; height: track.height; color: Qt.rgba(0, 0, 0, 0.55); visible: modelData > 0.002 }
    }
    Rectangle {
        x: s.width * Math.max(0, Math.min(1, s.shown)) - width / 2
        anchors.verticalCenter: parent.verticalCenter
        width: 12; height: 12; radius: 6; color: "white"
        visible: area.containsMouse || area.pressed
    }
    MouseArea {
        id: area
        anchors.fill: parent
        anchors.topMargin: -6; anchors.bottomMargin: -6
        hoverEnabled: true
        function at(x) { return Math.max(0, Math.min(1, x / s.width)) }
        onPressed: (m) => { s.dragValue = at(m.x); s.moved(s.dragValue) }
        onPositionChanged: (m) => { if (pressed) { s.dragValue = at(m.x); s.moved(s.dragValue) } }
    }
    GTip {
        visible: area.containsMouse && (s.hoverText !== null || s.tip !== "")
        text: s.hoverText !== null ? s.hoverText(area.at(area.mouseX)) : s.tip
        // The scrubber's time follows the pointer at once.
        delay: s.hoverText !== null ? 0 : Theme.tooltipDelay
        x: s.hoverText !== null ? area.mouseX - width / 2 : 0
        y: -height - 10
    }
}
