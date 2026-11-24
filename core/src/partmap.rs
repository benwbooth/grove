//! Grove Disk Utility's model (no Qt, no D-Bus): a disk's partition map,
//! the queue of pending operations (GParted's: create, delete,
//! resize/move, format, label, name, flags, new partition table, check,
//! paste), each checked and played out on a copy of the map so the window
//! shows the disk as it will be. src/disks.rs reads disks from UDisks2 and
//! carries the queue out; nothing here touches a device.
//!
//! Offsets and sizes are bytes. New partitions start and end on 1 MiB
//! boundaries, as GParted and every current partitioner place them.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const MIB: u64 = 1 << 20;

pub fn align_up(v: u64) -> u64 {
    v.div_ceil(MIB) * MIB
}
pub fn align_down(v: u64) -> u64 {
    v / MIB * MIB
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Primary,
    Extended,
    Logical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Part {
    /// The UDisks2 object of an existing partition, "new:N" for one the
    /// queue creates.
    pub id: String,
    #[serde(default)]
    pub number: u32,
    pub offset: u64,
    pub size: u64,
    #[serde(default)]
    pub kind: Kind,
    /// Filesystem (udisks' IdType: ext4, vfat, ntfs, exfat, btrfs, xfs,
    /// f2fs, swap, crypto_LUKS, LVM2_member...), "" when there's none.
    #[serde(default)]
    pub fs: String,
    #[serde(default)]
    pub label: String,
    /// GPT partition name.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub uuid: String,
    /// Partition type: a GPT type GUID or an MBR "0x83".
    #[serde(default)]
    pub ptype: String,
    /// Attribute bits (GPT) or 0x80 for bootable (MBR).
    #[serde(default)]
    pub attrs: u64,
    #[serde(default)]
    pub mounts: Vec<String>,
    /// Bytes in use, when known (mounted: statvfs; else the superblock).
    #[serde(default)]
    pub used: Option<u64>,
    #[serde(default)]
    pub device: String,
    /// In use by the system (mounted, active swap, an open LUKS or LVM
    /// member): nothing may change it until it's let go.
    #[serde(default)]
    pub busy: bool,
    /// Changed by the queue (shown highlighted).
    #[serde(default)]
    pub pending: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Disk {
    /// The UDisks2 block object of the whole disk.
    pub id: String,
    pub device: String,
    #[serde(default)]
    pub model: String,
    pub size: u64,
    #[serde(default = "default_sector")]
    pub sector: u64,
    /// "gpt", "dos" or "" (no partition table).
    #[serde(default)]
    pub table: String,
    #[serde(default)]
    pub parts: Vec<Part>,
    /// Holds the running system (/, /home, /boot, /nix): locked.
    #[serde(default)]
    pub system: bool,
    #[serde(default)]
    pub removable: bool,
    #[serde(default)]
    pub read_only: bool,
    /// A loop device's backing file.
    #[serde(default)]
    pub loop_file: String,
    /// A filesystem straight on the disk, without a partition table.
    #[serde(default)]
    pub whole_fs: String,
    #[serde(default)]
    pub whole_label: String,
    #[serde(default)]
    pub connection: String,
}

fn default_sector() -> u64 {
    512
}

/// What each filesystem allows (from udisks' CanFormat/CanResize/... plus
/// FAT resizing through libparted, as GParted does).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FsCaps {
    pub format: bool,
    pub grow: bool,
    pub shrink: bool,
    /// Resizing needs it mounted (btrfs): mounted for the resize only.
    pub online_only: bool,
    pub check: bool,
    pub repair: bool,
    pub label: bool,
    #[serde(default)]
    pub missing: String,
}

pub type Caps = std::collections::BTreeMap<String, FsCaps>;

/// The filesystems Grove offers, in menu order, with a plain description.
pub const FILESYSTEMS: &[(&str, &str, &str)] = &[
    ("ext4", "ext4", "Linux's standard. Use for Linux-only disks."),
    ("btrfs", "Btrfs", "Linux, with snapshots, compression and checksums."),
    ("xfs", "XFS", "Linux, for very large files. Can grow but never shrink."),
    ("f2fs", "F2FS", "Linux, made for flash memory (SD cards, USB sticks)."),
    ("exfat", "exFAT", "Works on Windows, macOS, Linux, cameras and TVs. Files over 4 GB are fine."),
    ("vfat", "FAT32", "Works almost everywhere (old cameras, car stereos). Files up to 4 GB."),
    ("ntfs", "NTFS", "Windows' own. Readable and writable on Linux; read-only on macOS."),
    ("ext3", "ext3", "Older Linux, with a journal."),
    ("ext2", "ext2", "Oldest Linux, no journal."),
    ("udf", "UDF", "For optical media and some players."),
    ("swap", "Linux swap", "Extra memory on disk for Linux."),
    ("empty", "Unformatted", "No filesystem: the space is left empty."),
];

pub fn fs_name(fs: &str) -> String {
    match fs {
        "" => "unknown".into(),
        "crypto_LUKS" => "LUKS (encrypted)".into(),
        "LVM2_member" => "LVM".into(),
        "linux_raid_member" => "RAID member".into(),
        "unallocated" => "unallocated".into(),
        _ => FILESYSTEMS.iter().find(|f| f.0 == fs).map(|f| f.1.to_string()).unwrap_or_else(|| fs.to_string()),
    }
}

/// The smallest a new filesystem of this kind can be.
pub fn fs_min(fs: &str) -> u64 {
    match fs {
        "vfat" => 33 * MIB,
        "btrfs" => 256 * MIB,
        "xfs" => 300 * MIB,
        "f2fs" => 64 * MIB,
        "ext2" | "ext3" | "ext4" => 8 * MIB,
        "ntfs" => 2 * MIB,
        _ => MIB,
    }
}

/// The largest (FAT32: 2 TiB with 512-byte sectors).
pub fn fs_max(fs: &str) -> u64 {
    match fs {
        "vfat" => 2048 * 1024 * MIB,
        _ => u64::MAX,
    }
}

pub fn label_max(fs: &str) -> usize {
    match fs {
        "vfat" => 11,
        "ntfs" => 128,
        "exfat" => 15,
        "ext2" | "ext3" | "ext4" => 16,
        "xfs" => 12,
        "btrfs" => 255,
        "f2fs" => 512,
        "swap" => 16,
        _ => 64,
    }
}

// ---- partition types and flags (GParted's names) ----

pub const GPT_LINUX: &str = "0fc63daf-8483-4772-8e79-3d69d8477de4";
pub const GPT_BASIC: &str = "ebd0a0a2-b9e5-4433-87c0-68b6b72699c7";
pub const GPT_SWAP: &str = "0657fd6d-a4ab-43c4-84e5-0933c84b4f4f";
pub const GPT_ESP: &str = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b";

/// Flags that are partition types: setting one sets the type.
const GPT_TYPE_FLAGS: &[(&str, &str)] = &[
    ("esp", GPT_ESP),
    ("bios_grub", "21686148-6449-6e6f-744e-656564454649"),
    ("msftres", "e3c9e316-0b5c-4db8-817d-f92df00215ae"),
    ("diag", "de94bba4-06d1-4d40-a16a-bfd50179d6ac"),
    ("lvm", "e6d6d379-f507-44c2-a23c-238f2a3df928"),
    ("raid", "a19d880f-05fc-4d3b-a006-743f0f84911e"),
    ("swap", GPT_SWAP),
    ("linux-home", "933ac7e1-2eb4-4f13-b844-0e14e2aef915"),
    ("msftdata", GPT_BASIC),
];
/// GPT attribute bits.
const GPT_ATTR_FLAGS: &[(&str, u32)] = &[("required", 0), ("legacy_boot", 2), ("hidden", 62), ("no_automount", 63)];

const MBR_TYPE_FLAGS: &[(&str, &str)] = &[("esp", "0xef"), ("lvm", "0x8e"), ("raid", "0xfd"), ("swap", "0x82"), ("diag", "0x27"), ("prep", "0x41")];

/// The flags a partition table offers, in GParted's order.
pub fn flag_names(table: &str) -> Vec<&'static str> {
    if table == "gpt" {
        let mut v: Vec<&str> = GPT_TYPE_FLAGS.iter().map(|f| f.0).collect();
        v.extend(GPT_ATTR_FLAGS.iter().map(|f| f.0));
        v
    } else {
        let mut v = vec!["boot", "hidden", "lba"];
        v.extend(MBR_TYPE_FLAGS.iter().map(|f| f.0));
        v
    }
}

