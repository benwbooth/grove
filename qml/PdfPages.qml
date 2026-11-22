import QtQuick
import QtQuick.Controls.Basic

// Quick Look's PDF reader: the pages one under another, each drawn at the
// view's width by pdftoppm (image://pdfpage) only when it scrolls near,
// with a page counter and previous/next buttons (Page Up/Page Down).
Item {
    id: pp
    property string src: ""
    property real mtime: 0
    property int pages: 0
    property var sizes: []          // [[w, h] points] per page (or just the first)
    readonly property int current: Math.max(1, Math.min(pages, list.indexAt(list.width / 2, list.contentY + list.height / 3) + 1))
    // Width the pages are drawn at, in steps so resizing doesn't redraw
    // on every pixel.
    // Zoom (1 = the page fits the width); zoomed pages pan in every direction.
    property real zoom: 1
    readonly property real pageW: Math.max(120, Math.min(list.width - 48, 1400) * zoom)
    function zoomBy(f) { const cx = (list.contentX + list.width / 2) / Math.max(1, list.contentWidth); zoom = Math.max(0.25, Math.min(6, zoom * f)); Qt.callLater(() => list.contentX = Math.max(0, cx * list.contentWidth - list.width / 2)) }
    readonly property int drawWidth: Math.ceil(Math.min(pageW, 4000) * Screen.devicePixelRatio / 160) * 160

    function pageSize(i) {
        const s = sizes.length ? sizes[Math.min(i, sizes.length - 1)] : [612, 792]
        return s
    }
    function go(n) { list.positionViewAtIndex(Math.max(0, Math.min(pages - 1, n - 1)), ListView.Beginning) }
    function next() { go(current + 1) }
    function prev() { go(current - 1) }

    PanWheel { flickable: list }
    // Ctrl+wheel zooms.
    WheelHandler {
        acceptedModifiers: Qt.ControlModifier
        acceptedDevices: PointerDevice.AllDevices
        onWheel: (e) => pp.zoomBy(e.angleDelta.y > 0 || e.pixelDelta.y > 0 ? 1.15 : 1 / 1.15)
    }
    Shortcut { sequences: [StandardKey.ZoomIn, "Ctrl+="]; enabled: pp.visible; onActivated: pp.zoomBy(1.25) }
    Shortcut { sequence: StandardKey.ZoomOut; enabled: pp.visible; onActivated: pp.zoomBy(0.8) }
    Shortcut { sequence: "Ctrl+0"; enabled: pp.visible; onActivated: pp.zoom = 1 }
    ListView {
        id: list
        anchors.fill: parent
        anchors.bottomMargin: bar.height + 6
        clip: true
        model: pp.pages
        // Its own wheel scrolling off: the PanWheel glides (momentum).
        interactive: false
        spacing: 14
        cacheBuffer: height
        topMargin: 12
        bottomMargin: 12
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.HorizontalAndVerticalFlick
        contentWidth: Math.max(width, pp.pageW + 48)
        ScrollBar.vertical: GScrollBar {}
        ScrollBar.horizontal: GScrollBar {}
        delegate: Item {
            id: pg
            required property int index
            readonly property var size: pp.pageSize(index)
            width: list.contentWidth
            height: Math.round(pp.pageW * size[1] / Math.max(1, size[0]))
            Rectangle {
                id: paper
                anchors.horizontalCenter: parent.horizontalCenter
                width: pp.pageW
                height: parent.height
                color: "white"
                border.width: 1
                border.color: Theme.glassEdge
                Image {
                    id: img
                    anchors.fill: parent
                    asynchronous: true
                    fillMode: Image.PreserveAspectFit
                    sourceSize.width: pp.drawWidth
                    source: pp.src ? "image://pdfpage/" + encodeURIComponent(pp.src) + "?p=" + (pg.index + 1) + "&m=" + pp.mtime : ""
                    smooth: true
                    mipmap: true
                }
                PreviewSpinner { image: img; size: 20; anchors.centerIn: parent; backdrop: false; color: "#888888" }
            }
        }
    }

    // Page counter and buttons (Preview's page controls).
    Rectangle {
        id: bar
        anchors.bottom: parent.bottom
        anchors.horizontalCenter: parent.horizontalCenter
        width: row.implicitWidth + 16
        height: Math.round(30 * Theme.pt)
        radius: height / 2
        color: Theme.glass
        border.width: 1
        border.color: Theme.glassEdge
        Row {
            id: row
            anchors.centerIn: parent
            spacing: 6
            GButton { iconName: Theme.glyph("go-up"); iconSize: 14; enabled: pp.current > 1; tip: "Previous page (Page Up)"; onClicked: pp.prev(); anchors.verticalCenter: parent.verticalCenter }
            GLabel {
                anchors.verticalCenter: parent.verticalCenter
                text: "Page " + pp.current + " of " + pp.pages
                color: Theme.fgSecondary
                font.pixelSize: Theme.small
            }
            GButton { iconName: Theme.glyph("go-down"); iconSize: 14; enabled: pp.current < pp.pages; tip: "Next page (Page Down)"; onClicked: pp.next(); anchors.verticalCenter: parent.verticalCenter }
            Rectangle { width: 1; height: 16; color: Theme.separator; anchors.verticalCenter: parent.verticalCenter }
            GButton { iconName: "zoom-out"; iconSize: 14; tip: "Zoom out (Ctrl+−, Ctrl+wheel)"; onClicked: pp.zoomBy(0.8); anchors.verticalCenter: parent.verticalCenter }
            GLabel { anchors.verticalCenter: parent.verticalCenter; text: Math.round(pp.zoom * 100) + "%"; color: Theme.fgSecondary; font.pixelSize: Theme.small
                MouseArea { anchors.fill: parent; onClicked: pp.zoom = 1 } }
            GButton { iconName: "zoom-in"; iconSize: 14; tip: "Zoom in (Ctrl+=, Ctrl+wheel); click the percentage to fit the width"; onClicked: pp.zoomBy(1.25); anchors.verticalCenter: parent.verticalCenter }
        }
    }
}
