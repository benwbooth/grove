# No external programs

Ben (2026-11-22): grove shouldn't rely on external binaries. Everything it
does itself goes through linked libraries or Rust crates; it only starts
other programs where that is the point (opening files in apps) or where a
separate privileged/FUSE service is needed (drives).

| Area | Was | Becomes | Status |
|---|---|---|---|
| Archives: list, extract, members | bsdtar, bsdcat | libarchive linked (compress-tools) | todo |
| Compress (zip, tar.zst) | bsdtar | zip, tar, zstd crates | todo |
| .msi / .cab | 7zz | msi + cab crates | todo |
| .dmg (HFS+, APFS) | 7zz | dmgwiz + hfsplus + apfs crates | todo |
| Office text, docx/odt pages | pandoc | own OOXML/ODF reader (quick-xml + zip) to HTML | todo |
| PDF pages, info, thumbnails | pdftoppm, pdfinfo, mutool | MuPDF linked (mupdf crate) | todo |
| EPUB, XPS, comics to PDF | mutool, bsdtar | MuPDF linked | todo |
| DjVu | ddjvu | libdjvulibre (ddjvuapi FFI) | todo |
| PostScript | ps2pdf | libgs (Ghostscript API) | todo |
| Office pages (layout) | soffice | LibreOfficeKit (lok C API) | todo |
| Waveform, probe, video thumbs | ffmpeg, ffprobe, ffmpegthumbnailer | FFmpeg linked (ffmpeg-next) | todo |
| Pictures Qt can't read | magick, convert, vipsthumbnail | libheif-rs, image crate, libvips | todo |
| File type AI | magika CLI | magika crate (ONNX) | todo |
| Git status | git | git2 (libgit2) | todo |
| Compare folders | diff | similar crate | todo |
| Network browsing | avahi-browse | mdns-sd crate | todo |
| Disk images (mount) | udisksctl | UDisks2 over D-Bus | todo |
| Permissions | chmod | std::fs | todo |
| Burn | xorriso | libisofs/libburn FFI | todo |

Stays external, by nature:
- apps grove opens files with (desktop entries);
- pkexec, for changes needing root;
- rclone, sshfs, simple-mtpfs/jmtpfs, fusermount: FUSE mounts run as their own processes;
- hearth-search: hearth's indexer.
