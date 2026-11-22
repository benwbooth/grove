import QtQuick
import QtQuick.Controls.Basic
import QtCore
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
    // Viewing choices, remembered: soft wrap for text and code, and
    // Markdown as its coloured source instead of the rendered page.
    Settings { id: prefs; category: "viewer"; property bool wrapText: false; property bool markdownSource: false }
    readonly property bool rendered: mode === "markdown" && !prefs.markdownSource
    readonly property string shownMode: mode === "markdown" && prefs.markdownSource ? "code" : mode
    readonly property bool wrap: rendered || prefs.wrapText
    readonly property alias lines: doc.lines
    readonly property alias loading: doc.loading
    readonly property real charWidth: fm.averageCharacterWidth

    TextDoc {
        id: doc
        family: Theme.family
        pixel: Theme.body
        pageWidth: Math.max(200, dv.width - 64)
    }
    function reload() { doc.load(active ? path : "", shownMode, Theme.dark) }
    onShownModeChanged: Qt.callLater(reload)
    onPathChanged: Qt.callLater(reload)
    onModeChanged: Qt.callLater(reload)
    onActiveChanged: Qt.callLater(reload)
    Connections { target: Theme; function onDarkChanged() { if (dv.active) dv.reload() } }

    FontMetrics { id: fm; font.family: Theme.mono; font.pixelSize: Theme.body * 0.95 }

    // Momentum both ways (unwrapped text pans sideways too).
    PanWheel { id: wheel; flickable: list; sideways: !dv.wrap }

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
            // Coloured code is <pre> HTML, which never wraps: pre-wrap does.
            text: dv.wrap && block.model.rich && dv.shownMode === "code" ? block.model.text.replace('<pre style="margin:0">', '<pre style="margin:0; white-space:pre-wrap">') : block.model.text
            width: dv.wrap ? list.width : list.contentWidth
            readOnly: true
            selectByMouse: true
            textFormat: block.model.rich ? TextEdit.RichText : TextEdit.PlainText
            wrapMode: dv.wrap ? TextEdit.Wrap : TextEdit.NoWrap
            // Pictures in Markdown next to the file.
            baseUrl: dv.mode === "markdown" ? "file://" + dv.path.substring(0, dv.path.lastIndexOf("/") + 1) : ""
            font.family: dv.shownMode === "code" ? Theme.mono : Theme.family
            font.pixelSize: dv.shownMode === "code" ? Theme.body * 0.95 : Theme.body
            renderType: Text.NativeRendering
            color: Theme.fg
            selectionColor: Theme.accent
            selectedTextColor: Theme.accentText
            leftPadding: dv.shownMode === "code" ? 12 : 24
            rightPadding: dv.shownMode === "code" ? 12 : 24
            onLinkActivated: (link) => Qt.openUrlExternally(link)
            HoverHandler { cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.IBeamCursor }
        }
    }

    // View options, top right: Rendered / Source for Markdown, Wrap.
    Row {
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.margins: 8
        spacing: 4
        z: 2
        opacity: optsHover.hovered ? 1 : 0.75
        HoverHandler { id: optsHover }
        GButton { visible: dv.mode === "markdown"; flat: false; capsule: true; text: "Rendered"; checked: !prefs.markdownSource
            tip: "Show the Markdown as a formatted page"; onClicked: prefs.markdownSource = false }
        GButton { visible: dv.mode === "markdown"; flat: false; capsule: true; text: "Source"; checked: prefs.markdownSource
            tip: "Show the Markdown text itself, coloured"; onClicked: prefs.markdownSource = true }
        GButton { visible: !dv.rendered; flat: false; capsule: true; text: "Wrap"; checked: prefs.wrapText
            tip: prefs.wrapText ? "Long lines wrap to the window; click to keep them on one line (scroll sideways)" : "Long lines stay on one line (scroll sideways, Shift+wheel); click to wrap them"
            onClicked: prefs.wrapText = !prefs.wrapText }
    }

    GLabel {
        anchors.centerIn: parent
        visible: doc.error !== "" || (doc.loading && list.count === 0)
        text: doc.error !== "" ? doc.error : "Reading…"
        color: Theme.fgSecondary
    }
}
