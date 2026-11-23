/* Documents through linked libraries, no mutool/pdftoppm/pdfinfo/ddjvu/
 * ps2pdf commands. MuPDF reports errors with fz_try/fz_catch (setjmp), so
 * every call into it is wrapped here, in C; Rust calls these functions.
 *
 *   grove_doc_info    page count, page sizes (points) and title of a PDF,
 *                     EPUB, XPS, CBZ, FB2, MOBI... (MuPDF)
 *   grove_doc_render  one page as a PNG
 *   grove_doc_to_pdf  every page into a PDF (MuPDF's PDF writer)
 *   grove_djvu_to_pdf DjVu pages (djvulibre) drawn into a PDF (MuPDF)
 *   grove_ps_to_pdf   PostScript/EPS to PDF (Ghostscript's pdfwrite; libgs
 *                     is loaded privately with dlopen: linked into the
 *                     process, its exported names (file_close...) replaced
 *                     other libraries' own and crashed libvips' FITS reader)
 *
 * Each returns 0, or -1 with a message in err. */

#include <mupdf/fitz.h>
#include <libdjvu/ddjvuapi.h>
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void set_err(char *err, int len, const char *msg) {
    if (err && len > 0) {
        snprintf(err, (size_t)len, "%s", msg ? msg : "unknown error");
    }
}

static fz_context *new_ctx(void) {
    fz_context *ctx = fz_new_context(NULL, NULL, FZ_STORE_DEFAULT);
    if (!ctx) return NULL;
    fz_try(ctx) fz_register_document_handlers(ctx);
    fz_catch(ctx) { fz_drop_context(ctx); return NULL; }
    /* Quiet: warnings went to stderr. */
    fz_set_warning_callback(ctx, NULL, NULL);
    fz_set_error_callback(ctx, NULL, NULL);
    return ctx;
}

int grove_doc_info(const char *path, int max_sizes, int *pages, float *sizes, char *title, int title_len, char *err, int errlen) {
    fz_context *ctx = new_ctx();
    fz_document *doc = NULL;
    int rc = 0;
    if (!ctx) { set_err(err, errlen, "Couldn't start MuPDF"); return -1; }
    *pages = 0;
    if (title && title_len > 0) title[0] = 0;
    fz_var(doc);
    fz_try(ctx) {
        doc = fz_open_document(ctx, path);
        if (fz_needs_password(ctx, doc)) fz_throw(ctx, FZ_ERROR_GENERIC, "This document is password-protected.");
        int n = fz_count_pages(ctx, doc);
        *pages = n;
        for (int i = 0; i < n && i < max_sizes; i++) {
            fz_page *page = fz_load_page(ctx, doc, i);
            fz_rect r = fz_bound_page(ctx, page);
            sizes[2 * i] = r.x1 - r.x0;
            sizes[2 * i + 1] = r.y1 - r.y0;
            fz_drop_page(ctx, page);
        }
        if (title && title_len > 0) fz_lookup_metadata(ctx, doc, FZ_META_INFO_TITLE, title, title_len);
    }
    fz_always(ctx) fz_drop_document(ctx, doc);
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); rc = -1; }
    fz_drop_context(ctx);
    return rc;
}

int grove_doc_render(const char *path, int page_no, int px, int width_only, const char *out_png, char *err, int errlen) {
    fz_context *ctx = new_ctx();
    fz_document *doc = NULL;
    fz_page *page = NULL;
    fz_pixmap *pix = NULL;
    int rc = 0;
    if (!ctx) { set_err(err, errlen, "Couldn't start MuPDF"); return -1; }
    fz_var(doc);
    fz_var(page);
    fz_var(pix);
    fz_try(ctx) {
        doc = fz_open_document(ctx, path);
        page = fz_load_page(ctx, doc, page_no);
        fz_rect r = fz_bound_page(ctx, page);
        float w = r.x1 - r.x0, h = r.y1 - r.y0;
        float side = width_only ? w : (w > h ? w : h);
        float k = side > 0 ? (float)px / side : 1.0f;
        pix = fz_new_pixmap_from_page(ctx, page, fz_scale(k, k), fz_device_rgb(ctx), 0);
        fz_save_pixmap_as_png(ctx, pix, out_png);
    }
    fz_always(ctx) {
        fz_drop_pixmap(ctx, pix);
        fz_drop_page(ctx, page);
        fz_drop_document(ctx, doc);
    }
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); rc = -1; }
    fz_drop_context(ctx);
    return rc;
}

