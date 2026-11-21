import QtQuick
import QtQuick.Controls.Basic
import Grove

// The terminal panel (F4, as in Dolphin): a shell in the current folder
// that follows you as you browse (while nothing is running in it).
Item {
    id: tp
    property var win
    property string folder: win.activePane && win.activePane.path.indexOf("/") === 0 ? win.activePane.path : ""
    property var screen: ({ rows: [] })
    readonly property real cellW: fm.averageCharacterWidth
    readonly property real cellH: Math.ceil(fm.height)
    readonly property int cols: Math.max(10, Math.floor((width - 16) / cellW))
    readonly property int rowsN: Math.max(3, Math.floor((height - 12) / cellH))

    FontMetrics { id: fm; font.family: Theme.mono; font.pixelSize: Theme.body * 0.95 }

    TerminalSession {
        id: session
        onChanged: { tp.screen = JSON.parse(session.screen()); if (tp.win.app.grove.env("GROVE_DEBUG")) console.log("term", JSON.stringify(tp.screen).substring(0, 300)) }
        onAliveChanged: if (!alive && tp.visible) restartTimer.start()
    }
    Timer { id: restartTimer; interval: 300; onTriggered: { session.stop(); tp.ensure() } }
    function ensure() {
        if (!session.alive && visible && width > 50 && height > 30)
            session.start(folder || tp.win.app.grove.home(), cols, rowsN, Math.round(cellW), Math.round(cellH), Theme.dark)
    }
    onVisibleChanged: if (visible) { ensure(); keys.forceActiveFocus() }
    Component.onCompleted: startTimer.start()
    Timer { id: startTimer; interval: 50; onTriggered: tp.ensure() }
    onColsChanged: if (session.alive) session.resize(cols, rowsN, Math.round(cellW), Math.round(cellH))
    onRowsNChanged: if (session.alive) session.resize(cols, rowsN, Math.round(cellW), Math.round(cellH))
    onFolderChanged: if (session.alive && folder) session.follow(folder)

    Rectangle { anchors.fill: parent; color: Theme.dark ? "#141416" : "#fbfbfb" }
    Rectangle { width: parent.width; height: 1; color: Theme.separator }

    Column {
        x: 8; y: 6
        Repeater {
            model: tp.screen.rows
            Text {
                required property string modelData
                height: tp.cellH
                text: modelData
                textFormat: Text.RichText
                font: fm.font
                renderType: Text.NativeRendering
                color: Theme.fg
                wrapMode: Text.NoWrap
            }
        }
    }
    Rectangle {
        visible: tp.screen.cursor === true
        x: 8 + tp.screen.cx * tp.cellW
        y: 6 + tp.screen.cy * tp.cellH
        width: keys.activeFocus ? tp.cellW : tp.cellW
        height: tp.cellH
        color: keys.activeFocus ? Qt.rgba(Theme.fg.r, Theme.fg.g, Theme.fg.b, 0.55) : "transparent"
        border.width: 1
        border.color: Theme.fg
    }

    TextInput { id: clip; visible: false }
    Item {
        id: keys
        anchors.fill: parent
        focus: true
        Keys.onPressed: (e) => {
            if ((e.modifiers & Qt.ControlModifier) && (e.modifiers & Qt.ShiftModifier) && e.key === Qt.Key_V) {
                clip.text = ""; clip.paste(); session.sendText(clip.text); e.accepted = true; return
            }
            if (e.key === Qt.Key_F4 || (e.key === Qt.Key_Escape && (e.modifiers & Qt.ShiftModifier))) return
            session.key(e.key, e.text, e.modifiers)
            e.accepted = true
        }
    }
    MouseArea {
        anchors.fill: parent
        onPressed: keys.forceActiveFocus()
        onWheel: (w) => session.scroll(Math.round(w.angleDelta.y / 40))
    }
    GButton {
        anchors.right: parent.right; anchors.rightMargin: 6
        anchors.top: parent.top; anchors.topMargin: 6
        iconName: "window-close"; iconSize: 10
        tip: "Close the terminal panel (F4)"
        onClicked: tp.win.setPanel("terminal_panel", false)
    }
}
