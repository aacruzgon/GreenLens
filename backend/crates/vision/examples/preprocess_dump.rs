//! The worker's pre-processing on image files, written out as `.npy` arrays for J1's parity
//! check (vision dev-roadmap, gap G8): `python -m ml.training.eval.preprocess_parity run`
//! compares every array with `ml/pdm_vision/preprocess.py`.
//!
//! ```text
//! cd backend
//! echo '{"manifest": "/x/ml/models/manifest.cpu.toml", "out": "/x/dump",
//!        "photos": [{"id": "p0", "path": "/x/a.jpg"}],
//!        "crops": [{"id": "c0", "photo": "p0", "bbox": [0.1, 0.2, 0.3, 0.4]}]}' | tr -d '\n' \
//!   | cargo run -q --release -p vision --example preprocess_dump
//! ```
//!
//! Reads one JSON job per line on stdin. For each photo it runs the steps of the upload that
//! change pixels, as `UploadProcessor::process` runs them on a photo with no face and no depth
//! (decode with the EXIF orientation applied, resize to the stored long side, lossy `WebP` at
//! `UploadConfig::webp_quality`), then the worker's (`decode_stored`, then
//! `preprocess::to_tensor` with the `bioclip`, `leaf_detector` and `sam_encoder` recipes of the
//! manifest). For each crop it grows the normalised `[x, y, w, h]` box by the disease heads'
//! `crop_grow` and cuts it from the stored photo as the disease stage does (`perception.rs`,
//! `classify`), then makes `BioCLIP`'s tensor of it.
//!
//! Writes into `out`: `<photo>.decoded.npy`, `<photo>.resized.npy` and `<photo>.stored.npy`
//! (`u8`, `[h, w, 3]`: after decoding, after the resize, after the `WebP` round trip),
//! `<photo>.<recipe>.npy` (`f32`, `[3, side, side]`), `<crop>.crop.npy` (`u8`) and
//! `<crop>.bioclip.npy` (`f32`), and `index.json` with the sizes, rectangles and settings used.
//! Prints one JSON line per job: `{"index": path}` or `{"error": message}`.
//!
//! A diagnostic beside them, which the worker doesn't run: the same resizes as two single-axis
//! passes, horizontal first, the order Pillow uses (`*.hfirst.npy`, see [`resize_h_first`]).

use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
};

use image::RgbImage;
use ndarray::Array3;
use serde::Deserialize;
use vision::{
    imaging::{self, DecodeLimits, ImageView, PixelRect},
    manifest::{Manifest, ModelInput, ResizeRule},
    perception::preprocess,
    upload::UploadConfig,
};

/// The recipes the worker runs on a stored photo, by manifest entry.
const RECIPES: [&str; 3] = ["bioclip", "leaf_detector", "sam_encoder"];

#[derive(Deserialize)]
struct Job {
    manifest: PathBuf,
    out: PathBuf,
    photos: Vec<Photo>,
    #[serde(default)]
    crops: Vec<Crop>,
}

#[derive(Deserialize)]
struct Photo {
    id: String,
    path: PathBuf,
}

#[derive(Deserialize)]
struct Crop {
    id: String,
    photo: String,
    bbox: [f64; 4],
}

/// Writes a C-order `.npy` file (format 1.0): `descr` is `|u1` or `<f4`.
fn npy(path: &Path, descr: &str, shape: &[usize], data: &[u8]) -> Result<(), String> {
    let dims: Vec<String> = shape.iter().map(ToString::to_string).collect();
    let shape = if dims.len() == 1 {
        format!("({},)", dims[0])
    } else {
        format!("({})", dims.join(", "))
    };
    let mut header = format!("{{'descr': '{descr}', 'fortran_order': False, 'shape': {shape}, }}");
    // Magic (6) + version (2) + length (2) + header + '\n' is a multiple of 64.
    let pad = (64 - (10 + header.len() + 1) % 64) % 64;
    header.push_str(&" ".repeat(pad));
    header.push('\n');
    let len = u16::try_from(header.len()).map_err(|e| e.to_string())?;
    let mut bytes = Vec::with_capacity(10 + header.len() + data.len());
    bytes.extend_from_slice(b"\x93NUMPY\x01\x00");
    bytes.extend_from_slice(&len.to_le_bytes());
    bytes.extend_from_slice(header.as_bytes());
    bytes.extend_from_slice(data);
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn save_image(path: &Path, image: &RgbImage) -> Result<(), String> {
    let (w, h) = image.dimensions();
    npy(path, "|u1", &[h as usize, w as usize, 3], image.as_raw())
}

fn save_tensor(path: &Path, tensor: &Array3<f32>) -> Result<(), String> {
    // `iter` walks the logical (C) order whatever the memory layout.
    let data: Vec<u8> = tensor.iter().flat_map(|v| v.to_le_bytes()).collect();
    npy(path, "<f4", tensor.shape(), &data)
}

fn recipe<'m>(manifest: &'m Manifest, name: &str) -> Result<&'m ModelInput, String> {
    manifest
        .entry(name)
        .map_err(|e| e.to_string())?
        .input
        .as_ref()
        .ok_or_else(|| format!("{name} has no [model.input]"))
}

