import QtQuick
import QtQuick.Controls.Basic
import Grove

// Grove Preview's document view: the pages one under another (Continuous
// Scroll), one at a time (Single Page), side by side (Two Pages) or as a
// grid of thumbnails (Contact Sheet). Zoom to fit, actual size, or any
// zoom (Ctrl+wheel, pinch, Ctrl+= / Ctrl+−).
Item {
    id: dv
    property var win
    property var doc: null
    readonly property var pages: doc ? doc.pages : []
    property string display: "continuous"   // continuous, single, two, contact
    property string fit: "width"            // width, page, actual, none
    property real zoom: 1
    readonly property int current: win.currentPage

    function dispW(p) { return p.rot % 180 ? p.h : p.w }
    function dispH(p) { return p.rot % 180 ? p.w : p.h }
    readonly property real maxW: pages.reduce((m, p) => Math.max(m, dispW(p)), 1) * (display === "two" ? 2 : 1) + (display === "two" ? 12 : 0)
    readonly property var curPage: pages.length ? pages[Math.max(0, Math.min(pages.length - 1, current))] : null
    // Screen pixels per point.
    readonly property real k: {
        const w = Math.max(100, list.width - 48)
        const h = Math.max(100, list.height - 32)
        if (fit === "width") return w / maxW
        if (fit === "page" && curPage) return Math.min(w / (dispW(curPage) * (display === "two" ? 2.05 : 1)), h / dispH(curPage))
        if (fit === "actual") return 96 / 72
        return zoom
    }
    // The zoom pages are drawn for: follows `k` once it stops changing.
    property real renderK: k
    property bool zooming: false
    onKChanged: { zooming = true; settle.restart() }
    Timer { id: settle; interval: 220; onTriggered: { dv.renderK = dv.k; dv.zooming = false } }
    readonly property real percent: Math.round(k * 72 / 96 * 100)
    function setZoom(z, at) {
        const a = anchorOf(at)
        zoom = Math.max(0.1, Math.min(8, z))
        fit = "none"
        restore(a)
    }
    // Zoom steps stop at 100% on the way past it (buttons, keys, Ctrl+wheel).
    // `at` (a point in the view) stays put; without it, the middle does.
    function zoomBy(f, at) {
        const one = 96 / 72, to = dv.k * f
        if ((dv.k < one * 0.995 && to > one * 1.005) || (dv.k > one * 1.005 && to < one * 0.995) || Math.abs(to - one) < one * 0.03) {
            const a = anchorOf(at)
            dv.fit = "actual"
            restore(a)
            return
        }
        setZoom(to, at)
    }
    // What's under a point of the view, as a row and an offset from that
    // row's middle (page content scales about it), so a zoom can put the
    // same spot back under the point.
    function anchorOf(at) {
        const px = at ? at.x : list.width / 2, py = at ? at.y : list.height / 2
        const cx = list.contentX + px, cy = list.contentY + py
        let r = -1
        for (const d of [0, -10, 10, -24, 24, -48, 48]) {
            r = list.indexAt(list.contentWidth / 2, cy + d)
            if (r >= 0) break
        }
        const it = r >= 0 ? list.itemAtIndex(r) : null
        if (!it) return { r: -1, px: px, py: py, k: k }
        return { r: r, dy: cy - (it.y + it.height / 2), dx: cx - list.contentWidth / 2, px: px, py: py, k: k }
    }
    function restore(a, at) {
        if (!a || a.r < 0 || display === "contact") return
        const s = k / a.k
        const px = at ? at.x : a.px, py = at ? at.y : a.py
        list.forceLayout()
        list.positionViewAtIndex(a.r, ListView.Beginning)
        const it = list.itemAtIndex(a.r)
        if (!it) return
        const minY = list.originY - list.topMargin
        const maxY = Math.max(minY, list.originY + list.contentHeight + list.bottomMargin - list.height)
        list.contentY = Math.max(minY, Math.min(maxY, it.y + it.height / 2 + a.dy * s - py))
        list.contentX = Math.max(0, Math.min(Math.max(0, list.contentWidth - list.width), list.contentWidth / 2 + a.dx * s - px))
    }
    function keepCenter(before) {
        const cy = (list.contentY + list.height / 2) / Math.max(1, list.contentHeight)
        const cx = (list.contentX + list.width / 2) / Math.max(1, list.contentWidth)
        Qt.callLater(() => {
            list.contentY = Math.max(0, cy * list.contentHeight - list.height / 2)
            list.contentX = Math.max(0, cx * list.contentWidth - list.width / 2)
        })
    }
    function go(i) {
        if (display === "contact") return
        const row = display === "two" ? Math.floor(i / 2) : i
        list.positionViewAtIndex(Math.max(0, row), ListView.Beginning)
    }
    // Scroll to a point on a page (points from its top).
    function goTo(i, y) {
        go(i)
        if (y > 0) Qt.callLater(() => { list.contentY = Math.max(0, list.contentY + y * k - 20) })
    }
    // Page at a Time: PgDn/PgUp (and Space) go to the next/previous page's
    // top, scrolling within a page first only when it's zoomed taller than
    // the window. Single Page and Two Pages then show one page (or pair) per
    // window, with nothing of the next peeking in.
    property bool pageAtATime: true
    readonly property bool paged: pageAtATime && display !== "contact"
    readonly property bool fill: pageAtATime && (display === "single" || display === "two")
    function curRow() { return display === "two" ? Math.floor(current / 2) : current }
    function pageUp() {
        if (paged) {
            const it = list.itemAtIndex(curRow())
            if (it && it.y < list.contentY - 1) list.contentY = Math.max(it.y, list.contentY - list.height * 0.9)
            else if (curRow() > 0) go((curRow() - 1) * (display === "two" ? 2 : 1))
            else list.contentY = 0
            return
        }
        list.contentY = Math.max(0, list.contentY - list.height * 0.9)
    }
    function pageDown() {
        if (paged) {
            const it = list.itemAtIndex(curRow())
            const bottom = it ? it.y + it.height : 0
            if (it && bottom > list.contentY + list.height + 1) list.contentY = Math.min(bottom - list.height, list.contentY + list.height * 0.9)
            else if (curRow() + 1 < list.count) go((curRow() + 1) * (display === "two" ? 2 : 1))
            return
        }
        list.contentY = Math.min(Math.max(0, list.contentHeight - list.height), list.contentY + list.height * 0.9)
    }
    // Keep the current row lined up when the layout or window changes.
    onDisplayChanged: if (paged) Qt.callLater(() => go(current))
    onPageAtATimeChanged: if (paged) Qt.callLater(() => go(current))
    property int alignTo: -1
    Connections {
        target: list
        function onHeightChanged() { if (dv.paged) { dv.alignTo = dv.current; realign.restart() } }
    }
    Timer { id: realign; interval: 0; onTriggered: if (dv.alignTo >= 0) dv.go(dv.alignTo) }
    function viewState() { return { y: Math.round(list.contentY), fit: fit, zoom: zoom, display: display } }

    // Which page is in the middle of the view.
    function updateCurrent() {
        if (display === "contact" || !pages.length) return
        const it = list.indexAt(list.width / 2 + list.contentX, list.contentY + (paged ? 1 : list.height / 3))
        if (it < 0) return
        const i = display === "two" ? it * 2 : it
        if (i !== win.currentPage) win.setCurrentQuiet(i)
    }

    PanWheel { flickable: list; enabled: dv.display !== "contact" }
    WheelHandler {
        acceptedModifiers: Qt.ControlModifier
        acceptedDevices: PointerDevice.AllDevices
        onWheel: (e) => dv.zoomBy(e.angleDelta.y > 0 || e.pixelDelta.y > 0 ? 1.12 : 1 / 1.12, point.position)
    }
    // Pinching stretches the pages already drawn, about the fingers (no
    // relayout per step: that's what made it slow), and lays them out at
    // the new zoom when the fingers pause or lift, the same spot still
    // under them.
    PinchHandler {
        id: pinch
        target: null
        property var a: null
        property real base: 1     // activeScale at the last settle
        property real k0: 1       // k at the last settle
        readonly property real rel: Math.max(0.1 / k0, Math.min(8 / k0, activeScale / base))
        // Where the fingers are (the centroid resets once they lift).
        property point at: Qt.point(0, 0)
        onCentroidChanged: if (active) at = centroid.position
        onActiveChanged: {
            if (active) {
                k0 = dv.k; base = activeScale
                at = centroid.position
                a = dv.anchorOf(at)
                dv.pinching = true
            } else {
                settlePinch.stop()
                dv.settlePinch()
                dv.pinching = false
            }
        }
        onActiveScaleChanged: if (active) settlePinch.restart()
    }
    property bool pinching: false
    Timer { id: settlePinch; interval: 260; onTriggered: dv.settlePinch() }
    function settlePinch() {
        if (!pinch.a) return
        const at = pinch.at
        const rel = pinch.rel
        const a = pinch.a
        zoom = Math.max(0.1, Math.min(8, pinch.k0 * rel))
        fit = "none"
        pinch.a = null       // drop the live stretch before laying out
        restore(a, at)
        if (pinch.active) {
            pinch.k0 = k; pinch.base = pinch.activeScale
            pinch.a = anchorOf(at)
        }
    }

    Rectangle { anchors.fill: parent; color: Theme.dark ? "#262628" : "#e9e9eb" }

    clip: true
    ListView {
        id: list
        anchors.fill: parent
        visible: dv.display !== "contact"
        clip: !pinch.a
        transform: [
            Scale {
                origin.x: pinch.a ? pinch.a.px : 0; origin.y: pinch.a ? pinch.a.py : 0
                xScale: pinch.a ? pinch.rel : 1; yScale: xScale
            },
            Translate {
                x: pinch.a ? pinch.at.x - pinch.a.px : 0
                y: pinch.a ? pinch.at.y - pinch.a.py : 0
            }
        ]
        interactive: false
        model: dv.display === "two" ? Math.ceil(dv.pages.length / 2) : dv.pages.length
        // Page at a Time rows are a whole window tall, so none show beside another.
        spacing: dv.fill ? 0 : 14
        topMargin: dv.fill ? 0 : 16
        bottomMargin: dv.fill ? 0 : 16
        cacheBuffer: Math.max(400, height)
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.HorizontalAndVerticalFlick
        contentWidth: Math.max(width, dv.maxW * dv.k + 48)
        ScrollBar.vertical: GScrollBar {}
        ScrollBar.horizontal: GScrollBar {}
        onContentYChanged: Qt.callLater(dv.updateCurrent)
        delegate: Item {
            id: row
            required property int index
            width: list.contentWidth
            // From the page sizes, not the Row (a positioner lays out later,
            // and zooming about a point needs the new heights at once).
            readonly property real rowH: {
                const a = dv.pages[dv.display === "two" ? index * 2 : index]
                const b = dv.display === "two" ? dv.pages[index * 2 + 1] : null
                return Math.max(a ? dv.dispH(a) : 0, b ? dv.dispH(b) : 0) * dv.k
            }
            height: dv.fill ? Math.max(list.height, rowH + 32) : rowH
            Row {
                id: pages
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.verticalCenter: parent.verticalCenter
                spacing: 12
                Repeater {
                    model: dv.display === "two" ? (row.index * 2 + 1 < dv.pages.length ? 2 : 1) : 1
                    Item {
                        id: slot
                        required property int index
                        readonly property int pi: dv.display === "two" ? row.index * 2 + slot.index : row.index
                        width: page.width
                        height: page.height
                        anchors.verticalCenter: parent ? parent.verticalCenter : undefined
                        PvPage {
                            id: page
                            win: dv.win
                            doc: dv.doc
                            index: slot.pi
                            pg: dv.pages[slot.pi] || null
                            k: dv.k
                            renderK: dv.renderK
                            zooming: dv.zooming
                        }
                    }
                }
            }
        }
    }

    // Contact Sheet: every page small; click one to go there.
    GridView {
        id: grid
        anchors.fill: parent
        anchors.margins: 16
        visible: dv.display === "contact"
        clip: true
        model: dv.pages.length
        cellWidth: Math.max(140, Math.floor(width / Math.max(1, Math.floor(width / 180))))
        cellHeight: cellWidth * 1.35
        ScrollBar.vertical: GScrollBar {}
        KineticWheel { flickable: grid }
        delegate: Item {
            required property int index
            readonly property var p: dv.pages[index]
            width: grid.cellWidth
            height: grid.cellHeight
            Rectangle {
                anchors.fill: thumb
                anchors.margins: -4
                radius: 6
                color: dv.win.isPageSelected(dv.doc, index) ? Theme.accent : "transparent"
            }
            PvThumb {
                id: thumb
                anchors.horizontalCenter: parent.horizontalCenter
                y: 6
                win: dv.win
                doc: dv.doc
                pg: parent.p
                box: grid.cellWidth - 28
            }
            GLabel { anchors.horizontalCenter: parent.horizontalCenter; anchors.top: thumb.bottom; anchors.topMargin: 6; text: String(index + 1); color: Theme.fgSecondary; font.pixelSize: Theme.small }
            MouseArea {
                anchors.fill: parent
                onClicked: (m) => dv.win.clickPage(dv.doc, index, m.modifiers)
                onDoubleClicked: { dv.display = "continuous"; dv.win.setCurrent(dv.doc, index) }
            }
        }
    }
}
