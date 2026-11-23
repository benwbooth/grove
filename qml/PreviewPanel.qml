import QtQuick
import QtQuick.Controls.Basic

// The selected file, big: thumbnail (or icon), name, kind, size, dates,
// tags. Used by the column view's last column and the gallery.
Item {
    id: p
    property var pane
    property var item: null
    property bool compact: false
    property alias viewer: viewer
    property var info: ({})
    // Archives and compressed files: sizes packed and unpacked.
    property var arch: null
    onItemChanged: load()
    Component.onCompleted: load()
    // The item whose details are on screen: the old one's stay until the new
    // one's arrive, then name, kind, dates and sizes change together (clearing
    // first blanked the rows and made the layout jump for a frame).
    property var shown: null
    function settle(it, i, a) { shown = it; info = i; arch = a }
    // Answers can arrive after this panel is gone: they check this first.
    property var life: ({ alive: true })
    Component.onDestruction: life.alive = false
    function load() {
        const life = p.life
        if (!item || !item.path) { settle(null, {}, null); return }
        if (item.path.indexOf("archive://") === 0) {
            // Inside an archive: facts from its local copy (taken out once).
            const ap = item.path
            if (item.isDir) { settle(item, {}, null); return }
            Async.call(pane.grove, k => pane.grove.archiveLocal(k, ap), r => {
                if (!life.alive || !r.path || !p.item || p.item.path !== ap) return
                Async.call(pane.grove, k => pane.grove.info(k, r.path), i => { if (life.alive && p.item && p.item.path === ap && !i.error) p.settle(p.item, Object.assign(i, { path: ap, parent: ap.substring(10, ap.indexOf("#")) }), null) })
            })
            return
        }
        if (item.path.indexOf("/") !== 0) { settle(item, {}, null); return }
        const path = item.path, it = item
        // Facts first; an archive's sizes (slower) join them when ready.
        Async.call(pane.grove, k => pane.grove.info(k, path), r => {
            if (!life.alive || !p.item || p.item.path !== path) return
            p.settle(it, r, null)
            if (it.isArchive)
                Async.call(pane.grove, k => pane.grove.archiveSizes(k, path), a => { if (life.alive && p.item && p.item.path === path && !a.error) p.arch = a })
        })
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
        y: !p.compact ? 12 : Math.max(12, (pflick.height - height) / 2)
        width: pflick.width - 32
        spacing: 10
        // The file itself, readable in full (text, Markdown, tables, every
        // page, a player, the picture); compact panels keep the thumbnail.
        Item {
            width: parent.width
            // One layout for every file (viewer on top, details under it), so
            // nothing jumps between files of different kinds.
            height: !p.compact ? Math.max(p.big, pflick.height - 230) : p.big
            FileViewer {
                id: viewer
                anchors.fill: parent
                visible: !p.compact
                slim: true
                app: p.pane ? p.pane.app : null
                item: p.compact ? null : p.item
                active: !p.compact && p.visible
            }
            GIcon {
                anchors.centerIn: parent
                visible: p.compact && (th.status !== Image.Ready || th.implicitWidth <= 1)
                size: Math.min(p.big, 256)
                name: p.item ? p.item.icon : ""
            }
            Image {
                id: th
                anchors.fill: parent
                visible: p.compact
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                sourceSize.width: Math.ceil(p.big * Screen.devicePixelRatio)
                sourceSize.height: Math.ceil(p.big * Screen.devicePixelRatio)
                source: p.compact && p.item && p.item.thumb ? "image://thumb/" + encodeURIComponent(p.item.path) + "?m=" + p.item.mtime : ""
            }
            PreviewSpinner { visible: p.compact; image: th; size: 18; anchors.horizontalCenter: parent.horizontalCenter; anchors.bottom: parent.bottom; anchors.bottomMargin: 6 }
        }
        GLabel {
            visible: !p.compact
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: p.shown ? p.shown.name : ""
            font.bold: true
            font.pixelSize: Theme.title
            wrapMode: Text.WrapAnywhere
            maximumLineCount: 3
        }
        GLabel {
            visible: !p.compact
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: (p.info.kind || (p.shown ? p.shown.kind : "")) + (p.info.sizeText && !p.info.isDir ? " — " + p.info.sizeText : "")
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
