//! Grove Disk Utility's work on a disk's bytes (no D-Bus): moving a
//! partition (its data, then its entry in the partition table), copying
//! one partition into another, and resizing FAT through libparted's
//! filesystem resizer (csrc/parted.c), as GParted does. Each takes an open
//! device (from UDisks2's OpenDevice/OpenForBackup/OpenForRestore, so
//! polkit decides who may) or, for FAT, a device path the root helper
//! opens. Unit-tested on image files.

use std::fs::File;
use std::io::{Seek, SeekFrom};
use std::os::unix::fs::FileExt;
use std::sync::atomic::{AtomicBool, Ordering};

const CHUNK: u64 = 4 << 20;

/// Copy `len` bytes from `from` to `to` within one device, safely when
/// the two overlap (backwards when moving right). progress(done, total).
/// A move can't stop half way without losing data, so it isn't cancellable.
pub fn move_bytes(f: &File, from: u64, to: u64, len: u64, _cancel: &AtomicBool, progress: &mut dyn FnMut(u64, u64)) -> Result<(), String> {
    if from == to || len == 0 {
        return Ok(());
    }
    let mut buf = vec![0u8; CHUNK as usize];
    let mut done = 0u64;
    while done < len {
        let n = CHUNK.min(len - done);
        // Moving right: the last chunk first.
        let off = if to > from { len - done - n } else { done };
        let b = &mut buf[..n as usize];
        f.read_exact_at(b, from + off).map_err(|e| format!("Reading the disk failed: {e}"))?;
        f.write_all_at(b, to + off).map_err(|e| format!("Writing the disk failed: {e}"))?;
        done += n;
        progress(done, len);
    }
    f.sync_all().map_err(|e| format!("Writing the disk failed: {e}"))
}

/// Copy a whole partition into another (paste): `len` bytes of `src` from
/// `src_off` into `dst` at `dst_off` (0 for partition devices; the
/// partitions' offsets when both are disk images).
pub fn copy_device(src: &File, src_off: u64, dst: &File, dst_off: u64, len: u64, cancel: &AtomicBool, progress: &mut dyn FnMut(u64, u64)) -> Result<(), String> {
    let mut buf = vec![0u8; CHUNK as usize];
    let mut done = 0u64;
    while done < len {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        let n = CHUNK.min(len - done) as usize;
        src.read_exact_at(&mut buf[..n], src_off + done).map_err(|e| format!("Reading the partition failed: {e}"))?;
        dst.write_all_at(&buf[..n], dst_off + done).map_err(|e| format!("Writing the copy failed: {e}"))?;
        done += n as u64;
        progress(done, len);
    }
    dst.sync_all().map_err(|e| format!("Writing the copy failed: {e}"))
}

