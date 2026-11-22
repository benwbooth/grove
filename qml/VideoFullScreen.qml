import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window
import QtMultimedia

// A video full screen: the same player, shown on a black full-screen
// window with play/pause, a seek bar and the time; the controls hide while
// it plays and the mouse rests. Esc, F or a double-click goes back.
Window {
    id: fs
    property var player: null
    property var restoreOutput: null
    color: "black"
    title: "Video"
    flags: Qt.Window | Qt.FramelessWindowHint

    function open(p, back) {
        player = p
        restoreOutput = back
        p.videoOutput = out
        showFullScreen(); raise(); requestActivate()
        poke()
    }
    function leave() {
        if (player) player.videoOutput = restoreOutput
        close()
    }
    onClosing: if (player && player.videoOutput === out) player.videoOutput = restoreOutput

    property bool controls: true
    function poke() { controls = true; hide.restart() }
    Timer { id: hide; interval: 2200; onTriggered: if (fs.player && fs.player.playbackState === MediaPlayer.PlayingState) fs.controls = false }

    Item {
        anchors.fill: parent
        focus: true
        transform: Translate { x: 0.001; y: 0.001 }
        Keys.onPressed: (e) => {
            if (e.key === Qt.Key_Escape || e.key === Qt.Key_F) { fs.leave(); e.accepted = true }
            else if (e.key === Qt.Key_Space) { fs.player.playbackState === MediaPlayer.PlayingState ? fs.player.pause() : fs.player.play(); fs.poke(); e.accepted = true }
            else if (e.key === Qt.Key_Right) { fs.player.position = Math.min(fs.player.duration, fs.player.position + 5000); fs.poke(); e.accepted = true }
            else if (e.key === Qt.Key_Left) { fs.player.position = Math.max(0, fs.player.position - 5000); fs.poke(); e.accepted = true }
        }
        VideoOutput { id: out; anchors.fill: parent }
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: fs.controls ? Qt.ArrowCursor : Qt.BlankCursor
            onPositionChanged: fs.poke()
            onClicked: fs.poke()
            onDoubleClicked: fs.leave()
        }
        Rectangle {
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 30
            anchors.horizontalCenter: parent.horizontalCenter
            width: Math.min(parent.width - 60, 900)
            height: 52
            radius: 14
            color: Qt.rgba(0, 0, 0, 0.6)
            opacity: fs.controls ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: 200 } }
            Row {
                anchors.centerIn: parent
                spacing: 12
                GButton { tint: "white"; anchors.verticalCenter: parent.verticalCenter
                    iconName: fs.player && fs.player.playbackState === MediaPlayer.PlayingState ? "media-playback-pause" : "media-playback-start"; tip: "Play / pause (Space)"
                    onClicked: { fs.player.playbackState === MediaPlayer.PlayingState ? fs.player.pause() : fs.player.play(); fs.poke() } }
                Slider {
                    width: Math.min(fs.width - 300, 640)
                    anchors.verticalCenter: parent.verticalCenter
                    from: 0; to: fs.player ? Math.max(1, fs.player.duration) : 1
                    value: fs.player ? fs.player.position : 0
                    onMoved: { fs.player.position = value; fs.poke() }
                }
                GLabel { anchors.verticalCenter: parent.verticalCenter; color: "white"
                    text: fs.player ? fmt(fs.player.position) + " / " + fmt(fs.player.duration) : ""
                    function fmt(ms) { const s = Math.floor(ms / 1000); return Math.floor(s / 60) + ":" + ("0" + (s % 60)).slice(-2) } }
                GButton { tint: "white"; iconName: "view-restore"; tip: "Leave full screen (Esc, F or double-click)"; anchors.verticalCenter: parent.verticalCenter; onClicked: fs.leave() }
            }
        }
    }
}
