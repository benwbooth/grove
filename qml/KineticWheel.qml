import QtQuick

// Momentum scrolling for any Flickable/ListView/ScrollView: put one inside
// the scrollable's parent (or the ScrollView) with `flickable` set. It takes
// the wheel from the list (whose own wheel scrolling is turned off) and
// scrolls with grove's glide (Kinetic), like the file views.
Item {
    id: kw
    property Flickable flickable
    property bool horizontal: false
    property real notch: 60
    anchors.fill: parent
    z: -1
    Kinetic { id: k; target: kw.flickable; horizontal: kw.horizontal; notch: kw.notch }
    WheelHandler {
        acceptedDevices: PointerDevice.AllDevices
        acceptedModifiers: Qt.NoModifier
        onWheel: (e) => k.wheel(e)
    }
    onFlickableChanged: if (flickable) flickable.interactive = false
}
