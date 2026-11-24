import QtQuick
import QtQuick.Controls.Basic
import Grove

// Grove Preview's picture view: the picture fitted (or at any zoom, panned
// in every direction) on a checkerboard where it's see-through, with its
// edits shown live: Adjust Color and Instant Alpha (drawn by PvCanvas),
// rotation and flips, the crop to come, a rectangular selection, and
// Markup. Units are picture pixels; `k` is screen pixels per pixel.
Item {
    id: iv
    property var win
    property var doc: null
    readonly property var img: doc ? doc.img : null
    property string fit: "fit"           // fit, actual, none
    property real zoom: 1
    readonly property real dpr: Screen.devicePixelRatio
    readonly property real iw: img ? img.w : 1
    readonly property real ih: img ? img.h : 1
    readonly property bool sideways: img ? img.rotate % 180 !== 0 : false
    readonly property real k: {
        const w = Math.max(50, view.width - 40), h = Math.max(50, view.height - 40)
        const dw = sideways ? ih : iw, dh = sideways ? iw : ih
        if (fit === "fit") return Math.min(w / dw, h / dh, 1 / dpr * 2)
        if (fit === "actual") return 1 / dpr
        return zoom
    }
    readonly property real percent: Math.round(k * dpr * 100)
    // The zoom pictures are drawn for: follows `k` once it stops changing
    // (pinching only stretches what's there).
    property real renderK: k
    property bool zooming: false
    onKChanged: { zooming = true; settle.restart() }
    Timer { id: settle; interval: 220; onTriggered: { iv.renderK = iv.k; iv.zooming = false } }
    function setZoom(z) { zoom = Math.max(0.02, Math.min(32, z)); fit = "none" }
    function zoomBy(f) { setZoom(k * f) }
    readonly property string key: doc ? doc.id + ":img" : ""
    readonly property var rectSel: win.rectSel && win.rectSel.key === key ? win.rectSel.rect : null

    Rectangle { anchors.fill: parent; color: Theme.dark ? "#1b1b1d" : "#ececee" }
    PanWheel { flickable: view }
    WheelHandler {
        acceptedModifiers: Qt.ControlModifier
        acceptedDevices: PointerDevice.AllDevices
        onWheel: (e) => iv.zoomBy(e.angleDelta.y > 0 || e.pixelDelta.y > 0 ? 1.12 : 1 / 1.12)
    }
    PinchHandler {
        target: null
        property real start: 1
        onActiveChanged: if (active) start = iv.k
        onActiveScaleChanged: if (active) iv.setZoom(start * activeScale)
    }

    Flickable {
        id: view
        anchors.fill: parent
        clip: true
        interactive: false
        contentWidth: Math.max(width, stage.width + 40)
        contentHeight: Math.max(height, stage.height + 40)
        ScrollBar.vertical: GScrollBar {}
        ScrollBar.horizontal: GScrollBar {}
        Item {
            id: stage
            width: (iv.sideways ? iv.ih : iv.iw) * iv.k
            height: (iv.sideways ? iv.iw : iv.ih) * iv.k
            x: (view.contentWidth - width) / 2
            y: (view.contentHeight - height) / 2
            // See-through: a checkerboard.
            Canvas {
                anchors.fill: parent
                visible: iv.doc && iv.doc.info && iv.doc.info.alpha || (iv.img && iv.img.alpha.length > 0)
                onPaint: {
                    const g = getContext("2d"), s = 8
                    for (let y = 0; y < height; y += s) for (let x = 0; x < width; x += s) {
                        g.fillStyle = ((x + y) / s) % 2 ? "#cfcfcf" : "#ffffff"
                        g.fillRect(x, y, s, s)
                    }
                }
                onWidthChanged: requestPaint()
                onHeightChanged: requestPaint()
            }
            Item {
                id: pic
                width: iv.iw * iv.k
                height: iv.ih * iv.k
                x: (stage.width - width) / 2
                y: (stage.height - height) / 2
                rotation: iv.img ? iv.img.rotate : 0
                transform: Scale {
                    origin.x: pic.width / 2; origin.y: pic.height / 2
                    xScale: iv.img && iv.img.flipH ? -1 : 1
                    yScale: iv.img && iv.img.flipV ? -1 : 1
                }
                PvCanvas {
                    id: canvas
                    anchors.fill: parent
                    readonly property int px: Math.min(Math.max(iv.iw, iv.ih), Math.ceil(Math.max(iv.iw, iv.ih) * iv.renderK * iv.dpr / 128) * 128)
                    smooth: !iv.zooming
                    request: iv.doc ? JSON.stringify({ kind: "image", path: iv.doc.path, px: Math.max(64, px), alpha: iv.img.alpha, adjust: iv.img.adjust, rev: iv.doc.rev || 0 }) : ""
                }
                BusyDots { anchors.centerIn: parent; running: !canvas.ready && canvas.error === "" }
                GLabel { anchors.centerIn: parent; visible: canvas.error !== ""; text: canvas.error; color: Theme.fgSecondary }

                // The crop to come: outside dimmed.
                Item {
                    anchors.fill: parent
                    visible: !!(iv.img && iv.img.crop)
                    readonly property var c: iv.img && iv.img.crop ? [iv.img.crop[0], iv.img.crop[1], iv.img.crop[0] + iv.img.crop[2], iv.img.crop[1] + iv.img.crop[3]].map(v => v * iv.k) : [0, 0, 0, 0]
                    Rectangle { x: 0; y: 0; width: parent.width; height: parent.c[1]; color: Qt.rgba(0, 0, 0, 0.55) }
                    Rectangle { x: 0; y: parent.c[3]; width: parent.width; height: parent.height - y; color: Qt.rgba(0, 0, 0, 0.55) }
                    Rectangle { x: 0; y: parent.c[1]; width: parent.c[0]; height: parent.c[3] - parent.c[1]; color: Qt.rgba(0, 0, 0, 0.55) }
                    Rectangle { x: parent.c[2]; y: parent.c[1]; width: parent.width - x; height: parent.c[3] - parent.c[1]; color: Qt.rgba(0, 0, 0, 0.55) }
                }
                // The rectangular selection, with its size.
                Rectangle {
                    id: selBox
                    visible: iv.rectSel !== null
                    x: iv.rectSel ? Math.min(iv.rectSel[0], iv.rectSel[2]) * iv.k : 0
                    y: iv.rectSel ? Math.min(iv.rectSel[1], iv.rectSel[3]) * iv.k : 0
                    width: iv.rectSel ? Math.abs(iv.rectSel[2] - iv.rectSel[0]) * iv.k : 0
                    height: iv.rectSel ? Math.abs(iv.rectSel[3] - iv.rectSel[1]) * iv.k : 0
                    color: "transparent"
                    border.color: "white"
                    border.width: 1
                    Rectangle { anchors.fill: parent; anchors.margins: -1; color: "transparent"; border.color: "black"; border.width: 1; opacity: 0.5 }
                    GLabel {
                        anchors.top: parent.bottom; anchors.topMargin: 4
                        anchors.horizontalCenter: parent.horizontalCenter
                        visible: iv.rectSel !== null
                        text: iv.rectSel ? Math.round(Math.abs(iv.rectSel[2] - iv.rectSel[0])) + " × " + Math.round(Math.abs(iv.rectSel[3] - iv.rectSel[1])) : ""
                        color: "white"; style: Text.Outline; styleColor: "black"
                        font.pixelSize: Theme.small
                    }
                }
                // Instant Alpha being dragged: how far is how alike.
                Rectangle {
                    visible: layer.alphaOn
                    x: layer.ax * iv.k - width / 2
                    y: layer.ay * iv.k - height / 2
                    width: Math.max(8, layer.tol * 2 * iv.k / Math.max(0.2, iv.k)) ; height: width
                    radius: width / 2
                    color: Qt.rgba(1, 0.2, 0.6, 0.18)
                    border.color: "#ff2d92"
                }

                PvMarkupLayer {
                    id: layer
                    anchors.fill: parent
                    win: iv.win
                    key: iv.key
                    k: iv.k
                    pdf: false
                    unit: Math.max(1, Math.max(iv.iw, iv.ih) / 1000)
                    ops: iv.img ? iv.img.ops : []
                    onEdited: (n, label) => iv.win.changeImage(iv.doc, { ops: n }, label)
                    property real ax: 0
                    property real ay: 0
                    property real tol: 0
                    property bool alphaOn: false
                    onPressAt: (x, y) => {
                        ax = Math.max(0, Math.min(iv.iw, x)); ay = Math.max(0, Math.min(iv.ih, y))
                        if (iv.win.tool === "alpha") { alphaOn = true; tol = 12; return }
                        iv.win.rectSel = { key: iv.key, docId: iv.doc.id, rect: [ax, ay, ax, ay] }
                    }
                    onDragTo: (x, y) => {
                        if (alphaOn) { tol = Math.min(160, 12 + Math.hypot(x - ax, y - ay) * iv.k * 0.6); return }
                        const r = iv.win.rectSel
                        if (r) iv.win.rectSel = Object.assign({}, r, { rect: [ax, ay, Math.max(0, Math.min(iv.iw, x)), Math.max(0, Math.min(iv.ih, y))] })
                    }
                    onReleasedAt: {
                        if (alphaOn) {
                            alphaOn = false
                            iv.win.changeImage(iv.doc, { alpha: iv.img.alpha.concat([{ x: ax, y: ay, tol: Math.round(tol) }]) }, "Instant Alpha")
                            return
                        }
                        const r = iv.win.rectSel
                        if (r && (Math.abs(r.rect[2] - r.rect[0]) < 3 || Math.abs(r.rect[3] - r.rect[1]) < 3)) iv.win.rectSel = null
                    }
                }
            }
        }
    }
}
