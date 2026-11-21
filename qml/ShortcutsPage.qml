import QtQuick
import QtQuick.Controls.Basic

// Settings › Keyboard: every action with its shortcuts. Click a shortcut
// to record a new one (Esc cancels, Backspace removes it), + adds another,
// ↺ goes back to the default. A shortcut another action already has asks
// first. Saved as settings.shortcuts; Help and menus show the result.
FocusScope {
    id: sp
    property var app
    implicitHeight: col.implicitHeight
    readonly property var custom: app && app.settings.shortcuts && typeof app.settings.shortcuts === "object" ? app.settings.shortcuts : ({})

    property string recId: ""          // action being recorded
    property int recIndex: -1          // which of its shortcuts (-1: a new one)
    property string pendingSeq: ""     // waiting on a conflict answer
    property var pendingOwner: null

    function save(obj) { app.grove.setSetting("shortcuts", JSON.stringify(obj)) }
    function setSeqs(id, list) {
        const c = Object.assign({}, custom)
        const e = Help.entry(id)
        if (e && JSON.stringify(e.defaults) === JSON.stringify(list)) delete c[id]
        else c[id] = list
        save(c)
    }
    function reset(id) { const c = Object.assign({}, custom); delete c[id]; save(c) }
    function startRecording(id, index) {
        recId = id; recIndex = index; pendingSeq = ""; pendingOwner = null
        catcher.forceActiveFocus()
    }
    function stopRecording() { recId = ""; recIndex = -1; pendingSeq = ""; pendingOwner = null }
    function assign(seq, steal) {
        const id = recId
        if (steal && pendingOwner) {
            const o = pendingOwner.id
            setSeqsNow(o, Help.seqs(o).filter(x => Help.normal(x) !== Help.normal(seq)))
        }
        const list = Help.seqs(id).slice()
        if (recIndex >= 0 && recIndex < list.length) list[recIndex] = seq
        else if (list.every(x => Help.normal(x) !== Help.normal(seq))) list.push(seq)
        setSeqsNow(id, list)
        stopRecording()
    }
    // Several changes in one go: work on a copy until saved.
    property var work: null
    function setSeqsNow(id, list) {
        const c = Object.assign({}, work || custom)
        const e = Help.entry(id)
        if (e && JSON.stringify(e.defaults) === JSON.stringify(list)) delete c[id]
        else c[id] = list
        work = c
        Help.custom = c
        save(c)
        Qt.callLater(() => sp.work = null)
    }
    function removeSeq(id, index) {
        const list = Help.seqs(id).slice()
        list.splice(index, 1)
        setSeqsNow(id, list)
    }

    // A recorded shortcut: take it, or ask when another action has it.
    function handleSeq(seq) {
        if (!seq || recId === "") return
        const o = Help.owner(seq, recId)
        if (o) { pendingSeq = seq; pendingOwner = o; return }
        assign(seq, false)
    }
    // Turns a key press into "Ctrl+Shift+N".
    function seqFromEvent(e) {
        const names = {}
        names[Qt.Key_Left] = "Left"; names[Qt.Key_Right] = "Right"; names[Qt.Key_Up] = "Up"; names[Qt.Key_Down] = "Down"
        names[Qt.Key_Home] = "Home"; names[Qt.Key_End] = "End"; names[Qt.Key_PageUp] = "PgUp"; names[Qt.Key_PageDown] = "PgDown"
        names[Qt.Key_Return] = "Return"; names[Qt.Key_Enter] = "Enter"; names[Qt.Key_Tab] = "Tab"; names[Qt.Key_Backtab] = "Tab"
        names[Qt.Key_Backspace] = "Backspace"; names[Qt.Key_Delete] = "Delete"; names[Qt.Key_Insert] = "Insert"
        names[Qt.Key_Space] = "Space"; names[Qt.Key_Escape] = "Esc"; names[Qt.Key_Print] = "Print"; names[Qt.Key_Pause] = "Pause"
        let k = names[e.key]
        if (!k) {
            if (e.key >= Qt.Key_F1 && e.key <= Qt.Key_F35) k = "F" + (e.key - Qt.Key_F1 + 1)
            else if (e.key >= 0x21 && e.key <= 0x7e) k = String.fromCharCode(e.key).toUpperCase()
        }
        if (!k) return ""
        const m = []
        if (e.modifiers & Qt.ControlModifier) m.push("Ctrl")
        if (e.modifiers & Qt.AltModifier) m.push("Alt")
        if ((e.modifiers & Qt.ShiftModifier) || e.key === Qt.Key_Backtab) m.push("Shift")
        if (e.modifiers & Qt.MetaModifier) m.push("Meta")
        return m.concat([k]).join("+")
    }

    // Catches the keys while recording.
    Item {
        id: catcher
        focus: sp.recId !== ""
        Keys.onPressed: (e) => {
            e.accepted = true
            if (sp.recId === "" || sp.pendingSeq !== "") return
            const plain = e.modifiers === Qt.NoModifier
            if (plain && e.key === Qt.Key_Escape) { sp.stopRecording(); return }
            if (plain && (e.key === Qt.Key_Backspace || e.key === Qt.Key_Delete) && sp.recIndex >= 0) {
                sp.removeSeq(sp.recId, sp.recIndex); sp.stopRecording(); return
            }
            sp.handleSeq(sp.seqFromEvent(e))
        }
        Keys.onShortcutOverride: (e) => e.accepted = true
    }

    Column {
        id: col
        width: parent.width
        spacing: 8
        Item {
            width: parent.width
            height: Theme.controlHeight
            GField {
                id: filterField
                width: parent.width - resetAll.width - 10
                iconName: "edit-find"
                clearable: true
                placeholderText: "Search actions or keys"
                tip: "Show only actions whose name or shortcut contains this"
            }
            PushButton {
                id: resetAll
                anchors.right: parent.right
                text: "Reset All"
                enabled: Object.keys(sp.custom).length > 0
                tip: "Put back every default shortcut"
                onClicked: { sp.stopRecording(); sp.save({}) }
            }
        }
        GLabel {
            width: parent.width
            wrapMode: Text.Wrap; elide: Text.ElideNone
            font.pixelSize: Theme.small
            color: Theme.fgSecondary
            text: "Click a shortcut and press the new keys (Esc cancels, Backspace removes it). + adds another shortcut, ↺ puts back the default. Typing, Return, Space, Esc and Tab in the file views always work."
        }
        // A conflict to settle.
        Rectangle {
            visible: sp.pendingSeq !== ""
            width: parent.width
            height: conflictCol.height + 16
            radius: 8
            color: Qt.rgba(Theme.danger.r, Theme.danger.g, Theme.danger.b, 0.12)
            border.color: Theme.danger
            Column {
                id: conflictCol
                x: 10; y: 8
                width: parent.width - 20
                spacing: 8
                GLabel {
                    width: parent.width
                    wrapMode: Text.Wrap; elide: Text.ElideNone
                    text: sp.pendingOwner ? "“" + sp.pendingSeq + "” is already the shortcut for “" + sp.pendingOwner.action + "”. Use it for “" + (Help.entry(sp.recId) || { action: "" }).action + "” instead?" : ""
                }
                Row {
                    spacing: 8
                    PushButton { text: "Use Anyway"; primary: true; tip: "Move the shortcut to this action (the other one loses it)"; onClicked: sp.assign(sp.pendingSeq, true) }
                    PushButton { text: "Cancel"; tip: "Keep the shortcuts as they are"; onClicked: sp.stopRecording() }
                }
            }
        }
        Repeater {
            model: Help.shortcuts.filter(s => {
                const q = filterField.text.toLowerCase()
                return !q || (s.action + " " + s.group + " " + Help.display(s).join(" ")).toLowerCase().indexOf(q) >= 0
            })
            Item {
                id: row
                required property var modelData
                required property int index
                readonly property var e: modelData
                readonly property bool newGroup: index === 0 || true
                width: col.width
                height: Math.max(36, chips.height + 10)
                Rectangle { anchors.fill: parent; radius: 6; color: rm.containsMouse ? Theme.hover : "transparent" }
                MouseArea { id: rm; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
                Column {
                    x: 8
                    width: parent.width * 0.42
                    anchors.verticalCenter: parent.verticalCenter
                    GLabel { width: parent.width; text: row.e.action; elide: Text.ElideRight }
                    GLabel { width: parent.width; text: row.e.group; font.pixelSize: Theme.caption; color: Theme.fgTertiary }
                }
                Flow {
                    id: chips
                    x: parent.width * 0.45
                    width: parent.width * 0.55 - 36
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 6
                    GLabel {
                        visible: !!row.e.fixed || (!row.e.fixed && Help.seqs(row.e.id).length === 0 && sp.recId !== row.e.id)
                        text: row.e.fixed ? row.e.fixed + "  (fixed)" : "none"
                        color: Theme.fgTertiary
                        font.pixelSize: Theme.small
                        height: Theme.rowHeight - 2
                        verticalAlignment: Text.AlignVCenter
                    }
                    Repeater {
                        model: row.e.fixed ? [] : Help.seqs(row.e.id)
                        Rectangle {
                            id: chip
                            required property string modelData
                            required property int index
                            readonly property bool recording: sp.recId === row.e.id && sp.recIndex === index
                            height: Theme.rowHeight - 2
                            width: chipL.implicitWidth + 16
                            radius: 5
                            color: recording ? Theme.accent : cm.containsMouse ? Theme.pressed : Theme.field
                            border.color: Theme.separator
                            GLabel { id: chipL; anchors.centerIn: parent; text: chip.recording ? (sp.pendingSeq || "Type keys…") : chip.modelData; font.pixelSize: Theme.small; color: chip.recording ? Theme.accentText : Theme.fg }
                            MouseArea { id: cm; anchors.fill: parent; hoverEnabled: true; onClicked: sp.startRecording(row.e.id, chip.index) }
                            GTip { visible: cm.containsMouse; text: "Click, then press the new shortcut for “" + row.e.action + "” (Backspace removes it)" }
                        }
                    }
                    Rectangle {
                        visible: !row.e.fixed
                        readonly property bool recording: sp.recId === row.e.id && sp.recIndex < 0
                        height: Theme.rowHeight - 2
                        width: recording ? addL.implicitWidth + 16 : height
                        radius: 5
                        color: recording ? Theme.accent : am.containsMouse ? Theme.pressed : "transparent"
                        border.color: Theme.separator
                        GLabel { id: addL; anchors.centerIn: parent; text: parent.recording ? (sp.pendingSeq || "Type keys…") : "+"; font.pixelSize: Theme.small; color: parent.recording ? Theme.accentText : Theme.fgSecondary }
                        MouseArea { id: am; anchors.fill: parent; hoverEnabled: true; onClicked: sp.startRecording(row.e.id, -1) }
                        GTip { visible: am.containsMouse; text: "Add another shortcut for “" + row.e.action + "”" }
                    }
                }
                GButton {
                    anchors.right: parent.right; anchors.rightMargin: 4
                    anchors.verticalCenter: parent.verticalCenter
                    visible: !row.e.fixed && Help.changed(row.e.id)
                    text: "↺"
                    width: 28
                    tip: "Back to the default: " + (row.e.defaults.join(", ") || "none")
                    onClicked: sp.reset(row.e.id)
                }
            }
        }
    }
}
