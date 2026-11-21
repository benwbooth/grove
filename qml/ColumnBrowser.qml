import QtQuick
import QtQuick.Controls.Basic
import Grove

// Column view (Finder's Miller columns): one column per folder from home
// (or /) down to the current folder, then a preview of the selected file.
// The last column is the pane's own model, so every action works on it.
Item {
    id: root
    property var pane
    property var fm: pane.model
    readonly property real colW: Math.round(230 * Theme.pt)
    property var chain: []
    property var current: null      // selected item in the last column

    function computeChain() {
        const p = pane.path
        if (!p || p.indexOf("/") !== 0) { chain = [p]; return }
        const home = pane.grove.home()
        const base = (p === home || p.indexOf(home + "/") === 0) ? home : "/"
        const out = [base]
        if (p !== base) {
            const rest = p.substring(base === "/" ? 1 : base.length + 1).split("/")
            let acc = base === "/" ? "" : base
            for (const r of rest) { acc += "/" + r; out.push(acc) }
        }
        chain = out
    }
    Component.onCompleted: computeChain()
    Connections {
        target: root.pane
        function onPathChanged() { root.computeChain() }
    }
    Connections {
        target: root.fm
        function onCurrentRowChanged() { root.updateCurrent() }
        function onSelectionCountChanged() { root.updateCurrent() }
    }
    function updateCurrent() {
        const r = fm.currentRow
        current = r >= 0 && fm.selectionCount === 1 ? pane.itemAt(r) : null
        Qt.callLater(() => strip.positionViewAtEnd())
    }

    function positionAt(row) {
        const col = strip.itemAtIndex(chain.length - 1)
        if (col && col.list) col.list.positionViewAtIndex(row, ListView.Contain)
    }
    function handleKey(e) {
        const shift = (e.modifiers & Qt.ShiftModifier) !== 0
        if (e.key === Qt.Key_Down) { pane.step(1, shift); return true }
        if (e.key === Qt.Key_Up) { pane.step(-1, shift); return true }
        if (e.key === Qt.Key_Right) {
            if (current && current.isDir) pane.navigate(current.path)
            return true
        }
        if (e.key === Qt.Key_Left) {
            if (chain.length > 1) pane.goUp()
            return true
        }
        return false
    }

    ListView {
        id: strip
        anchors.fill: parent
        orientation: ListView.Horizontal
        model: root.chain.length + 1
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.horizontal: ScrollBar { policy: ScrollBar.AsNeeded }
        onCountChanged: Qt.callLater(positionViewAtEnd)

        delegate: Item {
            id: column
            required property int index
            readonly property bool isPreview: index === root.chain.length
            readonly property bool isLast: index === root.chain.length - 1
            readonly property string folder: isPreview ? "" : root.chain[index]
            readonly property string selectedChild: index + 1 < root.chain.length ? root.chain[index + 1] : ""
            property alias list: lv
            width: isPreview ? Math.max(root.colW * 1.3, strip.width - root.chain.length * root.colW) : root.colW
            height: strip.height

            FolderModel {
                id: own
                onLoaded: if (column.selectedChild) selectPaths(JSON.stringify([column.selectedChild]))
            }
            readonly property var colModel: isLast ? root.fm : own
            Component.onCompleted: {
                if (!isPreview && !isLast) {
                    own.applyShowHidden(root.fm.showHidden)
                    own.applyFoldersFirst(root.fm.foldersFirst)
                    own.applySort("name", false)
                    own.loadPath(folder)
                }
            }
            onSelectedChildChanged: if (!isPreview && !isLast && selectedChild) own.selectPaths(JSON.stringify([selectedChild]))

            MouseArea {
                anchors.fill: parent
                visible: !column.isPreview
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                onPressed: (m) => {
                    root.pane.activated(); root.pane.forceActiveFocus()
                    if (!column.isLast) { root.pane.navigate(column.folder); return }
                    if (m.button === Qt.RightButton) root.pane.showBackgroundMenu()
                    else root.fm.clearSelection()
                }
            }
            ListView {
                id: lv
                visible: !column.isPreview
                anchors.fill: parent
                anchors.rightMargin: 1
                model: column.isPreview ? null : column.colModel
                clip: true
                reuseItems: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                delegate: Item {
                    id: r
                    required property int index
                    required property string name
                    required property string path
                    required property bool isDir
                    required property bool selected
                    required property string iconName
                    required property string tags
                    required property string tagColors
                    required property bool hidden
                    required property bool cut
                    width: lv.width
                    height: Theme.rowHeight
                    readonly property bool renaming: column.isLast && root.pane.renamingRow === index
                    Rectangle {
                        anchors.fill: parent
                        anchors.leftMargin: 5; anchors.rightMargin: 5
                        radius: 5
                        color: r.selected ? (column.isLast && root.pane.active ? Theme.accent : Theme.selectedInactive) : drop.hovering ? Theme.dropTarget : "transparent"
                    }
                    GIcon { id: ic; x: 12; anchors.verticalCenter: parent.verticalCenter; size: 18; name: r.iconName; opacity: r.hidden || r.cut ? 0.5 : 1 }
                    GLabel {
                        visible: !r.renaming
                        anchors.left: ic.right; anchors.leftMargin: 6
                        anchors.right: chevron.left; anchors.rightMargin: 4
                        anchors.verticalCenter: parent.verticalCenter
                        text: r.name
                        elide: Text.ElideMiddle
                        color: r.selected && column.isLast && root.pane.active ? Theme.accentText : Theme.fg
                        opacity: r.hidden || r.cut ? 0.6 : 1
                    }
                    Loader {
                        active: r.renaming
                        anchors.left: ic.right; anchors.leftMargin: 4
                        anchors.right: parent.right; anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        sourceComponent: RenameField { pane: root.pane; row: r.index; original: r.name; isDir: r.isDir }
                    }
                    TagDots { anchors.right: chevron.left; anchors.rightMargin: 4; anchors.verticalCenter: parent.verticalCenter; tags: r.tags; colors: r.tagColors }
                    GLabel {
                        id: chevron
                        anchors.right: parent.right; anchors.rightMargin: 12
                        anchors.verticalCenter: parent.verticalCenter
                        text: r.isDir ? "›" : ""
                        font.pixelSize: Theme.title
                        color: r.selected && column.isLast && root.pane.active ? Theme.accentText : Theme.fgSecondary
                    }
                    MouseArea {
                        anchors.fill: parent
                        visible: !column.isLast
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        onClicked: {
                            root.pane.activated()
                            root.pane.forceActiveFocus()
                            if (r.isDir) root.pane.navigate(r.path)
                            else root.pane.navigate(column.folder, { select: [r.path] })
                        }
                    }
                    ItemMouse {
                        anchors.fill: parent
                        visible: column.isLast
                        enabled: column.isLast && !r.renaming
                        pane: root.pane
                        row: r.index
                        iconUrl: r.iconName ? "image://icon/" + r.iconName : ""
                        onLeftPressed: (mods) => { if (r.isDir && !(mods & (Qt.ShiftModifier | Qt.ControlModifier))) openTimer.restart() }
                        Timer { id: openTimer; interval: 1; onTriggered: if (root.fm.selectionCount === 1) root.pane.navigate(r.path) }
                    }
                    FolderDrop { id: drop; anchors.fill: parent; pane: root.pane; path: r.path; enabledFor: r.isDir }
                }
            }
            Rectangle { visible: !column.isPreview; anchors.right: parent.right; width: 1; height: parent.height; color: Theme.separator }

            // The preview column.
            Loader {
                anchors.fill: parent
                active: column.isPreview && root.current !== null && !root.current.isDir
                sourceComponent: PreviewPanel { pane: root.pane; item: root.current }
            }
        }
    }
}
