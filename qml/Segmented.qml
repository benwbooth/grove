import QtQuick

// A glass capsule of icon buttons, one of them chosen (the view switcher).
Item {
    id: s
    property var items: []          // [{icon, tip, value}]
    property string current: ""
    signal picked(string value)
    implicitHeight: Theme.controlHeight
    implicitWidth: row.implicitWidth + 4
    GlassPanel { anchors.fill: parent; radius: height / 2; shadow: false; tint: Theme.field }
    Row {
        id: row
        anchors.centerIn: parent
        spacing: 0
        Repeater {
            model: s.items
            GButton {
                required property var modelData
                width: Math.round(34 * Theme.pt)
                height: s.height - 4
                capsule: true
                iconName: modelData.icon
                tip: modelData.tip
                checked: s.current === modelData.value
                onClicked: s.picked(modelData.value)
            }
        }
    }
}
