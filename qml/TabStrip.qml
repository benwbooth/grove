import QtQuick
import QtQuick.Controls.Basic

// Tabs (shown when there's more than one): Finder-style equal-width tabs;
// middle-click or × closes, drop files on a tab to move them there.
Item {
    id: ts
    property var win
    property var model
    property int currentIndex: 0
    Rectangle { anchors.fill: parent; color: Theme.dark ? Qt.rgba(0, 0, 0, 0.18) : Qt.rgba(0, 0, 0, 0.04) }
    Row {
        anchors.fill: parent
        anchors.leftMargin: 6
        anchors.rightMargin: 40
        Repeater {
            model: ts.model
            Item {
                id: tab
                required property int index
                width: Math.min(260, (ts.width - 46) / Math.max(1, ts.model.count))
                height: ts.height
                readonly property bool current: index === ts.currentIndex
                Rectangle {
                    anchors.fill: parent
                    anchors.topMargin: 3; anchors.bottomMargin: 3; anchors.leftMargin: 2; anchors.rightMargin: 2
                    radius: height / 2
                    color: tab.current ? Theme.field : tm.containsMouse ? Theme.hover : "transparent"
                    border.width: tabDrop.containsDrag ? 2 : 0
                    border.color: Theme.accent
                }
                GLabel {
                    anchors.centerIn: parent
                    width: parent.width - 50
                    horizontalAlignment: Text.AlignHCenter
                    text: {
                        const it = tabsRepeaterItem(tab.index)
                        return it ? it.title : ""
                    }
                    font.pixelSize: Theme.small
                    color: tab.current ? Theme.fg : Theme.fgSecondary
                }
                MouseArea {
                    id: tm
                    anchors.fill: parent
                    hoverEnabled: true
                    acceptedButtons: Qt.LeftButton | Qt.MiddleButton
                    onClicked: (m) => { if (m.button === Qt.MiddleButton) ts.win.closeTab(tab.index); else ts.currentIndex = tab.index }
                }
                GButton {
                    visible: tm.containsMouse || tab.current
                    anchors.left: parent.left; anchors.leftMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    width: 18; height: 18
                    iconName: "window-close"; iconSize: 10
                    tip: "Close tab (Ctrl+W)"
                    onClicked: ts.win.closeTab(tab.index)
                }
                DropArea {
                    id: tabDrop
                    anchors.fill: parent
                    keys: ["text/uri-list"]
                    onEntered: switchTimer.restart()
                    onExited: switchTimer.stop()
                    onDropped: (d) => { const t = tabsRepeaterItem(tab.index); if (t) t.activePane.dropInto(t.activePane.path, d) }
                    Timer { id: switchTimer; interval: 700; onTriggered: ts.currentIndex = tab.index }
                }
            }
        }
    }
    function tabsRepeaterItem(i) { return ts.win.tabAt(i) }
    GButton {
        anchors.right: parent.right; anchors.rightMargin: 6
        anchors.verticalCenter: parent.verticalCenter
        iconName: "list-add"; iconSize: 12
        tip: "New tab (Ctrl+T)"
        onClicked: ts.win.newTab(ts.win.activePane.path.indexOf("/") === 0 ? ts.win.activePane.path : ts.win.app.grove.home(), [])
    }
}
