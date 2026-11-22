import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window

// Shows text output: git log/diff, file comparisons. Highlighted when the
// caller passes HTML.
Window {
    id: tv
    property string heading: ""
    property string text: ""
    property string html: ""
    property string tool: ""
    property var toolAction: null
    width: 860
    height: 600
    color: Theme.window
    title: heading
    Item {
        anchors.fill: parent
        transform: Translate { x: 0.001; y: 0.001 }
        Row {
            id: head
            x: 16; y: 12
            spacing: 10
            GLabel { text: tv.heading; font.bold: true; font.pixelSize: Theme.title; anchors.verticalCenter: parent.verticalCenter }
            PushButton { visible: tv.tool !== ""; text: "Open in " + tv.tool; onClicked: if (tv.toolAction) tv.toolAction() }
            PushButton { text: "Copy"; tip: "Copy all of the text"; onClicked: { area.selectAll(); area.copy(); area.deselect() } }
        }
        KineticWheel { flickable: gsv3.contentItem; visible: gsv3.visible }
        ScrollView {
            id: gsv3
            ScrollBar.vertical: GScrollBar { parent: gsv3; x: gsv3.width - width; y: gsv3.topPadding; height: gsv3.availableHeight }
            ScrollBar.horizontal: GScrollBar { parent: gsv3; x: gsv3.leftPadding; y: gsv3.height - height; width: gsv3.availableWidth }
            anchors.top: head.bottom
            anchors.topMargin: 10
            anchors.bottom: parent.bottom
            width: parent.width
            TextEdit {
                id: area
                readOnly: true
                selectByMouse: true
                textFormat: tv.html !== "" ? TextEdit.RichText : TextEdit.PlainText
                text: tv.html !== "" ? tv.html : tv.text
                font.family: Theme.mono
                font.pixelSize: Theme.body * 0.95
                renderType: Text.NativeRendering
                color: Theme.fg
                selectionColor: Theme.accent
                padding: 14
            }
        }
    }
}
