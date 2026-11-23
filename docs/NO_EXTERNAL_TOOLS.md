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
| Office text (docx, odt, pptx, rtf) | pandoc | own reader (core/src/officehtml.rs) | done |
| PDF pages, info, thumbnails | pdftoppm, pdfinfo, mutool | MuPDF linked (core/csrc/docs.c) | done |
| EPUB, XPS, FB2, MOBI, comics to PDF | mutool, bsdtar | MuPDF linked | done |
| DjVu | ddjvu | djvulibre linked, pages into MuPDF's PDF writer | done |
| PostScript | ps2pdf | Ghostscript API (libgs dlopen'ed privately: its exported names broke libvips) | done |
| Office pages (layout) | soffice | LibreOfficeKit, in `grove --office-pdf` (its own process) | done |
| Waveform, probe, video frames, cover art, trim | ffmpeg, ffprobe, ffmpegthumbnailer, system thumbnailers | FFmpeg linked (ffmpeg-next, core/src/av.rs) | done |
| Pictures Qt can't read | magick, vipsthumbnail | libvips linked (core/src/vips.rs), image crate | done |
| File type AI | magika CLI | magika crate, onnxruntime linked | done |
| Git status and menu (add, commit, log, diff, pull, push) | git | git2 (libgit2), src/gitstatus.rs | done |
| Compare files/folders | diff | similar crate (src/compare.rs) | done |
| Network browsing | avahi-browse | mdns-sd crate | done |
| Disk images (mount) | udisksctl | UDisks2 over D-Bus | done |
| Permissions, owner | chmod, chown, env (as root) | std::fs; as root `pkexec grove --root-helper` (src/roothelper.rs) | done |
| Version control badges (git, hg, svn, jj, fossil) | git | libgit2, hg dirstate, svn/fossil SQLite (src/vcs.rs) | done |
| Burn to disc, ISO images | xorriso | libisofs + libburn linked (core/csrc/burn.c) | done |

Stays external, by nature:
- apps grove opens files with (desktop entries);
- pkexec, for changes needing root (it runs grove itself);
- umount, for mounts from fstab that udisks doesn't manage (setuid);
- rclone, sshfs, simple-mtpfs/jmtpfs, fusermount: FUSE mounts run as their own processes;
- hearth-search: hearth's indexer.
