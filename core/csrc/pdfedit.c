/* Grove Preview's PDF work through MuPDF (as docs.c, errors stay in C).
 *
 * Reading (a cached open document per "slot", so dragging a text selection
 * or drawing page after page doesn't reopen the file; slot 0 is for text,
 * 1 for drawing pages, each with its own lock and MuPDF context):
 *   grove_pdf_info      pages, sizes, metadata, outline, password state
 *   grove_pdf_render    one page as RGBA, some annotations left out
 *   grove_pdf_select    a text selection between two points: quads + text
 *   grove_pdf_search    every match of a phrase: page, quads, its line
 *   grove_pdf_links     a page's links (web address or page)
 *   grove_pdf_widgets   form fields of every page
 *   grove_pdf_annots    a page's own annotations (Highlights and Notes)
 *
 * Writing (grove_pdfw_*): a handle on a copy of the document in memory;
 * pages are grafted in from other files, blank or picture pages added,
 * fields filled, annotations added or removed, pages rotated and cropped,
 * the page order set, then saved (optionally encrypted).
 *
 * All coordinates are MuPDF page space: points, y down, the page's own
 * /Rotate applied (as the page is drawn). JSON answers are malloc'ed:
 * free with grove_pdf_free. */

#include <mupdf/fitz.h>
#include <mupdf/pdf.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

static void set_err(char *err, int len, const char *msg) {
    if (err && len > 0) snprintf(err, (size_t)len, "%s", msg ? msg : "unknown error");
}

static fz_context *pv_ctx(void) {
    fz_context *ctx = fz_new_context(NULL, NULL, FZ_STORE_DEFAULT);
    if (!ctx) return NULL;
    fz_try(ctx) fz_register_document_handlers(ctx);
    fz_catch(ctx) { fz_drop_context(ctx); return NULL; }
    fz_set_warning_callback(ctx, NULL, NULL);
    fz_set_error_callback(ctx, NULL, NULL);
    return ctx;
}

void grove_pdf_free(char *p) { free(p); }

/* ---------- JSON out ---------- */

static void js_str(fz_context *ctx, fz_buffer *b, const char *s) {
    fz_append_byte(ctx, b, '"');
    for (const unsigned char *p = (const unsigned char *)(s ? s : ""); *p; p++) {
        unsigned char c = *p;
        if (c == '"' || c == '\\') { fz_append_byte(ctx, b, '\\'); fz_append_byte(ctx, b, c); }
        else if (c == '\n') fz_append_string(ctx, b, "\\n");
        else if (c == '\r') fz_append_string(ctx, b, "\\r");
        else if (c == '\t') fz_append_string(ctx, b, "\\t");
        else if (c < 0x20) fz_append_printf(ctx, b, "\\u%04x", c);
        else fz_append_byte(ctx, b, c);
    }
    fz_append_byte(ctx, b, '"');
}

static void js_num(fz_context *ctx, fz_buffer *b, float v) {
    char t[48];
    snprintf(t, sizeof t, "%.2f", (double)v);
    fz_append_string(ctx, b, t);
}

static void js_quad(fz_context *ctx, fz_buffer *b, fz_quad q) {
    float v[8] = { q.ul.x, q.ul.y, q.ur.x, q.ur.y, q.ll.x, q.ll.y, q.lr.x, q.lr.y };
    fz_append_byte(ctx, b, '[');
    for (int i = 0; i < 8; i++) { if (i) fz_append_byte(ctx, b, ','); js_num(ctx, b, v[i]); }
    fz_append_byte(ctx, b, ']');
}

static void js_rect(fz_context *ctx, fz_buffer *b, fz_rect r) {
    fz_append_byte(ctx, b, '[');
    js_num(ctx, b, r.x0); fz_append_byte(ctx, b, ',');
    js_num(ctx, b, r.y0); fz_append_byte(ctx, b, ',');
    js_num(ctx, b, r.x1); fz_append_byte(ctx, b, ',');
    js_num(ctx, b, r.y1);
    fz_append_byte(ctx, b, ']');
}

static void js_color(fz_context *ctx, fz_buffer *b, int n, const float *c) {
    float r = 0, g = 0, bl = 0;
    if (n == 1) r = g = bl = c[0];
    else if (n == 3) { r = c[0]; g = c[1]; bl = c[2]; }
    else if (n == 4) { r = (1 - c[0]) * (1 - c[3]); g = (1 - c[1]) * (1 - c[3]); bl = (1 - c[2]) * (1 - c[3]); }
    else { fz_append_string(ctx, b, "\"\""); return; }
    fz_append_printf(ctx, b, "\"#%02x%02x%02x\"", (int)(r * 255 + 0.5f), (int)(g * 255 + 0.5f), (int)(bl * 255 + 0.5f));
}

static char *buf_take(fz_context *ctx, fz_buffer *b) {
    unsigned char *data = NULL;
    size_t n = fz_buffer_storage(ctx, b, &data);
    char *s = malloc(n + 1);
    if (!s) return NULL;
    memcpy(s, data, n);
    s[n] = 0;
    return s;
}

/* ---------- the cached documents ---------- */

typedef struct {
    pthread_mutex_t lock;
    fz_context *ctx;
    fz_document *doc;
    char path[4096];
    char pw[512];
    long long mtime, size;
    int pages;
    int locked;              /* needs a password it wasn't given */
    fz_stext_page **text;    /* per page, made on first use */
} slot_t;

static slot_t slots[2] = { { PTHREAD_MUTEX_INITIALIZER }, { PTHREAD_MUTEX_INITIALIZER } };

static void slot_close(slot_t *s) {
    if (!s->ctx) return;
    if (s->text) {
        for (int i = 0; i < s->pages; i++) fz_drop_stext_page(s->ctx, s->text[i]);
        free(s->text);
        s->text = NULL;
    }
    fz_drop_document(s->ctx, s->doc);
    s->doc = NULL;
    s->path[0] = 0;
}

/* Lock the slot and make sure it holds `path` (reopened when the file
 * changed). Returns NULL (unlocked) with err set on failure. */
