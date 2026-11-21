import QtQuick

// Mouse handling shared by every view's items: click/Shift/Ctrl selection,
// double-click (or single-click, a setting) to open, middle-click opens in
// a new tab, right-click menu, and dragging the selection to other folders
// or other apps (text/uri-list).
MouseArea {
    id: ma
    property var pane
    property int row: -1
    property string iconUrl: ""
    acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
    hoverEnabled: true
    drag.target: proxy
    drag.threshold: 6
    property bool wasSelected: false
    signal leftPressed(int modifiers)

    onPressed: (m) => {
        if (m.button === Qt.RightButton) { pane.showItemMenu(row, m.x, m.y); return }
        if (m.button === Qt.MiddleButton) { pane.openRow(row, true); return }
        pane.pressItem(row, m.modifiers)
        proxy.x = m.x; proxy.y = m.y
        proxy.Drag.mimeData = { "text/uri-list": pane.dragUris() }
        proxy.Drag.hotSpot.x = 0; proxy.Drag.hotSpot.y = 0
        ma.leftPressed(m.modifiers)
    }
    onReleased: (m) => {
        if (m.button === Qt.LeftButton && !drag.active) pane.releaseItem(row, m.modifiers)
        proxy.x = 0; proxy.y = 0
    }
    onClicked: (m) => {
        if (m.button === Qt.LeftButton && pane.app.settings.single_click && !pane.selectionMode && !(m.modifiers & (Qt.ShiftModifier | Qt.ControlModifier)))
            pane.openRow(row, false)
    }
    onDoubleClicked: (m) => {
        if (m.button === Qt.LeftButton && !pane.app.settings.single_click && !pane.selectionMode) pane.openRow(row, (m.modifiers & Qt.ControlModifier) !== 0)
    }

    Item {
        id: proxy
        width: 1; height: 1
        Drag.active: ma.drag.active
        Drag.dragType: Drag.Automatic
        Drag.supportedActions: Qt.CopyAction | Qt.MoveAction | Qt.LinkAction
        Drag.proposedAction: Qt.MoveAction
        Drag.imageSource: ma.iconUrl
        Drag.onDragFinished: (action) => { proxy.x = 0; proxy.y = 0 }
    }
}
