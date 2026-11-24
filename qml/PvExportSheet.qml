import QtQuick
import QtQuick.Controls.Basic

// Grove Preview's Export (Preview's File › Export…): the format (PDF,
// PNG, JPEG with its quality, WebP, TIFF, BMP, GIF), the resolution when a
// PDF page becomes a picture, and a password for PDFs (with or without
// printing and copying), then where to save it.
Sheet {
    id: ex
    property var win
    property var doc: null
    property bool pdfOnly: false        // Export as PDF…
    title: pdfOnly ? "Export as PDF" : "Export"
    message: doc ? (doc.kind === "pdf" && format !== "pdf" ? "The page you're on is saved as a picture." : "“" + doc.name + "” with your changes, as a new file.") : ""
    property string format: "pdf"
    property int quality: 85
    property int dpi: 150
    property bool encrypt: false
    readonly property var formats: doc && doc.kind === "pdf" ? (pdfOnly ? ["pdf"] : ["pdf", "png", "jpeg", "tiff", "webp"]) : (pdfOnly ? ["pdf"] : ["png", "jpeg", "webp", "tiff", "bmp", "gif", "pdf"])
    readonly property var names: ({ pdf: "PDF", png: "PNG", jpeg: "JPEG", tiff: "TIFF", webp: "WebP", bmp: "BMP", gif: "GIF" })
    function open2(d, onlyPdf) {
        doc = d
        pdfOnly = !!onlyPdf
        format = onlyPdf ? "pdf" : (d.kind === "pdf" ? "pdf" : (d.path.toLowerCase().match(/\.(jpe?g)$/) ? "jpeg" : d.path.toLowerCase().match(/\.(png|webp|tiff?|bmp|gif)$/) ? d.path.toLowerCase().match(/\.(png|webp|tiff?|bmp|gif)$/)[1].replace("tif", "tiff").replace("tifff", "tiff") : "png"))
        encrypt = false
        pw1.text = ""; pw2.text = ""
        open()
    }

    Row {
        spacing: 8
        GLabel { text: "Format"; width: 90; anchors.verticalCenter: parent.verticalCenter }
        GCombo {
            width: 200
            model: ex.formats.map(f => ex.names[f])
            currentIndex: Math.max(0, ex.formats.indexOf(ex.format))
            tip: "The kind of file to make"
            onActivated: (i) => ex.format = ex.formats[i]
        }
    }
    Row {
        visible: ex.format === "jpeg"
        spacing: 8
        GLabel { text: "Quality"; width: 90; anchors.verticalCenter: parent.verticalCenter }
        Slider { id: q; width: 180; from: 10; to: 100; stepSize: 1; value: ex.quality; onMoved: ex.quality = value
            ToolTip.visible: hovered; ToolTip.delay: Theme.tooltipDelay; ToolTip.text: "Higher: better looking, bigger file" }
        GLabel { text: ex.quality < 40 ? "Least" : ex.quality < 70 ? "Low" : ex.quality < 90 ? "High" : "Best"; color: Theme.fgSecondary; anchors.verticalCenter: parent.verticalCenter }
    }
    Row {
        visible: !!ex.doc && ex.doc.kind === "pdf" && ex.format !== "pdf"
        spacing: 8
        GLabel { text: "Resolution"; width: 90; anchors.verticalCenter: parent.verticalCenter }
        GCombo { width: 140; model: ["72", "150", "300", "600"]; currentIndex: model.indexOf(String(ex.dpi)); tip: "Pixels per inch of the picture made from the page"; onActivated: (i) => ex.dpi = Number(model[i]) }
        GLabel { text: "pixels/inch"; color: Theme.fgSecondary; anchors.verticalCenter: parent.verticalCenter }
    }
    Column {
        visible: ex.format === "pdf"
        width: parent.width
        spacing: 6
        GCheck { text: "Encrypt with a password"; checked: ex.encrypt; tip: "The PDF opens only with this password (AES-256)"; onToggled: ex.encrypt = checked }
        GField { id: pw1; visible: ex.encrypt; width: parent.width; echoMode: TextInput.Password; placeholderText: "Password"; tip: "The password needed to open it" }
        GField { id: pw2; visible: ex.encrypt; width: parent.width; echoMode: TextInput.Password; placeholderText: "Verify password"; tip: "The same password again" }
        GCheck { id: allowPrint; visible: ex.encrypt; text: "Allow printing"; checked: true; tip: "Whoever opens it may print it" }
        GCheck { id: allowCopy; visible: ex.encrypt; text: "Allow copying text and pictures"; checked: true; tip: "Whoever opens it may copy from it" }
        GLabel { visible: ex.encrypt && pw1.text !== pw2.text && pw2.text !== ""; text: "The passwords don't match."; color: Theme.danger }
    }
    buttons: [
        PushButton { text: "Cancel"; tip: "Don't export"; onClicked: ex.close() },
        PushButton {
            text: "Save…"
            primary: true
            enabled: !ex.encrypt || (pw1.text !== "" && pw1.text === pw2.text)
            tip: "Choose the name and folder"
            onClicked: {
                const enc = ex.encrypt ? { user: pw1.text, owner: pw1.text, print: allowPrint.checked, copy: allowCopy.checked } : null
                ex.win.exportTo(ex.doc, ex.format, ex.quality, ex.dpi, enc)
                ex.close()
            }
        }
    ]
}
