import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window

// Add to Archive (7-Zip's dialog, grove's look): name and folder, format,
// compression level and method, solid blocks and threads (7z), split into
// volumes, password with encrypted file names (7z) or AES-256/ZipCrypto
// (zip), and moving the originals to the Trash afterwards.
Window {
    id: cd
    property var app
    property var archiver
    property var items: []
    readonly property var s: app.settings
    width: Math.round(560 * Theme.pt)
    height: col.implicitHeight + 32
    minimumWidth: 480
    color: Theme.window
    title: "Add to Archive"

    function base(p) { return p.substring(p.lastIndexOf("/") + 1) }
    function dirOf(p) { return p.substring(0, p.lastIndexOf("/")) || "/" }
    readonly property string firstDir: items.length ? dirOf(items[0]) : app.grove.home()
    readonly property string stem: items.length === 1 ? base(items[0]) : (items.length ? base(firstDir) || "Archive" : "Archive")

    readonly property var formats: [
        { v: "7z", t: "7z", tip: "Best compression; passwords can hide the file names too. Opens with 7-Zip, Keka, The Unarchiver, grove" },
        { v: "zip", t: "zip", tip: "Opens everywhere (Windows, macOS, phones) without extra apps" },
        { v: "tar.zst", t: "tar.zst", tip: "Fast and small; keeps Linux permissions. Common on Linux" },
        { v: "tar.xz", t: "tar.xz", tip: "Small; keeps Linux permissions. Slow to make" },
        { v: "tar.gz", t: "tar.gz", tip: "The classic Linux archive; opens nearly everywhere" },
        { v: "tar.bz2", t: "tar.bz2", tip: "Older Linux archive" },
        { v: "tar", t: "tar", tip: "No compression; keeps Linux permissions" },
        { v: "zst", t: "zst (one file)", tip: "Compresses a single file, no archive around it" },
        { v: "xz", t: "xz (one file)", tip: "Compresses a single file, no archive around it" },
        { v: "gz", t: "gz (one file)", tip: "Compresses a single file, no archive around it" },
        { v: "bz2", t: "bz2 (one file)", tip: "Compresses a single file, no archive around it" }]
    readonly property var levels: [
        { v: 0, t: "Store (no compression)" }, { v: 1, t: "Fastest" }, { v: 3, t: "Fast" },
        { v: 5, t: "Normal" }, { v: 7, t: "Maximum" }, { v: 9, t: "Ultra" }]
    readonly property var methods: ({
        "7z": [{ v: "lzma2", t: "LZMA2" }, { v: "lzma", t: "LZMA" }, { v: "ppmd", t: "PPMd (text)" }, { v: "bzip2", t: "BZip2" }],
        "zip": [{ v: "deflate", t: "Deflate" }, { v: "zstd", t: "Zstandard" }, { v: "xz", t: "XZ" }, { v: "lzma", t: "LZMA" }, { v: "bzip2", t: "BZip2" }]
    })
    readonly property var volumeSizes: [
        { v: 0, t: "Don't split" }, { v: 10e6, t: "10 MB" }, { v: 100e6, t: "100 MB" }, { v: 650e6, t: "650 MB (CD)" },
        { v: 700e6, t: "700 MB (CD)" }, { v: 4092e6, t: "4092 MB (FAT32 drive)" }, { v: 4480e6, t: "4480 MB (DVD)" },
        { v: 8128e6, t: "8128 MB (DVD DL)" }, { v: 23040e6, t: "23040 MB (Blu-ray)" }, { v: -1, t: "Custom…" }]

    readonly property string fmt: formats[fmtBox.currentIndex].v
    readonly property bool canEncrypt: fmt === "7z" || fmt === "zip"
    readonly property bool single: ["zst", "xz", "gz", "bz2"].indexOf(fmt) >= 0
    readonly property int level: levels[levelBox.currentIndex].v
    readonly property real volumeSize: {
        const v = volumeSizes[volBox.currentIndex].v
        if (v >= 0) return v
        const n = parseFloat(customVol.text)
        return isFinite(n) && n > 0 ? n * 1e6 : 0
    }
    readonly property string outPath: folder.text.replace(/\/$/, "") + "/" + nameField.text + "." + fmt
    readonly property bool pwOk: !pw1.text || pw1.text === pw2.text || showPw.checked

    function go() {
        if (!nameField.text || !pwOk || (single && items.length !== 1)) return
        const a = {
            items: items, out: outPath, format: fmt, level: level,
            method: methods[fmt] ? methods[fmt][methodBox.currentIndex].v : "",
            password: canEncrypt ? pw1.text : "", encryptNames: fmt === "7z" && namesBox.checked,
            zipEncryption: zipEncBox.currentIndex === 1 ? "zipcrypto" : "aes256",
            solid: solidBox.checked, threads: threadBox.currentIndex, volumeSize: Math.round(volumeSize)
        }
        const trashAfter = trashBox.checked, reveal = !!s.archive_reveal, its = items, dir = folder.text
        archiver.uniqueOut(a.out, out => {
            a.out = out
            archiver.run("create", a, r => {
                if (trashAfter) Async.call(app.grove, k => app.grove.trashPaths(k, JSON.stringify(its)), x => app.refreshTrash())
                if (reveal && r.files && r.files.length) app.openOrTab([dir], r.files)
            })
            // Only now: closing the last window first would quit the app.
            cd.close()
        })
        cd.visible = false
    }

    Shortcut { sequence: "Escape"; onActivated: cd.close() }

    component Label2: GLabel { width: 140; horizontalAlignment: Text.AlignRight; color: Theme.fgSecondary; height: Theme.controlHeight }
    component Sep: Rectangle { width: col.width; height: 1; color: Theme.separator }

    Column {
        id: col
        x: 16; y: 16
        width: cd.width - 32
        spacing: 9
        transform: Translate { x: 0.001; y: 0.001 }

        Row {
            spacing: 12
            GIcon { size: 40; name: "package-x-generic|archive-insert" }
            Column {
                width: col.width - 52
                GLabel { text: cd.items.length === 1 ? "Compress “" + cd.base(cd.items[0]) + "”" : "Compress " + cd.items.length + " items"; font.bold: true; width: parent.width; elide: Text.ElideMiddle }
                GLabel { text: "Into a new archive"; font.pixelSize: Theme.small; color: Theme.fgSecondary }
            }
        }
        Row {
            spacing: 8
            Label2 { text: "Name" }
            GField { id: nameField; width: col.width - 148 - 80; text: cd.stem; tip: "The archive's name (the extension is added for you)"; onAccepted: cd.go() }
            GLabel { text: "." + cd.fmt; color: Theme.fgSecondary; height: Theme.controlHeight }
        }
        Row {
            spacing: 8
            Label2 { text: "Where" }
            GField { id: folder; width: col.width - 148 - 98; text: cd.firstDir; tip: "The folder the archive is made in" }
            PushButton { text: "Choose…"; width: 90; tip: "Pick a folder"
                onClicked: cd.archiver.choose("Save Archive In", { directory: true, currentFolder: folder.text, acceptLabel: "Choose" }, ps => { if (ps && ps.length) folder.text = ps[0] }) }
        }
        Sep {}
        Row {
            spacing: 8
            Label2 { text: "Format" }
            GCombo {
                id: fmtBox; width: 200
                model: cd.formats.map(f => f.t)
                currentIndex: Math.max(0, cd.formats.findIndex(f => f.v === (cd.s.archive_format || "7z")))
                tip: cd.formats[currentIndex].tip
            }
        }
        GLabel { x: 148; visible: cd.single && cd.items.length !== 1; color: Theme.danger; font.pixelSize: Theme.small
                 text: "This format holds one file. Pick 7z, zip or a tar format for folders or several files." }
        Row {
            spacing: 8
            Label2 { text: "Compression" }
            GCombo {
                id: levelBox; width: 200
                model: cd.levels.map(l => l.t)
                currentIndex: Math.max(0, cd.levels.findIndex(l => l.v === (cd.s.archive_level !== undefined ? cd.s.archive_level : 5)))
                enabled: cd.fmt !== "tar"
                tip: "Higher is smaller but slower. Store just packs the files together"
            }
        }
        Row {
            spacing: 8
            visible: !!cd.methods[cd.fmt]
            Label2 { text: "Method" }
            GCombo {
                id: methodBox; width: 200
                model: (cd.methods[cd.fmt] || []).map(m => m.t)
                enabled: cd.level > 0
                tip: cd.fmt === "7z" ? "LZMA2 is 7-Zip's default and uses every core. PPMd is best for plain text" : "Deflate opens everywhere; the others are smaller but need a newer unzipper"
            }
        }
        Row {
            spacing: 8
            visible: cd.fmt === "7z" || cd.fmt.indexOf("tar.") === 0 && (cd.fmt === "tar.xz" || cd.fmt === "tar.zst")
            Label2 { text: "Processor cores" }
            GCombo {
                id: threadBox; width: 200
                model: { const m = ["All"]; for (let i = 1; i <= 16; i++) m.push(String(i)); return m }
                tip: "How many cores compress at once. Fewer keeps the computer quicker meanwhile"
            }
        }
        GCheck {
            id: solidBox; x: 148
            visible: cd.fmt === "7z"; checked: true
            text: "Solid (files compressed together)"
            tip: "Smaller archives, especially with many small files; taking out one file means reading what's before it"
        }
        Row {
            spacing: 8
            visible: !cd.single
            Label2 { text: "Split into volumes" }
            GCombo { id: volBox; width: 200; model: cd.volumeSizes.map(v => v.t)
                     tip: "Several files of this size (name.7z.001, .002…), to fit drives, discs or mail limits. Open the .001 to read it all" }
            GField { id: customVol; visible: cd.volumeSizes[volBox.currentIndex].v < 0; width: 90; placeholderText: "MB"; tip: "Size of each volume in megabytes"
                     validator: DoubleValidator { bottom: 0.1 } }
        }
        Sep { visible: cd.canEncrypt }
        Row {
            spacing: 8
            visible: cd.canEncrypt
            Label2 { text: "Password" }
            GField { id: pw1; width: 200; echoMode: showPw.checked ? TextInput.Normal : TextInput.Password; placeholderText: "None"; tip: "Anyone opening the archive will need this. It can't be recovered if you forget it" }
            GCheck { id: showPw; text: "Show"; tip: "Show the password as you type (then it needn't be typed twice)" }
        }
        Row {
            spacing: 8
            visible: cd.canEncrypt && !showPw.checked && pw1.text.length > 0
            Label2 { text: "Again" }
            GField { id: pw2; width: 200; echoMode: TextInput.Password; tip: "Type the password again to be sure" }
            GLabel { visible: pw2.text.length > 0 && !cd.pwOk; text: "Doesn't match"; color: Theme.danger; height: Theme.controlHeight; font.pixelSize: Theme.small }
        }
        GCheck {
            id: namesBox; x: 148
            visible: cd.fmt === "7z" && pw1.text.length > 0; checked: true
            text: "Hide the file names too"
            tip: "Without the password nobody can even see what's inside (7-Zip's Encrypt file names)"
        }
        Row {
            spacing: 8
            visible: cd.fmt === "zip" && pw1.text.length > 0
            Label2 { text: "Encryption" }
            GCombo { id: zipEncBox; width: 200; model: ["AES-256", "ZipCrypto"]
                     tip: "AES-256 is secure; ZipCrypto is weak but opens in Windows' own Explorer" }
        }
        Sep {}
        GCheck { id: trashBox; x: 148; text: "Move the originals to the Trash afterwards"
                 tip: "Once the archive is made. You can get them back from the Trash" }
        Row {
            anchors.right: parent.right
            spacing: 8
            PushButton { text: "Cancel"; tip: "Close without making an archive"; onClicked: cd.close() }
            PushButton { text: "Compress"; primary: true; enabled: nameField.text.length > 0 && cd.pwOk && !(cd.single && cd.items.length !== 1)
                         tip: "Make the archive (Return)"; onClicked: cd.go() }
        }
    }

    // Tests: GROVE_ARC_TEST "fmt=zip;pw=x;ok" fills the dialog and presses Compress.
    Timer {
        interval: 600
        running: (cd.app.grove.env("GROVE_ARC_TEST") || "") !== ""
        onTriggered: {
            for (const st of cd.app.grove.env("GROVE_ARC_TEST").split(";")) {
                const [k, v] = st.split("=")
                if (k === "fmt") fmtBox.currentIndex = cd.formats.findIndex(f => f.v === v)
                else if (k === "pw") { pw1.text = v; showPw.checked = true }
                else if (k === "level") levelBox.currentIndex = cd.levels.findIndex(l => String(l.v) === v)
                else if (k === "vol") { volBox.currentIndex = cd.volumeSizes.length - 1; customVol.text = v }
                else if (k === "name") nameField.text = v
            }
            if (cd.app.grove.env("GROVE_ARC_TEST").indexOf("ok") >= 0) cd.go()
        }
    }
}
