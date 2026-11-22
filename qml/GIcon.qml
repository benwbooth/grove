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
    sourceSize.width: Math.ceil(size * renderScale)
    sourceSize.height: Math.ceil(size * renderScale)
    source: Theme.iconUrl(name, tint)
    asynchronous: true
    cache: true
    smooth: true
    mipmap: fileArt
    fillMode: Image.PreserveAspectFit
}
