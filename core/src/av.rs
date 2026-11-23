//! Video and audio through FFmpeg's libraries, linked (ffmpeg-next; no
//! ffmpeg/ffprobe commands): facts (size, length), a frame of a video, an
//! audio file's cover art, waveforms, and trimming.

use ffmpeg_next as ff;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

fn init() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = ff::init();
        ff::log::set_level(ff::log::Level::Fatal);
    });
}

fn input(path: &Path) -> Option<ff::format::context::Input> {
    init();
    ff::format::input(path).ok()
}

fn is_cover(s: &ff::format::stream::Stream) -> bool {
    s.disposition().contains(ff::format::stream::Disposition::ATTACHED_PIC)
}

/// The video stream (not cover art).
fn video_index(ictx: &ff::format::context::Input) -> Option<usize> {
    ictx.streams().filter(|s| s.parameters().medium() == ff::media::Type::Video && !is_cover(s)).max_by_key(|s| s.frames()).map(|s| s.index())
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Probe {
    pub width: u32,
    pub height: u32,
    pub seconds: f64,
    pub has_video: bool,
}

/// Size of the video (if any) and the length.
pub fn probe(path: &Path) -> Option<Probe> {
    let ictx = input(path)?;
    let mut p = Probe::default();
    if ictx.duration() > 0 {
        p.seconds = ictx.duration() as f64 / f64::from(ff::ffi::AV_TIME_BASE);
    }
    if let Some(i) = video_index(&ictx) {
        let par = ictx.stream(i)?.parameters();
        unsafe {
            p.width = (*par.as_ptr()).width.max(0) as u32;
            p.height = (*par.as_ptr()).height.max(0) as u32;
        }
        p.has_video = true;
    }
    Some(p)
}

/// A decoded frame as a PNG (temporary file), at most `px` on its longer side.
fn save_frame(frame: &ff::frame::Video, px: u32) -> Option<PathBuf> {
    let (w, h) = (frame.width(), frame.height());
    if w == 0 || h == 0 {
        return None;
    }
    let k = (px as f64 / w.max(h) as f64).min(1.0);
    let tw = ((w as f64 * k).round() as u32).max(2) & !1;
    let th = ((h as f64 * k).round() as u32).max(2) & !1;
    let mut sc = ff::software::scaling::Context::get(frame.format(), w, h, ff::format::Pixel::RGB24, tw, th, ff::software::scaling::Flags::BILINEAR).ok()?;
    let mut rgb = ff::frame::Video::empty();
    sc.run(frame, &mut rgb).ok()?;
    let stride = rgb.stride(0);
    let data = rgb.data(0);
    let mut buf = Vec::with_capacity((tw * th * 3) as usize);
    for y in 0..th as usize {
        buf.extend_from_slice(&data[y * stride..y * stride + tw as usize * 3]);
    }
    let img = image::RgbImage::from_raw(tw, th, buf)?;
    let out = crate::previews::temp_file("png");
    img.save(&out).ok()?;
    Some(out)
}

/// The first frame decoded from `idx` after where the input stands.
fn first_frame(ictx: &mut ff::format::context::Input, idx: usize, cancel: &AtomicBool) -> Option<ff::frame::Video> {
    let par = ictx.stream(idx)?.parameters();
    let mut dec = ff::codec::context::Context::from_parameters(par).ok()?.decoder().video().ok()?;
    let mut frame = ff::frame::Video::empty();
    let mut sent = 0;
    for (s, pkt) in ictx.packets() {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        if s.index() != idx {
            continue;
        }
        if dec.send_packet(&pkt).is_ok() && dec.receive_frame(&mut frame).is_ok() {
            return Some(frame);
        }
        sent += 1;
        if sent > 600 {
            break;
        }
    }
    let _ = dec.send_eof();
    dec.receive_frame(&mut frame).ok().map(|_| frame)
}

/// A frame from a video: a little way in (a tenth of it, at most 4 s),
/// past black openings.
pub fn video_frame(path: &Path, px: u32, cancel: &AtomicBool) -> Option<PathBuf> {
    let mut ictx = input(path)?;
    let idx = video_index(&ictx)?;
    let dur = ictx.duration();
    if dur > 0 {
        let at = (dur / 10).min(4 * i64::from(ff::ffi::AV_TIME_BASE));
        if ictx.seek(at, ..at).is_err() {
            let _ = ictx.seek(0, ..0);
        }
    }
    let f = first_frame(&mut ictx, idx, cancel).or_else(|| {
        let mut again = input(path)?;
        first_frame(&mut again, idx, cancel)
    })?;
    save_frame(&f, px)
}

/// The cover art embedded in an audio (or video) file.
pub fn audio_cover(path: &Path, px: u32, cancel: &AtomicBool) -> Option<PathBuf> {
    let mut ictx = input(path)?;
    let idx = ictx.streams().find(|s| is_cover(s))?.index();
    let f = first_frame(&mut ictx, idx, cancel)?;
    save_frame(&f, px)
}

/// Peak levels of the (first, up to `max_secs`) audio, one per 10 ms,
/// 0..1, and the stream's length in seconds.
pub fn peaks(path: &Path, max_secs: f64, cancel: &AtomicBool) -> Option<(Vec<f32>, f64)> {
    let mut ictx = input(path)?;
    let st = ictx.streams().best(ff::media::Type::Audio)?;
    let idx = st.index();
    let mut dec = ff::codec::context::Context::from_parameters(st.parameters()).ok()?.decoder().audio().ok()?;
    let mut out: Vec<f32> = Vec::new();
    let mut cur = 0f32;
    let mut in_chunk = 0usize;
    let mut total = 0usize;
    let mut chunk = 0usize;
    let mut frame = ff::frame::Audio::empty();
    let mut done = false;
    let mut take = |frame: &ff::frame::Audio, out: &mut Vec<f32>| -> bool {
        let rate = frame.rate().max(1) as usize;
        if chunk == 0 {
            chunk = (rate / 100).max(1);
        }
        let n = frame.samples();
        let ch = (frame.channels() as usize).max(1);
        let planar = frame.is_planar();
        // One sample's level (any channel), from the raw bytes.
        use ff::format::Sample as S;
        let fmt = frame.format();
        let level = |plane: &[u8], i: usize| -> f32 {
            match fmt {
                S::F32(_) => plane.get(i * 4..i * 4 + 4).map(|b| f32::from_ne_bytes([b[0], b[1], b[2], b[3]]).abs()).unwrap_or(0.0),
                S::F64(_) => plane.get(i * 8..i * 8 + 8).map(|b| f64::from_ne_bytes(b.try_into().unwrap()).abs() as f32).unwrap_or(0.0),
                S::I16(_) => plane.get(i * 2..i * 2 + 2).map(|b| (i16::from_ne_bytes([b[0], b[1]]) as f32 / 32768.0).abs()).unwrap_or(0.0),
                S::I32(_) => plane.get(i * 4..i * 4 + 4).map(|b| (i32::from_ne_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2147483648.0).abs()).unwrap_or(0.0),
                S::I64(_) => plane.get(i * 8..i * 8 + 8).map(|b| (i64::from_ne_bytes(b.try_into().unwrap()) as f64 / 9.223372036854776e18).abs() as f32).unwrap_or(0.0),
                S::U8(_) => plane.get(i).map(|&b| ((b as f32 - 128.0) / 128.0).abs()).unwrap_or(0.0),
                _ => 0.0,
            }
        };
        for s in 0..n {
            let mut v = 0f32;
            for c in 0..ch {
                let l = if planar { level(frame.data(c), s) } else { level(frame.data(0), s * ch + c) };
                if l > v {
                    v = l;
                }
            }
            if v > cur {
                cur = v;
            }
            in_chunk += 1;
            if in_chunk >= chunk {
                out.push(cur.min(1.0));
                cur = 0.0;
                in_chunk = 0;
            }
        }
        total += n;
        (total as f64) / rate as f64 >= max_secs
    };
    for (s, pkt) in ictx.packets() {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        if s.index() != idx {
            continue;
        }
        if dec.send_packet(&pkt).is_err() {
            continue;
        }
        while dec.receive_frame(&mut frame).is_ok() {
            if take(&frame, &mut out) {
                done = true;
                break;
            }
        }
        if done {
            break;
        }
    }
    if !done {
        let _ = dec.send_eof();
        while dec.receive_frame(&mut frame).is_ok() {
            if take(&frame, &mut out) {
                break;
            }
        }
    }
    let secs = out.len() as f64 / 100.0;
    (!out.is_empty()).then_some((out, secs))
}

/// "0x3b82f6" / "#3b82f6" to RGBA.
fn color(s: &str) -> image::Rgba<u8> {
    let h = s.trim_start_matches("0x").trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).unwrap_or(0x3b82f6);
    image::Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
}

