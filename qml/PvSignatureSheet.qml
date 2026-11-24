import QtQuick
import QtQuick.Controls.Basic

// Grove Preview's signatures: sign with the mouse or trackpad on the pad,
// name it, keep it (in ~/.config/grove/signatures.json) and put it on any
// page or picture with Markup › Sign. Saved ones can be removed.
Sheet {
    id: ss
    property var win
    title: "Signatures"
    message: "Sign your name on the line with your mouse or trackpad, then click Done. It's kept for next time."
    width: Math.min(parent ? parent.width - 40 : 520, Math.round(520 * Theme.pt))
    property var strokes: []
    property var cur: null
    property color ink: "#000000"
    function open2() { strokes = []; cur = null; nameField.text = ""; pad.requestPaint(); open() }

    Rectangle {
        width: parent.width
        height: 180
        radius: 8
        color: "white"
        border.color: Theme.glassEdge
        Rectangle { x: 24; width: parent.width - 48; y: parent.height - 44; height: 1; color: "#b0b0b0" }
        GLabel { x: 24; y: parent.height - 40; text: "Sign above the line"; color: "#9a9a9a"; font.pixelSize: Theme.small; visible: ss.strokes.length === 0 }
        Canvas {
            id: pad
            anchors.fill: parent
            onPaint: {
                const g = getContext("2d")
                g.reset()
                g.strokeStyle = ss.ink
                g.lineWidth = 2.4
                g.lineCap = "round"; g.lineJoin = "round"
                const all = ss.cur ? ss.strokes.concat([ss.cur]) : ss.strokes
                for (const s of all) {
                    g.beginPath()
                    for (let i = 0; i + 1 < s.length; i += 2) { if (i === 0) g.moveTo(s[0], s[1]); else g.lineTo(s[i], s[i + 1]) }
                    if (s.length === 2) g.lineTo(s[0] + 0.5, s[1])
                    g.stroke()
                }
            }
        }
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.CrossCursor
            onPressed: (m) => { ss.cur = [m.x, m.y]; pad.requestPaint() }
            onPositionChanged: (m) => { if (ss.cur) { ss.cur = ss.cur.concat([m.x, m.y]); pad.requestPaint() } }
            onReleased: { if (ss.cur) ss.strokes = ss.strokes.concat([ss.cur]); ss.cur = null; pad.requestPaint() }
        }
    }
    Row {
        spacing: 8
        GField { id: nameField; width: 220; placeholderText: "Name (optional)"; tip: "A name to tell your signatures apart" }
        Repeater {
            model: ["#000000", "#1c3faa", "#7a1f1f"]
            Rectangle {
                required property string modelData
                width: 22; height: 22; radius: 11; color: modelData
                anchors.verticalCenter: parent.verticalCenter
                border.width: Qt.colorEqual(ss.ink, modelData) ? 2.5 : 1
                border.color: Qt.colorEqual(ss.ink, modelData) ? Theme.accent : Theme.glassEdge
                MouseArea { anchors.fill: parent; onClicked: { ss.ink = parent.modelData; pad.requestPaint() } }
            }
        }
        GButton { text: "Clear"; flat: false; capsule: true; tip: "Start again"; onClicked: { ss.strokes = []; pad.requestPaint() } }
    }
    // Saved signatures.
    Flow {
        width: parent.width
        spacing: 8
        visible: ss.win.signatures.length > 0
        Repeater {
            model: ss.win.signatures
            Rectangle {
                required property var modelData
                required property int index
                width: 150; height: 64; radius: 6
                color: "white"
                border.color: ss.win.signatureIndex === index ? Theme.accent : Theme.glassEdge
                border.width: ss.win.signatureIndex === index ? 2 : 1
                Canvas {
                    anchors.fill: parent; anchors.margins: 8
                    onPaint: {
                        const g = getContext("2d"), s = parent.modelData
                        g.reset(); g.strokeStyle = s.c || "black"; g.lineWidth = 1.5; g.lineCap = "round"
                        const k = Math.min(width / s.w, height / s.h)
                        for (const st of s.strokes) { g.beginPath(); for (let i = 0; i + 1 < st.length; i += 2) { const x = st[i] * s.w * k, y = st[i + 1] * s.h * k; if (i === 0) g.moveTo(x, y); else g.lineTo(x, y) } g.stroke() }
                    }
                }
                MouseArea { anchors.fill: parent; onClicked: ss.win.signatureIndex = parent.index }
                GButton { anchors.right: parent.right; anchors.top: parent.top; iconName: "edit-delete"; iconSize: 12; tip: "Remove this signature"; onClicked: ss.win.removeSignature(parent.index) }
            }
        }
    }
    buttons: [
        PushButton { text: "Close"; tip: "Close without adding a signature"; onClicked: ss.close() },
        PushButton {
            text: "Done"
            primary: true
            enabled: ss.strokes.length > 0
            tip: "Keep this signature and use it (click where it goes)"
            onClicked: {
                // Normalise to its own box (0..1).
                let x0 = 1e9, y0 = 1e9, x1 = -1e9, y1 = -1e9
                for (const s of ss.strokes) for (let i = 0; i + 1 < s.length; i += 2) { x0 = Math.min(x0, s[i]); y0 = Math.min(y0, s[i + 1]); x1 = Math.max(x1, s[i]); y1 = Math.max(y1, s[i + 1]) }
                const w = Math.max(1, x1 - x0), h = Math.max(1, y1 - y0)
                const st = ss.strokes.map(s => s.map((v, i) => Math.round((i % 2 ? (v - y0) / h : (v - x0) / w) * 1000) / 1000))
                ss.win.addSignature({ name: nameField.text, c: "" + ss.ink, w: w, h: h, strokes: st })
                ss.close()
                ss.win.setTool("sign")
            }
        }
    ]
}
