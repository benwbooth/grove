import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window
import QtMultimedia
import QtQuick.Pdf

// Quick Look (Space): a floating preview of the selected file — pictures,
// video and audio, PDF, code with syntax highlighting, Markdown, archive
// contents, folder summaries. Arrow keys move to the next file while it's
// open; Space or Esc closes it.
Window {
    id: ql
    property var app
    property var win
    property var pane
    property var item: null
    property var pv: ({})
    width: 900
    height: 640
    minimumWidth: 320
    minimumHeight: 240
    color: Theme.glassMenu
    flags: Qt.Window | Qt.FramelessWindowHint
    title: item ? item.name + " — Quick Look" : "Quick Look"
    transientParent: win

    function showItem(it) {
        item = it
        pv = { kind: "loading" }
        player.stop()
        const p = it.path
        if (p.indexOf("/") !== 0) { pv = { kind: "other" }; return }
        Async.call(app.grove, k => app.grove.preview(k, p, Theme.dark), r => { if (ql.item && ql.item.path === p) ql.pv = r })
        if (!visible) { show(); raise(); requestActivate() }
    }
    onVisibleChanged: if (!visible) { player.stop(); if (pane) pane.forceActiveFocus() }

    Item {
        anchors.fill: parent
        focus: true
        transform: Translate { x: 0.001; y: 0.001 }
        Keys.onPressed: (e) => {
            if (e.key === Qt.Key_Space || e.key === Qt.Key_Escape) { ql.close(); e.accepted = true }
            else if (e.key === Qt.Key_Right || e.key === Qt.Key_Down) { ql.pane.step(1, false); e.accepted = true }
            else if (e.key === Qt.Key_Left || e.key === Qt.Key_Up) { ql.pane.step(-1, false); e.accepted = true }
            else if (e.key === Qt.Key_Return) { ql.pane.openSelected(false); ql.close(); e.accepted = true }
        }

        Rectangle { anchors.fill: parent; color: "transparent"; border.color: Theme.glassEdge; border.width: 1; radius: 0 }

        // Title bar.
        Item {
            id: bar
            width: parent.width
            height: Math.round(40 * Theme.pt)
            MouseArea { anchors.fill: parent; onPressed: ql.startSystemMove() }
            GButton { x: 10; anchors.verticalCenter: parent.verticalCenter; iconName: "window-close"; iconSize: 12; tip: "Close (Space or Esc)"; onClicked: ql.close() }
            GButton {
                x: 44; anchors.verticalCenter: parent.verticalCenter
                iconName: ql.visibility === Window.FullScreen ? "view-restore" : "view-fullscreen"; iconSize: 14
                tip: "Full screen"
                onClicked: ql.visibility === Window.FullScreen ? ql.showNormal() : ql.showFullScreen()
            }
            GLabel {
                anchors.centerIn: parent
                width: parent.width - 260
                horizontalAlignment: Text.AlignHCenter
                text: ql.item ? ql.item.name : ""
                font.bold: true
            }
            Row {
                anchors.right: parent.right; anchors.rightMargin: 10
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                GButton {
                    text: ql.pv.kind === "folder" ? "Open" : "Open with " + (ql.pv.app || "default app")
                    flat: false; capsule: true
                    tip: "Open it (Return)"
                    onClicked: { ql.pane.openSelected(false); ql.close() }
                }
            }
        }

        Item {
            id: body
            anchors.top: bar.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: foot.top
            anchors.margins: 8
            clip: true

            BusyDots { anchors.centerIn: parent; running: ql.pv.kind === "loading" }

            Image {
                anchors.fill: parent
                visible: ql.pv.kind === "image"
                source: ql.pv.kind === "image" ? "file://" + encodeURI(ql.pv.src).replace(/#/g, "%23") : ""
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                autoTransform: true
                smooth: true
                mipmap: true
            }

            Item {
                anchors.fill: parent
                visible: ql.pv.kind === "media"
                MediaPlayer {
                    id: player
                    source: ql.pv.kind === "media" ? "file://" + encodeURI(ql.item.path).replace(/#/g, "%23") : ""
                    videoOutput: video
                    audioOutput: AudioOutput {}
                    onSourceChanged: if (source != "") play()
                }
                VideoOutput { id: video; anchors.fill: parent; anchors.bottomMargin: 44; visible: !!ql.pv.video }
                GIcon { anchors.centerIn: parent; size: 128; name: ql.item ? ql.item.icon : ""; visible: !ql.pv.video }
                Row {
                    anchors.bottom: parent.bottom
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: 10
                    height: 36
                    GButton { iconName: player.playbackState === MediaPlayer.PlayingState ? "media-playback-pause" : "media-playback-start"; tip: "Play / pause"; onClicked: player.playbackState === MediaPlayer.PlayingState ? player.pause() : player.play(); anchors.verticalCenter: parent.verticalCenter }
                    Slider {
                        width: Math.min(500, body.width - 200)
                        anchors.verticalCenter: parent.verticalCenter
                        from: 0; to: Math.max(1, player.duration)
                        value: player.position
                        onMoved: player.position = value
                    }
                    GLabel {
                        anchors.verticalCenter: parent.verticalCenter
                        text: fmt(player.position) + " / " + fmt(player.duration)
                        color: Theme.fgSecondary
                        function fmt(ms) { const s = Math.floor(ms / 1000); return Math.floor(s / 60) + ":" + ("0" + (s % 60)).slice(-2) }
                    }
                }
            }

            Loader {
                anchors.fill: parent
                active: ql.pv.kind === "pdf"
                sourceComponent: PdfMultiPageView {
                    document: PdfDocument { source: "file://" + encodeURI(ql.item.path).replace(/#/g, "%23") }
                }
            }

            ScrollView {
                anchors.fill: parent
                visible: ql.pv.kind === "code"
                TextEdit {
                    readOnly: true
                    selectByMouse: true
                    textFormat: TextEdit.RichText
                    text: ql.pv.kind === "code" ? ql.pv.html : ""
                    font.family: Theme.mono
                    font.pixelSize: Theme.body * 0.95
                    renderType: Text.NativeRendering
                    color: Theme.fg
                    selectionColor: Theme.accent
                    padding: 12
                }
            }

            ScrollView {
                id: mdView
                anchors.fill: parent
                visible: ql.pv.kind === "markdown"
                TextEdit {
                    width: mdView.width - 24
                    readOnly: true
                    selectByMouse: true
                    textFormat: TextEdit.MarkdownText
                    wrapMode: TextEdit.Wrap
                    text: ql.pv.kind === "markdown" ? ql.pv.text : ""
                    font.family: Theme.family
                    font.pixelSize: Theme.body
                    renderType: Text.NativeRendering
                    color: Theme.fg
                    selectionColor: Theme.accent
                    padding: 16
                }
            }

            ListView {
                anchors.fill: parent
                visible: ql.pv.kind === "archive"
                clip: true
                model: ql.pv.kind === "archive" ? ql.pv.entries : []
                ScrollBar.vertical: ScrollBar {}
                header: GLabel { text: ql.pv.total + " items in this archive" + (ql.pv.total > 500 ? " (first 500 shown)" : ""); color: Theme.fgSecondary; padding: 8 }
                delegate: Item {
                    required property var modelData
                    width: ListView.view.width
                    height: Theme.rowHeight
                    GIcon { x: 8 + (modelData.path.split("/").length - 1) * 14; anchors.verticalCenter: parent.verticalCenter; size: 16; name: modelData.dir ? "folder" : "text-x-generic" }
                    GLabel { x: 30 + (modelData.path.split("/").length - 1) * 14; anchors.verticalCenter: parent.verticalCenter; text: modelData.path.substring(modelData.path.lastIndexOf("/") + 1) }
                    GLabel { anchors.right: parent.right; anchors.rightMargin: 16; anchors.verticalCenter: parent.verticalCenter; text: modelData.dir ? "" : ql.app.size(modelData.size); color: Theme.fgSecondary }
                }
            }

            Column {
                anchors.centerIn: parent
                spacing: 12
                visible: ql.pv.kind === "folder" || ql.pv.kind === "other" || ql.pv.kind === "error"
                GIcon { anchors.horizontalCenter: parent.horizontalCenter; size: 160; name: ql.item ? ql.item.icon : "" }
                GLabel { anchors.horizontalCenter: parent.horizontalCenter; text: ql.item ? ql.item.name : ""; font.pixelSize: Theme.large; font.bold: true }
                GLabel {
                    anchors.horizontalCenter: parent.horizontalCenter
                    color: Theme.fgSecondary
                    text: ql.pv.kind === "folder" ? (ql.pv.folders + " folders, " + ql.pv.files + " files") : ql.pv.kind === "error" ? ql.pv.error : ((ql.pv.kindText || "") + (ql.pv.sizeText ? " — " + ql.pv.sizeText : ""))
                }
                GLabel {
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: ql.pv.kind === "folder" && (ql.pv.sample || []).length > 0
                    text: (ql.pv.sample || []).join("   ·   ")
                    color: Theme.fgTertiary
                    width: Math.min(implicitWidth, body.width - 40)
                    wrapMode: Text.Wrap
                    horizontalAlignment: Text.AlignHCenter
                }
                GLabel { anchors.horizontalCenter: parent.horizontalCenter; visible: !!ql.pv.note; text: ql.pv.note || ""; color: Theme.fgSecondary; width: Math.min(implicitWidth, body.width - 40); wrapMode: Text.Wrap }
            }
        }
        GLabel {
            id: foot
            anchors.bottom: parent.bottom
            width: parent.width
            height: 26
            horizontalAlignment: Text.AlignHCenter
            font.pixelSize: Theme.small
            color: Theme.fgSecondary
            text: (ql.pv.kindText || "") + (ql.pv.sizeText ? "  ·  " + ql.pv.sizeText : "") + (ql.pv.lines ? "  ·  " + ql.pv.lines + " lines" + (ql.pv.truncated ? " (start shown)" : "") : "") + "     ←/→ next file · Space closes"
        }
    }
}