int grove_doc_to_pdf(const char *path, const char *out_pdf, char *err, int errlen) {
    fz_context *ctx = new_ctx();
    fz_document *doc = NULL;
    fz_document_writer *w = NULL;
    int rc = 0;
    if (!ctx) { set_err(err, errlen, "Couldn't start MuPDF"); return -1; }
    fz_var(doc);
    fz_var(w);
    fz_try(ctx) {
        doc = fz_open_document(ctx, path);
        w = fz_new_pdf_writer(ctx, out_pdf, NULL);
        int n = fz_count_pages(ctx, doc);
        for (int i = 0; i < n; i++) {
            fz_page *page = fz_load_page(ctx, doc, i);
            fz_try(ctx) {
                fz_rect r = fz_bound_page(ctx, page);
                fz_device *dev = fz_begin_page(ctx, w, r);
                fz_run_page(ctx, page, dev, fz_identity, NULL);
                fz_end_page(ctx, w);
            }
            fz_always(ctx) fz_drop_page(ctx, page);
            fz_catch(ctx) fz_rethrow(ctx);
        }
        fz_close_document_writer(ctx, w);
    }
    fz_always(ctx) {
        fz_drop_document_writer(ctx, w);
        fz_drop_document(ctx, doc);
    }
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); rc = -1; }
    fz_drop_context(ctx);
    return rc;
}

/* ---------------------------------------------------------------- DjVu */

static void djvu_pump(ddjvu_context_t *dc, int wait) {
    if (wait) ddjvu_message_wait(dc);
    while (ddjvu_message_peek(dc)) ddjvu_message_pop(dc);
}

