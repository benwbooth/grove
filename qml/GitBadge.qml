import QtQuick

// Version control overlay ("CODE/KIND", see vcs.rs): ✓ clean, M modified,
// A added, ? untracked, D deleted, R renamed, U conflict, I ignored; a
// repository's own folder ("@/KIND") shows which system it is.
Rectangle {
    id: g
    property string status: ""
    readonly property string code: status.split("/")[0]
    readonly property string kind: status.split("/")[1] || ""
    readonly property bool repo: code === "@"
    readonly property string system: ({ "git": "git", "jj": "Jujutsu", "hg": "Mercurial", "svn": "Subversion", "fossil": "Fossil", "bzr": "Bazaar" })[kind] || "version control"
    visible: status !== ""
    width: repo ? Math.max(14, mark.implicitWidth + 7) : 14
    height: 14
    radius: 7
    color: repo ? ({ "git": "#f05032", "jj": "#7c3aed", "hg": "#999999", "svn": "#3b6fb6", "fossil": "#548c9c", "bzr": "#c8a400" })[kind] || "#8e8e93"
         : code === "M" ? "#ff9f0a" : code === "A" ? "#30d158" : code === "?" ? "#8e8e93" : code === "D" ? "#ff453a" : code === "U" ? "#ff375f"
         : code === "C" ? "#28a745" : code === "I" ? "#5a5a5e" : "#64d2ff"
    border.width: 1.5
    border.color: Theme.content
    GLabel {
        id: mark
        anchors.centerIn: parent
        text: g.repo ? ({ "git": "git", "jj": "jj", "hg": "hg", "svn": "svn", "fossil": "fsl", "bzr": "bzr" })[g.kind] || "vcs" : g.code === "C" ? "✓" : g.code === "I" ? "–" : g.code
        color: "white"
        font.pixelSize: g.repo ? 8 : 9
        font.bold: true
    }
    GTip {
        visible: gm.containsMouse
        text: g.repo ? "A " + g.system + " repository"
            : ({ "C": "Unchanged since the last " + g.system + " commit",
                 "M": "Changed since the last commit (" + g.system + ")",
                 "A": "Added to " + g.system + ", not committed yet",
                 "?": "Not tracked by " + g.system,
                 "D": "Deleted (" + g.system + ")",
                 "R": "Renamed (" + g.system + ")",
                 "U": "Merge conflict (" + g.system + ")",
                 "I": "Ignored by " + g.system })[g.code] || ""
    }
    MouseArea { id: gm; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
}
