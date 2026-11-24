import QtQuick
import QtQuick.Controls.Basic

// Grove Preview's Print: the printer, copies, which pages, paper size,
// orientation, two-sided, colour or black and white, scale to fit, with a
// picture of the page. Printed through CUPS with the changes made so far.
Sheet {
    id: ps
    property var win
    property var doc: null
    property var printers: []
    property bool loading: false
    title: "Print"
    message: doc ? "“" + doc.name + "”" + (doc.kind === "pdf" ? ", " + doc.pages.length + (doc.pages.length === 1 ? " page" : " pages") : "") : ""
    width: Math.min(parent ? parent.width - 40 : 560, Math.round(560 * Theme.pt))
    readonly property var printer: printers.length ? printers[Math.max(0, printerBox.currentIndex)] : null
    function open2(d) {
        doc = d
        loading = true
        open()
        Async.call(win.app.grove, k => win.app.grove.pvPrinters(k), r => {
            ps.loading = false
            ps.printers = Array.isArray(r) ? r : []
            const i = ps.printers.findIndex(p => p.default)
            printerBox.currentIndex = i >= 0 ? i : 0
        })
    }

    Row {
        width: parent.width
        spacing: 16
        // The page to print, small.
        Rectangle {
            width: 150; height: 190
            color: Theme.field; radius: 6
            PvThumb {
                anchors.centerIn: parent
                win: ps.win
                doc: ps.doc
                pg: ps.doc && ps.doc.kind === "pdf" ? ps.doc.pages[Math.min(ps.win.currentPage, ps.doc.pages.length - 1)] : null
                box: 136
                visible: !!ps.doc && ps.visible
            }
        }
        Column {
            width: parent.width - 166
            spacing: 8
            Row {
                spacing: 8
                GLabel { text: "Printer"; width: 80; anchors.verticalCenter: parent.verticalCenter }
                GCombo {
                    id: printerBox
                    width: 220
                    model: ps.printers.map(p => (p.info || p.name) + (p.state === 5 ? " (stopped)" : ""))
                    tip: "Where to print"
                    displayText: ps.loading ? "Looking for printers…" : ps.printers.length ? currentText : "No printers"
                }
            }
            Row {
                spacing: 8
                GLabel { text: "Copies"; width: 80; anchors.verticalCenter: parent.verticalCenter }
                SpinBox { id: copies; from: 1; to: 999; value: 1; editable: true }
            }
            Row {
                visible: !!ps.doc && ps.doc.kind === "pdf"
                spacing: 8
                GLabel { text: "Pages"; width: 80; anchors.verticalCenter: parent.verticalCenter }
                GCombo { id: which; width: 140; model: ["All", "Current page", "Range"]; tip: "Which pages to print" }
                GField { id: range; visible: which.currentIndex === 2; width: 110; placeholderText: "1-3, 5"; tip: "Pages like 1-3, 5, 8-10" }
            }
            Row {
                spacing: 8
                GLabel { text: "Paper"; width: 80; anchors.verticalCenter: parent.verticalCenter }
                GCombo {
                    id: media
                    width: 220
                    readonly property var list: ps.printer ? ps.printer.media || [] : []
                    model: list.map(m => ps.mediaName(m))
                    currentIndex: Math.max(0, list.findIndex(m => ps.printer && m.name === ps.printer.defaultMedia))
                    tip: "The paper size"
                }
            }
            Row {
                spacing: 8
                GLabel { text: "Layout"; width: 80; anchors.verticalCenter: parent.verticalCenter }
                GCombo { id: orient; width: 140; model: ["Automatic", "Portrait", "Landscape"]; tip: "Which way up the page goes on the paper" }
                GCombo { id: perSheet; width: 90; model: ["1", "2", "4", "6", "9", "16"]; tip: "Pages on each sheet of paper" }
            }
            GCheck { id: duplex; text: "Two-sided"; enabled: !!ps.printer && ps.printer.duplex; tip: "Print on both sides of the paper (long-edge binding)" }
            GCheck { id: gray; text: "Black and white"; enabled: !!ps.printer && ps.printer.color; tip: "Print in shades of grey" }
            GCheck { id: fit; text: "Scale to fit the paper"; checked: true; tip: "Shrink or grow the page to fill the paper" }
        }
    }
    function mediaName(m) {
        const names = { "na_letter_8.5x11in": "US Letter", "na_legal_8.5x14in": "US Legal", "iso_a4_210x297mm": "A4", "iso_a5_148x210mm": "A5", "iso_a3_297x420mm": "A3", "na_index-4x6_4x6in": "4 × 6 in photo" }
        return names[m.name] || m.name
    }
    buttons: [
        PushButton { text: "Cancel"; tip: "Don't print"; onClicked: ps.close() },
        PushButton {
            text: "Print"
            primary: true
            enabled: !!ps.printer
            tip: "Send it to the printer"
            onClicked: {
                const o = { copies: String(copies.value) }
                if (ps.doc.kind === "pdf") {
                    if (which.currentIndex === 1) o["page-ranges"] = String(ps.win.currentPage + 1)
                    else if (which.currentIndex === 2 && range.text.trim()) o["page-ranges"] = range.text.replace(/\s+/g, "")
                }
                if (media.list.length) o.media = media.list[media.currentIndex].name
                if (orient.currentIndex === 1) o["orientation-requested"] = "3"
                if (orient.currentIndex === 2) o["orientation-requested"] = "4"
                if (perSheet.currentIndex > 0) o["number-up"] = perSheet.currentText
                o.sides = duplex.checked ? "two-sided-long-edge" : "one-sided"
                if (gray.checked) o["print-color-mode"] = "monochrome"
                if (fit.checked) o["print-scaling"] = "fit"
                ps.win.printTo(ps.doc, ps.printer.name, o)
                ps.close()
            }
        }
    ]
}
