import QtQuick
import QtQuick.Controls.Basic

// Inline rename: the name without its extension is selected first (as in
// Finder); Return saves, Esc cancels.
TextField {
    id: f
    property var pane
    property int row: -1
    property string original: ""
    property bool isDir: false
    font.family: Theme.family
    font.pixelSize: Theme.body
    renderType: Text.NativeRendering
    color: Theme.fg
    selectionColor: Theme.accent
    selectedTextColor: Theme.accentText
    padding: 2
    leftPadding: 4
    rightPadding: 4
    horizontalAlignment: TextInput.AlignLeft
    background: Rectangle { color: Theme.window; border.color: Theme.accent; border.width: 2; radius: 4 }
    property bool done: false
    Component.onCompleted: {
        text = original
        forceActiveFocus()
        const dot = original.lastIndexOf(".")
        if (!isDir && dot > 0) select(0, dot)
        else selectAll()
    }
    Keys.onReturnPressed: { done = true; pane.commitRename(row, text) }
    Keys.onEnterPressed: { done = true; pane.commitRename(row, text) }
    Keys.onEscapePressed: { done = true; pane.renamingRow = -1; pane.forceActiveFocus() }
    onActiveFocusChanged: if (!activeFocus && !done) { done = true; pane.commitRename(row, text) }
}
