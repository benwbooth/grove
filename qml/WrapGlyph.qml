import QtQuick

// The "wrap long lines" symbol (no icon theme grove uses has a good one):
// a full line, a line that turns back down into a hooked arrow, and a short
// line it lands on. Drawn crisp at the screen's pixel ratio.
Canvas {
    id: g
    property color color: Theme.fg
    property real size: 16
    width: size; height: size
    renderTarget: Canvas.Image
    onColorChanged: requestPaint()
    onSizeChanged: requestPaint()
    Component.onCompleted: requestPaint()
    onPaint: {
        const c = getContext("2d")
        c.reset()
        const s = size / 16
        c.scale(s, s)
        c.strokeStyle = g.color
        c.fillStyle = g.color
        c.lineWidth = 1.5
        c.lineCap = "round"
        c.lineJoin = "round"
        // top line
        c.beginPath(); c.moveTo(2, 3.5); c.lineTo(14, 3.5); c.stroke()
        // middle line turning back down
        c.beginPath()
        c.moveTo(2, 8); c.lineTo(11.5, 8)
        c.arc(11.5, 10.25, 2.25, -Math.PI / 2, Math.PI / 2, false)
        c.lineTo(8, 12.5)
        c.stroke()
        // arrow head pointing left
        c.beginPath(); c.moveTo(9.6, 10.6); c.lineTo(7.8, 12.5); c.lineTo(9.6, 14.4); c.stroke()
        // short bottom line
        c.beginPath(); c.moveTo(2, 12.5); c.lineTo(5, 12.5); c.stroke()
    }
}
