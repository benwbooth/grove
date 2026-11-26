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
    Settings { id: prefs; category: "viewer"; property bool wrapText: false; property bool markdownSource: false; property bool htmlSource: false; property bool treeSource: false; property bool wrapTree: false; property bool wrapMarkdown: true; property real textZoom: 1; property string diffView: "unified"; property bool wrapDiff: true }
    // Markdown and web pages: rendered, or their coloured source (each remembered).
    readonly property bool renderable: mode === "markdown" || mode === "html"
    readonly property bool showSource: mode === "html" ? prefs.htmlSource : prefs.markdownSource
    readonly property bool rendered: renderable && !showSource
    readonly property string shownMode: renderable && showSource ? "code" : mode
    // JSON, YAML, TOML, XML and INI files: a tree of their values, or the
    // coloured source (remembered). A file not known by its name is one
    // when the source's language comes out as one of them.
    property string sniffed: ""
    readonly property string treeFormat: mode === "code" && path !== "" ? (sview.doc.formatFor(path) || sniffed) : ""
    readonly property bool treeable: treeFormat !== ""
    readonly property bool treeChosen: treeable && !prefs.treeSource
    // It didn't parse: the source, with a word about where it went wrong.
    readonly property bool treeFailed: treeChosen && sview.doc.error !== ""
    readonly property bool treeShown: treeChosen && !treeFailed
    readonly property string formatTitle: ({ json: "JSON", yaml: "YAML", toml: "TOML", xml: "XML", ini: "INI" })[treeFormat] || ""
    function focusSearch() { if (treeShown) sview.focusSearch() }
    // Patches and diffs: delta-style, unified or side by side, or their
    // source (remembered). By name, or when the source reads as a diff.
    property bool sniffedDiff: false
    readonly property bool diffable: mode === "code" && path !== "" && (dview.doc.isPatchName(path) || sniffedDiff)
    readonly property bool diffChosen: diffable && prefs.diffView !== "source"
    // No changes in it after all: the source.
    readonly property bool diffShown: diffChosen && dview.doc.error === ""
    // Office documents read as a page: white paper at letter width, margins.
    readonly property bool paper: mode === "office"
    readonly property real paperW: Math.max(240, Math.min(list.width - 32, 816 * prefs.textZoom))
    readonly property real paperX: Math.max(16, (list.width - paperW) / 2)
    readonly property real paperPad: Math.min(72, paperW * 0.09)
    readonly property bool wrap: paper || (diffShown ? prefs.wrapDiff : treeShown ? prefs.wrapTree : rendered ? prefs.wrapMarkdown : prefs.wrapText)
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
        widest = 0; doc.load(active && !treeShown && !diffShown ? path : "", shownMode, Theme.dark) }
    onDiffShownChanged: Qt.callLater(reload)
    onShownModeChanged: Qt.callLater(reload)
    onWrapChanged: widest = 0
    onPathChanged: { sniffed = ""; sniffedDiff = false; Qt.callLater(reload) }
    onTreeShownChanged: Qt.callLater(reload)
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
    Connections {
        target: doc
        function onLoadingChanged() { if (dv.pendingView) Qt.callLater(dv.tryRestore) }
        function onDetectedChanged() {
            const f = ({ "JSON": "json", "YAML": "yaml", "XML": "xml", "TOML": "toml", "INI": "ini" })[doc.syntax || doc.detected]
            if (f && dv.mode === "code") dv.sniffed = f
            if ((doc.syntax || doc.detected) === "Diff" && dv.mode === "code") dv.sniffedDiff = true
        }
    }
    // Source after a failed tree: scroll to the line it stopped at.
    function showLine(line) {
        if (line < 1) return
        list.positionViewAtIndex(Math.floor((line - 1) / 200), ListView.Beginning)
        list.contentY = Math.max(0, Math.min(list.contentY + ((line - 1) % 200) * fm.height - list.height / 3, list.contentHeight - list.height))
    }

    // Tests (GROVE_TEST qlview=...): "end", "top", "dump".
    function testAction(a) {
        // "view:tree" / "view:source" switch; "tree:..." go to the tree.
        if (a === "view:unified" || a === "view:split" || a === "view:diffsource") { prefs.diffView = a === "view:diffsource" ? "source" : a.substring(5); console.warn("grove test doc", a, "diffable", dv.diffable); return }
        if (a.startsWith("diff:")) { dview.testAction(a); return }
        if (a === "view:tree" || a === "view:source") { prefs.treeSource = a === "view:source"; console.warn("grove test doc", a, "treeable", dv.treeable, dv.treeFormat); return }
        if (a.startsWith("tree:")) { if (dv.treeShown) sview.testAction(a); else console.warn("grove test doc no tree:", dv.treeFormat, "failed", dv.treeFailed, sview.doc.error); return }
        if (a === "showerror") dv.showLine(sview.doc.errorLine)
        // "syntax:Rust" picks a language, "syntax:" Automatic; "syntaxmenu" opens the menu.
        if (a.startsWith("syntax:")) syntaxPicker.choose(a.substring(7))
        else if (a === "syntaxmenu") syntaxPicker.open()
        else if (a === "end") list.positionViewAtEnd()
        else if (a === "top") list.positionViewAtBeginning()
        console.warn("grove test doc", a, "blocks", list.count, "lines", doc.lines, "loading", doc.loading, "syntax", doc.syntax, "detected", doc.detected, "y", Math.round(list.contentY))
    }

    FontMetrics { id: fm; font.family: Theme.mono; font.pixelSize: Theme.body * 0.95 * prefs.textZoom }

    // Momentum both ways (unwrapped text pans sideways too).
    PanWheel { id: wheel; visible: !dv.treeShown && !dv.diffShown; anchors.fill: parent; anchors.topMargin: list.anchors.topMargin; flickable: list; sideways: !dv.wrap }

    StructView {
        id: sview
        anchors.fill: parent
        anchors.topMargin: opts.height
        visible: dv.treeShown
        active: dv.active && dv.treeChosen
        path: dv.path
        format: dv.treeFormat
        slim: dv.slim
        zoom: prefs.textZoom
        wrap: prefs.wrapTree
    }

    DiffView {
        id: dview
        anchors.fill: parent
        anchors.topMargin: opts.height
        visible: dv.diffShown
        active: dv.active && dv.diffChosen
        path: dv.path
        split: prefs.diffView === "split"
        wrap: prefs.wrapDiff
        zoom: prefs.textZoom
        slim: dv.slim
    }

    // The tree didn't parse: why, where, and a way there.
    Rectangle {
        id: treeError
        visible: dv.treeFailed
        anchors.top: opts.bottom
        width: parent.width
        height: visible ? errRow.height + 12 : 0
        color: Qt.rgba(1, 0.62, 0.04, Theme.dark ? 0.16 : 0.14)
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.separator }
        Row {
            id: errRow
            x: 12
            anchors.verticalCenter: parent.verticalCenter
            spacing: 10
            GIcon { name: "dialog-warning"; size: 16; anchors.verticalCenter: parent.verticalCenter }
            GLabel {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, treeError.width - 24 - 26 - (showLineBtn.visible ? showLineBtn.width + 10 : 0))
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                font.pixelSize: Theme.small
                text: "This isn't valid " + dv.formatTitle + ", so here's its text: " + sview.doc.error + (sview.doc.errorLine > 0 ? " (line " + sview.doc.errorLine + ", column " + sview.doc.errorCol + ")" : "")
            }
            PushButton {
                id: showLineBtn
                visible: sview.doc.errorLine > 0
                anchors.verticalCenter: parent.verticalCenter
                text: "Show Line " + sview.doc.errorLine
                tip: "Scroll to where the " + dv.formatTitle + " stops making sense"
                onClicked: dv.showLine(sview.doc.errorLine)
            }
        }
    }

    ListView {
        id: list
        anchors.fill: parent
        anchors.topMargin: opts.height + treeError.height
        visible: !dv.treeShown && !dv.diffShown
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
    // the language (code), Rendered / Source (Markdown), text size and Wrap on the right.
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
            text: dv.paper ? "" : dv.diffShown ? (dview.doc.files > 0 ? dview.doc.files + (dview.doc.files === 1 ? " file" : " files") + " · +" + dview.doc.added + " −" + dview.doc.removed : "") : dv.treeShown ? (sview.doc.values > 0 ? sview.doc.values.toLocaleString(Qt.locale(), "f", 0) + (sview.doc.values === 1 ? " value" : " values") + " · " + dv.formatTitle : "") : doc.lines > 0 ? doc.lines.toLocaleString(Qt.locale(), "f", 0) + (doc.lines === 1 ? " line" : " lines") + (doc.loading ? " so far…" : "") + (doc.encoding ? " · " + doc.encoding : "") : ""
            color: Theme.fgSecondary
            font.pixelSize: Theme.small
        }
        Row {
            id: tools
            anchors.verticalCenter: parent.verticalCenter
            anchors.right: parent.right
            anchors.rightMargin: dv.slim ? 4 : 8
            spacing: dv.slim ? 2 : 6
            SyntaxPicker {
                id: syntaxPicker
                visible: dv.shownMode === "code" && dv.active && !dv.treeShown && !dv.diffShown
                doc: doc
                slim: dv.slim
                anchors.verticalCenter: parent.verticalCenter
            }
            Rectangle { visible: syntaxPicker.visible; width: 1; height: dv.slim ? 14 : 18; color: Theme.separator; anchors.verticalCenter: parent.verticalCenter }
            Segmented {
                visible: dv.diffable
                height: Math.round((dv.slim ? 22 : 28) * Theme.pt)
                anchors.verticalCenter: parent.verticalCenter
                items: [
                    { icon: "view-list-text|view-list-details", tip: "Unified: each change as removed lines (red) then added lines (green), with old and new line numbers and the changed words marked", value: "unified" },
                    { icon: "view-split-left-right|view-dual-symbolic|view-columns", tip: "Side by side: the old lines on the left, the new on the right", value: "split" },
                    { icon: "format-text-code|text-x-script", tip: "Source: the patch text itself, coloured", value: "source" }
                ]
                current: prefs.diffView
                onPicked: (v) => prefs.diffView = v
            }
            Rectangle { visible: dv.diffable; width: 1; height: dv.slim ? 14 : 18; color: Theme.separator; anchors.verticalCenter: parent.verticalCenter }
            Segmented {
                visible: dv.treeable
                height: Math.round((dv.slim ? 22 : 28) * Theme.pt)
                anchors.verticalCenter: parent.verticalCenter
                items: [
                    { icon: "view-list-tree|view-list-details", tip: "Tree: the " + dv.formatTitle + " as rows to open and close, with search (Ctrl+F) and copying of values and paths", value: "tree" },
                    { icon: "format-text-code|text-x-script", tip: "Source: the " + dv.formatTitle + " text itself, coloured", value: "source" }
                ]
                current: prefs.treeSource ? "source" : "tree"
                onPicked: (v) => prefs.treeSource = v === "source"
            }
            Rectangle { visible: dv.treeable; width: 1; height: dv.slim ? 14 : 18; color: Theme.separator; anchors.verticalCenter: parent.verticalCenter }
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
                onClicked: { if (dv.diffShown) prefs.wrapDiff = !prefs.wrapDiff; else if (dv.treeShown) prefs.wrapTree = !prefs.wrapTree; else if (dv.rendered) prefs.wrapMarkdown = !prefs.wrapMarkdown; else prefs.wrapText = !prefs.wrapText }
            }
        }
    }

    // "Reading…" only once it's been reading a while (quick files would flicker it).
    property bool readingLong: false
    Timer {
        interval: 500
        running: doc.loading
        onRunningChanged: if (running) dv.readingLong = false
        onTriggered: dv.readingLong = true
    }
    GLabel {
        anchors.centerIn: parent
        visible: !dv.treeShown && !dv.diffShown && (doc.error !== "" || (doc.loading && dv.readingLong && list.count === 0))
        text: doc.error !== "" ? doc.error : "Reading…"
        color: Theme.fgSecondary
    }
}
