import QtQuick
import QtQuick.Window

// Grove Preview's Slideshow: the pages (or the open pictures) one at a
// time on black, full screen. ←/→, Space, Page Up/Down to move, Esc to stop.
Window {
    id: ss
    property var win
    property int index: 0
    // Every page of a PDF, or every open picture.
    readonly property var items: {
        if (!win.cur) return []
        if (win.cur.kind === "pdf") return win.cur.pages.map(p => ({ doc: win.cur, pg: p }))
        return win.docs.filter(d => d.kind === "image" && d.img).map(d => ({ doc: d, pg: null }))
    }
    color: "black"
    flags: Qt.Window | Qt.FramelessWindowHint
    title: "Slideshow"
    function start() {
        if (!win.cur) return
        index = win.cur.kind === "pdf" ? win.currentPage : Math.max(0, items.findIndex(i => i.doc.id === win.cur.id))
        showFullScreen(); raise(); requestActivate()
    }
    function step(d) { index = Math.max(0, Math.min(items.length - 1, index + d)); hint.opacity = 1; hintTimer.restart() }

    Item {
        anchors.fill: parent
        focus: true
        transform: Translate { x: 0.001; y: 0.001 }
        Keys.onPressed: (e) => {
            if (e.key === Qt.Key_Right || e.key === Qt.Key_Down || e.key === Qt.Key_Space || e.key === Qt.Key_PageDown) ss.step(1)
            else if (e.key === Qt.Key_Left || e.key === Qt.Key_Up || e.key === Qt.Key_PageUp || e.key === Qt.Key_Backspace) ss.step(-1)
            else if (e.key === Qt.Key_Home) ss.index = 0
            else if (e.key === Qt.Key_End) ss.index = ss.items.length - 1
            else if (e.key === Qt.Key_Escape) ss.close()
            else return
            e.accepted = true
        }
        PvThumb {
            anchors.centerIn: parent
            visible: ss.visible && ss.items.length > 0
            win: ss.win
            readonly property var it: ss.items[ss.index] || null
            doc: it ? it.doc : null
            pg: it ? it.pg : null
            // The longer side, so the page fills the screen either way round.
            readonly property real aspect: !it ? 1 : it.pg ? (it.pg.rot % 180 ? it.pg.h / it.pg.w : it.pg.w / it.pg.h) : (it.doc.img ? (it.doc.img.rotate % 180 ? it.doc.img.h / it.doc.img.w : it.doc.img.w / it.doc.img.h) : 1)
            box: aspect >= 1 ? Math.min(ss.width, ss.height * aspect) : Math.min(ss.height, ss.width / aspect)
        }
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onClicked: (m) => ss.step(m.button === Qt.RightButton ? -1 : 1)
            onDoubleClicked: ss.close()
        }
        GLabel {
            id: hint
            anchors.bottom: parent.bottom; anchors.bottomMargin: 24
            anchors.horizontalCenter: parent.horizontalCenter
            text: (ss.index + 1) + " of " + ss.items.length + "   ·   ← → to move, Esc to stop"
            color: "white"
            opacity: 1
            Behavior on opacity { NumberAnimation { duration: 400 } }
            Timer { id: hintTimer; interval: 2500; running: ss.visible; onTriggered: hint.opacity = 0 }
        }
    }
}
