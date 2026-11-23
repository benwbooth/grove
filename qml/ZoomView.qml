import QtQuick

// Zoom and pan for a picture (Quick Look, the preview pane and column):
// trackpad pinch (or Ctrl+wheel) zooms around the fingers/pointer; two-
// finger scrolling or dragging pans once zoomed; double-click goes between
// fitting the view and the picture's own pixels. Put the picture inside,
// filling it; `key` (the file) resets the zoom when it changes.
Item {
    id: zv
    clip: true
    default property alias content: stage.data
    property string key: ""
    // The zoom at which one picture pixel is one screen pixel (from the
    // picture's natural and painted size; 2 if unknown).
    property real actualZoom: 2
    property real zoom: 1
    readonly property real maxZoom: Math.max(16, actualZoom * 4)
    readonly property bool zoomed: zoom > 1.001

    onKeyChanged: reset()
    onWidthChanged: clampPan()
    onHeightChanged: clampPan()
    function reset() { zoom = 1; stage.x = 0; stage.y = 0 }

    // Keep the picture covering the view (no empty edges when zoomed).
    function clampPan() {
        stage.x = Math.min(0, Math.max(width - width * zoom, stage.x))
        stage.y = Math.min(0, Math.max(height - height * zoom, stage.y))
    }
    // Zoom to `z` keeping the point (px, py) of the view where it is.
    function zoomAt(px, py, z) {
        const nz = Math.max(1, Math.min(maxZoom, z))
        if (nz === zoom) return
        stage.x = px - (px - stage.x) * nz / zoom
        stage.y = py - (py - stage.y) * nz / zoom
        zoom = nz
        if (zoom <= 1.001) reset()
        else clampPan()
    }
    function panBy(dx, dy) {
        stage.x += dx
        stage.y += dy
        clampPan()
    }

    Item {
        id: stage
        width: zv.width
        height: zv.height
        scale: zv.zoom
        transformOrigin: Item.TopLeft
    }

    // Trackpad pinch (native zoom gestures) and touch pinch.
    PinchHandler {
        target: null
        property real last: 1
        onActiveChanged: last = 1
        onActiveScaleChanged: {
            const f = activeScale / last
            last = activeScale
            zv.zoomAt(centroid.position.x, centroid.position.y, zv.zoom * f)
        }
    }
    // Ctrl+wheel: zoom around the pointer.
    WheelHandler {
        acceptedModifiers: Qt.ControlModifier
        acceptedDevices: PointerDevice.AllDevices
        onWheel: (e) => {
            const d = e.angleDelta.y !== 0 ? e.angleDelta.y / 120 : e.pixelDelta.y / 50
            zv.zoomAt(point.position.x, point.position.y, zv.zoom * Math.pow(1.2, d))
        }
    }
    // Zoomed in: scrolling pans (both directions; Shift: sideways).
    WheelHandler {
        enabled: zv.zoomed
        acceptedModifiers: Qt.NoModifier | Qt.ShiftModifier
        acceptedDevices: PointerDevice.AllDevices
        onWheel: (e) => {
            let dx = e.pixelDelta.x !== 0 || e.pixelDelta.y !== 0 ? e.pixelDelta.x : e.angleDelta.x / 2
            let dy = e.pixelDelta.x !== 0 || e.pixelDelta.y !== 0 ? e.pixelDelta.y : e.angleDelta.y / 2
            if (e.modifiers & Qt.ShiftModifier) { dx = dy; dy = 0 }
            zv.panBy(dx, dy)
        }
    }
    WheelHandler {
        enabled: zv.zoomed
        orientation: Qt.Horizontal
        acceptedDevices: PointerDevice.AllDevices
        onWheel: (e) => zv.panBy(e.pixelDelta.x !== 0 ? e.pixelDelta.x : e.angleDelta.x / 2, 0)
    }
    // Dragging pans once zoomed.
    DragHandler {
        target: null
        enabled: zv.zoomed
        property real lx: 0
        property real ly: 0
        onActiveChanged: { lx = 0; ly = 0 }
        onTranslationChanged: {
            zv.panBy(translation.x - lx, translation.y - ly)
            lx = translation.x
            ly = translation.y
        }
        cursorShape: active ? Qt.ClosedHandCursor : Qt.OpenHandCursor
    }
    TapHandler {
        acceptedButtons: Qt.LeftButton
        onDoubleTapped: (ev) => {
            if (zv.zoomed) zv.reset()
            else zv.zoomAt(ev.position.x, ev.position.y, zv.actualZoom > 1.05 ? zv.actualZoom : 2)
        }
    }
    // The zoom, briefly, while it changes.
    Rectangle {
        id: badge
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 8
        radius: 6
        color: Qt.rgba(0, 0, 0, 0.55)
        width: badgeText.implicitWidth + 14
        height: badgeText.implicitHeight + 6
        opacity: 0
        GLabel { id: badgeText; anchors.centerIn: parent; color: "white"; font.pixelSize: Theme.small; text: Math.round(zv.zoom / Math.max(0.01, zv.actualZoom) * 100) + "%" }
        Behavior on opacity { NumberAnimation { duration: 250 } }
    }
    Timer { id: badgeTimer; interval: 900; onTriggered: badge.opacity = 0 }
    onZoomChanged: { badge.opacity = 1; badgeTimer.restart() }
}
