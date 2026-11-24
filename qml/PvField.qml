import QtQuick
import QtQuick.Controls.Basic

// A PDF form field in Grove Preview, over the page where the field is:
// text boxes to type in, check boxes and radio buttons to click, lists to
// pick from. Fields keep the page's own drawing until changed; changed
// ones are drawn here (and written into the PDF when saved).
Item {
    id: f
    property var win
    property var doc
    property var w: ({})
    property real k: 1
    property string fieldKey: ""
    readonly property bool changed: doc && doc.fields && doc.fields[fieldKey] !== undefined
    readonly property string value: changed ? doc.fields[fieldKey] : (w.value || "")
    readonly property bool on: value !== "" && value !== "Off"
    x: w.r[0] * k
    y: w.r[1] * k
    width: (w.r[2] - w.r[0]) * k
    height: (w.r[3] - w.r[1]) * k
    visible: w.kind !== "button" && w.kind !== "signature"
    enabled: !w.readonly && win.tool === "textsel"
    readonly property color tint: Qt.rgba(0.25, 0.45, 1, hover.hovered || field.activeFocus ? 0.18 : 0.08)
    HoverHandler { id: hover }

    function set(v) { win.setField(doc, fieldKey, v, w) }

    // Text
    Rectangle {
        anchors.fill: parent
        visible: f.w.kind === "text"
        color: f.changed || field.activeFocus ? "white" : f.tint
        border.width: field.activeFocus ? 1 : 0
        border.color: Theme.accent
        TextArea {
            id: field
            anchors.fill: parent
            padding: 1
            leftPadding: 2
            topPadding: f.w.multiline ? 1 : Math.max(0, (height - contentHeight) / 2)
            background: null
            wrapMode: f.w.multiline ? TextEdit.Wrap : TextEdit.NoWrap
            font.family: "Helvetica"
            font.pixelSize: Math.max(5, Math.min(f.height * 0.72, 11 * f.k))
            // Unchanged: the page shows the value already.
            color: f.changed || activeFocus ? "black" : "transparent"
            selectByMouse: true
            text: f.value
            Keys.onTabPressed: (e) => { f.win.nextField(f, (e.modifiers & Qt.ShiftModifier) ? -1 : 1); e.accepted = true }
            Keys.onReturnPressed: (e) => { if (f.w.multiline) e.accepted = false; else { f.set(text); focus = false } }
            Keys.onEscapePressed: { text = f.value; focus = false }
            onActiveFocusChanged: if (!activeFocus && text !== f.value) f.set(text)
            onTextChanged: if (f.w.maxLen > 0 && text.length > f.w.maxLen) text = text.substring(0, f.w.maxLen)
        }
    }
    // Check box / radio button
    Rectangle {
        anchors.fill: parent
        visible: f.w.kind === "checkbox" || f.w.kind === "radio"
        color: f.changed ? "white" : f.tint
        radius: f.w.kind === "radio" ? width / 2 : 0
        GLabel {
            anchors.centerIn: parent
            visible: f.changed && f.on
            text: f.w.kind === "radio" ? "●" : "✓"
            color: "black"
            font.pixelSize: Math.max(6, f.height * 0.85)
        }
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: f.set(f.on && f.w.kind !== "radio" ? "Off" : (f.w.on || "Yes"))
        }
    }
    // List
    GCombo {
        anchors.fill: parent
        visible: f.w.kind === "combo" || f.w.kind === "list"
        model: f.w.options || []
        currentIndex: (f.w.options || []).indexOf(f.value)
        tip: f.w.name || "Choose one"
        font.pixelSize: Math.max(6, Math.min(f.height * 0.6, 11 * f.k))
        onActivated: (i) => f.set(f.w.options[i])
        opacity: f.changed || hovered || popup.visible ? 1 : 0.35
    }
    GTip { visible: hover.hovered && f.w.name !== "" && !field.activeFocus; text: f.w.name + (f.w.readonly ? " (can't be changed)" : "") }
}
