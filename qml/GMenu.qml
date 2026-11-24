import QtQuick
import QtQuick.Controls.Basic

// Glass menu (hearth's Look: 12 pt corners, 4 pt padding, 24 pt rows,
// accent selection). Fades in.
Menu {
    id: m
    topPadding: Theme.menuPad + 0.001
    bottomPadding: Theme.menuPad
    leftPadding: Theme.menuPad + 0.001
    rightPadding: Theme.menuPad
    overlap: 3
    // Drawn inside the app window, not as its own popup window: a separate
    // window got a different scale from arbor at 4/3 and its icons lost pixels.
    popupType: Popup.Item
    cascade: true
    implicitWidth: Math.max(Math.round(250 * Theme.pt), contentItem.implicitWidth + leftPadding + rightPadding)
    delegate: GMenuItem {}
    background: GlassPanel {
        implicitWidth: Math.round(200 * Theme.pt)
        tint: Theme.glassMenu
        radius: Theme.glassRadius
    }
    // Appears at once (a fade in made every right-click feel slow), fades
    // out, as macOS menus do. (No scaling: it resampled the icons.)
    enter: Transition { NumberAnimation { property: "opacity"; from: 1; to: 1; duration: 0 } }
    exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: 140 } }
}