static slot_t *slot_get(int which, const char *path, const char *pw, char *err, int errlen) {
    slot_t *s = &slots[which & 1];
    struct stat st;
    if (stat(path, &st) != 0) { set_err(err, errlen, "The file isn't there any more."); return NULL; }
    pthread_mutex_lock(&s->lock);
    if (!s->ctx) s->ctx = pv_ctx();
    if (!s->ctx) { pthread_mutex_unlock(&s->lock); set_err(err, errlen, "Couldn't start MuPDF"); return NULL; }
    if (s->doc && strcmp(s->path, path) == 0 && strcmp(s->pw, pw ? pw : "") == 0 && s->mtime == (long long)st.st_mtime && s->size == (long long)st.st_size)
        return s;
    slot_close(s);
    fz_context *ctx = s->ctx;
    int ok = 1;
    fz_try(ctx) {
        s->doc = fz_open_document(ctx, path);
        s->locked = 0;
        if (fz_needs_password(ctx, s->doc)) s->locked = !fz_authenticate_password(ctx, s->doc, pw ? pw : "");
        s->pages = s->locked ? 0 : fz_count_pages(ctx, s->doc);
        s->text = s->pages > 0 ? calloc((size_t)s->pages, sizeof(fz_stext_page *)) : NULL;
        snprintf(s->path, sizeof s->path, "%s", path);
        snprintf(s->pw, sizeof s->pw, "%s", pw ? pw : "");
        s->mtime = (long long)st.st_mtime;
        s->size = (long long)st.st_size;
    }
    fz_catch(ctx) {
        set_err(err, errlen, fz_caught_message(ctx));
        fz_drop_document(ctx, s->doc);
        s->doc = NULL;
        s->path[0] = 0;
        ok = 0;
    }
    if (!ok) { pthread_mutex_unlock(&s->lock); return NULL; }
    return s;
}

static fz_stext_page *slot_text(slot_t *s, int page) {
    if (page < 0 || page >= s->pages) fz_throw(s->ctx, FZ_ERROR_ARGUMENT, "No such page");
    if (!s->text[page]) {
        fz_stext_options opts = { 0 };
        opts.flags = FZ_STEXT_DEHYPHENATE;
        fz_page *p = fz_load_page(s->ctx, s->doc, page);
        fz_try(s->ctx) s->text[page] = fz_new_stext_page_from_page(s->ctx, p, &opts);
        fz_always(s->ctx) fz_drop_page(s->ctx, p);
        fz_catch(s->ctx) fz_rethrow(s->ctx);
    }
    return s->text[page];
}

static void outline_json(fz_context *ctx, fz_document *doc, fz_buffer *b, fz_outline *o, int depth) {
    fz_append_byte(ctx, b, '[');
    int first = 1;
    for (; o; o = o->next) {
        if (!first) fz_append_byte(ctx, b, ',');
        first = 0;
        int pg = -1;
        fz_try(ctx) pg = o->page.page >= 0 || o->page.chapter > 0 ? fz_page_number_from_location(ctx, doc, o->page) : -1;
        fz_catch(ctx) pg = -1;
        fz_append_string(ctx, b, "{\"title\":");
        js_str(ctx, b, o->title);
        fz_append_printf(ctx, b, ",\"page\":%d,\"y\":", pg);
        js_num(ctx, b, o->y);
        if (o->uri && pg < 0) { fz_append_string(ctx, b, ",\"uri\":"); js_str(ctx, b, o->uri); }
        fz_append_string(ctx, b, ",\"kids\":");
        if (o->down && depth < 12) outline_json(ctx, doc, b, o->down, depth + 1);
        else fz_append_string(ctx, b, "[]");
        fz_append_byte(ctx, b, '}');
    }
    fz_append_byte(ctx, b, ']');
}

/* {pages, sizes:[[w,h]], locked, encrypted, title, author, subject,
 *  keywords, creator, producer, created, modified, format, outline, isPdf} */
char *grove_pdf_info(const char *path, const char *pw, char *err, int errlen) {
    slot_t *s = slot_get(0, path, pw, err, errlen);
    if (!s) return NULL;
    fz_context *ctx = s->ctx;
    fz_buffer *b = NULL;
    fz_outline *ol = NULL;
    char *out = NULL;
    fz_var(b);
    fz_var(ol);
    fz_try(ctx) {
        b = fz_new_buffer(ctx, 4096);
        fz_append_printf(ctx, b, "{\"pages\":%d,\"locked\":%s,\"encrypted\":%s,\"isPdf\":%s,\"sizes\":[", s->pages,
                         s->locked ? "true" : "false", fz_needs_password(ctx, s->doc) ? "true" : "false",
                         pdf_specifics(ctx, s->doc) ? "true" : "false");
        for (int i = 0; i < s->pages; i++) {
            fz_page *p = fz_load_page(ctx, s->doc, i);
            fz_rect r = fz_bound_page(ctx, p);
            fz_drop_page(ctx, p);
            if (i) fz_append_byte(ctx, b, ',');
            fz_append_byte(ctx, b, '[');
            js_num(ctx, b, r.x1 - r.x0); fz_append_byte(ctx, b, ',');
            js_num(ctx, b, r.y1 - r.y0);
            fz_append_byte(ctx, b, ']');
        }
        fz_append_byte(ctx, b, ']');
        static const char *keys[][2] = {
            { "title", FZ_META_INFO_TITLE }, { "author", FZ_META_INFO_AUTHOR }, { "subject", FZ_META_INFO_SUBJECT },
            { "keywords", FZ_META_INFO_KEYWORDS }, { "creator", FZ_META_INFO_CREATOR }, { "producer", FZ_META_INFO_PRODUCER },
            { "created", FZ_META_INFO_CREATIONDATE }, { "modified", FZ_META_INFO_MODIFICATIONDATE }, { "format", FZ_META_FORMAT },
            { "encryption", FZ_META_ENCRYPTION } };
        for (size_t k = 0; k < sizeof keys / sizeof keys[0]; k++) {
            char v[1024] = { 0 };
            if (fz_lookup_metadata(ctx, s->doc, keys[k][1], v, sizeof v) > 0) {
                fz_append_printf(ctx, b, ",\"%s\":", keys[k][0]);
                js_str(ctx, b, v);
            }
        }
        fz_append_string(ctx, b, ",\"outline\":");
        if (!s->locked) {
            fz_try(ctx) ol = fz_load_outline(ctx, s->doc);
            fz_catch(ctx) ol = NULL;
        }
        if (ol) outline_json(ctx, s->doc, b, ol, 0);
        else fz_append_string(ctx, b, "[]");
        fz_append_byte(ctx, b, '}');
        out = buf_take(ctx, b);
    }
    fz_always(ctx) {
        fz_drop_outline(ctx, ol);
        fz_drop_buffer(ctx, b);
    }
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); out = NULL; }
    pthread_mutex_unlock(&s->lock);
    return out;
}

