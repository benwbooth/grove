import QtQuick

// Minimize, maximize/restore and close at the right end of grove's toolbar,
// drawn like KDE Plasma's Breeze decoration: thin stroked icons (⌄ minimize,
// ⌃ maximize, ◇ restore, ✕ close) with a round highlight behind the one under
// the pointer, red with a white ✕ for close.
Row {
    id: wc
    required property var window
    readonly property bool maximized: window.visibility === Window.Maximized
    // The icons' colour (Grove Player: white over the picture).
    property color tint: Theme.fg
    spacing: 6

    // One 1.5 px stroke of an icon, centred at (cx, cy) on a 16×16 grid.
    component Bar: Rectangle {
        property real len: 5
        property real angle: 0
        property real cx: 8
        property real cy: 8
        property color ink
        width: len; height: 1.5; radius: 0.75
        color: ink
        antialiasing: true
        x: cx - width / 2; y: cy - height / 2
        rotation: angle
    }

    component WinButton: Item {
        id: b
        property string glyph
        property bool danger: false
        property string tip
        signal hit()
        width: 22
        height: 22
        anchors.verticalCenter: parent ? parent.verticalCenter : undefined
        readonly property bool hot: area.containsMouse
        readonly property bool down: area.pressed

        Rectangle {
            anchors.fill: parent
            radius: width / 2
            visible: b.hot || b.down
            color: b.danger ? (b.down ? "#bf1010" : "#ed1515")
                            : (b.down ? Theme.fgSecondary : Theme.fg)
        }
        // The icon on a 16×16 grid: 1.5 px strokes built from rounded bars
        // (QtQuick.Shapes doesn't draw on every GPU backend).
        Item {
            id: icon
            anchors.centerIn: parent
            width: 16; height: 16
            opacity: wc.window.active || b.hot ? 1 : 0.55
            readonly property color ink: (b.hot || b.down) ? (b.danger ? "white" : Theme.window) : wc.tint
            // ⌄ minimize / ⌃ maximize: two bars meeting in the middle.
            Bar { ink: icon.ink; visible: b.glyph === "minimize"; len: 5.4; angle: 45;  cx: 6.25; cy: 8.25 }
            Bar { ink: icon.ink; visible: b.glyph === "minimize"; len: 5.4; angle: -45; cx: 9.75; cy: 8.25 }
            Bar { ink: icon.ink; visible: b.glyph === "maximize"; len: 5.4; angle: -45; cx: 6.25; cy: 8.25 }
            Bar { ink: icon.ink; visible: b.glyph === "maximize"; len: 5.4; angle: 45;  cx: 9.75; cy: 8.25 }
            // ✕ close.
            Bar { ink: icon.ink; visible: b.glyph === "close"; len: 9.2; angle: 45 }
            Bar { ink: icon.ink; visible: b.glyph === "close"; len: 9.2; angle: -45 }
            // ◇ restore.
            Rectangle {
                visible: b.glyph === "restore"
                anchors.centerIn: parent
                width: 6.5; height: 6.5
                rotation: 45
                antialiasing: true
                color: "transparent"
                border.width: 1.5
                border.color: icon.ink
            }
        }
        MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; onClicked: b.hit() }
        GTip { visible: b.hot && b.tip !== ""; text: b.tip }
    }

    WinButton { glyph: "minimize"; tip: "Minimize"; onHit: wc.window.showMinimized() }
    WinButton {
        glyph: wc.maximized ? "restore" : "maximize"
        tip: wc.maximized ? "Restore" : "Maximize"
        onHit: wc.maximized ? wc.window.showNormal() : wc.window.showMaximized()
    }
    WinButton { glyph: "close"; danger: true; tip: "Close this window (Ctrl+W closes a tab)"; onHit: wc.window.close() }
}
