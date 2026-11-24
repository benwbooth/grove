import QtQuick
import QtQuick.Controls.Basic
import Grove

// One page of a document in Grove Preview: the page drawn by MuPDF
// (PvCanvas), turned as the user turned it, with text selection, search
// matches, links, form fields to fill in, and the page's Markup.
// Units are points; `k` is screen pixels per point.
Item {
    id: pp
    property var win
    property var doc
    property int index: 0
    property var pg: null
    property real k: 1
    readonly property bool sideways: pg ? pg.rot % 180 !== 0 : false
    readonly property real pw: pg ? pg.w : 612
    readonly property real ph: pg ? pg.h : 792
    readonly property string key: doc && pg ? doc.id + ":" + pg.key : ""
    width: (sideways ? ph : pw) * k
    height: (sideways ? pw : ph) * k
    readonly property real dpr: Screen.devicePixelRatio

    // What the window has selected / found on this page.
    readonly property var textSel: win.textSel && win.textSel.key === key ? win.textSel : null
    readonly property var rectSel: win.rectSel && win.rectSel.key === key ? win.rectSel.rect : null
    readonly property var hits: (win.searchHits || []).filter(h => h.docId === (doc ? doc.id : -1) && h.src === (pg ? pg.src : "") && h.page === (pg ? pg.page : -1))
    readonly property var widgets: doc && pg && doc.widgets && doc.widgets[pg.src] ? doc.widgets[pg.src].filter(w => w.page === pg.page) : []
    property var links: []
    onKeyChanged: loadLinks()
    Component.onCompleted: loadLinks()
    function loadLinks() {
        links = []
        if (!pg || pg.blank || pg.image || !doc) return
        const src = pg.src, n = pg.page, k0 = key
        Async.call(win.app.grove, kk => win.app.grove.pvLinks(kk, src, win.passwordFor(src), n), r => { if (pp.key === k0 && Array.isArray(r)) pp.links = r })
    }
    function linkAt(x, y) {
        for (const l of links) if (x >= l.r[0] && x <= l.r[2] && y >= l.r[1] && y <= l.r[3]) return l
        return null
    }

    Rectangle {
        // The shadow under the paper.
        x: (pp.width - (pp.sideways ? paper.height : paper.width)) / 2 + 1
        y: (pp.height - (pp.sideways ? paper.width : paper.height)) / 2 + 2
        width: pp.sideways ? paper.height : paper.width
        height: pp.sideways ? paper.width : paper.height
        color: Qt.rgba(0, 0, 0, Theme.dark ? 0.5 : 0.14)
        radius: 2
    }

    Item {
        id: paper
        readonly property bool sideways: pp.sideways
        width: pp.pw * pp.k
        height: pp.ph * pp.k
        x: (pp.width - width) / 2
        y: (pp.height - height) / 2
        rotation: pp.pg ? pp.pg.rot : 0
        Rectangle { anchors.fill: parent; color: "white" }
        PvCanvas {
            id: canvas
            anchors.fill: parent
            readonly property int px: Math.min(5000, Math.ceil(pp.pw * pp.k * pp.dpr / 96) * 96)
            request: !pp.pg || pp.pg.blank ? ""
                   : pp.pg.image ? JSON.stringify({ kind: "image", path: pp.pg.image, px: px })
                   : JSON.stringify({ kind: "pdf", path: pp.pg.src, page: pp.pg.page, password: pp.win.passwordFor(pp.pg.src), hide: pp.pg.delAnnots || [], px: px, rev: pp.doc ? pp.doc.rev || 0 : 0 })
        }
        BusyDots { anchors.centerIn: parent; running: !canvas.ready && canvas.request !== "" && canvas.error === "" }
        GLabel { anchors.centerIn: parent; visible: canvas.error !== ""; text: canvas.error; color: "#888"; width: parent.width - 20; wrapMode: Text.Wrap; horizontalAlignment: Text.AlignHCenter }

        // Search matches (the current one stronger).
        Repeater {
            model: [].concat(...pp.hits.map(h => h.quads))
            Rectangle {
                required property var modelData
                readonly property var q: modelData
                x: Math.min(q[0], q[4]) * pp.k - 1
                y: Math.min(q[1], q[3]) * pp.k - 1
                width: (Math.max(q[2], q[6]) - Math.min(q[0], q[4])) * pp.k + 2
                height: (Math.max(q[5], q[7]) - Math.min(q[1], q[3])) * pp.k + 2
                radius: 2
                color: Qt.rgba(1, 0.85, 0, 0.42)
            }
        }
        Repeater {
            model: pp.win.searchCurrentHit && pp.hits.indexOf(pp.win.searchCurrentHit) >= 0 ? pp.win.searchCurrentHit.quads : []
            Rectangle {
                required property var modelData
                readonly property var q: modelData
                x: Math.min(q[0], q[4]) * pp.k - 2
                y: Math.min(q[1], q[3]) * pp.k - 2
                width: (Math.max(q[2], q[6]) - Math.min(q[0], q[4])) * pp.k + 4
                height: (Math.max(q[5], q[7]) - Math.min(q[1], q[3])) * pp.k + 4
                radius: 3
                color: Qt.rgba(1, 0.6, 0, 0.5)
                border.color: "#ff9500"
            }
        }
        // Selected text.
        Repeater {
            model: pp.textSel ? pp.textSel.quads : []
            Rectangle {
                required property var modelData
                readonly property var q: modelData
                x: Math.min(q[0], q[4]) * pp.k
                y: Math.min(q[1], q[3]) * pp.k
                width: (Math.max(q[2], q[6]) - Math.min(q[0], q[4])) * pp.k
                height: (Math.max(q[5], q[7]) - Math.min(q[1], q[3])) * pp.k
                color: Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.3)
            }
        }
        // A rectangular selection, outside dimmed when cropping.
        Rectangle {
            visible: pp.rectSel !== null
            x: pp.rectSel ? Math.min(pp.rectSel[0], pp.rectSel[2]) * pp.k : 0
            y: pp.rectSel ? Math.min(pp.rectSel[1], pp.rectSel[3]) * pp.k : 0
            width: pp.rectSel ? Math.abs(pp.rectSel[2] - pp.rectSel[0]) * pp.k : 0
            height: pp.rectSel ? Math.abs(pp.rectSel[3] - pp.rectSel[1]) * pp.k : 0
            color: Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.12)
            border.color: Theme.accent
            border.width: 1
        }
        // A page cropped but not saved yet: the part cut away, dimmed.
        Item {
            anchors.fill: parent
            visible: !!(pp.pg && pp.pg.crop)
            readonly property var c: pp.pg && pp.pg.crop ? pp.pg.crop.map(v => v * pp.k) : [0, 0, 0, 0]
            Rectangle { x: 0; y: 0; width: parent.width; height: parent.c[1]; color: Qt.rgba(0, 0, 0, 0.45) }
            Rectangle { x: 0; y: parent.c[3]; width: parent.width; height: parent.height - y; color: Qt.rgba(0, 0, 0, 0.45) }
            Rectangle { x: 0; y: parent.c[1]; width: parent.c[0]; height: parent.c[3] - parent.c[1]; color: Qt.rgba(0, 0, 0, 0.45) }
            Rectangle { x: parent.c[2]; y: parent.c[1]; width: parent.width - x; height: parent.c[3] - parent.c[1]; color: Qt.rgba(0, 0, 0, 0.45) }
        }

        // Text selection, links, rectangular selection, text highlights:
        // what Markup passes on.
        PvMarkupLayer {
            id: layer
            anchors.fill: parent
            win: pp.win
            key: pp.key
            k: pp.k
            pdf: true
            ops: pp.pg ? pp.pg.annots || [] : []
            onEdited: (n, label) => pp.win.setPageOps(pp.doc, pp.index, n, label)
            property real ax: 0
            property real ay: 0
            property int mode: 0
            property bool moved: false
            property bool busy: false
            property var pending: null
            function select(bx, by) {
                if (!pp.pg || pp.pg.blank || pp.pg.image) return
                const args = [ax, ay, bx, by, mode]
                if (busy) { pending = args; return }
                busy = true
                const src = pp.pg.src, n = pp.pg.page, k0 = pp.key, keep = pp.win.tool
                Async.call(pp.win.app.grove, kk => pp.win.app.grove.pvSelect(kk, src, pp.win.passwordFor(src), n, args[0], args[1], args[2], args[3], args[4]), r => {
                    busy = false
                    if (r && r.quads) pp.win.textSel = { key: k0, docId: pp.doc.id, index: pp.index, quads: r.quads, text: r.text || "" }
                    if (pending) { const p = pending; pending = null; ax = p[0]; ay = p[1]; mode = p[4]; select(p[2], p[3]) }
                    else if (layer.finishHl) { layer.finishHl = false; layer.makeHl() }
                })
            }
            property bool finishHl: false
            function makeHl() {
                const s = pp.win.textSel
                if (!s || s.key !== pp.key || !s.quads.length) return
                const t = pp.win.tool
                pp.win.setPageOps(pp.doc, pp.index, (pp.pg.annots || []).concat([{ t: t, c: "" + pp.win.hlColor, quads: s.quads, text: s.text }]), t === "hl" ? "Highlight" : t === "ul" ? "Underline" : "Strikethrough")
                pp.win.textSel = null
            }
            onPressAt: (x, y, clicks, mods) => {
                ax = x; ay = y; moved = false
                const t = pp.win.tool
                if (t === "rectsel") { pp.win.textSel = null; pp.win.rectSel = { key: pp.key, docId: pp.doc.id, index: pp.index, rect: [x, y, x, y] }; return }
                pp.win.rectSel = null
                mode = clicks >= 3 ? 2 : clicks === 2 ? 1 : 0
                if (mode > 0) select(x, y)
                else if (!(mods & Qt.ShiftModifier)) pp.win.textSel = null
                pp.win.setCurrent(pp.doc, pp.index)
            }
            onDragTo: (x, y) => {
                moved = true
                if (pp.win.tool === "rectsel") { const r = pp.win.rectSel; if (r) pp.win.rectSel = Object.assign({}, r, { rect: [ax, ay, Math.max(0, Math.min(pp.pw, x)), Math.max(0, Math.min(pp.ph, y))] }); return }
                select(x, y)
            }
            onReleasedAt: (x, y) => {
                const t = pp.win.tool
                if (t === "rectsel") {
                    const r = pp.win.rectSel
                    if (r && (Math.abs(r.rect[2] - r.rect[0]) < 3 || Math.abs(r.rect[3] - r.rect[1]) < 3)) pp.win.rectSel = null
                    return
                }
                if (!moved && mode === 0) {
                    const l = pp.linkAt(x, y)
                    if (l) { pp.win.followLink(pp.doc, l); return }
                }
                if (t === "hl" || t === "ul" || t === "so") {
                    if (busy || pending) finishHl = true
                    else makeHl()
                }
            }
        }
        // Links show a pointing hand (and the address as a tooltip).
        Repeater {
            model: pp.win.tool === "textsel" ? pp.links : []
            MouseArea {
                required property var modelData
                x: modelData.r[0] * pp.k; y: modelData.r[1] * pp.k
                width: (modelData.r[2] - modelData.r[0]) * pp.k; height: (modelData.r[3] - modelData.r[1]) * pp.k
                cursorShape: Qt.PointingHandCursor
                hoverEnabled: true
                onClicked: pp.win.followLink(pp.doc, modelData)
                GTip { visible: parent.containsMouse; text: parent.modelData.uri ? parent.modelData.uri : "Go to page " + (parent.modelData.page + 1) }
            }
        }
        // Form fields to fill in.
        Repeater {
            model: pp.widgets
            PvField {
                required property var modelData
                win: pp.win
                doc: pp.doc
                w: modelData
                k: pp.k
                fieldKey: pp.pg.src + "|" + modelData.page + "|" + modelData.i
            }
        }
    }
}
