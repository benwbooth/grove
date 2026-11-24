import QtQuick
import QtQuick.Controls.Basic

// Format… for a removable drive (USB stick, SD card), as macOS Disk
// Utility's Erase: pick a name and a format, confirm once, done. The
// whole drive becomes one partition (UDisks2 does the work: it unmounts,
// makes the partition table and the filesystem). Internal disks and the
// system's disk are refused with a pointer to Disk Utility.
Sheet {
    id: fs
    property var app
    property var win
    property var disk: null
    property string reason: ""
    property bool looking: false
    property bool busy: false
    property bool confirming: false
    property string progressText: ""
    property real fraction: 0
    readonly property var formats: [
        { id: "exfat", name: "exFAT", help: "Best for sticks shared with Windows, Macs, cameras and TVs. Files of any size." },
        { id: "vfat", name: "FAT32 (MS-DOS)", help: "Works with almost anything, even old devices. Files up to 4 GB; drives up to 2 TB." },
        { id: "ntfs", name: "NTFS", help: "Windows' own format. Macs can only read it." },
        { id: "ext4", name: "ext4", help: "Linux only. Keeps Linux permissions." },
        { id: "btrfs", name: "Btrfs", help: "Linux only, with snapshots and compression. Needs at least 256 MB." }
    ]
    readonly property var chosen: formats[picker.currentIndex] || formats[0]
    readonly property int labelMax: chosen.id === "vfat" ? 11 : chosen.id === "exfat" ? 15 : chosen.id === "ext4" ? 16 : chosen.id === "ntfs" ? 32 : 64
    readonly property string driveName: disk ? (disk.model || disk.device) : ""
    title: confirming ? "Erase “" + driveName + "”?" : busy ? "Formatting “" + driveName + "”…" : "Format " + (driveName || "Drive")
    message: looking ? "Looking at the drive…"
        : reason !== "" ? reason
        : confirming ? "Everything on " + (disk ? disk.device + " (" + app.size(disk.size) + ")" : "the drive") + " will be erased and can't be brought back. It becomes one " + chosen.name + " volume named “" + nameField.text + "”."
        : busy ? progressText
        : "Erases the whole drive and makes it one empty volume. Copy anything you want to keep off it first."

    // place: a sidebar mount point or a /dev path.
    function start(place) {
        disk = null; reason = ""; looking = true; busy = false; confirming = false
        nameField.text = "UNTITLED"
        picker.currentIndex = 0
        open()
        Async.call(app.grove, k => app.grove.driveLookup(k, place), r => {
            fs.looking = false
            if (r.error) { fs.reason = r.error; return }
            fs.disk = r.disk
            if (!r.canFormat) fs.reason = r.reason
            const p = (r.disk.parts || []).find(x => x.label)
            nameField.text = (p ? p.label : (r.disk.wholeLabel || "UNTITLED")).substring(0, 11)
        })
    }
    Connections {
        target: fs.app ? fs.app.grove : null
        enabled: fs.busy
        function onDiskProgress(json) { const p = JSON.parse(json); fs.progressText = p.text; fs.fraction = p.fraction }
    }

    Row {
        visible: !fs.looking && fs.reason === "" && !fs.busy && !fs.confirming
        width: parent.width
        spacing: 8
        GLabel { text: "Name:"; width: 70; anchors.verticalCenter: parent.verticalCenter; color: Theme.fgSecondary }
        GField {
            id: nameField
            width: parent.width - 78
            maximumLength: fs.labelMax
            placeholderText: "Name"
            tip: "The name the drive shows when it's plugged in (" + fs.chosen.name + ": up to " + fs.labelMax + " characters)"
        }
    }
    Row {
        visible: !fs.looking && fs.reason === "" && !fs.busy && !fs.confirming
        width: parent.width
        spacing: 8
        GLabel { text: "Format:"; width: 70; anchors.verticalCenter: parent.verticalCenter; color: Theme.fgSecondary }
        GCombo {
            id: picker
            width: parent.width - 78
            model: fs.formats.map(f => f.name)
            tip: "How the drive stores files: which computers and devices can use it"
        }
    }
    GLabel {
        visible: !fs.looking && fs.reason === "" && !fs.busy && !fs.confirming
        x: 78; width: parent.width - 78
        text: fs.chosen.help
        wrapMode: Text.Wrap
        color: Theme.fgSecondary
        font.pixelSize: Theme.small
    }
    Rectangle {
        visible: fs.busy
        width: parent.width; height: 6; radius: 3
        color: Theme.field
        Rectangle { width: parent.width * Math.max(0.03, fs.fraction); height: parent.height; radius: 3; color: Theme.accent }
    }

    buttons: [
        PushButton {
            text: fs.confirming ? "Back" : "Cancel"
            enabled: !fs.busy
            tip: fs.confirming ? "Change the name or format" : "Leave the drive as it is"
            onClicked: fs.confirming ? fs.confirming = false : fs.close()
        },
        PushButton {
            visible: fs.reason !== "" && !!fs.disk
            text: "Open Disk Utility"
            tip: "Partition and format this disk in Grove Disk Utility"
            onClicked: { const d = fs.disk; fs.close(); fs.app.openDiskUtility(d ? d.id : "") }
        },
        PushButton {
            visible: fs.reason === ""
            text: fs.confirming ? "Erase" : "Format…"
            primary: !fs.confirming
            destructive: fs.confirming
            enabled: !fs.looking && !fs.busy && !!fs.disk && nameField.text.trim() !== ""
            tip: fs.confirming ? "Erase the drive now (everything on it is lost)" : "Check what will happen, then erase"
            onClicked: fs.confirming ? fs.go() : fs.confirming = true
        }
    ]
    function go() {
        confirming = false; busy = true; fraction = 0; progressText = "Starting…"
        const name = nameField.text.trim(), f = chosen.id
        Async.call(app.grove, k => app.grove.driveFormat(k, disk.id, name, f), r => {
            fs.busy = false
            if (r.error) { fs.reason = "Formatting failed: " + r.error; return }
            fs.close()
            fs.win.toast("info", "“" + name + "” is ready" + (r.mount ? " at " + r.mount : "") + ".")
            if (fs.win.reloadPlaces) fs.win.reloadPlaces()
            if (r.mount) fs.win.go(r.mount)
        })
    }
    // Test hook: name and format, straight to erasing.
    function testGo(name, f) {
        nameField.text = name
        picker.currentIndex = Math.max(0, formats.findIndex(x => x.id === f))
        go()
    }
}
