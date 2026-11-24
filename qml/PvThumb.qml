import QtQuick
import Grove

// A small picture of a page (or of a whole picture document) for Grove
// Preview's sidebar and Contact Sheet, turned as the page is turned.
Item {
    id: t
    property var win
    property var doc
    property var pg: null            // a page; null: the picture document itself
    property real box: 120           // the longer side
    readonly property bool isImage: !pg
    readonly property real w0: pg ? pg.w : (doc && doc.img ? doc.img.w : 100)
    readonly property real h0: pg ? pg.h : (doc && doc.img ? doc.img.h : 100)
    readonly property int rot: pg ? pg.rot : (doc && doc.img ? doc.img.rotate : 0)
    readonly property bool sideways: rot % 180 !== 0
    readonly property real k: box / Math.max(1, sideways ? Math.max(h0, w0) : Math.max(w0, h0))
    width: Math.round((sideways ? h0 : w0) * k)
    height: Math.round((sideways ? w0 : h0) * k)
    Rectangle { x: 1; y: 2; width: parent.width; height: parent.height; color: Qt.rgba(0, 0, 0, 0.18); radius: 1 }
    Item {
        width: t.w0 * t.k
        height: t.h0 * t.k
        x: (t.width - width) / 2
        y: (t.height - height) / 2
        rotation: t.rot
        transform: Scale {
            origin.x: t.w0 * t.k / 2; origin.y: t.h0 * t.k / 2
            xScale: !t.pg && t.doc && t.doc.img && t.doc.img.flipH ? -1 : 1
            yScale: !t.pg && t.doc && t.doc.img && t.doc.img.flipV ? -1 : 1
        }
        Rectangle { anchors.fill: parent; color: t.isImage ? "transparent" : "white" }
        PvCanvas {
            anchors.fill: parent
            readonly property int px: Math.ceil(t.box * Screen.devicePixelRatio / 64) * 64
            request: !t.doc ? ""
                   : t.isImage ? JSON.stringify({ kind: "image", path: t.doc.path, px: px, alpha: t.doc.img ? t.doc.img.alpha : [], adjust: t.doc.img ? t.doc.img.adjust : {} })
                   : t.pg.blank ? ""
                   : t.pg.image ? JSON.stringify({ kind: "image", path: t.pg.image, px: px })
                   : JSON.stringify({ kind: "pdf", path: t.pg.src, page: t.pg.page, password: t.win.passwordFor(t.pg.src), hide: t.pg.delAnnots || [], px: px, rev: t.doc.rev || 0 })
        }
    }
}
