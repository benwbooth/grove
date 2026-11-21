import QtQuick
import QtQuick.Controls.Basic

// A Finder stack: files of one kind (or date) piled into one icon. A click
// opens it in place (its files show in a band below); another click folds
// it again. Right-click selects everything in it.
Item {
    id: sc
    property var grid
    property var stack: null           // {label, start, count}
    property int rev: 0
    property bool open: false
    readonly property int icon: grid.icon
    readonly property var pile: {
        rev
        const out = []
        if (!stack) return out
        for (let k = Math.min(2, stack.count - 1); k >= 0; k--) out.push(JSON.parse(grid.fm.get(stack.start + k)))
        return out
    }

    Rectangle {
        id: iconBg
        x: grid.labelRight ? 4 : Math.round((sc.width - width) / 2)
        y: grid.labelRight ? Math.round((sc.height - height) / 2) : 4
        width: sc.icon + 14
        height: sc.icon + 10
        radius: Math.round(8 * Theme.pt)
        color: sc.open ? Theme.selectedInactive : (ma.containsMouse ? Theme.hover : "transparent")
    }
    // The pile: up to three items, the back ones turned a little.
    Repeater {
        model: sc.pile
        Item {
            required property var modelData
            required property int index
            readonly property int depth: sc.pile.length - 1 - index
            width: sc.icon * 0.86
            height: sc.icon * 0.86
            anchors.centerIn: iconBg
            anchors.horizontalCenterOffset: depth * sc.icon * 0.05
            anchors.verticalCenterOffset: -depth * sc.icon * 0.05
            rotation: depth === 0 ? 0 : (depth === 1 ? 5 : -6)
            Rectangle {
                anchors.fill: parent
                radius: 4
                color: Theme.dark ? "#2c2c2e" : "white"
                border.color: Theme.glassEdge
                border.width: 1
            }
            GIcon {
                anchors.centerIn: parent
                size: parent.width * 0.8
                name: modelData.icon || ""
                visible: th.status !== Image.Ready
            }
            Image {
                id: th
                anchors.fill: parent
                anchors.margins: 2
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                sourceSize.width: Math.ceil(sc.icon * Screen.devicePixelRatio)
                sourceSize.height: Math.ceil(sc.icon * Screen.devicePixelRatio)
                source: modelData.thumb && sc.grid.thumbs && sc.icon >= 32 ? "image://thumb/" + encodeURIComponent(modelData.path) + "?m=" + modelData.mtime : ""
            }
        }
    }
    // Item count badge.
    Rectangle {
        x: iconBg.x + iconBg.width - width + 2
        y: iconBg.y + iconBg.height - height + 2
        height: 18
        width: Math.max(18, countL.implicitWidth + 10)
        radius: 9
        color: Theme.fgSecondary
        GLabel { id: countL; anchors.centerIn: parent; text: sc.stack && sc.stack.count ? "" + sc.stack.count : ""; color: Theme.content; font.pixelSize: Theme.caption; font.bold: true }
    }
    Rectangle {
        id: labelBg
        x: grid.labelRight ? iconBg.x + iconBg.width + 4 : Math.round((sc.width - width) / 2)
        y: grid.labelRight ? Math.round((sc.height - height) / 2) : iconBg.y + iconBg.height + 3
        width: Math.min(label.implicitWidth, sc.width - 6) + 10
        height: label.height + 2
        radius: 5
        color: sc.open ? Theme.accent : "transparent"
        GLabel {
            id: label
            anchors.centerIn: parent
            width: Math.min(implicitWidth, sc.width - 16)
            text: sc.stack && sc.stack.label ? sc.stack.label : ""
            font.pixelSize: sc.grid.textPx
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            maximumLineCount: 2
            elide: Text.ElideRight
            color: sc.open ? Theme.accentText : sc.grid.textColor
        }
    }
    MouseArea {
        id: ma
        x: Math.min(iconBg.x, labelBg.x)
        y: iconBg.y
        width: Math.max(iconBg.x + iconBg.width, labelBg.x + labelBg.width) - x
        height: labelBg.y + labelBg.height - y
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onPressed: (m) => {
            sc.grid.pane.activated()
            sc.grid.pane.forceActiveFocus()
            if (m.button === Qt.RightButton) {
                const rows = []
                for (let k = 0; k < sc.stack.count; k++) rows.push(sc.stack.start + k)
                sc.grid.fm.selectRows(JSON.stringify(rows), false)
            }
        }
        onClicked: (m) => { if (m.button === Qt.LeftButton) sc.grid.toggleStack(sc.stack.label) }
    }
    GTip {
        visible: ma.containsMouse
        text: (sc.stack ? sc.stack.label + ": " + sc.stack.count + " items. " : "") + (sc.open ? "Click to close the stack." : "Click to open the stack. Right-click selects everything in it.")
    }
}
