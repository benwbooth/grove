import QtQuick
import QtQuick.Controls.Basic

// Rename several items at once (Finder's "Rename Finder Items"): replace
// text, add text, number them, change case — with a live preview and
// clash checks. Undo puts the old names back.
Sheet {
    id: br
    property var app
    property var win
    property var paths: []
    property var plan: []
    property string mode: "replace"
    readonly property bool ok: plan.length > 0 && plan.every(p => p.problem === "")
    title: "Rename " + paths.length + " Items"
    width: Math.min(parent ? parent.width - 40 : 640, Math.round(600 * Theme.pt))

    function start(p) { paths = p; mode = "replace"; findField.text = ""; replField.text = ""; open(); refresh() }
    function rule() {
        return {
            mode: mode, find: findField.text, replace: replField.text, regex: regexBox.checked, case_sensitive: caseBox.checked,
            base: baseField.text, start: Number(startField.text) || 1, digits: Number(digitsField.text) || 1,
            case: caseMode.current, text: addField.text, keep_extension: keepExt.checked
        }
    }
    function refresh() {
        const r = JSON.stringify(rule())
        Async.call(app.grove, k => app.grove.renamePreview(k, JSON.stringify(paths), r), res => br.plan = res || [])
    }

    Segmented {
        items: [
            { icon: "edit-find-replace", tip: "Replace text in the names", value: "replace" },
            { icon: "list-add", tip: "Add text before or after the names", value: "prefix" },
            { icon: "format-list-ordered", tip: "Number the items: a name and a counter", value: "number" },
            { icon: "format-text-capitalize|format-text-uppercase", tip: "Change upper/lower case", value: "case" }
        ]
        current: br.mode === "suffix" ? "prefix" : br.mode
        onPicked: (v) => { br.mode = v; br.refresh() }
    }
    GLabel { text: ({ replace: "Replace text", prefix: "Add text", number: "Number them", case: "Change case" })[br.mode]; color: Theme.fgSecondary }
    Row {
        visible: br.mode === "replace"
        spacing: 8
        GField { id: findField; width: 200; placeholderText: "Find"; tip: "Text to look for in each name"; onTextChanged: br.refresh() }
        GField { id: replField; width: 200; placeholderText: "Replace with"; tip: "What to put instead (may be empty)"; onTextChanged: br.refresh() }
    }
    Row {
        visible: br.mode === "replace"
        spacing: 16
        GCheck { id: caseBox; text: "Match case"; onToggled: br.refresh() }
        GCheck { id: regexBox; text: "Regular expression"; tip: "Treat Find as a regular expression ($1 refers to groups)"; onToggled: br.refresh() }
    }
    Row {
        visible: br.mode === "prefix" || br.mode === "suffix"
        spacing: 8
        GField { id: addField; width: 200; placeholderText: "Text to add"; onTextChanged: br.refresh() }
        Segmented {
            id: where
            items: [{ icon: "go-first", tip: "Before the name", value: "prefix" }, { icon: "go-last", tip: "After the name", value: "suffix" }]
            current: "prefix"
            onPicked: (v) => { current = v; br.mode = v === "suffix" ? "suffix" : "prefix"; br.refresh() }
        }
    }
    Row {
        visible: br.mode === "number"
        spacing: 8
        GField { id: baseField; width: 180; placeholderText: "Name (e.g. Holiday)"; onTextChanged: br.refresh() }
        GField { id: startField; width: 80; text: "1"; placeholderText: "Start"; tip: "First number"; onTextChanged: br.refresh() }
        GField { id: digitsField; width: 80; text: "3"; placeholderText: "Digits"; tip: "At least this many digits (003)"; onTextChanged: br.refresh() }
    }
    Segmented {
        id: caseMode
        visible: br.mode === "case"
        items: [
            { icon: "format-text-lowercase", tip: "all lower case", value: "lower" },
            { icon: "format-text-uppercase", tip: "ALL UPPER CASE", value: "upper" },
            { icon: "format-text-capitalize", tip: "Title Case For Every Word", value: "title" },
            { icon: "format-text-bold", tip: "Sentence case", value: "sentence" }
        ]
        current: "title"
        onPicked: (v) => { current = v; br.refresh() }
    }
    GCheck { id: keepExt; text: "Leave file extensions unchanged"; checked: true; onToggled: br.refresh() }

    Rectangle {
        width: parent.width
        height: Math.min(240, Math.max(60, preview.contentHeight + 8))
        radius: 8
        color: Theme.field
        KineticWheel { flickable: preview }
        ListView {
            id: preview
            anchors.fill: parent
            anchors.margins: 4
            clip: true
            model: br.plan
            delegate: Item {
                required property var modelData
                width: preview.width
                height: Theme.rowHeight
                GLabel { x: 6; width: parent.width / 2 - 20; anchors.verticalCenter: parent.verticalCenter; text: modelData.from.substring(modelData.from.lastIndexOf("/") + 1); color: Theme.fgSecondary; elide: Text.ElideMiddle }
                GLabel { x: parent.width / 2 - 10; text: "→"; anchors.verticalCenter: parent.verticalCenter; color: Theme.fgTertiary }
                GLabel {
                    x: parent.width / 2 + 10
                    width: parent.width / 2 - 16
                    anchors.verticalCenter: parent.verticalCenter
                    text: modelData.problem !== "" ? modelData.to + " — " + modelData.problem : modelData.to
                    color: modelData.problem !== "" ? Theme.danger : Theme.fg
                    elide: Text.ElideMiddle
                }
            }
        }
    }
    buttons: [
        PushButton { text: "Cancel"; onClicked: br.close() },
        PushButton {
            text: "Rename"; primary: true; enabled: br.ok
            tip: br.ok ? "Rename the items (Ctrl+Z undoes it)" : "Fix the names shown in red first"
            onClicked: Async.call(br.app.grove, k => br.app.grove.renameApply(k, JSON.stringify(br.paths), JSON.stringify(br.rule())), r => {
                if (r.error) br.win.toast("error", r.error)
                else { br.close(); br.win.activePane.selectWhenThere(r.paths[0] || "", false) }
            })
        }
    ]
}
