#ifndef GROVE_MPV_GL_H
#define GROVE_MPV_GL_H

// Draws Grove Player's picture: mpv's OpenGL render API in a Qt Quick item.
// The only C++ in the player (cxx-qt can't subclass QQuickFramebufferObject's
// Renderer, which isn't a QObject); mpv itself is driven from Rust
// (src/mpv.rs, MpvPlayer), which hands over its handle:
//     MpvPlayer { id: mp }
//     MpvGlView { anchors.fill: parent; handle: mp.handle }
// The view takes its own client of that mpv core, so the core stays alive
// until the render context (freed on Qt's render thread) is gone.

#include <QQuickFramebufferObject>
#include <memory>

struct mpv_handle;

class MpvGlView : public QQuickFramebufferObject {
    Q_OBJECT
    Q_PROPERTY(quint64 handle READ handle WRITE setHandle NOTIFY handleChanged)
    // mpv's render context exists: load files only now (mpv turns video
    // off for a file loaded before it).
    Q_PROPERTY(bool ready READ ready NOTIFY readyChanged)

public:
    explicit MpvGlView(QQuickItem *parent = nullptr);
    Renderer *createRenderer() const override;
    quint64 handle() const { return m_handle; }
    void setHandle(quint64 h);
    std::shared_ptr<mpv_handle> client() const { return m_client; }
    bool ready() const { return m_ready; }
    Q_INVOKABLE void markReady() { if (!m_ready) { m_ready = true; emit readyChanged(); } }

signals:
    void handleChanged();
    void readyChanged();

private:
    quint64 m_handle = 0;
    bool m_ready = false;
    std::shared_ptr<mpv_handle> m_client;
};

#endif
