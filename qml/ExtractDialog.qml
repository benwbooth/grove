import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window

// Extract (7-Zip's Extract dialog, grove's look): where to, whether to
// make a folder for it, keep the folders inside or not, what to do with
// files already there, the password, and what happens afterwards. `extra`
// carries the selection from an archive window ({only, strip, password}).
Window {
    id: ed
    property var app
    property var archiver
    property string archive: ""
    property var extra: ({})
    readonly property var s: app.settings
    readonly property bool selection: !!(extra.only && extra.only.length)
    width: Math.round(500 * Theme.pt)
    height: col.implicitHeight + 32
    minimumWidth: 420
    color: Theme.window
    title: "Extract “" + name(archive) + "”"

    function name(p) { return p.substring(p.lastIndexOf("/") + 1) }
    function dirOf(p) { return p.substring(0, p.lastIndexOf("/")) || "/" }

    readonly property var folderModes: [
        { v: "auto", t: "When it has several items", tip: "A folder named after the archive, unless everything inside is already in one folder" },
        { v: "always", t: "Always", tip: "Always put it in a folder named after the archive" },
        { v: "never", t: "Never", tip: "Straight into the folder above" }]
    readonly property var overwriteModes: [
        { v: "ask", t: "Ask me" }, { v: "replace", t: "Replace them" }, { v: "skip", t: "Keep them (skip)" },
        { v: "rename", t: "Keep both (number the new one)" }, { v: "renameExisting", t: "Keep both (number the old one)" }]

    function go() {
        const a = {
            archive: archive, dest: dest.text.trim(),
            folder: folderModes[folderBox.currentIndex].v,
            keepPaths: pathsBox.currentIndex === 0,
            overwrite: overwriteModes[overBox.currentIndex].v,
            password: pwField.text, only: extra.only || [], strip: extra.strip || ""
        }
        archiver.extract(a, true, { reveal: revealBox.checked, trash: trashBox.checked && !selection, force: true })
        close()
    }

    Shortcut { sequence: "Escape"; onActivated: ed.close() }
    Shortcut { sequences: ["Return", "Enter"]; onActivated: ed.go() }

    component Label2: GLabel { width: 130; horizontalAlignment: Text.AlignRight; color: Theme.fgSecondary; height: Theme.controlHeight }

    Column {
        id: col
        x: 16; y: 16
        width: ed.width - 32
        spacing: 10
        transform: Translate { x: 0.001; y: 0.001 }

        Row {
            spacing: 12
            GIcon { size: 40; name: "package-x-generic|archive-extract" }
            Column {
                width: col.width - 52
                GLabel { text: ed.title; font.bold: true; width: parent.width; elide: Text.ElideMiddle }
                GLabel { text: ed.selection ? ed.extra.only.length + (ed.extra.only.length === 1 ? " selected item" : " selected items") : "Everything in it"
                         font.pixelSize: Theme.small; color: Theme.fgSecondary }
            }
        }
        Row {
            spacing: 8
            Label2 { text: "Extract to" }
            GField {
                id: dest
                width: col.width - 138 - 98
                text: ed.extra.dest || ed.dirOf(ed.archive)
                tip: "The folder to extract into"
            }
            PushButton {
                text: "Choose…"; width: 90
                tip: "Pick a folder"
                onClicked: ed.archiver.choose("Extract To", { directory: true, currentFolder: dest.text, acceptLabel: "Choose" }, ps => { if (ps && ps.length) dest.text = ps[0] })
            }
        }
        Row {
            spacing: 8
            Label2 { text: "Make a folder" }
            GCombo {
                id: folderBox
                width: 260
                model: ed.folderModes.map(m => m.t)
                currentIndex: Math.max(0, ed.folderModes.findIndex(m => m.v === (ed.s.extract_folder || "auto")))
                tip: ed.folderModes[currentIndex].tip
            }
        }
        Row {
            spacing: 8
            Label2 { text: "Folders inside" }
            GCombo {
                id: pathsBox
                width: 260
                model: ["Keep them", "Leave them out (files only)"]
                tip: "Keep: the archive's folders are made as they were (7-Zip's full pathnames). Leave out: every file lands in one folder"
            }
        }
        Row {
            spacing: 8
            Label2 { text: "Files already there" }
            GCombo {
                id: overBox
                width: 260
                model: ed.overwriteModes.map(m => m.t)
                tip: "What to do when a file with the same name is already in the folder"
            }
        }
        Row {
            spacing: 8
            Label2 { text: "Password" }
            GField {
                id: pwField
                width: 260
                text: ed.extra.password || ""
                echoMode: showPw.checked ? TextInput.Normal : TextInput.Password
                placeholderText: "Only if it has one"
                tip: "For encrypted archives; you'll be asked if it's needed and missing"
            }
            GCheck { id: showPw; text: "Show"; tip: "Show the password as you type" }
        }
        Column {
            x: 138
            spacing: 4
            GCheck { id: revealBox; text: "Show the files in grove afterwards"; checked: !!ed.s.archive_reveal
                     tip: "Opens a grove window at the extracted files when it's done" }
            GCheck { id: trashBox; text: "Move the archive to the Trash afterwards"; checked: !!ed.s.archive_trash_after; enabled: !ed.selection
                     tip: "Only when everything was extracted without problems. You can get it back from the Trash" }
        }
        Row {
            anchors.right: parent.right
            spacing: 8
            PushButton { text: "Cancel"; tip: "Close without extracting"; onClicked: ed.close() }
            PushButton { text: "Extract"; primary: true; tip: "Start extracting (Return)"; onClicked: ed.go() }
        }
    }

    // Tests: GROVE_ARC_TEST containing "ok" presses Extract.
    Timer { interval: 600; running: (ed.app.grove.env("GROVE_ARC_TEST") || "").indexOf("ok") >= 0; onTriggered: ed.go() }
}
