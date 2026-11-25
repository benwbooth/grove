import QtQuick
import QtQuick.Controls.Basic

// The text viewer's language menu: which syntax colours this file.
// "Automatic" (what the name and content say) or any language there is,
// typed to narrow the list. The choice is kept with the file (TextDoc).
GButton {
    id: sp
    required property var doc
    property bool slim: false
    readonly property string current: doc.syntax !== "" ? doc.syntax : "Automatic"
    readonly property string autoLabel: "Automatic" + (doc.detected ? " (" + doc.detected + ")" : "")
    text: (doc.syntax !== "" ? doc.syntax : doc.detected || "Automatic") + "  ⌄"
    implicitHeight: Math.round((slim ? 22 : 28) * Theme.pt)
    tip: doc.syntax !== ""
        ? "Colouring as " + doc.syntax + ", chosen for this file (click to change; Automatic goes back to " + (doc.detected || "the guess") + ")"
        : "Syntax colouring: " + (doc.detected || "automatic") + ", picked from the name and content (click to choose another language for this file)"
    checked: menu.visible
    onClicked: open()

    function open() {
        doc.requestLanguages()
        search.text = ""
        menu.open()
        search.forceActiveFocus()
    }
    // Picks a language ("" = Automatic) as the menu does (tests too).
    function choose(lang) {
        menu.close()
        if (lang !== doc.syntax) doc.chooseSyntax(lang)
    }

    readonly property var all: doc.languages ? JSON.parse(doc.languages) : []
    readonly property var shown: {
        const q = search.text.trim().toLowerCase()
        const l = q ? all.filter(n => n.toLowerCase().indexOf(q) >= 0) : all
        // Names that start with what's typed first.
        if (q) l.sort((a, b) => (b.toLowerCase().startsWith(q) ? 1 : 0) - (a.toLowerCase().startsWith(q) ? 1 : 0))
        return (q && "automatic".indexOf(q) < 0 ? [] : [""]).concat(l)
    }

    Popup {
        id: menu
        y: sp.height + 2
        x: Math.min(0, sp.width - width)
        width: Math.round(240 * Theme.pt)
        height: Math.min(Math.round(420 * Theme.pt), col.implicitHeight + 8.001)
        padding: 4
        topPadding: 4.001
        leftPadding: 4.001
        background: GlassPanel { tint: Theme.glassMenu }
        onOpened: list.showCurrent()
        Column {
            id: col
            width: parent.width
            spacing: 4
            GField {
                id: search
                width: parent.width
                iconName: "edit-find"
                placeholderText: "Language"
                tip: "Type to find a language"
                onTextChanged: list.currentIndex = sp.shown.length > 1 && text ? 1 : 0
                Keys.onDownPressed: list.currentIndex = Math.min(list.count - 1, list.currentIndex + 1)
                Keys.onUpPressed: list.currentIndex = Math.max(0, list.currentIndex - 1)
                Keys.onReturnPressed: if (list.currentIndex >= 0 && list.currentIndex < sp.shown.length) sp.choose(sp.shown[list.currentIndex])
                Keys.onEscapePressed: menu.close()
            }
            Item {
                width: parent.width
                height: list.height
                // Wheel and trackpad scrolling with grove's glide.
                KineticWheel { flickable: list; notch: Theme.menuRow * 3 }
                ListView {
                    id: list
                    width: parent.width
                    height: Math.min(contentHeight, Math.round(420 * Theme.pt) - search.height - 12)
                    clip: true
                    model: sp.shown
                    highlightMoveDuration: 0
                    // On the current choice (the list comes from a worker, so
                    // again once it's there).
                    function showCurrent() {
                        currentIndex = Math.max(0, sp.shown.indexOf(sp.doc.syntax))
                        positionViewAtIndex(currentIndex, ListView.Center)
                    }
                    onCountChanged: if (menu.visible && search.text === "") showCurrent()
                    ScrollBar.vertical: GScrollBar {}
                    delegate: ItemDelegate {
                        id: d
                        required property int index
                        required property string modelData
                        width: list.width - 4
                        height: Theme.menuRow
                        highlighted: list.currentIndex === index
                        hoverEnabled: true
                        onHoveredChanged: if (hovered) list.currentIndex = index
                        background: Rectangle { radius: 6; color: d.highlighted ? Theme.accent : "transparent" }
                        contentItem: GLabel {
                            text: (d.modelData === sp.doc.syntax ? "✓ " : "    ") + (d.modelData === "" ? sp.autoLabel : d.modelData)
                            color: d.highlighted ? Theme.accentText : Theme.fg
                            elide: Text.ElideRight
                            leftPadding: 4
                        }
                        onClicked: sp.choose(d.modelData)
                    }
                }
            }
        }
    }
}
