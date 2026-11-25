import QtQuick

// grove's menus: the shared macOS 26 menu (MacMenu, from
// hearth/shared/app-menus) in grove's theme, with icons from grove's own
// provider (off the UI thread, the theme art drawn 1:1).
MacMenu {
    dark: Theme.dark
    accent: Theme.accent
    iconUrl: (name, px) => Theme.iconUrl(name) + "&exact=1"
    delegate: GMenuItem {}
}
