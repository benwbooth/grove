#include <QWheelEvent>
#include <QPointingDevice>
#include <QUrlQuery>
#include <QPainter>
#include <QColor>
#include "native_helpers.h"
#include "grove/src/bridge.cxxqt.h"

#include <QtCore/QBuffer>
#include <QtCore/QDebug>
#include <QtCore/QFile>
#include <QtCore/QJsonArray>
#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtCore/QMimeData>
#include <QtCore/QMimeDatabase>
#include <QtCore/QThreadPool>
#include <QtCore/QUrl>
#include <QtGui/QClipboard>
#include <QtGui/QFontDatabase>
#include <QtCore/QDir>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QImageReader>
#include <QtGui/QImageWriter>
#include <QtGui/QKeyEvent>
#include <QtGui/QPainter>
#include <QtGui/QPainterPath>
#include <QtGui/QTransform>
#include <QtGui/QWindow>
#include <QtPdf/QPdfDocument>
#include <QtQml/QQmlApplicationEngine>
#include <QtQml/QQmlContext>
#include <QtQuick/QQuickImageProvider>
#include <QtQuick/QQuickWindow>
#include <atomic>
#include <algorithm>

static QString fromRust(const rust::String& s) { return QString::fromUtf8(s.data(), int(s.size())); }
static rust::String toRust(const QString& s) { const QByteArray b = s.toUtf8(); return rust::String(b.constData(), size_t(b.size())); }

// ---- thumbnails (freedesktop spec; grove_core::thumbs does the cache) ----

class ThumbResponse : public QQuickImageResponse, public QRunnable {
public:
    ThumbResponse(const QString& path, const QSize& req, QThreadPool* pool) : m_path(path), m_req(req), m_pool(pool) { setAutoDelete(false); }
    QQuickTextureFactory* textureFactory() const override { return QQuickTextureFactory::textureFactoryForImage(m_img); }
    // No picture: an error, so the view keeps showing the icon (and
    // doesn't cache an empty image for this file).
    QString errorString() const override { return m_img.isNull() ? QStringLiteral("no thumbnail") : QString(); }
    // Scrolled away: drop it if it hasn't started (the engine still needs
    // finished() to clean up).
    void cancel() override
    {
        m_cancelled = true;
        if (m_pool->tryTake(this)) QMetaObject::invokeMethod(this, [this] { emit finished(); }, Qt::QueuedConnection);
    }
    std::atomic<bool> m_cancelled { false };
    std::atomic<bool> m_started { false };

    void run() override
    {
        m_started = true;
        if (!m_cancelled) produce();
        emit finished();
    }

private:
    void produce()
    {
        const int px = qMax(16, qMax(m_req.width(), m_req.height()));
        const rust::String rp = toRust(m_path);
        const QString cached = fromRust(grove_thumb_lookup(rp, uint32_t(px)));
        if (!cached.isEmpty() && m_img.load(cached)) return;
        if (grove_thumb_failed(rp)) return;
        const uint32_t flavor = grove_thumb_flavor(uint32_t(px));
        QMimeDatabase db;
        const QMimeType mt = db.mimeTypeForFile(m_path);
        const QString mime = mt.name();
        QImage img;
        if (mime.startsWith(QLatin1String("image/"))) {
            QImageReader r(m_path);
            r.setAutoTransform(true);
            if (r.canRead()) {
                QSize s = r.size();
                if (s.isValid() && (s.width() > int(flavor) || s.height() > int(flavor)))
                    r.setScaledSize(s.scaled(int(flavor), int(flavor), Qt::KeepAspectRatio));
                img = r.read();
            }
        } else if (mime == QLatin1String("application/pdf")) {
            QPdfDocument doc;
            if (doc.load(m_path) == QPdfDocument::Error::None && doc.pageCount() > 0) {
                const QSizeF pt = doc.pagePointSize(0);
                const QSize s = pt.toSize().scaled(int(flavor), int(flavor), Qt::KeepAspectRatio);
                QImage page = doc.render(0, s);
                if (!page.isNull()) {
                    img = QImage(page.size(), QImage::Format_ARGB32_Premultiplied);
                    img.fill(Qt::white);
                    QPainter p(&img);
                    p.drawImage(0, 0, page);
                }
            }
        }
        if (m_cancelled) return;
        if (!img.isNull()) {
            if (img.width() > int(flavor) || img.height() > int(flavor))
                img = img.scaled(int(flavor), int(flavor), Qt::KeepAspectRatio, Qt::SmoothTransformation);
            // Save with the spec's keys (Rust writes the PNG text chunks).
            QByteArray png;
            {
                QBuffer buf(&png);
                buf.open(QIODevice::WriteOnly);
                img.save(&buf, "PNG");
            }
            rust::Slice<const uint8_t> bytes(reinterpret_cast<const uint8_t*>(png.constData()), size_t(png.size()));
            grove_thumb_store(rp, flavor, bytes, toRust(mime));
            m_img = img;
            return;
        }
        // External thumbnailers (ffmpegthumbnailer, heif-thumbnailer, ...).
        QStringList mimes { mime };
        for (const QString& p : mt.allAncestors()) mimes << p;
        const QString out = fromRust(grove_thumb_external(rp, toRust(mimes.join(QLatin1Char('\n'))), uint32_t(px)));
        if (!out.isEmpty()) m_img.load(out);
        else if (!m_cancelled) grove_thumb_mark_failed(rp);
    }

