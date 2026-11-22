import QtQuick
import QtQuick.Controls.Basic

// Dolphin's tooltip previews: hovering an item shows a bigger preview with
// its kind, size and date (Settings › "Previews in tooltips"). Without the
// setting it only shows a name that doesn't fit.
ToolTip {
    id: tip
    property var pane
    property int row: -1
    property string path: ""
    property string name: ""
    property bool thumb: false
    property real mtime: 0
    property bool truncated: false
    // In the app window (a separate popup window came up empty under arbor
    // and caught the scroll wheel).
    popupType: Popup.Item
    property bool hovering: false
    readonly property bool rich: !!(pane && pane.app && pane.app.settings.preview_tooltips)
    property var item: null
    readonly property bool forced: !!pane && pane.testTip === row && row >= 0
    visible: (hovering || forced) && (rich || truncated)
    onVisibleChanged: if (visible && rich && row >= 0) item = pane.itemAt(row)
    delay: forced ? 0 : rich ? 700 : Theme.tooltipDelay
    timeout: 15000
    padding: 0
    topPadding: 0.001
    leftPadding: 0.001
    text: name
    contentItem: Item {
        implicitWidth: tip.rich ? Math.max(200, Math.min(320, col.implicitWidth)) + 20 : plain.implicitWidth
        implicitHeight: tip.rich ? col.implicitHeight + 20 : plain.implicitHeight
        GLabel {
            id: plain
            visible: !tip.rich
            text: tip.name
            font.pixelSize: Theme.small
            wrapMode: Text.Wrap
            elide: Text.ElideNone
            leftPadding: 8; rightPadding: 8; topPadding: 4; bottomPadding: 4
            width: Math.min(implicitWidth, 360)
        }
        Column {
            id: col
            visible: tip.rich
            x: 10; y: 10
            width: parent.width - 20
            spacing: 4
            Item {
                width: parent.width
                height: big.visible ? Math.min(220, big.paintedHeight > 0 ? big.paintedHeight : 200) : icon.height
                Image {
                    id: big
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: parent.width
                    height: 200
                    visible: tip.thumb && status === Image.Ready
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                    sourceSize.width: Math.ceil(256 * Screen.devicePixelRatio)
                    sourceSize.height: Math.ceil(256 * Screen.devicePixelRatio)
                    source: tip.visible && tip.rich && tip.thumb ? "image://thumb/" + encodeURIComponent(tip.path) + "?m=" + tip.mtime : ""
                }
                GIcon {
                    id: icon
                    visible: !big.visible
                    anchors.horizontalCenter: parent.horizontalCenter
                    size: 96
                    name: tip.item ? tip.item.icon : ""
                }
            }
            GLabel { width: parent.width; text: tip.name; font.bold: true; wrapMode: Text.WrapAnywhere; elide: Text.ElideNone; horizontalAlignment: Text.AlignHCenter }
            GLabel {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                font.pixelSize: Theme.small
                color: Theme.fgSecondary
                wrapMode: Text.Wrap
                elide: Text.ElideNone
                text: {
                    const it = tip.item
                    if (!it) return ""
                    const lines = [it.kind]
                    if (!it.isDir) lines.push(it.sizeText)
                    if (it.dims) lines.push(it.dims)
                    if (it.duration) lines.push(it.duration)
                    if (it.mtime > 0) lines.push("Modified " + new Date(it.mtime * 1000).toLocaleString(Qt.locale(), Locale.ShortFormat))
                    if (it.tagText) lines.push("Tags: " + it.tagText)
                    return lines.join("\n")
                }
            }
        }
    }
    background: Rectangle {
        radius: tip.rich ? 10 : 7
        color: Theme.glassMenu
        border.color: Theme.glassEdge
        border.width: 1
    }
    enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 110 } }
    exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: 160 } }
}