/* Page `page` drawn `width` pixels wide as RGBA (premultiplied, alpha 255
 * on white). `hide` lists annotation indices (pdf_first_annot order) to
 * leave out; `nohide` < 0 draws everything. *w, *h get the size. */
unsigned char *grove_pdf_render(const char *path, const char *pw, int page, int width, const int *hide, int nhide, int *w, int *h, char *err, int errlen) {
    slot_t *s = slot_get(1, path, pw, err, errlen);
    if (!s) return NULL;
    fz_context *ctx = s->ctx;
    fz_page *pg = NULL;
    fz_pixmap *pix = NULL;
    fz_device *dev = NULL;
    unsigned char *out = NULL;
    fz_var(pg);
    fz_var(pix);
    fz_var(dev);
    fz_try(ctx) {
        if (s->locked) fz_throw(ctx, FZ_ERROR_GENERIC, "This document is locked with a password.");
        pg = fz_load_page(ctx, s->doc, page);
        fz_rect r = fz_bound_page(ctx, pg);
        float k = (r.x1 - r.x0) > 0 ? (float)width / (r.x1 - r.x0) : 1;
        fz_matrix m = fz_scale(k, k);
        fz_irect ir = fz_round_rect(fz_transform_rect(r, m));
        pix = fz_new_pixmap_with_bbox(ctx, fz_device_rgb(ctx), ir, NULL, 1);
        fz_clear_pixmap_with_value(ctx, pix, 255);
        dev = fz_new_draw_device(ctx, m, pix);
        pdf_page *ppg = pdf_page_from_fz_page(ctx, pg);
        if (nhide <= 0 || !ppg) {
            fz_run_page(ctx, pg, dev, fz_identity, NULL);
        } else {
            fz_run_page_contents(ctx, pg, dev, fz_identity, NULL);
            int i = 0;
            for (pdf_annot *a = pdf_first_annot(ctx, ppg); a; a = pdf_next_annot(ctx, a), i++) {
                int skip = 0;
                for (int j = 0; j < nhide; j++) if (hide[j] == i) skip = 1;
                if (!skip) pdf_run_annot(ctx, a, dev, fz_identity, NULL);
            }
            fz_run_page_widgets(ctx, pg, dev, fz_identity, NULL);
        }
        fz_close_device(ctx, dev);
        *w = fz_pixmap_width(ctx, pix);
        *h = fz_pixmap_height(ctx, pix);
        int stride = fz_pixmap_stride(ctx, pix);
        unsigned char *src = fz_pixmap_samples(ctx, pix);
        out = malloc((size_t)(*w) * (size_t)(*h) * 4);
        if (!out) fz_throw(ctx, FZ_ERROR_SYSTEM, "Out of memory");
        for (int y = 0; y < *h; y++) memcpy(out + (size_t)y * (*w) * 4, src + (size_t)y * stride, (size_t)(*w) * 4);
    }
    fz_always(ctx) {
        fz_drop_device(ctx, dev);
        fz_drop_pixmap(ctx, pix);
        fz_drop_page(ctx, pg);
    }
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); free(out); out = NULL; }
    pthread_mutex_unlock(&s->lock);
    return out;
}

/* mode 0: characters from a to b; 1: whole words; 2: whole lines;
 * 3: everything on the page. {quads, text} */
char *grove_pdf_select(const char *path, const char *pw, int page, float ax, float ay, float bx, float by, int mode, char *err, int errlen) {
    slot_t *s = slot_get(0, path, pw, err, errlen);
    if (!s) return NULL;
    fz_context *ctx = s->ctx;
    fz_buffer *b = NULL;
    char *text = NULL, *out = NULL;
    fz_quad quads[2000];
    fz_var(b);
    fz_var(text);
    fz_try(ctx) {
        fz_stext_page *st = slot_text(s, page);
        fz_point a = { ax, ay }, p2 = { bx, by };
        if (mode == 3) { a = fz_make_point(st->mediabox.x0, st->mediabox.y0); p2 = fz_make_point(st->mediabox.x1, st->mediabox.y1); }
        else if (mode == 1) fz_snap_selection(ctx, st, &a, &p2, FZ_SELECT_WORDS);
        else if (mode == 2) fz_snap_selection(ctx, st, &a, &p2, FZ_SELECT_LINES);
        int n = fz_highlight_selection(ctx, st, a, p2, quads, 2000);
        text = fz_copy_selection(ctx, st, a, p2, 0);
        b = fz_new_buffer(ctx, 1024);
        fz_append_string(ctx, b, "{\"quads\":[");
        for (int i = 0; i < n; i++) { if (i) fz_append_byte(ctx, b, ','); js_quad(ctx, b, quads[i]); }
        fz_append_string(ctx, b, "],\"text\":");
        js_str(ctx, b, text);
        fz_append_byte(ctx, b, '}');
        out = buf_take(ctx, b);
    }
    fz_always(ctx) {
        fz_free(ctx, text);
        fz_drop_buffer(ctx, b);
    }
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); out = NULL; }
    pthread_mutex_unlock(&s->lock);
    return out;
}

/* The line of text around a point (a search result's context). */
static void line_at(fz_context *ctx, fz_buffer *b, fz_stext_page *st, fz_point p) {
    for (fz_stext_block *bl = st->first_block; bl; bl = bl->next) {
        if (bl->type != FZ_STEXT_BLOCK_TEXT) continue;
        for (fz_stext_line *ln = bl->u.t.first_line; ln; ln = ln->next) {
            if (!fz_is_point_inside_rect(p, ln->bbox)) continue;
            for (fz_stext_char *ch = ln->first_char; ch; ch = ch->next) {
                char u[8];
                int n = fz_runetochar(u, ch->c);
                u[n] = 0;
                fz_append_string(ctx, b, u);
            }
            return;
        }
    }
}

