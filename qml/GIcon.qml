import QtQuick

// A theme icon by freedesktop name ("folder", "a|fallback|fallback").
// tint: recolours the icon (Finder's accent-blue sidebar glyphs).
Image {
    property string name: ""
    property int size: 16
    property color tint: "transparent"
    width: size
    height: size
    // Rendered at least 2× and shrunk with mipmaps: on Wayland the window's
    // fractional scale (4/3) isn't what Screen.devicePixelRatio reports, and
    // shrinking without mipmaps dropped pixels from thin strokes.
    // File and folder pictures (full colour) only: monochrome line-art
    // glyphs (menu, toolbar, action icons) are drawn at exactly the pixels
    // they're shown at (Qt multiplies the size by the window's scale), so
    // no thin stroke is lost shrinking them (rows went missing at 4/3).
    readonly property bool fileArt: {
        const n = (name.split("|")[0] || "")
        if (n.endsWith("-symbolic") || n.endsWith("-new")) return false
        return /^(folder|inode-|text-|application-|image-|video-|audio-|font-|model-|package-|x-office|user-home|user-desktop|unknown)/.test(n) || n.indexOf("-x-") >= 0 || n.indexOf("/") === 0
    }
    readonly property real renderScale: fileArt ? Math.max(2, Math.ceil(Screen.devicePixelRatio * 1.5)) : 1
    sourceSize.width: crispPx > 0 ? crispPt : Math.ceil(size * renderScale)
    sourceSize.height: crispPx > 0 ? crispPt : Math.ceil(size * renderScale)

    // Pixel-exact (menus): at a fractional scale (4/3) a 16 pt icon is
    // 21.33 device pixels, so the 16 px symbol was stretched between pixels
    // and its strokes went soft. With `crisp` the icon is drawn at an icon
    // theme design size (16/22/24/32 px) whose size in points is whole (Qt
    // rounds sourceSize to whole points, then multiplies by the scale: 22 px
    // can't be asked for at 4/3), the one nearest `size`, and placed on the
    // device pixel grid: at 4/3, 24 px (18 pt). Call snap() when the icon
    // moves in the window without its own x/y changing (a menu opening).
    property bool crisp: false
    readonly property real dpr: {
        const w = Window.window
        return w && w.devicePixelRatio > 0 ? w.devicePixelRatio : (Screen.devicePixelRatio || 1)
    }
    readonly property int crispPx: {
        if (!crisp || fileArt) return 0
        const want = size * dpr
        let best = 0
        for (const d of [16, 22, 24, 32]) {
            const pt = d / dpr
            if (Math.abs(pt - Math.round(pt)) > 0.01) continue
            if (!best || Math.abs(d - want) < Math.abs(best - want)) best = d
        }
        return best
    }
    readonly property int crispPt: Math.round(crispPx / dpr)
    Binding on width { when: crispPx > 0; value: crispPx / dpr }
    Binding on height { when: crispPx > 0; value: crispPx / dpr }
    transform: Translate { id: gridSnap }
    function snap() {
        if (crispPx <= 0 || !visible) { gridSnap.x = 0; gridSnap.y = 0; return }
        const p = mapToItem(null, 0, 0)
        const rx = p.x - gridSnap.x, ry = p.y - gridSnap.y
        gridSnap.x = Math.round(rx * dpr) / dpr - rx
        gridSnap.y = Math.round(ry * dpr) / dpr - ry
    }
    onVisibleChanged: if (crisp) Qt.callLater(snap)
    onXChanged: if (crisp) Qt.callLater(snap)
    onYChanged: if (crisp) Qt.callLater(snap)
    onCrispPxChanged: Qt.callLater(snap)
    source: Theme.iconUrl(name, tint) + (crispPx > 0 && name !== "" ? "&exact=1" : "")
    asynchronous: true
    cache: true
    smooth: true
    mipmap: fileArt
    fillMode: Image.PreserveAspectFit
}
