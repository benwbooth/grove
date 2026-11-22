import QtQuick
import QtQuick.Controls.Basic

// Finder's View Options (Ctrl+J): a floating glass panel for the current
// folder. Icon size, grid spacing, text size, label position, item info,
// previews, grouping, sorting, stacks and the background. Each folder keeps
// its own options; "Use as Defaults" applies them to every other folder.
Popup {
    id: vo
    property var win
    readonly property var pane: win ? win.activePane : null
    readonly property bool icons: pane && pane.viewMode === "icons"
    modal: false
    focus: true
    closePolicy: Popup.CloseOnEscape
    width: Math.round(300 * Theme.pt)
    padding: 16
    topPadding: 14.001
    leftPadding: 16.001
    background: GlassPanel { tint: Theme.glassMenu; radius: Theme.glassRadius }

    readonly property var swatches: ["#ffffff", "#f2f2f7", "#e5e5ea", "#1c1c1e", "#2c2c2e", "#dbeafe", "#d1fae5", "#fef3c7", "#fee2e2", "#ede9fe", "#fce7f3", "#e0f2fe"]

    component Caption: GLabel { font.pixelSize: Theme.small; color: Theme.fgSecondary }
    component OptSlider: Slider {
        id: sl
        property string tip: ""
        width: parent.width
        height: 20
        snapMode: Slider.SnapAlways
        hoverEnabled: true
        background: Rectangle {
            x: sl.leftPadding; y: sl.topPadding + sl.availableHeight / 2 - 2
            width: sl.availableWidth; height: 4; radius: 2
            color: Theme.separator
            Rectangle { width: sl.visualPosition * parent.width; height: parent.height; radius: 2; color: Theme.accent }
        }
        handle: Rectangle {
            x: sl.leftPadding + sl.visualPosition * (sl.availableWidth - width)
            y: sl.topPadding + sl.availableHeight / 2 - height / 2
            width: 16; height: 16; radius: 8
            color: "white"
            border.color: Qt.rgba(0, 0, 0, 0.25)
        }
        GTip { visible: sl.hovered && sl.tip !== ""; text: sl.tip }
    }
    component Line: Rectangle { width: parent.width; height: 1; color: Theme.separator }

    contentItem: Column {
        spacing: 8
        Item {
            width: parent.width
            height: titleL.height
            GLabel {
                id: titleL
                width: parent.width - 30
                text: vo.pane ? vo.pane.title : ""
                font.bold: true
                elide: Text.ElideMiddle
            }
            GButton {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                width: 22; height: 22
                iconName: "window-close"; iconSize: 10
                tip: "Close View Options (Esc or Ctrl+J)"
                onClicked: vo.close()
            }
        }
        Caption { text: vo.icons ? "Icon view options for this folder" : "These options are for icon view (Ctrl+1)"; width: parent.width; wrapMode: Text.Wrap; elide: Text.ElideNone }
        GCheck {
            text: "Always open in " + (vo.pane ? ({ icons: "icon", list: "list", columns: "column", gallery: "gallery" })[vo.pane.viewMode] || "this" : "this") + " view"
            checked: vo.pane && vo.pane.alwaysOpenIn
            tip: "This folder always opens in this view. Otherwise folders keep the view you're browsing in, as in Finder."
            onToggled: vo.pane.setViewOption("alwaysOpenIn", checked)
        }
        Line {}

        // --- sort / group ---
        Row {
            width: parent.width
            spacing: 8
            Caption { text: "Group by:"; width: 72; anchors.verticalCenter: parent.verticalCenter }
            GCombo {
                id: groupBox
                width: parent.width - 80
                readonly property var values: ["", "kind", "date", "size", "name", "tags"]
                model: ["None", "Kind", "Date Modified", "Size", "Name", "Tags"]
                currentIndex: vo.pane ? Math.max(0, values.indexOf(vo.pane.groupBy)) : 0
                onActivated: (i) => vo.pane.setGroupBy(values[i])
                tip: "Show items in groups with headings (turns stacks off)"
            }
        }
        Row {
            width: parent.width
            spacing: 8
            Caption { text: "Sort by:"; width: 72; anchors.verticalCenter: parent.verticalCenter }
            GCombo {
                width: parent.width - 80
                readonly property var values: ["name", "kind", "modified", "created", "added", "accessed", "size"]
                model: ["Name", "Kind", "Date Modified", "Date Created", "Date Added", "Last Opened", "Size"]
                currentIndex: vo.pane ? Math.max(0, values.indexOf(vo.pane.model.sortKey)) : 0
                onActivated: (i) => { vo.pane.model.applySort(values[i], vo.pane.model.descending); vo.pane.rememberView() }
                tip: "The order of items (within each group)"
            }
        }
        Row {
            width: parent.width
            spacing: 8
            Caption { text: "Stacks:"; width: 72; anchors.verticalCenter: parent.verticalCenter }
            GCombo {
                width: parent.width - 80
                readonly property var values: ["", "kind", "date"]
                model: ["Off", "By Kind", "By Date Modified"]
                currentIndex: vo.pane ? Math.max(0, values.indexOf(vo.pane.stacks)) : 0
                onActivated: (i) => vo.pane.setStacks(values[i])
                tip: "Pile loose files of one kind (or date) into stacks you can open in place (View › Use Stacks)"
            }
        }
        Line {}

        // --- icon size, spacing ---
        Item {
            width: parent.width; height: 16
            Caption { text: "Icon size:" }
            Caption { anchors.right: parent.right; text: vo.pane ? vo.pane.iconSize + " × " + vo.pane.iconSize : "" }
        }
        OptSlider {
            readonly property var sizes: [16, 24, 32, 48, 64, 96, 128, 192, 256]
            from: 0; to: 8; stepSize: 1
            value: vo.pane ? Math.max(0, sizes.indexOf(vo.pane.iconSize)) : 4
            onMoved: { vo.pane.iconSize = sizes[value]; vo.pane.rememberView() }
            tip: "How big icons and previews are (Ctrl+= / Ctrl+-)"
        }
        Caption { text: "Grid spacing:" }
        OptSlider {
            from: 1; to: 10; stepSize: 1
            value: vo.pane ? (vo.pane.gridSpacing || 4) : 4
            onMoved: vo.pane.setViewOption("gridSpacing", value)
            tip: "Room between icons in the grid"
        }
        Row {
            width: parent.width
            spacing: 8
            Caption { text: "Text size:"; width: 72; anchors.verticalCenter: parent.verticalCenter }
            GCombo {
                width: 90
                readonly property var values: [10, 11, 12, 13, 14, 15, 16]
                model: values.map(v => v + " pt")
                currentIndex: vo.pane ? Math.max(0, values.indexOf(vo.pane.textSize || 13)) : 3
                onActivated: (i) => vo.pane.setViewOption("textSize", values[i] === 13 ? 0 : values[i])
                tip: "Size of the names under (or next to) icons"
            }
        }
        Row {
            spacing: 8
            Caption { text: "Label position:"; anchors.verticalCenter: parent.verticalCenter }
            GButton {
                text: "Bottom"; flat: false; capsule: true
                checked: vo.pane && vo.pane.labelPos !== "right"
                tip: "Names under the icons"
                onClicked: vo.pane.setViewOption("labelPos", "bottom")
            }
            GButton {
                text: "Right"; flat: false; capsule: true
                checked: vo.pane && vo.pane.labelPos === "right"
                tip: "Names to the right of the icons"
                onClicked: vo.pane.setViewOption("labelPos", "right")
            }
        }
        GCheck {
            text: "Show item info"
            checked: vo.pane && vo.pane.itemInfo
            tip: "A second line under names: picture size, or file size"
            onToggled: vo.pane.setViewOption("itemInfo", checked)
        }
        GCheck {
            text: "Show icon preview"
            checked: vo.pane && vo.pane.previewsOn
            tip: "Pictures, videos and PDFs in this folder show their contents (Dolphin's per-folder previews; the default is in Settings)"
            onToggled: vo.pane.setViewOption("previews", checked)
        }
        Line {}

        // --- background ---
        Caption { text: "Background:" }
        Row {
            spacing: 6
            GButton {
                text: "Default"; flat: false; capsule: true
                checked: vo.pane && vo.pane.background === ""
                tip: "The window's own background"
                onClicked: vo.pane.setViewOption("background", "")
            }
            GButton {
                text: "Colour"; flat: false; capsule: true
                checked: vo.pane && vo.pane.background.indexOf("color:") === 0
                tip: "A plain colour behind the icons (pick one below)"
                onClicked: vo.pane.setViewOption("background", "color:" + (Theme.dark ? "#2c2c2e" : "#f2f2f7"))
            }
            GButton {
                text: "Picture"; flat: false; capsule: true
                checked: vo.pane && vo.pane.background.indexOf("picture:") === 0
                tip: "A picture behind the icons: drop one on the well below, or choose it"
                onClicked: vo.choosePicture()
            }
        }
        Flow {
            visible: vo.pane && vo.pane.background.indexOf("color:") === 0
            width: parent.width
            spacing: 6
            Repeater {
                model: vo.swatches
                Rectangle {
                    required property string modelData
                    width: 20; height: 20; radius: 10
                    color: modelData
                    border.width: vo.pane && vo.pane.background === "color:" + modelData ? 2 : 1
                    border.color: vo.pane && vo.pane.background === "color:" + modelData ? Theme.accent : Theme.glassEdge
                    MouseArea { id: swm; anchors.fill: parent; hoverEnabled: true; onClicked: vo.pane.setViewOption("background", "color:" + parent.modelData) }
                    GTip { visible: swm.containsMouse; text: "Use " + parent.modelData + " behind the icons" }
                }
            }
        }
        Rectangle {
            visible: vo.pane && vo.pane.background.indexOf("picture:") === 0
            width: parent.width
            height: 70
            radius: 8
            color: Theme.field
            border.width: well.containsDrag ? 2 : 1
            border.color: well.containsDrag ? Theme.accent : Theme.separator
            Image {
                anchors.fill: parent
                anchors.margins: 4
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                sourceSize.width: 300
                source: vo.pane && vo.pane.background.indexOf("picture:") === 0 ? "file://" + encodeURI(vo.pane.background.substring(8)) : ""
            }
            GLabel {
                anchors.centerIn: parent
                visible: vo.pane && vo.pane.background === "picture:"
                text: "Drop a picture here"
                color: Theme.fgSecondary
            }
            DropArea {
                id: well
                anchors.fill: parent
                keys: ["text/uri-list"]
                onDropped: (d) => {
                    const u = d.urls.length ? d.urls[0].toString() : ""
                    if (u.indexOf("file://") === 0) vo.pane.setViewOption("background", "picture:" + decodeURIComponent(u.substring(7)))
                }
            }
        }
        Line {}
        PushButton {
            anchors.horizontalCenter: parent.horizontalCenter
            text: "Use as Defaults"
            tip: "Use these options for every folder that hasn't got its own"
            onClicked: vo.pane.useAsDefaults()
        }
    }

    function choosePicture() {
        const p = vo.pane
        p.setViewOption("background", p.background.indexOf("picture:") === 0 ? p.background : "picture:")
        if (vo.win.app.screenshot !== "") return
        vo.win.chooseFiles("Choose a Background Picture", { filters: [{ name: "Pictures", patterns: [{ mime: true, pattern: "image/*" }] }], currentFolder: vo.win.app.grove.home() + "/Pictures" },
            paths => { if (paths.length) p.setViewOption("background", "picture:" + paths[0]) })
    }
}
