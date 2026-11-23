/* Burn to Disc through linked libraries (no xorriso): libisofs builds the
 * ISO 9660 image (Joliet and Rock Ridge names) from the chosen files, and
 * either it's written to an image file, or libburn writes it to a CD, DVD
 * or Blu-ray, blanking a rewritable disc first when it has to.
 *
 * grove_burn: paths[i] goes on the disc as targets[i] ("/" puts a folder's
 * contents at the top). device NULL or "" writes iso_path. Progress goes
 * to cb (fraction, or -1 with a phase); *cancel stops it. 0 or -1 + err. */

#include <stdint.h>
#include <sys/types.h>
#include <libisofs/libisofs.h>
#include <libburn/libburn.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

typedef void (*grove_burn_progress)(void *user, double fraction, const char *phase);

static void set_err(char *err, int len, const char *msg) {
    if (err && len > 0) snprintf(err, (size_t)len, "%s", msg ? msg : "unknown error");
}

/* libburn's last complaint, if any. */
static void burn_reason(char *err, int len, const char *fallback) {
    char msg[BURN_MSGS_MESSAGE_LEN], sev[80];
    int code = 0, os_errno = 0;
    msg[0] = 0;
    char *last = NULL;
    static char keep[BURN_MSGS_MESSAGE_LEN];
    while (burn_msgs_obtain("SORRY", &code, msg, &os_errno, sev) > 0) {
        snprintf(keep, sizeof keep, "%s", msg);
        last = keep;
    }
    set_err(err, len, last ? last : fallback);
}

static int build_image(IsoImage **img, const char *volid, const char **paths, const char **targets, int n, char *err, int errlen) {
    if (iso_image_new(volid, img) < 0) { set_err(err, errlen, "Couldn't start the disc image."); return -1; }
    IsoDir *root = iso_image_get_root(*img);
    for (int i = 0; i < n; i++) {
        int r;
        if (strcmp(targets[i], "/") == 0) {
            r = iso_tree_add_dir_rec(*img, root, paths[i]);
        } else {
            IsoNode *node = NULL;
            const char *name = targets[i][0] == '/' ? targets[i] + 1 : targets[i];
            r = iso_tree_add_new_node(*img, root, name, paths[i], &node);
            if (r >= 0 && node && iso_node_get_type(node) == LIBISO_DIR) r = iso_tree_add_dir_rec(*img, (IsoDir *)node, paths[i]);
        }
        if (r < 0) {
            char m[600];
            snprintf(m, sizeof m, "Couldn't add “%s” to the disc.", paths[i]);
            set_err(err, errlen, m);
            return -1;
        }
    }
    return 0;
}

