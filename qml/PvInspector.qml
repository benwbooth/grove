import QtQuick
import QtQuick.Controls.Basic

// Grove Preview's Inspector (Tools › Show Inspector): what the file is —
// size, kind, pages or pixels, resolution, dates — the PDF's title,
// author, subject and keywords (editable, saved with the document),
// whether it's encrypted, and the picture's own details.
Popup {
    id: ins
    property var win
    readonly property var doc: win.cur
    readonly property var i: doc && doc.info ? doc.info : ({})
    property var file: ({})
    width: Math.round(320 * Theme.pt)
    padding: 14
    topPadding: 14.001
    leftPadding: 14.001
    modal: false
    closePolicy: Popup.CloseOnEscape
    background: GlassPanel { tint: Theme.glassMenu; radius: Theme.windowRadius }
    onOpened: load()
    onDocChanged: if (visible) load()
    function load() {
        file = {}
        if (!doc) return
        const p = doc.path
        Async.call(win.app.grove, k => win.app.grove.info(k, p), r => { if (ins.doc && ins.doc.path === p) ins.file = r || {} })
    }
    function size(n) {
        if (!n) return ""
        const u = ["bytes", "KB", "MB", "GB"]
        let i = 0
        while (n >= 1000 && i < 3) { n /= 1000; i++ }
        return (i ? n.toFixed(1) : n) + " " + u[i]
    }
    component Line: Row {
        property string k: ""
        property string v: ""
        visible: v !== ""
        spacing: 8
        GLabel { text: parent.k; width: 90; color: Theme.fgSecondary; horizontalAlignment: Text.AlignRight }
        GLabel { text: parent.v; width: ins.width - 140; wrapMode: Text.Wrap; textFormat: Text.PlainText }
    }
    component Meta: Row {
        property string k: ""
        property string key: ""
        spacing: 8
        GLabel { text: parent.k; width: 90; color: Theme.fgSecondary; horizontalAlignment: Text.AlignRight; anchors.verticalCenter: parent.verticalCenter }
        GField {
            width: ins.width - 140
            text: ins.doc ? (ins.doc.meta && ins.doc.meta[parent.key] !== undefined ? ins.doc.meta[parent.key] : (ins.i[parent.key.toLowerCase()] || "")) : ""
            tip: "The document's " + parent.k.toLowerCase() + " (saved with the PDF)"
            onEditingFinished: ins.win.setMeta(ins.doc, parent.key, text)
        }
    }
    contentItem: Column {
        spacing: 6
        Item {
            width: parent.width; height: 26
            GLabel { text: ins.doc ? ins.doc.name : "Inspector"; font.bold: true; font.pixelSize: Theme.title; width: parent.width - 30; elide: Text.ElideMiddle; anchors.verticalCenter: parent.verticalCenter }
            GButton { anchors.right: parent.right; iconName: "window-close"; iconSize: 12; tip: "Close (Esc)"; onClicked: ins.close() }
        }
        Line { k: "Where"; v: ins.doc ? ins.doc.path.substring(0, ins.doc.path.lastIndexOf("/")) : "" }
        Line { k: "Kind"; v: ins.doc ? (ins.doc.kind === "pdf" ? (ins.i.format || "PDF document") : (ins.i.format || "Picture") + " picture") : "" }
        Line { k: "Size"; v: ins.size(ins.file.size || ins.i.bytes || 0) }
        Line { k: "Pages"; v: ins.doc && ins.doc.kind === "pdf" ? String(ins.doc.pages.length) : "" }
        Line { k: "Page size"; v: ins.doc && ins.doc.kind === "pdf" && ins.doc.pages.length ? (ins.doc.pages[0].w / 72).toFixed(2) + " × " + (ins.doc.pages[0].h / 72).toFixed(2) + " in (" + Math.round(ins.doc.pages[0].w / 72 * 25.4) + " × " + Math.round(ins.doc.pages[0].h / 72 * 25.4) + " mm)" : "" }
        Line { k: "Dimensions"; v: ins.doc && ins.doc.kind === "image" ? ins.i.w + " × " + ins.i.h + " pixels" : "" }
        Line { k: "Transparency"; v: ins.doc && ins.doc.kind === "image" ? (ins.i.alpha ? "Yes" : "No") : "" }
        Line { k: "Modified"; v: ins.file.modifiedText || ins.file.modified || "" }
        Line { k: "Encrypted"; v: ins.doc && ins.doc.kind === "pdf" ? (ins.i.encrypted ? (ins.i.encryption || "Yes") : "No") : "" }
        Line { k: "Producer"; v: ins.i.producer || "" }
        Line { k: "Creator"; v: ins.i.creator || "" }
        Column {
            visible: !!ins.doc && ins.doc.kind === "pdf"
            spacing: 6
            Meta { k: "Title"; key: "Title" }
            Meta { k: "Author"; key: "Author" }
            Meta { k: "Subject"; key: "Subject" }
            Meta { k: "Keywords"; key: "Keywords" }
        }
    }
}
