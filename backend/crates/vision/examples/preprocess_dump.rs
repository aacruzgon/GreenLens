//! The upload's and the worker's pre-processing on image files, written out as `.npy` arrays for
//! J1's parity check (vision dev-roadmap, gap G8): `python -m ml.training.eval.preprocess_parity
//! run` compares every array with `ml/pdm_vision/preprocess.py`.
//!
//! ```text
//! cd backend
//! echo '{"manifest": "/x/ml/models/manifest.toml", "upload_manifest": "/models/manifest.toml",
//!        "out": "/x/dump", "photos": [{"id": "p0", "path": "/x/a.jpg"}],
//!        "crops": [{"id": "c0", "photo": "p0", "bbox": [0.1, 0.2, 0.3, 0.4]}]}' | tr -d '\n' \
//!   | ORT_DYLIB_PATH=/x/libonnxruntime.so cargo run -q --release -p vision --example preprocess_dump
//! ```
//!
//! Reads one JSON job per line on stdin. Each photo goes through `UploadProcessor::process` as a
//! `whole_plant` shot with no depth (the quality gate, `YuNet` and the face blur on the CPU, the
//! resize to the stored long side, lossy `WebP`), loaded from `upload_manifest`, a directory
//! fetched with `python3 -m ml.models.fetch --for api`. The worker's side then runs on the stored
//! `WebP`: `decode_stored`, then `preprocess::to_tensor` with the `bioclip`, `leaf_detector` and
//! `sam_encoder` recipes of `manifest`. For each crop it grows the normalised `[x, y, w, h]` box by
//! `PerceptionConfig`'s `crop_grow` and cuts it from the stored photo with `classify`'s arithmetic
//! (see [`leaf_rect`]), then makes `BioCLIP`'s tensor of it.
//!
//! Writes into `out`: `<photo>.decoded.npy` and `<photo>.resized.npy` (`u8`, `[h, w, 3]`: the
//! upload's decode and resize, without the face blur), `<photo>.webp.npy` (the resized photo
//! through the same `WebP`, still without the blur), `<photo>.stored.npy` (what `process` stored,
//! decoded as the worker does), `<photo>.<recipe>.npy` (`f32`, `[3, side, side]`),
//! `<crop>.crop.npy` (`u8`) and `<crop>.bioclip.npy` (`f32`), and `index.json` with the sizes,
//! rectangles, settings and whether the face blur changed the stored photo. Prints one JSON line
//! per job: `{"index": path}` or `{"error": message}`.
//!
//! A diagnostic beside them, which the worker doesn't run: the same resizes as two single-axis
//! passes, horizontal first, the order Pillow uses (`*.hfirst.npy`, see [`resize_h_first`]).

use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
};

use domain::scan::ImageKind;
use image::RgbImage;
use ndarray::Array3;
use serde::Deserialize;
use vision::{
    imaging::{self, ImageView, PixelRect},
    manifest::{Manifest, ModelInput, ResizeRule},
    perception::{BatchCaps, PerceptionConfig, preprocess},
    runtime::{OrtEp, OrtRuntime, RuntimeConfig},
    upload::{UploadConfig, UploadInput, UploadProcessor},
};

/// The recipes the worker runs on a stored photo, by manifest entry.
const RECIPES: [&str; 3] = ["bioclip", "leaf_detector", "sam_encoder"];

