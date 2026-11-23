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
    // Small panes: viewers with light toolbars (DocView.slim).
    property bool slim: false
    property var pv: ({})
    property string path: ""          // local path being shown
    property bool converting: false
    // Binary files: a card; their bytes or text only on request.
    property bool hexAsked: false
    property bool textAsked: false
    onPvChanged: { hexAsked = false; textAsked = false; if (pendingView) Qt.callLater(applyPending) }
    // Whether there's more to show than a thumbnail.
    readonly property bool rich: ["code", "markdown", "html", "office", "table", "pdf", "media", "image"].indexOf(pv.kind) >= 0 || hexAsked || textAsked || !!pv.convert
    readonly property string thumbSrc: item && item.thumb && path ? "image://thumb/" + encodeURIComponent(path) + "?m=" + (item.mtime || 0) : ""

    onItemChanged: Qt.callLater(load)
    // The session: the file shown and where in it (scroll, page, playback).
    property var pendingView: null
    property real pendingPos: -1
    function viewState() {
        if (!item || !item.path) return null
        const s = { path: item.path }
        if (docView.visible) s.doc = docView.viewState()
        if (pdfLoader.item && pdfLoader.item.viewState) s.pdf = pdfLoader.item.viewState()
        if (pv.kind === "media" && player.position > 0) s.pos = player.position
        return s
    }
    function restoreView(s) { pendingView = s; Qt.callLater(applyPending) }
    function applyPending() {
        const s = pendingView
        if (!s || !item || item.path !== s.path || !pv.kind) return
        if (s.doc && docView.visible) docView.restoreView(s.doc)
        if (s.pdf && pdfLoader.item && pdfLoader.item.restoreView) pdfLoader.item.restoreView(s.pdf)
        if (s.pos && pv.kind === "media") pendingPos = s.pos
        pendingView = null
    }
    // Shown again (a panel moved or reopened): load what's selected now.
    onActiveChanged: { if (!active) player.stop(); Qt.callLater(load) }
    // The previous file stays on screen until the next one's details are in
    // (clearing first made every move flash an icon, then a thumbnail).
    property var life: ({ alive: true })
    Component.onDestruction: life.alive = false
    // The same file again (a new copy of its details): keep playing.
    property string loadedKey: ""
    function load() {
        const life = fv.life
        const key = active && item && item.path ? item.path + "|" + (item.mtime || 0) + "|" + (item.size || 0) : ""
        if (key !== "" && key === loadedKey) return
        loadedKey = key
        player.stop()
        gdoc = null; gdocError = ""; gdocLoading = false
        converting = false
        if (!active || !item || !item.path || item.isDir) { pv = {}; path = ""; return }
        const want = item.path
        if (want.indexOf("archive://") === 0) {
            Async.call(app.grove, k => app.grove.archiveLocal(k, want), r => {
                if (!life.alive || !fv.item || fv.item.path !== want || !r.path) return
                fv.show(r.path, want)
            })
        } else if (want.indexOf("/") === 0) show(want, want)
    }
    // A Google Docs/Sheets/Slides link file: its link, and why its
    // content can't be shown (no Google Drive connected...).
    property var gdoc: null
    // Signed in to Google meanwhile: try this document again.
    Connections {
        target: fv.app
        function onGoogleSignInsChanged() { if (fv.gdoc && fv.gdocError !== "") { fv.loadedKey = ""; fv.load() } }
    }
    property string gdocError: ""
    // With hearth's accounts service: why it couldn't sign in ({reason:
    // signedout|notallowed|none, account}), for its buttons.
    property var gdocSignIn: null
    property bool gdocLoading: false
    function show(local, want) {
        const life = fv.life
        Async.call(app.grove, k => app.grove.preview(k, local, Theme.dark), r => {
            if (!life.alive || !fv.item || fv.item.path !== want) return
            if (r.kind === "gdoc") {
                fv.gdoc = r
                fv.gdocError = ""
                fv.gdocSignIn = null
                fv.path = local
                fv.pv = r
                if (r.canExport) {
                    fv.gdocLoading = true
                    Async.call(app.grove, k => app.grove.gdocExport(k, local), e => {
                        if (!life.alive || !fv.item || fv.item.path !== want) return
                        fv.gdocLoading = false
                        if (e.path) fv.showExported(e.path, want)
                        else { fv.gdocError = e.error || ""; fv.gdocSignIn = e.service ? e : null }
                    })
                }
                return
            }
            if (local === want) fv.gdoc = null
            fv.path = local
            fv.pv = r
            if (r.convert) {
                fv.converting = true
                Async.call(app.grove, k => app.grove.docPdf(k, local), d => {
                    if (!life.alive || !fv.item || fv.item.path !== want) return
                    fv.converting = false
                    if (d.kind === "pdf" && d.paged) fv.pv = Object.assign({}, d, { kindText: r.kindText })
                })
            }
        })
    }

    // The exported copy of a Google document, in its own viewer.
    function showExported(local, want) {
        const life = fv.life
        Async.call(app.grove, k => app.grove.preview(k, local, Theme.dark), r => {
            if (!life.alive || !fv.item || fv.item.path !== want) return
            fv.path = local
            fv.pv = r
        })
    }
    // Google link files: what it is, and buttons to open it in Google or
    // connect a Google Drive (whose exports show the content right here).
    Column {
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 12
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: 8
        z: 3
        visible: !!fv.gdoc && fv.pv.kind === "gdoc"
        width: Math.min(parent.width - 24, 420)
        GLabel {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            color: Theme.fgSecondary
            font.pixelSize: Theme.small
            text: fv.gdocLoading ? "Getting it from Google Drive…" : fv.gdocError !== "" ? fv.gdocError : fv.gdoc ? (fv.gdoc.app + " — it lives online" + (fv.gdoc.email ? " (" + fv.gdoc.email + ")" : "")) : ""
        }
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 6
            PushButton { text: fv.gdoc ? "Open in " + fv.gdoc.app : ""; tip: "Open it in your web browser"; onClicked: fv.app.grove.openUrl(fv.gdoc.url) }
            PushButton {
                visible: !!fv.gdocSignIn && (fv.gdocSignIn.reason === "signedout" || fv.gdocSignIn.reason === "notallowed")
                text: "Sign In Again"
                tip: "Sign in to this Google account again in the browser (and allow reading your files): the document then shows here"
                onClicked: if (fv.app.activeWindow) fv.app.activeWindow.accountSignInAgain(fv.gdocSignIn.account)
            }
            PushButton {
                visible: !!fv.gdocSignIn && fv.gdocSignIn.reason !== "failed"
                text: "Open Settings"
                tip: "Your accounts are in hearth's Settings › Accounts, for all your apps"
                onClicked: if (fv.app.activeWindow) fv.app.activeWindow.openAccounts(fv.gdocSignIn.account || "setup:google", () => fv.app.activeWindow.googleSheetStart())
            }
            PushButton {
                visible: !fv.gdocSignIn && fv.gdocError.indexOf("Sign in") === 0
                text: "Sign in with Google…"
                tip: "Sign in to your Google account in the browser: Google documents then show here"
                onClicked: if (fv.app.activeWindow) fv.app.activeWindow.googleSignIn()
            }
        }
    }
    // Over an exported Google document: back to the real thing.
    PushButton {
        z: 3
        visible: !!fv.gdoc && fv.pv.kind !== "gdoc" && !!fv.pv.kind
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 10
        text: fv.gdoc ? "Open in " + fv.gdoc.app : ""
        tip: "This is a copy exported from Google Drive; open the document itself in your browser"
        onClicked: fv.app.grove.openUrl(fv.gdoc.url)
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
        // (Room under it for a Google link file's buttons.)
        readonly property real below: fv.pv.kind === "gdoc" ? 110 : 0
        anchors.centerIn: parent
        anchors.verticalCenterOffset: -below / 2
        size: Math.max(32, Math.min(parent.width, parent.height - below - 16, 256))
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

    // Tests: zoom the picture by `z` around its middle.
    function testZoom(z) { picZoom.zoomAt(picZoom.width / 2, picZoom.height / 2, z) }
    // Pictures zoom (pinch, Ctrl+wheel, double-click) and pan once zoomed;
    // decoded bigger as the zoom grows, so they stay sharp.
    ZoomView {
        id: picZoom
        anchors.fill: parent
        visible: fv.pv.kind === "image"
        key: visible ? fv.path : ""
        actualZoom: pic.paintedWidth > 0 && fv.pv.width > 0 ? fv.pv.width / pic.paintedWidth : 2
        SwapImage {
            id: pic
            anchors.fill: parent
            source: picZoom.visible ? "file://" + encodeURI(fv.pv.src || fv.path).replace(/#/g, "%23") : ""
            readonly property real wantZoom: picZoom.zoom > 1.5 ? Math.min(4, Math.pow(2, Math.ceil(Math.log(picZoom.zoom) / Math.LN2))) : 1
            sourceSize: Qt.size(Math.ceil(Math.min(width * wantZoom, 8192) * Screen.devicePixelRatio), 0)
        }
    }
    PreviewSpinner { image: pic; size: 18; anchors.centerIn: parent }

    DocView {
        id: docView
        anchors.fill: parent
        visible: fv.pv.kind === "code" || fv.pv.kind === "markdown" || fv.pv.kind === "html" || (fv.pv.kind === "office" && !!fv.pv.hasText) || (fv.pv.kind === "other" && fv.textAsked)
        active: visible && fv.active
        slim: fv.slim
        path: visible ? fv.path : ""
        mode: fv.pv.kind === "markdown" ? "markdown" : fv.pv.kind === "html" ? "html" : fv.pv.kind === "office" ? "office" : "code"
    }
    Row {
        visible: fv.pv.kind === "other" && !fv.hexAsked && !fv.textAsked && !!fv.item && !fv.item.isDir
        anchors.bottom: parent.bottom
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: 6
        z: 2
        PushButton { text: "View as Text"; tip: "Read it as text (logs and data files grove didn't recognise)"; onClicked: fv.textAsked = true }
        PushButton { text: "View as Hex"; tip: "Show the file's bytes in hex"; onClicked: fv.hexAsked = true }
    }
    HexView {
        anchors.fill: parent
        visible: fv.pv.kind === "other" && fv.hexAsked
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
        id: pdfLoader
        anchors.fill: parent
        active: fv.pv.kind === "pdf" && !!fv.pv.paged && fv.active
        onLoaded: if (fv.pendingView) Qt.callLater(fv.applyPending)
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
        // Back where it was (the session), paused.
        onMediaStatusChanged: if (fv.pendingPos >= 0 && (mediaStatus === MediaPlayer.LoadedMedia || mediaStatus === MediaPlayer.BufferedMedia)) { position = fv.pendingPos; fv.pendingPos = -1 }
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
