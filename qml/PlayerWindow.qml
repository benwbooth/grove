import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window
import QtMultimedia

// Grove Player: a QuickTime-style player. The picture fills a dark,
// borderless window that keeps the video's shape; a floating glass bar
// (play/pause, back/forward 15 s, the scrubber, time, volume, speed,
// subtitles and audio tracks, full screen) shows on mouse movement and
// fades while it plays. Music shows its cover art. Keys: Space, ←/→
// (5 s; Shift: 15 s), ↑/↓ volume, F full screen, M mute, Esc, Ctrl+W.
Window {
    id: pw
    property var app
    property string path: ""
    readonly property string fileName: path.substring(path.lastIndexOf("/") + 1)
    readonly property bool isVideo: player.hasVideo
    color: "black"
    title: fileName
    flags: Qt.Window | Qt.FramelessWindowHint
    minimumWidth: 360
    minimumHeight: 220
    width: 960
    height: 600

    // The window takes the video's shape once it's known (at most 80% of
    // the screen).
    property bool sized: false
    function fitToVideo() {
        if (sized || !isVideo) return
        const r = player.metaData.value(MediaMetaData.Resolution)
        if (!r || !r.width || !r.height) return
        const maxW = Screen.desktopAvailableWidth * 0.8, maxH = Screen.desktopAvailableHeight * 0.8
        const s = Math.min(1, maxW / r.width, maxH / r.height)
        width = Math.round(r.width * s); height = Math.round(r.height * s)
        sized = true
    }

    MediaPlayer {
        id: player
        source: pw.path ? "file://" + encodeURI(pw.path).replace(/#/g, "%23") : ""
        videoOutput: video
        audioOutput: AudioOutput { id: audio; volume: 0.8 }
        onMediaStatusChanged: if (mediaStatus === MediaPlayer.LoadedMedia) { pw.fitToVideo(); play() }
        onMetaDataChanged: pw.fitToVideo()
        onErrorOccurred: (err, msg) => errorText.text = msg
    }

    function toggle() { player.playbackState === MediaPlayer.PlayingState ? player.pause() : player.play(); poke() }
    function skip(ms) { player.position = Math.max(0, Math.min(player.duration, player.position + ms)); poke() }
    function fmt(ms) {
        const s = Math.max(0, Math.floor(ms / 1000)), h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60
        return (h ? h + ":" + ("0" + m).slice(-2) : m) + ":" + ("0" + (s % 60)).slice(-2)
    }
    function fullScreen() { visibility === Window.FullScreen ? showNormal() : showFullScreen(); poke() }

    property bool controls: true
    function poke() { controls = true; hideTimer.restart() }
    Timer { id: hideTimer; interval: 2500; onTriggered: if (player.playbackState === MediaPlayer.PlayingState && !bar.hovered) pw.controls = false }

    Item {
        anchors.fill: parent
        focus: true
        transform: Translate { x: 0.001; y: 0.001 }
        Keys.onPressed: (e) => {
            const big = (e.modifiers & Qt.ShiftModifier) ? 15000 : 5000
            if (e.key === Qt.Key_Space || e.key === Qt.Key_K) pw.toggle()
            else if (e.key === Qt.Key_Right || e.key === Qt.Key_L) pw.skip(big)
            else if (e.key === Qt.Key_Left || e.key === Qt.Key_J) pw.skip(-big)
            else if (e.key === Qt.Key_Up) { audio.volume = Math.min(1, audio.volume + 0.05); pw.poke() }
            else if (e.key === Qt.Key_Down) { audio.volume = Math.max(0, audio.volume - 0.05); pw.poke() }
            else if (e.key === Qt.Key_M) { audio.muted = !audio.muted; pw.poke() }
            else if (e.key === Qt.Key_F) pw.fullScreen()
            else if (e.key === Qt.Key_Escape) { if (pw.visibility === Window.FullScreen) pw.showNormal(); else pw.close() }
            else if (e.key === Qt.Key_W && (e.modifiers & Qt.ControlModifier)) pw.close()
            else return
            e.accepted = true
        }

        VideoOutput { id: video; anchors.fill: parent; visible: pw.isVideo; fillMode: VideoOutput.PreserveAspectFit }

        // Music: the cover art (or the icon), the title and artist.
        Column {
            anchors.centerIn: parent
            anchors.verticalCenterOffset: -30
            spacing: 14
            visible: !pw.isVideo && player.hasAudio
            Image {
                id: cover
                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.min(pw.width * 0.5, pw.height * 0.55, 360); height: width
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                sourceSize: Qt.size(512, 512)
                source: pw.path ? "image://thumb/" + encodeURIComponent(pw.path) + "?m=0" : ""
            }
            GLabel { anchors.horizontalCenter: parent.horizontalCenter; color: "white"; font.pixelSize: Theme.title; font.bold: true
                text: player.metaData.stringValue(MediaMetaData.Title) || pw.fileName }
            GLabel { anchors.horizontalCenter: parent.horizontalCenter; color: Qt.rgba(1, 1, 1, 0.6)
                text: player.metaData.stringValue(MediaMetaData.ContributingArtist) || player.metaData.stringValue(MediaMetaData.AlbumArtist) || "" }
        }
        GLabel { id: errorText; anchors.centerIn: parent; color: "white"; visible: text !== ""; text: ""; width: Math.min(implicitWidth, pw.width - 40); wrapMode: Text.Wrap }

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: pw.controls ? Qt.ArrowCursor : Qt.BlankCursor
            onPositionChanged: pw.poke()
            onClicked: pw.toggle()
            onDoubleClicked: pw.fullScreen()
            // Drag the window by its picture (it has no title bar).
            onPressAndHold: pw.startSystemMove()
        }

        // Top: close and full screen, the title, like QuickTime's.
        Rectangle {
            id: top
            width: parent.width
            height: 40
            opacity: pw.controls ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: 220 } }
            gradient: Gradient {
                GradientStop { position: 0; color: Qt.rgba(0, 0, 0, 0.55) }
                GradientStop { position: 1; color: "transparent" }
            }
            MouseArea { anchors.fill: parent; onPressed: pw.startSystemMove(); onDoubleClicked: pw.fullScreen() }
            Row {
                x: 12; anchors.verticalCenter: parent.verticalCenter
                spacing: 4
                GButton { iconName: "window-close"; iconSize: 14; tint: "white"; tip: "Close (Ctrl+W)"; onClicked: pw.close() }
                GButton { iconName: pw.visibility === Window.FullScreen ? "view-restore" : "view-fullscreen"; iconSize: 14; tint: "white"; tip: "Full screen (F)"; onClicked: pw.fullScreen() }
            }
            GLabel { anchors.centerIn: parent; text: pw.fileName; color: "white"; font.bold: true; elide: Text.ElideMiddle; width: Math.min(implicitWidth, parent.width - 200) }
        }

        // The floating control bar.
        Rectangle {
            id: bar
            property bool hovered: barHover.hovered
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 22
            anchors.horizontalCenter: parent.horizontalCenter
            width: Math.min(parent.width - 40, 720)
            height: 76
            radius: 16
            color: Qt.rgba(0.12, 0.12, 0.13, 0.78)
            border.color: Qt.rgba(1, 1, 1, 0.12)
            opacity: pw.controls ? 1 : 0
            visible: opacity > 0
            Behavior on opacity { NumberAnimation { duration: 220 } }
            HoverHandler { id: barHover; onHoveredChanged: if (hovered) pw.poke() }

            // Scrubber: time played, the track, time left.
            Row {
                id: scrubRow
                x: 16; y: 46
                width: parent.width - 32
                spacing: 10
                GLabel { id: tNow; text: pw.fmt(player.position); color: Qt.rgba(1, 1, 1, 0.8); font.pixelSize: Theme.small; font.family: Theme.mono; anchors.verticalCenter: parent.verticalCenter }
                Item {
                    id: track
                    width: scrubRow.width - tNow.width - tLeft.width - 20
                    height: 18
                    anchors.verticalCenter: parent.verticalCenter
                    readonly property real frac: player.duration > 0 ? player.position / player.duration : 0
                    Rectangle { anchors.verticalCenter: parent.verticalCenter; width: parent.width; height: 5; radius: 2.5; color: Qt.rgba(1, 1, 1, 0.25) }
                    Rectangle { anchors.verticalCenter: parent.verticalCenter; width: parent.width * track.frac; height: 5; radius: 2.5; color: "white" }
                    Rectangle { x: parent.width * track.frac - width / 2; anchors.verticalCenter: parent.verticalCenter; width: 12; height: 12; radius: 6; color: "white"; visible: scrubMouse.containsMouse || scrubMouse.pressed }
                    MouseArea {
                        id: scrubMouse
                        anchors.fill: parent
                        hoverEnabled: true
                        function seek(x) { if (player.duration > 0) player.position = Math.max(0, Math.min(1, x / width)) * player.duration }
                        onPressed: (m) => seek(m.x)
                        onPositionChanged: (m) => { if (pressed) seek(m.x); pw.poke() }
                    }
                    GTip { visible: scrubMouse.containsMouse && player.duration > 0; text: pw.fmt(scrubMouse.mouseX / track.width * player.duration) }
                }
                GLabel { id: tLeft; text: "−" + pw.fmt(player.duration - player.position); color: Qt.rgba(1, 1, 1, 0.8); font.pixelSize: Theme.small; font.family: Theme.mono; anchors.verticalCenter: parent.verticalCenter }
            }

            // Buttons: volume on the left, transport in the middle, the rest right.
            Row {
                x: 14; y: 10
                spacing: 4
                GButton { iconName: audio.muted || audio.volume === 0 ? "audio-volume-muted" : audio.volume < 0.5 ? "audio-volume-low" : "audio-volume-high"; tint: "white"; iconSize: 16
                    tip: "Mute (M)"; onClicked: audio.muted = !audio.muted; anchors.verticalCenter: parent.verticalCenter }
                Slider {
                    id: vol
                    width: 90; anchors.verticalCenter: parent.verticalCenter
                    from: 0; to: 1; value: audio.volume
                    background: Item {
                        x: vol.leftPadding; y: vol.topPadding + vol.availableHeight / 2 - 2
                        width: vol.availableWidth; height: 4
                        Rectangle { anchors.fill: parent; radius: 2; color: Qt.rgba(1, 1, 1, 0.25) }
                        Rectangle { width: vol.visualPosition * parent.width; height: parent.height; radius: 2; color: "white" }
                    }
                    handle: Rectangle {
                        x: vol.leftPadding + vol.visualPosition * (vol.availableWidth - width)
                        y: vol.topPadding + vol.availableHeight / 2 - height / 2
                        width: 12; height: 12; radius: 6; color: "white"
                    }
                    onMoved: { audio.volume = value; audio.muted = false; pw.poke() }
                    GTip { visible: parent.hovered; text: "Volume " + Math.round(audio.volume * 100) + "% (↑/↓)" }
                }
            }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                y: 6
                spacing: 18
                GButton { iconName: "media-seek-backward"; tint: "white"; iconSize: 20; tip: "Back 15 seconds (Shift+←)"; onClicked: pw.skip(-15000); anchors.verticalCenter: parent.verticalCenter }
                GButton { iconName: player.playbackState === MediaPlayer.PlayingState ? "media-playback-pause" : "media-playback-start"; tint: "white"; iconSize: 28
                    tip: "Play / pause (Space)"; onClicked: pw.toggle(); anchors.verticalCenter: parent.verticalCenter }
                GButton { iconName: "media-seek-forward"; tint: "white"; iconSize: 20; tip: "Forward 15 seconds (Shift+→)"; onClicked: pw.skip(15000); anchors.verticalCenter: parent.verticalCenter }
            }
            Row {
                anchors.right: parent.right
                anchors.rightMargin: 12
                y: 10
                spacing: 4
                GButton {
                    text: player.playbackRate === 1 ? "1×" : player.playbackRate + "×"
                    tint: "white"; tip: "Playback speed"
                    onClicked: speedMenu.popup()
                    GMenu {
                        id: speedMenu
                        Repeater {
                            model: [0.5, 0.75, 1, 1.25, 1.5, 2]
                            GMenuItem { required property var modelData; text: modelData + "×"; checkable: true; checked: player.playbackRate === modelData; onTriggered: player.playbackRate = modelData }
                        }
                    }
                }
                GButton {
                    iconName: "media-view-subtitles-symbolic|format-text-bold"; tint: "white"; iconSize: 16
                    visible: player.subtitleTracks.length > 0 || player.audioTracks.length > 1
                    tip: "Subtitles and audio"
                    onClicked: trackMenu.popup()
                    GMenu {
                        id: trackMenu
                        GMenuItem { text: "Subtitles Off"; checkable: true; checked: player.activeSubtitleTrack < 0; onTriggered: player.activeSubtitleTrack = -1; visible: player.subtitleTracks.length > 0 }
                        Repeater {
                            model: player.subtitleTracks.length
                            GMenuItem { required property int index; text: "Subtitles: " + (player.subtitleTracks[index].stringValue(MediaMetaData.Language) || ("Track " + (index + 1)))
                                checkable: true; checked: player.activeSubtitleTrack === index; onTriggered: player.activeSubtitleTrack = index }
                        }
                        Repeater {
                            model: player.audioTracks.length > 1 ? player.audioTracks.length : 0
                            GMenuItem { required property int index; text: "Audio: " + (player.audioTracks[index].stringValue(MediaMetaData.Language) || ("Track " + (index + 1)))
                                checkable: true; checked: player.activeAudioTrack === index; onTriggered: player.activeAudioTrack = index }
                        }
                    }
                }
                GButton { iconName: pw.visibility === Window.FullScreen ? "view-restore" : "view-fullscreen"; tint: "white"; iconSize: 16; tip: "Full screen (F, double-click)"; onClicked: pw.fullScreen() }
            }
        }
    }
    Component.onCompleted: poke()
}
