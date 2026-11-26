import QtQuick
import QtQuick.Controls.Basic
import Grove

// A patch or diff the way delta shows one (DocView's Unified / Side by Side
// switch): a header for each file with its +/− counts, each hunk under the
// function it's in, removed lines on red and added on green with their old
// and new line numbers, the words that changed marked stronger, the code
// syntax-coloured. Side by side puts the old file on the left and the new
// on the right. DiffDoc (src/diffdoc.rs) lays both out on a worker.
Item {
    id: dfv
    property string path: ""
    property bool active: false
    property bool split: false
    property bool wrap: true
    property real zoom: 1
    property bool slim: false
    readonly property alias doc: ddoc

    DiffDoc { id: ddoc; split: dfv.split }
    function reload() { widest = 0; ddoc.load(active ? path : "", Theme.dark) }
    onPathChanged: Qt.callLater(reload)
    onActiveChanged: Qt.callLater(reload)
    onWrapChanged: widest = 0
    onSplitChanged: widest = 0
    Connections { target: Theme; function onDarkChanged() { if (dfv.active) dfv.reload() } }

    // Tests: "diff:dump" prints the rows.
    function testAction(a) {
        const rows = []
        for (let i = 0; i < Math.min(list.count, 40); i++) {
            const d = k => ddoc.data(ddoc.index(i, 0), 0x0100 + ["kind", "oldNo", "newNo", "left", "right", "leftType", "rightType", "sub"].indexOf(k))
            rows.push(d("kind") + " " + d("oldNo") + "/" + d("newNo") + " " + d("leftType") + "/" + d("rightType") + " " + ddoc.plain(i, "old") + " | " + ddoc.plain(i, "new") + " " + d("sub"))
        }
        console.warn("grove test diff", a, "split", dfv.split, "files", ddoc.files, "+" + ddoc.added, "-" + ddoc.removed, "rows", list.count, "error", ddoc.error, "\n" + rows.join("\n"))
    }

    FontMetrics { id: fm; font.family: Theme.mono; font.pixelSize: Theme.body * 0.95 * dfv.zoom }
    readonly property real lineH: Math.ceil(fm.height) + 2
    readonly property real numW: Math.max(2, String(ddoc.maxNumber).length) * fm.averageCharacterWidth + 14
    // delta's colours: the line, and its number column a shade stronger.
    readonly property color delBg: Theme.dark ? "#3f0001" : "#ffebe9"
    readonly property color addBg: Theme.dark ? "#002800" : "#e6ffec"
    readonly property color delNum: Theme.dark ? "#5c1012" : "#ffd7d5"
    readonly property color addNum: Theme.dark ? "#0b4a12" : "#ccffd8"
    readonly property color numFg: Theme.fgSecondary
    function bgOf(t) { return t === 2 ? delBg : t === 3 ? addBg : "transparent" }
    function numBgOf(t) { return t === 2 ? delNum : t === 3 ? addNum : "transparent" }
    function html(s) { return "<div style=\"white-space:" + (dfv.wrap ? "pre-wrap" : "pre") + "\">" + s + "</div>" }
    // Unwrapped unified lines: as wide as the widest.
    property real widest: 0

    PanWheel { anchors.fill: parent; flickable: list; sideways: !dfv.wrap && !dfv.split }

    ListView {
        id: list
        anchors.fill: parent
        clip: true
        interactive: false
        model: ddoc
        flickableDirection: Flickable.HorizontalAndVerticalFlick
        contentWidth: dfv.wrap || dfv.split ? width : Math.max(width, dfv.widest)
        cacheBuffer: 800
        header: Item { height: 4; width: 1 }
        footer: Item { height: 16; width: 1 }
        ScrollBar.vertical: GScrollBar {}
        ScrollBar.horizontal: GScrollBar {}
        delegate: Item {
            id: row
            required property int index
            required property string kind
            required property int oldNo
            required property int newNo
            // (The row's HTML: "left"/"right" roles, read through the model
            // since Item has left and right of its own.)
            required property var model
            readonly property string lhtml: model.left
            readonly property string rhtml: model.right
            readonly property int leftType: model.leftType
            readonly property int rightType: model.rightType
            required property string sub
            width: list.contentWidth
            height: kind === "file" ? fileRow.height : kind === "hunk" ? hunkRow.height : kind === "text" ? textT.height + 12 : lineRow.height

            // Right-click: copy the line (either side when side by side).
            TapHandler {
                acceptedButtons: Qt.RightButton
                onTapped: (pt) => { rowMenu.row = row.index; rowMenu.sideBySide = dfv.split && row.kind === "line"; rowMenu.popup() }
            }

            // ---- text outside the files: a mail's headers, the message, a diffstat
            Text {
                id: textT
                visible: row.kind === "text"
                x: 12; y: 6
                width: dfv.wrap ? list.width - 24 : implicitWidth
                text: visible ? dfv.html(row.lhtml) : ""
                textFormat: Text.RichText
                wrapMode: dfv.wrap ? Text.Wrap : Text.NoWrap
                font: fm.font
                color: Theme.fgSecondary
                renderType: Text.NativeRendering
            }

            // ---- a file's header
            Item {
                id: fileRow
                visible: row.kind === "file"
                width: list.width
                height: visible ? nameT.height + 22 + (row.index > 0 ? 10 : 0) : 0
                Rectangle {
                    y: row.index > 0 ? 10 : 0
                    width: parent.width
                    height: parent.height - y
                    color: Theme.dark ? Qt.rgba(1, 1, 1, 0.06) : Qt.rgba(0, 0, 0, 0.04)
                    Rectangle { width: parent.width; height: 1; color: Theme.separator }
                    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.separator }
                    GIcon { id: fIcon; x: 12; anchors.verticalCenter: parent.verticalCenter; size: 16; name: "text-x-generic" }
                    Text {
                        id: nameT
                        anchors.left: fIcon.right; anchors.leftMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.min(implicitWidth, parent.width - 40 - subT.implicitWidth - 16)
                        elide: Text.ElideMiddle
                        text: row.kind === "file" ? row.lhtml : ""
                        font.family: Theme.mono
                        font.pixelSize: Theme.body * dfv.zoom
                        font.weight: Font.DemiBold
                        color: Theme.fg
                        renderType: Text.NativeRendering
                    }
                    Text {
                        id: subT
                        anchors.left: nameT.right; anchors.leftMargin: 12
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.max(0, parent.width - x - 12)
                        elide: Text.ElideRight
                        textFormat: Text.StyledText
                        // The counts in their colours.
                        text: row.kind === "file" ? row.sub.replace(/&/g, "&amp;").replace(/</g, "&lt;")
                                .replace(/\+(\d+) −(\d+)$/, "<font color=\"" + (Theme.dark ? "#3fb950" : "#1a7f37") + "\">+$1</font> <font color=\"" + (Theme.dark ? "#f85149" : "#cf222e") + "\">−$2</font>") : ""
                        font.pixelSize: Theme.small * dfv.zoom
                        color: Theme.fgSecondary
                        renderType: Text.NativeRendering
                    }
                }
            }

            // ---- a hunk's header: where it is and the function it's in
            Rectangle {
                id: hunkRow
                visible: row.kind === "hunk"
                width: list.width
                height: visible ? Math.max(dfv.lineH, hunkT.height) + 8 : 0
                color: Theme.dark ? Qt.rgba(0.35, 0.55, 1, 0.10) : Qt.rgba(0.2, 0.45, 1, 0.07)
                Text {
                    id: hunkAt
                    x: 12
                    anchors.verticalCenter: parent.verticalCenter
                    text: row.kind === "hunk" ? row.sub : ""
                    font.pixelSize: Theme.small * dfv.zoom
                    color: Theme.fgSecondary
                    renderType: Text.NativeRendering
                }
                Text {
                    id: hunkT
                    anchors.left: hunkAt.right; anchors.leftMargin: 12
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - x - 12
                    text: row.kind === "hunk" && row.lhtml !== "" ? dfv.html(row.lhtml) : ""
                    textFormat: Text.RichText
                    wrapMode: Text.NoWrap
                    clip: true
                    font: fm.font
                    color: Theme.fg
                    renderType: Text.NativeRendering
                }
            }

            // ---- a line (unified: both numbers then the line; side by side: each half)
            Item {
                id: lineRow
                visible: row.kind === "line"
                width: parent.width
                height: visible ? Math.max(dfv.lineH, (dfv.split ? Math.max(leftT.height, rightT.height) : leftT.height)) : 0
                readonly property real half: Math.floor((list.width - 1) / 2)
                // Left (or the only) half.
                Rectangle {
                    id: leftBox
                    width: dfv.split ? lineRow.half : parent.width
                    height: parent.height
                    color: dfv.bgOf(row.leftType)
                    clip: dfv.split
                    Rectangle {
                        id: nums
                        width: dfv.split ? dfv.numW : dfv.numW * 2
                        height: parent.height
                        color: dfv.numBgOf(row.leftType)
                        Text {
                            width: dfv.numW - 6
                            horizontalAlignment: Text.AlignRight
                            y: 1
                            text: row.oldNo > 0 ? row.oldNo : ""
                            font: fm.font
                            color: dfv.numFg
                            renderType: Text.NativeRendering
                        }
                        Text {
                            visible: !dfv.split
                            x: dfv.numW
                            width: dfv.numW - 6
                            horizontalAlignment: Text.AlignRight
                            y: 1
                            text: row.newNo > 0 ? row.newNo : ""
                            font: fm.font
                            color: dfv.numFg
                            renderType: Text.NativeRendering
                        }
                    }
                    // The +/− sign, as in the patch.
                    Text {
                        x: nums.width + 4
                        y: 1
                        text: row.leftType === 2 ? "−" : row.leftType === 3 ? "+" : ""
                        font: fm.font
                        color: dfv.numFg
                        renderType: Text.NativeRendering
                    }
                    Text {
                        id: leftT
                        x: nums.width + 4 + fm.averageCharacterWidth * 1.5
                        y: 1
                        width: dfv.wrap ? leftBox.width - x - 8 : implicitWidth
                        text: row.kind === "line" ? dfv.html(row.lhtml) : ""
                        textFormat: Text.RichText
                        wrapMode: dfv.wrap ? Text.Wrap : Text.NoWrap
                        font: fm.font
                        color: Theme.fg
                        renderType: Text.NativeRendering
                        onImplicitWidthChanged: if (!dfv.wrap && !dfv.split && x + implicitWidth + 16 > dfv.widest) dfv.widest = x + implicitWidth + 16
                    }
                }
                Rectangle { visible: dfv.split; x: lineRow.half; width: 1; height: parent.height; color: Theme.separator }
                // Right half (side by side).
                Rectangle {
                    id: rightBox
                    visible: dfv.split
                    x: lineRow.half + 1
                    width: list.width - x
                    height: parent.height
                    color: dfv.bgOf(row.rightType)
                    clip: true
                    Rectangle {
                        id: rnums
                        width: dfv.numW
                        height: parent.height
                        color: dfv.numBgOf(row.rightType)
                        Text {
                            width: dfv.numW - 6
                            horizontalAlignment: Text.AlignRight
                            y: 1
                            text: row.newNo > 0 ? row.newNo : ""
                            font: fm.font
                            color: dfv.numFg
                            renderType: Text.NativeRendering
                        }
                    }
                    Text {
                        x: rnums.width + 4
                        y: 1
                        text: row.rightType === 3 ? "+" : ""
                        font: fm.font
                        color: dfv.numFg
                        renderType: Text.NativeRendering
                    }
                    Text {
                        id: rightT
                        x: rnums.width + 4 + fm.averageCharacterWidth * 1.5
                        y: 1
                        width: dfv.wrap ? rightBox.width - x - 8 : implicitWidth
                        text: row.kind === "line" && dfv.split ? dfv.html(row.rhtml) : ""
                        textFormat: Text.RichText
                        wrapMode: dfv.wrap ? Text.Wrap : Text.NoWrap
                        font: fm.font
                        color: Theme.fg
                        renderType: Text.NativeRendering
                    }
                }
            }
        }
    }

    GMenu {
        id: rowMenu
        property int row: -1
        property bool sideBySide: false
        GMenuItem { visible: !rowMenu.sideBySide; text: "Copy Line"; iconName: "edit-copy"; tip: "Copy this line's text, without its + or −"; onTriggered: ddoc.copy(rowMenu.row, "") }
        GMenuItem { visible: rowMenu.sideBySide; enabled: ddoc.plain(rowMenu.row, "old") !== ""; text: "Copy Old Line"; iconName: "edit-copy"; tip: "Copy the left side's text (before the change)"; onTriggered: ddoc.copy(rowMenu.row, "old") }
        GMenuItem { visible: rowMenu.sideBySide; enabled: ddoc.plain(rowMenu.row, "new") !== ""; text: "Copy New Line"; iconName: "edit-copy"; tip: "Copy the right side's text (after the change)"; onTriggered: ddoc.copy(rowMenu.row, "new") }
    }

    GLabel {
        anchors.centerIn: parent
        visible: list.count === 0 && (ddoc.loading || ddoc.error !== "")
        text: ddoc.error !== "" ? ddoc.error : "Reading…"
        color: Theme.fgSecondary
    }
}