/* [{page, quads:[[...]], line}] for every match (first `max`). */
char *grove_pdf_search(const char *path, const char *pw, const char *needle, int max, char *err, int errlen) {
    slot_t *s = slot_get(0, path, pw, err, errlen);
    if (!s) return NULL;
    fz_context *ctx = s->ctx;
    fz_buffer *b = NULL, *line = NULL;
    char *out = NULL;
    fz_quad quads[512];
    int marks[512];
    fz_var(b);
    fz_var(line);
    fz_try(ctx) {
        b = fz_new_buffer(ctx, 4096);
        line = fz_new_buffer(ctx, 256);
        fz_append_byte(ctx, b, '[');
        int total = 0;
        for (int p = 0; p < s->pages && total < max; p++) {
            fz_stext_page *st = slot_text(s, p);
            int n = fz_search_stext_page(ctx, st, needle, marks, quads, 512);
            int i = 0;
            while (i < n && total < max) {
                int j = i + 1;
                while (j < n && !marks[j]) j++;
                if (total) fz_append_byte(ctx, b, ',');
                fz_append_printf(ctx, b, "{\"page\":%d,\"quads\":[", p);
                for (int k = i; k < j; k++) { if (k > i) fz_append_byte(ctx, b, ','); js_quad(ctx, b, quads[k]); }
                fz_append_string(ctx, b, "],\"line\":");
                fz_clear_buffer(ctx, line);
                fz_point c = { (quads[i].ul.x + quads[i].lr.x) / 2, (quads[i].ul.y + quads[i].lr.y) / 2 };
                line_at(ctx, line, st, c);
                fz_terminate_buffer(ctx, line);
                unsigned char *d = NULL;
                fz_buffer_storage(ctx, line, &d);
                js_str(ctx, b, (const char *)d);
                fz_append_byte(ctx, b, '}');
                total++;
                i = j;
            }
        }
        fz_append_byte(ctx, b, ']');
        out = buf_take(ctx, b);
    }
    fz_always(ctx) {
        fz_drop_buffer(ctx, line);
        fz_drop_buffer(ctx, b);
    }
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); out = NULL; }
    pthread_mutex_unlock(&s->lock);
    return out;
}

/* [{r:[x0,y0,x1,y1], uri} or {r, page, y}] */
char *grove_pdf_links(const char *path, const char *pw, int page, char *err, int errlen) {
    slot_t *s = slot_get(0, path, pw, err, errlen);
    if (!s) return NULL;
    fz_context *ctx = s->ctx;
    fz_buffer *b = NULL;
    fz_page *pg = NULL;
    fz_link *links = NULL;
    char *out = NULL;
    fz_var(b);
    fz_var(pg);
    fz_var(links);
    fz_try(ctx) {
        pg = fz_load_page(ctx, s->doc, page);
        links = fz_load_links(ctx, pg);
        b = fz_new_buffer(ctx, 512);
        fz_append_byte(ctx, b, '[');
        int first = 1;
        for (fz_link *l = links; l; l = l->next) {
            if (!l->uri) continue;
            if (!first) fz_append_byte(ctx, b, ',');
            first = 0;
            fz_append_string(ctx, b, "{\"r\":");
            js_rect(ctx, b, l->rect);
            if (fz_is_external_link(ctx, l->uri)) {
                fz_append_string(ctx, b, ",\"uri\":");
                js_str(ctx, b, l->uri);
            } else {
                float x = 0, y = 0;
                int pn = -1;
                fz_try(ctx) pn = fz_page_number_from_location(ctx, s->doc, fz_resolve_link(ctx, s->doc, l->uri, &x, &y));
                fz_catch(ctx) pn = -1;
                fz_append_printf(ctx, b, ",\"page\":%d,\"y\":", pn);
                js_num(ctx, b, y);
            }
            fz_append_byte(ctx, b, '}');
        }
        fz_append_byte(ctx, b, ']');
        out = buf_take(ctx, b);
    }
    fz_always(ctx) {
        fz_drop_link(ctx, links);
        fz_drop_page(ctx, pg);
        fz_drop_buffer(ctx, b);
    }
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); out = NULL; }
    pthread_mutex_unlock(&s->lock);
    return out;
}

static const char *widget_kind(enum pdf_widget_type t) {
    switch (t) {
    case PDF_WIDGET_TYPE_BUTTON: return "button";
    case PDF_WIDGET_TYPE_CHECKBOX: return "checkbox";
    case PDF_WIDGET_TYPE_COMBOBOX: return "combo";
    case PDF_WIDGET_TYPE_LISTBOX: return "list";
    case PDF_WIDGET_TYPE_RADIOBUTTON: return "radio";
    case PDF_WIDGET_TYPE_SIGNATURE: return "signature";
    case PDF_WIDGET_TYPE_TEXT: return "text";
    default: return "unknown";
    }
}