pub fn flag_help(flag: &str) -> &'static str {
    match flag {
        "boot" => "The partition the computer starts from (BIOS computers)",
        "esp" => "EFI System Partition: holds the boot loaders UEFI computers start",
        "bios_grub" => "Space for GRUB on a GPT disk started by an old BIOS",
        "msftres" => "Microsoft Reserved: space Windows keeps for itself",
        "msftdata" => "Windows data partition (the usual type for NTFS, FAT and exFAT)",
        "diag" => "A recovery or diagnostics partition (Windows Recovery)",
        "lvm" => "A member of an LVM volume group",
        "raid" => "A member of a software RAID array",
        "swap" => "Linux swap space",
        "linux-home" => "Linux /home: some systems mount it there by itself",
        "required" => "The firmware needs it: tools should leave it alone",
        "legacy_boot" => "Old BIOS boot loaders may start from it",
        "hidden" => "Windows and file managers won't show it",
        "no_automount" => "Windows won't give it a drive letter",
        "lba" => "Uses large-disk addressing (normal for FAT and NTFS)",
        "prep" => "PowerPC boot partition",
        _ => "",
    }
}

fn is_fat_ntfs(fs: &str) -> bool {
    matches!(fs, "vfat" | "ntfs" | "exfat")
}

/// The type a partition with this filesystem should have.
pub fn default_ptype(table: &str, fs: &str, kind: Kind) -> String {
    if table == "gpt" {
        return match fs {
            "swap" => GPT_SWAP,
            f if is_fat_ntfs(f) => GPT_BASIC,
            _ => GPT_LINUX,
        }
        .into();
    }
    if kind == Kind::Extended {
        return "0x0f".into();
    }
    match fs {
        "swap" => "0x82",
        "vfat" => "0x0c",
        "ntfs" | "exfat" => "0x07",
        _ => "0x83",
    }
    .into()
}

/// The flags set on a partition.
pub fn flags_of(table: &str, p: &Part) -> Vec<String> {
    let t = p.ptype.to_ascii_lowercase();
    let mut out = vec![];
    if table == "gpt" {
        for (n, g) in GPT_TYPE_FLAGS {
            if t == *g {
                out.push(n.to_string());
            }
        }
        for (n, b) in GPT_ATTR_FLAGS {
            if p.attrs & (1u64 << b) != 0 {
                out.push(n.to_string());
            }
        }
    } else {
        if p.attrs & 0x80 != 0 {
            out.push("boot".into());
        }
        let v = u8::from_str_radix(t.trim_start_matches("0x"), 16).unwrap_or(0);
        if matches!(v, 0x11 | 0x14 | 0x16 | 0x17 | 0x1b | 0x1c | 0x1e) {
            out.push("hidden".into());
        }
        if matches!(v & !0x10, 0x0c | 0x0e) {
            out.push("lba".into());
        }
        for (n, x) in MBR_TYPE_FLAGS {
            if t == *x {
                out.push(n.to_string());
            }
        }
    }
    out
}

/// The type and attributes giving a partition exactly these flags.
pub fn apply_flags(table: &str, p: &Part, flags: &[String]) -> (String, u64) {
    let has = |f: &str| flags.iter().any(|x| x == f);
    if table == "gpt" {
        let mut attrs = p.attrs;
        for (n, b) in GPT_ATTR_FLAGS {
            if has(n) {
                attrs |= 1u64 << b;
            } else {
                attrs &= !(1u64 << b);
            }
        }
        let cur = p.ptype.to_ascii_lowercase();
        // A newly set type flag wins over the one already there.
        let newly = GPT_TYPE_FLAGS.iter().find(|(n, g)| has(n) && cur != *g);
        let kept = GPT_TYPE_FLAGS.iter().find(|(n, g)| has(n) && cur == *g);
        let ptype = match (newly, kept) {
            (Some((_, g)), _) => g.to_string(),
            (None, Some(_)) => cur.clone(),
            (None, None) if GPT_TYPE_FLAGS.iter().any(|(_, g)| cur == *g) => default_ptype("gpt", &p.fs, p.kind),
            _ => cur.clone(),
        };
        // "msftdata" cleared on a basic data partition: Linux's type.
        let ptype = if ptype == GPT_BASIC && cur == GPT_BASIC && !has("msftdata") { GPT_LINUX.to_string() } else { ptype };
        return (ptype, attrs);
    }
    let attrs = if has("boot") { p.attrs | 0x80 } else { p.attrs & !0x80 };
    if p.kind == Kind::Extended {
        return (p.ptype.clone(), attrs);
    }
    let cur = p.ptype.to_ascii_lowercase();
    let newly = MBR_TYPE_FLAGS.iter().find(|(n, x)| has(n) && cur != *x);
    let kept = MBR_TYPE_FLAGS.iter().find(|(n, x)| has(n) && cur == *x);
    let mut ptype = match (newly, kept) {
        (Some((_, x)), _) => x.to_string(),
        (None, Some(_)) => cur.clone(),
        (None, None) if MBR_TYPE_FLAGS.iter().any(|(_, x)| cur == *x) => default_ptype("dos", &p.fs, p.kind),
        _ => cur.clone(),
    };
    let mut v = u8::from_str_radix(ptype.trim_start_matches("0x"), 16).unwrap_or(0x83);
    // lba: FAT 0x0b <-> 0x0c, 0x06 <-> 0x0e.
    match (v & !0x10, has("lba")) {
        (0x0b, true) => v = (v & 0x10) | 0x0c,
        (0x06, true) => v = (v & 0x10) | 0x0e,
        (0x0c, false) => v = (v & 0x10) | 0x0b,
        (0x0e, false) => v = (v & 0x10) | 0x06,
        _ => {}
    }
    // hidden: the 0x10 bit of the DOS/Windows types.
    if matches!(v & !0x10, 0x01 | 0x04 | 0x06 | 0x07 | 0x0b | 0x0c | 0x0e) {
        if has("hidden") {
            v |= 0x10;
        } else {
            v &= !0x10;
        }
    }
    if !MBR_TYPE_FLAGS.iter().any(|(_, x)| ptype == *x) || newly.is_none() && kept.is_none() {
        ptype = format!("0x{v:02x}");
    }
    (ptype, attrs)
}

