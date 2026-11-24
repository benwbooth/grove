import QtQuick
import QtQuick.Controls.Basic

// Grove Preview's Adjust Size (Preview's): fit into a common size or set
// width and height in pixels, percent, inches or centimetres, with the
// resolution, keeping the proportions; shows the resulting size.
Sheet {
    id: asz
    property var win
    property var doc: null
    title: "Adjust Size"
    message: "Change how many pixels the picture has, or its resolution for printing."
    property real w0: 1
    property real h0: 1
    property real pw: 1
    property real ph: 1
    property real dpi: 72
    property bool keep: true
    property string unit: "pixels"
    function open2(d) {
        doc = d
        const im = d.img
        const sw = im.crop ? im.crop[2] : im.w, sh = im.crop ? im.crop[3] : im.h
        const side = im.rotate % 180 !== 0
        w0 = side ? sh : sw; h0 = side ? sw : sh
        pw = im.resize ? im.resize[0] : w0
        ph = im.resize ? im.resize[1] : h0
        dpi = im.dpi || 72
        unit = "pixels"
        open()
    }
    function toUnit(px) {
        if (unit === "percent") return Math.round(px / (wField.isW ? w0 : h0) * 1000) / 10
        if (unit === "inches") return Math.round(px / dpi * 100) / 100
        if (unit === "cm") return Math.round(px / dpi * 254) / 100
        return Math.round(px)
    }
    function fromUnit(v, base) {
        if (unit === "percent") return base * v / 100
        if (unit === "inches") return v * dpi
        if (unit === "cm") return v / 2.54 * dpi
        return v
    }
    function setW(px) { pw = Math.max(1, Math.round(px)); if (keep) ph = Math.max(1, Math.round(pw * h0 / w0)) }
    function setH(px) { ph = Math.max(1, Math.round(px)); if (keep) pw = Math.max(1, Math.round(ph * w0 / h0)) }

    Row {
        spacing: 8
        GLabel { text: "Fit into"; width: 90; anchors.verticalCenter: parent.verticalCenter }
        GCombo {
            width: 220
            model: ["Custom", "320 × 240", "640 × 480", "800 × 600", "1024 × 768", "1280 × 1024", "1920 × 1080", "2560 × 1440", "3840 × 2160"]
            tip: "Shrink or grow the picture to fit inside this many pixels"
            onActivated: (i) => {
                if (i === 0) return
                const [mw, mh] = model[i].split(" × ").map(Number)
                const s = Math.min(mw / asz.w0, mh / asz.h0)
                asz.pw = Math.round(asz.w0 * s); asz.ph = Math.round(asz.h0 * s)
            }
        }
    }
    Row {
        spacing: 8
        GLabel { text: "Width"; width: 90; anchors.verticalCenter: parent.verticalCenter }
        GField { id: wField; property bool isW: true; width: 110; text: String(asz.toUnit(asz.pw)); tip: "The new width"; validator: DoubleValidator { bottom: 0.01 }
            onEditingFinished: asz.setW(asz.fromUnit(Number(text), asz.w0)) }
        GCombo { width: 110; model: ["pixels", "percent", "inches", "cm"]; currentIndex: model.indexOf(asz.unit); tip: "The unit for width and height"; onActivated: (i) => asz.unit = model[i] }
    }
    Row {
        spacing: 8
        GLabel { text: "Height"; width: 90; anchors.verticalCenter: parent.verticalCenter }
        GField { property bool isW: false; width: 110; text: String(asz.toUnit(asz.ph)); tip: "The new height"; validator: DoubleValidator { bottom: 0.01 }
            onEditingFinished: asz.setH(asz.fromUnit(Number(text), asz.h0)) }
    }
    Row {
        spacing: 8
        GLabel { text: "Resolution"; width: 90; anchors.verticalCenter: parent.verticalCenter }
        GField { width: 110; text: String(asz.dpi); tip: "Pixels per inch: how big it prints (the pixels stay the same)"; validator: IntValidator { bottom: 1; top: 10000 }
            onEditingFinished: asz.dpi = Math.max(1, Number(text)) }
        GLabel { text: "pixels/inch"; anchors.verticalCenter: parent.verticalCenter; color: Theme.fgSecondary }
    }
    GCheck { text: "Scale proportionally"; checked: asz.keep; tip: "Keep the width and height in step"; onToggled: { asz.keep = checked; if (checked) asz.setW(asz.pw) } }
    GLabel {
        width: parent.width
        wrapMode: Text.Wrap
        color: Theme.fgSecondary
        text: "Now " + Math.round(asz.w0) + " × " + Math.round(asz.h0) + " pixels; becomes " + asz.pw + " × " + asz.ph + " pixels (" + Math.round(asz.pw * asz.ph / Math.max(1, asz.w0 * asz.h0) * 100) + "%), " + (asz.pw / asz.dpi).toFixed(2) + " × " + (asz.ph / asz.dpi).toFixed(2) + " inches when printed."
    }
    buttons: [
        PushButton { text: "Cancel"; tip: "Leave the size as it was"; onClicked: asz.close() },
        PushButton { text: "OK"; primary: true; tip: "Use this size (the picture changes when you save)"; onClicked: {
            asz.win.changeImage(asz.doc, { resize: (asz.pw === Math.round(asz.w0) && asz.ph === Math.round(asz.h0)) ? null : [asz.pw, asz.ph], dpi: asz.dpi }, "Adjust Size")
            asz.close()
        } }
    ]
}
