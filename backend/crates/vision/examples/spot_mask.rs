//! The spot segmenter (`ColourSpotSegmenter`) on image files, with thresholds given as JSON:
//! `python -m ml.training.eval.spot_fit parity` checks that its Python copy of the segmenter, which
//! fits the thresholds, classifies every pixel as this code does.
//!
//! ```text
//! cd backend
//! echo '{"image": "/x/a.jpg", "mask": "/x/a.leaf.png", "out": "/x/a.classes.png", "thresholds": {}}' \
//!   | cargo run -q -p vision --example spot_mask
//! ```
//!
//! Reads one JSON object per line on stdin: `image` (any format the `image` crate reads),
//! `mask` (a greyscale PNG of the same size, non-zero inside the leaf), `out` (where the class
//! map goes, a greyscale PNG: 0 outside, 1 leaf, 2 necrotic, 3 halo) and `thresholds`, any
//! `SpotThresholds` field by name (the others keep their defaults). Writes one JSON line per
//! input: `{"out", "counts": [outside, leaf, necrotic, halo]}` or `{"out", "error"}`.

use std::io::{BufRead, Write};

use domain::ports::{MaskView, RgbView, SpotSegmenter};
use serde::Deserialize;
use vision::measure::spots::{ColourSpotSegmenter, SpotThresholds};

#[derive(Deserialize)]
struct Job {
    image: std::path::PathBuf,
    mask: std::path::PathBuf,
    out: std::path::PathBuf,
    #[serde(default)]
    thresholds: serde_json::Map<String, serde_json::Value>,
}

fn thresholds(
    given: &serde_json::Map<String, serde_json::Value>,
) -> Result<SpotThresholds, String> {
    let mut t = SpotThresholds::default();
    for (key, value) in given {
        let v = value
            .as_f64()
            .ok_or_else(|| format!("{key} isn't a number"))?;
        let field = match key.as_str() {
            "dark_value" => &mut t.dark_value,
            "brown_hue_min" => &mut t.brown_hue_min,
            "brown_hue_max" => &mut t.brown_hue_max,
            "brown_saturation" => &mut t.brown_saturation,
            "brown_value" => &mut t.brown_value,
            "pale_saturation" => &mut t.pale_saturation,
            "pale_value" => &mut t.pale_value,
            "halo_hue_min" => &mut t.halo_hue_min,
            "halo_hue_max" => &mut t.halo_hue_max,
            "halo_saturation" => &mut t.halo_saturation,
            other => return Err(format!("unknown threshold {other}")),
        };
        *field = v;
    }
    Ok(t)
}

fn one(job: &Job) -> Result<[u64; 4], String> {
    let rgb = image::open(&job.image)
        .map_err(|e| e.to_string())?
        .into_rgb8();
    let mask = image::open(&job.mask)
        .map_err(|e| e.to_string())?
        .into_luma8();
    if rgb.dimensions() != mask.dimensions() {
        return Err(format!(
            "image {:?} and mask {:?} differ in size",
            rgb.dimensions(),
            mask.dimensions()
        ));
    }
    let (w, h) = rgb.dimensions();
    let segmenter = ColourSpotSegmenter {
        thresholds: thresholds(&job.thresholds)?,
    };
    let image = RgbView::new(w, h, rgb.as_raw()).ok_or("bad image view")?;
    let leaf = MaskView::new(w, h, mask.as_raw()).ok_or("bad mask view")?;
    let classes = segmenter.segment(image, leaf).classes;
    let mut counts = [0u64; 4];
    let codes: Vec<u8> = classes
        .iter()
        .map(|&c| {
            let code = c as u8;
            counts[usize::from(code)] += 1;
            code
        })
        .collect();
    image::GrayImage::from_raw(w, h, codes)
        .ok_or("bad class map")?
        .save(&job.out)
        .map_err(|e| e.to_string())?;
    Ok(counts)
}

fn main() {
    let mut out = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let line = match line {
            Ok(l) if l.trim().is_empty() => continue,
            Ok(l) => l,
            Err(e) => {
                #[allow(clippy::print_stderr, reason = "a command-line tool's error")]
                {
                    eprintln!("spot_mask: can't read the input: {e}");
                }
                std::process::exit(2);
            }
        };
        let result = match serde_json::from_str::<Job>(&line) {
            Ok(job) => match one(&job) {
                Ok(counts) => serde_json::json!({"out": job.out, "counts": counts}),
                Err(e) => serde_json::json!({"out": job.out, "error": e}),
            },
            Err(e) => serde_json::json!({"error": e.to_string()}),
        };
        // reason: a closed stdout means the caller stopped reading; nothing to report to.
        let _ = writeln!(out, "{result}");
    }
}
