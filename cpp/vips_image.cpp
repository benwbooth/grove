#include "vips_image.h"

#include <QtCore/QFileDevice>
#include <QtCore/QVariant>
#include <QtGui/QImage>
#include <mutex>

// libvips, declared here (Rust links it: core/src/vips.rs).
extern "C" {
typedef struct _VipsImage VipsImage;
int vips_init(const char* argv0);
int vips_thumbnail(const char* filename, VipsImage** out, int width, ...);
int vips_thumbnail_buffer(void* buf, size_t len, VipsImage** out, int width, ...);
VipsImage* vips_image_new_from_file(const char* name, ...);
VipsImage* vips_image_new_from_buffer(const void* buf, size_t len, const char* option_string, ...);
int vips_colourspace(VipsImage* in, VipsImage** out, int space, ...);
int vips_cast_uchar(VipsImage* in, VipsImage** out, ...);
void* vips_image_write_to_memory(VipsImage* in, size_t* size);
int vips_image_get_width(const VipsImage* image);
int vips_image_get_height(const VipsImage* image);
int vips_image_get_bands(const VipsImage* image);
int vips_image_get_orientation_swap(VipsImage* image);
void vips_error_clear(void);
void g_object_unref(void* object);
void g_free(void* mem);
}

namespace {
const int SIZE_DOWN = 2;           // VIPS_SIZE_DOWN
const int INTERPRETATION_SRGB = 22; // VIPS_INTERPRETATION_sRGB

bool ready()
{
    static std::once_flag once;
    static bool ok = false;
    std::call_once(once, [] { ok = vips_init("grove") == 0; });
    return ok;
}

struct Unref {
    VipsImage* p = nullptr;
    ~Unref() { if (p) g_object_unref(p); }
};

// The file behind the device, when it is one (vips reads it itself then,
// only as much as it needs).
QString fileOf(QIODevice* d)
{
    auto* f = qobject_cast<QFileDevice*>(d);
    return f ? f->fileName() : QString();
}

class Handler : public QImageIOHandler {
public:
    bool canRead() const override
    {
        if (!device() || !grove_vips_sniff(device()->peek(64))) return false;
        setFormat("heic");
        return true;
    }

    bool supportsOption(ImageOption o) const override { return o == Size || o == ScaledSize; }
    void setOption(ImageOption o, const QVariant& v) override { if (o == ScaledSize) m_scaled = v.toSize(); }

    QVariant option(ImageOption o) const override
    {
        if (o == ScaledSize) return m_scaled;
        if (o != Size || !ready()) return {};
        // The header only (vips opens lazily), upright.
        const QString file = fileOf(device());
        Unref img;
        QByteArray data;
        if (!file.isEmpty()) {
            img.p = vips_image_new_from_file(file.toLocal8Bit().constData(), nullptr);
        } else {
            const qint64 at = device()->pos();
            data = device()->readAll();
            device()->seek(at);
            img.p = vips_image_new_from_buffer(data.constData(), size_t(data.size()), "", nullptr);
        }
        if (!img.p) { vips_error_clear(); return {}; }
        QSize s(vips_image_get_width(img.p), vips_image_get_height(img.p));
        if (vips_image_get_orientation_swap(img.p)) s.transpose();
        return s;
    }

    bool read(QImage* out) override
    {
        if (!ready()) return false;
        // At the size asked for, else whole; vips's thumbnail turns it upright
        // (and for HEIC uses libheif's own scaling).
        const int w = m_scaled.width() > 0 ? m_scaled.width() : 16384;
        const int h = m_scaled.height() > 0 ? m_scaled.height() : 16384;
        const QString file = fileOf(device());
        Unref img, rgb, bytes;
        int r;
        QByteArray data;
        if (!file.isEmpty()) {
            r = vips_thumbnail(file.toLocal8Bit().constData(), &img.p, w, "height", h, "size", SIZE_DOWN, nullptr);
            device()->seek(device()->size());
        } else {
            data = device()->readAll();
            r = vips_thumbnail_buffer(data.data(), size_t(data.size()), &img.p, w, "height", h, "size", SIZE_DOWN, nullptr);
        }
        if (r != 0 || !img.p) { vips_error_clear(); return false; }
        // 8-bit sRGB (HDR and grey ones too), with its alpha if it has one.
        if (vips_colourspace(img.p, &rgb.p, INTERPRETATION_SRGB, nullptr) != 0 || vips_cast_uchar(rgb.p, &bytes.p, nullptr) != 0) {
            vips_error_clear();
            return false;
        }
        const int bands = vips_image_get_bands(bytes.p);
        if (bands != 3 && bands != 4) return false;
        const int iw = vips_image_get_width(bytes.p), ih = vips_image_get_height(bytes.p);
        size_t len = 0;
        void* px = vips_image_write_to_memory(bytes.p, &len);
        if (!px) { vips_error_clear(); return false; }
        if (len < size_t(iw) * ih * bands) { g_free(px); return false; }
        *out = QImage(static_cast<const uchar*>(px), iw, ih, iw * bands, bands == 4 ? QImage::Format_RGBA8888 : QImage::Format_RGB888).copy();
        g_free(px);
        return !out->isNull();
    }

private:
    QSize m_scaled;
};
} // namespace

bool grove_vips_sniff(const QByteArray& head)
{
    // JPEG XL: a bare codestream or its ISO box.
    if (head.startsWith("\xFF\x0A") || head.startsWith(QByteArray("\x00\x00\x00\x0CJXL \x0D\x0A\x87\x0A", 12))) return true;
    // HEIF/AVIF: an ISO "ftyp" box whose brands name one.
    if (head.size() < 16 || head.mid(4, 4) != "ftyp") return false;
    const quint32 box = (quint8(head[0]) << 24) | (quint8(head[1]) << 16) | (quint8(head[2]) << 8) | quint8(head[3]);
    const int end = int(qMin<quint32>(box, quint32(head.size())));
    static const char* brands[] = { "heic", "heix", "hevc", "hevx", "heim", "heis", "hevm", "hevs", "mif1", "msf1", "avif", "avis" };
    for (int i = 8; i + 4 <= end; i += 4) {
        if (i == 12) continue; // (the minor version)
        const QByteArray b = head.mid(i, 4);
        for (const char* k : brands)
            if (b == k) return true;
    }
    return false;
}

QImageIOPlugin::Capabilities VipsImagePlugin::capabilities(QIODevice* device, const QByteArray& format) const
{
    if (format == "heic" || format == "heif" || format == "avif" || format == "jxl") return CanRead;
    if (!format.isEmpty() || !device || !device->isReadable()) return {};
    return grove_vips_sniff(device->peek(64)) ? CanRead : Capabilities();
}

QImageIOHandler* VipsImagePlugin::create(QIODevice* device, const QByteArray& format) const
{
    auto* h = new Handler;
    h->setDevice(device);
    h->setFormat(format.isEmpty() ? QByteArray("heic") : format);
    return h;
}
