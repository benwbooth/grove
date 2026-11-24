import QtQuick

// Disk usage mode: an item's share of its folder as a soft bar behind its
// name (relative to the folder's largest item). The darker part is what
// deleting it would free; the lighter rest is shared with other files or
// snapshots. Nothing while it's still being worked out.
Item {
    id: ub
    property var pane
    property string path: ""
    readonly property bool on: !!pane && pane.usageOn && path !== ""
    readonly property real frac: on ? pane.usageFraction(path) : 0
    readonly property real excl: on ? pane.usageExclusive(path) : 0
    visible: on && frac > 0
    Rectangle {
        width: Math.max(3, parent.width * ub.frac)
        height: parent.height
        radius: Math.min(4, height / 2)
        color: Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, Theme.dark ? 0.18 : 0.14)
        Behavior on width { NumberAnimation { duration: Theme.reduceMotion ? 0 : 220; easing.type: Easing.OutCubic } }
    }
    Rectangle {
        width: ub.excl > 0 ? Math.max(2, parent.width * ub.excl) : 0
        height: parent.height
        radius: Math.min(4, height / 2)
        color: Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, Theme.dark ? 0.32 : 0.26)
        Behavior on width { NumberAnimation { duration: Theme.reduceMotion ? 0 : 220; easing.type: Easing.OutCubic } }
    }
}