/// Point partition `number`'s table entry at a new place. It must still
/// be where `old_offset`/`old_size` say (else nothing is written).
pub fn set_geometry(f: &mut File, table: &str, number: u32, old_offset: u64, old_size: u64, offset: u64, size: u64) -> Result<(), String> {
    match table {
        "gpt" => {
            f.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
            let mut gpt = gptman::GPT::find_from(f).map_err(|e| format!("Couldn't read the GPT partition table: {e}"))?;
            let ss = gpt.sector_size;
            let (first, last) = (gpt.header.first_usable_lba, gpt.header.last_usable_lba);
            if number == 0 || number > gpt.header.number_of_partition_entries {
                return Err(format!("There's no partition {number}."));
            }
            let e = &mut gpt[number];
            if !e.is_used() || e.starting_lba * ss != old_offset || (e.ending_lba + 1 - e.starting_lba) * ss != old_size {
                return Err("The partition table changed since Grove read it. Nothing was written.".into());
            }
            if offset % ss != 0 || size % ss != 0 || size == 0 {
                return Err("The new place doesn't fall on whole sectors.".into());
            }
            e.starting_lba = offset / ss;
            e.ending_lba = (offset + size) / ss - 1;
            if e.starting_lba < first || e.ending_lba > last {
                return Err("The new place is outside the disk's usable space.".into());
            }
            f.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
            gpt.write_into(f).map_err(|e| format!("Writing the partition table failed: {e}"))?;
        }
        "dos" => {
            if !(1..=4).contains(&number) {
                return Err("Only primary partitions can be moved on an MBR disk.".into());
            }
            let mut mbr = [0u8; 512];
            f.read_exact_at(&mut mbr, 0).map_err(|e| e.to_string())?;
            if mbr[510] != 0x55 || mbr[511] != 0xaa {
                return Err("Couldn't read the MBR partition table.".into());
            }
            let at = 446 + 16 * (number as usize - 1);
            let rd = |o: usize| u32::from_le_bytes(mbr[at + o..at + o + 4].try_into().unwrap()) as u64;
            if rd(8) * 512 != old_offset || rd(12) * 512 != old_size {
                return Err("The partition table changed since Grove read it. Nothing was written.".into());
            }
            let (s, n) = (offset / 512, size / 512);
            if s > u32::MAX as u64 || n > u32::MAX as u64 || offset % 512 != 0 || size % 512 != 0 {
                return Err("An MBR disk can't address that far; use GPT.".into());
            }
            mbr[at + 8..at + 12].copy_from_slice(&(s as u32).to_le_bytes());
            mbr[at + 12..at + 16].copy_from_slice(&(n as u32).to_le_bytes());
            // Cylinder/head/sector fields: "use the LBA values" (as fdisk
            // writes them past 8 GB).
            mbr[at + 1..at + 4].copy_from_slice(&[0xfe, 0xff, 0xff]);
            mbr[at + 5..at + 8].copy_from_slice(&[0xfe, 0xff, 0xff]);
            f.write_all_at(&mbr, 0).map_err(|e| format!("Writing the partition table failed: {e}"))?;
        }
        _ => return Err("This disk has no partition table.".into()),
    }
    f.sync_all().map_err(|e| e.to_string())
}

/// Move partition `number` (data, then table entry). The partition must
/// not be mounted. `f` is the whole disk, open for reading and writing.
pub fn move_partition(f: &mut File, table: &str, number: u32, old_offset: u64, size: u64, new_offset: u64, cancel: &AtomicBool, progress: &mut dyn FnMut(u64, u64)) -> Result<(), String> {
    // Check the table entry before touching any data.
    {
        let mut probe = f.try_clone().map_err(|e| e.to_string())?;
        check_entry(&mut probe, table, number, old_offset, size)?;
    }
    move_bytes(f, old_offset, new_offset, size, cancel, progress)?;
    set_geometry(f, table, number, old_offset, size, new_offset, size)
}

fn check_entry(f: &mut File, table: &str, number: u32, offset: u64, size: u64) -> Result<(), String> {
    match table {
        "gpt" => {
            f.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
            let gpt = gptman::GPT::find_from(f).map_err(|e| format!("Couldn't read the GPT partition table: {e}"))?;
            let ss = gpt.sector_size;
            if number == 0 || number > gpt.header.number_of_partition_entries {
                return Err(format!("There's no partition {number}."));
            }
            let e = &gpt[number];
            if !e.is_used() || e.starting_lba * ss != offset || (e.ending_lba + 1 - e.starting_lba) * ss != size {
                return Err("The partition table changed since Grove read it. Nothing was written.".into());
            }
            Ok(())
        }
        "dos" => {
            if !(1..=4).contains(&number) {
                return Err("Only primary partitions can be moved on an MBR disk.".into());
            }
            let mut mbr = [0u8; 512];
            f.read_exact_at(&mut mbr, 0).map_err(|e| e.to_string())?;
            let at = 446 + 16 * (number as usize - 1);
            let rd = |o: usize| u32::from_le_bytes(mbr[at + o..at + o + 4].try_into().unwrap()) as u64;
            if mbr[510] != 0x55 || rd(8) * 512 != offset || rd(12) * 512 != size {
                return Err("The partition table changed since Grove read it. Nothing was written.".into());
            }
            Ok(())
        }
        _ => Err("This disk has no partition table.".into()),
    }
}

// ---- FAT resizing (libparted's filesystem resizer) ----