    QString m_path;
    QSize m_req;
    QThreadPool* m_pool;
    QImage m_img;
};

class ThumbProvider : public QQuickAsyncImageProvider {
public:
    ThumbProvider() { m_pool.setMaxThreadCount(qBound(2, QThread::idealThreadCount() / 2, 6)); }
    QQuickImageResponse* requestImageResponse(const QString& id, const QSize& req) override
    {
        // id: <percent-encoded path>?m=<mtime> (the mtime only busts caches).
        QString path = id;
        const int q = path.lastIndexOf(QLatin1Char('?'));
        if (q >= 0) path.truncate(q);
        path = QUrl::fromPercentEncoding(path.toUtf8());
        auto* r = new ThumbResponse(path, req.isValid() ? req : QSize(128, 128), &m_pool);
        // Newest first: what's on screen now beats what scrolled away.
        m_pool.start(r, ++m_seq);
        return r;
    }

private:
    QThreadPool m_pool;
    int m_seq = 0;
};

// ---- theme icons: image://icon/name|fallback|fallback ----
class IconProvider : public QQuickImageProvider {
public:
    // QImage, not QPixmap: this runs on a loader thread, where pixmaps
    // aren't safe on real GPU-backed windows (icons came out garbled).
    IconProvider() : QQuickImageProvider(QQuickImageProvider::Image, QQuickImageProvider::ForceAsynchronousImageLoading) {}
    QImage requestImage(const QString& id, QSize* size, const QSize& req) override
    {
        const int s = qMax(16, qMax(req.width(), req.height()));
        QIcon icon;
        QString key = id;
        QColor tint;
        const int q = key.lastIndexOf(QLatin1Char('?'));  // ?r=<rev> busts caches; &tint=rrggbb recolours
        if (q >= 0) {
            const QUrlQuery query(key.mid(q + 1));
            if (query.hasQueryItem(QStringLiteral("tint")))
                tint = QColor(QLatin1Char('#') + query.queryItemValue(QStringLiteral("tint")));
            key.truncate(q);
        }
        const QString decoded = QUrl::fromPercentEncoding(key.toUtf8());
        QString matched;
        for (const QString& n : decoded.split(QLatin1Char('|'), Qt::SkipEmptyParts)) {
            if (n.startsWith(QLatin1Char('/'))) { icon = QIcon(n); if (!icon.isNull()) break; continue; }
            if (QIcon::hasThemeIcon(n)) { icon = QIcon::fromTheme(n); matched = n; break; }
        }
        // File and folder pictures (mime types, places) only: line-art
        // action icons must render at their real size, since shrinking 1 px
        // strokes leaves gaps.
        static const QStringList filePrefixes {
            QStringLiteral("folder"), QStringLiteral("inode-"), QStringLiteral("text-"), QStringLiteral("application-"),
            QStringLiteral("image-"), QStringLiteral("video-"), QStringLiteral("audio-"), QStringLiteral("font-"),
            QStringLiteral("model-"), QStringLiteral("package-"), QStringLiteral("x-office"), QStringLiteral("user-home"),
            QStringLiteral("user-desktop"), QStringLiteral("unknown") };
        bool fileIcon = false;
        for (const QString& p : filePrefixes) if (matched.startsWith(p)) { fileIcon = true; break; }
        if (matched.contains(QLatin1String("-x-"))) fileIcon = true;
        if (matched.endsWith(QLatin1String("-new")) || matched.endsWith(QLatin1String("-symbolic"))) fileIcon = false;
        if (icon.isNull()) icon = QIcon::fromTheme(QStringLiteral("unknown"));
        // Small full-colour icons (list rows) come from the 32 px art scaled
        // down: Finder shows little blue folders there, while themes like
        // WhiteSur draw their 16 px sizes as outlines.
        // Always at device-pixel ratio 1 and exactly the requested pixel
        // size: QML already asks for size × the screen scale, and letting
        // QIcon apply the fractional scale (4/3) again smeared the art.
        QImage pm;
        if (fileIcon && !tint.isValid() && s < 32) {
            pm = icon.pixmap(QSize(32, 32), 1.0).toImage();
            if (!pm.isNull() && pm.width() > s)
                pm = pm.scaled(s, s, Qt::KeepAspectRatio, Qt::SmoothTransformation);
        } else {
            pm = icon.pixmap(QSize(s, s), 1.0).toImage();
        }
        if (pm.isNull()) { if (size) *size = QSize(); return pm; }
        pm = pm.convertToFormat(QImage::Format_ARGB32_Premultiplied);
        pm.setDevicePixelRatio(1.0);
        if (tint.isValid()) {
            // Monochrome glyph in one colour, keeping its alpha (Finder's sidebar).
            QPainter p(&pm);
            p.setCompositionMode(QPainter::CompositionMode_SourceIn);
            p.fillRect(pm.rect(), tint);
        }
        if (size) *size = pm.size();
        return pm;
    }
};