/// A waveform picture of `peaks`, `w`×`h`, bars centred, the level's square
/// root (as ffmpeg's showwavespic scale=sqrt), on transparent.
pub fn waveform_png(peaks: &[f32], w: u32, h: u32, col: &str, out: &Path) -> Option<()> {
    let mut img = image::RgbaImage::new(w, h);
    let c = color(col);
    let n = peaks.len().max(1);
    let mid = h as f32 / 2.0;
    for x in 0..w {
        let a = x as usize * n / w as usize;
        let b = ((x as usize + 1) * n / w as usize).max(a + 1).min(n);
        let p = peaks.get(a..b).map(|s| s.iter().cloned().fold(0f32, f32::max)).unwrap_or(0.0);
        let half = (p.sqrt() * mid).max(0.5);
        let (y0, y1) = ((mid - half).floor().max(0.0) as u32, ((mid + half).ceil() as u32).min(h));
        for y in y0..y1 {
            img.put_pixel(x, y, c);
        }
    }
    img.save(out).ok()
}

/// Whether a video cut at `start` seconds can be a stream copy: its video
/// has a keyframe there (within 50 ms), or there's no video.
pub fn keyframe_at(path: &Path, start: f64) -> bool {
    if start <= 0.0 {
        return true;
    }
    let Some(mut ictx) = input(path) else { return false };
    let Some(idx) = video_index(&ictx) else { return true };
    let tb = match ictx.stream(idx) {
        Some(s) => s.time_base(),
        None => return false,
    };
    let from = ((start - 3.0).max(0.0) * f64::from(ff::ffi::AV_TIME_BASE)) as i64;
    let _ = ictx.seek(from, ..from);
    for (s, pkt) in ictx.packets() {
        if s.index() != idx {
            continue;
        }
        let t = pkt.pts().or(pkt.dts()).unwrap_or(0) as f64 * f64::from(tb);
        if t > start + 1.0 {
            break;
        }
        if pkt.is_key() && (t - start).abs() < 0.05 {
            return true;
        }
    }
    false
}

