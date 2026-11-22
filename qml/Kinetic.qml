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

    // The first few wheel events always go to the log (journal), so a
    // misdetected device shows up; GROVE_WHEEL_DEBUG=1 logs more.
    property int debugLeft: Theme.wheelDebug ? 40 : 4
    function wheel(e) {
        if (!target) return
        const now = Date.now()
        const dev = e.device
        const devType = dev ? dev.type : -1
        if (debugLeft > 0) {
            debugLeft--
            console.warn("grove wheel: device", dev ? dev.name : "?", "type", devType,
                         "angle", e.angleDelta.x, e.angleDelta.y, "pixel", e.pixelDelta.x, e.pixelDelta.y,
                         "end", e.isEndEvent === true)
        }
        const ang0 = horizontal ? (e.angleDelta.x || e.angleDelta.y) : e.angleDelta.y
        const pix0 = horizontal ? (e.pixelDelta.x || e.pixelDelta.y) : e.pixelDelta.y
        // A touchpad (or other smooth scroller) follows the fingers; a mouse
        // wheel moves by notches. Qt on Wayland gives wheel notches pixel
        // deltas too, so go by the device, then by whole-notch angles.
        const isTouchpad = devType === PointerDevice.TouchPad
            || (devType !== PointerDevice.Mouse && ang0 % 120 !== 0)
        if (isTouchpad) {
            const px = pix0 || ang0 / 8
            if (px !== 0) {
                // Follow the fingers; remember the speed for the glide.
                anim.running = false
                moveBy(-px)
                samples = samples.filter(s => now - s[0] < 80).concat([[now, -px]])
            }
            // Fingers lifted (or the events stop): glide.
            if (e.isEndEvent === true) { release.stop(); fling() } else release.restart()
            return
        }
        if (ang0 === 0) return
        const step = -ang0 / 120 * notch
        if (!enabled) { moveBy(step); return }
        // Part of the notch moves at once (so a wheel always responds), the
        // rest glides and slows down; spinning faster builds speed.
        moveBy(step * 0.35)
        if (velocity !== 0 && Math.sign(velocity) !== Math.sign(step)) velocity = 0
        velocity += step * 0.65 / k.tau()
        velocity = Math.max(-12000, Math.min(12000, velocity))
        lastTick = Date.now()
        anim.running = true
    }
    function tau() { return -1 / Math.log(friction) }

    property Timer release: Timer { interval: 50; onTriggered: k.fling() }
    function fling() {
        if (!enabled || samples.length < 2) { samples = []; return }
        const t0 = samples[0][0], t1 = samples[samples.length - 1][0]
        const dist = samples.reduce((a, s) => a + s[1], 0)
        const dt = Math.max(16, t1 - t0) / 1000
        samples = []
        const v = dist / dt
        if (Math.abs(v) < 120) return
        velocity = Math.max(-12000, Math.min(12000, v))
        lastTick = Date.now()
        anim.running = true
    }
    // A plain timer, not FrameAnimation: it ticks wherever the window lives.
    property real lastTick: 0
    property Timer anim: Timer {
        interval: 8
        repeat: true
        onTriggered: {
            const now = Date.now()
            const dt = Math.min((now - k.lastTick) / 1000, 0.05)
            k.lastTick = now
            if (!k.moveBy(k.velocity * dt)) { k.stop(); return }
            k.velocity *= Math.pow(k.friction, dt)
            if (Math.abs(k.velocity) < 12) k.stop()
        }
    }
}