// Logs the first wheel events the app receives, before any QML sees them
// (to tell "never arrives" from "arrives but goes to the wrong place").
class WheelLogFilter : public QObject {
public:
    int left = 80;
    bool eventFilter(QObject* o, QEvent* e) override
    {
        if (left > 0 && e->type() == QEvent::Wheel && o->isWindowType()) {
            auto* w = static_cast<QWheelEvent*>(e);
            left--;
            qWarning().noquote() << "grove raw wheel: angle" << w->angleDelta().x() << w->angleDelta().y()
                                 << "pixel" << w->pixelDelta().x() << w->pixelDelta().y()
                                 << "phase" << int(w->phase()) << "at" << w->position().x() << w->position().y()
                                 << "device" << (w->pointingDevice() ? w->pointingDevice()->name() : QString());
        }
        return false;
    }
};

void grove_create_app()
{
    // Native (FreeType) text everywhere: Qt Quick's distance-field text is
    // soft at fractional scales.
    QQuickWindow::setTextRenderType(QQuickWindow::NativeTextRendering);
    static int argc = 1;
    static char name[] = "grove";
    static char* argv[] = { name, nullptr };
    new QGuiApplication(argc, argv);
    QGuiApplication::setApplicationName(QStringLiteral("grove"));
    qApp->installEventFilter(new WheelLogFilter);
    QGuiApplication::setApplicationDisplayName(QStringLiteral("Grove"));
    QGuiApplication::setOrganizationName(QStringLiteral("grove"));
    QGuiApplication::setDesktopFileName(QStringLiteral("grove"));
    // Inter (SF Pro's open relative) when SF Pro isn't installed: the
    // package hands its folder over in GROVE_FONT_DIR, as hearth does.
    const QString fontDir = QString::fromLocal8Bit(qgetenv("GROVE_FONT_DIR"));
    if (!fontDir.isEmpty()) {
        const QFileInfoList files = QDir(fontDir).entryInfoList({ QStringLiteral("Inter.ttc") }, QDir::Files);
        for (const QFileInfo& f : files) QFontDatabase::addApplicationFont(f.absoluteFilePath());
    }
    // Icon theme folders from the XDG directories (some platform plugins,
    // e.g. offscreen, leave only Qt's resources).
    {
        QStringList paths = QIcon::themeSearchPaths();
        const QString home = QString::fromLocal8Bit(qgetenv("HOME"));
        QString dataHome = QString::fromLocal8Bit(qgetenv("XDG_DATA_HOME"));
        if (dataHome.isEmpty()) dataHome = home + QStringLiteral("/.local/share");
        QStringList want { dataHome + QStringLiteral("/icons"), home + QStringLiteral("/.icons") };
        QString dirs = QString::fromLocal8Bit(qgetenv("XDG_DATA_DIRS"));
        if (dirs.isEmpty()) dirs = QStringLiteral("/usr/local/share:/usr/share");
        for (const QString& d : dirs.split(QLatin1Char(':'), Qt::SkipEmptyParts)) want << d + QStringLiteral("/icons");
        want << QStringLiteral("/run/current-system/sw/share/icons") << home + QStringLiteral("/.nix-profile/share/icons");
        for (const QString& w : want)
            if (!paths.contains(w) && QFile::exists(w)) paths << w;
        QIcon::setThemeSearchPaths(paths);
    }
    if (QIcon::themeName().isEmpty() || QIcon::themeName() == QLatin1String("hicolor"))
        QIcon::setThemeName(QStringLiteral("breeze"));
    QIcon::setFallbackThemeName(QStringLiteral("breeze"));
    if (qEnvironmentVariableIsSet("GROVE_DEBUG_ICONS"))
        qWarning() << "grove icons: theme" << QIcon::themeName() << "paths" << QIcon::themeSearchPaths() << "has folder" << QIcon::hasThemeIcon(QStringLiteral("folder"));
    QGuiApplication::setWindowIcon(QIcon::fromTheme(QStringLiteral("grove"), QIcon::fromTheme(QStringLiteral("system-file-manager"))));
}

