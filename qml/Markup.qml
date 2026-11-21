import QtQuick
import QtQuick.Controls.Basic

// Quick Look's Markup (Finder): draw on a picture with a pen or
// highlighter, add rectangles, ellipses, lines, arrows and text, crop and
// rotate, then save over the original (it goes to the Trash, Undo brings
// it back) or as a copy. Strokes are kept in picture pixels and drawn at
// full resolution when saving, off the UI thread.
Item {
    id: mk
    property string src: ""
    property string tool: "pen"           // pen, highlight, rect, ellipse, line, arrow, text, crop
    property color color: "#ff3b30"
    property int size: 1                  // 0 small, 1 medium, 2 large
    property var ops: []
    property var crop: null               // [x, y, w, h] in picture pixels
    property int turn: 0                  // 0, 90, 180, 270
    property var undoStack: []
    property var cur: null                // the stroke being drawn
    readonly property bool dirty: undoStack.length > 0
    readonly property real maxDim: Math.max(img.implicitWidth, img.implicitHeight, 1)
    readonly property var swatches: ["#ff3b30", "#ff9500", "#ffcc00", "#34c759", "#007aff", "#af52de", "#000000", "#ffffff"]

    function reset() { ops = []; crop = null; turn = 0; undoStack = []; cur = null; textBox.visible = false; cv.requestPaint() }
    function spec() { return { ops: ops, crop: crop, rotate: turn } }
    function strokeW() { return Math.max(2, Math.round(maxDim * [0.003, 0.006, 0.012][size])) }
    function push(kind, prev) { undoStack = undoStack.concat([{ kind: kind, prev: prev }]) }
    function addOp(o) { ops = ops.concat([o]); push("op", null); cv.requestPaint() }
    function undo() {
        if (!undoStack.length) return
        const u = undoStack[undoStack.length - 1]
        undoStack = undoStack.slice(0, -1)
        if (u.kind === "op") ops = ops.slice(0, -1)
        else if (u.kind === "crop") crop = u.prev
        else if (u.kind === "rotate") turn = u.prev
        cv.requestPaint()
    }
    function rotate(d) { push("rotate", turn); turn = (turn + d + 360) % 360 }

    // The picture and its drawing, turned as a whole.
    Item {
        id: stage
        anchors.centerIn: parent
        readonly property bool sideways: mk.turn % 180 !== 0
        readonly property real iw: Math.max(1, img.implicitWidth)
        readonly property real ih: Math.max(1, img.implicitHeight)
        readonly property real k: Math.min(mk.width / (sideways ? ih : iw), mk.height / (sideways ? iw : ih))
        width: iw * k
        height: ih * k
        rotation: mk.turn
        Behavior on rotation { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
        Image {
            id: img
            anchors.fill: parent
            source: mk.src
            autoTransform: true
            asynchronous: true
            cache: false
            smooth: true
            mipmap: true
        }
        Canvas {
            id: cv
            anchors.fill: parent
            renderStrategy: Canvas.Cooperative
            onPaint: {
                const g = getContext("2d")
                g.reset()
                g.clearRect(0, 0, width, height)
                const k = stage.k
                const all = mk.cur ? mk.ops.concat([mk.cur]) : mk.ops
                for (const o of all) {
                    g.globalAlpha = o.a !== undefined ? o.a : 1
                    g.strokeStyle = o.c
                    g.fillStyle = o.c
                    g.lineWidth = o.w * k
                    g.lineCap = "round"
                    g.lineJoin = "round"
                    g.beginPath()
                    if (o.t === "pen") {
                        for (let i = 0; i + 1 < o.p.length; i += 2) {
                            if (i === 0) g.moveTo(o.p[0] * k, o.p[1] * k)
                            else g.lineTo(o.p[i] * k, o.p[i + 1] * k)
                        }
                        if (o.p.length === 2) g.lineTo(o.p[0] * k + 0.01, o.p[1] * k)
                        g.stroke()
                    } else if (o.t === "rect") {
                        g.rect(Math.min(o.x1, o.x2) * k, Math.min(o.y1, o.y2) * k, Math.abs(o.x2 - o.x1) * k, Math.abs(o.y2 - o.y1) * k)
                        g.stroke()
                    } else if (o.t === "ellipse") {
                        g.ellipse(Math.min(o.x1, o.x2) * k, Math.min(o.y1, o.y2) * k, Math.abs(o.x2 - o.x1) * k, Math.abs(o.y2 - o.y1) * k)
                        g.stroke()
                    } else if (o.t === "line" || o.t === "arrow") {
                        g.moveTo(o.x1 * k, o.y1 * k)
                        g.lineTo(o.x2 * k, o.y2 * k)
                        if (o.t === "arrow") {
                            const ang = Math.atan2(o.y1 - o.y2, o.x1 - o.x2)
                            const len = Math.max(12, o.w * 4) * k
                            for (const d of [0.49, -0.49]) {
                                g.moveTo(o.x2 * k, o.y2 * k)
                                g.lineTo(o.x2 * k + Math.cos(ang + d) * len, o.y2 * k + Math.sin(ang + d) * len)
                            }
                        }
                        g.stroke()
                    } else if (o.t === "text") {
                        g.font = "bold " + Math.max(6, Math.round(o.s * k)) + "px sans-serif"
                        g.fillText(o.text, o.x1 * k, o.y1 * k)
                    }
                }
                g.globalAlpha = 1
            }
        }
        // Outside the crop, dimmed.
        Item {
            anchors.fill: parent
            visible: mk.crop !== null
            readonly property real cx: mk.crop ? mk.crop[0] * stage.k : 0
            readonly property real cy: mk.crop ? mk.crop[1] * stage.k : 0
            readonly property real cw: mk.crop ? mk.crop[2] * stage.k : 0
            readonly property real ch: mk.crop ? mk.crop[3] * stage.k : 0
            Rectangle { x: 0; y: 0; width: parent.width; height: parent.cy; color: Qt.rgba(0, 0, 0, 0.55) }
            Rectangle { x: 0; y: parent.cy + parent.ch; width: parent.width; height: parent.height - y; color: Qt.rgba(0, 0, 0, 0.55) }
            Rectangle { x: 0; y: parent.cy; width: parent.cx; height: parent.ch; color: Qt.rgba(0, 0, 0, 0.55) }
            Rectangle { x: parent.cx + parent.cw; y: parent.cy; width: parent.width - x; height: parent.ch; color: Qt.rgba(0, 0, 0, 0.55) }
            Rectangle { x: parent.cx; y: parent.cy; width: parent.cw; height: parent.ch; color: "transparent"; border.color: "white"; border.width: 1.5 }
        }
        MouseArea {
            anchors.fill: parent
            cursorShape: mk.tool === "text" ? Qt.IBeamCursor : Qt.CrossCursor
            property real sx: 0
            property real sy: 0
            function px(v) { return v / stage.k }
            onPressed: (m) => {
                sx = px(m.x); sy = px(m.y)
                const w = mk.strokeW()
                if (mk.tool === "text") {
                    textBox.at = [sx, sy]
                    textBox.x = m.x; textBox.y = m.y - textBox.height
                    textBox.text = ""
                    textBox.visible = true
                    textBox.forceActiveFocus()
                    return
                }
                if (mk.tool === "pen") mk.cur = { t: "pen", c: "" + mk.color, w: w, a: 1, p: [sx, sy] }
                else if (mk.tool === "highlight") mk.cur = { t: "pen", c: "" + mk.color, w: w * 4, a: 0.35, p: [sx, sy] }
                else if (mk.tool !== "crop") mk.cur = { t: mk.tool, c: "" + mk.color, w: w, x1: sx, y1: sy, x2: sx, y2: sy }
                else mk.cur = { t: "crop", x1: sx, y1: sy, x2: sx, y2: sy }
                cv.requestPaint()
            }
            onPositionChanged: (m) => {
                if (!mk.cur) return
                const x = Math.max(0, Math.min(stage.iw, px(m.x))), y = Math.max(0, Math.min(stage.ih, px(m.y)))
                const c = Object.assign({}, mk.cur)
                if (c.t === "pen") c.p = c.p.concat([x, y])
                else { c.x2 = x; c.y2 = y }
                mk.cur = c
                if (c.t === "crop") mk.crop = [Math.min(c.x1, x), Math.min(c.y1, y), Math.abs(x - c.x1), Math.abs(y - c.y1)]
                else cv.requestPaint()
            }
            onReleased: {
                const c = mk.cur
                mk.cur = null
                if (!c) return
                if (c.t === "crop") {
                    const ok = Math.abs(c.x2 - c.x1) > 8 && Math.abs(c.y2 - c.y1) > 8
                    mk.crop = ok ? [Math.min(c.x1, c.x2), Math.min(c.y1, c.y2), Math.abs(c.x2 - c.x1), Math.abs(c.y2 - c.y1)] : null
                    mk.push("crop", cropBefore)
                    return
                }
                if (c.t !== "pen" && Math.abs(c.x2 - c.x1) < 2 && Math.abs(c.y2 - c.y1) < 2) { cv.requestPaint(); return }
                mk.addOp(c)
            }
            property var cropBefore: null
            onPressedChanged: if (pressed && mk.tool === "crop") cropBefore = mk.crop
        }
        GField {
            id: textBox
            property var at: [0, 0]
            visible: false
            width: 220
            placeholderText: "Type, then Return"
            tip: "The text to put on the picture (Return adds it, Esc cancels)"
            Keys.onReturnPressed: {
                if (text !== "") mk.addOp({ t: "text", c: "" + mk.color, s: Math.round(mk.maxDim * [0.022, 0.032, 0.05][mk.size]), x1: at[0], y1: at[1], text: text })
                visible = false
            }
            Keys.onEscapePressed: visible = false
        }
    }
}