int grove_djvu_to_pdf(const char *path, const char *out_pdf, int max_dpi, char *err, int errlen) {
    ddjvu_context_t *dc = ddjvu_context_create("grove");
    if (!dc) { set_err(err, errlen, "Couldn't start djvulibre"); return -1; }
    ddjvu_document_t *dd = ddjvu_document_create_by_filename_utf8(dc, path, 1);
    if (!dd) { ddjvu_context_release(dc); set_err(err, errlen, "This DjVu file can't be read."); return -1; }
    while (!ddjvu_document_decoding_done(dd)) djvu_pump(dc, 1);
    if (ddjvu_document_decoding_error(dd)) {
        ddjvu_document_release(dd);
        ddjvu_context_release(dc);
        set_err(err, errlen, "This DjVu file can't be read.");
        return -1;
    }
    int n = ddjvu_document_get_pagenum(dd);
    fz_context *ctx = new_ctx();
    fz_document_writer *w = NULL;
    int rc = 0;
    if (!ctx) {
        ddjvu_document_release(dd);
        ddjvu_context_release(dc);
        set_err(err, errlen, "Couldn't start MuPDF");
        return -1;
    }
    ddjvu_format_t *fmt = ddjvu_format_create(DDJVU_FORMAT_RGB24, 0, NULL);
    ddjvu_format_set_row_order(fmt, 1);
    fz_var(w);
    fz_try(ctx) {
        w = fz_new_pdf_writer(ctx, out_pdf, NULL);
        for (int i = 0; i < n; i++) {
            ddjvu_page_t *pg = ddjvu_page_create_by_pageno(dd, i);
            if (!pg) continue;
            while (!ddjvu_page_decoding_done(pg)) djvu_pump(dc, 1);
            int pw = ddjvu_page_get_width(pg), ph = ddjvu_page_get_height(pg), dpi = ddjvu_page_get_resolution(pg);
            if (pw <= 0 || ph <= 0 || dpi <= 0) { ddjvu_page_release(pg); continue; }
            /* Drawn at up to max_dpi (scans are often 600 dpi). */
            float k = dpi > max_dpi ? (float)max_dpi / dpi : 1.0f;
            int rw = (int)(pw * k), rh = (int)(ph * k);
            if (rw < 1) rw = 1;
            if (rh < 1) rh = 1;
            ddjvu_rect_t rect = {0, 0, (unsigned)rw, (unsigned)rh};
            unsigned char *buf = malloc((size_t)rw * rh * 3);
            if (!buf) { ddjvu_page_release(pg); fz_throw(ctx, FZ_ERROR_GENERIC, "Out of memory"); }
            if (!ddjvu_page_render(pg, DDJVU_RENDER_COLOR, &rect, &rect, fmt, (unsigned long)rw * 3, (char *)buf))
                memset(buf, 255, (size_t)rw * rh * 3);
            ddjvu_page_release(pg);
            fz_pixmap *pix = NULL;
            fz_image *img = NULL;
            fz_var(pix);
            fz_var(img);
            fz_try(ctx) {
                pix = fz_new_pixmap_with_data(ctx, fz_device_rgb(ctx), rw, rh, NULL, 0, rw * 3, buf);
                img = fz_new_image_from_pixmap(ctx, pix, NULL);
                /* The page at its real size: pixels at the file's dpi. */
                float wpt = pw * 72.0f / dpi, hpt = ph * 72.0f / dpi;
                fz_device *dev = fz_begin_page(ctx, w, fz_make_rect(0, 0, wpt, hpt));
                fz_fill_image(ctx, dev, img, fz_scale(wpt, hpt), 1.0f, fz_default_color_params);
                fz_end_page(ctx, w);
            }
            fz_always(ctx) {
                fz_drop_image(ctx, img);
                fz_drop_pixmap(ctx, pix);
                free(buf);
            }
            fz_catch(ctx) fz_rethrow(ctx);
        }
        fz_close_document_writer(ctx, w);
    }
    fz_always(ctx) fz_drop_document_writer(ctx, w);
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); rc = -1; }
    ddjvu_format_release(fmt);
    fz_drop_context(ctx);
    ddjvu_document_release(dd);
    ddjvu_context_release(dc);
    return rc;
}

/* ---------------------------------------------------------------- PostScript */

#ifndef GROVE_LIBGS
#define GROVE_LIBGS "libgs.so.10"
#endif

typedef int (*gs_new_t)(void **, void *);
typedef int (*gs_io_in_t)(void *, char *, int);
typedef int (*gs_io_out_t)(void *, const char *, int);
typedef int (*gs_stdio_t)(void *, gs_io_in_t, gs_io_out_t, gs_io_out_t);
typedef int (*gs_enc_t)(void *, int);
typedef int (*gs_init_t)(void *, int, char **);
typedef int (*gs_exit_t)(void *);
typedef void (*gs_del_t)(void *);

static int gs_quiet(void *h, const char *s, int len) {
    (void)h;
    (void)s;
    return len;
}

