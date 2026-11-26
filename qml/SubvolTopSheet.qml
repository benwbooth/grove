import QtQuick
import QtQuick.Controls.Basic

// Show All Subvolumes: opens the top of a btrfs disk (subvolid=5), where
// @, @home, @snapshots and the like live, read-only unless asked. Says
// plainly when the administrator password is needed and why, and that it's
// closed again on Eject or when Grove quits.
Sheet {
    id: ts
    property var app
    property var win
    property string spec: ""
    property var facts: ({})
    property bool busy: false
    property bool working: false

    title: facts.name ? "Show All Subvolumes of “" + facts.name + "”?" : "Show All Subvolumes"
    message: busy ? "" : facts.error ? facts.error
        : "This opens the top of the btrfs disk, above the folders you normally see, where subvolumes like @ (the system), @home and the snapshots live. "
          + "It opens read-only, so nothing there can change by accident."

    function ask(s) {
        spec = s
        facts = ({})
        busy = true
        working = false
        rwBox.checked = false
        Async.call(app.grove, k => app.grove.subvolTopInfo(k, s), r => {
            ts.busy = false
            ts.facts = r || ({})
            // Already mounted somewhere (e.g. /mnt/stuff): just go there.
            if (r && r.existing) { ts.close(); ts.win.go(r.existing) }
        })
        open()
    }

    GCheck {
        id: rwBox
        visible: !ts.busy && !ts.facts.error
        text: "Allow changes (writable)"
        tip: "Open it so files and subvolumes there can be changed, renamed or deleted. Be careful: @ is the running system"
    }
    GLabel {
        visible: rwBox.checked
        width: parent.width
        wrapMode: Text.Wrap
        elide: Text.ElideNone
        color: Theme.danger
        text: "Changes made there are real: deleting or renaming @ or @home can stop the system from starting."
    }
    GLabel {
        visible: !ts.busy && !ts.facts.error
        width: parent.width
        wrapMode: Text.Wrap
        elide: Text.ElideNone
        color: Theme.fgSecondary
        text: ts.facts.needsRoot
            ? "Your password will be asked for. This disk is in use (" + (ts.facts.device || "") + "), and only an administrator can open it a second time at its top, so Grove asks the system for permission to mount just that, in a private folder of yours. It's closed again when you eject it in the sidebar or quit Grove."
            : "No password needed: the disk isn't open yet, so Grove opens it at its top instead. Eject it in the sidebar when you're done; quitting Grove closes it too."
    }
    buttons: [
        PushButton { text: ts.facts.error ? "OK" : "Cancel"; tip: "Close without opening anything"; onClicked: ts.close() },
        PushButton {
            visible: !ts.facts.error
            text: ts.working ? "Opening…" : "Show Subvolumes"
            primary: true
            enabled: !ts.busy && !ts.working && !!ts.facts.device
            tip: ts.facts.needsRoot ? "Ask for the administrator password and open the disk's top" : "Open the disk's top"
            onClicked: ts.go()
        }
    ]
    function go() {
        const s = spec
        const rw = rwBox.checked
        working = true
        Async.call(app.grove, k => app.grove.openSubvolTop(k, s, rw), r => {
            ts.working = false
            ts.close()
            if (r.error) { ts.win.toast("error", r.error); return }
            ts.win.go(r.path)
        })
    }
}