int grove_burn(const char **paths, const char **targets, int n, const char *volid, const char *device, const char *iso_path, int simulate, int eject,
               volatile int *cancel, grove_burn_progress cb, void *user, char *err, int errlen) {
    int rc = -1;
    IsoImage *img = NULL;
    IsoWriteOpts *wopts = NULL;
    struct burn_source *src = NULL;
    if (iso_init() < 0) { set_err(err, errlen, "Couldn't start libisofs."); return -1; }
    if (build_image(&img, volid, paths, targets, n, err, errlen) < 0) goto out;
    if (iso_write_opts_new(&wopts, 2) < 0) { set_err(err, errlen, "Couldn't set up the disc image."); goto out; }
    iso_write_opts_set_rockridge(wopts, 1);
    iso_write_opts_set_joliet(wopts, 1);
    if (iso_image_create_burn_source(img, wopts, &src) < 0) { set_err(err, errlen, "Couldn't make the disc image."); goto out; }

    if (!device || !*device) {
        /* Into an image file. */
        FILE *f = fopen(iso_path, "wb");
        if (!f) { set_err(err, errlen, "Couldn't create the image file."); goto out; }
        off_t total = src->get_size(src);
        off_t done = 0;
        unsigned char buf[2048 * 32];
        if (cb) cb(user, -1, "Writing the image");
        for (;;) {
            if (cancel && *cancel) { fclose(f); unlink(iso_path); set_err(err, errlen, "Cancelled"); goto out; }
            int got = src->read_xt ? src->read_xt(src, buf, sizeof buf) : src->read(src, buf, sizeof buf);
            if (got < 0) { fclose(f); unlink(iso_path); set_err(err, errlen, "Couldn't make the disc image."); goto out; }
            if (got == 0) break;
            if (fwrite(buf, 1, (size_t)got, f) != (size_t)got) { fclose(f); unlink(iso_path); set_err(err, errlen, "Couldn't write the image file (is the disk full?)."); goto out; }
            done += got;
            if (cb && total > 0) cb(user, (double)done / (double)total, "");
        }
        if (fclose(f) != 0) { unlink(iso_path); set_err(err, errlen, "Couldn't write the image file."); goto out; }
        rc = 0;
        goto out;
    }

    /* Onto a disc. */
    if (!burn_initialize()) { set_err(err, errlen, "Couldn't start libburn."); goto out; }
    burn_msgs_set_severities("NEVER", "NEVER", "grove");
    struct burn_drive_info *infos = NULL;
    if (burn_drive_scan_and_grab(&infos, (char *)device, 1) <= 0 || !infos) {
        burn_reason(err, errlen, "grove couldn't use the drive (your user needs to be in the cdrom or optical group).");
        burn_finish();
        goto out;
    }
    struct burn_drive *drive = infos[0].drive;
    struct burn_progress p;
    while (burn_drive_get_status(drive, NULL) != BURN_DRIVE_IDLE) usleep(100000);
    enum burn_disc_status ds = burn_disc_get_status(drive);
    if (ds == BURN_DISC_EMPTY) {
        set_err(err, errlen, "no media");
        burn_drive_release(drive, 0);
        burn_drive_info_free(infos);
        burn_finish();
        goto out;
    }
    if (ds != BURN_DISC_BLANK) {
        if (!burn_disc_erasable(drive)) {
            set_err(err, errlen, "closed media");
            burn_drive_release(drive, 0);
            burn_drive_info_free(infos);
            burn_finish();
            goto out;
        }
        if (cb) cb(user, -1, "Blanking the disc");
        burn_disc_erase(drive, 1);
        usleep(500000);
        while (burn_drive_get_status(drive, &p) != BURN_DRIVE_IDLE) usleep(300000);
    }
    struct burn_disc *disc = burn_disc_create();
    struct burn_session *session = burn_session_create();
    struct burn_track *track = burn_track_create();
    burn_disc_add_session(disc, session, BURN_POS_END);
    burn_track_define_data(track, 0, 0, 1, BURN_MODE1);
    burn_track_set_source(track, src);
    burn_session_add_track(session, track, BURN_POS_END);
    struct burn_write_opts *o = burn_write_opts_new(drive);
    char reasons[BURN_REASONS_LEN];
    burn_write_opts_set_perform_opc(o, 0);
    burn_write_opts_set_underrun_proof(o, 1);
    burn_write_opts_set_simulate(o, simulate ? 1 : 0);
    if (burn_write_opts_auto_write_type(o, disc, reasons, 0) == BURN_WRITE_NONE) {
        set_err(err, errlen, "The disc in the drive can't be written this way.");
    } else {
        burn_drive_set_speed(drive, 0, 0);
        if (cb) cb(user, -1, "Writing");
        burn_disc_write(o, disc);
        usleep(500000);
        while (burn_drive_get_status(drive, &p) != BURN_DRIVE_IDLE) {
            if (cancel && *cancel) burn_drive_cancel(drive);
            if (cb && p.sectors > 0) cb(user, (double)p.sector / (double)p.sectors, "");
            usleep(300000);
        }
        if (cancel && *cancel) set_err(err, errlen, "Cancelled");
        else if (burn_drive_wrote_well(drive)) rc = 0;
        else burn_reason(err, errlen, "The burn didn't finish.");
    }
    burn_write_opts_free(o);
    burn_track_free(track);
    burn_session_free(session);
    burn_disc_free(disc);
    burn_drive_release(drive, eject && rc == 0);
    burn_drive_info_free(infos);
    burn_finish();
out:
    if (src) burn_source_free(src);
    if (wopts) iso_write_opts_free(wopts);
    if (img) iso_image_unref(img);
    iso_finish();
    return rc;
}
