import QtQuick

// A theme icon by freedesktop name ("folder", "a|fallback|fallback").
Image {
    property string name: ""
    property int size: 16
    width: size
    height: size
    sourceSize.width: Math.ceil(size * Screen.devicePixelRatio)
    sourceSize.height: Math.ceil(size * Screen.devicePixelRatio)
    source: name ? "image://icon/" + name : ""
    asynchronous: true
    cache: true
    smooth: true
    mipmap: false
    fillMode: Image.PreserveAspectFit
}