// ---- layout ----

/// A stretch of the disk: a partition or free space.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Segment {
    /// Index into Disk::parts, or None for free space.
    pub part: Option<usize>,
    pub offset: u64,
    pub size: u64,
    /// Free space inside the extended partition (logical partitions go here).
    pub in_extended: bool,
}

/// First and last usable bytes for partitions.
pub fn usable(d: &Disk) -> (u64, u64) {
    let end = if d.table == "gpt" { d.size.saturating_sub(33 * d.sector) } else { d.size };
    (MIB, align_down(end))
}

/// Room before a logical partition for its extended boot record.
pub const EBR_GAP: u64 = MIB;

/// The disk as partitions and free space, in order (logical partitions
/// and the free space between them follow their extended partition).
pub fn segments(d: &Disk) -> Vec<Segment> {
    let mut out = vec![];
    if d.table.is_empty() {
        return out;
    }
    let (lo, hi) = usable(d);
    let mut top: Vec<usize> = (0..d.parts.len()).filter(|&i| d.parts[i].kind != Kind::Logical).collect();
    top.sort_by_key(|&i| d.parts[i].offset);
    let mut logical: Vec<usize> = (0..d.parts.len()).filter(|&i| d.parts[i].kind == Kind::Logical).collect();
    logical.sort_by_key(|&i| d.parts[i].offset);
    let free = |out: &mut Vec<Segment>, a: u64, b: u64, ext: bool| {
        let a = align_up(a);
        let b = align_down(b);
        if b > a && b - a >= MIB {
            out.push(Segment { part: None, offset: a, size: b - a, in_extended: ext });
        }
    };
    let mut pos = lo;
    for &i in &top {
        let p = &d.parts[i];
        free(&mut out, pos, p.offset, false);
        out.push(Segment { part: Some(i), offset: p.offset, size: p.size, in_extended: false });
        if p.kind == Kind::Extended {
            let mut q = p.offset;
            for &j in &logical {
                let l = &d.parts[j];
                free(&mut out, q + EBR_GAP, l.offset, true);
                out.push(Segment { part: Some(j), offset: l.offset, size: l.size, in_extended: true });
                q = l.offset + l.size;
            }
            free(&mut out, q + EBR_GAP, p.offset + p.size, true);
        }
        pos = pos.max(p.offset + p.size);
    }
    free(&mut out, pos, hi, false);
    out
}