/* [{page, i, kind, name, value, on, r, readonly, multiline, options, maxLen}] */
char *grove_pdf_widgets(const char *path, const char *pw, char *err, int errlen) {
    slot_t *s = slot_get(0, path, pw, err, errlen);
    if (!s) return NULL;
    fz_context *ctx = s->ctx;
    fz_buffer *b = NULL;
    char *out = NULL;
    fz_var(b);
    fz_try(ctx) {
        b = fz_new_buffer(ctx, 1024);
        fz_append_byte(ctx, b, '[');
        pdf_document *pdf = pdf_specifics(ctx, s->doc);
        int first = 1;
        for (int p = 0; pdf && p < s->pages; p++) {
            pdf_page *pg = pdf_load_page(ctx, pdf, p);
            fz_try(ctx) {
                int i = 0;
                for (pdf_annot *w = pdf_first_widget(ctx, pg); w; w = pdf_next_widget(ctx, w), i++) {
                    pdf_obj *obj = pdf_annot_obj(ctx, w);
                    enum pdf_widget_type t = pdf_widget_type(ctx, w);
                    int flags = pdf_field_flags(ctx, obj);
                    char *name = pdf_load_field_name(ctx, obj);
                    if (!first) fz_append_byte(ctx, b, ',');
                    first = 0;
                    fz_append_printf(ctx, b, "{\"page\":%d,\"i\":%d,\"kind\":\"%s\",\"name\":", p, i, widget_kind(t));
                    js_str(ctx, b, name);
                    fz_free(ctx, name);
                    fz_append_string(ctx, b, ",\"value\":");
                    js_str(ctx, b, pdf_field_value(ctx, obj));
                    if (t == PDF_WIDGET_TYPE_CHECKBOX || t == PDF_WIDGET_TYPE_RADIOBUTTON) {
                        fz_append_string(ctx, b, ",\"on\":");
                        pdf_obj *on = pdf_button_field_on_state(ctx, obj);
                        js_str(ctx, b, on ? pdf_to_name(ctx, on) : "Yes");
                    }
                    fz_append_string(ctx, b, ",\"r\":");
                    js_rect(ctx, b, pdf_bound_widget(ctx, w));
                    fz_append_printf(ctx, b, ",\"readonly\":%s,\"multiline\":%s,\"maxLen\":%d",
                                     (flags & PDF_FIELD_IS_READ_ONLY) ? "true" : "false",
                                     (t == PDF_WIDGET_TYPE_TEXT && (flags & PDF_TX_FIELD_IS_MULTILINE)) ? "true" : "false",
                                     t == PDF_WIDGET_TYPE_TEXT ? pdf_text_widget_max_len(ctx, w) : 0);
                    if (t == PDF_WIDGET_TYPE_COMBOBOX || t == PDF_WIDGET_TYPE_LISTBOX) {
                        int n = pdf_choice_widget_options(ctx, w, 0, NULL);
                        const char **opts = n > 0 ? fz_malloc(ctx, sizeof(char *) * (size_t)n) : NULL;
                        if (n > 0) pdf_choice_widget_options(ctx, w, 0, opts);
                        fz_append_string(ctx, b, ",\"options\":[");
                        for (int k = 0; k < n; k++) { if (k) fz_append_byte(ctx, b, ','); js_str(ctx, b, opts[k]); }
                        fz_append_byte(ctx, b, ']');
                        fz_free(ctx, opts);
                    }
                    fz_append_byte(ctx, b, '}');
                }
            }
            fz_always(ctx) fz_drop_page(ctx, (fz_page *)pg);
            fz_catch(ctx) fz_rethrow(ctx);
        }
        fz_append_byte(ctx, b, ']');
        out = buf_take(ctx, b);
    }
    fz_always(ctx) fz_drop_buffer(ctx, b);
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); out = NULL; }
    pthread_mutex_unlock(&s->lock);
    return out;
}

/* A page's annotations (not form fields): [{i, type, r, color, contents,
 * author, quads?}] */
char *grove_pdf_annots(const char *path, const char *pw, int page, char *err, int errlen) {
    slot_t *s = slot_get(0, path, pw, err, errlen);
    if (!s) return NULL;
    fz_context *ctx = s->ctx;
    fz_buffer *b = NULL;
    pdf_page *pg = NULL;
    char *out = NULL;
    fz_var(b);
    fz_var(pg);
    fz_try(ctx) {
        b = fz_new_buffer(ctx, 512);
        fz_append_byte(ctx, b, '[');
        pdf_document *pdf = pdf_specifics(ctx, s->doc);
        if (pdf) {
            pg = pdf_load_page(ctx, pdf, page);
            int i = 0, first = 1;
            for (pdf_annot *a = pdf_first_annot(ctx, pg); a; a = pdf_next_annot(ctx, a), i++) {
                enum pdf_annot_type t = pdf_annot_type(ctx, a);
                if (t == PDF_ANNOT_WIDGET || t == PDF_ANNOT_POPUP || t == PDF_ANNOT_LINK) continue;
                if (!first) fz_append_byte(ctx, b, ',');
                first = 0;
                fz_append_printf(ctx, b, "{\"i\":%d,\"type\":", i);
                js_str(ctx, b, pdf_string_from_annot_type(ctx, t));
                fz_append_string(ctx, b, ",\"r\":");
                js_rect(ctx, b, pdf_bound_annot(ctx, a));
                int n = 0;
                float c[4];
                pdf_annot_color(ctx, a, &n, c);
                fz_append_string(ctx, b, ",\"color\":");
                js_color(ctx, b, n, c);
                fz_append_string(ctx, b, ",\"contents\":");
                js_str(ctx, b, pdf_annot_contents(ctx, a));
                fz_append_string(ctx, b, ",\"author\":");
                const char *au = NULL;
                fz_try(ctx) au = pdf_annot_has_author(ctx, a) ? pdf_annot_author(ctx, a) : NULL;
                fz_catch(ctx) au = NULL;
                js_str(ctx, b, au);
                if (pdf_annot_has_quad_points(ctx, a)) {
                    int nq = pdf_annot_quad_point_count(ctx, a);
                    fz_append_string(ctx, b, ",\"quads\":[");
                    for (int q = 0; q < nq; q++) { if (q) fz_append_byte(ctx, b, ','); js_quad(ctx, b, pdf_annot_quad_point(ctx, a, q)); }
                    fz_append_byte(ctx, b, ']');
                }
                fz_append_byte(ctx, b, '}');
            }
        }
        fz_append_byte(ctx, b, ']');
        out = buf_take(ctx, b);
    }
    fz_always(ctx) {
        fz_drop_page(ctx, (fz_page *)pg);
        fz_drop_buffer(ctx, b);
    }
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); out = NULL; }
    pthread_mutex_unlock(&s->lock);
    return out;
}

/* ---------- writing ---------- */

#define MAX_SRC 64

typedef struct {
    fz_context *ctx;
    pdf_document *doc;
    int nsrc;
    char *src_path[MAX_SRC];
    pdf_document *src[MAX_SRC];
    pdf_graft_map *map[MAX_SRC];
} pdfw_t;

