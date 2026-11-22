import QtQuick
import QtQuick.Controls.Basic
import QtMultimedia

// The whole file, readable in place (the preview column, the information
// panel, the gallery): text and code with every line, Markdown rendered,
// tables, every page of PDFs and books, a player for music and video,
// pictures at full size. Same viewers as Quick Look (DocView, DataTable,
// PdfPages); what has none shows its thumbnail. Files inside archives are
// viewed through their local copy.
Item {
    id: fv
    property var app
    property var item: null
    property bool active: true
    property var pv: ({})
    property string path: ""          // local path being shown
    property bool converting: false
    // Whether there's more to show than a thumbnail.
    readonly property bool rich: ["code", "markdown", "office", "table", "pdf", "media", "image", "other"].indexOf(pv.kind) >= 0 || !!pv.convert
    readonly property string thumbSrc: item && item.thumb && path ? "image://thumb/" + encodeURIComponent(path) + "?m=" + (item.mtime || 0) : ""

    onItemChanged: Qt.callLater(load)
    onActiveChanged: if (!active) player.stop()
    // The previous file stays on screen until the next one's details are in
    // (clearing first made every move flash an icon, then a thumbnail).
    function load() {
        player.stop()
        converting = false
        if (!active || !item || !item.path || item.isDir) { pv = {}; path = ""; return }
        const want = item.path
        if (want.indexOf("archive://") === 0) {
            Async.call(app.grove, k => app.grove.archiveLocal(k, want), r => {
                if (!fv.item || fv.item.path !== want || !r.path) return
                fv.show(r.path, want)
            })
        } else if (want.indexOf("/") === 0) show(want, want)
    }
    function show(local, want) {
        Async.call(app.grove, k => app.grove.preview(k, local, Theme.dark), r => {
            if (!fv.item || fv.item.path !== want) return
            fv.path = local
            fv.pv = r
            if (r.convert) {
                fv.converting = true
                Async.call(app.grove, k => app.grove.docPdf(k, local), d => {
                    if (!fv.item || fv.item.path !== want) return
                    fv.converting = false
                    if (d.kind === "pdf" && d.paged) fv.pv = Object.assign({}, d, { kindText: r.kindText })
                })
            }
        })
    }

    // Pictures (and anything without a viewer of its own): the thumbnail.
    SwapImage {
        id: thumb
        anchors.fill: parent
        visible: !fv.rich || fv.pv.kind === "media" && !fv.pv.video || !!fv.pv.convert
        sourceSize: Qt.size(Math.ceil(Math.min(width, 1024) * Screen.devicePixelRatio), Math.ceil(Math.min(height, 1024) * Screen.devicePixelRatio))
        source: visible && fv.pv.kind !== "image" ? fv.thumbSrc : ""
        anchors.bottomMargin: fv.pv.kind === "media" ? (fv.pv.video ? 40 : 40 + Math.min(56, fv.height * 0.22) + 8) : 0
    }
    GIcon {
        anchors.centerIn: parent
        size: Math.min(parent.width, parent.height, 256)
        name: fv.item ? (fv.item.icon || fv.item.iconName || "") : ""
        visible: thumb.visible && thumb.status !== Image.Ready && thumb.status !== Image.Loading && fv.pv.kind !== "image"
    }
    PreviewSpinner { image: thumb; size: 18; anchors.centerIn: parent }
    Row {
        visible: fv.converting
        anchors.bottom: parent.bottom
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: 6
        PreviewSpinner { pending: parent.visible; delay: 0; size: 12; backdrop: false; anchors.verticalCenter: parent.verticalCenter }
        GLabel { text: "Getting every page ready…"; color: Theme.fgSecondary; font.pixelSize: Theme.small }
    }

    SwapImage {
        id: pic
        anchors.fill: parent
        visible: fv.pv.kind === "image"
        source: visible ? "file://" + encodeURI(fv.pv.src || fv.path).replace(/#/g, "%23") : ""
        sourceSize: Qt.size(Math.ceil(Math.min(width, 2048) * Screen.devicePixelRatio), 0)
    }
    PreviewSpinner { image: pic; size: 18; anchors.centerIn: parent }

    DocView {
        anchors.fill: parent
        visible: fv.pv.kind === "code" || fv.pv.kind === "markdown" || (fv.pv.kind === "office" && !!fv.pv.hasText)
        active: visible && fv.active
        path: visible ? fv.path : ""
        mode: fv.pv.kind === "markdown" ? "markdown" : fv.pv.kind === "office" ? "office" : "code"
    }
    HexView {
        anchors.fill: parent
        visible: fv.pv.kind === "other"
        active: visible && fv.active
        path: visible ? fv.path : ""
    }
    DataTable {
        anchors.fill: parent
        visible: fv.pv.kind === "table"
        active: visible && fv.active
        path: visible ? fv.path : ""
        app: fv.app
    }
    Loader {
        anchors.fill: parent
        active: fv.pv.kind === "pdf" && !!fv.pv.paged && fv.active
        sourceComponent: PdfPages { src: fv.pv.src || ""; pages: fv.pv.pages || 0; sizes: fv.pv.sizes || []; mtime: fv.item ? fv.item.mtime || 0 : 0 }
    }

    // Music and video: play on request (browsing doesn't start sound).
    MediaPlayer {
        id: player
        source: (fv.pv.kind === "media" && fv.active) || fsWin.visible ? "file://" + encodeURI(fv.path).replace(/#/g, "%23") : ""
        videoOutput: video
        audioOutput: AudioOutput {}
        // A video shows its first frame.
        onSourceChanged: if (source != "" && fv.pv.video) { play(); pause() }
    }
    VideoOutput { id: video; anchors.fill: parent; anchors.bottomMargin: 40; visible: fv.pv.kind === "media" && !!fv.pv.video }
    // Music: the waveform with a moving playhead, above the controls.
    Waveform {
        visible: fv.pv.kind === "media" && !fv.pv.video
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 40
        width: parent.width
        height: Math.min(56, parent.height * 0.22)
        app: fv.app
        player: player
        duration: Math.max(player.duration, 1000 * (fv.pv.seconds || 0))
        path: visible ? fv.path : ""
    }
    Row {
        visible: fv.pv.kind === "media"
        anchors.bottom: parent.bottom
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: 6
        height: 34
        GButton { iconName: player.playbackState === MediaPlayer.PlayingState ? "media-playback-pause" : "media-playback-start"; tip: "Play / pause"
            anchors.verticalCenter: parent.verticalCenter
            onClicked: player.playbackState === MediaPlayer.PlayingState ? player.pause() : player.play() }
        Slider {
            width: Math.max(60, fv.width - 140)
            anchors.verticalCenter: parent.verticalCenter
            from: 0; to: Math.max(1, player.duration, 1000 * (fv.pv.seconds || 0))
            value: player.position
            onMoved: player.position = value
        }
        GLabel {
            anchors.verticalCenter: parent.verticalCenter
            text: fmt(player.position) + " / " + fmt(Math.max(player.duration, 1000 * (fv.pv.seconds || 0)))
            color: Theme.fgSecondary
            font.pixelSize: Theme.small
            function fmt(ms) { const s = Math.floor(ms / 1000); return Math.floor(s / 60) + ":" + ("0" + (s % 60)).slice(-2) }
        }
        GButton { visible: !!fv.pv.video; iconName: "view-fullscreen"; tip: "Full screen (double-click the video)"; anchors.verticalCenter: parent.verticalCenter; onClicked: fv.fullScreen() }
    }
    VideoFullScreen { id: fsWin }
    function fullScreen() { fsWin.open(player, video); if (player.playbackState !== MediaPlayer.PlayingState) player.play() }
    MouseArea {
        anchors.fill: video
        visible: video.visible
        onDoubleClicked: fv.fullScreen()
        onClicked: player.playbackState === MediaPlayer.PlayingState ? player.pause() : player.play()
    }
}