int grove_ps_to_pdf(const char *path, const char *out_pdf, char *err, int errlen) {
    static void *lib = NULL;
    if (!lib) lib = dlopen(GROVE_LIBGS, RTLD_NOW | RTLD_LOCAL | RTLD_DEEPBIND);
    if (!lib) { set_err(err, errlen, "PostScript needs Ghostscript's library (libgs), which isn't installed."); return -1; }
    gs_new_t gs_new = (gs_new_t)dlsym(lib, "gsapi_new_instance");
    gs_stdio_t gs_stdio = (gs_stdio_t)dlsym(lib, "gsapi_set_stdio");
    gs_enc_t gs_enc = (gs_enc_t)dlsym(lib, "gsapi_set_arg_encoding");
    gs_init_t gs_init = (gs_init_t)dlsym(lib, "gsapi_init_with_args");
    gs_exit_t gs_exit = (gs_exit_t)dlsym(lib, "gsapi_exit");
    gs_del_t gs_del = (gs_del_t)dlsym(lib, "gsapi_delete_instance");
    if (!gs_new || !gs_stdio || !gs_enc || !gs_init || !gs_exit || !gs_del) { set_err(err, errlen, "This Ghostscript library is missing its API."); return -1; }
    void *gs = NULL;
    if (gs_new(&gs, NULL) < 0) { set_err(err, errlen, "Couldn't start Ghostscript"); return -1; }
    gs_stdio(gs, NULL, gs_quiet, gs_quiet);
    gs_enc(gs, 1 /* GS_ARG_ENCODING_UTF8 */);
    char out_arg[4200];
    snprintf(out_arg, sizeof out_arg, "-sOutputFile=%s", out_pdf);
    const char *argv[] = {"gs", "-q", "-dSAFER", "-dBATCH", "-dNOPAUSE", "-sDEVICE=pdfwrite", out_arg, "-f", path};
    int code = gs_init(gs, (int)(sizeof argv / sizeof argv[0]), (char **)argv);
    int code1 = gs_exit(gs);
    gs_del(gs);
    /* 0, or gs_error_Quit (-101) when the file ends with quit. */
    if ((code == 0 || code == -101) && code1 == 0) return 0;
    set_err(err, errlen, "Ghostscript couldn't convert this PostScript file.");
    return -1;
}

/* ---------------------------------------------------------------- LibreOffice */
/* LibreOfficeKit (LibreOffice as a library, no soffice command): load a
 * document and save it as PDF. Called only from `grove --office-pdf`, a
 * process of its own (LibreOffice is big and not reentrant). The structs
 * are LibreOfficeKit.h's leading members, which are stable. */

typedef struct LokDoc LokDoc;
typedef struct {
    size_t nSize;
    void (*destroy)(LokDoc *);
    int (*saveAs)(LokDoc *, const char *url, const char *format, const char *options);
} LokDocClass;
struct LokDoc { LokDocClass *pClass; };

typedef struct Lok Lok;
typedef struct {
    size_t nSize;
    void (*destroy)(Lok *);
    LokDoc *(*documentLoad)(Lok *, const char *url);
    char *(*getError)(Lok *);
} LokClass;
struct Lok { LokClass *pClass; };

typedef Lok *(*lok_hook2_t)(const char *install_path, const char *user_profile_url);
typedef Lok *(*lok_hook_t)(const char *install_path);

int grove_office_to_pdf(const char *program_dir, const char *profile_url, const char *path, const char *out_pdf, char *err, int errlen) {
    char lib[4200];
    void *h = NULL;
    const char *names[] = {"libsofficeapp.so", "libmergedlo.so"};
    for (int i = 0; i < 2 && !h; i++) {
        snprintf(lib, sizeof lib, "%s/%s", program_dir, names[i]);
        h = dlopen(lib, RTLD_LAZY | RTLD_GLOBAL);
    }
    if (!h) { set_err(err, errlen, "LibreOffice's library couldn't be loaded."); return -1; }
    lok_hook2_t hook2 = (lok_hook2_t)dlsym(h, "libreofficekit_hook_2");
    lok_hook_t hook = (lok_hook_t)dlsym(h, "libreofficekit_hook");
    Lok *lok = hook2 ? hook2(program_dir, profile_url) : hook ? hook(program_dir) : NULL;
    if (!lok) { set_err(err, errlen, "LibreOffice didn't start."); return -1; }
    LokDoc *doc = lok->pClass->documentLoad(lok, path);
    if (!doc) {
        char *e = lok->pClass->getError ? lok->pClass->getError(lok) : NULL;
        set_err(err, errlen, e && *e ? e : "LibreOffice couldn't open this document.");
        return -1;
    }
    int ok = doc->pClass->saveAs(doc, out_pdf, "pdf", NULL);
    /* Closing it removes LibreOffice's lock file (.~lock.NAME#) beside it. */
    doc->pClass->destroy(doc);
    if (!ok) { set_err(err, errlen, "LibreOffice couldn't make a PDF of this document."); return -1; }
    return 0;
}
