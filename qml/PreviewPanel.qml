import QtQuick
import QtQuick.Controls.Basic

// The selected file, big: thumbnail (or icon), name, kind, size, dates,
// tags. Used by the column view's last column and the gallery.
Item {
    id: p
    property var pane
    property var item: null
    property bool compact: false
    property var info: ({})
    onItemChanged: load()
    Component.onCompleted: load()
    function load() {
        info = {}
        if (!item || !item.path || item.path.indexOf("/") !== 0) return
        const path = item.path
        Async.call(pane.grove, k => pane.grove.info(k, path), r => { if (p.item && p.item.path === path) p.info = r })
    }
    readonly property int big: Math.max(64, Math.min(width - 40, compact ? height - 20 : height * 0.55, 512))

    Column {
        anchors.centerIn: parent
        width: parent.width - 32
        spacing: 10
        Item {
            width: parent.width
            height: p.big
            GIcon {
                anchors.centerIn: parent
                size: Math.min(p.big, 256)
                name: p.item ? p.item.icon : ""
                visible: th.status !== Image.Ready
            }
            Image {
                id: th
                anchors.fill: parent
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                sourceSize.width: Math.ceil(p.big * Screen.devicePixelRatio)
                sourceSize.height: Math.ceil(p.big * Screen.devicePixelRatio)
                source: p.item && p.item.thumb ? "image://thumb/" + encodeURIComponent(p.item.path) + "?m=" + p.item.mtime : ""
            }
        }
        GLabel {
            visible: !p.compact
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: p.item ? p.item.name : ""
            font.bold: true
            font.pixelSize: Theme.title
            wrapMode: Text.WrapAnywhere
            maximumLineCount: 3
        }
        GLabel {
            visible: !p.compact
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: (p.info.kind || (p.item ? p.item.kind : "")) + (p.info.sizeText && !p.info.isDir ? " — " + p.info.sizeText : "")
            color: Theme.fgSecondary
        }
        Grid {
            visible: !p.compact && p.info.path !== undefined
            anchors.horizontalCenter: parent.horizontalCenter
            columns: 2
            columnSpacing: 10
            rowSpacing: 4
            GLabel { text: "Created"; color: Theme.fgSecondary; horizontalAlignment: Text.AlignRight; width: 90 }
            GLabel { text: p.info.btime ? new Date(p.info.btime * 1000).toLocaleString(Qt.locale(), Locale.ShortFormat) : "--" }
            GLabel { text: "Modified"; color: Theme.fgSecondary; horizontalAlignment: Text.AlignRight; width: 90 }
            GLabel { text: p.info.mtime ? new Date(p.info.mtime * 1000).toLocaleString(Qt.locale(), Locale.ShortFormat) : "--" }
            GLabel { text: "Opens with"; color: Theme.fgSecondary; horizontalAlignment: Text.AlignRight; width: 90; visible: !!p.info.defaultApp }
            GLabel { text: p.info.defaultApp ? p.info.defaultApp.name : ""; visible: !!p.info.defaultApp }
            GLabel { text: "Tags"; color: Theme.fgSecondary; horizontalAlignment: Text.AlignRight; width: 90; visible: p.info.tags && p.info.tags.length > 0 }
            GLabel { text: p.info.tags ? p.info.tags.join(", ") : ""; visible: p.info.tags && p.info.tags.length > 0 }
        }
        PushButton {
            visible: !p.compact
            anchors.horizontalCenter: parent.horizontalCenter
            text: "More Info…"
            tip: "Open the Get Info window (Alt+Return)"
            onClicked: p.pane.win.showInfo([p.item.path])
        }
    }
}