/* A PDF to edit: `path` (empty: a new, empty document). */
void *grove_pdfw_open(const char *path, const char *pw, char *err, int errlen) {
    pdfw_t *h = calloc(1, sizeof *h);
    if (!h) return NULL;
    h->ctx = pv_ctx();
    if (!h->ctx) { free(h); set_err(err, errlen, "Couldn't start MuPDF"); return NULL; }
    fz_context *ctx = h->ctx;
    fz_try(ctx) {
        if (path && *path) {
            h->doc = pdf_open_document(ctx, path);
            if (pdf_needs_password(ctx, h->doc) && !pdf_authenticate_password(ctx, h->doc, pw ? pw : ""))
                fz_throw(ctx, FZ_ERROR_GENERIC, "The password is wrong.");
        } else {
            h->doc = pdf_create_document(ctx);
        }
    }
    fz_catch(ctx) {
        set_err(err, errlen, fz_caught_message(ctx));
        pdf_drop_document(ctx, h->doc);
        fz_drop_context(ctx);
        free(h);
        return NULL;
    }
    return h;
}

void grove_pdfw_close(void *hp) {
    pdfw_t *h = hp;
    if (!h) return;
    for (int i = 0; i < h->nsrc; i++) {
        pdf_drop_graft_map(h->ctx, h->map[i]);
        pdf_drop_document(h->ctx, h->src[i]);
        free(h->src_path[i]);
    }
    pdf_drop_document(h->ctx, h->doc);
    fz_drop_context(h->ctx);
    free(h);
}

int grove_pdfw_count(void *hp) {
    pdfw_t *h = hp;
    int n = 0;
    fz_try(h->ctx) n = pdf_count_pages(h->ctx, h->doc);
    fz_catch(h->ctx) n = -1;
    return n;
}

/* Append page `page` of another PDF; returns its index or -1. */
int grove_pdfw_graft(void *hp, const char *src, const char *pw, int page, char *err, int errlen) {
    pdfw_t *h = hp;
    fz_context *ctx = h->ctx;
    int at = -1;
    fz_try(ctx) {
        int k = -1;
        for (int i = 0; i < h->nsrc; i++) if (strcmp(h->src_path[i], src) == 0) k = i;
        if (k < 0) {
            if (h->nsrc >= MAX_SRC) fz_throw(ctx, FZ_ERROR_GENERIC, "Pages from too many documents");
            pdf_document *d = pdf_open_document(ctx, src);
            if (pdf_needs_password(ctx, d) && !pdf_authenticate_password(ctx, d, pw ? pw : "")) {
                pdf_drop_document(ctx, d);
                fz_throw(ctx, FZ_ERROR_GENERIC, "A document is locked with a password.");
            }
            k = h->nsrc++;
            h->src[k] = d;
            h->src_path[k] = strdup(src);
            h->map[k] = pdf_new_graft_map(ctx, h->doc);
        }
        pdf_graft_mapped_page(ctx, h->map[k], -1, h->src[k], page);
        at = pdf_count_pages(ctx, h->doc) - 1;
    }
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); at = -1; }
    return at;
}

/* Append a blank page (points); its index or -1. */
int grove_pdfw_blank(void *hp, float w, float hgt, char *err, int errlen) {
    pdfw_t *h = hp;
    fz_context *ctx = h->ctx;
    fz_buffer *c = NULL;
    pdf_obj *page = NULL;
    int at = -1;
    fz_var(c);
    fz_var(page);
    fz_try(ctx) {
        c = fz_new_buffer(ctx, 1);
        page = pdf_add_page(ctx, h->doc, fz_make_rect(0, 0, w, hgt), 0, NULL, c);
        pdf_insert_page(ctx, h->doc, -1, page);
        at = pdf_count_pages(ctx, h->doc) - 1;
    }
    fz_always(ctx) {
        pdf_drop_obj(ctx, page);
        fz_drop_buffer(ctx, c);
    }
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); at = -1; }
    return at;
}

/* Append a page holding a picture file (its size at its resolution, or
 * fitted into `fit_w` x `fit_h` points when those are > 0). */
int grove_pdfw_image_page(void *hp, const char *img_path, float fit_w, float fit_h, char *err, int errlen) {
    pdfw_t *h = hp;
    fz_context *ctx = h->ctx;
    fz_image *img = NULL;
    fz_buffer *c = NULL;
    pdf_obj *res = NULL, *xobjs = NULL, *ref = NULL, *page = NULL;
    int at = -1;
    fz_var(img); fz_var(c); fz_var(res); fz_var(xobjs); fz_var(ref); fz_var(page);
    fz_try(ctx) {
        img = fz_new_image_from_file(ctx, img_path);
        int xres = 96, yres = 96;
        fz_image_resolution(img, &xres, &yres);
        if (xres < 10) xres = 72;
        if (yres < 10) yres = 72;
        float w = img->w * 72.0f / xres, hh = img->h * 72.0f / yres;
        float pw = w, ph = hh, dx = 0, dy = 0, dw = w, dh = hh;
        if (fit_w > 0 && fit_h > 0) {
            pw = fit_w; ph = fit_h;
            float k = fz_min(fit_w / w, fit_h / hh);
            dw = w * k; dh = hh * k;
            dx = (fit_w - dw) / 2; dy = (fit_h - dh) / 2;
        }
        ref = pdf_add_image(ctx, h->doc, img);
        res = pdf_new_dict(ctx, h->doc, 1);
        xobjs = pdf_dict_put_dict(ctx, res, PDF_NAME(XObject), 1);
        pdf_dict_puts(ctx, xobjs, "Im0", ref);
        c = fz_new_buffer(ctx, 64);
        fz_append_printf(ctx, c, "q %g 0 0 %g %g %g cm /Im0 Do Q", dw, dh, dx, dy);
        page = pdf_add_page(ctx, h->doc, fz_make_rect(0, 0, pw, ph), 0, res, c);
        pdf_insert_page(ctx, h->doc, -1, page);
        at = pdf_count_pages(ctx, h->doc) - 1;
    }
    fz_always(ctx) {
        pdf_drop_obj(ctx, page);
        pdf_drop_obj(ctx, ref);
        pdf_drop_obj(ctx, res);
        fz_drop_buffer(ctx, c);
        fz_drop_image(ctx, img);
    }
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); at = -1; }
    return at;
}

/* Turn a page by `deg` more (multiples of 90). */
int grove_pdfw_rotate(void *hp, int page, int deg, char *err, int errlen) {
    pdfw_t *h = hp;
    fz_context *ctx = h->ctx;
    int rc = 0;
    fz_try(ctx) {
        pdf_obj *p = pdf_lookup_page_obj(ctx, h->doc, page);
        int cur = pdf_to_int(ctx, pdf_dict_get_inheritable(ctx, p, PDF_NAME(Rotate)));
        pdf_dict_put_int(ctx, p, PDF_NAME(Rotate), (((cur + deg) % 360) + 360) % 360);
    }
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); rc = -1; }
    return rc;
}

