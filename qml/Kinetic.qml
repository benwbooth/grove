import QtQuick

// Scrolling with momentum for a Flickable whose own dragging is off
// (grove's views scroll through WheelHandlers so clicks and rubber bands
// stay theirs). Call wheel(event) from a WheelHandler. The physics are
// Kog's (KineticWheelHandler), i.e. Chrome's on Linux:
//  - Touchpad (pixel deltas): the content follows the fingers 1:1; the
//    release speed is a smoothed average of the last moves (so the jitter
//    of lifting the fingers can't stutter or reverse it), then it glides
//    with constant deceleration. Momentum the system already sends
//    (ScrollMomentum phase) is followed as is, with no second glide.
//  - Mouse wheel (angle deltas): each notch eases to a destination over
//    0.1-0.2 s; a notch mid-glide extends the destination and keeps the
//    speed, a reversal starts from where the content is.
// stop() halts any glide (a click on the view should call it).
QtObject {
    id: k
    property Flickable target
    property bool horizontal: false
    // Pixels one wheel notch moves (the view's row or cell size).
    property real notch: 60
    property bool enabled: Theme.reduceMotion !== true

    property real velocity: 0          // px/s, positive = towards the end
    property real maximumVelocity: 9000
    property real deceleration: 2300   // px/s² (Chrome's fling)
    property bool wheelAnimating: false
    property real wheelTarget: 0
    property real wheelStart: 0
    property real wheelStartVelocity: 0
    property real wheelElapsed: 0
    property real wheelDuration: 0.2
    property double lastPixelTime: 0
    property real pixelVelocity: 0

    function pos() { return horizontal ? target.contentX : target.contentY }
    function minPos() { return horizontal ? target.originX : target.originY }
    function maxPos() {
        return horizontal ? target.originX + Math.max(0, target.contentWidth - target.width)
                          : target.originY + Math.max(0, target.contentHeight - target.height)
    }
    function moveTo(p) {
        const c = Math.max(minPos(), Math.min(maxPos(), p))
        if (horizontal) target.contentX = c; else target.contentY = c
    }
    function stop() {
        velocity = 0; wheelAnimating = false; anim.stop(); release.stop()
        lastPixelTime = 0; pixelVelocity = 0
    }

    // The first few wheel events always go to the log (journal), so a
    // misdetected device shows up; GROVE_WHEEL_DEBUG=1 logs more.
    property int debugLeft: Theme.wheelDebug ? 40 : 4
    function wheel(e) {
        if (!target) return
        if (debugLeft > 0) {
            debugLeft--
            console.warn("grove wheel: device", e.device ? e.device.name : "?", "type", e.device ? e.device.type : -1,
                         "angle", e.angleDelta.x, e.angleDelta.y, "pixel", e.pixelDelta.x, e.pixelDelta.y,
                         "phase", e.phase, "end", e.isEndEvent === true)
        }
        const ang = horizontal ? e.angleDelta.x : e.angleDelta.y
        const pix = horizontal ? e.pixelDelta.x : e.pixelDelta.y
        const touchpad = pix !== 0 || (e.device && e.device.type === PointerDevice.TouchPad)
        if (touchpad) {
            // Fingers lifted (Qt's ScrollEnd, no amount): glide now.
            if (pix === 0 && ang === 0) {
                if (e.isEndEvent === true || e.phase === Qt.ScrollEnd) { release.stop(); finishPixels() }
                return
            }
            applyPixels(pix !== 0 ? pix : ang / 8, e.phase !== Qt.ScrollMomentum)
            if (e.isEndEvent === true || e.phase === Qt.ScrollEnd) { release.stop(); finishPixels() }
            return
        }
        // Some wheels arrive with only a pixel amount: 15 px = one notch.
        const steps = (ang !== 0 ? ang : pix * 8) / 120
        if (steps !== 0) start(steps)
    }

    function applyPixels(px, synthesize) {
        if (maxPos() <= minPos()) return
        const now = Date.now()
        const delta = -px
        const dt = lastPixelTime > 0 ? (now - lastPixelTime) / 1000 : 0
        anim.stop(); wheelAnimating = false; velocity = 0
        moveTo(pos() + delta)
        if (!synthesize || !enabled) { release.stop(); lastPixelTime = 0; pixelVelocity = 0; return }
        if (dt >= 0.004 && dt <= 0.08) {
            const inst = delta / dt
            pixelVelocity = pixelVelocity === 0 ? inst : pixelVelocity * 0.65 + inst * 0.35
        } else {
            pixelVelocity = delta * 60
        }
        pixelVelocity = Math.max(-maximumVelocity, Math.min(maximumVelocity, pixelVelocity))
        lastPixelTime = now
        release.restart()
    }
    function finishPixels() {
        lastPixelTime = 0
        velocity = pixelVelocity
        pixelVelocity = 0
        if (Math.abs(velocity) < 40) { stop(); return }
        anim.start()
    }

    function start(steps) {
        const delta = -steps * notch
        const cur = pos()
        if (!enabled) { stop(); moveTo(cur + delta); return }
        const pending = wheelAnimating ? wheelTarget - cur : 0
        const origin = pending * delta > 0 ? wheelTarget : cur
        wheelTarget = Math.max(minPos(), Math.min(maxPos(), origin + delta))
        release.stop(); lastPixelTime = 0; pixelVelocity = 0
        if (wheelTarget === cur) { stop(); return }
        const dist = Math.abs(wheelTarget - cur)
        wheelDuration = Math.max(6, Math.min(12, 14 - dist / 60)) / 60
        wheelStartVelocity = wheelAnimating && velocity * (wheelTarget - cur) > 0 ? velocity : 0
        wheelStart = cur
        wheelElapsed = 0
        wheelAnimating = true
        anim.start()
    }
    // From the start position and speed to the destination at rest.
    function curve(t) {
        const span = wheelTarget - wheelStart
        const v0 = wheelStartVelocity * wheelDuration
        const t2 = t * t, t3 = t2 * t
        return { position: wheelStart + v0 * (t3 - 2 * t2 + t) + span * (3 * t2 - 2 * t3),
                 velocity: (v0 * (3 * t2 - 4 * t + 1) + span * (6 * t - 6 * t2)) / wheelDuration }
    }
    function advance(frameTime) {
        if (!target) { stop(); return }
        if (wheelAnimating) {
            wheelElapsed += Math.max(0, frameTime)
            const t = Math.min(1, wheelElapsed / wheelDuration)
            const c = curve(t)
            moveTo(c.position)
            velocity = t === 1 ? 0 : c.velocity
            if (t === 1 || pos() === minPos() || pos() === maxPos()) stop()
            return
        }
        const dt = Math.min(0.05, frameTime)
        if (dt <= 0) return
        const next = Math.max(minPos(), Math.min(maxPos(), pos() + velocity * dt))
        const hit = next === pos() && ((velocity < 0 && next <= minPos()) || (velocity > 0 && next >= maxPos()))
        moveTo(next)
        const speed = Math.max(0, Math.abs(velocity) - deceleration * dt)
        velocity = Math.sign(velocity) * speed
        if (hit || speed < 1) stop()
    }

    // Fingers lifted without an end event (the events just stop): glide.
    property Timer release: Timer { interval: 45; onTriggered: k.finishPixels() }
    // In step with the frames (a plain timer stutters against vsync).
    property FrameAnimation anim: FrameAnimation { onTriggered: k.advance(frameTime) }
}
