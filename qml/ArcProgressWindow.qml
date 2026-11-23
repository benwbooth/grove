import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window

// One archive job (extract, add to archive, test, change an archive) in a
// small window, like 7-Zip's progress dialog: the bar, the file it's on,
// elapsed and remaining time, speed, sizes, compression ratio, Pause and
// Cancel. It asks here too: a file that's already there (Replace / Keep
// Both / Skip, for all), and the password when the archive needs one (the
// job starts again with it). Testing ends with what it found.
Window {
    id: pw
    property var app
    property var archiver
    property string kind: "extract"
    property var args: ({})
    property int jobId: -1
    property var snap: ({})
    property string phase: "running"   // running, asking, password, done, failed
    property string message: ""
    property bool hiddenByUser: false
    signal finished(var result)

    readonly property var grove: app.grove
    width: Math.round(470 * Theme.pt)
    height: content.implicitHeight + 32
    minimumWidth: 380
    color: Theme.window
    title: snap.title || (kind === "test" ? "Testing" : kind === "create" ? "Compressing" : kind === "update" ? "Updating" : "Extracting")

    Component.onCompleted: start()
    function start() {
        phase = "running"; message = ""
        snap = {}
        jobId = grove.arcStart(kind, JSON.stringify(args))
        poll.start()
    }
    onClosing: (close) => {
        poll.stop()
        if (jobId >= 0) grove.arcForget(jobId)   // cancels if still running
        jobId = -1
    }
    Timer {
        id: poll
        interval: 150; repeat: true
        onTriggered: pw.tick()
    }
    function tick() {
        if (jobId < 0) return
        const s = JSON.parse(grove.arcProgress(jobId))
        snap = s
        if (s.state === "asking") { if (phase !== "asking") { phase = "asking"; allBox.checked = false; showAgain() } return }
        if (phase === "asking") phase = "running"
        if (s.state === "done") return end(s)
        if (s.state === "failed" || s.state === "cancelled") return fail(s)
    }
    function showAgain() { if (hiddenByUser) { hiddenByUser = false; show() } raise(); requestActivate() }
    function end(s) {
        poll.stop()
        const errs = (s.report && s.report.errors) || []
        phase = "done"
        if (kind === "test") {
            message = errs.length ? errs.length + (errs.length === 1 ? " problem found:\n" : " problems found:\n") + errs.join("\n")
                                  : "No errors found. " + s.report.files + (s.report.files === 1 ? " file" : " files") + ", " + app.size(s.report.bytes || 0) + " checked."
            showAgain()
        } else if (errs.length) {
            message = "Finished with " + errs.length + (errs.length === 1 ? " problem:\n" : " problems:\n") + errs.join("\n")
            showAgain()
        } else {
            Qt.callLater(close)
        }
        finished(s)
    }
    function fail(s) {
        poll.stop()
        if (s.code === "needPassword" || s.code === "badPassword") {
            phase = "password"
            message = s.code === "badPassword" ? "That password didn't work. Try again:" : "“" + name(args.archive || "") + "” is protected by a password."
            pwField.text = ""
            showAgain()
            pwField.forceActiveFocus()
            return
        }
        if (s.state === "cancelled") { close(); return }
        phase = "failed"
        message = s.error || "Something went wrong."
        showAgain()
    }
    function name(p) { return p.substring(p.lastIndexOf("/") + 1) }
    function retryWithPassword() {
        if (!pwField.text) return
        const a = Object.assign({}, args); a.password = pwField.text; args = a
        if (jobId >= 0) grove.arcForget(jobId)
        start()
    }
    function answer(a) {
        grove.arcAnswer(jobId, allBox.checked && a !== "cancel" ? a + "All" : a)
        phase = "running"
    }
    function cancel() { if (jobId >= 0 && (phase === "running" || phase === "asking")) grove.arcCancel(jobId); else close() }

    function hms(sec) {
        if (!isFinite(sec) || sec < 0) return "—"
        sec = Math.round(sec)
        const h = Math.floor(sec / 3600), m = Math.floor(sec / 60) % 60, s = sec % 60
        return (h ? h + ":" + ("0" + m).slice(-2) : m) + ":" + ("0" + s).slice(-2)
    }
    readonly property real fraction: snap.total > 0 ? Math.min(1, snap.done / snap.total) : 0
    readonly property real speed: snap.elapsed > 0.2 ? (snap.bytes || 0) / snap.elapsed : 0
    readonly property real remaining: fraction > 0.01 ? snap.elapsed * (1 - fraction) / fraction : -1
    readonly property bool paused: snap.state === "paused"

    Shortcut { sequence: "Escape"; onActivated: pw.cancel() }
    Shortcut { sequences: ["Ctrl+W"]; onActivated: pw.phase === "running" ? pw.cancel() : pw.close() }

    component Fact: Row {
        property string k: ""
        property string v: ""
        property string tip: ""
        spacing: 8
        width: (content.width - 16) / 2
        GLabel { text: parent.k; width: 96; horizontalAlignment: Text.AlignRight; color: Theme.fgSecondary; font.pixelSize: Theme.small }
        GLabel { id: fv; text: parent.v; width: parent.width - 104; font.pixelSize: Theme.small; font.features: { "tnum": 1 } }
        HoverHandler { id: fh }
        GTip { visible: fh.hovered && parent.tip !== ""; text: parent.tip }
    }

    Column {
        id: content
        x: 16; y: 16
        width: pw.width - 32
        spacing: 12
        transform: Translate { x: 0.001; y: 0.001 }

        Row {
            spacing: 12
            width: parent.width
            GIcon {
                size: 40
                name: pw.kind === "create" ? "package-x-generic|archive-insert" : pw.kind === "test" ? "package-x-generic|dialog-ok" : "package-x-generic|archive-extract"
            }
            Column {
                width: parent.width - 52
                spacing: 2
                GLabel { text: pw.title; font.bold: true; width: parent.width }
                GLabel {
                    width: parent.width
                    font.pixelSize: Theme.small; color: Theme.fgSecondary
                    elide: Text.ElideMiddle
                    text: pw.phase === "running" ? (pw.paused ? "Paused — " : "") + (pw.snap.current || "Starting…")
                        : pw.phase === "done" ? "Done" : pw.phase === "asking" ? "Waiting for you" : ""
                }
            }
        }

        // The bar.
        Rectangle {
            visible: pw.phase === "running" || pw.phase === "asking"
            width: parent.width; height: 6; radius: 3
            color: Theme.field
            Rectangle { width: parent.width * pw.fraction; height: parent.height; radius: 3; color: pw.paused ? Theme.fgTertiary : Theme.accent
                Behavior on width { NumberAnimation { duration: 140 } } }
        }

        Grid {
            visible: pw.phase === "running"
            columns: 2
            columnSpacing: 16
            rowSpacing: 4
            Fact { k: "Elapsed"; v: pw.hms(pw.snap.elapsed || 0) }
            Fact { k: "Remaining"; v: pw.hms(pw.remaining); tip: "Estimated from the speed so far" }
            Fact { k: pw.kind === "create" ? "Read" : pw.kind === "test" ? "Checked" : "Written"; v: pw.app.size(pw.snap.bytes || 0) }
            Fact { k: "Speed"; v: pw.speed > 0 ? pw.app.size(pw.speed) + "/s" : "—" }
            Fact { k: "Files"; v: String(pw.snap.files || 0) }
            Fact { k: "Done"; v: Math.round(pw.fraction * 100) + "%" }
        }

        // A file already there.
        Column {
            visible: pw.phase === "asking"
            width: parent.width
            spacing: 8
            readonly property var q: pw.snap.question || {}
            GLabel { width: parent.width; wrapMode: Text.Wrap; elide: Text.ElideNone
                text: "“" + (parent.q.name || "") + "” is already in this folder. Replace it with the one from the archive?" }
            Grid {
                columns: 3; columnSpacing: 12; rowSpacing: 2
                GLabel { text: ""; font.pixelSize: Theme.small }
                GLabel { text: "Size"; font.pixelSize: Theme.small; color: Theme.fgSecondary }
                GLabel { text: "Modified"; font.pixelSize: Theme.small; color: Theme.fgSecondary }
                GLabel { text: "Already there"; font.pixelSize: Theme.small; color: Theme.fgSecondary }
                GLabel { text: pw.app.size(parent.parent.q.existingSize || 0); font.pixelSize: Theme.small }
                GLabel { text: parent.parent.q.existingTime ? new Date(parent.parent.q.existingTime * 1000).toLocaleString(Qt.locale(), Locale.ShortFormat) : "—"; font.pixelSize: Theme.small }
                GLabel { text: "From the archive"; font.pixelSize: Theme.small; color: Theme.fgSecondary }
                GLabel { text: pw.app.size(parent.parent.q.newSize || 0); font.pixelSize: Theme.small }
                GLabel { text: parent.parent.q.newTime ? new Date(parent.parent.q.newTime * 1000).toLocaleString(Qt.locale(), Locale.ShortFormat) : "—"; font.pixelSize: Theme.small }
            }
            GCheck { id: allBox; text: "Do this for every file that's already there"; tip: "Answer once for the rest of this archive (7-Zip's Yes to All / No to All)" }
            Row {
                spacing: 8
                anchors.right: parent.right
                PushButton { text: "Stop"; tip: "Stop extracting (what's already out stays)"; onClicked: pw.answer("cancel") }
                PushButton { text: "Skip"; tip: "Keep the file that's there; don't extract this one"; onClicked: pw.answer("skip") }
                PushButton { text: "Keep Both"; tip: "Extract it with a number added to its name (“file 2.txt”)"; onClicked: pw.answer("rename") }
                PushButton { text: "Replace"; primary: true; tip: "Overwrite the file that's there with the one from the archive"; onClicked: pw.answer("replace") }
            }
        }

        // The password.
        Column {
            visible: pw.phase === "password"
            width: parent.width
            spacing: 8
            GLabel { text: pw.message; width: parent.width; wrapMode: Text.Wrap; elide: Text.ElideNone }
            GField {
                id: pwField
                width: parent.width
                echoMode: showPw.checked ? TextInput.Normal : TextInput.Password
                placeholderText: "Password"
                tip: "The password the archive was made with"
                onAccepted: pw.retryWithPassword()
            }
            GCheck { id: showPw; text: "Show password"; tip: "Show the characters as you type" }
        }

        GLabel {
            visible: pw.phase === "done" || pw.phase === "failed"
            width: parent.width
            wrapMode: Text.Wrap; elide: Text.ElideNone
            text: pw.message
            color: pw.phase === "failed" ? Theme.danger : Theme.fg
        }

        Row {
            anchors.right: parent.right
            spacing: 8
            visible: pw.phase !== "asking"
            PushButton {
                text: "Hide"; visible: pw.phase === "running"
                tip: "Keep working in the background; this window comes back if it needs you"
                onClicked: { pw.hiddenByUser = true; pw.hide() }
            }
            PushButton {
                text: pw.paused ? "Resume" : "Pause"; visible: pw.phase === "running"
                tip: pw.paused ? "Carry on" : "Pause for now (frees the disk and processor)"
                onClicked: pw.grove.arcPause(pw.jobId, !pw.paused)
            }
            PushButton {
                text: pw.phase === "password" ? "Cancel" : pw.phase === "running" ? "Cancel" : "Close"
                tip: pw.phase === "running" ? "Stop (nothing half-made is left behind)" : "Close this window"
                onClicked: pw.phase === "running" ? pw.cancel() : pw.close()
            }
            PushButton {
                text: "OK"; primary: true; visible: pw.phase === "password"
                enabled: pwField.text.length > 0
                tip: "Try this password"
                onClicked: pw.retryWithPassword()
            }
        }
    }
}