/// Copy `[start, end)` seconds of every stream into `dest` without
/// re-encoding (cuts land on keyframes).
pub fn trim_copy(src: &Path, start: f64, end: f64, dest: &Path) -> Result<(), String> {
    init();
    let mut ictx = ff::format::input(src).map_err(|e| e.to_string())?;
    let mut octx = ff::format::output(dest).map_err(|e| e.to_string())?;
    let mut map = vec![None; ictx.nb_streams() as usize];
    let mut n = 0;
    for (i, ist) in ictx.streams().enumerate() {
        let m = ist.parameters().medium();
        if !matches!(m, ff::media::Type::Video | ff::media::Type::Audio | ff::media::Type::Subtitle) {
            continue;
        }
        let mut ost = octx.add_stream(ff::encoder::find(ff::codec::Id::None)).map_err(|e| e.to_string())?;
        ost.set_parameters(ist.parameters());
        unsafe {
            (*ost.parameters().as_mut_ptr()).codec_tag = 0;
        }
        map[i] = Some(n);
        n += 1;
    }
    octx.set_metadata(ictx.metadata().to_owned());
    octx.write_header().map_err(|e| format!("This format can't be copied: {e}"))?;
    let at = (start * f64::from(ff::ffi::AV_TIME_BASE)) as i64;
    let _ = ictx.seek(at, ..at);
    let mut first: Vec<Option<i64>> = vec![None; map.len()];
    for (ist, mut pkt) in ictx.packets() {
        let i = ist.index();
        let Some(o) = map[i] else { continue };
        let itb = ist.time_base();
        let t = pkt.pts().or(pkt.dts()).unwrap_or(0) as f64 * f64::from(itb);
        if t >= end {
            if map.iter().flatten().count() == 1 {
                break;
            }
            continue;
        }
        // Start each stream at zero (make_zero).
        let base = *first[i].get_or_insert(pkt.dts().or(pkt.pts()).unwrap_or(0));
        pkt.set_pts(pkt.pts().map(|p| p - base));
        pkt.set_dts(pkt.dts().map(|d| d - base));
        let otb = octx.stream(o).map(|s| s.time_base()).unwrap_or(itb);
        pkt.rescale_ts(itb, otb);
        pkt.set_position(-1);
        pkt.set_stream(o);
        pkt.write_interleaved(&mut octx).map_err(|e| e.to_string())?;
    }
    octx.write_trailer().map_err(|e| e.to_string())
}

