#ifndef VIPS_IMAGE_H
#define VIPS_IMAGE_H

// (A plugin built into grove itself: moc must export it as a static one.)
#ifndef QT_STATICPLUGIN
#define QT_STATICPLUGIN
#endif

#include <QtGui/QImageIOPlugin>

// HEIC/HEIF (iPhone photos), AVIF and JPEG XL read straight into a QImage by
// libvips (libheif, libjxl), so every Image, thumbnail and QImageReader in
// grove opens them like a JPEG: no converted copy on disk. Decoded at the
// size asked for (an Image's sourceSize), turned upright.
class VipsImagePlugin : public QImageIOPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID "org.qt-project.Qt.QImageIOHandlerFactoryInterface" FILE "vips_image.json")

public:
    Capabilities capabilities(QIODevice* device, const QByteArray& format) const override;
    QImageIOHandler* create(QIODevice* device, const QByteArray& format = QByteArray()) const override;
};

// The header bytes are a picture this plugin reads.
bool grove_vips_sniff(const QByteArray& head);

#endif
