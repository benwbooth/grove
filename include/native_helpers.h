// Qt-side helpers grove's Rust code calls: the application, the QML engine
// with its image providers (thumbnails, theme icons), file types
// (QMimeDatabase), the clipboard, and model change notifications.
#pragma once

#include <QtCore/QAbstractListModel>
#include <QtCore/QString>
#include <cstdint>
#include "rust/cxx.h"

// The application object: native text rendering, names, icon theme.
void grove_create_app();
// Create the QML engine (thumbnail and icon providers), load `url`, run.
int grove_run(const QString& url, const QString& importPath);
// "mime\nicon\ngenericIcon\ncomment" for a file name (deep: sniff `path`).
QString grove_mime_info(const QString& name, const QString& path, bool deep);
// Parent types of a MIME type, newline-separated.
QString grove_mime_parents(const QString& mime);
// Put paths on the clipboard (text/uri-list, plus the cut markers GNOME
// and KDE apps read).
void grove_set_clipboard(const QString& pathsNewline, bool cut);
// {"cut": bool, "paths": [...]} from the clipboard.
QString grove_clipboard();
// Put plain text on the clipboard (Copy Path).
void grove_set_clipboard_text(const QString& text);
// Whether Qt can decode this image file itself.
bool grove_can_read_image(const QString& path);
// Rotate an image file in place (degrees: 90, -90, 180) / convert it.
QString grove_image_rotate(const QString& path, int degrees);
QString grove_image_convert(const QString& path, const QString& format);
// The model told its views rows first..last changed.
void grove_rows_changed(QAbstractListModel& model, int32_t first, int32_t last);
// layoutAboutToBeChanged (done=false) / layoutChanged (done=true).
void grove_layout_changing(QAbstractListModel& model, bool done);
// Breeze or Breeze Dark icons to match grove's light/dark look.
void grove_set_icon_theme(bool dark, bool mac);
// Test hook: save every visible window (the first to `path`, others to
// path-1.png, path-2.png...).
void grove_grab_windows(const QString& path);
// Test hook: send a key press to the focused window.
void grove_test_key(int32_t key, int32_t modifiers, const QString& text);
