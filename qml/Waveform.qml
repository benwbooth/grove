import QtQuick
import QtMultimedia

// A track's waveform under the player: grey for the whole track, the
// accent colour for what has played, and a vertical line at the playhead
// that moves while it plays. Click or drag on it to jump there.
Item {
    id: wf
    property var app
    property string path: ""
    property var player: null
    property real duration: player ? player.duration : 0
    readonly property real fraction: duration > 0 && player ? Math.max(0, Math.min(1, player.position / duration)) : 0
    property string base: ""
    property string played: ""
    implicitHeight: 64

    function load() {
        base = ""; played = ""
        if (!path || !app) return
        const p = path, w = Math.round(Math.max(400, width) * Screen.devicePixelRatio), h = Math.round(Math.max(32, height) * Screen.devicePixelRatio)
        Async.call(app.grove, k => app.grove.audioWaveform(k, p, Math.min(w, 2400), Math.min(h, 240), Theme.dark), r => {
            if (wf.path !== p || r.error) return
            wf.base = "file://" + encodeURI(r.base).replace(/#/g, "%23")
            wf.played = "file://" + encodeURI(r.played).replace(/#/g, "%23")
        })
    }
    onPathChanged: Qt.callLater(load)
    Connections { target: Theme; function onDarkChanged() { wf.load() } }

    // Until it's drawn: a thin track line.
    Rectangle { anchors.verticalCenter: parent.verticalCenter; width: parent.width; height: 2; radius: 1; color: Theme.separator; visible: wf.base === "" }
    Image { id: baseImg; anchors.fill: parent; source: wf.base; fillMode: Image.Stretch; smooth: true; asynchronous: true; cache: false }
    Item {
        width: parent.width * wf.fraction
        height: parent.height
        clip: true
        Image { width: wf.width; height: wf.height; source: wf.played; fillMode: Image.Stretch; smooth: true; asynchronous: true; cache: false }
    }
    // The playhead.
    Rectangle {
        visible: wf.duration > 0
        x: Math.round(parent.width * wf.fraction) - 1
        width: 2
        height: parent.height
        radius: 1
        color: Theme.accent
        Rectangle { anchors.horizontalCenter: parent.horizontalCenter; y: -3; width: 8; height: 8; radius: 4; color: Theme.accent }
    }
    MouseArea {
        anchors.fill: parent
        anchors.topMargin: -4
        cursorShape: Qt.PointingHandCursor
        function seek(x) { if (wf.player && wf.duration > 0) wf.player.position = Math.max(0, Math.min(1, x / width)) * wf.duration }
        onPressed: (m) => seek(m.x)
        onPositionChanged: (m) => { if (pressed) seek(m.x) }
    }
}