// ---- operations ----

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Op {
    #[serde(rename_all = "camelCase")]
    Create { new_id: String, offset: u64, size: u64, #[serde(default)] kind: Kind, #[serde(default)] fs: String, #[serde(default)] label: String, #[serde(default)] name: String },
    Delete { id: String },
    Format { id: String, fs: String, #[serde(default)] label: String },
    /// Move and/or resize (GParted's Resize/Move).
    Resize { id: String, offset: u64, size: u64 },
    Label { id: String, label: String },
    Name { id: String, name: String },
    Flags { id: String, flags: Vec<String> },
    /// A new, empty partition table ("gpt" or "dos"): everything goes.
    NewTable { kind: String },
    /// Check and repair, then grow the filesystem to fill its partition.
    Check { id: String },
    /// A copied partition put into free space.
    #[serde(rename_all = "camelCase")]
    Paste { new_id: String, offset: u64, size: u64, #[serde(default)] kind: Kind, source: Part, source_disk: String },
}

impl Op {
    pub fn target(&self) -> Option<&str> {
        match self {
            Op::Delete { id } | Op::Format { id, .. } | Op::Resize { id, .. } | Op::Label { id, .. } | Op::Name { id, .. } | Op::Flags { id, .. } | Op::Check { id } => Some(id),
            Op::Create { new_id, .. } | Op::Paste { new_id, .. } => Some(new_id),
            Op::NewTable { .. } => None,
        }
    }
}

pub fn part_title(d: &Disk, p: &Part) -> String {
    if p.number > 0 {
        if p.device.is_empty() { format!("partition {}", p.number) } else { p.device.clone() }
    } else {
        let n = d.parts.iter().filter(|q| q.number == 0).position(|q| q.id == p.id).map(|i| i + 1).unwrap_or(1);
        format!("New Partition #{n}")
    }
}

/// Plain words for a pending operation, on the disk as it is just before it.
pub fn describe(d: &Disk, op: &Op) -> String {
    let h = crate::util::human_size;
    let title = |id: &str| d.parts.iter().find(|p| p.id == id).map(|p| part_title(d, p)).unwrap_or_else(|| id.to_string());
    match op {
        Op::Create { size, fs, kind, new_id, .. } => {
            let what = match kind { Kind::Extended => "extended partition".to_string(), Kind::Logical => format!("logical partition ({})", fs_name(fs)), Kind::Primary => format!("{} partition", fs_name(fs)) };
            let _ = new_id;
            format!("Create a {} {what} on {}", h(*size), d.device)
        }
        Op::Delete { id } => format!("Delete {}", title(id)),
        Op::Format { id, fs, .. } => format!("Format {} as {}", title(id), fs_name(fs)),
        Op::Resize { id, offset, size } => {
            let p = d.parts.iter().find(|p| &p.id == id);
            let moved = p.map(|p| p.offset != *offset).unwrap_or(false);
            let resized = p.map(|p| p.size != *size).unwrap_or(false);
            let verb = match (moved, resized) { (true, true) => "Move and resize", (true, false) => "Move", _ => "Resize" };
            let from = p.map(|p| format!(" from {}", h(p.size))).unwrap_or_default();
            if resized { format!("{verb} {}{from} to {}", title(id), h(*size)) } else { format!("{verb} {}", title(id)) }
        }
        Op::Label { id, label } => if label.is_empty() { format!("Clear the label of {}", title(id)) } else { format!("Label {} “{label}”", title(id)) },
        Op::Name { id, name } => format!("Name {} “{name}”", title(id)),
        Op::Flags { id, flags } => if flags.is_empty() { format!("Clear the flags of {}", title(id)) } else { format!("Set flags on {}: {}", title(id), flags.join(", ")) },
        Op::NewTable { kind } => format!("Create a new {} partition table on {} (erases everything on it)", if kind == "gpt" { "GPT" } else { "MBR (msdos)" }, d.device),
        Op::Check { id } => format!("Check and repair {}", title(id)),
        Op::Paste { size, source, .. } => format!("Paste a copy of {} ({}) as a new {} partition", if source.device.is_empty() { "a partition".into() } else { source.device.clone() }, fs_name(&source.fs), h(*size)),
    }
}

/// Room a partition could take: from the end of what's before it to the
/// start of what's after (ignoring itself), in its own context
/// (logical partitions inside their extended partition).
pub fn room(d: &Disk, idx: usize) -> (u64, u64) {
    let p = &d.parts[idx];
    let (lo, hi) = usable(d);
    let (mut a, mut b) = (lo, hi);
    if p.kind == Kind::Logical {
        if let Some(e) = d.parts.iter().find(|q| q.kind == Kind::Extended) {
            a = e.offset + EBR_GAP;
            b = e.offset + e.size;
        }
    }
    let gap = if p.kind == Kind::Logical { EBR_GAP } else { 0 };
    for (j, q) in d.parts.iter().enumerate() {
        if j == idx || (q.kind == Kind::Logical) != (p.kind == Kind::Logical) {
            continue;
        }
        if q.offset + q.size <= p.offset {
            a = a.max(q.offset + q.size + gap);
        } else if q.offset >= p.offset + p.size {
            b = b.min(q.offset - gap);
        }
    }
    // The extended partition must keep its logical partitions inside.
    (align_up(a), align_down(b))
}

/// Smallest size a partition's filesystem can shrink to (None: can't).
pub fn min_size(p: &Part, caps: &Caps) -> u64 {
    if p.number == 0 {
        return fs_min(&p.fs).max(MIB);
    }
    match p.used {
        // GParted's margin: what's used plus a little room.
        Some(u) => align_up(u + (u / 20).max(16 * MIB)).max(fs_min(&p.fs)).min(p.size),
        None => {
            let _ = caps;
            p.size
        }
    }
}

fn err<T>(s: impl Into<String>) -> Result<T, String> {
    Err(s.into())
}

fn check_free(d: &Disk, offset: u64, size: u64, kind: Kind) -> Result<(), String> {
    if size < MIB {
        return err("A partition needs at least 1 MiB.");
    }
    let seg = segments(d).into_iter().find(|s| s.part.is_none() && s.offset <= offset && offset + size <= s.offset + s.size);
    match seg {
        None => err("That space isn't free: it overlaps another partition or runs past the end of the disk."),
        Some(s) if s.in_extended != (kind == Kind::Logical) => {
            if kind == Kind::Logical { err("A logical partition has to go inside the extended partition.") } else { err("Only logical partitions can go inside the extended partition.") }
        }
        Some(_) => Ok(()),
    }
}

fn check_count(d: &Disk, kind: Kind) -> Result<(), String> {
    if d.table == "dos" {
        let prim = d.parts.iter().filter(|p| p.kind != Kind::Logical).count();
        if kind != Kind::Logical && prim >= 4 {
            return err("An MBR disk holds at most four primary partitions (an extended partition counts as one). Make an extended partition for more.");
        }
        if kind == Kind::Extended && d.parts.iter().any(|p| p.kind == Kind::Extended) {
            return err("There can only be one extended partition.");
        }
        if kind == Kind::Logical && !d.parts.iter().any(|p| p.kind == Kind::Extended) {
            return err("There's no extended partition for a logical one to go in.");
        }
    } else if kind != Kind::Primary {
        return err("Extended and logical partitions only exist on MBR (msdos) disks.");
    } else if d.parts.len() >= 128 {
        return err("This GPT disk already has 128 partitions.");
    }
    Ok(())
}

fn find<'a>(d: &'a Disk, id: &str) -> Result<(usize, &'a Part), String> {
    d.parts.iter().enumerate().find(|(_, p)| p.id == id).ok_or_else(|| "That partition isn't there any more.".to_string())
}

fn not_busy(p: &Part) -> Result<(), String> {
    if p.busy {
        let at = p.mounts.first().map(|m| format!(" (it's open at {m})")).unwrap_or_default();
        return err(format!("{} is in use{at}. Unmount it first.", if p.device.is_empty() { "This partition".into() } else { p.device.clone() }));
    }
    Ok(())
}

/// Play `op` out on `d`: the disk as it will be, or why it can't be done.
pub fn apply(d: &Disk, op: &Op, caps: &Caps) -> Result<Disk, String> {
    if d.read_only {
        return err("This disk is read-only.");
    }
    let mut n = d.clone();
    let cap = |fs: &str| caps.get(fs).cloned().unwrap_or_default();
    match op {
        Op::NewTable { kind } => {
            if kind != "gpt" && kind != "dos" {
                return err("Choose GPT or MBR.");
            }
            if let Some(p) = d.parts.iter().find(|p| p.busy) {
                not_busy(p)?;
            }
            n.table = kind.clone();
            n.parts.clear();
            n.whole_fs.clear();
        }
        Op::Create { new_id, offset, size, kind, fs, label, name } | Op::Paste { new_id, offset, size, kind, source: Part { fs, label, name, .. }, .. } => {
            if d.table.is_empty() {
                return err("This disk has no partition table yet: make one first (Device › New Partition Table…).");
            }
            check_count(d, *kind)?;
            check_free(d, *offset, *size, *kind)?;
            if offset % MIB != 0 || size % MIB != 0 {
                return err("Partitions start and end on whole MiB.");
            }
            if *kind != Kind::Extended && !fs.is_empty() && fs != "empty" {
                if matches!(op, Op::Create { .. }) && !cap(fs).format {
                    return err(format!("{} can't be made on this computer{}.", fs_name(fs), if cap(fs).missing.is_empty() { String::new() } else { format!(" (it needs {})", cap(fs).missing) }));
                }
                if *size < fs_min(fs) {
                    return err(format!("{} needs at least {}.", fs_name(fs), crate::util::human_size(fs_min(fs))));
                }
                if *size > fs_max(fs) {
                    return err(format!("{} can't be bigger than {}.", fs_name(fs), crate::util::human_size(fs_max(fs))));
                }
                if label.chars().count() > label_max(fs) {
                    return err(format!("{} labels are at most {} characters.", fs_name(fs), label_max(fs)));
                }
            }
            if let Op::Paste { source, .. } = op {
                if *size < source.size {
                    return err("The copy needs at least as much space as the original.");
                }
            }
            let used = if let Op::Paste { source, .. } = op { source.used } else { None };
            let fs = if fs == "empty" || *kind == Kind::Extended { String::new() } else { fs.clone() };
            n.parts.push(Part {
                id: new_id.clone(),
                offset: *offset,
                size: *size,
                kind: *kind,
                ptype: default_ptype(&d.table, &fs, *kind),
                fs,
                label: label.clone(),
                name: name.clone(),
                used,
                pending: true,
                ..Default::default()
            });
        }
        Op::Delete { id } => {
            let (i, p) = find(d, id)?;
            not_busy(p)?;
            if p.kind == Kind::Extended && d.parts.iter().any(|q| q.kind == Kind::Logical) {
                return err("Delete the logical partitions inside the extended partition first.");
            }
            n.parts.remove(i);
        }
        Op::Format { id, fs, label } => {
            let (i, p) = find(d, id)?;
            not_busy(p)?;
            if p.kind == Kind::Extended {
                return err("An extended partition only holds other partitions; it can't be formatted.");
            }
            if fs != "empty" {
                if !cap(fs).format {
                    return err(format!("{} can't be made on this computer.", fs_name(fs)));
                }
                if p.size < fs_min(fs) {
                    return err(format!("{} needs at least {}; this partition is {}.", fs_name(fs), crate::util::human_size(fs_min(fs)), crate::util::human_size(p.size)));
                }
                if p.size > fs_max(fs) {
                    return err(format!("{} can't be bigger than {}.", fs_name(fs), crate::util::human_size(fs_max(fs))));
                }
                if label.chars().count() > label_max(fs) {
                    return err(format!("{} labels are at most {} characters.", fs_name(fs), label_max(fs)));
                }
            }
            let q = &mut n.parts[i];
            q.fs = if fs == "empty" { String::new() } else { fs.clone() };
            q.label = label.clone();
            q.used = None;
            q.pending = true;
            q.ptype = default_ptype(&d.table, &q.fs, q.kind);
        }
        Op::Resize { id, offset, size } => {
            let (i, p) = find(d, id)?;
            not_busy(p)?;
            if *offset == p.offset && *size == p.size {
                return err("Nothing changes.");
            }
            if offset % MIB != 0 || size % MIB != 0 {
                return err("Partitions start and end on whole MiB.");
            }
            let (a, b) = room(d, i);
            let (a, b) = (a.min(p.offset), b.max(p.offset + p.size));
            if *offset < a || offset + size > b {
                return err("That runs into another partition or past the end of the disk.");
            }
            if p.kind == Kind::Extended {
                let inner: Vec<&Part> = d.parts.iter().filter(|q| q.kind == Kind::Logical).collect();
                if inner.iter().any(|q| q.offset < offset + EBR_GAP || q.offset + q.size > offset + size) {
                    return err("The extended partition has to keep its logical partitions inside it.");
                }
            } else if p.number > 0 && !p.fs.is_empty() {
                let c = cap(&p.fs);
                if *size < p.size {
                    if !c.shrink {
                        return err(format!("{} can't be shrunk{}.", fs_name(&p.fs), if c.missing.is_empty() { String::new() } else { format!(" here (it needs {})", c.missing) }));
                    }
                    if *size < min_size(p, caps) {
                        return err(format!("The files on it need at least {}.", crate::util::human_size(min_size(p, caps))));
                    }
                }
                if *size > p.size && !c.grow {
                    return err(format!("{} can't be grown{}.", fs_name(&p.fs), if c.missing.is_empty() { String::new() } else { format!(" here (it needs {})", c.missing) }));
                }
                if *size > fs_max(&p.fs) {
                    return err(format!("{} can't be bigger than {}.", fs_name(&p.fs), crate::util::human_size(fs_max(&p.fs))));
                }
            } else if p.number > 0 && *size != p.size && !matches!(p.fs.as_str(), "") {
                return err("Only the position can change.");
            }
            if p.number > 0 && *offset != p.offset && p.kind == Kind::Logical {
                return err("Moving the start of a logical partition isn't supported yet; its end can still move.");
            }
            if p.number > 0 && *offset != p.offset && p.kind == Kind::Extended {
                return err("Moving the start of an extended partition isn't supported; its end can still move.");
            }
            if p.number > 0 && p.fs.is_empty() && p.kind != Kind::Extended && *size != p.size {
                // Unknown contents: GParted only moves these whole.
                return err("Grove can't tell what's on this partition, so it can only be moved, not resized.");
            }
            let q = &mut n.parts[i];
            q.offset = *offset;
            q.size = *size;
            q.pending = true;
        }
        Op::Label { id, label } => {
            let (i, p) = find(d, id)?;
            if p.fs.is_empty() || p.kind == Kind::Extended {
                return err("There's no filesystem to label.");
            }
            if p.busy && !matches!(p.fs.as_str(), "ext2" | "ext3" | "ext4" | "btrfs" | "xfs" | "swap") {
                not_busy(p)?;
            }
            if label.chars().count() > label_max(&p.fs) {
                return err(format!("{} labels are at most {} characters.", fs_name(&p.fs), label_max(&p.fs)));
            }
            n.parts[i].label = label.clone();
            n.parts[i].pending = true;
        }
        Op::Name { id, name } => {
            let (i, _) = find(d, id)?;
            if d.table != "gpt" {
                return err("Only GPT partitions have names.");
            }
            if name.encode_utf16().count() > 36 {
                return err("Partition names are at most 36 characters.");
            }
            n.parts[i].name = name.clone();
            n.parts[i].pending = true;
        }
        Op::Flags { id, flags } => {
            let (i, p) = find(d, id)?;
            let known = flag_names(&d.table);
            if let Some(f) = flags.iter().find(|f| !known.contains(&f.as_str())) {
                return err(format!("“{f}” isn't a flag on this kind of disk."));
            }
            let (t, a) = apply_flags(&d.table, p, flags);
            n.parts[i].ptype = t;
            n.parts[i].attrs = a;
            n.parts[i].pending = true;
        }
        Op::Check { id } => {
            let (_, p) = find(d, id)?;
            not_busy(p)?;
            if p.fs.is_empty() || p.kind == Kind::Extended {
                return err("There's no filesystem to check.");
            }
            if !cap(&p.fs).repair && !cap(&p.fs).check {
                return err(format!("{} can't be checked on this computer.", fs_name(&p.fs)));
            }
        }
    }
    Ok(n)
}

/// The operation queue for one disk: each step checked against the disk
/// as the steps before it leave it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Queue {
    pub ops: Vec<Op>,
    #[serde(default)]
    pub next_new: u32,
}

impl Queue {
    pub fn new_id(&mut self) -> String {
        self.next_new += 1;
        format!("new:{}", self.next_new)
    }

    /// The disk after every queued step.
    pub fn result(&self, base: &Disk, caps: &Caps) -> Result<Disk, String> {
        let mut d = base.clone();
        for op in &self.ops {
            d = apply(&d, op, caps)?;
        }
        Ok(d)
    }

    /// Add a step (it must work on the disk as the queue leaves it).
    /// Deleting a partition the queue itself makes takes back every step
    /// about it instead, as GParted does.
    pub fn push(&mut self, base: &Disk, op: Op, caps: &Caps) -> Result<(), String> {
        let cur = self.result(base, caps)?;
        apply(&cur, &op, caps)?;
        if let Op::Delete { id } = &op {
            if id.starts_with("new:") {
                self.ops.retain(|o| o.target() != Some(id.as_str()));
                return Ok(());
            }
        }
        // Successive moves/resizes, labels, names or flags of one
        // partition become one step.
        let same = match (self.ops.last(), &op) {
            (Some(Op::Resize { id: a, .. }), Op::Resize { id: b, .. }) | (Some(Op::Label { id: a, .. }), Op::Label { id: b, .. }) | (Some(Op::Name { id: a, .. }), Op::Name { id: b, .. }) | (Some(Op::Flags { id: a, .. }), Op::Flags { id: b, .. }) => a == b,
            _ => false,
        };
        if same {
            let last = self.ops.pop();
            let before = self.result(base, caps)?;
            let unchanged = matches!(&op, Op::Resize { id, offset, size } if before.parts.iter().any(|p| &p.id == id && p.offset == *offset && p.size == *size));
            if !unchanged {
                self.ops.push(op);
            }
            let _ = last;
            return Ok(());
        }
        // Moving or resizing a partition the queue makes: change how it's made.
        if let Op::Resize { id, offset, size } = &op {
            if id.starts_with("new:") {
                for o in self.ops.iter_mut() {
                    if let Op::Create { new_id, offset: a, size: b, .. } | Op::Paste { new_id, offset: a, size: b, .. } = o {
                        if new_id == id {
                            *a = *offset;
                            *b = *size;
                            return Ok(());
                        }
                    }
                }
            }
        }
        self.ops.push(op);
        Ok(())
    }

    pub fn undo(&mut self) -> Option<Op> {
        self.ops.pop()
    }
}

// ---- the disks that hold the running system ----

/// Kernel names of the whole disks under /, /home, /boot and /nix: never
/// changed unless the user unlocks them.
pub fn system_disks() -> BTreeSet<String> {
    system_disks_in(Path::new("/proc/self/mountinfo"), Path::new("/sys"))
}

const PROTECTED: &[&str] = &["/", "/home", "/boot", "/boot/efi", "/efi", "/nix", "/nix/store", "/usr", "/var"];

pub fn system_disks_in(mountinfo: &Path, sys: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Ok(t) = std::fs::read_to_string(mountinfo) else { return out };
    let mut sources = BTreeSet::new();
    for line in t.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        let Some(dash) = f.iter().position(|x| *x == "-") else { continue };
        if f.len() < 5 || !PROTECTED.contains(&f[4]) {
            continue;
        }
        if let Some(src) = f.get(dash + 2) {
            sources.insert(src.to_string());
        }
        // The device number too (it names the device when the source is
        // something like "rootfs").
        let mm = f[2];
        if let Ok(target) = std::fs::read_link(sys.join("dev/block").join(mm)) {
            if let Some(n) = target.file_name() {
                sources.insert(format!("/dev/{}", n.to_string_lossy()));
            }
        }
    }
    // Active swap too.
    if let Ok(sw) = std::fs::read_to_string("/proc/swaps") {
        for l in sw.lines().skip(1) {
            if let Some(dev) = l.split_whitespace().next() {
                sources.insert(dev.to_string());
            }
        }
    }
    let mut names = BTreeSet::new();
    for s in &sources {
        if !s.starts_with("/dev/") {
            continue;
        }
        let real = std::fs::canonicalize(s).unwrap_or_else(|_| PathBuf::from(s));
        if let Some(n) = real.file_name() {
            names.insert(n.to_string_lossy().into_owned());
        }
    }
    // btrfs filesystems spanning several devices: all of them.
    if let Ok(rd) = std::fs::read_dir(sys.join("fs/btrfs")) {
        for e in rd.flatten() {
            let devs: Vec<String> = std::fs::read_dir(e.path().join("devices")).map(|r| r.flatten().map(|x| x.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
            if devs.iter().any(|d| names.contains(d)) {
                names.extend(devs);
            }
        }
    }
    for n in names {
        leaves(sys, &n, &mut out, 0);
    }
    out
}

/// The whole disks under a block device (through dm/LUKS/LVM/md slaves).
fn leaves(sys: &Path, name: &str, out: &mut BTreeSet<String>, depth: u32) {
    if depth > 8 {
        return;
    }
    let dir = sys.join("class/block").join(name);
    let slaves: Vec<String> = std::fs::read_dir(dir.join("slaves")).map(|r| r.flatten().map(|x| x.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    if !slaves.is_empty() {
        for s in slaves {
            leaves(sys, &s, out, depth + 1);
        }
        return;
    }
    if dir.join("partition").exists() {
        if let Ok(real) = std::fs::canonicalize(&dir) {
            if let Some(parent) = real.parent().and_then(|p| p.file_name()) {
                out.insert(parent.to_string_lossy().into_owned());
                return;
            }
        }
    }
    out.insert(name.to_string());
}

// ---- how full an unmounted filesystem is (its superblock) ----

fn le16(b: &[u8], o: usize) -> u64 {
    u16::from_le_bytes([b[o], b[o + 1]]) as u64
}
fn le32(b: &[u8], o: usize) -> u64 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as u64
}
fn le64(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}
fn be32(b: &[u8], o: usize) -> u64 {
    u32::from_be_bytes(b[o..o + 4].try_into().unwrap()) as u64
}
fn be64(b: &[u8], o: usize) -> u64 {
    u64::from_be_bytes(b[o..o + 8].try_into().unwrap())
}

/// Bytes in use on a filesystem, read from the start of its partition
/// (the first 128 KiB is enough for ext, btrfs, XFS and FAT32).
pub fn used_from_superblock(fs: &str, head: &[u8]) -> Option<u64> {
    match fs {
        "ext2" | "ext3" | "ext4" => {
            let s = head.get(1024..2048)?;
            if le16(s, 0x38) != 0xef53 {
                return None;
            }
            let bs = 1024u64 << le32(s, 0x18);
            let mut blocks = le32(s, 0x04);
            let mut free = le32(s, 0x0c);
            // 64-bit feature: the high halves.
            if le32(s, 0x60) & 0x80 != 0 && le16(s, 0xfe) >= 64 {
                blocks |= le32(s, 0x150) << 32;
                free |= le32(s, 0x158) << 32;
            }
            Some(blocks.saturating_sub(free) * bs)
        }
        "btrfs" => {
            let s = head.get(0x10000..0x10000 + 0x100)?;
            if &s[0x40..0x48] != b"_BHRfS_M" {
                return None;
            }
            Some(le64(s, 0x78))
        }
        "xfs" => {
            let s = head.get(0..0x100)?;
            if &s[0..4] != b"XFSB" {
                return None;
            }
            let bs = be32(s, 4);
            Some(be64(s, 8).saturating_sub(be64(s, 0x90)) * bs)
        }
        "vfat" => {
            let s = head.get(0..1024)?;
            if le16(s, 510) != 0xaa55 {
                return None;
            }
            let bps = le16(s, 11);
            let spc = s[13] as u64;
            let reserved = le16(s, 14);
            let fats = s[16] as u64;
            let total = if le16(s, 19) != 0 { le16(s, 19) } else { le32(s, 32) };
            let fat_sz = if le16(s, 22) != 0 { le16(s, 22) } else { le32(s, 36) };
            if bps == 0 || spc == 0 || le16(s, 22) != 0 {
                return None; // FAT12/16: no free count kept.
            }
            let fsinfo = le16(s, 48) * bps;
            let fi = head.get(fsinfo as usize..fsinfo as usize + 512)?;
            if le32(fi, 0) != 0x4161_5252 {
                return None;
            }
            let free = le32(fi, 488);
            let clusters = (total - reserved - fats * fat_sz) / spc;
            if free == 0xffff_ffff || free > clusters {
                return None;
            }
            Some((clusters - free) * spc * bps + (reserved + fats * fat_sz) * bps)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps() -> Caps {
        let mut c = Caps::new();
        for fs in ["ext4", "ntfs", "btrfs", "vfat", "exfat", "xfs", "swap", "empty"] {
            c.insert(fs.into(), FsCaps { format: true, grow: fs != "exfat", shrink: !matches!(fs, "xfs" | "exfat"), check: true, repair: true, label: true, online_only: fs == "btrfs", missing: String::new() });
        }
        c
    }

    fn disk(table: &str) -> Disk {
        Disk { id: "/d".into(), device: "/dev/loop9".into(), size: 1024 * MIB, sector: 512, table: table.into(), ..Default::default() }
    }

    fn part(id: &str, n: u32, off: u64, size: u64, fs: &str) -> Part {
        Part { id: id.into(), number: n, offset: off * MIB, size: size * MIB, fs: fs.into(), device: format!("/dev/loop9p{n}"), used: Some(10 * MIB), ..Default::default() }
    }

    #[test]
    fn segments_find_free_space() {
        let mut d = disk("gpt");
        d.parts = vec![part("a", 1, 1, 100, "ext4"), part("b", 2, 300, 200, "ntfs")];
        let s = segments(&d);
        let free: Vec<(u64, u64)> = s.iter().filter(|s| s.part.is_none()).map(|s| (s.offset / MIB, s.size / MIB)).collect();
        // GPT keeps the last 33 sectors: the last MiB isn't usable.
        assert_eq!(free, vec![(101, 199), (500, 523)]);
        let mut d = disk("dos");
        assert_eq!(segments(&d).len(), 1);
        assert_eq!(segments(&d)[0].size, 1023 * MIB);
        d.table.clear();
        assert!(segments(&d).is_empty());
    }

    #[test]
    fn extended_and_logical() {
        let mut d = disk("dos");
        let mut q = Queue::default();
        let c = caps();
        q.push(&d, Op::Create { new_id: "new:1".into(), offset: MIB, size: 500 * MIB, kind: Kind::Extended, fs: String::new(), label: String::new(), name: String::new() }, &c).unwrap();
        let r = q.result(&d, &c).unwrap();
        let s = segments(&r);
        assert!(s.iter().any(|s| s.in_extended && s.offset == 2 * MIB));
        // A primary can't go inside it, a logical can't go outside it.
        assert!(q.clone().push(&d, Op::Create { new_id: "new:2".into(), offset: 10 * MIB, size: 10 * MIB, kind: Kind::Primary, fs: "ext4".into(), label: String::new(), name: String::new() }, &c).is_err());
        assert!(q.clone().push(&d, Op::Create { new_id: "new:2".into(), offset: 600 * MIB, size: 10 * MIB, kind: Kind::Logical, fs: "ext4".into(), label: String::new(), name: String::new() }, &c).is_err());
        q.push(&d, Op::Create { new_id: "new:2".into(), offset: 2 * MIB, size: 100 * MIB, kind: Kind::Logical, fs: "ext4".into(), label: String::new(), name: String::new() }, &c).unwrap();
        // The extended partition can't be deleted with a logical inside...
        let r = q.result(&d, &c).unwrap();
        assert!(apply(&r, &Op::Delete { id: "new:1".into() }, &c).is_err());
        // ...and deleting a queued partition takes back its steps.
        q.push(&d, Op::Delete { id: "new:2".into() }, &c).unwrap();
        assert_eq!(q.ops.len(), 1);
        // A second extended one: no.
        assert!(q.clone().push(&d, Op::Create { new_id: "new:3".into(), offset: 600 * MIB, size: 10 * MIB, kind: Kind::Extended, fs: String::new(), label: String::new(), name: String::new() }, &c).is_err());
        d.parts.clear();
    }

    #[test]
    fn four_primaries_on_mbr() {
        let mut d = disk("dos");
        d.parts = (0..4).map(|i| part(&format!("p{i}"), i + 1, 1 + 100 * i as u64, 100, "ext4")).collect();
        let e = apply(&d, &Op::Create { new_id: "new:1".into(), offset: 500 * MIB, size: 10 * MIB, kind: Kind::Primary, fs: "ext4".into(), label: String::new(), name: String::new() }, &caps());
        assert!(e.unwrap_err().contains("four primary"));
    }

    #[test]
    fn resize_rules() {
        let mut d = disk("gpt");
        d.parts = vec![part("a", 1, 1, 100, "ext4"), part("b", 2, 300, 200, "xfs"), part("c", 3, 600, 100, "exfat")];
        let c = caps();
        // Grow into the free space after it, not over the next one.
        assert!(apply(&d, &Op::Resize { id: "a".into(), offset: MIB, size: 299 * MIB }, &c).is_ok());
        assert!(apply(&d, &Op::Resize { id: "a".into(), offset: MIB, size: 300 * MIB }, &c).is_err());
        // Move right.
        assert!(apply(&d, &Op::Resize { id: "a".into(), offset: 150 * MIB, size: 100 * MIB }, &c).is_ok());
        // Not below what's in use (10 MiB + margin).
        assert!(apply(&d, &Op::Resize { id: "a".into(), offset: MIB, size: 20 * MIB }, &c).is_err());
        assert!(apply(&d, &Op::Resize { id: "a".into(), offset: MIB, size: 40 * MIB }, &c).is_ok());
        // XFS never shrinks; exFAT doesn't resize, but moves.
        assert!(apply(&d, &Op::Resize { id: "b".into(), offset: 300 * MIB, size: 100 * MIB }, &c).unwrap_err().contains("shrunk"));
        assert!(apply(&d, &Op::Resize { id: "c".into(), offset: 600 * MIB, size: 150 * MIB }, &c).is_err());
        assert!(apply(&d, &Op::Resize { id: "c".into(), offset: 650 * MIB, size: 100 * MIB }, &c).is_ok());
        // Unknown usage: can't shrink at all.
        d.parts[0].used = None;
        assert!(apply(&d, &Op::Resize { id: "a".into(), offset: MIB, size: 90 * MIB }, &c).is_err());
        // Mounted: no.
        d.parts[0].busy = true;
        d.parts[0].mounts = vec!["/mnt/x".into()];
        assert!(apply(&d, &Op::Resize { id: "a".into(), offset: MIB, size: 200 * MIB }, &c).unwrap_err().contains("Unmount"));
    }

    #[test]
    fn queue_merges_and_undo() {
        let mut d = disk("gpt");
        d.parts = vec![part("a", 1, 1, 100, "ext4")];
        let c = caps();
        let mut q = Queue::default();
        q.push(&d, Op::Resize { id: "a".into(), offset: MIB, size: 200 * MIB }, &c).unwrap();
        q.push(&d, Op::Resize { id: "a".into(), offset: MIB, size: 300 * MIB }, &c).unwrap();
        assert_eq!(q.ops.len(), 1);
        // Back where it started: the step goes.
        q.push(&d, Op::Resize { id: "a".into(), offset: MIB, size: 100 * MIB }, &c).unwrap();
        assert!(q.ops.is_empty());
        let id = q.new_id();
        q.push(&d, Op::Create { new_id: id.clone(), offset: 200 * MIB, size: 100 * MIB, kind: Kind::Primary, fs: "ntfs".into(), label: "Data".into(), name: String::new() }, &c).unwrap();
        q.push(&d, Op::Resize { id: id.clone(), offset: 300 * MIB, size: 200 * MIB }, &c).unwrap();
        assert_eq!(q.ops.len(), 1);
        assert!(matches!(q.ops[0], Op::Create { offset, size, .. } if offset == 300 * MIB && size == 200 * MIB));
        q.push(&d, Op::Label { id: "a".into(), label: "X".into() }, &c).unwrap();
        let r = q.result(&d, &c).unwrap();
        assert_eq!(r.parts[0].label, "X");
        assert!(r.parts[0].pending);
        assert!(q.undo().is_some());
        assert_eq!(q.result(&d, &c).unwrap().parts[0].label, "");
        // A new table empties the disk; later steps see the empty disk.
        q.push(&d, Op::NewTable { kind: "dos".into() }, &c).unwrap();
        assert!(q.clone().push(&d, Op::Delete { id: "a".into() }, &c).is_err());
        assert_eq!(q.result(&d, &c).unwrap().parts.len(), 0);
    }

    #[test]
    fn formats_need_room_and_short_labels() {
        let mut d = disk("gpt");
        d.parts = vec![part("a", 1, 1, 100, "ext4")];
        let c = caps();
        assert!(apply(&d, &Op::Format { id: "a".into(), fs: "btrfs".into(), label: String::new() }, &c).unwrap_err().contains("at least"));
        assert!(apply(&d, &Op::Format { id: "a".into(), fs: "vfat".into(), label: "TWELVE_CHARS".into() }, &c).unwrap_err().contains("11"));
        let r = apply(&d, &Op::Format { id: "a".into(), fs: "vfat".into(), label: "STICK".into() }, &c).unwrap();
        assert_eq!(r.parts[0].ptype, GPT_BASIC);
        assert!(apply(&d, &Op::Format { id: "a".into(), fs: "f2fs".into(), label: String::new() }, &c).is_err(), "f2fs isn't in these caps");
    }

    #[test]
    fn flags_round_trip() {
        let mut p = part("a", 1, 1, 100, "vfat");
        p.ptype = GPT_BASIC.into();
        assert_eq!(flags_of("gpt", &p), vec!["msftdata"]);
        let (t, a) = apply_flags("gpt", &p, &["esp".into(), "hidden".into()]);
        assert_eq!(t, GPT_ESP);
        assert_eq!(a, 1 << 62);
        p.ptype = t;
        p.attrs = a;
        assert_eq!(flags_of("gpt", &p), vec!["esp", "hidden"]);
        // Clearing esp: back to the filesystem's own type.
        let (t, _) = apply_flags("gpt", &p, &[]);
        assert_eq!(t, GPT_BASIC);
        let (t, _) = apply_flags("gpt", &Part { ptype: GPT_BASIC.into(), fs: "vfat".into(), ..Default::default() }, &[]);
        assert_eq!(t, GPT_LINUX);
        // MBR: boot is a bit, hidden and lba change the FAT type.
        let mut m = Part { ptype: "0x0c".into(), fs: "vfat".into(), ..Default::default() };
        assert_eq!(flags_of("dos", &m), vec!["lba"]);
        let (t, a) = apply_flags("dos", &m, &["boot".into(), "lba".into(), "hidden".into()]);
        assert_eq!((t.as_str(), a), ("0x1c", 0x80));
        m.ptype = t;
        m.attrs = a;
        assert_eq!(flags_of("dos", &m), vec!["boot", "hidden", "lba"]);
        let (t, a) = apply_flags("dos", &m, &[]);
        assert_eq!((t.as_str(), a), ("0x0b", 0));
        let (t, _) = apply_flags("dos", &m, &["esp".into()]);
        assert_eq!(t, "0xef");
    }

    #[test]
    fn superblocks() {
        // ext4: 1 KiB blocks, 1000 blocks, 400 free.
        let mut b = vec![0u8; 0x20000];
        b[1024 + 0x38..1024 + 0x3a].copy_from_slice(&0xef53u16.to_le_bytes());
        b[1024 + 0x04..1024 + 0x08].copy_from_slice(&1000u32.to_le_bytes());
        b[1024 + 0x0c..1024 + 0x10].copy_from_slice(&400u32.to_le_bytes());
        assert_eq!(used_from_superblock("ext4", &b), Some(600 * 1024));
        assert_eq!(used_from_superblock("btrfs", &b), None);
        b[0x10040..0x10048].copy_from_slice(b"_BHRfS_M");
        b[0x10078..0x10080].copy_from_slice(&12345u64.to_le_bytes());
        assert_eq!(used_from_superblock("btrfs", &b), Some(12345));
    }

    #[test]
    fn system_disk_detection() {
        let t = tempfile::tempdir().unwrap();
        let sys = t.path().join("sys");
        // nvme0n1p2 (a partition of nvme0n1) under dm-0 (LUKS) mounted at /.
        std::fs::create_dir_all(sys.join("devices/nvme0n1/nvme0n1p2")).unwrap();
        std::fs::write(sys.join("devices/nvme0n1/nvme0n1p2/partition"), "2").unwrap();
        std::fs::create_dir_all(sys.join("devices/dm-0/slaves")).unwrap();
        std::fs::create_dir_all(sys.join("class/block")).unwrap();
        std::os::unix::fs::symlink(sys.join("devices/nvme0n1/nvme0n1p2"), sys.join("class/block/nvme0n1p2")).unwrap();
        std::os::unix::fs::symlink(sys.join("devices/dm-0"), sys.join("class/block/dm-0")).unwrap();
        std::fs::write(sys.join("devices/dm-0/slaves/nvme0n1p2"), "").unwrap();
        let mi = t.path().join("mountinfo");
        std::fs::write(&mi, "22 1 254:0 / / rw - btrfs /dev/dm-0 rw\n23 22 0:5 / /mnt/x rw - ext4 /dev/sdz1 rw\n").unwrap();
        let s = system_disks_in(&mi, &sys);
        assert!(s.contains("nvme0n1"), "{s:?}");
        assert!(!s.contains("sdz"));
    }

    #[test]
    fn describes_in_plain_words() {
        let mut d = disk("gpt");
        d.parts = vec![part("a", 1, 1, 100, "ext4")];
        assert_eq!(describe(&d, &Op::Resize { id: "a".into(), offset: MIB, size: 200 * MIB }), "Resize /dev/loop9p1 from 105 MB to 210 MB");
        assert!(describe(&d, &Op::NewTable { kind: "gpt".into() }).contains("erases everything"));
    }
}