/* Crop a page to a rectangle in its page space (sets /CropBox). */
int grove_pdfw_crop(void *hp, int page, float x0, float y0, float x1, float y1, char *err, int errlen) {
    pdfw_t *h = hp;
    fz_context *ctx = h->ctx;
    pdf_page *pg = NULL;
    int rc = 0;
    fz_var(pg);
    fz_try(ctx) {
        pg = pdf_load_page(ctx, h->doc, page);
        fz_matrix ctm;
        pdf_page_transform(ctx, pg, NULL, &ctm);
        fz_rect r = fz_transform_rect(fz_make_rect(x0, y0, x1, y1), fz_invert_matrix(ctm));
        pdf_dict_put_rect(ctx, pg->obj, PDF_NAME(CropBox), r);
    }
    fz_always(ctx) fz_drop_page(ctx, (fz_page *)pg);
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); rc = -1; }
    return rc;
}

/* Fill form field `index` (pdf_first_widget order) of a page. Check
 * boxes and radio buttons: "Off" or their on-state name. */
int grove_pdfw_field(void *hp, int page, int index, const char *value, char *err, int errlen) {
    pdfw_t *h = hp;
    fz_context *ctx = h->ctx;
    pdf_page *pg = NULL;
    int rc = 0;
    fz_var(pg);
    fz_try(ctx) {
        pg = pdf_load_page(ctx, h->doc, page);
        pdf_annot *w = pdf_first_widget(ctx, pg);
        for (int i = 0; w && i < index; i++) w = pdf_next_widget(ctx, w);
        if (!w) fz_throw(ctx, FZ_ERROR_GENERIC, "No such form field");
        enum pdf_widget_type t = pdf_widget_type(ctx, w);
        if (t == PDF_WIDGET_TYPE_TEXT) pdf_set_text_field_value(ctx, w, value);
        else if (t == PDF_WIDGET_TYPE_COMBOBOX || t == PDF_WIDGET_TYPE_LISTBOX) pdf_set_choice_field_value(ctx, w, value);
        else if (t == PDF_WIDGET_TYPE_CHECKBOX || t == PDF_WIDGET_TYPE_RADIOBUTTON) {
            const char *cur = pdf_field_value(ctx, pdf_annot_obj(ctx, w));
            int on_now = cur && strcmp(cur, "Off") != 0 && *cur;
            int want = value && strcmp(value, "Off") != 0 && *value;
            if (on_now != want) pdf_toggle_widget(ctx, w);
        }
        pdf_update_page(ctx, pg);
    }
    fz_always(ctx) fz_drop_page(ctx, (fz_page *)pg);
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); rc = -1; }
    return rc;
}

/* Remove a page's annotations by index (pdf_first_annot order). */
int grove_pdfw_delete_annots(void *hp, int page, const int *idx, int n, char *err, int errlen) {
    pdfw_t *h = hp;
    fz_context *ctx = h->ctx;
    pdf_page *pg = NULL;
    pdf_annot **del = NULL;
    int rc = 0;
    fz_var(pg);
    fz_var(del);
    fz_try(ctx) {
        pg = pdf_load_page(ctx, h->doc, page);
        del = fz_calloc(ctx, (size_t)(n > 0 ? n : 1), sizeof(pdf_annot *));
        int i = 0, k = 0;
        for (pdf_annot *a = pdf_first_annot(ctx, pg); a; a = pdf_next_annot(ctx, a), i++)
            for (int j = 0; j < n; j++) if (idx[j] == i) del[k++] = a;
        for (int j = 0; j < k; j++) pdf_delete_annot(ctx, pg, del[j]);
        pdf_update_page(ctx, pg);
    }
    fz_always(ctx) {
        fz_free(ctx, del);
        fz_drop_page(ctx, (fz_page *)pg);
    }
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); rc = -1; }
    return rc;
}

/* Add an annotation. kind:
 *   0 ink (strokes: pts split by counts), 1 square, 2 circle, 3 line,
 *   4 arrow, 5 free text (box x0 y0 x1 y1, text, font size), 6 note
 *   (point, text), 7 highlight, 8 underline, 9 strike-out (quads, 8
 *   numbers each), 10 squiggly.
 * rgb: 3 floats; fill: 3 floats or NULL. */
