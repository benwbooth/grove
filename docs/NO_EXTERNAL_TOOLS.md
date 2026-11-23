# No external programs

Ben (2026-11-22): grove shouldn't rely on external binaries. Everything it
does itself goes through linked libraries or Rust crates; it only starts
other programs where that is the point (opening files in apps) or where a
separate privileged/FUSE service is needed (drives).

| Area | Was | Becomes | Status |
|---|---|---|---|
| Archives: list, extract, members | bsdtar, bsdcat | libarchive linked (core/src/libarchive.rs) | done |
| Compress (zip, tar.zst) | bsdtar | libarchive linked | done |
| .msi | 7zz | msi + cab crates (core/src/msi.rs) | done |
| .dmg (HFS+, APFS) | 7zz | dmgwiz + hfsplus + apfs crates (core/src/dmg.rs) | done |
| Office text, docx/odt pages | pandoc | own OOXML/ODF reader (quick-xml + zip) to HTML | todo |
| PDF pages, info, thumbnails | pdftoppm, pdfinfo, mutool | MuPDF linked (mupdf crate) | todo |
| EPUB, XPS, comics to PDF | mutool, bsdtar | MuPDF linked | todo |
| DjVu | ddjvu | libdjvulibre (ddjvuapi FFI) | todo |
| PostScript | ps2pdf | libgs (Ghostscript API) | todo |
| Office pages (layout) | soffice | LibreOfficeKit (lok C API) | todo |
| Waveform, probe, video frames, cover art, trim | ffmpeg, ffprobe, ffmpegthumbnailer, system thumbnailers | FFmpeg linked (ffmpeg-next, core/src/av.rs) | done |
| Pictures Qt can't read | magick, vipsthumbnail | libvips linked (core/src/vips.rs), image crate | done |
| File type AI | magika CLI | magika crate, onnxruntime linked | done |
| Git status and menu (add, commit, log, diff, pull, push) | git | git2 (libgit2), src/gitstatus.rs | done |
| Compare files/folders | diff | similar crate (src/compare.rs) | done |
| Network browsing | avahi-browse | mdns-sd crate | done |
| Disk images (mount) | udisksctl | UDisks2 over D-Bus | done |
| Permissions, owner | chmod, chown, env (as root) | std::fs; as root `pkexec grove --root-helper` (src/roothelper.rs) | done |
| Version control badges (git, hg, svn, jj, fossil) | git | libgit2, hg dirstate, svn/fossil SQLite (src/vcs.rs) | done |
| Burn | xorriso | libisofs/libburn FFI | todo |

Stays external, by nature:
- apps grove opens files with (desktop entries);
- pkexec, for changes needing root (it runs grove itself);
- umount, for mounts from fstab that udisks doesn't manage (setuid);
- rclone, sshfs, simple-mtpfs/jmtpfs, fusermount: FUSE mounts run as their own processes;
- hearth-search: hearth's indexer.
