import QtQuick

// Finder-style overlapping colour dots for an item's tags.
Row {
    id: t
    property string colors: ""      // comma-separated, "" for uncoloured tags
    property string tags: ""
    spacing: -3
    visible: tags !== ""
    Repeater {
        model: t.tags === "" ? [] : t.colors.split(",")
        Rectangle {
            required property string modelData
            width: 9; height: 9; radius: 5
            color: modelData !== "" ? modelData : "transparent"
            border.width: modelData !== "" ? 1 : 1.5
            border.color: modelData !== "" ? Theme.content : Theme.fgSecondary
        }
    }
}
