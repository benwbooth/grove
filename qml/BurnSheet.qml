import QtQuick
import QtQuick.Controls.Basic

// Burn to Disc (Finder's File › Burn): pick a burner (or save a disc
// image file instead), name the disc, burn. Progress shows with the other
// jobs. Without a burner the sheet says so and offers the image file.
Sheet {
    id: bs
    property var app
    property var win
    property var paths: []
    property var drives: []
    property bool looked: false
    readonly property bool isoChoice: picker.currentIndex >= drives.length
    readonly property string what: paths.length === 1 ? "“" + paths[0].substring(paths[0].lastIndexOf("/") + 1) + "”" : paths.length + " items"
    title: "Burn " + what + " to Disc"
    message: looked && drives.length === 0
        ? "There's no disc burner connected to this computer. Grove can save a disc image (.iso) instead: burn it later on another computer, or open it in a virtual machine."
        : "Insert a blank or rewritable disc. A folder's contents go at the top of the disc."

    function start(list) {
        paths = list
        looked = false
        drives = []
        const base = list.length === 1 ? list[0].substring(list[0].lastIndexOf("/") + 1).replace(/\.[^.]*$/, "") : "Disc"
        volField.text = base
        isoField.text = app.grove.home() + "/" + base + ".iso"
        open()
        Async.call(app.grove, k => app.grove.opticalDrives(k), r => { bs.drives = r || []; bs.looked = true; picker.currentIndex = 0 })
    }

    Row {
        width: parent.width
        spacing: 8
        GLabel { text: "Burner:"; width: 90; anchors.verticalCenter: parent.verticalCenter; color: Theme.fgSecondary }
        GCombo {
            id: picker
            width: parent.width - 98
            model: bs.drives.map(d => d.name + " (" + d.dev + ")" + (d.canWrite ? "" : " — reads only")).concat(["Disc image file (.iso)"])
            tip: "Which drive to burn with, or a disc image file"
        }
    }
    Row {
        width: parent.width
        spacing: 8
        GLabel { text: "Disc name:"; width: 90; anchors.verticalCenter: parent.verticalCenter; color: Theme.fgSecondary }
        GField { id: volField; width: parent.width - 98; placeholderText: "Disc name"; tip: "The name the disc shows when it's inserted (capitals, digits and _ only; others become _)" }
    }
    Row {
        visible: bs.isoChoice
        width: parent.width
        spacing: 8
        GLabel { text: "Save as:"; width: 90; anchors.verticalCenter: parent.verticalCenter; color: Theme.fgSecondary }
        GField { id: isoField; width: parent.width - 98; tip: "Where to save the disc image" }
    }
    Column {
        visible: !bs.isoChoice
        spacing: 4
        GCheck { id: ejectBox; text: "Eject the disc when it's done"; checked: true; tip: "Push the disc out after burning" }
        GCheck { id: simBox; text: "Test only (nothing is written)"; tip: "The drive goes through the burn without writing, to check it would work" }
    }
    buttons: [
        PushButton { text: "Cancel"; onClicked: bs.close() },
        PushButton {
            text: bs.isoChoice ? "Save Image" : "Burn"
            primary: true
            enabled: bs.looked && bs.paths.length > 0 && (bs.isoChoice ? isoField.text !== "" : bs.drives[picker.currentIndex] && bs.drives[picker.currentIndex].canWrite)
            tip: bs.isoChoice ? "Write a disc image file with these items" : "Burn these items to the disc in the drive"
            onClicked: bs.go()
        }
    ]
    // Test hook: always the image file, never a real drive.
    function goImage() { picker.currentIndex = drives.length; go() }
    function go() {
                const req = { paths: bs.paths, volid: volField.text, eject: ejectBox.checked, simulate: simBox.checked,
                              device: bs.isoChoice ? "" : bs.drives[picker.currentIndex].dev,
                              iso: bs.isoChoice ? isoField.text.replace(/^~/, bs.app.grove.home()) : "",
                              dry_run: bs.app.grove.env("GROVE_BURN_DRY_RUN") === "1" }
                bs.app.grove.burn(JSON.stringify(req))
                bs.close()
    }
}
