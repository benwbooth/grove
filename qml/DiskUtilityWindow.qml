import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window
import GlobalMenu

// Grove Disk Utility: GParted's features in grove's look. Disks on the
// left; the chosen disk's partition map drawn to scale (used space shaded,
// logical partitions inside their extended one) above a table of its
// partitions. Every change (new, delete, resize/move, format, label,
// name, flags, new partition table, check, paste) is queued and shown as
// it will be; Undo takes the last step back, Apply carries them all out
// in order (src/disks.rs, through UDisks2). The disks holding the running
// system are locked until unlocked on purpose.
Window {
    id: dk
    property var app
    property var info: ({ filesystems: [], flags: { gpt: [], dos: [] }, caps: {} })
    property var disks: []
    property string diskId: ""
    property bool loading: false
    // Queues per disk: {ops, nextNew}; plans: the disk as it will be.
    property var queues: ({})
    property var plan: null
    property string planError: ""
    property int sel: -1            // index into plan.result.segments
    property var unlocked: ({})     // disk id → true
    property var clip: null         // a copied partition: {part, diskId}
    property bool applying: false
    property var progress: ({})
    readonly property var base: disks.find(d => d.id === diskId) || null
    readonly property var queue: (queues[diskId] || { ops: [], next_new: 0 })
    readonly property var view: plan && plan.result ? plan.result : base
    readonly property var seg: view && sel >= 0 && sel < (view.segments || []).length ? view.segments[sel] : null
    readonly property var part: seg && seg.part >= 0 ? view.parts[seg.part] : null
    readonly property bool locked: !!base && (base.system && !unlocked[base.id] || base.read_only)
    readonly property bool canEdit: !!base && !locked && !applying
    readonly property int pending: queue.ops.length
    readonly property real mib: 1048576

    // Fits the screen it opens on (the window manager doesn't shrink it).
    width: Math.min(Math.round(1060 * Theme.pt), Math.round((Screen.desktopAvailableWidth || 1600) * 0.8))
    height: Math.min(Math.round(680 * Theme.pt), Math.round((Screen.desktopAvailableHeight || 1000) * 0.75))
    minimumWidth: 640
    minimumHeight: 480
    color: Theme.window
    title: "Grove Disk Utility" + (base ? " — " + base.device : "")

    Component.onCompleted: {
        Async.call(app.grove, k => app.grove.diskInfo(k), r => { dk.info = r; dk.reload() })
    }
    onClosing: (close) => {
        if (applying) { close.accepted = false; dk.toast("Wait for the changes to finish: stopping half way could lose data.") }
    }

    function reload(keepQueue) {
        loading = true
        Async.call(app.grove, k => app.grove.diskList(k), r => {
            dk.loading = false
            if (r.error) { dk.toast(r.error); return }
            dk.disks = r.disks || []
            if (!keepQueue) dk.queues = ({})
            if (!dk.disks.find(d => d.id === dk.diskId)) dk.diskId = dk.disks.length ? dk.disks[0].id : ""
            dk.replan()
        })
    }
    function chooseDisk(id) { diskId = id; sel = -1; replan() }
    function replan() {
        planError = ""
        if (!base) { plan = null; return }
        const r = JSON.parse(app.grove.diskPlan(JSON.stringify(base), JSON.stringify(queue)))
        if (r.error) planError = r.error
        plan = r
        if (sel >= (view.segments || []).length) sel = -1
    }
    // Queue a step: it's checked against the disk as the queue leaves it.
    function push(op) {
        if (!canEdit) return false
        const r = JSON.parse(app.grove.diskPush(JSON.stringify(base), JSON.stringify(queue), JSON.stringify(op)))
        if (r.error) { problem.show(r.error); return false }
        const q = Object.assign({}, queues); q[diskId] = r.queue; queues = q
        replan()
        return true
    }
    function setQueue(ops) {
        const q = Object.assign({}, queues); q[diskId] = { ops: ops, next_new: queue.next_new || 0 }; queues = q
        replan()
    }
    function undo() { if (pending && !applying) setQueue(queue.ops.slice(0, -1)) }
    function clearQueue() { if (pending && !applying) setQueue([]) }
    function toast(msg) { toastLabel.text = msg; toastBox.opacity = 1; toastTimer.restart() }
    function size(n) { return app.size(n) }
    function plural(n, w) { return n + " " + w + (n === 1 ? "" : "s") }
    function mb(n) { return Math.round(n / mib) }
    function segIndexOf(id) { return (view.segments || []).findIndex(s => s.part >= 0 && view.parts[s.part].id === id) }

    // GParted's colours, so its users recognise them at a glance.
    function fsColor(fs) {
        switch (fs) {
        case "ext2": return "#9db8d2"
        case "ext3": return "#7590ae"
        case "ext4": return "#4b6983"
        case "btrfs": return "#ff9955"
        case "xfs": return "#eed680"
        case "f2fs": return "#df421e"
        case "ntfs": return "#42e5ac"
        case "vfat": return "#18d918"
        case "exfat": return "#2e8b57"
        case "swap": return "#c1665a"
        case "crypto_LUKS": return "#625b81"
        case "LVM2_member": return "#cc9966"
        case "udf": return "#105210"
        case "": return "#555555"
        default: return "#888888"
        }
    }
    function fsName(fs) { const f = info.filesystems.find(x => x.id === fs); return fs === "" ? "unknown" : f ? f.name : fs }
    function caps(fs) { return (info.caps || {})[fs] || {} }

    // ---- actions (each also in the menus with its shortcut) ----
    function newPartition() { if (canEdit && seg && seg.part < 0) geo.openNew() }
    function deletePartition() { if (canEdit && part) push({ op: "delete", id: part.id }) }
    function resizePartition() { if (canEdit && part) geo.openResize() }
    function formatAs(fs) { if (canEdit && part && part.kind !== "extended") { fmt.fs = fs; fmt.open() } }
    function labelPartition() { if (canEdit && part && part.kind !== "extended") lbl.open() }
    function flagsPartition() { if (canEdit && part) flg.open() }
    function checkPartition() { if (canEdit && part && part.fs) push({ op: "check", id: part.id }) }
    function newTable() { if (canEdit) tbl.open() }
    function copyPartition() {
        if (!part || part.number === 0 || part.kind === "extended") return
        // The partition as it is on the disk now (not as pending steps leave it).
        const now = base.parts.find(x => x.id === part.id)
        if (!now) return
        clip = { part: now, diskId: diskId }
        toast("Copied " + part.device + ". Select free space and choose Paste.")
    }
    function pastePartition() { if (canEdit && clip && seg && seg.part < 0) geo.openPaste() }
    function applyAll() { if (pending && !applying && !planError) confirmApply.open() }
    function unmountSel() {
        if (!part || !part.busy) return
        Async.call(app.grove, k => app.grove.diskUnmount(k, part.id), r => { if (r.error) problem.show(r.error); dk.reload(true) })
    }
    function mountSel() {
        if (!part || part.busy || !part.fs) return
        Async.call(app.grove, k => app.grove.diskMount(k, part.id), r => { if (r.error) problem.show(r.error); dk.reload(true) })
    }
    function startApply() {
        applying = true
        progress = { text: "Starting…", fraction: 0, step: 0, steps: pending }
        const d = JSON.stringify(base), q = JSON.stringify(queue)
        Async.call(app.grove, k => app.grove.diskApply(k, d, q, !!dk.unlocked[dk.diskId]), r => {
            dk.applying = false
            done.ok = !r.error
            done.message = r.error ? r.error : "All " + dk.pending + " operation" + (dk.pending === 1 ? " was" : "s were") + " carried out."
            done.open()
            dk.reload(false)
        })
    }
    Connections {
        target: dk.app.grove
        enabled: dk.applying
        function onDiskProgress(json) { dk.progress = JSON.parse(json) }
    }

    Shortcut { sequence: "Ctrl+N"; onActivated: dk.newPartition() }
    Shortcut { sequences: ["Delete"]; onActivated: dk.deletePartition() }
    Shortcut { sequence: "Ctrl+R"; onActivated: dk.resizePartition() }
    Shortcut { sequence: "Ctrl+C"; onActivated: dk.copyPartition() }
    Shortcut { sequence: "Ctrl+V"; onActivated: dk.pastePartition() }
    Shortcut { sequence: "Ctrl+Z"; onActivated: dk.undo() }
    Shortcut { sequence: "Ctrl+Return"; onActivated: dk.applyAll() }
    Shortcut { sequence: "F5"; onActivated: if (!dk.applying) dk.reload(true) }
    Shortcut { sequence: "F2"; onActivated: dk.labelPartition() }
    Shortcut { sequence: "Ctrl+I"; onActivated: if (dk.part) infoSheet.open() }
    Shortcut { sequence: "Ctrl+W"; onActivated: dk.close() }

    Item {
        anchors.fill: parent
        transform: Translate { x: 0.001; y: 0.001 }

        // ---- disks ----
        Rectangle {
            id: side
            width: Math.round(230 * Theme.pt)
            height: parent.height
            color: Theme.sidebar
            Column {
                anchors.fill: parent
                anchors.topMargin: 14
                spacing: 2
                GLabel { x: 16; text: "Disks"; font.bold: true; color: Theme.fgSecondary; font.pixelSize: Theme.small }
                Repeater {
                    model: dk.disks
                    Item {
                        required property var modelData
                        width: side.width; height: Math.round(44 * Theme.pt)
                        Rectangle {
                            anchors.fill: parent; anchors.leftMargin: 8; anchors.rightMargin: 8
                            radius: Math.round(7 * Theme.pt)
                            color: modelData.id === dk.diskId ? Theme.selectedInactive : dma.containsMouse ? Theme.hover : "transparent"
                        }
                        GIcon {
                            x: 16; anchors.verticalCenter: parent.verticalCenter; size: 24
                            name: modelData.loop_file ? "media-optical|drive-harddisk" : modelData.removable ? "drive-removable-media-usb|drive-removable-media" : "drive-harddisk"
                        }
                        Column {
                            x: 48; width: parent.width - 48 - 30
                            anchors.verticalCenter: parent.verticalCenter
                            GLabel { width: parent.width; text: modelData.model || modelData.device; elide: Text.ElideRight }
                            GLabel { width: parent.width; text: modelData.device + " · " + dk.size(modelData.size) + (dk.queues[modelData.id] && dk.queues[modelData.id].ops.length ? " · " + dk.queues[modelData.id].ops.length + " pending" : ""); color: Theme.fgSecondary; font.pixelSize: Theme.small; elide: Text.ElideRight }
                        }
                        GIcon {
                            visible: modelData.system
                            anchors.right: parent.right; anchors.rightMargin: 16
                            anchors.verticalCenter: parent.verticalCenter
                            size: 14; name: dk.unlocked[modelData.id] ? "object-unlocked|changes-allow" : "object-locked|changes-prevent"
                            tint: Theme.fgSecondary
                        }
                        MouseArea {
                            id: dma; anchors.fill: parent; hoverEnabled: true
                            onClicked: if (!dk.applying) dk.chooseDisk(modelData.id)
                        }
                        GTip { visible: dma.containsMouse; text: (modelData.model || modelData.device) + " (" + modelData.device + ", " + dk.size(modelData.size) + ")" + (modelData.system ? ". Holds the running system: locked." : "") + (modelData.loop_file ? ". A disk image: " + modelData.loop_file : "") }
                    }
                }
                GLabel {
                    visible: dk.disks.length === 0 && !dk.loading
                    x: 16; width: side.width - 32
                    wrapMode: Text.Wrap
                    color: Theme.fgSecondary
                    text: dk.info.sandbox ? "No disk images attached in " + dk.info.sandbox + "." : "No disks found."
                }
            }
        }

        // ---- the chosen disk ----
        Item {
            id: main
            x: side.width
            width: parent.width - side.width
            height: parent.height

            // Toolbar.
            Row {
                id: bar
                x: 12; y: 10
                spacing: 2
                height: Theme.controlHeight
                GButton { iconName: "list-add"; enabled: dk.canEdit && !!dk.seg && dk.seg.part < 0; tip: "New partition in the selected free space (Ctrl+N)"; onClicked: dk.newPartition() }
                GButton { iconName: "list-remove|edit-delete"; enabled: dk.canEdit && !!dk.part; tip: "Delete the selected partition (Delete)"; onClicked: dk.deletePartition() }
                GButton { iconName: "transform-scale|zoom-fit-best"; enabled: dk.canEdit && !!dk.part; tip: "Resize or move the selected partition (Ctrl+R)"; onClicked: dk.resizePartition() }
                Item { width: 8; height: 1 }
                GButton { iconName: "edit-copy"; enabled: !!dk.part && dk.part.number > 0 && dk.part.kind !== "extended"; tip: "Copy the selected partition, to paste it into free space (Ctrl+C)"; onClicked: dk.copyPartition() }
                GButton { iconName: "edit-paste"; enabled: dk.canEdit && !!dk.clip && !!dk.seg && dk.seg.part < 0; tip: dk.clip ? "Paste a copy of " + dk.clip.part.device + " into the selected free space (Ctrl+V)" : "Paste a copied partition (copy one first)"; onClicked: dk.pastePartition() }
                Item { width: 8; height: 1 }
                GButton { iconName: "edit-undo"; enabled: dk.pending > 0 && !dk.applying; tip: "Undo the last pending operation (Ctrl+Z)"; onClicked: dk.undo() }
                GButton { iconName: "dialog-ok-apply|checkmark"; enabled: dk.pending > 0 && !dk.applying && dk.planError === ""; tip: "Apply all pending operations (Ctrl+Return)"; onClicked: dk.applyAll() }
                Item { width: 8; height: 1 }
                GButton { iconName: "view-refresh"; enabled: !dk.applying; tip: "Look at the disks again (F5). Pending operations are kept."; onClicked: dk.reload(true) }
                // Every menu, for desktops without a global menu bar.
                GButton {
                    id: menuButton
                    iconName: "view-more-horizontal-symbolic|application-menu|open-menu-symbolic|overflow-menu"
                    tip: "Menus: Disk Utility, Edit, Partition, Disk, Window, Help (also in the menu bar at the top of the screen)"
                    onClicked: dk.toggleAppMenu(menuButton)
                }
            }

            // Disk header and lock banner.
            Column {
                id: head
                x: 16; y: bar.y + bar.height + 12
                width: main.width - 32
                spacing: 6
                GLabel {
                    text: dk.base ? (dk.base.model || dk.base.device) : (dk.loading ? "Looking for disks…" : "")
                    font.pixelSize: Theme.large; font.bold: true
                }
                GLabel {
                    visible: !!dk.base
                    color: Theme.fgSecondary
                    text: dk.base ? dk.base.device + " · " + dk.size(dk.base.size) + " · " + (dk.view.table === "gpt" ? "GPT partition table" : dk.view.table === "dos" ? "MBR (msdos) partition table" : "no partition table") + (dk.base.connection ? " · " + dk.base.connection.toUpperCase() : "") : ""
                }
                Rectangle {
                    visible: !!dk.base && (dk.base.system || dk.base.read_only)
                    width: parent.width; height: lockRow.height + 16
                    radius: Theme.radius
                    color: dk.locked ? Qt.rgba(1, 0.75, 0.2, Theme.dark ? 0.18 : 0.22) : Qt.rgba(1, 0.27, 0.23, 0.18)
                    Row {
                        id: lockRow
                        x: 10; anchors.verticalCenter: parent.verticalCenter
                        spacing: 10
                        GIcon { size: 16; name: dk.locked ? "object-locked|changes-prevent" : "dialog-warning"; anchors.verticalCenter: parent.verticalCenter }
                        GLabel {
                            width: main.width - 32 - 150
                            wrapMode: Text.Wrap
                            anchors.verticalCenter: parent.verticalCenter
                            text: !dk.base ? "" : dk.base.read_only ? "This disk is read-only: nothing on it can be changed."
                                : dk.locked ? "This disk holds the running system (/, /home or /boot). It's locked so nothing here can break the computer by accident."
                                : "Unlocked: changes to the system's disk can stop the computer from starting. Partitions in use (mounted) still can't be changed."
                        }
                    }
                    PushButton {
                        visible: !!dk.base && dk.base.system && !dk.base.read_only
                        anchors.right: parent.right; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter
                        text: dk.locked ? "Unlock…" : "Lock"
                        tip: dk.locked ? "Allow changes to the system's disk (asks first)" : "Lock the system's disk again"
                        onClicked: {
                            if (dk.locked) unlockSheet.open()
                            else { const u = Object.assign({}, dk.unlocked); delete u[dk.diskId]; dk.unlocked = u; dk.clearQueue() }
                        }
                    }
                }
            }

            // ---- the partition map, to scale ----
            Item {
                id: map
                x: 16; y: head.y + head.height + 12
                width: main.width - 32
                height: Math.round(98 * Theme.pt)
                visible: !!dk.view
                readonly property var segs: dk.view ? (dk.view.segments || []) : []
                readonly property real total: dk.view ? Math.max(1, dk.view.size) : 1
                // Small partitions still get room to click.
                readonly property real minW: 18
                readonly property var layout: {
                    const out = []
                    const top = []
                    for (let i = 0; i < segs.length; i++) if (!segs[i].inExtended) top.push(i)
                    const ws = top.map(i => Math.max(minW, segs[i].size / total * width))
                    const sum = ws.reduce((a, b) => a + b, 0)
                    const k = sum > 0 ? Math.min(1, width / sum) : 1
                    let x = 0
                    for (let t = 0; t < top.length; t++) {
                        const i = top[t]
                        out[i] = { x: x, w: ws[t] * k }
                        x += ws[t] * k
                    }
                    // Logical partitions and free space inside the extended one.
                    for (let i = 0; i < segs.length; i++) {
                        if (!segs[i].inExtended) continue
                        const e = segs.findIndex(s => s.part >= 0 && dk.view.parts[s.part].kind === "extended")
                        if (e < 0 || !out[e]) continue
                        const E = segs[e], L = out[e]
                        out[i] = { x: L.x + 3 + (segs[i].offset - E.offset) / Math.max(1, E.size) * (L.w - 6), w: Math.max(6, segs[i].size / Math.max(1, E.size) * (L.w - 6)) }
                    }
                    return out
                }
                Rectangle { anchors.fill: parent; radius: 6; color: Theme.field }
                Repeater {
                    model: map.segs.length
                    Item {
                        id: cell
                        required property int index
                        readonly property var s: map.segs[index]
                        readonly property var p: s && s.part >= 0 ? dk.view.parts[s.part] : null
                        readonly property bool ext: !!p && p.kind === "extended"
                        readonly property var lay: map.layout[index] || { x: 0, w: 0 }
                        x: Math.round(lay.x); width: Math.round(lay.x + lay.w) - Math.round(lay.x)
                        y: s && s.inExtended ? 5 : 0
                        height: s && s.inExtended ? map.height - 10 : map.height
                        z: s && s.inExtended ? 2 : 1
                        Rectangle {
                            anchors.fill: parent; anchors.margins: 1
                            radius: 5
                            color: !cell.p ? (Theme.dark ? "#3a3a3c" : "#d8d8dc") : cell.ext ? Qt.rgba(0.49, 0.99, 1, 0.12) : Theme.content
                            border.width: cell.ext ? 2 : 0
                            border.color: "#7dfcfe"
                            // The filesystem's colour along the top, as GParted.
                            Rectangle {
                                visible: !!cell.p && !cell.ext
                                width: parent.width; height: 6; radius: 3
                                color: cell.p ? dk.fsColor(cell.p.fs) : "transparent"
                            }
                            // Used space: a thin bar along the bottom in the
                            // filesystem's colour (like the sidebar's capacity bars).
                            Rectangle {
                                visible: !!cell.p && !cell.ext && cell.p.used !== null && cell.p.used !== undefined && cell.p.fs !== "swap"
                                x: 6; y: parent.height - 9
                                width: parent.width - 12; height: 4; radius: 2
                                color: Theme.separator
                                Rectangle {
                                    width: cell.p && cell.p.used ? Math.max(2, parent.width * Math.min(1, cell.p.used / Math.max(1, cell.p.size))) : 0
                                    height: parent.height; radius: 2
                                    color: cell.p ? dk.fsColor(cell.p.fs) : "transparent"
                                }
                            }
                            // Pending changes are hatched with the accent.
                            Rectangle {
                                visible: !!cell.p && cell.p.pending
                                anchors.fill: parent; radius: 5
                                color: "transparent"
                                border.width: 2; border.color: Theme.accent
                            }
                            Column {
                                id: cellText
                                visible: cell.width > 46 && !cell.ext
                                // Whole pixels, so the native text stays crisp.
                                x: Math.round((parent.width - width) / 2)
                                y: Math.round((parent.height - height) / 2)
                                GLabel {
                                    x: Math.round((cellText.width - width) / 2)
                                    width: Math.ceil(Math.min(implicitWidth, cell.width - 8)); elide: Text.ElideRight
                                    text: cell.p ? (cell.p.label || cell.p.title.replace("/dev/", "")) : "unallocated"
                                    font.pixelSize: Theme.small
                                }
                                GLabel {
                                    x: Math.round((cellText.width - width) / 2)
                                    width: Math.ceil(Math.min(implicitWidth, cell.width - 8)); elide: Text.ElideRight
                                    text: cell.s ? dk.size(cell.s.size) : ""
                                    font.pixelSize: Theme.caption; color: Theme.fgSecondary
                                }
                                // How full it is.
                                GLabel {
                                    visible: !!cell.p && cell.p.used !== null && cell.p.used !== undefined && cell.p.fs !== "swap"
                                    x: Math.round((cellText.width - width) / 2)
                                    width: Math.ceil(Math.min(implicitWidth, cell.width - 8)); elide: Text.ElideRight
                                    text: cell.p && cell.p.used !== null && cell.p.used !== undefined ? dk.size(cell.p.used) + " used · " + dk.size(Math.max(0, cell.p.size - cell.p.used)) + " free" : ""
                                    font.pixelSize: Theme.caption; color: Theme.fgSecondary
                                }
                                // btrfs: its subvolumes and snapshots (when mounted).
                                GLabel {
                                    visible: !!cell.p && cell.p.subvols !== null && cell.p.subvols !== undefined
                                    x: Math.round((cellText.width - width) / 2)
                                    width: Math.ceil(Math.min(implicitWidth, cell.width - 8)); elide: Text.ElideRight
                                    text: visible ? dk.plural(cell.p.subvols, "subvolume") + " · " + dk.plural(cell.p.snapshots, "snapshot") : ""
                                    font.pixelSize: Theme.caption; color: Theme.fgSecondary
                                }
                                // Where it's open (every mount point; swap: "swap").
                                GLabel {
                                    visible: !!cell.p && (cell.p.mounts.length > 0 || (cell.p.busy && cell.p.fs === "swap"))
                                    x: Math.round((cellText.width - width) / 2)
                                    width: Math.ceil(Math.min(implicitWidth, cell.width - 8)); elide: Text.ElideMiddle
                                    text: cell.p ? (cell.p.mounts.length ? cell.p.mounts.join(", ") : "swap") : ""
                                    font.pixelSize: Theme.caption; color: Theme.fg
                                }
                            }
                        }
                        // Selection.
                        Rectangle {
                            visible: dk.sel === cell.index
                            anchors.fill: parent
                            radius: 6; color: "transparent"
                            border.width: 2; border.color: Theme.fg
                            Rectangle { anchors.fill: parent; anchors.margins: 2; radius: 4; color: "transparent"; border.width: 1; border.color: Theme.window }
                        }
                        MouseArea {
                            id: cm
                            anchors.fill: parent
                            hoverEnabled: true
                            acceptedButtons: Qt.LeftButton | Qt.RightButton
                            onClicked: (m) => { dk.sel = cell.index; if (m.button === Qt.RightButton) partMenu.popup() }
                            onDoubleClicked: { dk.sel = cell.index; if (cell.p) infoSheet.open(); else dk.newPartition() }
                        }
                        GTip {
                            visible: cm.containsMouse
                            text: cell.p ? cell.p.title + " — " + dk.fsName(cell.p.fs) + (cell.p.label ? " “" + cell.p.label + "”" : "") + ", " + dk.size(cell.p.size) + (cell.p.mounts.length ? ", open at " + cell.p.mounts[0] : "") + ". Double-click for details; right-click for what you can do."
                                : "Free space, " + dk.size(cell.s.size) + (cell.s.inExtended ? " inside the extended partition (logical partitions go here)" : "") + ". Double-click to make a partition here."
                        }
                    }
                }
            }

            // ---- the table ----
            Item {
                id: table
                x: 16; y: map.y + map.height + 14
                width: main.width - 32
                height: queueBox.y - y - 12
                readonly property var cols: [
                    { t: "Partition", w: 0.17 }, { t: "Name", w: 0.10 }, { t: "File System", w: 0.12 }, { t: "Label", w: 0.11 },
                    { t: "Size", w: 0.09 }, { t: "Used", w: 0.09 }, { t: "Unused", w: 0.09 }, { t: "Flags", w: 0.12 }, { t: "Mount Point", w: 0.11 }
                ]
                Row {
                    id: thead
                    height: Theme.rowHeight
                    Repeater {
                        model: table.cols
                        GLabel { required property var modelData; width: table.width * modelData.w; text: modelData.t; color: Theme.fgSecondary; font.pixelSize: Theme.small; elide: Text.ElideRight }
                    }
                }
                Rectangle { y: thead.height; width: parent.width; height: 1; color: Theme.separator }
                ListView {
                    id: rows
                    y: thead.height + 2
                    width: parent.width; height: parent.height - y
                    clip: true
                    model: map.segs.length
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: GScrollBar {}
                    delegate: Rectangle {
                        id: row
                        required property int index
                        readonly property var s: map.segs[index]
                        readonly property var p: s && s.part >= 0 ? dk.view.parts[s.part] : null
                        width: rows.width; height: Theme.rowHeight
                        radius: 5
                        color: dk.sel === index ? Theme.accent : index % 2 ? Theme.rowAlt : "transparent"
                        readonly property color tc: dk.sel === index ? Theme.accentText : Theme.fg
                        readonly property var cells: [
                            (s && s.inExtended ? "    " : "") + (p ? p.title : "unallocated") + (p && p.busy ? "  🔑" : ""),
                            p ? p.name : "",
                            p ? (p.kind === "extended" ? "extended" : dk.fsName(p.fs)) : "unallocated",
                            p ? p.label : "",
                            s ? dk.size(s.size) : "",
                            p && p.used !== null && p.used !== undefined && p.kind !== "extended" ? dk.size(p.used) : p ? "—" : "",
                            p && p.used !== null && p.used !== undefined && p.kind !== "extended" ? dk.size(Math.max(0, p.size - p.used)) : p ? "—" : "",
                            p ? (p.flags || []).join(", ") : "",
                            p ? (p.mounts || []).join(", ") : ""
                        ]
                        Row {
                            anchors.verticalCenter: parent.verticalCenter
                            Repeater {
                                model: table.cols.length
                                Item {
                                    required property int index
                                    width: table.width * table.cols[index].w; height: Theme.rowHeight
                                    Rectangle {
                                        visible: index === 2 && !!row.p && row.p.kind !== "extended"
                                        x: 0; anchors.verticalCenter: parent.verticalCenter
                                        width: 10; height: 10; radius: 2
                                        color: row.p ? dk.fsColor(row.p.fs) : "transparent"
                                    }
                                    GLabel {
                                        x: index === 2 && !!row.p && row.p.kind !== "extended" ? 16 : 4
                                        width: parent.width - x - 4
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: row.cells[index]
                                        color: row.tc
                                        font.bold: index === 0 && !!row.p && row.p.pending
                                        elide: Text.ElideRight
                                    }
                                }
                            }
                        }
                        MouseArea {
                            anchors.fill: parent
                            acceptedButtons: Qt.LeftButton | Qt.RightButton
                            onClicked: (m) => { dk.sel = row.index; if (m.button === Qt.RightButton) partMenu.popup() }
                            onDoubleClicked: { dk.sel = row.index; if (row.p) infoSheet.open(); else dk.newPartition() }
                        }
                    }
                }
                GLabel {
                    visible: !!dk.view && dk.view.table === "" && !!dk.base
                    anchors.centerIn: parent
                    width: parent.width * 0.7
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    color: Theme.fgSecondary
                    text: dk.base && dk.base.whole_fs ? "This disk holds a " + dk.fsName(dk.base.whole_fs) + " filesystem directly, without a partition table. To partition it, make a new partition table (Device › New Partition Table…): that erases it."
                        : "This disk has no partition table. Make one (Device › New Partition Table…) to add partitions."
                }
            }

            // ---- pending operations ----
            Rectangle {
                id: queueBox
                x: 16; width: main.width - 32
                height: Math.round((dk.pending || dk.applying || dk.planError ? 150 : 40) * Theme.pt)
                y: main.height - height - 12
                radius: Theme.radius
                color: Theme.field
                Behavior on height { NumberAnimation { duration: Theme.reduceMotion ? 0 : 160 } }
                GLabel {
                    id: qtitle
                    x: 12; y: 10
                    font.bold: true
                    text: dk.applying ? "Applying " + dk.pending + " operation" + (dk.pending === 1 ? "" : "s") + "…" : dk.pending ? dk.pending + " operation" + (dk.pending === 1 ? "" : "s") + " pending" : "No pending operations"
                }
                GLabel {
                    anchors.left: qtitle.right; anchors.leftMargin: 12; anchors.verticalCenter: qtitle.verticalCenter
                    visible: dk.pending === 0 && !dk.applying
                    text: "Changes you make are listed here and only carried out when you click Apply."
                    color: Theme.fgSecondary; font.pixelSize: Theme.small
                }
                Row {
                    anchors.right: parent.right; anchors.rightMargin: 8; y: 5
                    spacing: 8
                    visible: dk.pending > 0
                    PushButton { text: "Undo"; enabled: !dk.applying; tip: "Take back the last pending operation (Ctrl+Z)"; onClicked: dk.undo() }
                    PushButton { text: "Clear All"; enabled: !dk.applying; tip: "Take back every pending operation"; onClicked: dk.clearQueue() }
                    PushButton { text: dk.applying ? "Stop After This Step" : "Apply"; primary: !dk.applying; enabled: dk.applying || dk.planError === ""; tip: dk.applying ? "Finish the current step, then stop (a step can't stop half way without losing data)" : "Carry out all pending operations, in order (Ctrl+Return)"; onClicked: dk.applying ? dk.app.grove.diskCancel() : dk.applyAll() }
                }
                ListView {
                    x: 12; y: 38
                    width: parent.width - 24; height: parent.height - y - (dk.applying ? 30 : 8)
                    clip: true
                    visible: dk.pending > 0
                    model: dk.plan && dk.plan.steps ? dk.plan.steps : []
                    delegate: Row {
                        required property var modelData
                        required property int index
                        spacing: 8
                        height: Math.round(20 * Theme.pt)
                        GLabel {
                            width: 18; text: dk.applying ? (index < dk.progress.step ? "✓" : index === dk.progress.step ? "›" : "") : (index + 1) + "."
                            color: Theme.fgSecondary
                        }
                        GLabel { text: modelData.text }
                    }
                    ScrollBar.vertical: GScrollBar {}
                }
                GLabel {
                    visible: dk.planError !== ""
                    x: 12; y: parent.height - 26
                    width: parent.width - 24
                    color: Theme.danger
                    text: "Can't be carried out: " + dk.planError
                    elide: Text.ElideRight
                }
                Column {
                    visible: dk.applying
                    x: 12; y: parent.height - 28
                    width: parent.width - 24
                    spacing: 4
                    GLabel { width: parent.width; text: dk.progress.text || ""; color: Theme.fgSecondary; font.pixelSize: Theme.small; elide: Text.ElideRight }
                    Rectangle {
                        width: parent.width; height: 5; radius: 2.5; color: Theme.separator
                        Rectangle { width: parent.width * Math.max(0.02, dk.progress.fraction || 0); height: parent.height; radius: 2.5; color: Theme.accent }
                    }
                }
            }
        }

        // Toast.
        Rectangle {
            id: toastBox
            anchors.horizontalCenter: parent.horizontalCenter
            y: parent.height - height - 180
            width: toastLabel.implicitWidth + 32; height: toastLabel.implicitHeight + 16
            radius: height / 2
            color: Theme.glassMenu
            border.width: 1; border.color: Theme.glassEdge
            opacity: 0
            visible: opacity > 0
            Behavior on opacity { NumberAnimation { duration: 200 } }
            GLabel { id: toastLabel; anchors.centerIn: parent }
            Timer { id: toastTimer; interval: 3500; onTriggered: toastBox.opacity = 0 }
        }
    }

    // Move by dragging the toolbar's empty space; double-click zooms.
    MouseArea {
        parent: dk.contentItem
        x: side.width; width: dk.width - side.width; height: Math.round(48 * Theme.pt)
        z: -1
        onPressed: dk.startSystemMove()
        onDoubleClicked: dk.visibility === Window.Maximized ? dk.showNormal() : dk.showMaximized()
    }
    // Resize from any edge: the window manager draws no borders of its own.
    Repeater {
        model: dk.visibility !== Window.Maximized && dk.visibility !== Window.FullScreen ? [
            { e: Qt.LeftEdge, x: 0, y: 6, w: 5, h: -12, c: Qt.SizeHorCursor }, { e: Qt.RightEdge, x: -5, y: 6, w: 5, h: -12, c: Qt.SizeHorCursor },
            { e: Qt.BottomEdge, x: 6, y: -5, w: -12, h: 5, c: Qt.SizeVerCursor }, { e: Qt.TopEdge, x: 6, y: 0, w: -12, h: 4, c: Qt.SizeVerCursor },
            { e: Qt.BottomEdge | Qt.RightEdge, x: -10, y: -10, w: 10, h: 10, c: Qt.SizeFDiagCursor }, { e: Qt.BottomEdge | Qt.LeftEdge, x: 0, y: -10, w: 10, h: 10, c: Qt.SizeBDiagCursor },
            { e: Qt.TopEdge | Qt.RightEdge, x: -10, y: 0, w: 10, h: 10, c: Qt.SizeBDiagCursor }, { e: Qt.TopEdge | Qt.LeftEdge, x: 0, y: 0, w: 10, h: 10, c: Qt.SizeFDiagCursor } ] : []
        MouseArea {
            required property var modelData
            parent: dk.contentItem
            z: 1000
            x: modelData.x < 0 ? dk.contentItem.width + modelData.x : modelData.x
            y: modelData.y < 0 ? dk.contentItem.height + modelData.y : modelData.y
            width: modelData.w <= 0 ? dk.contentItem.width + modelData.w : modelData.w
            height: modelData.h <= 0 ? dk.contentItem.height + modelData.h : modelData.h
            cursorShape: modelData.c
            onPressed: dk.startSystemResize(modelData.e)
        }
    }

    // ---- menus: one model for the ☰ button and the global menu bar ----
    readonly property var menuModel: {
        const p = part, can = canEdit, free = !!seg && seg.part < 0
        const fsItems = (info.filesystems || []).map(f => ({ id: "format." + f.id, icon: "", text: f.name, enabled: can && !!p && p.kind !== "extended" && !!f.caps && f.caps.format }))
        return [
            { title: "Disk Utility", icon: "drive-harddisk", items: [
                { id: "refresh", icon: "view-refresh", text: "Refresh", shortcut: "F5", enabled: !applying },
                { separator: true },
                { id: "close", icon: "window-close", text: "Close Window", shortcut: "Ctrl+W" }
            ] },
            { title: "Edit", icon: "edit-undo", items: [
                { id: "undo", icon: "edit-undo", text: "Undo Last Operation", shortcut: "Ctrl+Z", enabled: pending > 0 && !applying },
                { id: "clear", icon: "edit-clear-all|edit-clear", text: "Clear All Operations", enabled: pending > 0 && !applying },
                { id: "apply", icon: "dialog-ok-apply", text: "Apply All Operations", shortcut: "Ctrl+Return", enabled: pending > 0 && !applying && planError === "" },
                { separator: true },
                { id: "copy", icon: "edit-copy", text: "Copy Partition", shortcut: "Ctrl+C", enabled: !!p && p.number > 0 && p.kind !== "extended" },
                { id: "paste", icon: "edit-paste", text: "Paste Partition…", shortcut: "Ctrl+V", enabled: can && !!clip && free }
            ] },
            { title: "Partition", icon: "drive-harddisk", items: [
                { id: "new", icon: "list-add", text: "New…", shortcut: "Ctrl+N", enabled: can && free },
                { id: "delete", icon: "list-remove|edit-delete", text: "Delete", shortcut: "Delete", enabled: can && !!p },
                { id: "resize", icon: "transform-scale|zoom-fit-best", text: "Resize/Move…", shortcut: "Ctrl+R", enabled: can && !!p },
                { separator: true },
                { id: "formatmenu", icon: "drive-harddisk", text: "Format to", enabled: can && !!p && p.kind !== "extended", items: fsItems },
                { id: "label", icon: "edit-rename", text: "Label and Name…", shortcut: "F2", enabled: can && !!p && p.kind !== "extended" },
                { id: "flags", icon: "flag", text: "Manage Flags…", enabled: can && !!p },
                { id: "check", icon: "tools-check-spelling|system-run", text: "Check and Repair", enabled: can && !!p && !!p.fs && p.kind !== "extended" },
                { separator: true },
                { id: "unmount", icon: "media-eject", text: p && p.fs === "swap" ? "Turn Swap Off" : "Unmount", enabled: !!p && p.busy && !applying },
                { id: "mount", icon: "drive-harddisk", text: "Mount", enabled: !!p && !p.busy && !!p.fs && p.number > 0 && p.fs !== "swap" && !applying },
                { id: "info", icon: "documentinfo|dialog-information", text: "Information", shortcut: "Ctrl+I", enabled: !!p }
            ] },
            { title: "Disk", icon: "drive-harddisk", items: [
                { id: "newtable", icon: "document-new", text: "New Partition Table…", enabled: can },
                { id: "unlock", icon: locked ? "object-unlocked|changes-allow" : "object-locked|changes-prevent", text: locked ? "Unlock System Disk…" : "Lock System Disk", enabled: !!base && base.system && !base.read_only && !applying }
            ] },
            { title: "Window", icon: "window-duplicate", items: [
                { id: "minimize", icon: "window-minimize", text: "Minimize" },
                { id: "zoom", icon: "window-maximize", text: "Zoom" }
            ] },
            { title: "Help", icon: "help-browser", items: [
                { id: "help", icon: "help-contents", text: "Disk Utility Help and Shortcuts", shortcut: "F1" }
            ] }
        ]
    }
    function menuAction(id) {
        if (id.indexOf("format.") === 0) { formatAs(id.substring(7)); return }
        switch (id) {
        case "refresh": if (!applying) reload(true); break
        case "close": close(); break
        case "undo": undo(); break
        case "clear": clearQueue(); break
        case "apply": applyAll(); break
        case "copy": copyPartition(); break
        case "paste": pastePartition(); break
        case "new": newPartition(); break
        case "delete": deletePartition(); break
        case "resize": resizePartition(); break
        case "label": labelPartition(); break
        case "flags": flagsPartition(); break
        case "check": checkPartition(); break
        case "unmount": unmountSel(); break
        case "mount": mountSel(); break
        case "info": if (part) infoSheet.open(); break
        case "newtable": newTable(); break
        case "unlock":
            if (locked) unlockSheet.open()
            else { const u = Object.assign({}, unlocked); delete u[diskId]; unlocked = u; clearQueue() }
            break
        case "minimize": showMinimized(); break
        case "zoom": visibility === Window.Maximized ? showNormal() : showMaximized(); break
        case "help": showHelp(); break
        }
    }
    // grove's Help window (searched for Disk Utility), from a browser window
    // when there is one, else its own.
    function showHelp() {
        const w = app.activeWindow || app.windows[0]
        if (w) { w.showHelp("Disk Utility"); return }
        helpWin.query = "Disk Utility"; helpWin.show(); helpWin.raise(); helpWin.requestActivate()
    }
    HelpWindow { id: helpWin; app: dk.app }
    Shortcut { sequence: "F1"; onActivated: dk.showHelp() }
    GlobalMenuBar { id: globalMenu; window: dk; model: dk.menuModel; onTriggered: id => dk.menuAction(id) }
    onVisibleChanged: if (visible) Qt.callLater(globalMenu.attach)
    AppMenu {
        id: appMenu
        model: dk.menuModel
        onTriggered: id => dk.menuAction(id)
        accent: Theme.accent
        dark: Theme.dark
        popupType: Popup.Item
        property real closedAt: 0
        onClosed: closedAt = Date.now()
    }
    function toggleAppMenu(button) {
        if (appMenu.visible || Date.now() - appMenu.closedAt < 250) appMenu.close()
        else appMenu.openBelow(button)
    }

    // ---- menus ----
    GMenu {
        id: partMenu
        GMenuItem { text: "New…"; iconName: "list-add"; shortcutText: "Ctrl+N"; enabled: dk.canEdit && !!dk.seg && dk.seg.part < 0; tip: "Make a partition in this free space"; onTriggered: dk.newPartition() }
        GMenuItem { text: "Delete"; iconName: "list-remove|edit-delete"; shortcutText: "Delete"; enabled: dk.canEdit && !!dk.part; tip: "Remove this partition and everything on it"; onTriggered: dk.deletePartition() }
        GMenuItem { text: "Resize/Move…"; iconName: "transform-scale|zoom-fit-best"; shortcutText: "Ctrl+R"; enabled: dk.canEdit && !!dk.part; tip: "Change its size or where it starts"; onTriggered: dk.resizePartition() }
        GMenuSeparator {}
        GMenuItem { text: "Copy"; iconName: "edit-copy"; shortcutText: "Ctrl+C"; enabled: !!dk.part && dk.part.number > 0 && dk.part.kind !== "extended"; tip: "Copy this partition (then paste it into free space)"; onTriggered: dk.copyPartition() }
        GMenuItem { text: "Paste…"; iconName: "edit-paste"; shortcutText: "Ctrl+V"; enabled: dk.canEdit && !!dk.clip && !!dk.seg && dk.seg.part < 0; tip: "Put a copy of the copied partition here"; onTriggered: dk.pastePartition() }
        GMenuSeparator {}
        GMenu {
            id: fmtMenu
            title: "Format to"
            enabled: dk.canEdit && !!dk.part && dk.part.kind !== "extended"
            Instantiator {
                model: dk.info.filesystems
                delegate: GMenuItem {
                    required property var modelData
                    text: modelData.name
                    swatch: dk.fsColor(modelData.id === "empty" ? "" : modelData.id)
                    enabled: !!modelData.caps && modelData.caps.format
                    tip: modelData.help + (modelData.caps && !modelData.caps.format && modelData.caps.missing ? " (needs " + modelData.caps.missing + ")" : "")
                    onTriggered: dk.formatAs(modelData.id)
                }
                onObjectAdded: (i, o) => fmtMenu.insertItem(i, o)
                onObjectRemoved: (i, o) => fmtMenu.removeItem(o)
            }
        }
        GMenuItem { text: "Label and Name…"; iconName: "edit-rename"; shortcutText: "F2"; enabled: dk.canEdit && !!dk.part && dk.part.kind !== "extended"; tip: "The filesystem's label (shown when it's mounted) and the GPT partition name"; onTriggered: dk.labelPartition() }
        GMenuItem { text: "Manage Flags…"; iconName: "flag"; enabled: dk.canEdit && !!dk.part; tip: "Boot, EFI system, hidden, LVM, RAID and other flags"; onTriggered: dk.flagsPartition() }
        GMenuItem { text: "Check and Repair"; iconName: "tools-check-spelling|system-run"; enabled: dk.canEdit && !!dk.part && !!dk.part.fs && dk.part.kind !== "extended"; tip: "Check the filesystem for errors, repair them, then let it fill its partition"; onTriggered: dk.checkPartition() }
        GMenuSeparator {}
        GMenuItem { text: "Unmount"; iconName: "media-eject"; visible: !!dk.part && dk.part.busy; height: visible ? implicitHeight : 0; enabled: !dk.applying; tip: "Close it (or turn the swap off), so it can be changed"; onTriggered: dk.unmountSel() }
        GMenuItem { text: "Mount"; iconName: "drive-harddisk"; visible: !!dk.part && !dk.part.busy && !!dk.part.fs && dk.part.number > 0 && dk.part.fs !== "swap"; height: visible ? implicitHeight : 0; enabled: !dk.applying; tip: "Open it (in /run/media)"; onTriggered: dk.mountSel() }
        GMenuItem { text: "Information"; iconName: "documentinfo|dialog-information"; shortcutText: "Ctrl+I"; enabled: !!dk.part; tip: "Everything known about this partition"; onTriggered: infoSheet.open() }
    }
    GMenu {
        id: devMenu
        GMenuItem { text: "New Partition Table…"; iconName: "document-new"; enabled: dk.canEdit; tip: "Start the disk over with an empty GPT or MBR partition table (erases everything on it)"; onTriggered: dk.newTable() }
        GMenuItem { text: "Refresh"; iconName: "view-refresh"; shortcutText: "F5"; enabled: !dk.applying; tip: "Look at the disks again"; onTriggered: dk.reload(true) }
        GMenuSeparator {}
        GMenuItem { text: "Apply All Operations"; iconName: "dialog-ok-apply"; shortcutText: "Ctrl+Return"; enabled: dk.pending > 0 && !dk.applying && dk.planError === ""; onTriggered: dk.applyAll() }
        GMenuItem { text: "Undo Last Operation"; iconName: "edit-undo"; shortcutText: "Ctrl+Z"; enabled: dk.pending > 0 && !dk.applying; onTriggered: dk.undo() }
        GMenuItem { text: "Clear All Operations"; iconName: "edit-clear-all|edit-clear"; enabled: dk.pending > 0 && !dk.applying; onTriggered: dk.clearQueue() }
    }

    // ---- sheets ----
    component Field: Row {
        property alias label: l.text
        default property alias body: holder.data
        width: parent ? parent.width : 0
        spacing: 8
        GLabel { id: l; width: Math.round(130 * Theme.pt); anchors.verticalCenter: parent.verticalCenter; color: Theme.fgSecondary; horizontalAlignment: Text.AlignRight }
        Item { id: holder; width: parent.width - l.width - 8; height: Theme.controlHeight }
    }

    // New / Resize-Move / Paste: GParted's dialog. A bar to drag (the
    // partition inside the room it may take) and the same in numbers.
    Sheet {
        id: geo
        property string mode: "new"      // new, resize, paste
        property real lo: 0              // room, in bytes
        property real hi: 0
        property real start: 0
        property real len: 0
        property real minLen: 1048576
        property real maxLen: 0
        property bool canMove: true
        property bool canResize: true
        property bool inExt: false
        property string kind: "primary"
        property string err: ""
        width: Math.min(parent ? parent.width - 40 : 560, Math.round(560 * Theme.pt))
        title: mode === "new" ? "New Partition" : mode === "paste" ? "Paste " + (dk.clip ? dk.clip.part.device : "") : "Resize/Move " + (dk.part ? dk.part.title : "")
        message: mode === "resize" && dk.part ? (canResize ? "Drag the ends to resize, the middle to move. " : "This filesystem can't be resized here, only moved. ") + (dk.part.used !== null && dk.part.used !== undefined ? "Files on it use " + dk.size(dk.part.used) + "." : "How full it is isn't known, so it can't shrink.") + (canMove ? " Moving copies all its data: it can take a while." : "")
            : mode === "paste" ? "The copy is the same size as the original or bigger (its filesystem then grows to fill it)."
            : "Partitions start and end on whole MiB."
        function fill(s) { lo = s.offset; hi = s.offset + s.size; inExt = s.inExtended }
        function openNew() {
            mode = "new"; fill(dk.seg); start = lo; len = hi - lo; canMove = true; canResize = true
            kind = inExt ? "logical" : "primary"
            minLen = 1048576; maxLen = hi - lo
            fsPick.currentIndex = Math.max(0, dk.info.filesystems.findIndex(f => f.id === "ext4"))
            newLabel.text = ""; newName.text = ""; err = ""
            open()
        }
        function openPaste() {
            mode = "paste"; fill(dk.seg); start = lo; len = Math.min(hi - lo, Math.ceil(dk.clip.part.size / dk.mib) * dk.mib); canMove = true; canResize = true
            kind = inExt ? "logical" : "primary"
            minLen = Math.ceil(dk.clip.part.size / dk.mib) * dk.mib; maxLen = hi - lo; err = ""
            if (minLen > maxLen) { problem.show("The copied partition (" + dk.size(dk.clip.part.size) + ") doesn't fit in this free space (" + dk.size(hi - lo) + ")."); return }
            open()
        }
        function openResize() {
            const p = dk.part
            mode = "resize"; lo = p.roomStart; hi = p.roomEnd; start = p.offset; len = p.size; err = ""
            const c = dk.caps(p.fs)
            const fresh = p.number === 0
            canMove = !(p.number > 0 && (p.kind === "logical" || p.kind === "extended"))
            canResize = fresh || p.kind === "extended" || (!!p.fs && (c.grow || c.shrink))
            minLen = fresh ? 1048576 : p.kind === "extended" ? p.size : (c.shrink ? p.minSize : p.size)
            maxLen = fresh || p.kind === "extended" || c.grow ? hi - lo : p.size
            if (!canResize) { minLen = p.size; maxLen = p.size }
            open()
            // How full it is, when that wasn't known (unmounted).
            if (!fresh && (p.used === null || p.used === undefined) && p.fs && p.kind !== "extended")
                Async.call(dk.app.grove, k => dk.app.grove.diskUsage(k, p.id, p.fs), r => {
                    if (r.used === null || r.used === undefined || !dk.part || dk.part.id !== p.id) return
                    const m = Math.ceil((r.used + Math.max(r.used / 20, 16 * dk.mib)) / dk.mib) * dk.mib
                    geo.minLen = c.shrink ? Math.min(p.size, Math.max(m, 1048576)) : p.size
                    const b = dk.disks.find(d => d.id === dk.diskId)
                    const bp = b ? b.parts.find(x => x.id === p.id) : null
                    if (bp) { bp.used = r.used; dk.disks = dk.disks.slice(); dk.replan() }
                })
        }
        function clampAll() {
            len = Math.max(minLen, Math.min(maxLen, len))
            if (!canMove && mode === "resize") start = dk.part.offset
            start = Math.max(lo, Math.min(hi - len, start))
            start = Math.round(start / dk.mib) * dk.mib
            len = Math.floor(len / dk.mib) * dk.mib
        }
        function submit() {
            clampAll()
            let op
            if (mode === "new") {
                const f = dk.info.filesystems[fsPick.currentIndex]
                op = { op: "create", newId: "", offset: start, size: len, kind: kind, fs: kind === "extended" ? "" : f.id, label: newLabel.text, name: newName.text }
            } else if (mode === "paste") {
                op = { op: "paste", newId: "", offset: start, size: len, kind: kind, source: dk.clip.part, sourceDisk: dk.clip.diskId }
            } else op = { op: "resize", id: dk.part.id, offset: start, size: len }
            if (dk.push(op)) { close(); if (mode !== "resize") dk.sel = -1 }
        }
        // The bar.
        Item {
            width: parent.width; height: 44
            Rectangle { anchors.fill: parent; radius: 6; color: Theme.field; border.width: 1; border.color: Theme.separator }
            Rectangle {
                id: block
                readonly property real scale: (parent.width - 4) / Math.max(1, geo.hi - geo.lo)
                x: 2 + (geo.start - geo.lo) * scale
                width: Math.max(10, geo.len * scale)
                y: 2; height: parent.height - 4
                radius: 5
                color: Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.30)
                border.width: 2; border.color: Theme.accent
                Rectangle {
                    visible: geo.mode === "resize" && !!dk.part && dk.part.used !== null && dk.part.used !== undefined
                    x: 3; y: 3; height: parent.height - 6; radius: 3
                    width: dk.part && dk.part.used ? Math.min(parent.width - 6, dk.part.used * block.scale) : 0
                    color: Qt.rgba(0.95, 0.80, 0.35, 0.6)
                }
                MouseArea {
                    anchors.fill: parent; anchors.leftMargin: 8; anchors.rightMargin: 8
                    enabled: geo.canMove
                    cursorShape: enabled ? Qt.SizeAllCursor : Qt.ArrowCursor
                    property real px: 0; property real s0: 0
                    onPressed: (m) => { px = mapToItem(geo.contentItem, m.x, 0).x; s0 = geo.start }
                    onPositionChanged: (m) => { if (pressed) { geo.start = s0 + (mapToItem(geo.contentItem, m.x, 0).x - px) / block.scale; geo.start = Math.max(geo.lo, Math.min(geo.hi - geo.len, geo.start)) } }
                    onReleased: geo.clampAll()
                }
                Repeater {
                    model: 2
                    Rectangle {
                        required property int index
                        visible: geo.canResize
                        x: index === 0 ? -3 : parent.width - 5
                        y: 4; width: 8; height: parent.height - 8; radius: 3
                        color: Theme.accent
                        MouseArea {
                            anchors.fill: parent; anchors.margins: -4
                            cursorShape: Qt.SizeHorCursor
                            property real px: 0; property real s0: 0; property real l0: 0
                            onPressed: (m) => { px = mapToItem(geo.contentItem, m.x, 0).x; s0 = geo.start; l0 = geo.len }
                            onPositionChanged: (m) => {
                                if (!pressed) return
                                const d = (mapToItem(geo.contentItem, m.x, 0).x - px) / block.scale
                                if (index === 1) geo.len = Math.max(geo.minLen, Math.min(Math.min(geo.maxLen, geo.hi - geo.start), l0 + d))
                                else if (geo.canMove) {
                                    const e = s0 + l0
                                    const s = Math.max(geo.lo, Math.min(e - geo.minLen, s0 + d))
                                    geo.start = s; geo.len = Math.min(geo.maxLen, e - s)
                                }
                            }
                            onReleased: geo.clampAll()
                        }
                    }
                }
            }
        }
        Field {
            label: "Free before (MiB):"
            GField {
                width: parent.width; enabled: geo.canMove
                text: Math.round((geo.start - geo.lo) / dk.mib)
                validator: IntValidator { bottom: 0 }
                tip: "Space left empty before the partition"
                onEditingFinished: { geo.start = geo.lo + Number(text) * dk.mib; geo.clampAll() }
            }
        }
        Field {
            label: "Size (MiB):"
            GField {
                width: parent.width; enabled: geo.canResize
                text: Math.round(geo.len / dk.mib)
                validator: IntValidator { bottom: 1 }
                tip: "The partition's size: from " + Math.ceil(geo.minLen / dk.mib) + " to " + Math.floor(geo.maxLen / dk.mib) + " MiB"
                onEditingFinished: { geo.len = Number(text) * dk.mib; geo.clampAll() }
            }
        }
        Field {
            label: "Free after (MiB):"
            GField {
                width: parent.width; enabled: geo.canMove || geo.canResize
                text: Math.round((geo.hi - geo.start - geo.len) / dk.mib)
                validator: IntValidator { bottom: 0 }
                tip: "Space left empty after the partition"
                onEditingFinished: { const after = Number(text) * dk.mib; if (geo.canResize) geo.len = geo.hi - geo.start - after; else geo.start = geo.hi - geo.len - after; geo.clampAll() }
            }
        }
        Field {
            visible: geo.mode !== "resize" && dk.view && dk.view.table === "dos"
            label: "Create as:"
            GCombo {
                width: parent.width
                model: geo.inExt ? ["Logical partition"] : ["Primary partition", "Extended partition"]
                currentIndex: geo.kind === "extended" ? 1 : 0
                tip: "MBR disks hold four primary partitions; an extended one holds any number of logical partitions"
                onActivated: geo.kind = geo.inExt ? "logical" : currentIndex === 1 ? "extended" : "primary"
            }
        }
        Field {
            visible: geo.mode === "new" && geo.kind !== "extended"
            label: "File system:"
            GCombo {
                id: fsPick
                width: parent.width
                model: dk.info.filesystems.map(f => f.name + (f.caps && f.caps.format ? "" : " (not available)"))
                tip: dk.info.filesystems[currentIndex] ? dk.info.filesystems[currentIndex].help : "The filesystem to make"
            }
        }
        Field {
            visible: geo.mode === "new" && geo.kind !== "extended"
            label: "Label:"
            GField { id: newLabel; width: parent.width; placeholderText: "Optional"; tip: "The filesystem's name, shown when it's mounted" }
        }
        Field {
            visible: geo.mode === "new" && geo.kind !== "extended" && dk.view && dk.view.table === "gpt"
            label: "Partition name:"
            GField { id: newName; width: parent.width; placeholderText: "Optional"; maximumLength: 36; tip: "A GPT partition's own name (separate from the filesystem label)" }
        }
        buttons: [
            PushButton { text: "Cancel"; tip: "Leave it as it is"; onClicked: geo.close() },
            PushButton { text: geo.mode === "new" ? "Add" : geo.mode === "paste" ? "Paste" : "Resize/Move"; primary: true; tip: "Add this to the pending operations (nothing changes until you Apply)"; onClicked: geo.submit() }
        ]
    }

    Sheet {
        id: fmt
        property string fs: "ext4"
        title: "Format " + (dk.part ? dk.part.title : "") + " as " + dk.fsName(fs)
        message: "Everything on it will be erased when the operations are applied."
        onOpened: fmtLabel.text = dk.part ? dk.part.label : ""
        Field {
            label: "Label:"
            visible: fmt.fs !== "empty"
            GField { id: fmtLabel; width: parent.width; placeholderText: "Optional"; tip: "The new filesystem's name" }
        }
        buttons: [
            PushButton { text: "Cancel"; onClicked: fmt.close() },
            PushButton { text: "Format"; primary: true; tip: "Add this to the pending operations"; onClicked: if (dk.push({ op: "format", id: dk.part.id, fs: fmt.fs, label: fmtLabel.text })) fmt.close() }
        ]
    }

    Sheet {
        id: lbl
        title: "Label and Name"
        message: dk.part ? dk.part.title + " (" + dk.fsName(dk.part.fs) + ")" : ""
        onOpened: { lblField.text = dk.part ? dk.part.label : ""; nameField.text = dk.part ? dk.part.name : "" }
        Field {
            label: "Filesystem label:"
            visible: !!dk.part && !!dk.part.fs
            GField { id: lblField; width: parent.width; tip: "The name shown when the filesystem is mounted" }
        }
        Field {
            label: "Partition name:"
            visible: !!dk.view && dk.view.table === "gpt"
            GField { id: nameField; width: parent.width; maximumLength: 36; tip: "A GPT partition's own name" }
        }
        buttons: [
            PushButton { text: "Cancel"; onClicked: lbl.close() },
            PushButton {
                text: "OK"; primary: true; tip: "Add the changes to the pending operations"
                onClicked: {
                    let ok = true
                    if (dk.part.fs && lblField.text !== dk.part.label) ok = dk.push({ op: "label", id: dk.part.id, label: lblField.text })
                    if (ok && dk.view.table === "gpt" && nameField.text !== dk.part.name) ok = dk.push({ op: "name", id: dk.part.id, name: nameField.text })
                    if (ok) lbl.close()
                }
            }
        ]
    }

    Sheet {
        id: flg
        property var chosen: []
        title: "Flags on " + (dk.part ? dk.part.title : "")
        message: "Some flags set the partition's type, so setting one of those clears the others."
        onOpened: chosen = dk.part ? (dk.part.flags || []).slice() : []
        Grid {
            columns: 2
            columnSpacing: 16
            rowSpacing: 2
            Repeater {
                model: dk.view ? (dk.view.table === "gpt" ? dk.info.flags.gpt : dk.info.flags.dos) : []
                GCheck {
                    required property var modelData
                    text: modelData.id
                    checked: flg.chosen.indexOf(modelData.id) >= 0
                    tip: modelData.help
                    onToggled: flg.chosen = checked ? flg.chosen.concat([modelData.id]) : flg.chosen.filter(f => f !== modelData.id)
                }
            }
        }
        buttons: [
            PushButton { text: "Cancel"; onClicked: flg.close() },
            PushButton { text: "OK"; primary: true; tip: "Add the change to the pending operations"; onClicked: if (dk.push({ op: "flags", id: dk.part.id, flags: flg.chosen })) flg.close() }
        ]
    }

    Sheet {
        id: tbl
        title: "New Partition Table on " + (dk.base ? dk.base.device : "")
        message: "Every partition on " + (dk.base ? dk.base.device + " (" + dk.size(dk.base.size) + ")" : "the disk") + " and everything on them will be erased when the operations are applied."
        Field {
            label: "Type:"
            GCombo {
                id: tblPick
                width: parent.width
                model: ["GPT (current computers, disks of any size)", "MBR / msdos (old computers, up to 2 TB)"]
                tip: "GPT for nearly everything; MBR for very old computers and some devices"
            }
        }
        buttons: [
            PushButton { text: "Cancel"; onClicked: tbl.close() },
            PushButton { text: "Create"; destructive: true; tip: "Add this to the pending operations"; onClicked: if (dk.push({ op: "newTable", kind: tblPick.currentIndex === 0 ? "gpt" : "dos" })) { tbl.close(); dk.sel = 0 } }
        ]
    }

    Sheet {
        id: confirmApply
        title: "Apply " + dk.pending + " Operation" + (dk.pending === 1 ? "" : "s") + "?"
        message: "Changing partitions can lose data if something goes wrong (a power cut, a disk error). Make sure anything important on " + (dk.base ? dk.base.device : "the disk") + " is backed up. Don't unplug the disk until it's done." + (dk.base && dk.base.system ? "\n\nThis is the disk the running system is on." : "")
        buttons: [
            PushButton { text: "Cancel"; onClicked: confirmApply.close() },
            PushButton { text: "Apply"; primary: true; tip: "Carry out the pending operations now"; onClicked: { confirmApply.close(); dk.startApply() } }
        ]
    }

    Sheet {
        id: unlockSheet
        title: "Unlock the System's Disk?"
        message: (dk.base ? dk.base.device : "This disk") + " holds the running system. A mistake here can stop the computer from starting or lose your files. Partitions in use (mounted) still can't be changed. Only unlock it if you know exactly what you're changing."
        buttons: [
            PushButton { text: "Keep Locked"; primary: true; onClicked: unlockSheet.close() },
            PushButton { text: "Unlock"; destructive: true; tip: "Allow changes to this disk until the window is closed"; onClicked: { const u = Object.assign({}, dk.unlocked); u[dk.diskId] = true; dk.unlocked = u; unlockSheet.close() } }
        ]
    }

    Sheet {
        id: done
        property bool ok: true
        title: ok ? "Done" : "Not All Done"
        buttons: [ PushButton { text: "OK"; primary: true; onClicked: done.close() } ]
    }

    Sheet {
        id: problem
        title: "Can't Do That"
        function show(m) { message = m; open() }
        buttons: [ PushButton { text: "OK"; primary: true; onClicked: problem.close() } ]
    }

    Sheet {
        id: infoSheet
        title: dk.part ? dk.part.title : ""
        readonly property var p: dk.part
        Grid {
            columns: 2
            columnSpacing: 12; rowSpacing: 4
            Repeater {
                model: !infoSheet.p ? [] : [
                    ["File system", infoSheet.p.kind === "extended" ? "extended" : dk.fsName(infoSheet.p.fs)],
                    ["Label", infoSheet.p.label || "—"],
                    ["Name", infoSheet.p.name || "—"],
                    ["UUID", infoSheet.p.uuid || "—"],
                    ["Size", dk.size(infoSheet.p.size) + " (" + infoSheet.p.size + " bytes)"],
                    ["Used", infoSheet.p.used !== null && infoSheet.p.used !== undefined ? dk.size(infoSheet.p.used) : "not known (unmounted)"],
                    ["Starts at", "sector " + Math.round(infoSheet.p.offset / dk.base.sector) + " (" + dk.mb(infoSheet.p.offset) + " MiB)"],
                    ["Ends at", "sector " + (Math.round((infoSheet.p.offset + infoSheet.p.size) / dk.base.sector) - 1)],
                    ["Type", infoSheet.p.ptype || "—"],
                    ["Flags", (infoSheet.p.flags || []).join(", ") || "—"],
                    ["Subvolumes", infoSheet.p.subvols !== null && infoSheet.p.subvols !== undefined ? dk.plural(infoSheet.p.subvols, "subvolume") + ", " + dk.plural(infoSheet.p.snapshots, "snapshot") : infoSheet.p.fs === "btrfs" ? "mount it to see" : "—"],
                    ["Mounted at", (infoSheet.p.mounts || []).join(", ") || "not mounted"],
                    ["Status", infoSheet.p.pending ? "changed by pending operations" : infoSheet.p.busy ? "in use" : "not in use"]
                ].reduce((a, r) => a.concat(r), [])
                GLabel {
                    required property var modelData
                    required property int index
                    text: modelData
                    color: index % 2 ? Theme.fg : Theme.fgSecondary
                    horizontalAlignment: index % 2 ? Text.AlignLeft : Text.AlignRight
                    width: index % 2 ? Math.round(280 * Theme.pt) : Math.round(90 * Theme.pt)
                    wrapMode: Text.WrapAnywhere
                }
            }
        }
        buttons: [ PushButton { text: "Close"; primary: true; onClicked: infoSheet.close() } ]
    }

    // ---- test steps (BrowserWindow's "dk=..." forwards here) ----
    function testStep(s) {
        const a = s.split(":")
        const cmd = a[0]
        const M = n => Number(n) * mib
        if (cmd === "disk") { const d = disks.find(x => x.device === a[1] || x.loop_file.endsWith(a[1])); if (d) chooseDisk(d.id); else console.warn("grove test dk: no disk", a[1]) }
        else if (cmd === "sel") sel = Number(a[1])
        else if (cmd === "selpart") sel = (view.segments || []).findIndex(x => x.part >= 0 && view.parts[x.part].number === Number(a[1]))
        else if (cmd === "selfree") { const f = (view.segments || []).map((x, i) => [x, i]).filter(x => x[0].part < 0); if (f.length) sel = f[Number(a[1] || 0)][1] }
        else if (cmd === "table") push({ op: "newTable", kind: a[1] })
        else if (cmd === "new") push({ op: "create", newId: "", offset: a[3] ? M(a[3]) : seg.offset, size: M(a[2]), kind: a[4] || (seg && seg.inExtended ? "logical" : "primary"), fs: a[1], label: a[5] || "", name: "" })
        else if (cmd === "del") deletePartition()
        else if (cmd === "resize") push({ op: "resize", id: part.id, offset: M(a[1]), size: M(a[2]) })
        else if (cmd === "format") push({ op: "format", id: part.id, fs: a[1], label: a[2] || "" })
        else if (cmd === "label") push({ op: "label", id: part.id, label: a[1] })
        else if (cmd === "name") push({ op: "name", id: part.id, name: a[1] })
        else if (cmd === "flags") push({ op: "flags", id: part.id, flags: a[1] ? a[1].split(",") : [] })
        else if (cmd === "check") checkPartition()
        else if (cmd === "copy") copyPartition()
        else if (cmd === "paste") push({ op: "paste", newId: "", offset: seg.offset, size: M(a[1]) || Math.ceil(clip.part.size / mib) * mib, kind: seg.inExtended ? "logical" : "primary", source: clip.part, sourceDisk: clip.diskId })
        else if (cmd === "undo") undo()
        else if (cmd === "apply") startApply()
        else if (cmd === "reload") reload(true)
        else if (cmd === "unlock") { const u = Object.assign({}, unlocked); u[diskId] = true; unlocked = u }
        else if (cmd === "sheet") {
            if (a[1] === "new") newPartition(); else if (a[1] === "resize") resizePartition(); else if (a[1] === "flags") flagsPartition()
            else if (a[1] === "table") newTable(); else if (a[1] === "format") formatAs(a[2] || "exfat"); else if (a[1] === "label") labelPartition()
            else if (a[1] === "info") infoSheet.open(); else if (a[1] === "apply") applyAll(); else if (a[1] === "unlock") unlockSheet.open()
        }
        else if (cmd === "menu") partMenu.popup(map, 40, map.height)
        else if (cmd === "appmenu") appMenu.openBelow(menuButton)
        else if (cmd === "dump") console.warn("grove test dk", JSON.stringify({ disks: disks.map(d => d.device + (d.system ? "(system)" : "")), disk: base ? base.device : "", table: view ? view.table : "", locked: locked, pending: pending, planError: planError, error: problem.opened ? problem.message : "", done: done.opened ? done.message : "",
            parts: view ? view.parts.map(p => ({ n: p.number, kind: p.kind, fs: p.fs, label: p.label, name: p.name, off: mb(p.offset), size: mb(p.size), flags: p.flags, used: p.used, busy: p.busy })) : [] }))
    }
}
