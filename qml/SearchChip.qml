import QtQuick
import QtQuick.Controls.Basic

// A search criterion in the search bar (Finder's Kind / Date / Tag rows,
// as a capsule): click to choose, × to clear.
Item {
    id: sc
    property string label: ""
    property string value: ""
    property var choices: []            // [{v, t, color?}]
    property string tip: ""
    signal picked(string v)
    signal aboutToOpen()
    readonly property bool set: value !== ""
    readonly property string shown: {
        for (const c of choices) if (c.v === value) return c.t
        return value
    }
    implicitWidth: row.implicitWidth + 20
    implicitHeight: Theme.controlHeight - 4
    anchors.verticalCenter: parent ? parent.verticalCenter : undefined
    Rectangle {
        anchors.fill: parent
        radius: height / 2
        color: sc.set ? Theme.accent : (ma.containsMouse ? Theme.hover : "transparent")
        border.width: sc.set ? 0 : 1
        border.color: Theme.separator
    }
    Row {
        id: row
        anchors.centerIn: parent
        spacing: 5
        GLabel {
            text: sc.set ? sc.label + ": " + sc.shown : sc.label
            color: sc.set ? Theme.accentText : Theme.fg
            font.pixelSize: Theme.small
            anchors.verticalCenter: parent.verticalCenter
        }
        GLabel {
            text: sc.set ? "×" : "⌄"
            color: sc.set ? Theme.accentText : Theme.fgSecondary
            font.pixelSize: Theme.small
            anchors.verticalCenter: parent.verticalCenter
        }
    }
    MouseArea {
        id: ma
        anchors.fill: parent
        hoverEnabled: true
        onClicked: (m) => {
            if (sc.set && m.x > width - 18) { sc.picked(""); return }
            sc.aboutToOpen()
            menu.popup(sc, 0, sc.height + 4)
        }
    }
    GTip { visible: ma.containsMouse && !menu.visible; text: sc.set ? sc.tip + ". Click × to clear." : sc.tip }
    GMenu {
        id: menu
        GMenuItem { text: "Any"; iconName: "edit-clear"; checkable: true; checked: !sc.set; onTriggered: sc.picked("") }
        GMenuSeparator {}
        Instantiator {
            model: sc.choices
            delegate: GMenuItem {
                required property var modelData
                text: modelData.t
                iconName: "view-filter|funnel"
                checkable: true
                checked: sc.value === modelData.v
                onTriggered: sc.picked(modelData.v)
            }
            onObjectAdded: (i, o) => menu.insertItem(i + 2, o)
            onObjectRemoved: (i, o) => menu.removeItem(o)
        }
    }
}
