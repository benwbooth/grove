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
    property string mode: ""        // code, markdown, html, office, plain
    property bool active: false
    // Small panes (the preview column, the list's preview strip): a light
    // toolbar with smaller controls, no bar of its own.
    property bool slim: false
    // Viewing choices, remembered: soft wrap for text and code, and
    // Markdown as its coloured source instead of the rendered page.
    Settings { id: prefs; category: "viewer"; property bool wrapText: false; property bool markdownSource: false; property bool htmlSource: false; property bool wrapMarkdown: true; property real textZoom: 1 }
    // Markdown and web pages: rendered, or their coloured source (each remembered).
    readonly property bool renderable: mode === "markdown" || mode === "html"
    readonly property bool showSource: mode === "html" ? prefs.htmlSource : prefs.markdownSource
    readonly property bool rendered: renderable && !showSource
    readonly property string shownMode: renderable && showSource ? "code" : mode
    // Office documents read as a page: white paper at letter width, margins.
    readonly property bool paper: mode === "office"
    readonly property real paperW: Math.max(240, Math.min(list.width - 32, 816 * prefs.textZoom))
    readonly property real paperX: Math.max(16, (list.width - paperW) / 2)
    readonly property real paperPad: Math.min(72, paperW * 0.09)
    readonly property bool wrap: paper || (rendered ? prefs.wrapMarkdown : prefs.wrapText)
    readonly property alias lines: doc.lines
    readonly property alias loading: doc.loading
    readonly property real charWidth: fm.averageCharacterWidth

    TextDoc {
        id: doc
        family: Theme.family
        pixel: Theme.body * prefs.textZoom
        pageWidth: Math.max(200, dv.width - 64)
    }
    property real widest: 0
    function reload() {
        widest = 0; doc.load(active ? path : "", shownMode, Theme.dark) }
    onShownModeChanged: Qt.callLater(reload)
    onWrapChanged: widest = 0
    onPathChanged: Qt.callLater(reload)
    onModeChanged: Qt.callLater(reload)
    onActiveChanged: Qt.callLater(reload)
    Connections { target: Theme; function onDarkChanged() { if (dv.active) dv.reload() } }

    // The session: where the reader was (restored once the text is there).
    property var pendingView: null
    function viewState() { return { y: Math.round(list.contentY), x: Math.round(list.contentX) } }
    function restoreView(s) { pendingView = s; Qt.callLater(tryRestore) }
    function tryRestore() {
        const s = pendingView
        if (!s) return
        const maxY = Math.max(0, list.contentHeight - list.height)
        // Still reading, or not laid out that far yet: wait for more (the
        // list's height changes as blocks are laid out).
        if (s.y > maxY && (doc.loading || list.count === 0 || maxY <= 0)) return
        list.contentY = Math.min(s.y || 0, maxY)
        list.contentX = Math.min(s.x || 0, Math.max(0, list.contentWidth - list.width))
        pendingView = null
    }
    Connections { target: list; function onContentHeightChanged() { if (dv.pendingView) dv.tryRestore() } }
    Connections { target: doc; function onLoadingChanged() { if (dv.pendingView) Qt.callLater(dv.tryRestore) } }

    // Tests (GROVE_TEST qlview=...): "end", "top", "dump".
    function testAction(a) {
        if (a === "end") list.positionViewAtEnd()
        else if (a === "top") list.positionViewAtBeginning()
        console.warn("grove test doc", a, "blocks", list.count, "lines", doc.lines, "loading", doc.loading, "y", Math.round(list.contentY))
    }

    FontMetrics { id: fm; font.family: Theme.mono; font.pixelSize: Theme.body * 0.95 * prefs.textZoom }

    // Momentum both ways (unwrapped text pans sideways too).
    PanWheel { id: wheel; anchors.fill: parent; anchors.topMargin: opts.height; flickable: list; sideways: !dv.wrap }

    ListView {
        id: list
        anchors.fill: parent
        anchors.topMargin: opts.height
        clip: true
        interactive: false
        model: doc
        flickableDirection: Flickable.HorizontalAndVerticalFlick
        // Unwrapped: as wide as the widest line (counted in characters for
        // code; rendered Markdown is measured as its blocks are laid out).
        contentWidth: dv.wrap ? width : Math.max(width, dv.widest, dv.shownMode === "code" ? doc.maxCols * fm.averageCharacterWidth + 40 : 0)
        cacheBuffer: 800
        header: Item { height: 12; width: 1 }
        footer: Item { height: 16; width: 1 }
        ScrollBar.vertical: GScrollBar {}
        ScrollBar.horizontal: GScrollBar {}
        delegate: TextEdit {
            id: block
            required property var model
            required property int index
            readonly property bool firstBlock: index === 0
            readonly property bool lastBlock: index === list.count - 1 && !doc.loading
            // The page behind the text (office documents).
            Rectangle {
                visible: dv.paper
                z: -1
                x: dv.paperX
                y: block.firstBlock ? 16 : 0
                width: dv.paperW
                height: block.height - (block.firstBlock ? 16 : 0) - (block.lastBlock ? 24 : 0)
                color: "white"
                Rectangle { visible: block.firstBlock; width: parent.width; height: 1; color: Qt.rgba(0, 0, 0, 0.18) }
                Rectangle { visible: block.lastBlock; anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Qt.rgba(0, 0, 0, 0.18) }
                Rectangle { x: -1; width: 1; height: parent.height; color: Qt.rgba(0, 0, 0, 0.25) }
                Rectangle { x: parent.width; width: 1; height: parent.height; color: Qt.rgba(0, 0, 0, 0.25) }
            }
            // Coloured code is <pre> HTML, which never wraps: pre-wrap does.
            text: dv.wrap && block.model.rich && dv.shownMode === "code" ? block.model.text.replace('<pre style="margin:0">', '<pre style="margin:0; white-space:pre-wrap">') : block.model.text
            width: dv.wrap ? list.width : list.contentWidth
            readOnly: true
            selectByMouse: true
            textFormat: block.model.rich ? TextEdit.RichText : TextEdit.PlainText
            wrapMode: dv.wrap ? TextEdit.Wrap : TextEdit.NoWrap
            onImplicitWidthChanged: if (!dv.wrap && implicitWidth > dv.widest) dv.widest = implicitWidth
            // Pictures in Markdown next to the file.
            baseUrl: dv.renderable ? "file://" + dv.path.substring(0, dv.path.lastIndexOf("/") + 1) : dv.paper ? "file:///" : ""
            font.family: dv.shownMode === "code" ? Theme.mono : Theme.family
            font.pixelSize: (dv.shownMode === "code" ? Theme.body * 0.95 : Theme.body) * prefs.textZoom
            renderType: Text.NativeRendering
            color: dv.paper ? "#1d1d1f" : Theme.fg
            selectionColor: Theme.accent
            selectedTextColor: Theme.accentText
            leftPadding: dv.paper ? dv.paperX + dv.paperPad : dv.shownMode === "code" ? 12 : 24
            rightPadding: dv.paper ? list.width - dv.paperX - dv.paperW + dv.paperPad : dv.shownMode === "code" ? 12 : 24
            topPadding: dv.paper && firstBlock ? 16 + dv.paperPad : 0
            bottomPadding: dv.paper && lastBlock ? 24 + dv.paperPad : 0
            onLinkActivated: (link) => Qt.openUrlExternally(link)
            HoverHandler { cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.IBeamCursor }
        }
    }

    // A toolbar above the text, like the window's: the length on the left;
    // Rendered / Source (Markdown), text size and Wrap on the right.
    Rectangle {
        id: opts
        width: parent.width
        height: Math.round((dv.slim ? 28 : 36) * Theme.pt)
        color: dv.slim ? "transparent" : Theme.toolbar
        Rectangle { visible: !dv.slim; anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.separator }
        GLabel {
            anchors.verticalCenter: parent.verticalCenter
            x: dv.slim ? 10 : 12
            width: parent.width - tools.width - 30
            elide: Text.ElideRight
            text: dv.paper ? "" : doc.lines > 0 ? doc.lines.toLocaleString(Qt.locale(), "f", 0) + (doc.lines === 1 ? " line" : " lines") + (doc.loading ? " so far…" : "") : ""
            color: Theme.fgSecondary
            font.pixelSize: Theme.small
        }
        Row {
            id: tools
            anchors.verticalCenter: parent.verticalCenter
            anchors.right: parent.right
            anchors.rightMargin: dv.slim ? 4 : 8
            spacing: dv.slim ? 2 : 6
            Segmented {
                visible: dv.renderable
                height: Math.round((dv.slim ? 22 : 28) * Theme.pt)
                anchors.verticalCenter: parent.verticalCenter
                items: [
                    { icon: "view-preview|document-preview", tip: dv.mode === "html" ? "Rendered: the web page as a page" : "Rendered: the Markdown as a formatted page", value: "rendered" },
                    { icon: "format-text-code|text-x-script", tip: dv.mode === "html" ? "Source: the page's HTML, coloured" : "Source: the Markdown text itself, coloured", value: "source" }
                ]
                current: dv.showSource ? "source" : "rendered"
                onPicked: (v) => { if (dv.mode === "html") prefs.htmlSource = v === "source"; else prefs.markdownSource = v === "source" }
            }
            Rectangle { visible: dv.renderable; width: 1; height: dv.slim ? 14 : 18; color: Theme.separator; anchors.verticalCenter: parent.verticalCenter }
            GButton { iconName: "zoom-out"; iconSize: dv.slim ? 14 : 16; enabled: prefs.textZoom > 0.6; tip: "Smaller text"; anchors.verticalCenter: parent.verticalCenter
                onClicked: prefs.textZoom = Math.max(0.6, Math.round((prefs.textZoom - 0.1) * 10) / 10) }
            GButton { iconName: "zoom-in"; iconSize: dv.slim ? 14 : 16; enabled: prefs.textZoom < 2.5; tip: "Larger text"; anchors.verticalCenter: parent.verticalCenter
                onClicked: prefs.textZoom = Math.min(2.5, Math.round((prefs.textZoom + 0.1) * 10) / 10) }
            Rectangle { visible: !dv.paper; width: 1; height: dv.slim ? 14 : 18; color: Theme.separator; anchors.verticalCenter: parent.verticalCenter }
            GButton {
                id: wrapBtn
                visible: !dv.paper
                checked: dv.wrap
                WrapGlyph { anchors.centerIn: parent; size: dv.slim ? 14 : 16; color: wrapBtn.enabled ? wrapBtn.tint : Theme.fgDisabled }
                anchors.verticalCenter: parent.verticalCenter
                tip: dv.wrap ? "Wrap long lines: on (click to keep each line whole and scroll sideways)" : "Wrap long lines: off (scroll sideways or Shift+wheel; click to wrap)"
                onClicked: { if (dv.rendered) prefs.wrapMarkdown = !prefs.wrapMarkdown; else prefs.wrapText = !prefs.wrapText }
            }
        }
    }

    GLabel {
        anchors.centerIn: parent
        visible: doc.error !== "" || (doc.loading && list.count === 0)
        text: doc.error !== "" ? doc.error : "Reading…"
        color: Theme.fgSecondary
    }
}
