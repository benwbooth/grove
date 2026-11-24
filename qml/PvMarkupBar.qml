import QtQuick
import QtQuick.Controls.Basic

// Grove Preview's Markup toolbar (macOS Preview's): selection tools,
// Instant Alpha, Sketch and Draw, Shapes, Text, Sign, Note; Adjust Color,
// Adjust Size and Crop for pictures; shape style, border and fill colours,
// text style; rotate. Every button says what it does when hovered.
Item {
    id: mb
    property var win
    readonly property bool pdf: win.cur ? win.cur.kind === "pdf" : true
    implicitHeight: Math.round(40 * Theme.pt)

    readonly property var palette: ["#000000", "#545454", "#a8a8a8", "#ffffff", "#ff3b30", "#ff9500", "#ffcc00", "#34c759", "#00c7be", "#007aff", "#5856d6", "#af52de", "#ff2d55", "#a2845e"]
    readonly property var shapes: [
        { t: "line", icon: "draw-line", text: "Line" },
        { t: "arrow", icon: "draw-arrow", text: "Arrow" },
        { t: "rect", icon: "draw-rectangle", text: "Rectangle" },
        { t: "rrect", icon: "draw-rectangle", text: "Rounded Rectangle" },
        { t: "ellipse", icon: "draw-ellipse", text: "Oval" },
        { t: "bubble", icon: "draw-text", text: "Speech Bubble" },
        { t: "star", icon: "draw-star", text: "Star" },
        { t: "poly", icon: "draw-polygon", text: "Polygon" },
        { t: "loupe", icon: "zoom-in", text: "Loupe (magnify part of the picture)", image: true }
    ]

    component Sep: Rectangle { width: 1; height: 20; color: Theme.separator; anchors.verticalCenter: parent.verticalCenter }
    component Tool: GButton {
        property string t: ""
        checked: mb.win.tool === t
        onClicked: mb.win.setTool(mb.win.tool === t && t !== "textsel" ? "textsel" : t)
        anchors.verticalCenter: parent.verticalCenter
    }
    component Swatches: Popup {
        id: sw
        property bool allowNone: false
        property string current: ""
        signal picked(string c)
        padding: 8
        topPadding: 8.001
        leftPadding: 8.001
        background: GlassPanel { tint: Theme.glassMenu }
        contentItem: Column {
            spacing: 6
            Grid {
                columns: 7
                spacing: 5
                Repeater {
                    model: mb.palette
                    Rectangle {
                        required property string modelData
                        width: 20; height: 20; radius: 10
                        color: modelData
                        border.width: sw.current.toLowerCase() === modelData ? 2.5 : 1
                        border.color: sw.current.toLowerCase() === modelData ? Theme.accent : Theme.glassEdge
                        MouseArea { id: m; anchors.fill: parent; hoverEnabled: true; onClicked: { sw.picked(parent.modelData); sw.close() } }
                    }
                }
            }
            GButton { visible: sw.allowNone; text: "No Fill"; flat: false; capsule: true; tip: "Shapes and text boxes without a fill"; onClicked: { sw.picked(""); sw.close() } }
        }
    }

    Row {
        anchors.verticalCenter: parent.verticalCenter
        x: 12
        spacing: 3
        Tool { t: "textsel"; iconName: "edit-select-text|text-field"; tip: "Text Selection: select text to copy (double-click a word, triple-click a line); click a shape to move it" }
        Tool { t: "rectsel"; iconName: "select-rectangular|edit-select"; tip: "Rectangular Selection: drag a rectangle to copy or crop (" + mb.win.keyFor("pv.crop") + ")" }
        Tool { t: "alpha"; visible: !mb.pdf; iconName: "draw-eraser|tool_eraser"; tip: "Instant Alpha: drag from a colour to make the area around it see-through (farther drag: more alike colours)" }
        Sep {}
        Tool { t: "pen"; iconName: "draw-freehand"; tip: "Sketch / Draw: draw freehand" }
        Tool { t: "marker"; iconName: "draw-highlight|draw-brush"; tip: "Highlighter pen: a wide see-through stroke" }
        GButton {
            id: shapeBtn
            iconName: { const s = mb.shapes.find(x => x.t === mb.win.tool); return s ? s.icon : "draw-rectangle" }
            checked: mb.shapes.some(x => x.t === mb.win.tool)
            tip: "Shapes: line, arrow, rectangle, oval, speech bubble, star, polygon" + (mb.pdf ? "" : ", loupe") + ". Shift keeps them square"
            onClicked: shapeMenu.popup(shapeBtn, 0, shapeBtn.height)
            anchors.verticalCenter: parent.verticalCenter
        }
        Tool { t: "text"; iconName: "draw-text|insert-text"; tip: "Text: click where it goes and type" }
        GButton {
            id: signBtn
            iconName: "document-sign|draw-freehand"
            checked: mb.win.tool === "sign"
            tip: "Sign: put your signature here (make one with your mouse or trackpad first)"
            onClicked: signMenu.popup(signBtn, 0, signBtn.height)
            anchors.verticalCenter: parent.verticalCenter
        }
        Tool { t: "note"; visible: mb.pdf; iconName: "note|knotes"; tip: "Note: a sticky note on the page" }
        Sep { visible: mb.pdf }
        GButton {
            id: hlBtn
            visible: mb.pdf
            iconName: "format-text-color|draw-highlight"
            checked: mb.win.tool === "hl" || mb.win.tool === "ul" || mb.win.tool === "so"
            tip: "Highlight, underline or strike through text: pick a style, then select the text"
            onClicked: hlMenu.popup(hlBtn, 0, hlBtn.height)
            anchors.verticalCenter: parent.verticalCenter
        }
        Sep {}
        GButton { visible: !mb.pdf; iconName: "color-management|preferences-color"; checked: mb.win.adjustColorOpen; tip: "Adjust Color: exposure, contrast, saturation, temperature, levels… (" + mb.win.keyFor("pv.adjustcolor") + ")"; onClicked: mb.win.toggleAdjustColor(); anchors.verticalCenter: parent.verticalCenter }
        GButton { visible: !mb.pdf; iconName: "transform-scale|zoom-fit-best"; tip: "Adjust Size: change the picture's pixel size or resolution"; onClicked: mb.win.showAdjustSize(); anchors.verticalCenter: parent.verticalCenter }
        GButton { iconName: "transform-crop|crop"; enabled: mb.win.rectSel !== null; tip: "Crop to the rectangular selection (" + mb.win.keyFor("pv.crop") + ")"; onClicked: mb.win.crop(); anchors.verticalCenter: parent.verticalCenter }
        Sep {}
        // Style: line width, border colour, fill colour, text style.
        GButton {
            id: widthBtn
            tip: "Shape Style: line thickness"
            onClicked: widthPop.open()
            anchors.verticalCenter: parent.verticalCenter
            contentItem: Item { implicitWidth: 18; implicitHeight: 18
                Column { anchors.centerIn: parent; spacing: 2
                    Repeater { model: [1, 2, 3.5]; Rectangle { required property real modelData; width: 14; height: modelData; color: Theme.fg; radius: height / 2 } } } }
            Popup {
                id: widthPop
                y: parent.height + 4
                padding: 8; topPadding: 8.001; leftPadding: 8.001
                background: GlassPanel { tint: Theme.glassMenu }
                contentItem: Column {
                    spacing: 2
                    Repeater {
                        model: [0.5, 1, 2, 3, 5, 8, 12]
                        GButton {
                            required property real modelData
                            width: 120
                            checked: mb.win.mkWidth === modelData
                            tip: modelData + " pt lines"
                            onClicked: { mb.win.setStyle("w", modelData); widthPop.close() }
                            contentItem: Item { implicitHeight: 18; Rectangle { anchors.centerIn: parent; width: 90; height: Math.max(1, parent.parent.modelData); radius: height / 2; color: Theme.fg } }
                        }
                    }
                }
            }
        }
        GButton {
            id: borderBtn
            tip: "Border Color: the colour of lines, shapes and text"
            onClicked: borderPop.open()
            anchors.verticalCenter: parent.verticalCenter
            contentItem: Item { implicitWidth: 18; implicitHeight: 18; Rectangle { anchors.centerIn: parent; width: 16; height: 16; radius: 3; color: "transparent"; border.width: 3; border.color: mb.win.mkColor } }
            Swatches { id: borderPop; y: parent.height + 4; current: "" + mb.win.mkColor; onPicked: (c) => mb.win.setStyle("c", c) }
        }
        GButton {
            id: fillBtn
            tip: "Fill Color: the inside of shapes and text boxes"
            onClicked: fillPop.open()
            anchors.verticalCenter: parent.verticalCenter
            contentItem: Item { implicitWidth: 18; implicitHeight: 18
                Rectangle { anchors.centerIn: parent; width: 16; height: 16; radius: 3; color: mb.win.mkFill || "transparent"; border.width: 1; border.color: Theme.fgSecondary
                    Rectangle { visible: mb.win.mkFill === ""; anchors.centerIn: parent; width: 20; height: 1.5; rotation: -45; color: Theme.danger } } }
            Swatches { id: fillPop; y: parent.height + 4; allowNone: true; current: mb.win.mkFill; onPicked: (c) => mb.win.setStyle("fill", c) }
        }
        GButton {
            id: fontBtn
            text: "Aa"
            tip: "Text Style: font, size, bold, italic, alignment"
            onClicked: fontPop.open()
            anchors.verticalCenter: parent.verticalCenter
            Popup {
                id: fontPop
                y: parent.height + 4
                width: 260
                padding: 10; topPadding: 10.001; leftPadding: 10.001
                background: GlassPanel { tint: Theme.glassMenu }
                contentItem: Column {
                    spacing: 8
                    GCombo {
                        width: parent.width
                        model: ["Helvetica", "Times", "Courier", "Inter", "sans-serif", "serif", "monospace"].concat(Qt.fontFamilies().slice(0, 300))
                        currentIndex: Math.max(0, model.indexOf(mb.win.mkFont))
                        tip: "The font for new text (PDF text boxes keep to Helvetica, Times or Courier)"
                        onActivated: (i) => mb.win.setStyle("font", model[i])
                    }
                    Row {
                        spacing: 6
                        GLabel { text: "Size"; anchors.verticalCenter: parent.verticalCenter }
                        SpinBox { from: 4; to: 288; value: mb.win.mkFontSize; editable: true; onValueModified: mb.win.setStyle("s", value) }
                    }
                    Row {
                        spacing: 4
                        GButton { text: "B"; font.bold: true; checked: mb.win.mkBold; tip: "Bold"; onClicked: mb.win.setStyle("b", !mb.win.mkBold) }
                        GButton { text: "I"; font.italic: true; checked: mb.win.mkItalic; tip: "Italic"; onClicked: mb.win.setStyle("i", !mb.win.mkItalic) }
                        Rectangle { width: 1; height: 20; color: Theme.separator }
                        GButton { iconName: "format-justify-left"; checked: mb.win.mkAlign === "left"; tip: "Align left"; onClicked: mb.win.setStyle("al", "left") }
                        GButton { iconName: "format-justify-center"; checked: mb.win.mkAlign === "center"; tip: "Centre"; onClicked: mb.win.setStyle("al", "center") }
                        GButton { iconName: "format-justify-right"; checked: mb.win.mkAlign === "right"; tip: "Align right"; onClicked: mb.win.setStyle("al", "right") }
                    }
                }
            }
        }
        Sep {}
        GButton { iconName: "object-rotate-left"; tip: "Rotate Left (" + mb.win.keyFor("pv.rotleft") + ")"; onClicked: mb.win.rotate(-90); anchors.verticalCenter: parent.verticalCenter }
        GButton { iconName: "object-rotate-right"; tip: "Rotate Right (" + mb.win.keyFor("pv.rotright") + ")"; onClicked: mb.win.rotate(90); anchors.verticalCenter: parent.verticalCenter }
        GButton { visible: !mb.pdf; iconName: "object-flip-horizontal"; tip: "Flip Horizontal"; onClicked: mb.win.flip(true); anchors.verticalCenter: parent.verticalCenter }
        GButton { visible: !mb.pdf; iconName: "object-flip-vertical"; tip: "Flip Vertical"; onClicked: mb.win.flip(false); anchors.verticalCenter: parent.verticalCenter }
    }

    GMenu {
        id: shapeMenu
        Repeater {
            model: mb.shapes.filter(s => !s.image || !mb.pdf)
            GMenuItem { required property var modelData; text: modelData.text; iconName: modelData.icon; onTriggered: mb.win.setTool(modelData.t) }
        }
    }
    GMenu {
        id: signMenu
        Repeater {
            model: mb.win.signatures
            GMenuItem { required property var modelData; required property int index; text: modelData.name || "Signature " + (index + 1); iconName: "document-sign"; onTriggered: { mb.win.signatureIndex = index; mb.win.setTool("sign") } }
        }
        GMenuSeparator { visible: mb.win.signatures.length > 0 }
        GMenuItem { text: "Create Signature…"; iconName: "list-add"; onTriggered: mb.win.showSignatures() }
        GMenuItem { text: "Manage Signatures…"; iconName: "configure"; enabled: mb.win.signatures.length > 0; onTriggered: mb.win.showSignatures() }
    }
    GMenu {
        id: hlMenu
        GMenuItem { text: "Highlight"; iconName: "draw-highlight"; onTriggered: mb.win.setTool("hl") }
        GMenuItem { text: "Underline"; iconName: "format-text-underline"; onTriggered: mb.win.setTool("ul") }
        GMenuItem { text: "Strikethrough"; iconName: "format-text-strikethrough"; onTriggered: mb.win.setTool("so") }
        GMenuSeparator {}
        Repeater {
            model: [{ c: "#ffcc00", n: "Yellow" }, { c: "#34c759", n: "Green" }, { c: "#007aff", n: "Blue" }, { c: "#ff2d55", n: "Pink" }, { c: "#af52de", n: "Purple" }]
            GMenuItem { required property var modelData; text: modelData.n; swatch: modelData.c; checkable: true; checked: Qt.colorEqual(mb.win.hlColor, modelData.c); onTriggered: mb.win.hlColor = modelData.c }
        }
    }
}
