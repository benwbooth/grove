#include <QWheelEvent>
#include <QPointingDevice>
#include <QUrlQuery>
#include <QPainter>
#include <QColor>
#include "native_helpers.h"
#include "global_menu.h"
#include <QtQml/qqml.h>
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
#include <QtGui/QRawFont>
#include <QtGui/QGlyphRun>
#include <QtGui/QVector3D>
#include <QtGui/QMatrix4x4>
#include <QtCore/QElapsedTimer>
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
#include <QtGui/QTextBlock>
#include <QtGui/QTextCursor>
#include <QtGui/QTextDocument>
#include <QtGui/QTextFrame>
#include <QtGui/QTextTable>
#include <QtGui/QTransform>
#include <QtGui/QWindow>
#include <QtPdf/QPdfDocument>
#include <QtQml/QQmlApplicationEngine>
#include <QtQml/QQmlContext>
#include <QtQuick/QQuickImageProvider>
#include <QtQuick/QQuickWindow>
#include <atomic>
#include <functional>
#include <algorithm>

static QString fromRust(const rust::String& s) { return QString::fromUtf8(s.data(), int(s.size())); }
static rust::String toRust(const QString& s) { const QByteArray b = s.toUtf8(); return rust::String(b.constData(), size_t(b.size())); }

// ---- previews: thumbnails (freedesktop spec; grove_core::thumbs keeps the
// cache), PDF pages for Quick Look, font specimens ----
//
// Three pools, so nothing waits behind something slower:
//   lookup: cache hits (a stat and a small PNG read),
//   make:   in-process drawing (Qt-readable pictures, text pages, fonts,
//           3D models),
//   tools:  external tools (PDF pages, video frames, cover art, office
//           documents, HEIC/RAW...; grove/src/thumbgen.rs).
// Quick Look's PDF pages get a pool of their own. Newest requests first;
// a request whose item scrolled away is dropped if it hasn't started and
// its tool is killed if it has.

namespace {

struct Pools {
    QThreadPool lookup, make, tools, pages;
    Pools()
    {
        const int ideal = QThread::idealThreadCount();
        lookup.setMaxThreadCount(2);
        make.setMaxThreadCount(qBound(2, ideal / 2, 4));
        tools.setMaxThreadCount(qBound(2, ideal / 2, 6));
        pages.setMaxThreadCount(3);
    }
};
// Never destroyed: a tool still running at exit mustn't hold up quitting.
Pools& pools() { static Pools* p = new Pools; return *p; }

std::atomic<quint64> g_token { 1 };
// Set when the app quits: every job stops at its next check.
std::atomic<bool> g_quitting { false };
std::atomic<int> g_seq { 0 };
bool thumbLog() { static const bool on = qEnvironmentVariableIsSet("GROVE_THUMB_LOG"); return on; }

// A request answered on a worker pool, possibly handed from one pool to
// another (lookup -> make/tools).
class JobResponse : public QQuickImageResponse, public QRunnable {
public:
    JobResponse() : m_token(g_token++), m_prio(++g_seq) { setAutoDelete(false); m_clock.start(); }
    ~JobResponse() override { grove_thumb_done(m_token); }
    QQuickTextureFactory* textureFactory() const override { return QQuickTextureFactory::textureFactoryForImage(m_img); }
    // No picture: an error, so the view keeps showing the icon (and
    // doesn't cache an empty image for this file).
    QString errorString() const override { return m_img.isNull() ? QStringLiteral("no preview") : QString(); }
    void cancel() override
    {
        m_cancelled = true;
        grove_thumb_cancel(m_token);
        QThreadPool* p = m_pool.load();
        if (p && p->tryTake(this)) QMetaObject::invokeMethod(this, [this] { finish(); }, Qt::QueuedConnection);
    }
    void startOn(QThreadPool& p) { m_pool = &p; p.start(this, m_prio); }
    void run() override
    {
        if (!m_cancelled && work()) return;  // handed on to another pool
        if (thumbLog()) qWarning().noquote() << "grove preview" << m_clock.elapsed() << "ms" << (m_cancelled ? "cancelled" : m_img.isNull() ? "none" : "ok") << m_what;
        finish();
    }

protected:
    // Do the work; true when it was handed to another pool instead.
    virtual bool work() = 0;
    void finish() { if (!m_finished.exchange(true)) emit finished(); }

