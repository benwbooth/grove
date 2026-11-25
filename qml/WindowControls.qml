import QtQuick

// Minimize, maximize/restore and close at the right end of grove's toolbar:
// the shared ChromeButtons (hearth/shared/app-menus), drawn as arbor draws
// Breeze's on its title bars, the same in all of Ben's apps.
ChromeButtons {
    // The icons' colour (Grove Player: white over the picture).
    property color tint: Theme.fg
    fg: tint
    bg: Theme.window
}
