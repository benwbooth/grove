import QtQuick

// An image that never shows a blank frame between pictures: the next one
// loads in a second Image behind the current one, and they swap only when
// it's ready (Image alone clears as soon as its source changes, so moving
// between pictures flashed empty, then the thumbnail, then the picture).
Item {
    id: si
    property url source: ""
    property int fillMode: Image.PreserveAspectFit
    property bool mipmap: true
    property bool autoTransform: true
    property size sourceSize: Qt.size(0, 0)
    property bool cache: true
    // Ready when the picture on screen is the one asked for.
    readonly property int status: front === 0 ? a.status : b.status
    readonly property bool pending: (front === 0 ? a.source : b.source) != source || status === Image.Loading
    property int front: 0

    onSourceChanged: {
        const back = front === 0 ? b : a
        if (source == "") { a.source = ""; b.source = ""; return }
        back.source = source
        if (back.status === Image.Ready) front = 1 - front
    }
    component Pic: Image {
        anchors.fill: parent
        asynchronous: true
        smooth: true
        fillMode: si.fillMode
        mipmap: si.mipmap
        autoTransform: si.autoTransform
        sourceSize: si.sourceSize
        cache: si.cache
    }
    Pic {
        id: a
        visible: si.front === 0
        onStatusChanged: if (status === Image.Ready && si.front !== 0 && source == si.source) si.front = 0
    }
    Pic {
        id: b
        visible: si.front === 1
        onStatusChanged: if (status === Image.Ready && si.front !== 1 && source == si.source) si.front = 1
    }
}
