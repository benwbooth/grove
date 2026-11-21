#include "native_helpers.h"
#include "grove/src/bridge.cxxqt.h"

#include <QtCore/QBuffer>
#include <QtCore/QFile>
#include <QtCore/QJsonArray>
#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtCore/QMimeData>
#include <QtCore/QMimeDatabase>
#include <QtCore/QThreadPool>
#include <QtCore/QUrl>
#include <QtGui/QClipboard>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QImageReader>
#include <QtGui/QImageWriter>
#include <QtGui/QKeyEvent>
#include <QtGui/QPainter>
#include <QtGui/QTransform>
#include <QtGui/QWindow>
#include <QtPdf/QPdfDocument>
#include <QtQml/QQmlApplicationEngine>
#include <QtQml/QQmlContext>
#include <QtQuick/QQuickImageProvider>
#include <QtQuick/QQuickWindow>
#include <atomic>

static QString fromRust(const rust::String& s) { return QString::fromUtf8(s.data(), int(s.size())); }
static rust::String toRust(const QString& s) { const QByteArray b = s.toUtf8(); return rust::String(b.constData(), size_t(b.size())); }

// ---- thumbnails (freedesktop spec; grove_core::thumbs does the cache) ----

class ThumbResponse : public QQuickImageResponse, public QRunnable {
public:
    ThumbResponse(const QString& path, const QSize& req, QThreadPool* pool) : m_path(path), m_req(req), m_pool(pool) { setAutoDelete(false); }
    QQuickTextureFactory* textureFactory() const override { return QQuickTextureFactory::textureFactoryForImage(m_img); }
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
    IconProvider() : QQuickImageProvider(QQuickImageProvider::Pixmap, QQuickImageProvider::ForceAsynchronousImageLoading) {}
    QPixmap requestPixmap(const QString& id, QSize* size, const QSize& req) override
    {
        const int s = qMax(16, qMax(req.width(), req.height()));
        QIcon icon;
        for (const QString& n : id.split(QLatin1Char('|'), Qt::SkipEmptyParts)) {
            if (n.startsWith(QLatin1Char('/'))) { icon = QIcon(n); if (!icon.isNull()) break; continue; }
            if (QIcon::hasThemeIcon(n)) { icon = QIcon::fromTheme(n); break; }
        }
        if (icon.isNull()) icon = QIcon::fromTheme(QStringLiteral("unknown"));
        QPixmap pm = icon.pixmap(QSize(s, s));
        if (size) *size = pm.size();
        return pm;
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
    QGuiApplication::setApplicationDisplayName(QStringLiteral("Grove"));
    QGuiApplication::setOrganizationName(QStringLiteral("grove"));
    QGuiApplication::setDesktopFileName(QStringLiteral("grove"));
    if (QIcon::themeName().isEmpty() || QIcon::themeName() == QLatin1String("hicolor"))
        QIcon::setThemeName(QStringLiteral("breeze"));
    QIcon::setFallbackThemeName(QStringLiteral("breeze"));
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