int grove_run(const QString& url, const QString& importPath)
{
    auto* engine = new QQmlApplicationEngine;
    engine->addImageProvider(QStringLiteral("thumb"), new ThumbProvider);
    engine->addImageProvider(QStringLiteral("icon"), new IconProvider);
    if (!importPath.isEmpty()) engine->addImportPath(importPath);
    engine->load(QUrl(url));
    if (engine->rootObjects().isEmpty()) return 1;
    const int code = QGuiApplication::exec();
    return code;
}

QString grove_mime_info(const QString& name, const QString& path, bool deep)
{
    static thread_local QMimeDatabase db;
    QMimeType t = deep && !path.isEmpty() ? db.mimeTypeForFile(path) : db.mimeTypeForFile(name, QMimeDatabase::MatchExtension);
    if (!t.isValid()) t = db.mimeTypeForName(QStringLiteral("application/octet-stream"));
    return t.name() + QLatin1Char('\n') + t.iconName() + QLatin1Char('\n') + t.genericIconName() + QLatin1Char('\n') + t.comment();
}

QString grove_mime_parents(const QString& mime)
{
    static thread_local QMimeDatabase db;
    return db.mimeTypeForName(mime).allAncestors().join(QLatin1Char('\n'));
}

void grove_set_clipboard(const QString& pathsNewline, bool cut)
{
    auto* md = new QMimeData;
    QList<QUrl> urls;
    QStringList plain;
    for (const QString& p : pathsNewline.split(QLatin1Char('\n'), Qt::SkipEmptyParts)) {
        urls << QUrl::fromLocalFile(p);
        plain << p;
    }
    md->setUrls(urls);
    md->setText(plain.join(QLatin1Char('\n')));
    QByteArray gnome = cut ? "cut" : "copy";
    for (const QUrl& u : urls) gnome += '\n' + u.toEncoded();
    md->setData(QStringLiteral("x-special/gnome-copied-files"), gnome);
    md->setData(QStringLiteral("application/x-kde-cutselection"), cut ? "1" : "0");
    QGuiApplication::clipboard()->setMimeData(md);
}

