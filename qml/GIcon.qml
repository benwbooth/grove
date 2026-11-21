import QtQuick

// A theme icon by freedesktop name ("folder", "a|fallback|fallback").
// tint: recolours the icon (Finder's accent-blue sidebar glyphs).
Image {
    property string name: ""
    property int size: 16
    property color tint: "transparent"
    width: size
    height: size
    sourceSize.width: Math.ceil(size * Screen.devicePixelRatio)
    sourceSize.height: Math.ceil(size * Screen.devicePixelRatio)
    source: Theme.iconUrl(name, tint)
    asynchronous: true
    cache: true
    smooth: true
    mipmap: false
    fillMode: Image.PreserveAspectFit
}