extern "C" {
    fn grove_parted_fat_resize(path: *const libc::c_char, number: libc::c_int, start: i64, end: i64, err: *mut libc::c_char, errlen: libc::c_int) -> libc::c_int;
}

/// Resize the FAT filesystem in partition `number` of the disk (or disk
/// image) at `path`, and the partition with it, to [start, end] sectors
/// (512-byte, inclusive). Run as root through the root helper for real
/// disks.
pub fn fat_resize(path: &str, number: u32, start: i64, end: i64) -> Result<(), String> {
    let c = std::ffi::CString::new(path).map_err(|e| e.to_string())?;
    let mut err = vec![0 as libc::c_char; 1024];
    let r = unsafe { grove_parted_fat_resize(c.as_ptr(), number as libc::c_int, start, end, err.as_mut_ptr(), err.len() as libc::c_int) };
    if r == 0 {
        Ok(())
    } else {
        let msg = unsafe { std::ffi::CStr::from_ptr(err.as_ptr()) }.to_string_lossy().into_owned();
        Err(if msg.is_empty() { "Resizing the FAT filesystem failed.".into() } else { msg })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const MIB: u64 = 1 << 20;

    fn pattern(i: u64) -> u8 {
        (i.wrapping_mul(2654435761) >> 7) as u8
    }

    fn gpt_image(dir: &std::path::Path) -> (std::path::PathBuf, File) {
        let p = dir.join("disk.img");
        let mut f = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&p).unwrap();
        f.set_len(64 * MIB).unwrap();
        let mut gpt = gptman::GPT::new_from(&mut f, 512, [7u8; 16]).unwrap();
        gpt[1] = gptman::GPTPartitionEntry {
            partition_type_guid: [1u8; 16],
            unique_partition_guid: [2u8; 16],
            starting_lba: 2048,
            ending_lba: 2048 + 8 * 2048 - 1,
            attribute_bits: 0,
            partition_name: "data".into(),
        };
        gpt.write_into(&mut f).unwrap();
        // Data in the partition.
        let data: Vec<u8> = (0..8 * MIB).map(pattern).collect();
        f.write_all_at(&data, MIB).unwrap();
        f.flush().unwrap();
        (p, f)
    }

    fn read_back(f: &File, at: u64) -> bool {
        let mut b = vec![0u8; 8 * MIB as usize];
        f.read_exact_at(&mut b, at).unwrap();
        b.iter().enumerate().all(|(i, &x)| x == pattern(i as u64))
    }

    #[test]
    fn move_right_overlapping_keeps_data() {
        let t = tempfile::tempdir().unwrap();
        let (_, mut f) = gpt_image(t.path());
        let c = AtomicBool::new(false);
        let mut steps = 0;
        move_partition(&mut f, "gpt", 1, MIB, 8 * MIB, 3 * MIB, &c, &mut |_, _| steps += 1).unwrap();
        assert!(read_back(&f, 3 * MIB));
        assert!(steps >= 2);
        f.seek(SeekFrom::Start(0)).unwrap();
        let gpt = gptman::GPT::find_from(&mut f).unwrap();
        assert_eq!(gpt[1].starting_lba, 3 * 2048);
        assert_eq!(gpt[1].ending_lba, 11 * 2048 - 1);
        assert_eq!(gpt[1].partition_name.as_str(), "data");
        // And back left, overlapping again.
        move_partition(&mut f, "gpt", 1, 3 * MIB, 8 * MIB, 2 * MIB, &c, &mut |_, _| {}).unwrap();
        assert!(read_back(&f, 2 * MIB));
    }

    #[test]
    fn refuses_a_changed_table() {
        let t = tempfile::tempdir().unwrap();
        let (_, mut f) = gpt_image(t.path());
        let c = AtomicBool::new(false);
        let e = move_partition(&mut f, "gpt", 1, 2 * MIB, 8 * MIB, 20 * MIB, &c, &mut |_, _| {}).unwrap_err();
        assert!(e.contains("changed"));
        // Nothing moved.
        assert!(read_back(&f, MIB));
    }

    #[test]
    fn mbr_entry_moves() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("mbr.img");
        let f = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&p).unwrap();
        f.set_len(32 * MIB).unwrap();
        let mut mbr = [0u8; 512];
        mbr[510] = 0x55;
        mbr[511] = 0xaa;
        mbr[446 + 4] = 0x83;
        mbr[446 + 8..446 + 12].copy_from_slice(&2048u32.to_le_bytes());
        mbr[446 + 12..446 + 16].copy_from_slice(&4096u32.to_le_bytes());
        f.write_all_at(&mbr, 0).unwrap();
        let mut f = f;
        set_geometry(&mut f, "dos", 1, MIB, 2 * MIB, 4 * MIB, 2 * MIB).unwrap();
        let mut b = [0u8; 16];
        f.read_exact_at(&mut b, 446).unwrap();
        assert_eq!(u32::from_le_bytes(b[8..12].try_into().unwrap()), 8192);
        assert_eq!(b[4], 0x83);
        assert!(set_geometry(&mut f, "dos", 5, MIB, 2 * MIB, 4 * MIB, 2 * MIB).is_err());
    }

    #[test]
    fn copies_a_partition() {
        let t = tempfile::tempdir().unwrap();
        let a = File::create(t.path().join("a")).unwrap();
        let data: Vec<u8> = (0..(5 * MIB + 3)).map(pattern).collect();
        a.write_all_at(&data, 0).unwrap();
        let a = File::open(t.path().join("a")).unwrap();
        let b = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(t.path().join("b")).unwrap();
        copy_device(&a, 0, &b, 0, data.len() as u64, &AtomicBool::new(false), &mut |_, _| {}).unwrap();
        assert_eq!(std::fs::read(t.path().join("b")).unwrap(), data);
    }

    /// FAT32 grown and shrunk through libparted (needs mkfs.fat to make one).
    #[test]
    fn fat_resize_grows_and_shrinks() {
        let Some(mkfs) = std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join("mkfs.fat")).find(|p| p.exists())) else {
            eprintln!("mkfs.fat not found: skipped");
            return;
        };
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("fat.img");
        let mut f = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&p).unwrap();
        f.set_len(1024 * MIB).unwrap();
        let mut gpt = gptman::GPT::new_from(&mut f, 512, [7u8; 16]).unwrap();
        gpt[1] = gptman::GPTPartitionEntry { partition_type_guid: [0xa2; 16], unique_partition_guid: [2u8; 16], starting_lba: 2048, ending_lba: 2048 + 300 * 2048 - 1, attribute_bits: 0, partition_name: "fat".into() };
        gpt.write_into(&mut f).unwrap();
        gptman::GPT::write_protective_mbr_into(&mut f, 512).unwrap();
        drop(f);
        let ok = std::process::Command::new(&mkfs).args(["-F", "32", "--offset", "2048", "-n", "TESTFAT"]).arg(&p).arg((300 * 1024).to_string()).status().unwrap();
        assert!(ok.success());
        let total = |f: &File| {
            let mut b = [0u8; 512];
            f.read_exact_at(&mut b, MIB).unwrap();
            u32::from_le_bytes(b[32..36].try_into().unwrap()) as u64
        };
        fat_resize(p.to_str().unwrap(), 1, 2048, 2048 + 600 * 2048 - 1).unwrap();
        let mut f = std::fs::OpenOptions::new().read(true).write(true).open(&p).unwrap();
        assert!(total(&f) > 590 * 2048, "{}", total(&f));
        let gpt = gptman::GPT::find_from(&mut f).unwrap();
        assert_eq!(gpt[1].ending_lba, 2048 + 600 * 2048 - 1);
        fat_resize(p.to_str().unwrap(), 1, 2048, 2048 + 400 * 2048 - 1).unwrap();
        let f = File::open(&p).unwrap();
        assert!(total(&f) <= 400 * 2048);
        let fsck = std::process::Command::new(mkfs.with_file_name("fsck.fat")).args(["-n", "--offset"]).arg((2048 * 512).to_string()).arg(&p).output();
        if let Ok(o) = fsck { let _ = o; }
    }
}