QString grove_clipboard()
{
    const QMimeData* md = QGuiApplication::clipboard()->mimeData();
    QJsonObject o;
    QJsonArray paths;
    bool cut = false;
    if (md) {
        if (md->hasFormat(QStringLiteral("application/x-kde-cutselection")))
            cut = md->data(QStringLiteral("application/x-kde-cutselection")).startsWith('1');
        if (md->hasFormat(QStringLiteral("x-special/gnome-copied-files")))
            cut = cut || md->data(QStringLiteral("x-special/gnome-copied-files")).startsWith("cut");
        for (const QUrl& u : md->urls())
            if (u.isLocalFile()) paths.append(u.toLocalFile());
        if (paths.isEmpty() && md->hasText()) {
            for (const QString& l : md->text().split(QLatin1Char('\n'), Qt::SkipEmptyParts)) {
                const QString t = l.trimmed();
                if (t.startsWith(QLatin1Char('/')) && QFile::exists(t)) paths.append(t);
                else if (t.startsWith(QLatin1String("file://"))) paths.append(QUrl(t).toLocalFile());
            }
        }
    }
    o[QStringLiteral("cut")] = cut;
    o[QStringLiteral("paths")] = paths;
    return QString::fromUtf8(QJsonDocument(o).toJson(QJsonDocument::Compact));
}

void grove_set_clipboard_text(const QString& text) { QGuiApplication::clipboard()->setText(text); }

bool grove_can_read_image(const QString& path)
{
    QImageReader r(path);
    return r.canRead();
}

QString grove_image_rotate(const QString& path, int degrees)
{
    QImageReader r(path);
    r.setAutoTransform(true);
    const QByteArray fmt = r.format();
    QImage img = r.read();
    if (img.isNull()) return QStringLiteral("This picture can't be opened for rotating.");
    img = img.transformed(QTransform().rotate(degrees), Qt::SmoothTransformation);
    QImageWriter w(path, fmt);
    if (fmt == "jpeg" || fmt == "jpg") w.setQuality(95);
    if (!w.write(img)) return QStringLiteral("Couldn't save the rotated picture: ") + w.errorString();
    return QString();
}

QString grove_markup_render(const QString& src, const QString& opsJson, const QString& dest)
{
    QImageReader r(src);
    r.setAutoTransform(true);
    QImage img = r.read();
    if (img.isNull()) return QStringLiteral("This picture can't be opened.");
    const QJsonObject spec = QJsonDocument::fromJson(opsJson.toUtf8()).object();
    img = img.convertToFormat(img.hasAlphaChannel() ? QImage::Format_ARGB32_Premultiplied : QImage::Format_RGB32);
    {
        QPainter p(&img);
        p.setRenderHint(QPainter::Antialiasing);
        p.setRenderHint(QPainter::TextAntialiasing);
        for (const QJsonValue& v : spec.value(QStringLiteral("ops")).toArray()) {
            const QJsonObject o = v.toObject();
            const QString t = o.value(QStringLiteral("t")).toString();
            QColor c(o.value(QStringLiteral("c")).toString(QStringLiteral("#ff3b30")));
            c.setAlphaF(o.value(QStringLiteral("a")).toDouble(1.0));
            const double w = o.value(QStringLiteral("w")).toDouble(4);
            QPen pen(c, w, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin);
            p.setPen(pen);
            p.setBrush(Qt::NoBrush);
            const QPointF a(o.value(QStringLiteral("x1")).toDouble(), o.value(QStringLiteral("y1")).toDouble());
            const QPointF b(o.value(QStringLiteral("x2")).toDouble(), o.value(QStringLiteral("y2")).toDouble());
            if (t == QLatin1String("pen")) {
                const QJsonArray pts = o.value(QStringLiteral("p")).toArray();
                QPainterPath path;
                for (int i = 0; i + 1 < pts.size(); i += 2) {
                    const QPointF q(pts[i].toDouble(), pts[i + 1].toDouble());
                    if (i == 0) path.moveTo(q); else path.lineTo(q);
                }
                if (pts.size() == 2) path.lineTo(QPointF(pts[0].toDouble() + 0.01, pts[1].toDouble()));
                // A highlighter shouldn't darken where its stroke crosses itself.
                p.drawPath(path);
            } else if (t == QLatin1String("rect")) {
                p.drawRect(QRectF(a, b).normalized());
            } else if (t == QLatin1String("ellipse")) {
                p.drawEllipse(QRectF(a, b).normalized());
            } else if (t == QLatin1String("line") || t == QLatin1String("arrow")) {
                p.drawLine(a, b);
                if (t == QLatin1String("arrow")) {
                    const QLineF l(b, a);
                    const double len = std::max(12.0, w * 4);
                    QLineF h1 = l; h1.setLength(len); h1.setAngle(l.angle() + 28);
                    QLineF h2 = l; h2.setLength(len); h2.setAngle(l.angle() - 28);
                    p.drawLine(h1);
                    p.drawLine(h2);
                }
            } else if (t == QLatin1String("text")) {
                QFont f;
                f.setPixelSize(std::max(6, int(o.value(QStringLiteral("s")).toDouble(24))));
                f.setBold(true);
                p.setFont(f);
                p.drawText(a, o.value(QStringLiteral("text")).toString());
            }
        }
    }
    const QJsonArray crop = spec.value(QStringLiteral("crop")).toArray();
    if (crop.size() == 4) {
        const QRect cr = QRectF(crop[0].toDouble(), crop[1].toDouble(), crop[2].toDouble(), crop[3].toDouble()).toAlignedRect().intersected(img.rect());
        if (cr.width() > 1 && cr.height() > 1) img = img.copy(cr);
    }
    const int rot = spec.value(QStringLiteral("rotate")).toInt(0) % 360;
    if (rot != 0) img = img.transformed(QTransform().rotate(rot), Qt::SmoothTransformation);
    QImageWriter wr(dest);
    const QByteArray fmt = wr.format().toLower();
    if (fmt == "jpeg" || fmt == "jpg") { wr.setQuality(92); img = img.convertToFormat(QImage::Format_RGB32); }
    if (!wr.write(img)) return QStringLiteral("Couldn't save the picture: ") + wr.errorString();
    return QString();
}

