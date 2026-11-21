import QtQuick
import QtQuick.Controls.Basic

// "An item named X already exists": Replace (the old one goes to the
// Trash), Skip, Keep Both, Merge (folders), Stop; optionally for all.
Sheet {
    id: c
    property var app
    property int jobId: -1
    property var info: ({})
    property var queue: []
    readonly property string name: info.target ? info.target.substring(info.target.lastIndexOf("/") + 1) : ""
    readonly property string where: info.target ? info.target.substring(0, info.target.lastIndexOf("/")) : ""
    readonly property bool bothDirs: !!info.source_is_dir && !!info.target_is_dir
    title: "“" + name + "” already exists"
    message: bothDirs
        ? "A folder with this name is already in “" + where.substring(where.lastIndexOf("/") + 1) + "”. Merge puts the contents together (you'll be asked about files that are in both); Replace moves the existing folder to the Trash."
        : "An item with this name is already in “" + where.substring(where.lastIndexOf("/") + 1) + "”. Replace moves the existing one to the Trash, so you can get it back."
    closePolicy: Popup.NoAutoClose

    function ask(id, i) {
        if (opened) { queue.push({ id: id, info: i }); return }
        jobId = id
        info = i
        all.checked = false
        open()
    }
    function answer(r) {
        if (jobId >= 0) app.grove.answerConflict(jobId, r, all.checked)
        close()
        if (queue.length) { const n = queue.shift(); Qt.callLater(() => ask(n.id, n.info)) }
    }
    function fmt(t) { return t ? new Date(t * 1000).toLocaleString(Qt.locale(), Locale.ShortFormat) : "" }

    Grid {
        columns: 2
        columnSpacing: 16
        rowSpacing: 4
        width: parent.width
        GLabel { text: "Existing:"; color: Theme.fgSecondary; font.bold: true }
        GLabel { text: (c.info.target_is_dir ? "Folder" : c.app.size(c.info.target_size || 0)) + ", modified " + c.fmt(c.info.target_mtime) }
        GLabel { text: "New:"; color: Theme.fgSecondary; font.bold: true }
        GLabel { text: (c.info.source_is_dir ? "Folder" : c.app.size(c.info.source_size || 0)) + ", modified " + c.fmt(c.info.source_mtime) + (c.info.source_mtime > c.info.target_mtime ? "  (newer)" : c.info.source_mtime < c.info.target_mtime ? "  (older)" : "") }
    }
    GCheck { id: all; text: "Do this for all remaining conflicts"; tip: "Use the same answer for every other item that already exists in this operation" }
    buttons: [
        PushButton { text: "Stop"; tip: "Stop the whole operation (what's done stays done)"; onClicked: c.answer("cancel") },
        PushButton { text: "Skip"; tip: "Leave the existing item and don't copy/move this one"; onClicked: c.answer("skip") },
        PushButton { text: "Keep Both"; tip: "Give the new one a different name (“name 2”)"; onClicked: c.answer("keepboth") },
        PushButton { visible: c.bothDirs; text: "Merge"; primary: true; tip: "Put the contents of both folders together"; onClicked: c.answer("merge") },
        PushButton { text: "Replace"; primary: !c.bothDirs; tip: "Move the existing item to the Trash and put the new one here"; onClicked: c.answer("replace") }
    ]
}
