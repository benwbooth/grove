import QtQuick
import QtQuick.Controls.Basic

// Connect to Server (Ctrl+K): plain-language fields for SFTP, Windows
// shares (SMB), WebDAV, cloud drives set up in rclone, and phones (MTP).
// Servers found on the network are listed to pick from.
Sheet {
    id: cs
    property var app
    property var win
    property string kind: "sftp"
    property var found: ({ hosts: [], remotes: [] })
    property bool busy: false
    property string error: ""
    title: "Connect to Server"
    width: Math.min(parent ? parent.width - 40 : 560, Math.round(520 * Theme.pt))

    function start(s) {
        error = ""; busy = false
        if (s) { kind = s.kind; host.text = s.host || ""; user.text = s.user || ""; folder.text = s.path || ""; nameField.text = s.name || "" }
        else { host.text = ""; user.text = ""; folder.text = ""; password.text = ""; nameField.text = "" }
        open()
        Async.call(app.grove, k => app.grove.discoverNetwork(k), r => cs.found = r)
    }
    function connect() {
        busy = true; error = ""
        const j = { kind: kind, host: host.text.trim(), user: user.text.trim(), path: folder.text.trim(), password: password.text,
                    port: Number(port.text) || 0, remote: remote.currentText || "", device: "", name: nameField.text.trim(),
                    remember: remember.checked, savePassword: savePw.checked }
        Async.call(app.grove, k => app.grove.connectServer(k, JSON.stringify(j)), r => {
            busy = false
            if (r.error) { cs.error = r.error; return }
            password.text = ""
            cs.close()
            cs.win.go(r.path)
        })
    }

    Segmented {
        items: [
            { icon: "network-server", tip: "SFTP: a computer you can reach with SSH", value: "sftp" },
            { icon: "folder-remote|network-workgroup", tip: "Windows / Samba share (SMB)", value: "smb" },
            { icon: "folder-cloud|network-workgroup", tip: "WebDAV (Nextcloud, ownCloud, …)", value: "webdav" },
            { icon: "folder-cloud|cloudstatus", tip: "A cloud drive set up with `rclone config` (Google Drive, Dropbox, OneDrive…)", value: "rclone" },
            { icon: "phone|smartphone", tip: "A phone connected with USB (choose “File transfer” on the phone)", value: "mtp" }
        ]
        current: cs.kind
        onPicked: (v) => { cs.kind = v; cs.error = "" }
    }
    GLabel {
        width: parent.width
        wrapMode: Text.Wrap
        elide: Text.ElideNone
        color: Theme.fgSecondary
        text: ({
            sftp: "SFTP: files on another computer over SSH. If your SSH key works, leave the password empty.",
            smb: "A shared folder on a Windows PC or a NAS. The share name goes in Folder (for example “Public” or “Public/Photos”).",
            webdav: "A WebDAV address, such as https://cloud.example.com/remote.php/dav/files/you/",
            rclone: "Cloud drives you've set up in rclone (run `rclone config` once in a terminal to add Google Drive, Dropbox, OneDrive…).",
            mtp: "Plug in the phone with USB, unlock it, and choose “File transfer” on its notification."
        })[cs.kind]
    }
    Grid {
        columns: 2
        columnSpacing: 10
        rowSpacing: 8
        width: parent.width
        visible: cs.kind !== "mtp"
        GLabel { text: cs.kind === "webdav" ? "Address" : cs.kind === "rclone" ? "Drive" : "Server"; width: 80; horizontalAlignment: Text.AlignRight; height: Theme.controlHeight }
        Item {
            width: parent.width - 90; height: Theme.controlHeight
            GField { id: host; anchors.fill: parent; visible: cs.kind !== "rclone"; placeholderText: cs.kind === "webdav" ? "https://…" : "name or address, e.g. nas.local"; tip: "The computer's name or IP address" }
            ComboBox {
                id: remote
                anchors.fill: parent
                visible: cs.kind === "rclone"
                model: cs.found.remotes || []
                font.family: Theme.family
                font.pixelSize: Theme.body
                displayText: count ? currentText : "No rclone drives set up yet"
            }
        }
        GLabel { text: "User"; width: 80; horizontalAlignment: Text.AlignRight; height: Theme.controlHeight; visible: cs.kind !== "rclone" }
        GField { id: user; width: parent.width - 90; visible: cs.kind !== "rclone"; placeholderText: cs.kind === "sftp" ? cs.app.grove.env("USER") : "user name (empty: guest)" }
        GLabel { text: "Password"; width: 80; horizontalAlignment: Text.AlignRight; height: Theme.controlHeight; visible: cs.kind !== "rclone" }
        GField { id: password; width: parent.width - 90; visible: cs.kind !== "rclone"; echoMode: TextInput.Password; placeholderText: "leave empty to use a saved one or your SSH key" }
        GLabel { text: "Folder"; width: 80; horizontalAlignment: Text.AlignRight; height: Theme.controlHeight }
        GField { id: folder; width: parent.width - 90; placeholderText: cs.kind === "smb" ? "share name, e.g. Public" : "optional, e.g. /home/me" }
        GLabel { text: "Port"; width: 80; horizontalAlignment: Text.AlignRight; height: Theme.controlHeight; visible: cs.kind === "sftp" || cs.kind === "smb" }
        GField { id: port; width: 90; visible: cs.kind === "sftp" || cs.kind === "smb"; placeholderText: cs.kind === "sftp" ? "22" : "445" }
        GLabel { text: "Name"; width: 80; horizontalAlignment: Text.AlignRight; height: Theme.controlHeight }
        GField { id: nameField; width: parent.width - 90; placeholderText: "how it's called in the sidebar (optional)" }
    }
    Row {
        spacing: 16
        GCheck { id: remember; text: "Remember this server"; checked: true; tip: "Keep it in the sidebar under Network" }
        GCheck { id: savePw; text: "Save password in keyring"; visible: cs.kind !== "rclone" && cs.kind !== "mtp"; tip: "Stored with the desktop's Secret Service (KWallet), not in a file" }
    }
    Column {
        width: parent.width
        spacing: 4
        visible: (cs.found.hosts || []).length > 0 && cs.kind !== "rclone" && cs.kind !== "mtp"
        GLabel { text: "Found on your network:"; color: Theme.fgSecondary; font.pixelSize: Theme.small }
        Flow {
            width: parent.width
            spacing: 6
            Repeater {
                model: (cs.found.hosts || []).filter(h => h.kind === cs.kind || (cs.kind === "sftp" && h.kind === "sftp"))
                GButton {
                    required property var modelData
                    flat: false; capsule: true
                    text: modelData.name
                    iconName: "network-server"
                    tip: modelData.host + " (" + modelData.address + ")"
                    onClicked: { host.text = modelData.host; if (modelData.port && modelData.port !== 22 && modelData.port !== 445) port.text = modelData.port }
                }
            }
        }
    }
    GLabel { visible: cs.error !== ""; width: parent.width; text: cs.error; color: Theme.danger; wrapMode: Text.Wrap; elide: Text.ElideNone }
    buttons: [
        BusyDots { running: cs.busy; anchors.verticalCenter: parent ? parent.verticalCenter : undefined },
        PushButton { text: "Cancel"; onClicked: cs.close() },
        PushButton { text: "Connect"; primary: true; enabled: !cs.busy && (cs.kind === "mtp" || cs.kind === "rclone" ? true : host.text !== ""); onClicked: cs.connect() }
    ]
}
