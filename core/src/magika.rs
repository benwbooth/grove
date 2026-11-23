//! Magika (Google's file-type model) run in pure Rust on tract: no
//! onnxruntime. The model (standard_v3_3) and its label table come from the
//! magika crate (Apache-2.0; regenerate with scripts/gen-magika.py); the
//! input is Magika's own: the first and last 1 KiB of the file with
//! leading/trailing whitespace stripped, padded with token 256.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::OnceLock;
use tract_onnx::prelude::*;

#[path = "magika_table.rs"]
mod table;
pub use table::Label;

const MODEL: &[u8] = include_bytes!("../data/magika/standard_v3_3.onnx");
const BEG: usize = 1024;
const END: usize = 1024;
const BLOCK: usize = 4096;
const PAD: i32 = 256;
/// Fewer meaningful bytes than this: decided by a rule, not the model.
const MIN_FOR_MODEL: usize = 8;

type Plan = std::sync::Arc<TypedRunnableModel>;

fn plan() -> Option<&'static Plan> {
    static P: OnceLock<Option<Plan>> = OnceLock::new();
    P.get_or_init(|| {
        let m = tract_onnx::onnx()
            .model_for_read(&mut std::io::Cursor::new(MODEL))
            .and_then(|m| m.with_input_fact(0, i32::fact([1, BEG + END]).into()))
            .and_then(|m| m.into_optimized())
            .and_then(|m| m.into_runnable());
        match m {
            Ok(p) => Some(p),
            Err(e) => {
                eprintln!("grove: Magika's model didn't load: {e}");
                None
            }
        }
    })
    .as_ref()
}

/// What a file's content is.
#[derive(Clone, Debug)]
pub struct Guess {
    pub label: &'static str,
    pub description: &'static str,
    pub mime: &'static str,
    pub group: &'static str,
    pub extensions: &'static [&'static str],
    pub is_text: bool,
    /// The model's confidence (1 when a rule decided).
    pub score: f32,
}

fn by_label(l: &str) -> Option<&'static Label> {
    table::LABELS.iter().find(|x| x.label == l)
}

fn guess_of(l: &'static Label, score: f32) -> Guess {
    Guess { label: l.label, description: l.description, mime: l.mime, group: l.group, extensions: l.extensions, is_text: l.is_text, score }
}

fn generic(text: bool) -> Guess {
    if text {
        by_label("txt").map(|l| guess_of(l, 1.0)).unwrap_or(Guess { label: "txt", description: "Generic text document", mime: "text/plain", group: "text", extensions: &["txt"], is_text: true, score: 1.0 })
    } else {
        Guess { label: "unknown", description: "Unknown binary data", mime: "application/octet-stream", group: "unknown", extensions: &[], is_text: false, score: 1.0 }
    }
}

fn ws(b: u8) -> bool {
    b.is_ascii_whitespace() || b == 0x0b
}

/// Magika's features: [first 1 KiB after leading whitespace] then [last
/// 1 KiB before trailing whitespace, right-aligned], padded with 256.
fn features(f: &mut std::fs::File, len: u64) -> std::io::Result<(Vec<i32>, Vec<u8>)> {
    let n = (BLOCK as u64).min(len) as usize;
    let mut beg = vec![0u8; n];
    f.seek(SeekFrom::Start(0))?;
    f.read_exact(&mut beg)?;
    let mut end = vec![0u8; n];
    f.seek(SeekFrom::Start(len - n as u64))?;
    f.read_exact(&mut end)?;
    let b = {
        let i = beg.iter().position(|&c| !ws(c)).unwrap_or(beg.len());
        &beg[i..]
    };
    let e = {
        let i = end.iter().rposition(|&c| !ws(c)).map(|i| i + 1).unwrap_or(0);
        &end[..i]
    };
    let mut feat = vec![PAD; BEG + END];
    for (i, &c) in b.iter().take(BEG).enumerate() {
        feat[i] = c as i32;
    }
    let k = e.len().min(END);
    for (i, &c) in e[e.len() - k..].iter().enumerate() {
        feat[BEG + END - k + i] = c as i32;
    }
    Ok((feat, beg))
}

/// The content type of `path`, by Magika.
pub fn identify(path: &Path) -> Option<Guess> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    if len == 0 {
        return Some(Guess { label: "empty", description: "Empty file", mime: "inode/x-empty", group: "inode", extensions: &[], is_text: false, score: 1.0 });
    }
    let (feat, first) = features(&mut f, len).ok()?;
    // Too little to go on: text if it's UTF-8, else unknown (as Magika).
    if feat[MIN_FOR_MODEL - 1] == PAD {
        return Some(generic(std::str::from_utf8(&first).is_ok()));
    }
    let p = plan()?;
    let input = tract_ndarray::Array2::from_shape_vec((1, BEG + END), feat).ok()?.into_tensor();
    let out = p.run(tvec!(input.into())).ok()?;
    let scores = out[0].to_plain_array_view::<f32>().ok()?;
    let scores: Vec<f32> = scores.iter().copied().collect();
    let (best, &score) = scores.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))?;
    let l = table::LABELS.get(best)?;
    if score < l.threshold {
        return Some(Guess { score, ..generic(l.is_text) });
    }
    let l = by_label(l.overwrite).unwrap_or(l);
    Some(guess_of(l, score))
}