QString grove_image_convert(const QString& path, const QString& format)
{
    QImageReader r(path);
    r.setAutoTransform(true);
    QImage img = r.read();
    if (img.isNull()) return QStringLiteral("!This picture can't be opened.");
    QString out = path;
    const int dot = out.lastIndexOf(QLatin1Char('.'));
    if (dot > out.lastIndexOf(QLatin1Char('/'))) out.truncate(dot);
    out += QLatin1Char('.') + format;
    int n = 2;
    while (QFile::exists(out)) {
        QString base = path;
        if (dot > base.lastIndexOf(QLatin1Char('/'))) base.truncate(dot);
        out = base + QStringLiteral(" %1.").arg(n++) + format;
    }
    QImageWriter w(out, format.toLatin1());
    if (!w.write(img)) return QStringLiteral("!Couldn't save: ") + w.errorString();
    return out;
}

void grove_rows_changed(QAbstractListModel& model, int32_t first, int32_t last)
{
    if (first < 0 || last < first) return;
    emit model.dataChanged(model.index(first, 0), model.index(last, 0));
}

void grove_layout_changing(QAbstractListModel& model, bool done)
{
    if (done) emit model.layoutChanged();
    else emit model.layoutAboutToBeChanged();
}

void grove_test_key(int32_t key, int32_t modifiers, const QString& text)
{
    QWindow* w = QGuiApplication::focusWindow();
    if (!w && !QGuiApplication::topLevelWindows().isEmpty()) w = QGuiApplication::topLevelWindows().first();
    if (!w) return;
    QKeyEvent press(QEvent::KeyPress, key, Qt::KeyboardModifiers(modifiers), text);
    QKeyEvent release(QEvent::KeyRelease, key, Qt::KeyboardModifiers(modifiers), text);
    QGuiApplication::sendEvent(w, &press);
    QGuiApplication::sendEvent(w, &release);
}

