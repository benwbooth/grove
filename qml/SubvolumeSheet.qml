import QtQuick
import QtQuick.Controls.Basic

// btrfs subvolumes: Take Snapshot (name, where, read-only), Restore (a
// subvolume put back as it was in one of its snapshots) and Delete
// Subvolume (a clear confirm, saying when the administrator password is
// needed and why). Snapper's own snapshots are shown but not deleted here.
Sheet {
    id: ss
    property var app
    property var win
    property string mode: "snapshot"   // "snapshot" | "restore" | "delete"
    property string path: ""
    property var facts: ({})
    property bool busy: false
    readonly property string name: path.substring(path.lastIndexOf("/") + 1)
    readonly property string kindText: (facts.kind || "Subvolume").toLowerCase()
    readonly property string sourceName: facts.source ? facts.source.substring(facts.source.lastIndexOf("/") + 1) : ""

    title: mode === "snapshot" ? "Take a Snapshot of “" + name + "”"
         : mode === "restore" ? (sourceName ? "Restore “" + sourceName + "” from this snapshot?" : "Restore from “" + name + "”")
         : facts.snapper ? "“" + name + "” is snapper's snapshot"
         : "Delete the " + kindText + " “" + name + "” permanently?"
    message: mode === "snapshot"
        ? "A snapshot is an instant copy of the subvolume as it is now. It takes no extra room at first: only later changes use space."
        : mode === "restore"
            ? (busy ? "" : sourceName
                ? "“" + sourceName + "” will be put back exactly as it was in “" + name + "”. Nothing is lost: what's in it now is kept beside it as “" + (facts.asideName || "") + "”; delete that once you're sure. Programs with files open in it keep using the set-aside copy until they reopen them."
                : "Grove can't find the subvolume this snapshot was taken of (it looks beside the snapshot and in the folder holding it). To get files back, open the snapshot and copy them out.")
        : facts.snapper
            ? "Snapper made this snapshot and removes it on its own schedule. Grove leaves snapper's snapshots alone; use snapper to delete it."
            : "Everything inside it will be deleted at once. It won't go to the Trash, and this can't be undone."
              + (facts.info && !facts.info.snapshot ? " Snapshots taken of it are kept." : "")

    function snapshot(p) { begin("snapshot", p) }
    function remove(p) { begin("delete", p) }
    function restore(p) { begin("restore", p) }
    function begin(m, p) {
        mode = m
        path = p
        facts = ({})
        busy = true
        Async.call(app.grove, k => app.grove.subvolInfo(k, p), r => {
            ss.busy = false
            if (!r.subvol) { ss.close(); win.toast("error", "“" + ss.name + "” isn't a btrfs subvolume."); return }
            ss.facts = r
            nameField.text = r.snapshotName || ""
            placeField.text = r.place || ""
        })
        open()
    }

    Row {
        visible: ss.mode === "snapshot"
        width: parent.width
        spacing: 8
        GLabel { text: "Name:"; width: 80; anchors.verticalCenter: parent.verticalCenter; color: Theme.fgSecondary }
        GField { id: nameField; width: parent.width - 88; placeholderText: "Snapshot name"; tip: "The new snapshot's folder name (the date and time by default)" }
    }
    Row {
        visible: ss.mode === "snapshot"
        width: parent.width
        spacing: 8
        GLabel { text: "Put it in:"; width: 80; anchors.verticalCenter: parent.verticalCenter; color: Theme.fgSecondary }
        GField { id: placeField; width: parent.width - 88; tip: "The folder the snapshot goes in. It has to be on the same btrfs disk (beside the subvolume by default, or inside it when the folder above can't be written to)" }
    }
    GCheck {
        id: roBox
        visible: ss.mode === "snapshot"
        text: "Read-only (recommended)"
        checked: true
        tip: "A read-only snapshot can't be changed by accident, so it stays exactly as the subvolume is now. Untick it for a copy you can work in"
    }
    GLabel {
        // Why the password is asked for, in plain words.
        visible: ss.mode === "delete" && !ss.facts.snapper && !!ss.facts.needsRoot
        width: parent.width
        wrapMode: Text.Wrap
        elide: Text.ElideNone
        color: Theme.fgSecondary
        text: "Your password will be asked for. This disk only lets an administrator delete subvolumes that still hold files (it isn't mounted with the user_subvol_rm_allowed option), so Grove asks the system for permission and deletes just this one."
    }
    buttons: [
        PushButton { text: (ss.facts.snapper && ss.mode === "delete") || (ss.mode === "restore" && !ss.busy && !ss.facts.source) ? "OK" : "Cancel"; tip: "Close without changing anything"; onClicked: ss.close() },
        PushButton {
            visible: !(ss.mode === "delete" && ss.facts.snapper) && !(ss.mode === "restore" && !ss.busy && !ss.facts.source)
            text: ss.mode === "snapshot" ? "Take Snapshot" : ss.mode === "restore" ? "Restore" : "Delete"
            primary: true
            destructive: ss.mode === "delete"
            enabled: !ss.busy && !!ss.facts.subvol && (ss.mode !== "snapshot" || nameField.text.trim() !== "")
            tip: ss.mode === "snapshot" ? "Make the snapshot now" : ss.mode === "restore" ? "Swap in the snapshot's contents (the current ones are kept aside)" : "Delete the " + ss.kindText + " and everything in it"
            onClicked: ss.go()
        }
    ]
    function go() {
        const p = path
        if (mode === "snapshot") {
            const ro = roBox.checked
            Async.call(app.grove, k => app.grove.snapshotSubvolume(k, p, placeField.text.replace(/^~/, app.grove.home()), nameField.text.trim(), ro), r => {
                if (r.error) { win.toast("error", r.error); return }
                win.toast("info", "Snapshot “" + r.path.substring(r.path.lastIndexOf("/") + 1) + "” taken")
                win.forEachPane(q => { if (r.path.indexOf(q.path + "/") === 0) q.model.reload() })
            })
        } else if (mode === "restore") {
            const target = facts.source
            if (!target) { close(); return }
            const aside = facts.asideName || ""
            Async.call(app.grove, k => app.grove.restoreSubvolume(k, p, target, aside), r => {
                if (r.error) { win.toast("error", r.error); return }
                win.toast("info", "“" + target.substring(target.lastIndexOf("/") + 1) + "” restored. The previous contents are in “" + r.aside.substring(r.aside.lastIndexOf("/") + 1) + "”")
                const parent = target.substring(0, target.lastIndexOf("/")) || "/"
                // Panes inside it were showing the set-aside copy: reload them.
                win.forEachPane(q => { if (q.path === parent || q.path === target || q.path.indexOf(target + "/") === 0) q.model.reload() })
            })
        } else {
            Async.call(app.grove, k => app.grove.deleteSubvolume(k, p), r => {
                if (r.error) { win.toast("error", r.error); return }
                win.toast("info", "“" + p.substring(p.lastIndexOf("/") + 1) + "” deleted")
                const parent = p.substring(0, p.lastIndexOf("/")) || "/"
                win.forEachPane(q => {
                    if (q.path === p || q.path.indexOf(p + "/") === 0) q.navigate(parent)
                    else if (q.path === parent) q.model.reload()
                })
            })
        }
        close()
    }
}
