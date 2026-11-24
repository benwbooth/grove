import QtQuick
import QtQuick.Controls.Basic

// Grove Preview's Adjust Color panel (Preview's): the picture's histogram
// with levels (black point, midtones, white point) and Auto Levels, then
// exposure, contrast, highlights, shadows, saturation, temperature, tint,
// sepia and sharpness. Changes show at once; Reset All undoes them.
Popup {
    id: ac
    property var win
    readonly property var doc: win.cur && win.cur.kind === "image" ? win.cur : null
    readonly property var a: doc ? doc.img.adjust : ({})
    property var info: ({})
    width: Math.round(300 * Theme.pt)
    padding: 14
    topPadding: 14.001
    leftPadding: 14.001
    modal: false
    closePolicy: Popup.CloseOnEscape
    background: GlassPanel { tint: Theme.glassMenu; radius: Theme.windowRadius }
    onOpened: load()
    onDocChanged: if (visible) load()
    function load() {
        info = {}
        if (!doc) return
        const p = doc.path
        Async.call(win.app.grove, k => win.app.grove.pvInfo(k, p, ""), r => { if (ac.doc && ac.doc.path === p) ac.info = r })
    }
    function set(key, v) {
        if (!doc) return
        const n = Object.assign({}, a)
        n[key] = v
        win.changeImage(doc, { adjust: n }, "Adjust Color")
    }
    function val(key, def) { return a[key] !== undefined ? a[key] : def }

    component Adj: Item {
        id: row
        property string key: ""
        property string label: ""
        property string tip: ""
        property real from: -1
        property real to: 1
        property real def: 0
        width: parent ? parent.width : 200
        height: 30
        GLabel { id: lab; text: row.label; width: 90; anchors.verticalCenter: parent.verticalCenter; color: Theme.fgSecondary }
        Slider {
            id: sl
            anchors.left: lab.right
            anchors.right: num.left
            anchors.rightMargin: 6
            anchors.verticalCenter: parent.verticalCenter
            from: row.from; to: row.to
            value: ac.val(row.key, row.def)
            onMoved: ac.set(row.key, Math.round(value * 100) / 100)
            ToolTip.visible: hovered && !pressed
            ToolTip.delay: Theme.tooltipDelay
            ToolTip.text: row.tip + " (double-click to reset)"
            TapHandler { onDoubleTapped: ac.set(row.key, row.def) }
        }
        GLabel { id: num; anchors.right: parent.right; width: 38; horizontalAlignment: Text.AlignRight; anchors.verticalCenter: parent.verticalCenter
            text: (ac.val(row.key, row.def) - row.def).toFixed(2); color: Theme.fgSecondary; font.pixelSize: Theme.small }
    }

    contentItem: Column {
        spacing: 2
        Item {
            width: parent.width; height: 26
            GLabel { text: "Adjust Color"; font.bold: true; font.pixelSize: Theme.title; anchors.verticalCenter: parent.verticalCenter }
            GButton { anchors.right: parent.right; iconName: "window-close"; iconSize: 12; tip: "Close (Esc)"; onClicked: ac.close() }
        }
        GLabel { visible: !ac.doc; text: "Adjust Color works on pictures."; color: Theme.fgSecondary }
        // Histogram with the levels handles under it.
        Item {
            visible: !!ac.doc
            width: parent.width
            height: 90
            Rectangle { anchors.fill: hist; color: Theme.field; radius: 4 }
            Canvas {
                id: hist
                width: parent.width; height: 70
                property var h: ac.info.hist || []
                onHChanged: requestPaint()
                onPaint: {
                    const g = getContext("2d")
                    g.reset()
                    g.fillStyle = Theme.dark ? "rgba(255,255,255,0.55)" : "rgba(0,0,0,0.45)"
                    const n = h.length || 1, bw = width / n
                    for (let i = 0; i < h.length; i++) g.fillRect(i * bw, height - h[i] * (height - 4), Math.ceil(bw), h[i] * (height - 4))
                }
            }
            RangeSlider {
                id: levels
                anchors.top: hist.bottom
                width: parent.width
                from: 0; to: 1
                first.value: ac.val("black", 0)
                second.value: ac.val("white", 1)
                first.onMoved: ac.set("black", Math.round(Math.min(first.value, second.value - 0.05) * 100) / 100)
                second.onMoved: ac.set("white", Math.round(Math.max(second.value, first.value + 0.05) * 100) / 100)
                ToolTip.visible: hovered && !first.pressed && !second.pressed
                ToolTip.delay: Theme.tooltipDelay
                ToolTip.text: "Levels: drag the left handle to set what becomes black, the right one for white"
            }
        }
        Row {
            visible: !!ac.doc
            spacing: 6
            GButton { text: "Auto Levels"; flat: false; capsule: true; tip: "Stretch the tones so the darkest part is black and the lightest white"; onClicked: { const n = Object.assign({}, ac.a, { black: ac.info.autoBlack || 0, white: ac.info.autoWhite || 1 }); ac.win.changeImage(ac.doc, { adjust: n }, "Auto Levels") } }
            GButton { text: "Reset All"; flat: false; capsule: true; tip: "Undo every colour adjustment"; onClicked: ac.win.changeImage(ac.doc, { adjust: {} }, "Reset Colors") }
        }
        Column {
            visible: !!ac.doc
            width: parent.width
            Adj { key: "gamma"; label: "Midtones"; from: 0.3; to: 3; def: 1; tip: "Lighten or darken the middle tones" }
            Adj { key: "exposure"; label: "Exposure"; from: -2; to: 2; tip: "Brighten or darken the whole picture (in stops)" }
            Adj { key: "contrast"; label: "Contrast"; tip: "The difference between light and dark" }
            Adj { key: "highlights"; label: "Highlights"; tip: "Recover or brighten the lightest parts" }
            Adj { key: "shadows"; label: "Shadows"; tip: "Bring out or deepen the darkest parts" }
            Adj { key: "saturation"; label: "Saturation"; tip: "How strong the colours are (all the way left: black and white)" }
            Adj { key: "temperature"; label: "Temperature"; tip: "Cooler (blue) or warmer (orange)" }
            Adj { key: "tint"; label: "Tint"; tip: "Greener or more magenta" }
            Adj { key: "sepia"; label: "Sepia"; from: 0; to: 1; tip: "An old-photo brown tone" }
            Adj { key: "sharpness"; label: "Sharpness"; tip: "Crisper edges (right) or softer (left)" }
        }
    }
}
