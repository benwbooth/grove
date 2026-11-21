import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window

// Settings, each with a one-line explanation. Changes apply at once and
// are saved to ~/.config/grove/settings.json.
Window {
    id: sw
    property var app
    readonly property var s: app.settings
    width: Math.round(560 * Theme.pt)
    height: Math.round(620 * Theme.pt)
    minimumWidth: 420
    color: Theme.window
    title: "Grove Settings"
    function set(name, value) { app.grove.setSetting(name, JSON.stringify(value)) }

    component Group: Column {
        property string title: ""
        default property alias rows: inner.data
        width: parent ? parent.width : 0
        spacing: 2
        GLabel { text: parent.title; font.bold: true; topPadding: 12; bottomPadding: 4 }
        Rectangle {
            width: parent.width
            height: inner.height + 8
            radius: 10
            color: Theme.field
            Column { id: inner; y: 4; width: parent.width; spacing: 0 }
        }
    }
    component ToggleRow: Item {
        property string label: ""
        property string help: ""
        property string key: ""
        property bool invert: false
        width: parent ? parent.width : 0
        height: Math.max(44, col.height + 14)
        Column {
            id: col
            x: 12
            width: parent.width - 80
            anchors.verticalCenter: parent.verticalCenter
            GLabel { text: parent.parent.label; width: parent.width }
            GLabel { text: parent.parent.help; width: parent.width; font.pixelSize: Theme.small; color: Theme.fgSecondary; wrapMode: Text.Wrap; elide: Text.ElideNone }
        }
        GSwitch {
            anchors.right: parent.right; anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            checked: parent.invert ? !sw.s[parent.key] : !!sw.s[parent.key]
            onToggled: sw.set(parent.key, parent.invert ? !checked : checked)
            tip: parent.help
        }
    }
    component ChoiceRow: Item {
        property string label: ""
        property string help: ""
        property string key: ""
        property var choices: []     // [{label, value}]
        width: parent ? parent.width : 0
        height: Math.max(44, col2.height + 14)
        Column {
            id: col2
            x: 12
            width: parent.width - 200
            anchors.verticalCenter: parent.verticalCenter
            GLabel { text: parent.parent.label; width: parent.width }
            GLabel { text: parent.parent.help; width: parent.width; font.pixelSize: Theme.small; color: Theme.fgSecondary; wrapMode: Text.Wrap; elide: Text.ElideNone }
        }
        GCombo {
            anchors.right: parent.right; anchors.rightMargin: 12
            anchors.verticalCenter: parent.verticalCenter
            width: 170
            font.family: Theme.family
            font.pixelSize: Theme.body
            model: parent.choices.map(c => c.label)
            currentIndex: Math.max(0, parent.choices.findIndex(c => c.value === sw.s[parent.key]))
            onActivated: (i) => sw.set(parent.key, parent.choices[i].value)
        }
    }

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth
        transform: Translate { x: 0.001; y: 0.001 }
        Column {
            x: 20
            width: sw.width - 40
            bottomPadding: 20
            Group {
                title: "Appearance"
                ChoiceRow { label: "Light or dark"; help: "Automatic follows hearth's setting (or the time of day)."; key: "theme"; choices: [{ label: "Automatic", value: "auto" }, { label: "Dark", value: "dark" }, { label: "Light", value: "light" }] }
                ChoiceRow { label: "Icons"; help: "macOS style looks like Finder. System uses your desktop's icon theme."; key: "icon_style"; choices: [{ label: "macOS style", value: "macos" }, { label: "System", value: "system" }] }
                ChoiceRow { label: "New folders open as"; help: "The view for folders you haven't opened before."; key: "default_view"; choices: [{ label: "Icons", value: "icons" }, { label: "List", value: "list" }, { label: "Columns", value: "columns" }, { label: "Gallery", value: "gallery" }] }
                ToggleRow { label: "Remember each folder's view"; help: "Folders reopen in the view, sort order and icon size you left them in."; key: "per_folder_views" }
                ToggleRow { label: "Show previews (thumbnails)"; help: "Pictures, videos and PDFs show their contents instead of an icon."; key: "show_thumbnails" }
                ToggleRow { label: "Thumbnails in list view"; help: "Small previews next to names in list view too."; key: "list_thumbnails" }
                ToggleRow { label: "Status bar"; help: "The line at the bottom with item counts and the icon size slider."; key: "show_status_bar" }
                ToggleRow { label: "Grove's own title bar"; help: "Window buttons in the sidebar, like macOS. Off uses the desktop's title bar (applies to new windows)."; key: "client_decorations" }
                ToggleRow { label: "Git status badges"; help: "Show which files changed in git folders (M, A, ?)."; key: "show_git_status" }
            }
            Group {
                title: "Files and folders"
                ToggleRow { label: "Show hidden files"; help: "Files whose names start with a dot (Ctrl+H)."; key: "show_hidden" }
                ToggleRow { label: "Folders first"; help: "Keep folders above files when sorting by name."; key: "folders_first" }
                ToggleRow { label: "Open with a single click"; help: "Dolphin's style. Off: double-click to open, like Finder."; key: "single_click" }
                ToggleRow { label: "Browse archives as folders"; help: "Opening a .zip, .tar or .7z shows what's inside instead of extracting it."; key: "browse_archives" }
                ToggleRow { label: "Ask before moving to the Trash"; help: "Moving to the Trash can always be undone, so this is off by default."; key: "confirm_trash" }
                ToggleRow { label: "Ask before emptying the Trash"; help: "Emptying the Trash can't be undone."; key: "confirm_empty_trash" }
            }
            Group {
                title: "Search"
                ToggleRow { label: "Search inside files by default"; help: "The toolbar search looks at file contents, not just names."; key: "search_contents" }
                ToggleRow { label: "Skip what .gitignore lists"; help: "Leaves build output and other ignored files out of search results."; key: "search_respect_ignore" }
            }
            Group {
                title: "Terminal"
                Item {
                    width: parent.width
                    height: 64
                    Column {
                        x: 12
                        width: parent.width - 250
                        anchors.verticalCenter: parent.verticalCenter
                        GLabel { text: "Terminal app" }
                        GLabel { text: "Used by Open in Terminal. Empty picks one: tessera, konsole, foot…"; width: parent.width; font.pixelSize: Theme.small; color: Theme.fgSecondary; wrapMode: Text.Wrap; elide: Text.ElideNone }
                    }
                    GField {
                        anchors.right: parent.right; anchors.rightMargin: 12
                        anchors.verticalCenter: parent.verticalCenter
                        width: 220
                        text: sw.s.terminal || ""
                        placeholderText: "automatic"
                        onEditingFinished: sw.set("terminal", text)
                    }
                }
            }
            Group {
                title: "Default file manager"
                GLabel {
                    x: 12
                    width: parent.width - 24
                    topPadding: 8; bottomPadding: 8
                    wrapMode: Text.Wrap; elide: Text.ElideNone
                    font.pixelSize: Theme.small
                    color: Theme.fgSecondary
                    text: "Grove answers “Show in folder” from browsers and other apps (org.freedesktop.FileManager1) whenever it's running or installed as the default. To make folders open in Grove everywhere, it's set as the app for inode/directory."
                }
                Row {
                    x: 12
                    spacing: 8
                    bottomPadding: 8
                    PushButton {
                        text: "Make Grove the Default"
                        tip: "Sets Grove for folders in ~/.config/mimeapps.list"
                        onClicked: Async.call(sw.app.grove, k => sw.app.grove.setDefaultApp(k, "inode/directory", "grove.desktop"), r => done.text = r.error ? r.error : "Done: folders open in Grove.")
                    }
                    GLabel { id: done; anchors.verticalCenter: parent.verticalCenter; color: Theme.fgSecondary }
                }
            }
        }
    }
}