// Test hook: a real wheel event at window position (x, y), delivered the
// way the platform delivers one (so handlers, popups and overlays all see it).
void grove_test_wheel(double x, double y, int32_t angleY, int32_t pixelY)
{
    QWindow* w = QGuiApplication::focusWindow();
    if (!w && !QGuiApplication::topLevelWindows().isEmpty()) w = QGuiApplication::topLevelWindows().first();
    if (!w) return;
    const QPointF pos(x, y);
    QWheelEvent ev(pos, w->mapToGlobal(pos), QPoint(0, pixelY), QPoint(0, angleY), Qt::NoButton, Qt::NoModifier,
                   Qt::NoScrollPhase, false, Qt::MouseEventNotSynthesized, QPointingDevice::primaryPointingDevice());
    QGuiApplication::sendEvent(w, &ev);
}

void grove_test_wheel_x(double x, double y, int32_t angleX)
{
    QWindow* w = QGuiApplication::focusWindow();
    if (!w && !QGuiApplication::topLevelWindows().isEmpty()) w = QGuiApplication::topLevelWindows().first();
    if (!w) return;
    const QPointF pos(x, y);
    QWheelEvent ev(pos, w->mapToGlobal(pos), QPoint(), QPoint(angleX, 0), Qt::NoButton, Qt::NoModifier,
                   Qt::NoScrollPhase, false, Qt::MouseEventNotSynthesized, QPointingDevice::primaryPointingDevice());
    QGuiApplication::sendEvent(w, &ev);
}

// Test hook: one trackpad scroll event exactly as Qt on Wayland makes it
// (a touchpad device, pixel and angle amounts, system-synthesized source).
void grove_test_touchpad(double x, double y, int32_t ax, int32_t ay, int32_t px, int32_t py)
{
    static QPointingDevice* pad = nullptr;
    if (!pad) {
        pad = new QPointingDevice(QStringLiteral("touchpad"), 990, QInputDevice::DeviceType::TouchPad,
                                  QPointingDevice::PointerType::Finger, QInputDevice::Capability::Position, 5, 0);
    }
    QWindow* w = QGuiApplication::focusWindow();
    if (!w && !QGuiApplication::topLevelWindows().isEmpty()) w = QGuiApplication::topLevelWindows().first();
    if (!w) return;
    const QPointF pos(x, y);
    QWheelEvent ev(pos, w->mapToGlobal(pos), QPoint(px, py), QPoint(ax, ay), Qt::NoButton, Qt::NoModifier,
                   Qt::NoScrollPhase, false, Qt::MouseEventSynthesizedBySystem, pad);
    QGuiApplication::sendEvent(w, &ev);
}

static QString g_systemIconTheme;

static bool iconThemeInstalled(const QString& name)
{
    for (const QString& d : QIcon::themeSearchPaths())
        if (QFile::exists(d + QLatin1Char('/') + name + QStringLiteral("/index.theme"))) return true;
    return false;
}

// mac: WhiteSur (Finder-like folders and file icons, shipped with grove);
// otherwise the desktop's theme, switching breeze <-> breeze-dark.
void grove_set_icon_theme(bool dark, bool mac)
{
    if (g_systemIconTheme.isEmpty()) g_systemIconTheme = QIcon::themeName();
    QString want = g_systemIconTheme;
    if (mac && iconThemeInstalled(QStringLiteral("WhiteSur")))
        want = dark ? QStringLiteral("WhiteSur-dark") : QStringLiteral("WhiteSur");
    else if (want.startsWith(QLatin1String("breeze"), Qt::CaseInsensitive))
        want = dark ? QStringLiteral("breeze-dark") : QStringLiteral("breeze");
    if (QIcon::themeName() != want) QIcon::setThemeName(want);
}

void grove_grab_windows(const QString& path)
{
    int n = 0;
    QList<QQuickWindow*> wins;
    for (QWindow* w : QGuiApplication::topLevelWindows())
        if (auto* q = qobject_cast<QQuickWindow*>(w); q && q->isVisible()) wins << q;
    // Browser windows first (largest), then the rest.
    std::stable_sort(wins.begin(), wins.end(), [](QQuickWindow* a, QQuickWindow* b) { return a->width() * a->height() > b->width() * b->height(); });
    for (QQuickWindow* q : wins) {
        QString out = path;
        if (n > 0) out.replace(QStringLiteral(".png"), QStringLiteral("-%1.png").arg(n));
        q->grabWindow().save(out);
        n++;
    }
}
