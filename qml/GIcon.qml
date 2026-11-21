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
    readonly property real renderScale: Math.max(2, Math.ceil(Screen.devicePixelRatio * 1.5))
    sourceSize.width: Math.ceil(size * renderScale)
    sourceSize.height: Math.ceil(size * renderScale)
    source: Theme.iconUrl(name, tint)
    asynchronous: true
    cache: true
    smooth: true
    mipmap: true
    fillMode: Image.PreserveAspectFit
}
