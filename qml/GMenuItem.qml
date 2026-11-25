import QtQuick

// A row of a GMenu: the shared MacMenuItem (icon via `iconName`, a colour
// `swatch`, the key in `shortcutText`), with a tooltip.
MacMenuItem {
    id: mi
    property string tip: ""
    GTip { visible: mi.hovered && mi.tip !== ""; text: mi.tip }
}
