import QtQuick
import QtQuick.Controls.Basic
import Grove

// A whole text file for Quick Look: code (syntax-coloured), Markdown
// (rendered), an office document's text. TextDoc reads it on a worker
// thread in blocks of lines; the list lays out only the blocks on screen,
// so every line of a huge file can be scrolled to without stalling.
Item {
    id: dv
    property string path: ""
    property string mode: ""        // code, markdown, office, plain
    property bool active: false
    readonly property bool wrap: mode !== "code"
    readonly property alias lines: doc.lines
    readonly property alias loading: doc.loading
    readonly property real charWidth: fm.averageCharacterWidth

    TextDoc {
        id: doc
        family: Theme.family
        pixel: Theme.body
        pageWidth: Math.max(200, dv.width - 64)
    }
    function reload() { doc.load(active ? path : "", mode, Theme.dark) }
    onPathChanged: Qt.callLater(reload)
    onModeChanged: Qt.callLater(reload)
    onActiveChanged: Qt.callLater(reload)
    Connections { target: Theme; function onDarkChanged() { if (dv.active) dv.reload() } }

    FontMetrics { id: fm; font.family: Theme.mono; font.pixelSize: Theme.body * 0.95 }

    // Momentum both ways (the code view scrolls sideways too).
    Kinetic { id: kv; target: list; notch: 60 }
    Kinetic { id: kh; target: list; horizontal: true; notch: 60 }
    WheelHandler {
        target: null
        acceptedDevices: PointerDevice.AllDevices
        onWheel: (e) => { kv.wheel(e); if (!dv.wrap) kh.wheel(e) }
    }

    ListView {
        id: list
        anchors.fill: parent
        clip: true
        interactive: false
        model: doc
        flickableDirection: Flickable.HorizontalAndVerticalFlick
        contentWidth: dv.wrap ? width : Math.max(width, doc.maxCols * fm.averageCharacterWidth + 40)
        cacheBuffer: 800
        header: Item { height: 12; width: 1 }
        footer: Item { height: 16; width: 1 }
        ScrollBar.vertical: GScrollBar { parent: list; x: list.width - width; height: list.height }
        ScrollBar.horizontal: GScrollBar { parent: list; y: list.height - height; width: list.width }
        delegate: TextEdit {
            id: block
            required property var model
            text: block.model.text
            width: dv.wrap ? list.width : list.contentWidth
            readOnly: true
            selectByMouse: true
            textFormat: block.model.rich ? TextEdit.RichText : TextEdit.PlainText
            wrapMode: dv.wrap ? TextEdit.Wrap : TextEdit.NoWrap
            // Pictures in Markdown next to the file.
            baseUrl: dv.mode === "markdown" ? "file://" + dv.path.substring(0, dv.path.lastIndexOf("/") + 1) : ""
            font.family: dv.mode === "code" ? Theme.mono : Theme.family
            font.pixelSize: dv.mode === "code" ? Theme.body * 0.95 : Theme.body
            renderType: Text.NativeRendering
            color: Theme.fg
            selectionColor: Theme.accent
            selectedTextColor: Theme.accentText
            leftPadding: dv.mode === "code" ? 12 : 24
            rightPadding: dv.mode === "code" ? 12 : 24
            onLinkActivated: (link) => Qt.openUrlExternally(link)
            HoverHandler { cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.IBeamCursor }
        }
    }

    GLabel {
        anchors.centerIn: parent
        visible: doc.error !== "" || (doc.loading && list.count === 0)
        text: doc.error !== "" ? doc.error : "Reading…"
        color: Theme.fgSecondary
    }
}
