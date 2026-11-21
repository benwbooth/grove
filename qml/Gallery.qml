import QtQuick
import QtQuick.Controls.Basic

// Gallery view: the selected item large, a filmstrip of the folder below.
Item {
    id: root
    property var pane
    property var fm: pane.model
    property var current: null
    readonly property int stripH: Math.round(96 * Theme.pt)

    Connections {
        target: root.fm
        function onCurrentRowChanged() { root.current = root.fm.currentRow >= 0 ? root.pane.itemAt(root.fm.currentRow) : null }
        function onLoaded() { if (root.fm.currentRow < 0 && root.fm.count) root.fm.select(0, "set") }
    }
    Component.onCompleted: if (fm.count && fm.currentRow < 0) fm.select(0, "set")

    function positionAt(row) { if (row >= 0) strip.positionViewAtIndex(row, ListView.Center) }
    function handleKey(e) {
        const shift = (e.modifiers & Qt.ShiftModifier) !== 0
        if (e.key === Qt.Key_Right || e.key === Qt.Key_Down) { pane.step(1, shift); return true }
        if (e.key === Qt.Key_Left || e.key === Qt.Key_Up) { pane.step(-1, shift); return true }
        return false
    }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onPressed: (m) => { root.pane.activated(); root.pane.forceActiveFocus(); if (m.button === Qt.RightButton) root.pane.showBackgroundMenu() }
    }

    Item {
        id: stage
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: strip.top
        anchors.margins: 16
        GIcon {
            anchors.centerIn: parent
            size: Math.min(256, Math.min(stage.width, stage.height))
            name: root.current ? root.current.icon : ""
            visible: big.status !== Image.Ready
        }
        Image {
            id: big
            anchors.fill: parent
            fillMode: Image.PreserveAspectFit
            asynchronous: true
            sourceSize.width: Math.ceil(Math.min(1024, stage.width) * Screen.devicePixelRatio)
            sourceSize.height: Math.ceil(Math.min(1024, stage.height) * Screen.devicePixelRatio)
            source: root.current && root.current.thumb ? "image://thumb/" + encodeURIComponent(root.current.path) + "?m=" + root.current.mtime : ""
        }
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onDoubleClicked: if (root.fm.currentRow >= 0) root.pane.openRow(root.fm.currentRow, false)
            onPressed: (m) => { root.pane.forceActiveFocus(); if (m.button === Qt.RightButton && root.fm.currentRow >= 0) root.pane.showItemMenu(root.fm.currentRow, m.x, m.y) }
        }
    }
    GLabel {
        anchors.bottom: strip.top
        anchors.bottomMargin: 2
        anchors.horizontalCenter: parent.horizontalCenter
        text: root.current ? root.current.name + "  ·  " + root.current.kind : ""
        color: Theme.fgSecondary
        width: Math.min(implicitWidth, parent.width - 40)
        horizontalAlignment: Text.AlignHCenter
    }

    ListView {
        id: strip
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 8
        height: root.stripH
        orientation: ListView.Horizontal
        model: root.fm
        spacing: 4
        clip: true
        reuseItems: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.horizontal: ScrollBar { policy: ScrollBar.AsNeeded }
        delegate: Item {
            id: t
            required property int index
            required property string path
            required property string name
            required property bool selected
            required property bool thumb
            required property string iconName
            required property real mtime
            required property bool isDir
            width: strip.height - 14
            height: strip.height - 14
            Rectangle {
                anchors.fill: parent
                radius: 8
                color: t.selected ? Theme.accent : "transparent"
                opacity: t.selected ? 0.9 : 1
            }
            GIcon { anchors.centerIn: parent; size: parent.width - 16; name: t.iconName; visible: th.status !== Image.Ready }
            Image {
                id: th
                anchors.fill: parent
                anchors.margins: 4
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                sourceSize.width: Math.ceil(width * Screen.devicePixelRatio)
                sourceSize.height: Math.ceil(height * Screen.devicePixelRatio)
                source: t.thumb ? "image://thumb/" + encodeURIComponent(t.path) + "?m=" + t.mtime : ""
            }
            ItemMouse { anchors.fill: parent; pane: root.pane; row: t.index; iconUrl: t.iconName ? "image://icon/" + t.iconName : "" }
            FolderDrop { anchors.fill: parent; pane: root.pane; path: t.path; enabledFor: t.isDir }
            GTip { visible: false; text: t.name }
        }
        WheelHandler {
            onWheel: (e) => {
                const d = e.angleDelta.y !== 0 ? e.angleDelta.y : e.angleDelta.x
                strip.contentX = Math.max(strip.originX, Math.min(strip.contentX - d, strip.originX + Math.max(0, strip.contentWidth - strip.width)))
            }
        }
    }
}