int grove_pdfw_annot(void *hp, int page, int kind, const float *rgb, float alpha, float width,
                     const float *pts, int npts, const int *counts, int nstrokes,
                     const char *text, float font_size, const float *fill, const char *author, char *err, int errlen) {
    pdfw_t *h = hp;
    fz_context *ctx = h->ctx;
    pdf_page *pg = NULL;
    pdf_annot *a = NULL;
    int rc = 0;
    fz_var(pg);
    fz_var(a);
    static const enum pdf_annot_type types[] = { PDF_ANNOT_INK, PDF_ANNOT_SQUARE, PDF_ANNOT_CIRCLE, PDF_ANNOT_LINE, PDF_ANNOT_LINE,
        PDF_ANNOT_FREE_TEXT, PDF_ANNOT_TEXT, PDF_ANNOT_HIGHLIGHT, PDF_ANNOT_UNDERLINE, PDF_ANNOT_STRIKE_OUT, PDF_ANNOT_SQUIGGLY };
    fz_try(ctx) {
        if (kind < 0 || kind > 10) fz_throw(ctx, FZ_ERROR_ARGUMENT, "Unknown annotation");
        pg = pdf_load_page(ctx, h->doc, page);
        a = pdf_create_annot(ctx, pg, types[kind]);
        if (author && *author) pdf_set_annot_author(ctx, a, author);
        if (kind == 5) {
            if (npts < 4) fz_throw(ctx, FZ_ERROR_ARGUMENT, "A text box needs a rectangle");
            pdf_set_annot_rect(ctx, a, fz_make_rect(fz_min(pts[0], pts[2]), fz_min(pts[1], pts[3]), fz_max(pts[0], pts[2]), fz_max(pts[1], pts[3])));
            pdf_set_annot_contents(ctx, a, text ? text : "");
            pdf_set_annot_default_appearance(ctx, a, "Helv", font_size > 1 ? font_size : 12, 3, rgb);
            pdf_set_annot_border_width(ctx, a, 0);
            if (fill) pdf_set_annot_interior_color(ctx, a, 3, fill);
        } else {
            pdf_set_annot_color(ctx, a, 3, rgb);
        }
        if (alpha < 1) pdf_set_annot_opacity(ctx, a, alpha);
        switch (kind) {
        case 0: {
            pdf_clear_annot_ink_list(ctx, a);
            int k = 0;
            for (int s = 0; s < nstrokes; s++) {
                pdf_add_annot_ink_list_stroke(ctx, a);
                for (int i = 0; i < counts[s] && k + 1 < npts; i++, k += 2)
                    pdf_add_annot_ink_list_stroke_vertex(ctx, a, fz_make_point(pts[k], pts[k + 1]));
            }
            pdf_set_annot_border_width(ctx, a, width);
            break;
        }
        case 1:
        case 2:
            if (npts < 4) fz_throw(ctx, FZ_ERROR_ARGUMENT, "A shape needs a rectangle");
            pdf_set_annot_rect(ctx, a, fz_make_rect(fz_min(pts[0], pts[2]), fz_min(pts[1], pts[3]), fz_max(pts[0], pts[2]), fz_max(pts[1], pts[3])));
            pdf_set_annot_border_width(ctx, a, width);
            if (fill) pdf_set_annot_interior_color(ctx, a, 3, fill);
            break;
        case 3:
        case 4:
            if (npts < 4) fz_throw(ctx, FZ_ERROR_ARGUMENT, "A line needs two points");
            pdf_set_annot_line(ctx, a, fz_make_point(pts[0], pts[1]), fz_make_point(pts[2], pts[3]));
            pdf_set_annot_border_width(ctx, a, width);
            if (kind == 4) pdf_set_annot_line_ending_styles(ctx, a, PDF_ANNOT_LE_NONE, PDF_ANNOT_LE_OPEN_ARROW);
            break;
        case 6:
            if (npts < 2) fz_throw(ctx, FZ_ERROR_ARGUMENT, "A note needs a place");
            pdf_set_annot_rect(ctx, a, fz_make_rect(pts[0], pts[1], pts[0] + 20, pts[1] + 20));
            pdf_set_annot_contents(ctx, a, text ? text : "");
            pdf_set_annot_icon_name(ctx, a, "Comment");
            break;
        case 7: case 8: case 9: case 10:
            pdf_clear_annot_quad_points(ctx, a);
            for (int i = 0; i + 7 < npts; i += 8) {
                fz_quad q = { { pts[i], pts[i + 1] }, { pts[i + 2], pts[i + 3] }, { pts[i + 4], pts[i + 5] }, { pts[i + 6], pts[i + 7] } };
                pdf_add_annot_quad_point(ctx, a, q);
            }
            if (text && *text) pdf_set_annot_contents(ctx, a, text);
            break;
        }
        pdf_update_annot(ctx, a);
    }
    fz_always(ctx) {
        pdf_drop_annot(ctx, a);
        fz_drop_page(ctx, (fz_page *)pg);
    }
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); rc = -1; }
    return rc;
}

/* Keep only these pages, in this order. */
int grove_pdfw_reorder(void *hp, const int *pages, int n, char *err, int errlen) {
    pdfw_t *h = hp;
    fz_context *ctx = h->ctx;
    int rc = 0;
    fz_try(ctx) pdf_rearrange_pages(ctx, h->doc, n, pages, PDF_CLEAN_STRUCTURE_DROP);
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); rc = -1; }
    return rc;
}

/* Set a metadata entry (Title, Author, Subject, Keywords). */
int grove_pdfw_meta(void *hp, const char *key, const char *value, char *err, int errlen) {
    pdfw_t *h = hp;
    fz_context *ctx = h->ctx;
    int rc = 0;
    fz_try(ctx) {
        char k[64];
        snprintf(k, sizeof k, "info:%s", key);
        fz_set_metadata(ctx, (fz_document *)h->doc, k, value);
    }
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); rc = -1; }
    return rc;
}

/* Save to `out`. With a user or owner password, encrypted (AES-256);
 * `allow_print`/`allow_copy` 0 take those rights away. */
int grove_pdfw_save(void *hp, const char *out, const char *user_pw, const char *owner_pw, int allow_print, int allow_copy, char *err, int errlen) {
    pdfw_t *h = hp;
    fz_context *ctx = h->ctx;
    int rc = 0;
    fz_try(ctx) {
        pdf_write_options o = pdf_default_write_options;
        o.do_garbage = 1;
        o.do_compress = 1;
        o.do_compress_images = 1;
        o.do_compress_fonts = 1;
        if ((user_pw && *user_pw) || (owner_pw && *owner_pw)) {
            o.do_encrypt = PDF_ENCRYPT_AES_256;
            snprintf(o.upwd_utf8, sizeof o.upwd_utf8, "%s", user_pw ? user_pw : "");
            snprintf(o.opwd_utf8, sizeof o.opwd_utf8, "%s", owner_pw && *owner_pw ? owner_pw : (user_pw ? user_pw : ""));
            int perm = PDF_PERM_PRINT | PDF_PERM_MODIFY | PDF_PERM_COPY | PDF_PERM_ANNOTATE | PDF_PERM_FORM | PDF_PERM_ACCESSIBILITY | PDF_PERM_ASSEMBLE | PDF_PERM_PRINT_HQ;
            if (!allow_print) perm &= ~(PDF_PERM_PRINT | PDF_PERM_PRINT_HQ);
            if (!allow_copy) perm &= ~PDF_PERM_COPY;
            o.permissions = perm;
        } else if (pdf_crypt_version(ctx, h->doc->crypt) > 0 || h->doc->crypt) {
            /* Saved without a password: decrypted. */
            o.do_encrypt = PDF_ENCRYPT_NONE;
        }
        pdf_save_document(ctx, h->doc, out, &o);
    }
    fz_catch(ctx) { set_err(err, errlen, fz_caught_message(ctx)); rc = -1; }
    return rc;
}
