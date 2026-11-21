import QtQuick
import QtQuick.Controls.Basic

// One tab: a pane, or two side by side (F3, like Dolphin). The active pane
// is the one the toolbar and keyboard act on.
Item {
    id: tp
    property var win
    property string initialPath: ""
    property var initialSelect: []
    property bool split: false
    property var activePane: first
    readonly property string title: activePane ? activePane.title : ""

    function toggleSplit() {
        split = !split
        if (split) {
            second.navigate(first.path)
            activePane = second
            second.forceActiveFocus()
        } else {
            activePane = first
            first.forceActiveFocus()
        }
    }
    function switchPane() {
        if (!split) return
        activePane = activePane === first ? second : first
        activePane.forceActiveFocus()
    }
    function otherPane() { return split ? (activePane === first ? second : first) : null }

    SplitView {
        anchors.fill: parent
        orientation: Qt.Horizontal
        handle: Rectangle {
            implicitWidth: 1
            color: Theme.separator
            Rectangle { anchors.centerIn: parent; width: 7; height: parent.height; color: "transparent" }
        }
        Pane {
            id: first
            win: tp.win
            SplitView.fillWidth: true
            SplitView.minimumWidth: 200
            active: tp.activePane === first && (tp.win ? tp.win.active : true)
            onActivated: tp.activePane = first
            focus: true
        }
        Pane {
            id: second
            win: tp.win
            visible: tp.split
            SplitView.preferredWidth: tp.width / 2
            SplitView.minimumWidth: 200
            active: tp.activePane === second && (tp.win ? tp.win.active : true)
            onActivated: tp.activePane = second
        }
    }
    // A tint on the inactive pane in split view.
    Rectangle {
        visible: tp.split
        x: tp.activePane === first ? second.x : first.x
        width: tp.activePane === first ? second.width : first.width
        height: 2
        color: "transparent"
    }
    Rectangle {
        visible: tp.split
        x: tp.activePane.x
        width: tp.activePane.width
        height: 2
        color: Theme.accent
    }
    Component.onCompleted: first.navigate(initialPath || win.app.grove.home(), { select: initialSelect })
}
