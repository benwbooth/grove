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
    Settings { id: prefs; category: "viewer"; property bool wrapText: false; property bool markdownSource: false; property bool wrapMarkdown: true; property real textZoom: 1 }
    readonly property bool rendered: mode === "markdown" && !prefs.markdownSource
    readonly property string shownMode: mode === "markdown" && prefs.markdownSource ? "code" : mode
    readonly property bool wrap: rendered ? prefs.wrapMarkdown : prefs.wrapText
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

    FontMetrics { id: fm; font.family: Theme.mono; font.pixelSize: Theme.body * 0.95 * prefs.textZoom }

    // Momentum both ways (unwrapped text pans sideways too).
    PanWheel { id: wheel; anchors.fill: list; flickable: list; sideways: !dv.wrap }

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
            onImplicitWidthChanged: if (!dv.wrap && implicitWidth > dv.widest) dv.widest = implicitWidth
            // Pictures in Markdown next to the file.
            baseUrl: dv.mode === "markdown" ? "file://" + dv.path.substring(0, dv.path.lastIndexOf("/") + 1) : ""
            font.family: dv.shownMode === "code" ? Theme.mono : Theme.family
            font.pixelSize: (dv.shownMode === "code" ? Theme.body * 0.95 : Theme.body) * prefs.textZoom
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

    // A toolbar above the text, like the window's: the length on the left;
    // Rendered / Source (Markdown), text size and Wrap on the right.
    Rectangle {
        id: opts
        width: parent.width
        height: Math.round(36 * Theme.pt)
        color: Theme.toolbar
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.separator }
        GLabel {
            anchors.verticalCenter: parent.verticalCenter
            x: 12
            width: parent.width - tools.width - 30
            text: doc.lines > 0 ? doc.lines.toLocaleString(Qt.locale(), "f", 0) + (doc.lines === 1 ? " line" : " lines") + (doc.loading ? " so far…" : "") : ""
            color: Theme.fgSecondary
            font.pixelSize: Theme.small
        }
        Row {
            id: tools
            anchors.verticalCenter: parent.verticalCenter
            anchors.right: parent.right
            anchors.rightMargin: 8
            spacing: 6
            Segmented {
                visible: dv.mode === "markdown"
                height: Math.round(28 * Theme.pt)
                anchors.verticalCenter: parent.verticalCenter
                items: [
                    { icon: "view-preview|document-preview", tip: "Rendered: the Markdown as a formatted page", value: "rendered" },
                    { icon: "format-text-code|text-x-script", tip: "Source: the Markdown text itself, coloured", value: "source" }
                ]
                current: prefs.markdownSource ? "source" : "rendered"
                onPicked: (v) => prefs.markdownSource = v === "source"
            }
            Rectangle { visible: dv.mode === "markdown"; width: 1; height: 18; color: Theme.separator; anchors.verticalCenter: parent.verticalCenter }
            GButton { iconName: "zoom-out"; iconSize: 16; enabled: prefs.textZoom > 0.6; tip: "Smaller text"; anchors.verticalCenter: parent.verticalCenter
                onClicked: prefs.textZoom = Math.max(0.6, Math.round((prefs.textZoom - 0.1) * 10) / 10) }
            GButton { iconName: "zoom-in"; iconSize: 16; enabled: prefs.textZoom < 2.5; tip: "Larger text"; anchors.verticalCenter: parent.verticalCenter
                onClicked: prefs.textZoom = Math.min(2.5, Math.round((prefs.textZoom + 0.1) * 10) / 10) }
            Rectangle { width: 1; height: 18; color: Theme.separator; anchors.verticalCenter: parent.verticalCenter }
            GButton {
                id: wrapBtn
                checked: dv.wrap
                WrapGlyph { anchors.centerIn: parent; size: 16; color: wrapBtn.enabled ? wrapBtn.tint : Theme.fgDisabled }
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