    quint64 m_token;
    int m_prio;
    std::atomic<bool> m_cancelled { false };
    std::atomic<bool> m_finished { false };
    std::atomic<QThreadPool*> m_pool { nullptr };
    QElapsedTimer m_clock;
    QImage m_img;
    QString m_what;
};

QString takePath(QString id)
{
    const int q = id.lastIndexOf(QLatin1Char('?'));
    if (q >= 0) id.truncate(q);
    return QUrl::fromPercentEncoding(id.toUtf8());
}

// A picture file scaled to fit `px` (decoding only what's needed).
QImage readScaled(const QString& file, int px)
{
    QImageReader r(file);
    r.setAutoTransform(true);
    const QSize s = r.size();
    if (s.isValid() && (s.width() > px || s.height() > px)) r.setScaledSize(s.scaled(px, px, Qt::KeepAspectRatio));
    QImage img = r.read();
    if (!img.isNull() && (img.width() > px || img.height() > px))
        img = img.scaled(px, px, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    return img;
}

// Markdown as a styled document (QTextDocument's own reader): spacing
// between blocks, tinted code, accent links, quiet quotes, ruled tables,
// pictures fitted to `maxWidth`. Used for Quick Look (as HTML) and for
// thumbnails (drawn on a sheet of paper).
static void styleMarkdown(QTextDocument& doc, const QString& md, bool dark, const QString& family, qreal px, const QString& baseDir, qreal maxWidth)
{
    QFont f(family.isEmpty() ? QStringLiteral("Sans Serif") : family);
    f.setPixelSize(qMax(1, qRound(px)));
    doc.setDefaultFont(f);
    doc.setDocumentMargin(0);
    if (!baseDir.isEmpty()) doc.setBaseUrl(QUrl::fromLocalFile(baseDir + QLatin1Char('/')));
    doc.setMarkdown(md, QTextDocument::MarkdownDialectGitHub);
    const QColor fg = dark ? QColor(0xf5, 0xf5, 0xf7) : QColor(0x1d, 0x1d, 0x1f);
    const QColor quiet = dark ? QColor(0xa8, 0xa8, 0xae) : QColor(0x6e, 0x6e, 0x73);
    const QColor link = dark ? QColor(0x0a, 0x84, 0xff) : QColor(0x00, 0x66, 0xcc);
    const QColor codeBg = dark ? QColor(0x3a, 0x3a, 0x40) : QColor(0xeb, 0xeb, 0xf0);
    const QColor rule = dark ? QColor(0x48, 0x48, 0x4e) : QColor(0xd2, 0xd2, 0xd7);
    const QString mono = QFontDatabase::systemFont(QFontDatabase::FixedFont).family();
    QTextCursor c(&doc);
    for (QTextBlock b = doc.begin(); b.isValid(); b = b.next()) {
        QTextBlockFormat bf = b.blockFormat();
        const int level = bf.headingLevel();
        const bool code = bf.hasProperty(QTextFormat::BlockCodeFence) || bf.hasProperty(QTextFormat::BlockCodeLanguage) || bf.nonBreakableLines();
        const int quote = bf.intProperty(QTextFormat::BlockQuoteLevel);
        if (level > 0) {
            bf.setTopMargin(px * (level <= 2 ? 1.1 : 0.8));
            bf.setBottomMargin(px * 0.45);
        } else if (code) {
            const bool first = !b.previous().isValid() || !(b.previous().blockFormat().hasProperty(QTextFormat::BlockCodeFence) || b.previous().blockFormat().nonBreakableLines());
            const bool last = !b.next().isValid() || !(b.next().blockFormat().hasProperty(QTextFormat::BlockCodeFence) || b.next().blockFormat().nonBreakableLines());
            bf.setBackground(codeBg);
            bf.setTopMargin(first ? px * 0.4 : 0);
            bf.setBottomMargin(last ? px * 0.8 : 0);
            bf.setLeftMargin(0);
            bf.setIndent(0);
            bf.setTextIndent(px * 0.7);
        } else if (!b.textList()) {
            bf.setBottomMargin(px * 0.7);
        } else {
            // List items close together; space after the list.
            const bool lastItem = !b.next().isValid() || !b.next().textList();
            bf.setBottomMargin(lastItem ? px * 0.7 : px * 0.2);
        }
        if (quote > 0) bf.setLeftMargin(px * 1.4 * quote);
        c.setPosition(b.position());
        c.setBlockFormat(bf);
        // Per run: links, inline code, pictures, heading weight.
        for (auto it = b.begin(); !it.atEnd(); ++it) {
            const QTextFragment fr = it.fragment();
            if (!fr.isValid()) continue;
            QTextCharFormat cf = fr.charFormat();
            if (cf.isImageFormat()) {
                QTextImageFormat im = cf.toImageFormat();
                QString src = im.name();
                if (src.startsWith(QLatin1String("file:"))) src = QUrl(src).toLocalFile();
                else if (!src.contains(QLatin1String("://")) && !baseDir.isEmpty() && !src.startsWith(QLatin1Char('/'))) src = baseDir + QLatin1Char('/') + src;
                const QSize sz = QImageReader(src).size();
                if (sz.isValid() && maxWidth > 0 && sz.width() > maxWidth) {
                    im.setWidth(maxWidth);
                    im.setHeight(sz.height() * maxWidth / sz.width());
                }
                if (sz.isValid()) im.setName(QUrl::fromLocalFile(src).toString());
                c.setPosition(fr.position());
                c.setPosition(fr.position() + fr.length(), QTextCursor::KeepAnchor);
                c.setCharFormat(im);
                continue;
            }
            bool changed = false;
            if (cf.isAnchor()) { cf.setForeground(link); cf.setFontUnderline(false); changed = true; }
            else if (quote > 0) { cf.setForeground(quiet); changed = true; }
            else if (cf.fontFixedPitch() || code) {
                cf.setFontFamilies({ mono });
                if (!code) cf.setBackground(codeBg);
                changed = true;
            } else if (!cf.foreground().style() || cf.foreground().color() != fg) { cf.setForeground(fg); changed = true; }
            if (level > 0) { cf.setFontWeight(QFont::Bold); changed = true; }
            if (code || cf.fontFixedPitch()) { cf.clearProperty(QTextFormat::FontPointSize); cf.setProperty(QTextFormat::FontPixelSize, qRound(px * 0.92)); changed = true; }
            if (changed) {
                c.setPosition(fr.position());
                c.setPosition(fr.position() + fr.length(), QTextCursor::KeepAnchor);
                c.setCharFormat(cf);
            }
        }
    }
    // Tables: thin rules, padded cells, a bold header row.
    std::function<void(QTextFrame*)> walk = [&](QTextFrame* fr) {
        for (QTextFrame* ch : fr->childFrames()) {
            if (auto* t = qobject_cast<QTextTable*>(ch)) {
                QTextTableFormat tf = t->format();
                tf.setBorder(1);
                tf.setBorderBrush(rule);
                tf.setBorderStyle(QTextFrameFormat::BorderStyle_Solid);
                tf.setBorderCollapse(true);
                tf.setCellPadding(px * 0.35);
                tf.setCellSpacing(0);
                tf.setTopMargin(px * 0.4);
                tf.setBottomMargin(px * 0.8);
                t->setFormat(tf);
                for (int col = 0; col < t->columns(); ++col) {
                    QTextTableCell cell = t->cellAt(0, col);
                    // Header text bold (the reader leaves some cells plain).
                    QTextCursor hc = cell.firstCursorPosition();
                    hc.setPosition(cell.lastCursorPosition().position(), QTextCursor::KeepAnchor);
                    QTextCharFormat bold;
                    bold.setFontWeight(QFont::Bold);
                    hc.mergeCharFormat(bold);
                }
                for (int r = 0; r < t->rows(); ++r)
                    for (int col = 0; col < t->columns(); ++col) {
                        const QTextTableCell cell = t->cellAt(r, col);
                        // The reader can leave an empty paragraph before a
                        // cell's text (it sat the first header lower).
                        while (true) {
                            const QTextBlock first = cell.firstCursorPosition().block();
                            if (first.length() > 1 || !first.next().isValid() || first.next().position() > cell.lastCursorPosition().position()) break;
                            QTextCursor d(first);
                            d.deleteChar();
                        }
                        for (QTextFrame::iterator it = cell.begin(); !it.atEnd(); ++it) {
                            QTextBlock b = it.currentBlock();
                            if (!b.isValid()) continue;
                            QTextBlockFormat bf = b.blockFormat();
                            bf.setBottomMargin(0);
                            c.setPosition(b.position());
                            c.setBlockFormat(bf);
                        }
                    }
            }
            walk(ch);
        }
    };
    walk(doc.rootFrame());
}

QString markdownHtml(const QString& md, bool dark, const QString& family, double px, const QString& baseDir, double maxWidth)
{
    QTextDocument doc;
    styleMarkdown(doc, md, dark, family, px, baseDir, maxWidth);
    return doc.toHtml();
}

// Text drawn as a sheet of paper: {"mono": bool, "lines": [[[colour, text], ...], ...]}
// or {"markdown": text}.
QImage drawPage(const QByteArray& json, int px)
{
    const QJsonObject o = QJsonDocument::fromJson(json).object();
    const bool mono = o.value(QStringLiteral("mono")).toBool();
    const QJsonArray lines = o.value(QStringLiteral("lines")).toArray();
    // A letter-shaped page, drawn big and scaled down (the text reads as
    // text at large sizes and as its shape at small ones).
    const int W = 320, H = 414, margin = 18;
    // Big previews (gallery, Quick Look) draw it at their size, not scaled up.
    const qreal k = qMax(1.0, px / qreal(H));
    QImage page(qRound(W * k), qRound(H * k), QImage::Format_ARGB32_Premultiplied);
    page.fill(Qt::white);
    {
        QPainter p(&page);
        p.scale(k, k);
        p.setRenderHint(QPainter::TextAntialiasing);
        QFont f = mono ? QFontDatabase::systemFont(QFontDatabase::FixedFont) : QFont(QStringLiteral("Sans Serif"));
        f.setPixelSize(mono ? 9 : 10);
        if (mono) f.setStyleHint(QFont::TypeWriter);
        p.setFont(f);
        if (o.contains(QStringLiteral("markdown"))) {
            // Rendered Markdown (headings, lists, code, tables), as on paper.
            QTextDocument doc;
            styleMarkdown(doc, o.value(QStringLiteral("markdown")).toString(), false, QString(), 10, QString(), W - 2 * margin);
            doc.setTextWidth(W - 2 * margin);
            p.translate(margin, margin);
            doc.drawContents(&p, QRectF(0, 0, W - 2 * margin, H - 2 * margin));
        } else {
        const QFontMetrics fm(f);
        const int lh = fm.height() + (mono ? 1 : 3);
        int y = margin + fm.ascent();
        for (const QJsonValue& lv : lines) {
            if (y > H - margin) break;
            int x = margin;
            for (const QJsonValue& rv : lv.toArray()) {
                const QJsonArray run = rv.toArray();
                const QString t = run.at(1).toString();
                p.setPen(QColor(run.at(0).toString()));
                p.drawText(x, y, t);
                x += fm.horizontalAdvance(t);
                if (x > W - margin / 2) break;
            }
            y += lh;
        }
        }
    }
    QImage out = page.scaled(px, px, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    QPainter p(&out);
    p.setPen(QColor(0, 0, 0, 50));
    p.drawRect(out.rect().adjusted(0, 0, -1, -1));
    return out;
}

// An audio waveform on a soft card.
QImage drawWave(const QString& file, int px)
{
    QImage wave(file);
    if (wave.isNull()) return {};
    QImage out(px, px, QImage::Format_ARGB32_Premultiplied);
    out.fill(Qt::transparent);
    QPainter p(&out);
    p.setRenderHint(QPainter::Antialiasing);
    QLinearGradient g(0, 0, 0, px);
    g.setColorAt(0, QColor(0xf4, 0xf6, 0xfb));
    g.setColorAt(1, QColor(0xe3, 0xe8, 0xf2));
    p.setBrush(g);
    p.setPen(QColor(0, 0, 0, 40));
    const qreal r = px * 0.12;
    p.drawRoundedRect(QRectF(0.5, 0.5, px - 1, px - 1), r, r);
    const QImage w = wave.scaled(int(px * 0.84), int(px * 0.5), Qt::IgnoreAspectRatio, Qt::SmoothTransformation);
    p.drawImage((px - w.width()) / 2, (px - w.height()) / 2, w);
    return out;
}

// Glyphs of `text` in a raw font (no font database: adding application
// fonts would make every text item in the app lay itself out again).
QGlyphRun glyphs(const QRawFont& font, const QString& text)
{
    QList<quint32> idx = font.glyphIndexesForString(text);
    QList<QPointF> adv = font.advancesForGlyphIndexes(idx);
    QList<QPointF> pos;
    QPointF at;
    for (const QPointF& a : adv) { pos << at; at += a; }
    QGlyphRun run;
    run.setRawFont(font);
    run.setGlyphIndexes(idx);
    run.setPositions(pos);
    return run;
}

// Characters a font can show: "Aa" or, for symbol fonts, its first few.
QString sampleChars(const QRawFont& f, int want)
{
    if (f.supportsCharacter(QLatin1Char('A')) && f.supportsCharacter(QLatin1Char('a'))) return want <= 2 ? QStringLiteral("Aa") : QString();
    QString s;
    for (uint c = 0x21; c < 0x30000 && s.size() < want * 2; c++) {
        if (c >= 0xd800 && c < 0xe000) continue;
        if (f.supportsCharacter(char32_t(c))) s += QString::fromUcs4(reinterpret_cast<const char32_t*>(&c), 1);
    }
    return s;
}

QImage drawFont(const QString& path, int px)
{
    QRawFont font(path, px * 0.42);
    if (!font.isValid()) return {};
    const QString text = sampleChars(font, 2);
    if (text.isEmpty()) return {};
    QImage out(px, px, QImage::Format_ARGB32_Premultiplied);
    out.fill(Qt::transparent);
    QPainter p(&out);
    p.setRenderHint(QPainter::Antialiasing);
    p.setBrush(Qt::white);
    p.setPen(QColor(0, 0, 0, 50));
    p.drawRoundedRect(QRectF(px * 0.06 + 0.5, 0.5, px * 0.88 - 1, px - 1), px * 0.06, px * 0.06);
    QGlyphRun run = glyphs(font, text);
    QRectF b = run.boundingRect();
    if (b.width() > px * 0.8) {  // too wide: shrink
        font.setPixelSize(font.pixelSize() * px * 0.8 / b.width());
        run = glyphs(font, text);
        b = run.boundingRect();
    }
    p.setPen(QColor(0x22, 0x22, 0x22));
    p.drawGlyphRun(QPointF((px - b.width()) / 2 - b.x(), (px - b.height()) / 2 - b.y()), run);
    return out;
}

// A font specimen for Quick Look: its name, alphabet, digits, and a line
// at several sizes.
QImage drawSpecimen(const QString& path, int width)
{
    QRawFont font(path, 20);
    if (!font.isValid()) return {};
    const int W = qBound(400, width, 2400);
    const qreal k = W / 1000.0;
    QImage out(W, int(W * 0.72), QImage::Format_ARGB32_Premultiplied);
    out.fill(Qt::white);
    QPainter p(&out);
    p.setRenderHint(QPainter::Antialiasing);
    p.setPen(QColor(0x1d, 0x1d, 0x1f));
    const bool latin = font.supportsCharacter(QLatin1Char('a'));
    QStringList lines;
    if (latin) lines << QStringLiteral("ABCDEFGHIJKLMNOPQRSTUVWXYZ") << QStringLiteral("abcdefghijklmnopqrstuvwxyz") << QStringLiteral("0123456789 .,:;!?&@#%()");
    else lines << sampleChars(font, 24);
    qreal y = 40 * k;
    auto line = [&](const QString& t, qreal size) {
        font.setPixelSize(size);
        const QGlyphRun run = glyphs(font, t);
        y += font.ascent();
        p.drawGlyphRun(QPointF(40 * k, y), run);
        y += font.descent() + size * 0.35;
    };
    line(font.familyName() + (font.styleName().isEmpty() ? QString() : QStringLiteral(" ") + font.styleName()), 44 * k);
    y += 10 * k;
    for (const QString& l : lines) line(l, 34 * k);
    y += 14 * k;
    const QString pangram = latin ? QStringLiteral("The quick brown fox jumps over the lazy dog") : sampleChars(font, 16);
    for (qreal s : { 14.0, 20.0, 28.0, 40.0, 56.0 }) {
        if (y + s * k * 1.4 > out.height()) break;
        line(pangram, s * k);
    }
    return out;
}

// ---- 3D models (STL, OBJ): flat-shaded, seen from above and in front ----

bool loadStl(const QString& path, QList<QVector3D>& tris)
{
    QFile f(path);
    if (!f.open(QIODevice::ReadOnly) || f.size() > 400 * 1024 * 1024) return false;
    const QByteArray d = f.readAll();
    if (d.size() >= 84) {
        quint32 n;
        memcpy(&n, d.constData() + 80, 4);
        if (qint64(84) + qint64(n) * 50 == d.size()) {
            tris.reserve(int(n) * 3);
            for (quint32 i = 0; i < n; i++) {
                const char* t = d.constData() + 84 + i * 50 + 12;
                for (int v = 0; v < 3; v++) {
                    float xyz[3];
                    memcpy(xyz, t + v * 12, 12);
                    tris << QVector3D(xyz[0], xyz[2], -xyz[1]);  // Z up -> Y up
                }
            }
            return !tris.isEmpty();
        }
    }
    // ASCII: "vertex x y z" lines.
    for (const QByteArray& raw : d.split('\n')) {
        const QByteArray l = raw.trimmed();
        if (!l.startsWith("vertex")) continue;
        const QList<QByteArray> p = l.simplified().split(' ');
        if (p.size() >= 4) tris << QVector3D(p[1].toFloat(), p[3].toFloat(), -p[2].toFloat());
    }
    tris.resize(tris.size() / 3 * 3);
    return !tris.isEmpty();
}

bool loadObj(const QString& path, QList<QVector3D>& tris)
{
    QFile f(path);
    if (!f.open(QIODevice::ReadOnly) || f.size() > 400 * 1024 * 1024) return false;
    QList<QVector3D> v;
    while (!f.atEnd()) {
        const QByteArray l = f.readLine().simplified();
        if (l.startsWith("v ")) {
            const QList<QByteArray> p = l.split(' ');
            if (p.size() >= 4) v << QVector3D(p[1].toFloat(), p[2].toFloat(), p[3].toFloat());
        } else if (l.startsWith("f ")) {
            const QList<QByteArray> p = l.split(' ');
            QList<int> idx;
            for (int i = 1; i < p.size(); i++) {
                int k = p[i].split('/').first().toInt();
                if (k < 0) k = int(v.size()) + k + 1;
                if (k >= 1 && k <= v.size()) idx << k - 1;
            }
            for (int i = 1; i + 1 < idx.size(); i++) tris << v[idx[0]] << v[idx[i]] << v[idx[i + 1]];
        }
    }
    return !tris.isEmpty();
}

QImage drawModel(const QString& path, int px, const std::atomic<bool>& cancelled)
{
    QList<QVector3D> t;
    if (!(path.endsWith(QLatin1String(".obj"), Qt::CaseInsensitive) ? loadObj(path, t) : loadStl(path, t))) return {};
    QVector3D lo = t[0], hi = t[0];
    for (const QVector3D& p : t) {
        lo = QVector3D(qMin(lo.x(), p.x()), qMin(lo.y(), p.y()), qMin(lo.z(), p.z()));
        hi = QVector3D(qMax(hi.x(), p.x()), qMax(hi.y(), p.y()), qMax(hi.z(), p.z()));
    }
    const QVector3D c = (lo + hi) / 2;
    QMatrix4x4 m;
    m.rotate(-28, 1, 0, 0);
    m.rotate(35, 0, 1, 0);
    m.translate(-c);
    struct Face { QPointF p[3]; float z; float light; };
    QList<Face> faces;
    faces.reserve(t.size() / 3);
    float minx = 1e30f, maxx = -1e30f, miny = 1e30f, maxy = -1e30f;
    const QVector3D L = QVector3D(-0.4f, 0.7f, 0.6f).normalized();
    for (qsizetype i = 0; i + 2 < t.size(); i += 3) {
        const QVector3D a = m.map(t[i]), b = m.map(t[i + 1]), d = m.map(t[i + 2]);
        QVector3D n = QVector3D::crossProduct(b - a, d - a);
        if (n.isNull()) continue;
        n.normalize();
        Face f { { QPointF(a.x(), -a.y()), QPointF(b.x(), -b.y()), QPointF(d.x(), -d.y()) }, (a.z() + b.z() + d.z()) / 3, 0.28f + 0.72f * qAbs(QVector3D::dotProduct(n, L)) };
        for (const QPointF& q : f.p) {
            minx = qMin(minx, float(q.x())); maxx = qMax(maxx, float(q.x()));
            miny = qMin(miny, float(q.y())); maxy = qMax(maxy, float(q.y()));
        }
        faces << f;
        if ((i & 0xffff) == 0 && (cancelled || g_quitting)) return {};
    }
    if (faces.isEmpty()) return {};
    std::sort(faces.begin(), faces.end(), [](const Face& x, const Face& y) { return x.z < y.z; });
    const int S = px * 2;  // drawn big, scaled down (smooth edges without seams)
    const float span = qMax(maxx - minx, maxy - miny);
    const float k = span > 0 ? S * 0.86f / span : 1;
    QImage img(S, S, QImage::Format_ARGB32_Premultiplied);
    img.fill(Qt::transparent);
    QPainter p(&img);
    p.translate(S / 2.0 - (minx + maxx) / 2 * k, S / 2.0 - (miny + maxy) / 2 * k);
    p.scale(k, k);
    p.setPen(Qt::NoPen);
    const QColor base(0x7d, 0xa7, 0xd9);
    for (qsizetype i = 0; i < faces.size(); i++) {
        const Face& f = faces[i];
        p.setBrush(QColor::fromRgbF(base.redF() * f.light, base.greenF() * f.light, base.blueF() * f.light));
        p.drawPolygon(f.p, 3);
        if ((i & 0xffff) == 0 && (cancelled || g_quitting)) return {};
    }
    p.end();
    return img.scaled(px, px, Qt::KeepAspectRatio, Qt::SmoothTransformation);
}

QImage drawPdfWithQt(const QString& path, int px)
{
    QPdfDocument doc;
    if (doc.load(path) != QPdfDocument::Error::None || doc.pageCount() < 1) return {};
    const QSize s = doc.pagePointSize(0).toSize().scaled(px, px, Qt::KeepAspectRatio);
    const QImage page = doc.render(0, s);
    if (page.isNull()) return {};
    QImage img(page.size(), QImage::Format_ARGB32_Premultiplied);
    img.fill(Qt::white);
    QPainter p(&img);
    p.drawImage(0, 0, page);
    return img;
}

QStringList mimeChain(const QString& path, QString* name)
{
    static thread_local QMimeDatabase db;
    const QMimeType mt = db.mimeTypeForFile(path);
    *name = mt.name();
    QStringList mimes { mt.name() };
    for (const QString& p : mt.allAncestors()) mimes << p;
    return mimes;
}

class ThumbResponse : public JobResponse {
public:
    ThumbResponse(const QString& path, const QSize& req) : m_path(path), m_req(req) { m_what = path; }

protected:
    bool work() override
    {
        const int px = qMax(16, qMax(m_req.width(), m_req.height()));
        const rust::String rp = toRust(m_path);
        if (m_stage == 0 && m_path.startsWith(QLatin1String("archive://"))) {
            // A file inside an archive: preview its local copy, taken out
            // (once, then reused) on a tools thread.
            const QString local = fromRust(grove_archive_local(rp, false));
            if (local.isEmpty()) { m_stage = 2; startOn(pools().tools); return true; }
            m_path = local;
            return work();
        }
        if (m_stage == 2) {
            const QString local = fromRust(grove_archive_local(rp, true));
            if (local.isEmpty() || m_cancelled) return false;
            m_path = local;
            m_stage = 0;
            return work();
        }
        if (m_stage == 0) {
            // The fast part: an up-to-date thumbnail in the cache.
            const QString cached = fromRust(grove_thumb_lookup(rp, uint32_t(px)));
            if (!cached.isEmpty() && m_img.load(cached)) return false;
            if (grove_thumb_failed(rp)) return false;
            m_mimes = mimeChain(m_path, &m_mime);
            m_kind = fromRust(grove_thumb_kind(rp, toRust(m_mimes.join(QLatin1Char('\n')))));
            m_qtImage = m_kind == QLatin1String("picture") && QImageReader(m_path).canRead();
            const bool inProcess = m_qtImage || m_kind == QLatin1String("text") || m_kind == QLatin1String("font") || m_kind == QLatin1String("model");
            m_stage = 1;
            m_what = m_kind + QLatin1Char(' ') + m_path;
            startOn(inProcess ? pools().make : pools().tools);
            return true;
        }
        // Test hook: slow every preview down (to see the spinners).
        static const int testDelay = qEnvironmentVariableIntValue("GROVE_THUMB_DELAY");
        for (int t = 0; t < testDelay && !m_cancelled && !g_quitting; t += 20) QThread::msleep(20);
        make(px, rp);
        if (!m_img.isNull()) return false;
        if (!m_cancelled) grove_thumb_mark_failed(rp);
        return false;
    }

private:
    void keep(QImage img, uint32_t flavor)
    {
        if (img.isNull() || m_cancelled) return;
        if (img.width() > int(flavor) || img.height() > int(flavor))
            img = img.scaled(int(flavor), int(flavor), Qt::KeepAspectRatio, Qt::SmoothTransformation);
        // Saved with the spec's keys (Rust writes the PNG text chunks).
        QByteArray png;
        {
            QBuffer buf(&png);
            buf.open(QIODevice::WriteOnly);
            img.save(&buf, "PNG");
        }
        rust::Slice<const uint8_t> bytes(reinterpret_cast<const uint8_t*>(png.constData()), size_t(png.size()));
        grove_thumb_store(toRust(m_path), flavor, bytes, toRust(m_mime));
        m_img = img;
    }

    void make(int px, const rust::String& rp)
    {
        const uint32_t flavor = grove_thumb_flavor(uint32_t(px));
        const int f = int(flavor);
        if (m_qtImage) { keep(readScaled(m_path, f), flavor); if (!m_img.isNull()) return; }
        if (m_kind == QLatin1String("font")) { keep(drawFont(m_path, f), flavor); return; }
        if (m_kind == QLatin1String("model")) { keep(drawModel(m_path, f, m_cancelled), flavor); return; }
        const QString r = fromRust(grove_thumb_make(rp, toRust(m_mimes.join(QLatin1Char('\n'))), flavor, m_token));
        if (m_cancelled) { cleanup(r); return; }
        if (r.startsWith(QLatin1String("thumb:"))) m_img.load(r.mid(6));
        else if (r.startsWith(QLatin1String("image:"))) keep(readScaled(r.mid(6), f), flavor);
        else if (r.startsWith(QLatin1String("wave:"))) keep(drawWave(r.mid(5), f), flavor);
        else if (r.startsWith(QLatin1String("page:"))) keep(drawPage(r.mid(5).toUtf8(), f), flavor);
        else if (r == QLatin1String("qtpdf")) keep(drawPdfWithQt(m_path, f), flavor);
        cleanup(r);
    }

    static void cleanup(const QString& r)
    {
        if (r.startsWith(QLatin1String("image:"))) QFile::remove(r.mid(6));
        else if (r.startsWith(QLatin1String("wave:"))) QFile::remove(r.mid(5));
    }

    QString m_path;
    QSize m_req;
    int m_stage = 0;
    QString m_mime, m_kind;
    QStringList m_mimes;
    bool m_qtImage = false;
};

class ThumbProvider : public QQuickAsyncImageProvider {
public:
    QQuickImageResponse* requestImageResponse(const QString& id, const QSize& req) override
    {
        // id: <percent-encoded path>?m=<mtime> (the mtime only busts caches).
        auto* r = new ThumbResponse(takePath(id), req.isValid() ? req : QSize(128, 128));
        r->startOn(pools().lookup);
        return r;
    }
};

// image://pdfpage/<path>?p=<page>&m=<mtime>: one page at the requested width.
class PdfPageResponse : public JobResponse {
public:
    PdfPageResponse(const QString& path, int page, int width) : m_path(path), m_page(page), m_width(width) { m_what = QStringLiteral("pdf page %1 ").arg(page) + path; }

protected:
    bool work() override
    {
        const QString out = fromRust(grove_pdf_page(toRust(m_path), uint32_t(m_page), uint32_t(m_width), m_token));
        if (!out.isEmpty()) {
            m_img.load(out);
            QFile::remove(out);
            return false;
        }
        if (m_cancelled) return false;
        // No pdftoppm/mutool: QtPdf.
        QPdfDocument doc;
        if (doc.load(m_path) == QPdfDocument::Error::None && m_page >= 1 && m_page <= doc.pageCount()) {
            const QSizeF pt = doc.pagePointSize(m_page - 1);
            const QSize s(m_width, int(m_width * pt.height() / qMax(1.0, pt.width())));
            QImage page = doc.render(m_page - 1, s);
            if (!page.isNull()) {
                m_img = QImage(page.size(), QImage::Format_ARGB32_Premultiplied);
                m_img.fill(Qt::white);
                QPainter p(&m_img);
                p.drawImage(0, 0, page);
            }
        }
        return false;
    }

private:
    QString m_path;
    int m_page, m_width;
};

class PdfPageProvider : public QQuickAsyncImageProvider {
public:
    QQuickImageResponse* requestImageResponse(const QString& id, const QSize& req) override
    {
        const int q = id.lastIndexOf(QLatin1Char('?'));
        const QUrlQuery query(q >= 0 ? id.mid(q + 1) : QString());
        auto* r = new PdfPageResponse(takePath(id), qMax(1, query.queryItemValue(QStringLiteral("p")).toInt()), req.width() > 0 ? req.width() : 800);
        r->startOn(pools().pages);
        return r;
    }
};

// image://specimen/<path>: a font specimen at the requested width.
class SpecimenResponse : public JobResponse {
public:
    SpecimenResponse(const QString& path, int width) : m_path(path), m_width(width) { m_what = QStringLiteral("specimen ") + path; }

protected:
    bool work() override { m_img = drawSpecimen(m_path, m_width); return false; }

private:
    QString m_path;
    int m_width;
};

class SpecimenProvider : public QQuickAsyncImageProvider {
public:
    QQuickImageResponse* requestImageResponse(const QString& id, const QSize& req) override
    {
        auto* r = new SpecimenResponse(takePath(id), req.width() > 0 ? req.width() : 1000);
        r->startOn(pools().make);
        return r;
    }
};

// image://treemap/<scan>/<node>/<apparent 0|1>/<rev>: the Disk Usage
// window's cushion treemap at the requested size (drawn by Rust).
class TreemapResponse : public JobResponse {
public:
    TreemapResponse(int scan, uint32_t node, bool apparent, const QSize& size) : m_scan(scan), m_node(node), m_apparent(apparent), m_size(size) { m_what = QStringLiteral("treemap"); }

protected:
    bool work() override
    {
        const int w = qMax(1, m_size.width()), h = qMax(1, m_size.height());
        const rust::Vec<uint8_t> px = grove_du_treemap(m_scan, m_node, uint32_t(w), uint32_t(h), m_apparent);
        if (px.size() == size_t(w) * size_t(h) * 4)
            m_img = QImage(px.data(), w, h, w * 4, QImage::Format_RGBA8888).copy();
        return false;
    }

private:
    int m_scan;
    uint32_t m_node;
    bool m_apparent;
    QSize m_size;
};

class TreemapProvider : public QQuickAsyncImageProvider {
public:
    QQuickImageResponse* requestImageResponse(const QString& id, const QSize& req) override
    {
        const QStringList p = id.split(QLatin1Char('/'));
        auto* r = new TreemapResponse(p.value(0).toInt(), p.value(1).toUInt(), p.value(2) == QLatin1String("1"), req.isValid() ? req : QSize(800, 400));
        r->startOn(pools().make);
        return r;
    }
};

}  // namespace

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
        bool exact = false;
        // ?r=<rev> busts caches; &tint=rrggbb recolours; &exact=1 draws the
        // theme's own art at exactly the size asked (GIcon's crisp menus).
        const int q = key.lastIndexOf(QLatin1Char('?'));
        if (q >= 0) {
            const QUrlQuery query(key.mid(q + 1));
            if (query.hasQueryItem(QStringLiteral("tint")))
                tint = QColor(QLatin1Char('#') + query.queryItemValue(QStringLiteral("tint")));
            exact = query.hasQueryItem(QStringLiteral("exact"));
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
        if (exact) {
            // A design size of the theme (16/22/24/32 px): drawn 1:1.
            pm = icon.pixmap(QSize(s, s), 1.0).toImage();
        } else if (fileIcon && !tint.isValid() && s < 32) {
            pm = icon.pixmap(QSize(32, 32), 1.0).toImage();
            if (!pm.isNull() && pm.width() > s)
                pm = pm.scaled(s, s, Qt::KeepAspectRatio, Qt::SmoothTransformation);
        } else if (!fileIcon && s < 64) {
            // Line-art glyphs (menus, toolbars): drawn at twice the size and
            // shrunk here with proper filtering to the exact pixels shown, so
            // hairline strokes at a fractional scale come out even instead of
            // patchy (rows went missing at 4/3).
            pm = icon.pixmap(QSize(s * 2, s * 2), 1.0).toImage();
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
    QGuiApplication::setApplicationDisplayName(QStringLiteral("Grove"));
    QGuiApplication::setApplicationVersion(QStringLiteral(GROVE_VERSION));
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
    qmlRegisterType<GlobalMenuBar>("GlobalMenu", 1, 0, "GlobalMenuBar");
    auto* engine = new QQmlApplicationEngine;
    engine->addImageProvider(QStringLiteral("thumb"), new ThumbProvider);
    engine->addImageProvider(QStringLiteral("pdfpage"), new PdfPageProvider);
    engine->addImageProvider(QStringLiteral("specimen"), new SpecimenProvider);
    engine->addImageProvider(QStringLiteral("treemap"), new TreemapProvider);
    engine->addImageProvider(QStringLiteral("icon"), new IconProvider);
    if (!importPath.isEmpty()) engine->addImportPath(importPath);
    engine->load(QUrl(url));
    if (engine->rootObjects().isEmpty()) return 1;
    const int code = QGuiApplication::exec();
    // Stop preview work (killing its tools) before tearing down, so no
    // worker is still drawing while the app goes away.
    g_quitting = true;
    grove_thumb_cancel_all();
    for (QThreadPool* p : { &pools().lookup, &pools().make, &pools().tools, &pools().pages }) p->clear();
    for (QThreadPool* p : { &pools().lookup, &pools().make, &pools().tools, &pools().pages }) p->waitForDone(3000);
    return code;
}

QString grove_mime_info(const QString& name, const QString& path, bool deep)
{
    // A listing thread still running while grove quits: Qt's MIME database
    // is gone with the application (a crash on quit).
    if (g_quitting || !QCoreApplication::instance())
        return QStringLiteral("application/octet-stream\napplication-octet-stream\nunknown\n");
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

QString grove_markdown_html(const QString& md, bool dark, const QString& family, double px, const QString& baseDir, double maxWidth)
{
    return markdownHtml(md, dark, family, px, baseDir, maxWidth);
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
void grove_test_touchpad(double x, double y, int32_t ax, int32_t ay, int32_t px, int32_t py, int32_t phase)
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
                   Qt::ScrollPhase(phase), false, Qt::MouseEventSynthesizedBySystem, pad);
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
