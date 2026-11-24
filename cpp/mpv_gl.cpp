// Grove Player's picture: mpv's OpenGL render API (see include/mpv_gl.h).

#include "mpv_gl.h"

#include <QGuiApplication>
#include <QOpenGLContext>
#include <QPointer>
#include <QOpenGLFramebufferObject>
#include <QQuickOpenGLUtils>
#include <QQuickWindow>
#include <QtGui/qguiapplication_platform.h>
#include <thread>

#include <mpv/client.h>
#include <mpv/render_gl.h>

namespace {

void *procAddress(void *, const char *name) {
    QOpenGLContext *c = QOpenGLContext::currentContext();
    return c ? reinterpret_cast<void *>(c->getProcAddress(QByteArray(name))) : nullptr;
}

// Lives on the render thread and owns mpv's render context.
class MpvGlRenderer : public QQuickFramebufferObject::Renderer {
public:
    explicit MpvGlRenderer(const MpvGlView *item) : m_view(const_cast<MpvGlView *>(item)) {}
    ~MpvGlRenderer() override {
        // Before this client (and so possibly the core) goes.
        if (m_ctx) mpv_render_context_free(m_ctx);
    }

    // The GUI thread waits meanwhile: take the view's client once it has one.
    void synchronize(QQuickFramebufferObject *item) override {
        if (!m_mpv) m_mpv = static_cast<MpvGlView *>(item)->client();
    }

    void render() override {
        if (!m_ctx && m_mpv) init();
        if (!m_ctx) return;
        QOpenGLFramebufferObject *fbo = framebufferObject();
        mpv_opengl_fbo f{static_cast<int>(fbo->handle()), fbo->width(), fbo->height(), 0};
        int flip = 0;
        mpv_render_param params[] = {
            {MPV_RENDER_PARAM_OPENGL_FBO, &f},
            {MPV_RENDER_PARAM_FLIP_Y, &flip},
            {MPV_RENDER_PARAM_INVALID, nullptr},
        };
        mpv_render_context_render(m_ctx, params);
        QQuickOpenGLUtils::resetOpenGLState();
    }

private:
    void init() {
        mpv_opengl_init_params gl{procAddress, nullptr};
        QList<mpv_render_param> params = {
            {MPV_RENDER_PARAM_API_TYPE, const_cast<char *>(MPV_RENDER_API_TYPE_OPENGL)},
            {MPV_RENDER_PARAM_OPENGL_INIT_PARAMS, &gl},
        };
        // The display connection, for zero-copy hardware decoding (VA-API).
        if (auto *wl = qGuiApp->nativeInterface<QNativeInterface::QWaylandApplication>()) {
            if (wl->display()) params << mpv_render_param{MPV_RENDER_PARAM_WL_DISPLAY, wl->display()};
        } else if (auto *x = qGuiApp->nativeInterface<QNativeInterface::QX11Application>()) {
            if (x->display()) params << mpv_render_param{MPV_RENDER_PARAM_X11_DISPLAY, x->display()};
        }
        params << mpv_render_param{MPV_RENDER_PARAM_INVALID, nullptr};
        const int err = mpv_render_context_create(&m_ctx, m_mpv.get(), params.data());
        if (err < 0) {
            qWarning("Grove Player: mpv's OpenGL render context failed: %s", mpv_error_string(err));
            m_ctx = nullptr;
            return;
        }
        QPointer<QQuickItem> view = m_view;
        QMetaObject::invokeMethod(qApp, [view] { if (view) static_cast<MpvGlView *>(view.data())->markReady(); }, Qt::QueuedConnection);
        // A new frame (called on mpv's threads): draw again.
        // The view may be gone by then (it dies before this renderer): the
        // GUI thread checks.
        mpv_render_context_set_update_callback(m_ctx, [](void *ctx) {
            QPointer<QQuickItem> view = static_cast<MpvGlRenderer *>(ctx)->m_view;
            QMetaObject::invokeMethod(qApp, [view] { if (view) view->update(); }, Qt::QueuedConnection);
        }, this);
    }

    QPointer<QQuickItem> m_view;
    std::shared_ptr<mpv_handle> m_mpv;
    mpv_render_context *m_ctx = nullptr;
};

} // namespace

MpvGlView::MpvGlView(QQuickItem *parent) : QQuickFramebufferObject(parent) {}

void MpvGlView::setHandle(quint64 h) {
    if (h == m_handle || m_client) return;
    m_handle = h;
    if (h) {
        // Its own client: mpv_destroy waits for nothing here, but the last
        // one out may tear the core down, so off the UI thread.
        mpv_handle *c = mpv_create_client(reinterpret_cast<mpv_handle *>(h), "grove-gl");
        if (c) m_client = std::shared_ptr<mpv_handle>(c, [](mpv_handle *p) { std::thread([p] { mpv_destroy(p); }).detach(); });
        update();
    }
    emit handleChanged();
}

QQuickFramebufferObject::Renderer *MpvGlView::createRenderer() const {
    if (window()) window()->setPersistentGraphics(true);
    return new MpvGlRenderer(this);
}
