import QtQuick
import QtQuick.Controls.Basic

// File operations in progress: a ring in the toolbar; click for the list
// with pause/resume and stop for each.
Item {
    id: jb
    property var app
    property var win
    readonly property int running: {
        let n = 0
        for (let i = 0; i < app.jobs.count; i++) { const s = app.jobs.get(i).state; if (s === "running" || s === "paused") n++ }
        return n
    }
    readonly property real overall: {
        let sum = 0, n = 0
        for (let i = 0; i < app.jobs.count; i++) { const j = app.jobs.get(i); if (j.state === "running" || j.state === "paused") { sum += j.fraction; n++ } }
        return n ? sum / n : 0
    }
    visible: app.jobs.count > 0
    width: visible ? Theme.controlHeight : 0
    height: Theme.controlHeight

    Canvas {
        id: ring
        anchors.centerIn: parent
        width: 20; height: 20
        property real f: jb.overall
        property color c: Theme.accent
        property color bg: Theme.separator
        onFChanged: requestPaint()
        onPaint: {
            const ctx = getContext("2d")
            ctx.reset()
            ctx.lineWidth = 3
            ctx.strokeStyle = bg
            ctx.beginPath(); ctx.arc(10, 10, 8, 0, Math.PI * 2); ctx.stroke()
            ctx.strokeStyle = c
            ctx.beginPath(); ctx.arc(10, 10, 8, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * Math.max(0.03, f)); ctx.stroke()
        }
    }
    MouseArea { id: m; anchors.fill: parent; hoverEnabled: true; onClicked: pop.open() }
    GTip { visible: m.containsMouse && !pop.opened; text: jb.running ? jb.running + " operation" + (jb.running > 1 ? "s" : "") + " in progress — click for details" : "Recent file operations" }

    Popup {
        id: pop
        y: jb.height + 6
        x: jb.width - width
        width: Math.round(360 * Theme.pt)
        padding: 12
        topPadding: 12.001
        leftPadding: 12.001
        background: GlassPanel { tint: Theme.glassMenu }
        contentItem: Column {
            spacing: 10
            GLabel { text: "File Operations"; font.bold: true }
            Repeater {
                model: jb.app.jobs
                Column {
                    required property int jobId
                    required property string title
                    required property string state
                    required property real fraction
                    required property string detail
                    required property bool indeterminate
                    width: pop.availableWidth
                    spacing: 4
                    Row {
                        width: parent.width
                        spacing: 6
                        GLabel { width: parent.width - 60; text: title; font.pixelSize: Theme.small }
                        GButton {
                            visible: state === "running" || state === "paused"
                            width: 22; height: 22
                            iconName: state === "paused" ? "media-playback-start" : "media-playback-pause"
                            iconSize: 12
                            tip: state === "paused" ? "Resume" : "Pause"
                            onClicked: jb.app.grove.pauseJob(jobId, state !== "paused")
                        }
                        GButton {
                            visible: state === "running" || state === "paused"
                            width: 22; height: 22
                            iconName: "process-stop|dialog-cancel"
                            iconSize: 12
                            tip: "Stop (what's done so far stays; a half-copied file is removed)"
                            onClicked: jb.app.grove.cancelJob(jobId)
                        }
                    }
                    Rectangle {
                        width: parent.width; height: 4; radius: 2
                        color: Theme.separator
                        Rectangle {
                            width: parent.width * (state === "done" ? 1 : indeterminate ? 0.3 : fraction)
                            height: 4; radius: 2
                            color: state === "failed" ? Theme.danger : state === "paused" ? Theme.fgSecondary : Theme.accent
                        }
                    }
                    GLabel { width: parent.width; text: detail; font.pixelSize: Theme.caption; color: state === "failed" ? Theme.danger : Theme.fgSecondary; wrapMode: Text.Wrap; maximumLineCount: 3 }
                }
            }
        }
    }
}
