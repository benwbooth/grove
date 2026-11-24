/* FAT resizing for Grove Disk Utility through libparted's filesystem
 * resizer (libparted-fs-resize), the code GParted uses for FAT16/FAT32.
 * Resizes the filesystem in partition `number` of the disk (or image) at
 * `path` to [start, end] (sectors, inclusive) and moves the partition's
 * table entry with it. 0, or -1 with a message in err. */

#include <parted/parted.h>
#include <parted/filesys.h>
#include <stdio.h>
#include <string.h>

static char last_msg[512];

static PedExceptionOption handler(PedException *ex) {
    if (ex->type >= PED_EXCEPTION_ERROR || !last_msg[0])
        snprintf(last_msg, sizeof last_msg, "%s", ex->message ? ex->message : "");
    /* Warnings (e.g. "the filesystem isn't clean") are answered as
     * GParted's non-interactive runs do: ignore where possible. */
    if (ex->type < PED_EXCEPTION_ERROR && (ex->options & PED_EXCEPTION_IGNORE)) return PED_EXCEPTION_IGNORE;
    if (ex->options & PED_EXCEPTION_CANCEL) return PED_EXCEPTION_CANCEL;
    return PED_EXCEPTION_UNHANDLED;
}

static int fail(char *err, int len, const char *what) {
    if (err && len > 0) {
        if (last_msg[0]) snprintf(err, (size_t)len, "%s: %s", what, last_msg);
        else snprintf(err, (size_t)len, "%s", what);
    }
    return -1;
}

int grove_parted_fat_resize(const char *path, int number, long long start, long long end, char *err, int errlen) {
    last_msg[0] = 0;
    ped_exception_set_handler(handler);
    PedDevice *dev = ped_device_get(path);
    if (!dev) return fail(err, errlen, "Couldn't open the disk");
    PedDisk *disk = ped_disk_new(dev);
    if (!disk) { ped_device_destroy(dev); return fail(err, errlen, "Couldn't read the partition table"); }
    int rc = -1;
    PedPartition *part = ped_disk_get_partition(disk, number);
    PedFileSystem *fs = NULL;
    PedGeometry *ng = NULL;
    PedConstraint *c = NULL;
    if (!part) { fail(err, errlen, "There's no such partition"); goto out; }
    if (!ped_device_open(dev)) { fail(err, errlen, "Couldn't open the disk"); goto out; }
    fs = ped_file_system_open(&part->geom);
    if (!fs) { fail(err, errlen, "Couldn't read the FAT filesystem"); goto close; }
    ng = ped_geometry_new(dev, start, end - start + 1);
    if (!ng) { fail(err, errlen, "Bad new size"); goto close; }
    if (!ped_file_system_resize(fs, ng, NULL)) { fail(err, errlen, "Resizing the FAT filesystem failed"); goto close; }
    ped_file_system_close(fs);
    fs = NULL;
    c = ped_constraint_exact(ng);
    if (!c || !ped_disk_set_partition_geom(disk, part, c, start, end)) { fail(err, errlen, "Couldn't change the partition"); goto close; }
    if (!ped_disk_commit_to_dev(disk)) { fail(err, errlen, "Writing the partition table failed"); goto close; }
    /* Tell the kernel (a block device as root; nothing for an image). */
    ped_disk_commit_to_os(disk);
    rc = 0;
close:
    if (fs) ped_file_system_close(fs);
    ped_device_close(dev);
out:
    if (c) ped_constraint_destroy(c);
    if (ng) ped_geometry_destroy(ng);
    ped_disk_destroy(disk);
    ped_device_destroy(dev);
    return rc;
}
