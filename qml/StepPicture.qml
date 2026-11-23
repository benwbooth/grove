import QtQuick

// A drawn picture of a web page for a setup step: a browser window with
// the page's address, its heading, and the fields, choices and buttons on
// it; the one to use is highlighted (pulsing ring). Drawn, not a
// screenshot, so it never goes stale or shows anyone's account.
//   rows: [{ kind: "field"|"choice"|"text"|"button"|"link", label, value, hot }]
Rectangle {
    id: sp
    property string address: ""
    property string heading: ""
    property var rows: []
    width: parent ? parent.width : 420
    height: page.implicitHeight + bar.height + 26
    radius: 10
    color: Theme.dark ? "#2b2d31" : "#f2f2f4"
    border.color: Theme.separator
    clip: true

    // The browser's bar: dots and the address.
    Rectangle {
        id: bar
        width: parent.width
        height: 30
        color: Theme.dark ? "#202124" : "#e4e4e8"
        Row {
            x: 10
            anchors.verticalCenter: parent.verticalCenter
            spacing: 5
            Repeater { model: ["#ff5f57", "#febc2e", "#28c840"]; Rectangle { required property string modelData; width: 9; height: 9; radius: 4.5; color: modelData } }
        }
        Rectangle {
            x: 56
            width: parent.width - 66
            height: 20
            radius: 10
            anchors.verticalCenter: parent.verticalCenter
            color: Theme.dark ? "#35363a" : "#ffffff"
            GLabel {
                anchors.verticalCenter: parent.verticalCenter
                x: 10
                width: parent.width - 20
                text: "🔒 " + sp.address
                elide: Text.ElideRight
                font.pixelSize: Theme.small - 1
                color: Theme.fgSecondary
            }
        }
    }
    // The page.
    Column {
        id: page
        anchors.top: bar.bottom
        anchors.topMargin: 10
        x: 16
        width: parent.width - 32
        spacing: 8
        GLabel { text: sp.heading; font.pixelSize: Theme.body + 2; font.bold: true; visible: sp.heading !== ""; color: Theme.dark ? "#e8eaed" : "#202124" }
        Repeater {
            model: sp.rows
            Item {
                id: row
                required property var modelData
                width: page.width
                height: Math.max(24, inner.implicitHeight)
                readonly property bool hot: !!modelData.hot
                Loader {
                    id: inner
                    width: parent.width
                    sourceComponent: row.modelData.kind === "field" ? fieldC : row.modelData.kind === "choice" ? choiceC : row.modelData.kind === "button" ? buttonC : row.modelData.kind === "link" ? linkC : textC
                    property var d: row.modelData
                }
                // The thing to use: a pulsing ring.
                Rectangle {
                    visible: row.hot
                    x: -4; y: -4
                    // A button's ring hugs the button; others take the row.
                    width: (row.modelData.kind === "button" && inner.item ? inner.item.implicitWidth : parent.width) + 8
                    height: parent.height + 8
                    radius: 8
                    color: "transparent"
                    border.width: 2.5
                    border.color: Theme.accent
                    SequentialAnimation on opacity {
                        running: row.hot && sp.visible
                        loops: Animation.Infinite
                        NumberAnimation { from: 1; to: 0.25; duration: 700 }
                        NumberAnimation { from: 0.25; to: 1; duration: 700 }
                    }
                }
            }
        }
    }
    Component {
        id: fieldC
        Column {
            spacing: 3
            GLabel { text: parent.parent.d.label; font.pixelSize: Theme.small - 1; color: Theme.fgSecondary }
            Rectangle {
                width: parent.parent.width
                height: 24
                radius: 4
                color: "transparent"
                border.color: Theme.dark ? "#5f6368" : "#bdc1c6"
                GLabel { x: 8; anchors.verticalCenter: parent.verticalCenter; text: parent.parent.parent.d.value || ""; font.pixelSize: Theme.small; color: Theme.dark ? "#e8eaed" : "#202124"; width: parent.width - 16; elide: Text.ElideRight }
            }
        }
    }
    Component {
        id: choiceC
        Row {
            spacing: 8
            Rectangle {
                width: 14; height: 14; radius: 7
                anchors.verticalCenter: parent.verticalCenter
                color: "transparent"
                border.color: parent.parent.d.value ? "#1a73e8" : Theme.fgTertiary
                border.width: 2
                Rectangle { anchors.centerIn: parent; width: 6; height: 6; radius: 3; color: "#1a73e8"; visible: !!parent.parent.parent.d.value }
            }
            GLabel { text: parent.parent.d.label; font.pixelSize: Theme.small; anchors.verticalCenter: parent.verticalCenter; color: Theme.dark ? "#e8eaed" : "#202124" }
        }
    }
    Component {
        id: buttonC
        Item {
            height: 26
            implicitWidth: btnText.implicitWidth + 24
            Rectangle {
                width: btnText.implicitWidth + 24
                height: 26
                radius: 4
                color: "#1a73e8"
                GLabel { id: btnText; anchors.centerIn: parent; text: parent.parent.parent.d.label; color: "white"; font.pixelSize: Theme.small; font.bold: true }
            }
        }
    }
    Component {
        id: linkC
        GLabel { text: parent.d.label; color: "#1a73e8"; font.pixelSize: Theme.small; font.underline: true }
    }
    Component {
        id: textC
        GLabel { text: parent.d.label; wrapMode: Text.Wrap; width: parent.width; font.pixelSize: Theme.small; color: Theme.dark ? "#bdc1c6" : "#5f6368" }
    }
}
