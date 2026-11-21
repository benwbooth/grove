import QtQuick

// Scrolling with momentum for a Flickable whose own dragging is off
// (grove's views scroll through WheelHandlers so clicks and rubber bands
// stay theirs). Call wheel(event) from a WheelHandler.
//  - Touchpad (pixel deltas): the content follows the fingers 1:1; when
//    the events stop (fingers lifted) it keeps gliding at the release
//    speed and slows down like macOS.
//  - Mouse wheel (angle deltas): each notch adds speed, so one notch is a
//    short smooth glide and fast spinning builds up, as on a Mac.
// stop() halts any glide (a click on the view should call it).
QtObject {
    id: k
    property Flickable target
    property bool horizontal: false
    // Pixels one wheel notch moves (the view's row or cell size).
    property real notch: 60
    // Velocity decay per second (macOS: about 0.998 per ms = 0.135/s).
    property real friction: 0.135
    property bool enabled: Theme.reduceMotion !== true

    property real velocity: 0          // px/s, positive = towards the end
    property real lastTime: 0
    property var samples: []           // recent touchpad moves [t, d]

    function pos() { return horizontal ? target.contentX : target.contentY }
    function minPos() { return horizontal ? target.originX : target.originY }
    function maxPos() {
        return horizontal ? target.originX + Math.max(0, target.contentWidth - target.width)
                          : target.originY + Math.max(0, target.contentHeight - target.height)
    }
    // Moves by d px; returns false at an edge.
    function moveBy(d) {
        const p = Math.max(minPos(), Math.min(pos() + d, maxPos()))
        const moved = p !== pos()
        if (horizontal) target.contentX = p; else target.contentY = p
        return moved && p > minPos() && p < maxPos()
    }
    function stop() { velocity = 0; anim.running = false; samples = [] }

    function wheel(e) {
        if (!target) return
        const now = Date.now()
        const px = horizontal ? (e.pixelDelta.x || e.pixelDelta.y) : e.pixelDelta.y
        if (px !== 0) {
            // Touchpad: follow the fingers, remember the speed.
            anim.running = false
            moveBy(-px)
            samples = samples.filter(s => now - s[0] < 80).concat([[now, -px]])
            release.restart()
            return
        }
        const ang = horizontal ? (e.angleDelta.x || e.angleDelta.y) : e.angleDelta.y
        if (ang === 0) return
        const step = -ang / 120 * notch
        if (!enabled) { moveBy(step); return }
        // Same direction keeps building; a reversal starts over.
        if (velocity !== 0 && Math.sign(velocity) !== Math.sign(step)) velocity = 0
        // A notch's glide covers about one notch: distance = v * tau, tau = -1/ln(friction).
        velocity += step / k.tau()
        const cap = 12000
        velocity = Math.max(-cap, Math.min(cap, velocity))
        anim.running = true
    }
    function tau() { return -1 / Math.log(friction) }

    property Timer release: Timer {
        interval: 50
        onTriggered: {
            if (!k.enabled || k.samples.length < 2) { k.samples = []; return }
            const t0 = k.samples[0][0], t1 = k.samples[k.samples.length - 1][0]
            const dist = k.samples.reduce((a, s) => a + s[1], 0)
            const dt = Math.max(16, t1 - t0) / 1000
            k.samples = []
            const v = dist / dt
            if (Math.abs(v) < 120) return
            k.velocity = Math.max(-12000, Math.min(12000, v))
            k.anim.running = true
        }
    }
    property FrameAnimation anim: FrameAnimation {
        onTriggered: {
            const dt = Math.min(frameTime, 0.05)
            if (!k.moveBy(k.velocity * dt)) { k.stop(); return }
            k.velocity *= Math.pow(k.friction, dt)
            if (Math.abs(k.velocity) < 12) k.stop()
        }
    }
}
