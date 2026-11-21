import QtQuick
import QtQuick.Controls.Basic

// Glass menu (hearth's Look: 12 pt corners, 4 pt padding, 24 pt rows,
// accent selection). Fades and scales in from its anchor.
Menu {
    id: m
    topPadding: Theme.menuPad + 0.001
    bottomPadding: Theme.menuPad
    leftPadding: Theme.menuPad + 0.001
    rightPadding: Theme.menuPad
    overlap: 3
    cascade: true
    implicitWidth: Math.max(Math.round(250 * Theme.pt), contentItem.implicitWidth + leftPadding + rightPadding)
    delegate: GMenuItem {}
    background: GlassPanel {
        implicitWidth: Math.round(200 * Theme.pt)
        tint: Theme.glassMenu
        radius: Theme.glassRadius
    }
    enter: Transition {
        ParallelAnimation {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 160; easing.type: Easing.OutCubic }
            NumberAnimation { property: "scale"; from: 0.94; to: 1; duration: 220; easing.type: Easing.OutBack; easing.overshoot: 1.2 }
        }
    }
    exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: 140 } }
}