/// The disease heads' `crop_grow`, as `PerceptionConfig` reads it (0 with no head).
fn crop_grow(manifest: &Manifest) -> Result<f64, String> {
    let mut grow = None;
    for head in manifest.entries_with_prefix("disease_head") {
        let g = head
            .config
            .get("crop_grow")
            .and_then(toml::Value::as_float)
            .ok_or_else(|| format!("{} has no crop_grow", head.name))?;
        if grow.is_some_and(|known: f64| known.to_bits() != g.to_bits()) {
            return Err("the disease heads disagree on crop_grow".to_owned());
        }
        grow = Some(g);
    }
    Ok(grow.unwrap_or(0.0))
}

/// The disease stage's crop of one leaf box: the box grown by `grow` of its size on each side,
/// rounded outwards and clamped (`perception.rs`, `classify`).
fn leaf_rect(bbox: [f64; 4], grow: f64, size: (u32, u32)) -> ([f64; 4], PixelRect) {
    let [bx, by, bw, bh] = bbox;
    let grown = [
        bx - bw * grow,
        by - bh * grow,
        bw * (1.0 + 2.0 * grow),
        bh * (1.0 + 2.0 * grow),
    ];
    (grown, PixelRect::from_normalised(grown, size))
}

/// `imaging::resize` as two single-axis passes, horizontal first, as Pillow resamples. For 8-bit
/// images `fast_image_resize` 6.1 runs the vertical pass first (`Resizer::do_convolution`), and
/// each pass rounds and clips to 8 bits, so the two orders clip Lanczos3's overshoot at strong
/// edges differently. A diagnostic: the worker resizes with `imaging::resize`.
fn resize_h_first(view: ImageView<'_>, width: u32, height: u32) -> Result<RgbImage, String> {
    let wide = imaging::resize(view, width, view.height()).map_err(|e| e.to_string())?;
    imaging::resize(ImageView::from(&wide), width, height).map_err(|e| e.to_string())
}

/// `preprocess::to_tensor` with [`resize_h_first`] in place of `imaging::resize`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the same size arithmetic as preprocess::to_tensor, on image sides"
)]
fn tensor_h_first(image: ImageView<'_>, input: &ModelInput) -> Result<Array3<f32>, String> {
    let side = input.side().ok_or("the recipe has no fixed side")?;
    let square = match input.resize {
        ResizeRule::Squash => resize_h_first(image, side, side)?,
        ResizeRule::ShortestSideCrop => {
            let (w, h) = (image.width(), image.height());
            let scale = f64::from(side) / f64::from(w.min(h).max(1));
            let (rw, rh) = (
                ((f64::from(w) * scale).round() as u32).max(side),
                ((f64::from(h) * scale).round() as u32).max(side),
            );
            let r = resize_h_first(image, rw, rh)?;
            let rect = PixelRect {
                x: (rw - side) / 2,
                y: (rh - side) / 2,
                w: side,
                h: side,
            };
            imaging::crop(ImageView::from(&r), rect)
        }
        ResizeRule::LongestSidePad => {
            let (w, h) = imaging::fit_long_side((image.width(), image.height()), side);
            let fitted = resize_h_first(image, w, h)?;
            let mut padded = RgbImage::new(side, side);
            image::imageops::replace(&mut padded, &fitted, 0, 0);
            padded
        }
    };
    // Already `side × side`, so a squash only normalises.
    let mut normalise = input.clone();
    normalise.resize = ResizeRule::Squash;
    preprocess::to_tensor(ImageView::from(&square), &normalise).map_err(|e| e.to_string())
}

