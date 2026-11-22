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
    // Archives and compressed files: sizes packed and unpacked.
    property var arch: null
    onItemChanged: load()
    Component.onCompleted: load()
    function load() {
        info = {}
        arch = null
        if (!item || !item.path || item.path.indexOf("/") !== 0) return
        const path = item.path
        Async.call(pane.grove, k => pane.grove.info(k, path), r => { if (p.item && p.item.path === path) p.info = r })
        if (item.isArchive)
            Async.call(pane.grove, k => pane.grove.archiveSizes(k, path), r => { if (p.item && p.item.path === path && !r.error) p.arch = r })
    }
    readonly property int big: Math.max(64, Math.min(width - 40, compact ? height - 20 : height * 0.55, 512))

    // Scrolls (with momentum) when the details don't fit; centred when they do.
    KineticWheel { flickable: pflick }
    Flickable {
    id: pflick
    anchors.fill: parent
    clip: true
    contentWidth: width
    contentHeight: Math.max(height, pcol.height + 24)
    Column {
        id: pcol
        x: 16
        y: Math.max(12, (pflick.height - height) / 2)
        width: pflick.width - 32
        spacing: 10
        Item {
            width: parent.width
            height: p.big
            GIcon {
                anchors.centerIn: parent
                size: Math.min(p.big, 256)
                name: p.item ? p.item.icon : ""
                visible: th.status !== Image.Ready || th.implicitWidth <= 1
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
            PreviewSpinner { image: th; size: 18; anchors.horizontalCenter: parent.horizontalCenter; anchors.bottom: parent.bottom; anchors.bottomMargin: 6 }
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
            GLabel { text: "Compressed"; color: Theme.fgSecondary; horizontalAlignment: Text.AlignRight; width: 90; visible: !!p.arch }
            GLabel { text: p.arch ? p.arch.compressedText : ""; visible: !!p.arch }
            GLabel { text: "Uncompressed"; color: Theme.fgSecondary; horizontalAlignment: Text.AlignRight; width: 90; visible: !!p.arch }
            GLabel { visible: !!p.arch
                text: !p.arch ? "" : p.arch.uncompressedText + (p.arch.ratio && p.arch.ratio < 1 ? " (packed to " + Math.round(p.arch.ratio * 100) + "%)" : "") + (p.arch.files > 1 ? ", " + p.arch.files + " files" : "") }
            GLabel { text: "Created"; color: Theme.fgSecondary; horizontalAlignment: Text.AlignRight; width: 90 }
            GLabel { text: p.info.btime ? new Date(p.info.btime * 1000).toLocaleString(Qt.locale(), Locale.ShortFormat) : "--" }
            GLabel { text: "Modified"; color: Theme.fgSecondary; horizontalAlignment: Text.AlignRight; width: 90 }
            GLabel { text: p.info.mtime ? new Date(p.info.mtime * 1000).toLocaleString(Qt.locale(), Locale.ShortFormat) : "--" }
            GLabel { text: "Opens with"; color: Theme.fgSecondary; horizontalAlignment: Text.AlignRight; width: 90; visible: !!p.info.defaultApp }
            GLabel { text: p.info.defaultApp ? p.info.defaultApp.name : ""; visible: !!p.info.defaultApp }
            GLabel { text: "Tags"; color: Theme.fgSecondary; horizontalAlignment: Text.AlignRight; width: 90; visible: !!(p.info.tags && p.info.tags.length > 0) }
            GLabel { text: p.info.tags ? p.info.tags.join(", ") : ""; visible: !!(p.info.tags && p.info.tags.length > 0) }
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
}
