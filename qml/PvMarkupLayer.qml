import QtQuick
import QtQuick.Controls.Basic

// Grove Preview's Markup over one PDF page or picture: draws its ops
// (units: points on a page, pixels on a picture; `k` screen px per unit),
// draws new ones with the window's current tool and style, and selects,
// moves, resizes and deletes them. Tools it doesn't draw with itself (text
// and rectangular selection, Instant Alpha, text highlights) come out as
// pressAt / dragTo / releasedAt for the page or picture to handle.
Item {
    id: ml
    property var ops: []
    property real k: 1
    property var win
    property string key: ""          // which page/picture (for the selection)
    property bool pdf: true
    property real unit: 1            // widths and text sizes are multiplied by this
    readonly property string tool: win ? win.tool : "textsel"
    readonly property int selected: win && win.sel && win.sel.key === key ? win.sel.i : -1
    property var cur: null
    property int editing: -1
    signal edited(var newOps, string label)
    signal pressAt(real x, real y, int clicks, int mods)
    signal dragTo(real x, real y)
    signal releasedAt(real x, real y)

    readonly property var drawTools: ["pen", "marker", "rect", "rrect", "ellipse", "line", "arrow", "star", "poly", "bubble", "loupe", "text", "note", "sign"]
    onOpsChanged: cv.requestPaint()
    onKChanged: cv.requestPaint()
    onSelectedChanged: cv.requestPaint()
    onCurChanged: cv.requestPaint()

    function px(v) { return v / k }
    function setOps(n, label) { edited(n, label) }
    function replaceOp(i, o, label) { const n = ops.slice(); n[i] = o; setOps(n, label) }
    function removeSelected() {
        if (selected < 0 || selected >= ops.length) return false
        const n = ops.slice(); n.splice(selected, 1)
        win.select("", -1)
        setOps(n, "Delete")
        return true
    }
    function styled(t) {
        const w = win.mkWidth * unit
        const o = { t: t, c: "" + win.mkColor, w: w }
        if (win.mkFill !== "" && ["rect", "rrect", "ellipse", "star", "poly", "bubble", "text"].indexOf(t) >= 0) o.fill = "" + win.mkFill
        return o
    }
    function hitTest(x, y) {
        const tol = 4 / k
        for (let i = ops.length - 1; i >= 0; i--) if (PvShapes.hit(ops[i], x, y, tol)) return i
        return -1
    }
    function handleHit(x, y) {
        if (selected < 0 || selected >= ops.length) return false
        const b = PvShapes.bounds(ops[selected])
        return Math.abs(x - b[2]) < 7 / k && Math.abs(y - b[3]) < 7 / k
    }
    // Text: edit in place.
    function editText(i) {
        editing = i
        const o = ops[i]
        editor.text = o.text || ""
        Qt.callLater(() => { editor.forceActiveFocus(); editor.cursorPosition = editor.text.length })
    }
    function finishEdit() {
        if (editing < 0) return
        const i = editing
        editing = -1
        if (i >= ops.length) return
        const o = Object.assign({}, ops[i], { text: editor.text })
        // The box grows with the text.
        const need = (editor.contentHeight + 4) / k
        if (need > Math.abs(o.y2 - o.y1)) o.y2 = Math.min(o.y1, o.y2) + need
        if (editor.text === "" && o.t === "text") { const n = ops.slice(); n.splice(i, 1); setOps(n, "Delete Text"); return }
        if (o.text !== ops[i].text || o.y2 !== ops[i].y2) replaceOp(i, o, "Typing")
    }

    // ---------- drawing ----------
    Canvas {
        id: cv
        anchors.fill: parent
        renderStrategy: Canvas.Cooperative
        function quadBox(q) {
            const xs = [q[0], q[2], q[4], q[6]], ys = [q[1], q[3], q[5], q[7]]
            return [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)]
        }
        function wrap(g, text, maxw) {
            const lines = []
            for (const para of String(text).split("\n")) {
                let line = ""
                for (const word of para.split(" ")) {
                    const t = line ? line + " " + word : word
                    if (g.measureText(t).width > maxw && line) { lines.push(line); line = word } else line = t
                }
                lines.push(line)
            }
            return lines
        }
        onPaint: {
            const g = getContext("2d")
            g.reset()
            g.clearRect(0, 0, width, height)
            const k = ml.k
            const all = ml.cur ? ml.ops.concat([ml.cur]) : ml.ops
            for (let n = 0; n < all.length; n++) {
                const o = all[n]
                if (n === ml.editing) continue
                g.save()
                g.globalAlpha = o.a !== undefined ? o.a : 1
                g.strokeStyle = o.c || "#ff3b30"
                g.fillStyle = o.fill || "transparent"
                g.lineWidth = Math.max(0.5, (o.w || 2) * k)
                g.lineCap = "round"
                g.lineJoin = "round"
                const x1 = (o.x1 || 0) * k, y1 = (o.y1 || 0) * k, x2 = (o.x2 || 0) * k, y2 = (o.y2 || 0) * k
                const bx = Math.min(x1, x2), by = Math.min(y1, y2), bw = Math.abs(x2 - x1), bh = Math.abs(y2 - y1)
                g.beginPath()
                if (o.t === "pen") {
                    for (let i = 0; i + 1 < o.p.length; i += 2) {
                        if (i === 0) g.moveTo(o.p[0] * k, o.p[1] * k); else g.lineTo(o.p[i] * k, o.p[i + 1] * k)
                    }
                    if (o.p.length === 2) g.lineTo(o.p[0] * k + 0.01, o.p[1] * k)
                    g.stroke()
                } else if (o.t === "sign") {
                    for (const s of o.strokes) for (let i = 0; i + 1 < s.length; i += 2) {
                        const sx = bx + s[i] * bw, sy = by + s[i + 1] * bh
                        if (i === 0) g.moveTo(sx, sy); else g.lineTo(sx, sy)
                    }
                    g.stroke()
                } else if (o.t === "rect") {
                    g.rect(bx, by, bw, bh)
                    if (o.fill) g.fill()
                    g.stroke()
                } else if (o.t === "rrect") {
                    g.roundedRect(bx, by, bw, bh, Math.min(bw, bh) * 0.18, Math.min(bw, bh) * 0.18)
                    if (o.fill) g.fill()
                    g.stroke()
                } else if (o.t === "ellipse" || o.t === "loupe") {
                    g.ellipse(bx, by, bw, bh)
                    if (o.fill && o.t === "ellipse") g.fill()
                    g.stroke()
                } else if (o.t === "star" || o.t === "poly" || o.t === "bubble") {
                    const p = o.p || PvShapes.outline(o)
                    for (let i = 0; i + 1 < p.length; i += 2) { if (i === 0) g.moveTo(p[0] * k, p[1] * k); else g.lineTo(p[i] * k, p[i + 1] * k) }
                    g.closePath()
                    if (o.fill) g.fill()
                    g.stroke()
                } else if (o.t === "line" || o.t === "arrow") {
                    g.moveTo(x1, y1); g.lineTo(x2, y2)
                    if (o.t === "arrow") {
                        const ang = Math.atan2(y1 - y2, x1 - x2), len = Math.max(12, (o.w || 2) * 4) * k
                        for (const d of [0.49, -0.49]) { g.moveTo(x2, y2); g.lineTo(x2 + Math.cos(ang + d) * len, y2 + Math.sin(ang + d) * len) }
                    }
                    g.stroke()
                } else if (o.t === "text") {
                    // Drawn by the Text items below.
                } else if (o.t === "note") {
                    // A sticky note.
                    const s = 20 * k
                    g.fillStyle = o.c || "#ffcc00"
                    g.strokeStyle = Qt.darker(o.c || "#ffcc00", 1.6)
                    g.lineWidth = 1
                    g.rect(x1, y1, s, s)
                    g.fill(); g.stroke()
                    g.beginPath()
                    for (let i = 1; i <= 3; i++) { g.moveTo(x1 + s * 0.2, y1 + s * i / 4.2); g.lineTo(x1 + s * 0.8, y1 + s * i / 4.2) }
                    g.stroke()
                } else if (o.t === "hl" || o.t === "ul" || o.t === "so" || o.t === "sq") {
                    for (const q of o.quads) {
                        const b = quadBox(q).map(v => v * k)
                        if (o.t === "hl") { g.globalAlpha = 0.38; g.fillStyle = o.c; g.fillRect(b[0], b[1], b[2] - b[0], b[3] - b[1]) }
                        else {
                            g.lineWidth = Math.max(1, (b[3] - b[1]) * 0.07)
                            const yy = o.t === "so" ? (b[1] + b[3]) / 2 : b[3] - g.lineWidth
                            g.beginPath()
                            if (o.t === "sq") { for (let x = b[0], i = 0; x <= b[2]; x += 3 * k, i++) { if (!i) g.moveTo(x, yy); else g.lineTo(x, yy + (i % 2 ? -1.5 : 1.5) * k) } }
                            else { g.moveTo(b[0], yy); g.lineTo(b[2], yy) }
                            g.stroke()
                        }
                    }
                }
                g.restore()
            }
            // The selected op: a dashed frame and its resize handle.
            if (ml.selected >= 0 && ml.selected < ml.ops.length && ml.editing < 0) {
                const b = PvShapes.bounds(ml.ops[ml.selected]).map(v => v * k)
                g.save()
                g.strokeStyle = Theme.accent
                g.lineWidth = 1
                g.setLineDash([4, 3])
                g.strokeRect(b[0] - 3, b[1] - 3, b[2] - b[0] + 6, b[3] - b[1] + 6)
                g.setLineDash([])
                g.fillStyle = "white"
                g.fillRect(b[2] - 4, b[3] - 4, 8, 8)
                g.strokeRect(b[2] - 4, b[3] - 4, 8, 8)
                g.restore()
            }
        }
    }

    // Text boxes (real text: wrapped, native rendering).
    Repeater {
        model: ml.liveOps || ml.ops
        Rectangle {
            required property var modelData
            required property int index
            readonly property var o: modelData
            visible: o.t === "text" && index !== ml.editing
            x: visible ? Math.min(o.x1, o.x2) * ml.k : 0
            y: visible ? Math.min(o.y1, o.y2) * ml.k : 0
            width: visible ? Math.abs(o.x2 - o.x1) * ml.k : 0
            height: visible ? Math.abs(o.y2 - o.y1) * ml.k : 0
            color: visible && o.fill ? o.fill : "transparent"
            Text {
                visible: parent.visible
                anchors.fill: parent
                anchors.leftMargin: 2 * ml.k
                anchors.rightMargin: 2 * ml.k
                text: parent.o.text || ""
                color: parent.o.c || "black"
                wrapMode: Text.Wrap
                font.family: parent.o.font || Theme.family
                font.pixelSize: Math.max(4, (parent.o.s || 14) * ml.k)
                font.bold: !!parent.o.b
                font.italic: !!parent.o.i
                horizontalAlignment: parent.o.al === "center" ? Text.AlignHCenter : parent.o.al === "right" ? Text.AlignRight : Text.AlignLeft
            }
        }
    }

    // ---------- input ----------
    MouseArea {
        id: ma
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton
        hoverEnabled: true
        cursorShape: ml.tool === "textsel" ? (ml.hoverOp >= 0 ? Qt.SizeAllCursor : Qt.IBeamCursor)
                   : ml.tool === "text" ? Qt.IBeamCursor
                   : ml.tool === "hl" || ml.tool === "ul" || ml.tool === "so" ? Qt.IBeamCursor
                   : Qt.CrossCursor
        property string mode: ""          // draw, move, resize, pass
        property real sx: 0
        property real sy: 0
        property var orig: null
        property int clicks: 0
        property real lastClick: 0
        onPositionChanged: (m) => {
            if (!pressed) { ml.hoverOp = ml.tool === "textsel" || ml.tool === "rectsel" ? ml.hitTest(ml.px(m.x), ml.px(m.y)) : -1; return }
            const x = ml.px(m.x), y = ml.px(m.y)
            if (mode === "pass") { ml.dragTo(x, y); return }
            if (mode === "move") { ml.liveOp(PvShapes.moved(orig, x - sx, y - sy)); return }
            if (mode === "resize") {
                const b = PvShapes.bounds(orig)
                ml.liveOp(PvShapes.resized(orig, [b[0], b[1], Math.max(b[0] + 4 / ml.k, x), Math.max(b[1] + 4 / ml.k, y)]))
                return
            }
            if (mode !== "draw" || !ml.cur) return
            const c = Object.assign({}, ml.cur)
            if (c.t === "pen") c.p = c.p.concat([x, y])
            else {
                c.x2 = x; c.y2 = y
                if (m.modifiers & Qt.ShiftModifier) {
                    // Shift: squares, circles, 45° lines.
                    if (c.t === "line" || c.t === "arrow") {
                        const a = Math.round(Math.atan2(y - c.y1, x - c.x1) / (Math.PI / 4)) * Math.PI / 4, l = Math.hypot(x - c.x1, y - c.y1)
                        c.x2 = c.x1 + Math.cos(a) * l; c.y2 = c.y1 + Math.sin(a) * l
                    } else {
                        const d = Math.max(Math.abs(x - c.x1), Math.abs(y - c.y1))
                        c.x2 = c.x1 + Math.sign(x - c.x1 || 1) * d; c.y2 = c.y1 + Math.sign(y - c.y1 || 1) * d
                    }
                }
                PvShapes.withOutline(c)
            }
            ml.cur = c
        }
        onPressed: (m) => {
            ml.forceActiveFocus()
            if (ml.editing >= 0) ml.finishEdit()
            const now = Date.now()
            clicks = now - lastClick < 400 ? clicks + 1 : 1
            lastClick = now
            const x = ml.px(m.x), y = ml.px(m.y)
            sx = x; sy = y
            // A selected op's handle, or any op under the pointer (selection tools).
            if (ml.handleHit(x, y)) { mode = "resize"; orig = ml.ops[ml.selected]; return }
            const hit = ml.tool === "textsel" || ml.tool === "rectsel" || ml.drawTools.indexOf(ml.tool) >= 0 && ml.tool !== "pen" && ml.tool !== "marker" ? ml.hitTest(x, y) : -1
            if (hit >= 0) {
                ml.win.select(ml.key, hit)
                const o = ml.ops[hit]
                if (clicks >= 2 && o.t === "text") { ml.editText(hit); mode = ""; return }
                if (clicks >= 2 && o.t === "note") { ml.win.editNote(ml, hit); mode = ""; return }
                mode = "move"; orig = o
                return
            }
            if (ml.selected >= 0) ml.win.select("", -1)
            if (ml.drawTools.indexOf(ml.tool) < 0) { mode = "pass"; ml.pressAt(x, y, clicks, m.modifiers); return }
            mode = "draw"
            const t = ml.tool
            if (t === "pen") ml.cur = Object.assign(ml.styled("pen"), { a: 1, p: [x, y] })
            else if (t === "marker") ml.cur = Object.assign(ml.styled("pen"), { w: ml.win.mkWidth * ml.unit * 5, a: 0.4, p: [x, y] })
            else if (t === "text") {
                const o = Object.assign(ml.styled("text"), { s: ml.win.mkFontSize * ml.unit, font: ml.win.mkFont, b: ml.win.mkBold, i: ml.win.mkItalic, al: ml.win.mkAlign,
                                                             x1: x, y1: y, x2: x + 200 * ml.unit, y2: y + ml.win.mkFontSize * ml.unit * 1.6, text: "" })
                delete o.w
                mode = ""
                const n = ml.ops.concat([o])
                ml.setOps(n, "Add Text")
                ml.win.select(ml.key, n.length - 1)
                Qt.callLater(() => ml.editText(n.length - 1))
                ml.win.toolDone()
            } else if (t === "note") {
                mode = ""
                const n = ml.ops.concat([{ t: "note", c: "#ffcc00", x1: x - 10, y1: y - 10, text: "" }])
                ml.setOps(n, "Add Note")
                ml.win.select(ml.key, n.length - 1)
                Qt.callLater(() => ml.win.editNote(ml, n.length - 1))
                ml.win.toolDone()
            } else if (t === "sign") {
                mode = ""
                const s = ml.win.currentSignature()
                if (!s) { ml.win.showSignatures(); return }
                const w = 150 * ml.unit, h = w * s.h / Math.max(1, s.w)
                const n = ml.ops.concat([{ t: "sign", c: s.c || "#000000", w: 1.4 * ml.unit, x1: x - w / 2, y1: y - h / 2, x2: x + w / 2, y2: y + h / 2, strokes: s.strokes }])
                ml.setOps(n, "Sign")
                ml.win.select(ml.key, n.length - 1)
                ml.win.toolDone()
            } else {
                const o = Object.assign(ml.styled(t), { x1: x, y1: y, x2: x, y2: y })
                if (t === "loupe") { o.zoom = 2; o.c = "#888888"; delete o.fill }
                ml.cur = o
            }
        }
        onReleased: (m) => {
            const x = ml.px(m.x), y = ml.px(m.y)
            const md = mode
            mode = ""
            if (md === "pass") { ml.releasedAt(x, y); return }
            if (md === "move" || md === "resize") {
                const o = ml.live
                ml.live = null
                if (o && JSON.stringify(o) !== JSON.stringify(orig)) ml.replaceOp(ml.selected, o, md === "move" ? "Move" : "Resize")
                else cv.requestPaint()
                return
            }
            if (md !== "draw" || !ml.cur) return
            const c = ml.cur
            ml.cur = null
            if (c.t !== "pen" && Math.abs(c.x2 - c.x1) < 3 / ml.k && Math.abs(c.y2 - c.y1) < 3 / ml.k) {
                // A click with a shape tool: a shape of a usual size.
                const s = (c.t === "line" || c.t === "arrow" ? 80 : 60) * ml.unit
                c.x2 = c.x1 + s; c.y2 = c.t === "line" || c.t === "arrow" ? c.y1 : c.y1 + s * (c.t === "bubble" ? 0.75 : 1)
                PvShapes.withOutline(c)
            }
            const n = ml.ops.concat([c])
            ml.setOps(n, c.t === "pen" ? "Drawing" : "Add Shape")
            if (c.t !== "pen") { ml.win.select(ml.key, n.length - 1); ml.win.toolDone() }
        }
    }
    property int hoverOp: -1
    // While moving/resizing: the op as it is now (drawn in place of the original).
    property var live: null
    function liveOp(o) {
        live = o
        const n = ops.slice(); n[selected] = o
        liveOps = n
    }
    property var liveOps: null
    onLiveChanged: if (!live) liveOps = null

    // Text being typed into a text box.
    TextArea {
        id: editor
        visible: ml.editing >= 0
        readonly property var op: ml.editing >= 0 && ml.editing < ml.ops.length ? ml.ops[ml.editing] : null
        x: op ? Math.min(op.x1, op.x2) * ml.k : 0
        y: op ? Math.min(op.y1, op.y2) * ml.k : 0
        width: op ? Math.max(40, Math.abs(op.x2 - op.x1) * ml.k) : 0
        height: Math.max(op ? Math.abs(op.y2 - op.y1) * ml.k : 0, contentHeight + 4)
        padding: 2 * ml.k
        leftPadding: 2 * ml.k
        topPadding: 0
        wrapMode: TextEdit.Wrap
        color: op ? op.c : "black"
        font.family: op && op.font ? op.font : Theme.family
        font.pixelSize: op ? Math.max(4, (op.s || 14) * ml.k) : 12
        font.bold: !!(op && op.b)
        font.italic: !!(op && op.i)
        horizontalAlignment: op && op.al === "center" ? TextEdit.AlignHCenter : op && op.al === "right" ? TextEdit.AlignRight : TextEdit.AlignLeft
        background: Rectangle { color: editor.op && editor.op.fill ? editor.op.fill : "transparent"; border.color: Theme.accent; border.width: 1 }
        Keys.onEscapePressed: ml.finishEdit()
        onActiveFocusChanged: if (!activeFocus && ml.editing >= 0) ml.finishEdit()
    }
    // Moving/resizing shows the live op.
    states: State {
        when: ml.liveOps !== null
        PropertyChanges { target: ml; ops: ml.liveOps }
    }
}
