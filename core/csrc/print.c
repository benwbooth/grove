/* Printing through libcups (no lp/lpstat commands).
 *
 *   grove_print_dests  the printers: JSON [{name, info, location, default,
 *                      state, media:[...], color}]
 *   grove_print_file   send a file (PDF or picture) with options
 *
 * JSON answers are malloc'ed: free with grove_print_free. */

#include <cups/cups.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

void grove_print_free(char *p) { free(p); }

typedef struct { char *s; size_t n, cap; } sbuf;

static void sb_add(sbuf *b, const char *t) {
    size_t l = strlen(t);
    if (b->n + l + 1 > b->cap) {
        size_t c = (b->cap ? b->cap * 2 : 1024);
        while (c < b->n + l + 1) c *= 2;
        char *ns = realloc(b->s, c);
        if (!ns) return;
        b->s = ns;
        b->cap = c;
    }
    memcpy(b->s + b->n, t, l + 1);
    b->n += l;
}

static void sb_str(sbuf *b, const char *s) {
    sb_add(b, "\"");
    char one[8];
    for (const unsigned char *p = (const unsigned char *)(s ? s : ""); *p; p++) {
        if (*p == '"' || *p == '\\') { one[0] = '\\'; one[1] = (char)*p; one[2] = 0; }
        else if (*p < 0x20) snprintf(one, sizeof one, "\\u%04x", *p);
        else { one[0] = (char)*p; one[1] = 0; }
        sb_add(b, one);
    }
    sb_add(b, "\"");
}

char *grove_print_dests(void) {
    cups_dest_t *dests = NULL;
    int n = cupsGetDests2(CUPS_HTTP_DEFAULT, &dests);
    sbuf b = { 0 };
    sb_add(&b, "[");
    for (int i = 0; i < n; i++) {
        cups_dest_t *d = &dests[i];
        if (d->instance) continue;
        if (b.n > 1) sb_add(&b, ",");
        sb_add(&b, "{\"name\":");
        sb_str(&b, d->name);
        sb_add(&b, ",\"info\":");
        sb_str(&b, cupsGetOption("printer-info", d->num_options, d->options));
        sb_add(&b, ",\"location\":");
        sb_str(&b, cupsGetOption("printer-location", d->num_options, d->options));
        sb_add(&b, ",\"model\":");
        sb_str(&b, cupsGetOption("printer-make-and-model", d->num_options, d->options));
        const char *st = cupsGetOption("printer-state", d->num_options, d->options);
        sb_add(&b, ",\"state\":");
        sb_add(&b, st ? st : "3");
        sb_add(&b, d->is_default ? ",\"default\":true" : ",\"default\":false");
        const char *types = cupsGetOption("printer-type", d->num_options, d->options);
        long t = types ? strtol(types, NULL, 10) : 0;
        sb_add(&b, (t & CUPS_PRINTER_COLOR) ? ",\"color\":true" : ",\"color\":false");
        sb_add(&b, (t & CUPS_PRINTER_DUPLEX) ? ",\"duplex\":true" : ",\"duplex\":false");
        /* The paper sizes it takes. */
        sb_add(&b, ",\"media\":[");
        http_t *http = NULL;
        cups_dinfo_t *info = cupsCopyDestInfo(CUPS_HTTP_DEFAULT, d);
        if (info) {
            int count = cupsGetDestMediaCount(http, d, info, 0);
            int first = 1;
            for (int k = 0; k < count && k < 80; k++) {
                cups_size_t sz;
                if (!cupsGetDestMediaByIndex(http, d, info, k, 0, &sz)) continue;
                if (!first) sb_add(&b, ",");
                first = 0;
                char num[96];
                sb_add(&b, "{\"name\":");
                sb_str(&b, sz.media);
                snprintf(num, sizeof num, ",\"w\":%d,\"l\":%d}", sz.width, sz.length);
                sb_add(&b, num);
            }
            cups_size_t def;
            if (cupsGetDestMediaDefault(http, d, info, 0, &def)) {
                sb_add(&b, "],\"defaultMedia\":");
                sb_str(&b, def.media);
            } else sb_add(&b, "],\"defaultMedia\":\"\"");
            cupsFreeDestInfo(info);
        } else {
            sb_add(&b, "],\"defaultMedia\":\"\"");
        }
        sb_add(&b, "}");
    }
    sb_add(&b, "]");
    cupsFreeDests(n, dests);
    return b.s;
}

/* Print `path`. opts: "name=value" pairs separated by newlines (copies,
 * page-ranges, sides, print-color-mode, media, orientation-requested,
 * fit-to-page, number-up...). Returns the job id, or 0 with err. */
int grove_print_file(const char *printer, const char *path, const char *title, const char *opts, char *err, int errlen) {
    cups_option_t *o = NULL;
    int no = 0;
    if (opts) {
        char *copy = strdup(opts);
        for (char *line = strtok(copy, "\n"); line; line = strtok(NULL, "\n")) {
            char *eq = strchr(line, '=');
            if (!eq) continue;
            *eq = 0;
            no = cupsAddOption(line, eq + 1, no, &o);
        }
        free(copy);
    }
    int id = cupsPrintFile2(CUPS_HTTP_DEFAULT, printer, path, title && *title ? title : "Document", no, o);
    cupsFreeOptions(no, o);
    if (id == 0 && err && errlen > 0) snprintf(err, (size_t)errlen, "%s", cupsLastErrorString());
    return id;
}