fn one(job: &Job) -> Result<PathBuf, String> {
    let manifest = Manifest::load(&job.manifest).map_err(|e| e.to_string())?;
    let recipes: Vec<(&str, &ModelInput)> = RECIPES
        .iter()
        .map(|&name| recipe(&manifest, name).map(|r| (name, r)))
        .collect::<Result<_, _>>()?;
    let grow = crop_grow(&manifest)?;
    let upload = UploadConfig::default();
    std::fs::create_dir_all(&job.out).map_err(|e| e.to_string())?;

    let mut photos = Vec::new();
    let mut stored_by_id = std::collections::HashMap::new();
    for photo in &job.photos {
        let bytes =
            std::fs::read(&photo.path).map_err(|e| format!("{}: {e}", photo.path.display()))?;
        let decoded = imaging::decode_upload(&bytes, DecodeLimits::default())
            .map_err(|e| format!("{}: {e}", photo.path.display()))?;
        let upright = decoded.image;
        let (w, h) = upright.dimensions();
        let target =
            domain::rules::uploads::stored_long_side(w.max(h), false, upload.default_long_side);
        let resized = imaging::resize_long_side(ImageView::from(&upright), target)
            .map_err(|e| e.to_string())?;
        let webp = imaging::encode_webp(ImageView::from(&resized), upload.webp_quality);
        let stored = imaging::decode_stored(&webp).map_err(|e| e.to_string())?;

        let base = |what: &str| job.out.join(format!("{}.{what}.npy", photo.id));
        save_image(&base("decoded"), &upright)?;
        save_image(&base("resized"), &resized)?;
        save_image(&base("stored"), &stored)?;
        let (rw, rh) = resized.dimensions();
        save_image(
            &base("resized.hfirst"),
            &resize_h_first(ImageView::from(&upright), rw, rh)?,
        )?;
        for &(name, input) in &recipes {
            let t = preprocess::to_tensor(ImageView::from(&stored), input)
                .map_err(|e| e.to_string())?;
            save_tensor(&base(name), &t)?;
            let diagnostic = tensor_h_first(ImageView::from(&stored), input)?;
            save_tensor(&base(&format!("{name}.hfirst")), &diagnostic)?;
        }
        photos.push(serde_json::json!({
            "id": photo.id,
            "path": photo.path,
            "decoded": [w, h],
            "stored": stored.dimensions(),
            "webp_bytes": webp.len(),
        }));
        stored_by_id.insert(photo.id.as_str(), stored);
    }

    let bioclip = recipe(&manifest, "bioclip")?;
    let mut crops = Vec::new();
    for crop in &job.crops {
        let stored = stored_by_id
            .get(crop.photo.as_str())
            .ok_or_else(|| format!("crop {}: no photo {}", crop.id, crop.photo))?;
        let (grown, rect) = leaf_rect(crop.bbox, grow, stored.dimensions());
        let pixels = imaging::crop(ImageView::from(stored), rect);
        let t =
            preprocess::to_tensor(ImageView::from(&pixels), bioclip).map_err(|e| e.to_string())?;
        save_image(&job.out.join(format!("{}.crop.npy", crop.id)), &pixels)?;
        save_tensor(&job.out.join(format!("{}.bioclip.npy", crop.id)), &t)?;
        save_tensor(
            &job.out.join(format!("{}.bioclip.hfirst.npy", crop.id)),
            &tensor_h_first(ImageView::from(&pixels), bioclip)?,
        )?;
        crops.push(serde_json::json!({
            "id": crop.id,
            "photo": crop.photo,
            "bbox": crop.bbox,
            "grown": grown,
            "rect": [rect.x, rect.y, rect.w, rect.h],
            "size": pixels.dimensions(),
        }));
    }

    let index = serde_json::json!({
        "manifest": job.manifest,
        "recipes": recipes.iter().map(|(name, r)| serde_json::json!({
            "name": name,
            "side": r.side(),
            "resize": format!("{:?}", r.resize),
            "mean": r.mean,
            "std": r.std,
        })).collect::<Vec<_>>(),
        "crop_grow": grow,
        "stored_long_side": upload.default_long_side,
        "webp_quality": upload.webp_quality,
        "photos": photos,
        "crops": crops,
    });
    let path = job.out.join("index.json");
    let text = serde_json::to_string_pretty(&index).map_err(|e| e.to_string())?;
    std::fs::write(&path, text + "\n").map_err(|e| e.to_string())?;
    Ok(path)
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
                    eprintln!("preprocess_dump: can't read the input: {e}");
                }
                std::process::exit(2);
            }
        };
        let result = match serde_json::from_str::<Job>(&line) {
            Ok(job) => match one(&job) {
                Ok(index) => serde_json::json!({"index": index}),
                Err(e) => serde_json::json!({"error": e}),
            },
            Err(e) => serde_json::json!({"error": e.to_string()}),
        };
        // reason: a closed stdout means the caller stopped reading; nothing to report to.
        let _ = writeln!(out, "{result}");
    }
}