/// Re-encode the video (H.264) for an exact cut; audio and subtitles are
/// copied (their packets are short enough to cut cleanly).
pub fn trim_encode(src: &Path, start: f64, end: f64, dest: &Path) -> Result<(), String> {
    init();
    let e = |x: ff::Error| x.to_string();
    let mut ictx = ff::format::input(src).map_err(e)?;
    let mut octx = ff::format::output(dest).map_err(e)?;
    let vidx = video_index(&ictx).ok_or("No video to cut.")?;
    let global = octx.format().flags().contains(ff::format::Flags::GLOBAL_HEADER);
    let mut map = vec![None; ictx.nb_streams() as usize];
    let mut dec: Option<ff::decoder::Video> = None;
    let mut enc: Option<ff::encoder::Video> = None;
    let mut vtb = ff::Rational(1, 25);
    let mut n = 0;
    for (i, ist) in ictx.streams().enumerate() {
        let m = ist.parameters().medium();
        if i == vidx {
            let d = ff::codec::context::Context::from_parameters(ist.parameters()).map_err(e)?.decoder().video().map_err(e)?;
            let codec = ff::encoder::find(ff::codec::Id::H264).ok_or("No H.264 encoder in this FFmpeg.")?;
            let mut ost = octx.add_stream(codec).map_err(e)?;
            let mut en = ff::codec::context::Context::new_with_codec(codec).encoder().video().map_err(e)?;
            en.set_width(d.width());
            en.set_height(d.height());
            en.set_aspect_ratio(d.aspect_ratio());
            en.set_format(d.format());
            en.set_frame_rate(d.frame_rate());
            vtb = ist.time_base();
            en.set_time_base(vtb);
            if global {
                en.set_flags(ff::codec::Flags::GLOBAL_HEADER);
            }
            let mut opts = ff::Dictionary::new();
            opts.set("preset", "veryfast");
            opts.set("crf", "20");
            let opened = en.open_as_with(codec, opts).map_err(e)?;
            ost.set_parameters(&opened);
            ost.set_time_base(vtb);
            dec = Some(d);
            enc = Some(opened);
            map[i] = Some(n);
            n += 1;
        } else if matches!(m, ff::media::Type::Audio | ff::media::Type::Subtitle) && !is_cover(&ist) {
            let mut ost = octx.add_stream(ff::encoder::find(ff::codec::Id::None)).map_err(e)?;
            ost.set_parameters(ist.parameters());
            unsafe {
                (*ost.parameters().as_mut_ptr()).codec_tag = 0;
            }
            map[i] = Some(n);
            n += 1;
        }
    }
    let (mut dec, mut enc) = (dec.unwrap(), enc.unwrap());
    octx.set_metadata(ictx.metadata().to_owned());
    octx.write_header().map_err(e)?;
    let vo = map[vidx].unwrap();
    let start_ts = (start / f64::from(vtb)) as i64;
    let at = (start * f64::from(ff::ffi::AV_TIME_BASE)) as i64;
    let _ = ictx.seek(at, ..at);
    let mut frame = ff::frame::Video::empty();
    let mut out_pkt = ff::Packet::empty();
    let flush = |enc: &mut ff::encoder::Video, octx: &mut ff::format::context::Output, pkt: &mut ff::Packet| -> Result<(), String> {
        while enc.receive_packet(pkt).is_ok() {
            pkt.set_stream(vo);
            let otb = octx.stream(vo).map(|s| s.time_base()).unwrap_or(vtb);
            pkt.rescale_ts(vtb, otb);
            pkt.write_interleaved(octx).map_err(|x| x.to_string())?;
        }
        Ok(())
    };
    let mut first: Vec<Option<i64>> = vec![None; map.len()];
    for (ist, mut pkt) in ictx.packets() {
        let i = ist.index();
        let Some(o) = map[i] else { continue };
        let itb = ist.time_base();
        if i == vidx {
            dec.send_packet(&pkt).map_err(e)?;
            while dec.receive_frame(&mut frame).is_ok() {
                let ts = frame.timestamp().unwrap_or(0);
                let t = ts as f64 * f64::from(vtb);
                if t < start || t >= end {
                    continue;
                }
                frame.set_pts(Some(ts - start_ts));
                frame.set_kind(ff::picture::Type::None);
                enc.send_frame(&frame).map_err(e)?;
                flush(&mut enc, &mut octx, &mut out_pkt)?;
            }
            continue;
        }
        let t = pkt.pts().or(pkt.dts()).unwrap_or(0) as f64 * f64::from(itb);
        if t < start || t >= end {
            continue;
        }
        let base = *first[i].get_or_insert(pkt.dts().or(pkt.pts()).unwrap_or(0));
        pkt.set_pts(pkt.pts().map(|p| p - base));
        pkt.set_dts(pkt.dts().map(|d| d - base));
        let otb = octx.stream(o).map(|s| s.time_base()).unwrap_or(itb);
        pkt.rescale_ts(itb, otb);
        pkt.set_position(-1);
        pkt.set_stream(o);
        pkt.write_interleaved(&mut octx).map_err(e)?;
    }
    let _ = dec.send_eof();
    while dec.receive_frame(&mut frame).is_ok() {
        let ts = frame.timestamp().unwrap_or(0);
        let t = ts as f64 * f64::from(vtb);
        if t >= start && t < end {
            frame.set_pts(Some(ts - start_ts));
            enc.send_frame(&frame).map_err(e)?;
            flush(&mut enc, &mut octx, &mut out_pkt)?;
        }
    }
    enc.send_eof().map_err(e)?;
    flush(&mut enc, &mut octx, &mut out_pkt)?;
    octx.write_trailer().map_err(e)
}
