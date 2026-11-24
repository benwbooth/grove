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
    function setZoom(z) {
        const before = k
        zoom = Math.max(0.1, Math.min(8, z))
        fit = "none"
        keepCenter(before)
    }
    // Zoom steps stop at 100% on the way past it (buttons, keys, Ctrl+wheel).
    function zoomBy(f) {
        const one = 96 / 72, to = dv.k * f
        if ((dv.k < one * 0.995 && to > one * 1.005) || (dv.k > one * 1.005 && to < one * 0.995) || Math.abs(to - one) < one * 0.03) { dv.fit = "actual"; return }
        setZoom(to)
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
    function pageUp() { list.contentY = Math.max(0, list.contentY - list.height * 0.9) }
    function pageDown() { list.contentY = Math.min(Math.max(0, list.contentHeight - list.height), list.contentY + list.height * 0.9) }
    function viewState() { return { y: Math.round(list.contentY), fit: fit, zoom: zoom, display: display } }

    // Which page is in the middle of the view.
    function updateCurrent() {
        if (display === "contact" || !pages.length) return
        const it = list.indexAt(list.width / 2 + list.contentX, list.contentY + list.height / 3)
        if (it < 0) return
        const i = display === "two" ? it * 2 : it
        if (i !== win.currentPage) win.setCurrentQuiet(i)
    }

    PanWheel { flickable: list; enabled: dv.display !== "contact" }
    WheelHandler {
        acceptedModifiers: Qt.ControlModifier
        acceptedDevices: PointerDevice.AllDevices
        onWheel: (e) => dv.zoomBy(e.angleDelta.y > 0 || e.pixelDelta.y > 0 ? 1.12 : 1 / 1.12)
    }
    PinchHandler {
        target: null
        property real start: 1
        onActiveChanged: if (active) start = dv.k
        onActiveScaleChanged: if (active) dv.setZoom(start * activeScale)
    }

    Rectangle { anchors.fill: parent; color: Theme.dark ? "#262628" : "#e9e9eb" }

    ListView {
        id: list
        anchors.fill: parent
        visible: dv.display !== "contact"
        clip: true
        interactive: false
        model: dv.display === "two" ? Math.ceil(dv.pages.length / 2) : dv.pages.length
        spacing: dv.display === "single" ? 40 : 14
        topMargin: 16
        bottomMargin: 16
        cacheBuffer: Math.max(400, height)
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.HorizontalAndVerticalFlick
        contentWidth: Math.max(width, dv.maxW * dv.k + 48)
        snapMode: dv.display === "single" ? ListView.SnapOneItem : ListView.NoSnap
        ScrollBar.vertical: GScrollBar {}
        ScrollBar.horizontal: GScrollBar {}
        onContentYChanged: Qt.callLater(dv.updateCurrent)
        delegate: Item {
            id: row
            required property int index
            width: list.contentWidth
            height: dv.display === "single" ? Math.max(list.height - 32, pages.height) : pages.height
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