#[derive(Deserialize)]
struct Job {
    /// The recipes and `crop_grow`: the repository's manifest.
    manifest: PathBuf,
    /// `YuNet` for the upload: the manifest of a directory `fetch --for api` filled.
    upload_manifest: PathBuf,
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

/// The disease stage's crop of one leaf box: the box grown by `grow` of its size on each side,
/// rounded outwards and clamped. A copy of the arithmetic inside `classify` (`perception.rs`), which
/// isn't a function of its own; `ml/tests/test_preprocess_parity.py` checks `pp.crop_box` against
/// the rectangles it gives.
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

/// One photo through the upload and the worker's recipes: its `index.json` row and the stored
/// photo.
fn one_photo(
    processor: &UploadProcessor,
    upload: &UploadConfig,
    recipes: &[(&str, &ModelInput)],
    out: &Path,
    photo: &Photo,
) -> Result<(serde_json::Value, RgbImage), String> {
    let at = |e: &dyn std::fmt::Display| format!("{}: {e}", photo.path.display());
    let bytes = std::fs::read(&photo.path).map_err(|e| at(&e))?;
    let processed = processor
        .process(UploadInput {
            kind: ImageKind::WholePlant,
            file: &bytes,
            depth: None,
        })
        .map_err(|e| at(&e))?;
    let stored = imaging::decode_stored(&processed.image.webp).map_err(|e| at(&e))?;

    // The same steps without the face blur, to tell the layers apart.
    let upright = imaging::decode_upload(&bytes, upload.limits)
        .map_err(|e| at(&e))?
        .image;
    let (w, h) = upright.dimensions();
    let resized = imaging::resize_long_side(
        ImageView::from(&upright),
        stored.width().max(stored.height()),
    )
    .map_err(|e| at(&e))?;
    if resized.dimensions() != stored.dimensions() {
        return Err(at(&"the resize without the blur has another size"));
    }
    let webp = imaging::decode_stored(&imaging::encode_webp(
        ImageView::from(&resized),
        upload.webp_quality,
    ))
    .map_err(|e| at(&e))?;

    let base = |what: &str| out.join(format!("{}.{what}.npy", photo.id));
    save_image(&base("decoded"), &upright)?;
    save_image(&base("resized"), &resized)?;
    save_image(&base("webp"), &webp)?;
    save_image(&base("stored"), &stored)?;
    let (rw, rh) = resized.dimensions();
    save_image(
        &base("resized.hfirst"),
        &resize_h_first(ImageView::from(&upright), rw, rh)?,
    )?;
    for &(name, input) in recipes {
        let t =
            preprocess::to_tensor(ImageView::from(&stored), input).map_err(|e| e.to_string())?;
        save_tensor(&base(name), &t)?;
        let diagnostic = tensor_h_first(ImageView::from(&stored), input)?;
        save_tensor(&base(&format!("{name}.hfirst")), &diagnostic)?;
    }
    let row = serde_json::json!({
        "id": photo.id,
        "path": photo.path,
        "decoded": [w, h],
        "stored": stored.dimensions(),
        "webp_bytes": processed.image.webp.len(),
        "blurred": webp.as_raw() != stored.as_raw(),
        "quality": {
            "blur_score": processed.quality.blur_score,
            "exposure": processed.quality.exposure,
        },
    });
    Ok((row, stored))
}

fn one(job: &Job) -> Result<PathBuf, String> {
    let manifest = Manifest::load(&job.manifest).map_err(|e| e.to_string())?;
    let recipes: Vec<(&str, &ModelInput)> = RECIPES
        .iter()
        .map(|&name| recipe(&manifest, name).map(|r| (name, r)))
        .collect::<Result<_, _>>()?;
    let grow = PerceptionConfig::from_manifest(&manifest, BatchCaps::default())
        .map_err(|e| e.to_string())?
        .crop_grow;
    let upload = UploadConfig::default();
    let runtime = OrtRuntime::init(&RuntimeConfig {
        ep: OrtEp::Cpu,
        device_id: 0,
        dylib_path: std::env::var_os("ORT_DYLIB_PATH").map(PathBuf::from),
        intra_threads: 4,
        name: "preprocess-dump",
    })
    .map_err(|e| format!("ONNX Runtime: {e}"))?;
    let upload_manifest = Manifest::load(&job.upload_manifest).map_err(|e| e.to_string())?;
    let processor = UploadProcessor::load(&runtime, &upload_manifest, upload)
        .map_err(|e| format!("the upload's face detector: {e}"))?;
    std::fs::create_dir_all(&job.out).map_err(|e| e.to_string())?;

    let mut photos = Vec::new();
    let mut stored_by_id = std::collections::HashMap::new();
    for photo in &job.photos {
        let (row, stored) = one_photo(&processor, &upload, &recipes, &job.out, photo)?;
        photos.push(row);
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
        "upload_manifest": job.upload_manifest,
        "face_model": processor.face_model_version(),
        "upload_kind": "whole_plant",
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
