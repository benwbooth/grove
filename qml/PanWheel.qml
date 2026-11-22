import QtQuick

// Momentum scrolling both ways for a Flickable whose own wheel is off:
// up/down, sideways (trackpad swipes, tilt wheels) and Shift+wheel for
// sideways. Put it next to the Flickable (it sits below it, so the
// Flickable passes the wheel on). WheelHandler takes only its own
// orientation, so purely sideways events need a horizontal handler.
Item {
    id: pw
    property Flickable flickable
    property bool sideways: true        // false: up/down only (wrapped text)
    property real notch: 60
    property real sideNotch: 80
    anchors.fill: parent
    z: -1
    Kinetic { id: kv; target: pw.flickable; notch: pw.notch }
    Kinetic { id: kh; target: pw.flickable; horizontal: true; notch: pw.sideNotch }
    function stop() { kv.stop(); kh.stop() }
    // Shift+wheel: the y amount scrolls sideways.
    function shifted(e) {
        return { device: e.device, isEndEvent: e.isEndEvent, phase: e.phase, modifiers: e.modifiers,
                 angleDelta: Qt.point(e.angleDelta.y, 0), pixelDelta: Qt.point(e.pixelDelta.y, 0) }
    }
    WheelHandler {
        acceptedDevices: PointerDevice.AllDevices
        acceptedModifiers: Qt.KeyboardModifierMask
        onWheel: (e) => {
            if (e.modifiers & Qt.ControlModifier) return
            if (e.modifiers & Qt.ShiftModifier) { if (pw.sideways) kh.wheel(pw.shifted(e)); return }
            kv.wheel(e)
            if (pw.sideways && (e.angleDelta.x !== 0 || e.pixelDelta.x !== 0)) kh.wheel(e)
        }
    }
    WheelHandler {
        acceptedDevices: PointerDevice.AllDevices
        acceptedModifiers: Qt.KeyboardModifierMask
        orientation: Qt.Horizontal
        onWheel: (e) => { if (pw.sideways && e.angleDelta.y === 0 && e.pixelDelta.y === 0) kh.wheel(e) }
    }
}
