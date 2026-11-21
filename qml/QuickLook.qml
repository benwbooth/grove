import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window
import QtMultimedia
import QtQuick.Pdf

// Quick Look (Space): a floating preview of the selected file — pictures,
// video and audio, PDF, code with syntax highlighting, Markdown, archive
// contents, folder summaries. Arrow keys move to the next file while it's
// open; Space or Esc closes it.
Window {
    id: ql
    property var app
    property var win
    property var pane
    property var item: null
    property var pv: ({})
    // Editing: "" (viewing), "markup" (pictures), "trim" (video and audio).
    property string editing: ""
    property bool saving: false
    // Test hook: a few marks, as if drawn.
    function markupDemo() {
        const m = markup
        m.addOp({ t: "pen", c: "#ff3b30", w: 6, a: 1, p: [60, 300, 120, 250, 200, 320, 280, 260, 360, 330] })
        m.addOp({ t: "pen", c: "#ffcc00", w: 24, a: 0.35, p: [80, 120, 500, 120] })
        m.addOp({ t: "rect", c: "#007aff", w: 5, x1: 420, y1: 360, x2: 700, y2: 520 })
        m.addOp({ t: "ellipse", c: "#34c759", w: 5, x1: 520, y1: 60, x2: 740, y2: 200 })
        m.addOp({ t: "arrow", c: "#ffffff", w: 5, x1: 260, y1: 480, x2: 410, y2: 430 })
        m.addOp({ t: "text", c: "#ffffff", s: 36, x1: 90, y1: 440, text: "grove markup" })
    }
    function trimFrom(a, b) { trimRange.setValues(a, b) }
    function stopEditing() { editing = ""; markup.reset() }
    function saveEdit(mode) {
        const p = item.path
        saving = true
        const done = r => {
            saving = false
            if (r.error) { win.toast("error", r.error); return }
            win.toast("info", r.replaced ? "Saved. The original is in the Trash (Undo puts it back)." : "Saved “" + r.path.substring(r.path.lastIndexOf("/") + 1) + "”" + (r.converted ? " (this kind of file can't be written back, so it's a copy)" : ""))
            stopEditing()
            if (r.replaced) { pv = { kind: "loading" }; Qt.callLater(() => showItem(item)) }
        }
        if (editing === "markup") Async.call(app.grove, k => app.grove.markupSave(k, p, JSON.stringify(markup.spec()), mode), done)
        else Async.call(app.grove, k => app.grove.mediaTrim(k, p, trimRange.first.value, trimRange.second.value, mode), done)
    }
    width: 900
    height: 640
    minimumWidth: 320
    minimumHeight: 240
    color: Theme.glassMenu
    flags: Qt.Window | Qt.FramelessWindowHint
    title: item ? item.name + " — Quick Look" : "Quick Look"
    transientParent: win

    function showItem(it) {
        if (editing !== "") stopEditing()
        item = it
        pv = { kind: "loading" }
        player.stop()
        const p = it.path
        if (p.indexOf("/") !== 0) { pv = { kind: "other" }; return }
        Async.call(app.grove, k => app.grove.preview(k, p, Theme.dark), r => { if (ql.item && ql.item.path === p) ql.pv = r })
        if (!visible) { show(); raise(); requestActivate() }
    }
    onVisibleChanged: if (!visible) { player.stop(); if (pane) pane.forceActiveFocus() }

    Item {
        anchors.fill: parent
        focus: true
        transform: Translate { x: 0.001; y: 0.001 }
        Keys.onPressed: (e) => {
            if (ql.editing !== "") {
                if (e.key === Qt.Key_Escape) { ql.stopEditing(); e.accepted = true }
                else if (e.key === Qt.Key_Z && (e.modifiers & Qt.ControlModifier)) { markup.undo(); e.accepted = true }
                return
            }
            if (e.key === Qt.Key_Space || e.key === Qt.Key_Escape) { ql.close(); e.accepted = true }
            else if (e.key === Qt.Key_Right || e.key === Qt.Key_Down) { ql.pane.step(1, false); e.accepted = true }
            else if (e.key === Qt.Key_Left || e.key === Qt.Key_Up) { ql.pane.step(-1, false); e.accepted = true }
            else if (e.key === Qt.Key_Return) { ql.pane.openSelected(false); ql.close(); e.accepted = true }
        }

        Rectangle { anchors.fill: parent; color: "transparent"; border.color: Theme.glassEdge; border.width: 1; radius: 0 }

        // Title bar.
        Item {
            id: bar
            width: parent.width
            height: Math.round(40 * Theme.pt)
            MouseArea { anchors.fill: parent; onPressed: ql.startSystemMove() }
            GButton { x: 10; anchors.verticalCenter: parent.verticalCenter; iconName: "window-close"; iconSize: 12; tip: "Close (Space or Esc)"; onClicked: ql.close() }
            GButton {
                x: 44; anchors.verticalCenter: parent.verticalCenter
                iconName: ql.visibility === Window.FullScreen ? "view-restore" : "view-fullscreen"; iconSize: 14
                tip: "Full screen"
                onClicked: ql.visibility === Window.FullScreen ? ql.showNormal() : ql.showFullScreen()
            }
            GLabel {
                anchors.centerIn: parent
                width: parent.width - 260
                horizontalAlignment: Text.AlignHCenter
                text: ql.item ? ql.item.name : ""
                font.bold: true
            }
            Row {
                anchors.right: parent.right; anchors.rightMargin: 10
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                visible: ql.editing !== ""
                GButton { text: "Cancel"; flat: false; capsule: true; tip: "Stop editing without saving (Esc)"; onClicked: ql.stopEditing() }
                GButton { text: "Save as Copy"; flat: false; capsule: true; enabled: !ql.saving && (ql.editing === "trim" || markup.dirty); tip: "Save the result next to the original, as a new file"; onClicked: ql.saveEdit("copy") }
                GButton { text: "Save"; flat: false; capsule: true; tint: Theme.accent; enabled: !ql.saving && (ql.editing === "trim" || markup.dirty); tip: "Save over the original (it goes to the Trash, so Undo can bring it back)"; onClicked: ql.saveEdit("replace") }
            }
            Row {
                anchors.right: parent.right; anchors.rightMargin: 10
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                visible: ql.editing === ""
                GButton {
                    visible: ql.pv.kind === "image" && ql.item && ql.item.path.indexOf("/") === 0
                    iconName: "draw-freehand|document-edit"
                    tip: "Markup: draw, add shapes and text, crop or rotate"
                    onClicked: ql.editing = "markup"
                }
                GButton {
                    visible: ql.pv.kind === "media" && ql.item && ql.item.path.indexOf("/") === 0
                    iconName: "edit-cut"
                    tip: "Trim: keep only part of this " + (ql.pv.video ? "video" : "recording")
                    onClicked: { ql.editing = "trim"; trimRange.first.value = 0; trimRange.second.value = Math.max(1, player.duration); player.pause() }
                }
                GButton {
                    text: ql.pv.kind === "folder" ? "Open" : "Open with " + (ql.pv.app || "default app")
                    flat: false; capsule: true
                    tip: "Open it (Return)"
                    onClicked: { ql.pane.openSelected(false); ql.close() }
                }
            }
        }

        // Markup tools (Finder's markup toolbar).
        Item {
            id: tools
            anchors.top: bar.bottom
            width: parent.width
            height: ql.editing === "markup" ? Math.round(40 * Theme.pt) : 0
            visible: height > 0
            Row {
                anchors.centerIn: parent
                spacing: 4
                Repeater {
                    model: [
                        { t: "pen", icon: "draw-freehand", tip: "Pen: draw freehand" },
                        { t: "highlight", icon: "draw-highlight|draw-brush", tip: "Highlighter: a wide see-through stroke" },
                        { t: "rect", icon: "draw-rectangle", tip: "Rectangle" },
                        { t: "ellipse", icon: "draw-ellipse", tip: "Ellipse" },
                        { t: "line", icon: "draw-line", tip: "Line" },
                        { t: "arrow", icon: "draw-arrow", tip: "Arrow" },
                        { t: "text", icon: "draw-text|insert-text", tip: "Text: click where it goes, type, Return" },
                        { t: "crop", icon: "crop|transform-crop", tip: "Crop: drag the part to keep" }
                    ]
                    GButton {
                        required property var modelData
                        iconName: modelData.icon
                        checked: markup.tool === modelData.t
                        tip: modelData.tip
                        onClicked: markup.tool = modelData.t
                    }
                }
                Rectangle { width: 1; height: 20; color: Theme.separator; anchors.verticalCenter: parent.verticalCenter }
                GButton { iconName: "object-rotate-left"; tip: "Rotate left"; onClicked: markup.rotate(-90) }
                GButton { iconName: "object-rotate-right"; tip: "Rotate right"; onClicked: markup.rotate(90) }
                Rectangle { width: 1; height: 20; color: Theme.separator; anchors.verticalCenter: parent.verticalCenter }
                Repeater {
                    model: markup.swatches
                    Rectangle {
                        required property string modelData
                        anchors.verticalCenter: parent.verticalCenter
                        width: 18; height: 18; radius: 9
                        color: modelData
                        border.width: Qt.colorEqual(markup.color, modelData) ? 2.5 : 1
                        border.color: Qt.colorEqual(markup.color, modelData) ? Theme.accent : Theme.glassEdge
                        MouseArea { id: swm; anchors.fill: parent; anchors.margins: -2; hoverEnabled: true; onClicked: markup.color = parent.modelData }
                        GTip { visible: swm.containsMouse; text: "Draw in this colour" }
                    }
                }
                Rectangle { width: 1; height: 20; color: Theme.separator; anchors.verticalCenter: parent.verticalCenter }
                Repeater {
                    model: [{ s: 0, d: 4, tip: "Thin lines, small text" }, { s: 1, d: 7, tip: "Medium lines and text" }, { s: 2, d: 11, tip: "Thick lines, big text" }]
                    GButton {
                        required property var modelData
                        checked: markup.size === modelData.s
                        tip: modelData.tip
                        onClicked: markup.size = modelData.s
                        contentItem: Item { Rectangle { anchors.centerIn: parent; width: parent.parent.modelData.d; height: width; radius: width / 2; color: Theme.fg } }
                    }
                }
                Rectangle { width: 1; height: 20; color: Theme.separator; anchors.verticalCenter: parent.verticalCenter }
                GButton { iconName: "edit-undo"; enabled: markup.dirty; tip: "Undo the last change (Ctrl+Z)"; onClicked: markup.undo() }
            }
        }

        Item {
            id: body
            anchors.top: tools.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: foot.top
            anchors.margins: 8
            clip: true

            BusyDots { anchors.centerIn: parent; running: ql.pv.kind === "loading" }

            Markup {
                id: markup
                anchors.fill: parent
                visible: ql.editing === "markup"
                src: ql.editing === "markup" && ql.item ? "file://" + encodeURI(ql.item.path).replace(/#/g, "%23") : ""
            }
            Image {
                anchors.fill: parent
                cache: false
                visible: ql.pv.kind === "image" && ql.editing !== "markup"
                source: ql.pv.kind === "image" ? "file://" + encodeURI(ql.pv.src).replace(/#/g, "%23") : ""
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                autoTransform: true
                smooth: true
                mipmap: true
            }

            Item {
                anchors.fill: parent
                visible: ql.pv.kind === "media"
                MediaPlayer {
                    id: player
                    source: ql.pv.kind === "media" ? "file://" + encodeURI(ql.item.path).replace(/#/g, "%23") : ""
                    videoOutput: video
                    audioOutput: AudioOutput {}
                    onSourceChanged: if (source != "") play()
                }
                VideoOutput { id: video; anchors.fill: parent; anchors.bottomMargin: ql.editing === "trim" ? 80 : 44; visible: !!ql.pv.video }
                GIcon { anchors.centerIn: parent; size: 128; name: ql.item ? ql.item.icon : ""; visible: !ql.pv.video }
                // Trim: the part to keep, between the two handles.
                Column {
                    visible: ql.editing === "trim"
                    anchors.bottom: parent.bottom
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: 4
                    width: Math.min(640, body.width - 40)
                    RangeSlider {
                        id: trimRange
                        width: parent.width
                        from: 0; to: Math.max(1, player.duration, ql.item ? 1000 * (ql.pv.seconds || 0) : 0)
                        first.onMoved: player.position = first.value
                        second.onMoved: player.position = second.value
                        background: Rectangle {
                            x: trimRange.leftPadding; y: trimRange.topPadding + trimRange.availableHeight / 2 - 14
                            width: trimRange.availableWidth; height: 28; radius: 6
                            color: Theme.field
                            Rectangle {
                                x: trimRange.first.visualPosition * parent.width
                                width: (trimRange.second.visualPosition - trimRange.first.visualPosition) * parent.width
                                height: parent.height; radius: 6
                                color: "transparent"
                                border.width: 3; border.color: "#ffcc00"
                            }
                            Rectangle { x: player.duration > 0 ? player.position / player.duration * parent.width : 0; width: 2; height: parent.height; color: Theme.fg }
                        }
                        first.handle: Rectangle { x: trimRange.leftPadding + trimRange.first.visualPosition * (trimRange.availableWidth - width); y: trimRange.topPadding + trimRange.availableHeight / 2 - 15; width: 10; height: 30; radius: 3; color: "#ffcc00" }
                        second.handle: Rectangle { x: trimRange.leftPadding + trimRange.second.visualPosition * (trimRange.availableWidth - width); y: trimRange.topPadding + trimRange.availableHeight / 2 - 15; width: 10; height: 30; radius: 3; color: "#ffcc00" }
                        GTip { visible: trimRange.hovered; text: "Drag the yellow handles to the start and end of the part to keep" }
                    }
                    Row {
                        anchors.horizontalCenter: parent.horizontalCenter
                        spacing: 10
                        GButton { iconName: player.playbackState === MediaPlayer.PlayingState ? "media-playback-pause" : "media-playback-start"; tip: "Play the trimmed part"; onClicked: { if (player.playbackState === MediaPlayer.PlayingState) player.pause(); else { if (player.position < trimRange.first.value || player.position >= trimRange.second.value) player.position = trimRange.first.value; player.play() } } }
                        GLabel {
                            anchors.verticalCenter: parent.verticalCenter
                            color: Theme.fgSecondary
                            text: "Keep " + fmt(trimRange.first.value) + " – " + fmt(trimRange.second.value) + "  (" + fmt(trimRange.second.value - trimRange.first.value) + ")"
                            function fmt(ms) { const s = Math.floor(ms / 1000); return Math.floor(s / 60) + ":" + ("0" + (s % 60)).slice(-2) + "." + Math.floor((ms % 1000) / 100) }
                        }
                    }
                    // Stop at the end of the part being kept.
                    Connections {
                        target: player
                        enabled: ql.editing === "trim"
                        function onPositionChanged() { if (player.playbackState === MediaPlayer.PlayingState && player.position >= trimRange.second.value) player.pause() }
                    }
                }
                Row {
                    visible: ql.editing !== "trim"
                    anchors.bottom: parent.bottom
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: 10
                    height: 36
                    GButton { iconName: player.playbackState === MediaPlayer.PlayingState ? "media-playback-pause" : "media-playback-start"; tip: "Play / pause"; onClicked: player.playbackState === MediaPlayer.PlayingState ? player.pause() : player.play(); anchors.verticalCenter: parent.verticalCenter }
                    Slider {
                        width: Math.min(500, body.width - 200)
                        anchors.verticalCenter: parent.verticalCenter
                        from: 0; to: Math.max(1, player.duration, ql.item ? 1000 * (ql.pv.seconds || 0) : 0)
                        value: player.position
                        onMoved: player.position = value
                    }
                    GLabel {
                        anchors.verticalCenter: parent.verticalCenter
                        text: fmt(player.position) + " / " + fmt(player.duration)
                        color: Theme.fgSecondary
                        function fmt(ms) { const s = Math.floor(ms / 1000); return Math.floor(s / 60) + ":" + ("0" + (s % 60)).slice(-2) }
                    }
                }
            }

            Loader {
                anchors.fill: parent
                active: ql.pv.kind === "pdf"
                sourceComponent: PdfMultiPageView {
                    document: PdfDocument { source: "file://" + encodeURI(ql.item.path).replace(/#/g, "%23") }
                }
            }

            ScrollView {
                anchors.fill: parent
                visible: ql.pv.kind === "code"
                TextEdit {
                    readOnly: true
                    selectByMouse: true
                    textFormat: TextEdit.RichText
                    text: ql.pv.kind === "code" ? ql.pv.html : ""
                    font.family: Theme.mono
                    font.pixelSize: Theme.body * 0.95
                    renderType: Text.NativeRendering
                    color: Theme.fg
                    selectionColor: Theme.accent
                    padding: 12
                }
            }

            ScrollView {
                id: mdView
                anchors.fill: parent
                visible: ql.pv.kind === "markdown"
                TextEdit {
                    width: mdView.width - 24
                    readOnly: true
                    selectByMouse: true
                    textFormat: TextEdit.MarkdownText
                    wrapMode: TextEdit.Wrap
                    text: ql.pv.kind === "markdown" ? ql.pv.text : ""
                    font.family: Theme.family
                    font.pixelSize: Theme.body
                    renderType: Text.NativeRendering
                    color: Theme.fg
                    selectionColor: Theme.accent
                    padding: 16
                }
            }

            ListView {
                anchors.fill: parent
                visible: ql.pv.kind === "archive"
                clip: true
                model: ql.pv.kind === "archive" ? ql.pv.entries : []
                ScrollBar.vertical: ScrollBar {}
                header: GLabel { text: ql.pv.total + " items in this archive" + (ql.pv.total > 500 ? " (first 500 shown)" : ""); color: Theme.fgSecondary; padding: 8 }
                delegate: Item {
                    required property var modelData
                    width: ListView.view.width
                    height: Theme.rowHeight
                    GIcon { x: 8 + (modelData.path.split("/").length - 1) * 14; anchors.verticalCenter: parent.verticalCenter; size: 16; name: modelData.dir ? "folder" : "text-x-generic" }
                    GLabel { x: 30 + (modelData.path.split("/").length - 1) * 14; anchors.verticalCenter: parent.verticalCenter; text: modelData.path.substring(modelData.path.lastIndexOf("/") + 1) }
                    GLabel { anchors.right: parent.right; anchors.rightMargin: 16; anchors.verticalCenter: parent.verticalCenter; text: modelData.dir ? "" : ql.app.size(modelData.size); color: Theme.fgSecondary }
                }
            }

            Column {
                anchors.centerIn: parent
                spacing: 12
                visible: ql.pv.kind === "folder" || ql.pv.kind === "other" || ql.pv.kind === "error"
                GIcon { anchors.horizontalCenter: parent.horizontalCenter; size: 160; name: ql.item ? ql.item.icon : "" }
                GLabel { anchors.horizontalCenter: parent.horizontalCenter; text: ql.item ? ql.item.name : ""; font.pixelSize: Theme.large; font.bold: true }
                GLabel {
                    anchors.horizontalCenter: parent.horizontalCenter
                    color: Theme.fgSecondary
                    text: ql.pv.kind === "folder" ? (ql.pv.folders + " folders, " + ql.pv.files + " files") : ql.pv.kind === "error" ? ql.pv.error : ((ql.pv.kindText || "") + (ql.pv.sizeText ? " — " + ql.pv.sizeText : ""))
                }
                GLabel {
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: ql.pv.kind === "folder" && (ql.pv.sample || []).length > 0
                    text: (ql.pv.sample || []).join("   ·   ")
                    color: Theme.fgTertiary
                    width: Math.min(implicitWidth, body.width - 40)
                    wrapMode: Text.Wrap
                    horizontalAlignment: Text.AlignHCenter
                }
                GLabel { anchors.horizontalCenter: parent.horizontalCenter; visible: !!ql.pv.note; text: ql.pv.note || ""; color: Theme.fgSecondary; width: Math.min(implicitWidth, body.width - 40); wrapMode: Text.Wrap }
            }
        }
        GLabel {
            id: foot
            anchors.bottom: parent.bottom
            width: parent.width
            height: 26
            horizontalAlignment: Text.AlignHCenter
            font.pixelSize: Theme.small
            color: Theme.fgSecondary
            text: (ql.pv.kindText || "") + (ql.pv.sizeText ? "  ·  " + ql.pv.sizeText : "") + (ql.pv.lines ? "  ·  " + ql.pv.lines + " lines" + (ql.pv.truncated ? " (start shown)" : "") : "") + "     ←/→ next file · Space closes"
        }
    }
}
