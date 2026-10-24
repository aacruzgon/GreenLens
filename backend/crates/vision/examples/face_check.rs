//! The face check (task M7; [vision evaluation 4], face blur): runs the upload's face detector
//! and quality gate on photos, in the order `UploadProcessor::process` runs them, and prints
//! what they found. `ml/training/eval/face_check.py` drives it to score face recall and false
//! blurs, and to compare upload sizes (issue #41).
//!
//! ```text
//! cd backend
//! ls ~/photos/*.jpg | ORT_DYLIB_PATH=/path/to/libonnxruntime.so.1.31.0 \
//!   cargo run -q --release -p vision --example face_check -- --models-dir ~/.cache/pdm-models \
//!   [--score-threshold 0.3] [--pass-a-long-side 1280] [--tile 1280] [--tile-stride 960] [--threads 2] \
//!   [--landmarks]
//! ```
//!
//! Loads `yunet` from `<models-dir>/manifest.toml` (hash checked) on the CPU, as `api` does, with
//! `FaceConfig::default()` and the overrides given. Reads one image path per line on stdin and
//! writes one JSON line per path: `path`; `width` and `height` (upright); the gate's
//! `blur_score`, `exposure`, `dark_share`, `bright_share` and `verdict` (its problem, or `null`);
//! `faces`, the merged boxes normalised to the upright image **before** the 25% growth, each
//! `{x, y, w, h, score}`; `stored`, the stored `WebP`'s `width`, `height` and `bytes`; and `ms`:
//! `decode`, `quality`, `detect`, `store` (grow, blur, resize and encode) and `total`. Unlike
//! `process`, a shot that fails the gate still goes through face detection, so every photo is
//! measured. A file that fails gives `{"path", "error"}`. Exit code 0; 2 when the arguments are
//! wrong or the detector doesn't load.
//!
//! `--landmarks` (a prototype check; `api` ignores the landmarks) runs `detect`'s passes a second
//! time on a session of its own, with a decode that keeps `YuNet`'s five landmarks (`kps`: right
//! eye, left eye, nose tip, right and left mouth corners), and merges them as `union_merge` does
//! while keeping each merged box's members. Each face then has `members`: every pass box merged
//! into it, as `{pass, x, y, w, h, score, kps}`, `pass` `"whole"` or `"tile"`, the box and the
//! five `[x, y]` points in pixels of the upright image, not clamped. `landmarks_match` says
//! whether the merged boxes equal `detect`'s exactly; when they don't, the faces carry no
//! `members`. `ms.landmarks` is the second run's time.
//!
//! [vision evaluation 4]: ../../../../docs/vision/evaluation.md
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "the landmark run repeats faces.rs's pixel arithmetic, which the vision crate allows"
)]

use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    time::Instant,
};

use domain::rules::uploads::stored_long_side;
use ndarray::Array4;
use ort::{inputs, session::Session, value::TensorRef};
use serde_json::{Value, json};
use vision::{
    VisionError,
    imaging::{self, ImageView, PixelRect},
    manifest::Manifest,
    runtime::{OrtRuntime, RuntimeConfig, SessionSettings},
    upload::{
        UploadConfig,
        faces::{FaceBox, FaceConfig, FaceDetector, blur_faces, nms, tile_origins},
        quality,
    },
};

/// The command line.
struct Args {
    models_dir: PathBuf,
    faces: FaceConfig,
    threads: usize,
    landmarks: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut models_dir = std::env::var_os("MODELS_DIR").map(PathBuf::from);
    let mut faces = FaceConfig {
        sessions: 1,
        ..UploadConfig::default().faces
    };
    let mut threads = RuntimeConfig::cpu("face_check").intra_threads;
    let mut landmarks = false;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--landmarks" {
            landmarks = true;
            continue;
        }
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        let number = |what: &str| format!("{flag}: {what}, not {value:?}");
        match flag.as_str() {
            "--models-dir" => models_dir = Some(PathBuf::from(&value)),
            "--score-threshold" => {
                faces.score_threshold = value.parse().map_err(|_| number("a number"))?;
            }
            "--pass-a-long-side" => {
                faces.pass_a_long_side = value.parse().map_err(|_| number("pixels"))?;
            }
            "--tile" => faces.tile = value.parse().map_err(|_| number("pixels"))?,
            "--tile-stride" => faces.tile_stride = value.parse().map_err(|_| number("pixels"))?,
            "--threads" => threads = value.parse().map_err(|_| number("a count"))?,
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    let models_dir = models_dir.ok_or("give --models-dir or set MODELS_DIR")?;
    Ok(Args {
        models_dir,
        faces,
        threads,
        landmarks,
    })
}

/// The detector, and with `--landmarks` a second `YuNet` session for the landmark run.
fn load(args: &Args) -> Result<(FaceDetector, Option<Session>), VisionError> {
    let runtime = OrtRuntime::init(&RuntimeConfig {
        dylib_path: std::env::var_os("ORT_DYLIB_PATH").map(PathBuf::from),
        intra_threads: args.threads,
        ..RuntimeConfig::cpu("face_check")
    })?;
    let manifest = Manifest::load(&args.models_dir.join("manifest.toml"))?;
    let detector = FaceDetector::load(&runtime, &manifest, args.faces)?;
    let session = if args.landmarks {
        let entry = manifest.entry(FaceDetector::MODEL)?;
        Some(runtime.session(&manifest.path(entry), SessionSettings::default())?)
    } else {
        None
    };
    Ok((detector, session))
}

/// A pass box `[x, y, w, h, score]` and its ten landmark coordinates.
type Detection = ([f32; 5], [f32; 10]);

/// One pass box with its landmarks, in pixels of the upright image (not clamped).
#[derive(Debug, Clone, Copy)]
struct Member {
    pass: &'static str,
    rect: [f32; 4],
    score: f32,
    kps: [f32; 10],
}

impl Member {
    fn to_json(self) -> Value {
        let points: Vec<[f32; 2]> = self.kps.as_chunks::<2>().0.to_vec();
        json!({
            "pass": self.pass, "x": self.rect[0], "y": self.rect[1], "w": self.rect[2],
            "h": self.rect[3], "score": self.score, "kps": points,
        })
    }
}

/// `FaceDetector::detect`'s passes again, keeping the landmarks: the merged boxes (which must
/// equal `detect`'s) and, for each, the pass boxes merged into it.
#[allow(
    clippy::many_single_char_names,
    reason = "w, h, tx, ty: image and tile geometry, as in faces.rs"
)]
fn detect_with_landmarks(
    session: &mut Session,
    image: ImageView<'_>,
    config: &FaceConfig,
) -> Result<(Vec<FaceBox>, Vec<Vec<Member>>), VisionError> {
    let (w, h) = (image.width(), image.height());
    let (mut found, mut members) = (Vec::new(), Vec::new());
    let small = imaging::resize_long_side(image, config.pass_a_long_side)?;
    let scale = (f64::from(w) / f64::from(small.width())) as f32;
    for (b, kps) in run_pass(session, ImageView::from(&small), config)? {
        found.push(normalise(b, (0, 0), scale, (w, h)));
        members.push(vec![member("whole", b, kps, (0, 0), scale)]);
    }
    if w.max(h) > config.tile {
        for (tx, ty) in tile_origins((w, h), config.tile, config.tile_stride) {
            let rect = PixelRect {
                x: tx,
                y: ty,
                w: config.tile.min(w - tx),
                h: config.tile.min(h - ty),
            };
            let tile = imaging::crop(image, rect);
            for (b, kps) in run_pass(session, ImageView::from(&tile), config)? {
                found.push(normalise(b, (tx, ty), 1.0, (w, h)));
                members.push(vec![member("tile", b, kps, (tx, ty), 1.0)]);
            }
        }
    }
    Ok(union_merge_members(found, members, config.merge_iou))
}

/// `faces::run_pass` with the landmarks: BGR, 0–255, zero-padded to multiples of 32; each box
/// `[x, y, w, h, score]` and its ten landmark coordinates, in the pass image's pixels, after `NMS`.
#[allow(
    clippy::many_single_char_names,
    reason = "w, h, x, y, r, g, b: image notation, as in faces.rs"
)]
fn run_pass(
    session: &mut Session,
    image: ImageView<'_>,
    config: &FaceConfig,
) -> Result<Vec<Detection>, VisionError> {
    let (w, h) = (image.width(), image.height());
    let (pw, ph) = (w.div_ceil(32) * 32, h.div_ceil(32) * 32);
    let mut input = Array4::<f32>::zeros((1, 3, ph as usize, pw as usize));
    for y in 0..h {
        for x in 0..w {
            let [r, g, b] = image.rgb(x, y);
            let (yy, xx) = (y as usize, x as usize);
            input[[0, 0, yy, xx]] = f32::from(b);
            input[[0, 1, yy, xx]] = f32::from(g);
            input[[0, 2, yy, xx]] = f32::from(r);
        }
    }
    let outputs = session.run(inputs!["input" => TensorRef::from_array_view(&input)?])?;
    let mut decoded = Vec::new();
    for stride in [8u32, 16, 32] {
        let tensor = |prefix: &str| -> Result<&[f32], VisionError> {
            let name = format!("{prefix}_{stride}");
            let value = outputs
                .get(name.as_str())
                .ok_or_else(|| VisionError::Shape {
                    model: FaceDetector::MODEL,
                    detail: format!("missing output {name}"),
                })?;
            Ok(value.try_extract_tensor::<f32>()?.1)
        };
        let (cls, obj, bbox, kps) = (
            tensor("cls")?,
            tensor("obj")?,
            tensor("bbox")?,
            tensor("kps")?,
        );
        decoded.extend(decode_with_landmarks(
            stride,
            cls,
            obj,
            bbox,
            kps,
            pw,
            config.score_threshold,
        ));
    }
    // NMS by index, as `nms` does it (a stable sort by score, then greedy), so the landmarks
    // stay with their boxes; checked against `nms` itself.
    let boxes: Vec<FaceBox> = decoded.iter().map(|(b, _)| face_box(*b)).collect();
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by(|&a, &b| boxes[b].score.total_cmp(&boxes[a].score));
    let mut kept: Vec<usize> = Vec::new();
    for i in order {
        if kept
            .iter()
            .all(|&k| boxes[k].iou(&boxes[i]) <= config.nms_iou)
        {
            kept.push(i);
        }
    }
    let ours: Vec<FaceBox> = kept.iter().map(|&i| boxes[i]).collect();
    if ours != nms(boxes.clone(), config.nms_iou) {
        return Err(VisionError::Shape {
            model: FaceDetector::MODEL,
            detail: "NMS with landmarks differs from faces::nms".to_owned(),
        });
    }
    Ok(kept.into_iter().map(|i| decoded[i]).collect())
}

/// `faces::decode_stride` plus the landmarks, decoded as `OpenCV`'s `face_detect.cpp` does:
/// point n = ((col + kps[2n])·s, (row + kps[2n+1])·s).
fn decode_with_landmarks(
    stride: u32,
    class: &[f32],
    obj: &[f32],
    bbox: &[f32],
    kps: &[f32],
    padded_width: u32,
    threshold: f32,
) -> Vec<Detection> {
    let cols = (padded_width / stride) as usize;
    let s = stride as f32;
    let mut out = Vec::new();
    for (i, (&c, &o)) in class.iter().zip(obj).enumerate() {
        let score = (c.clamp(0.0, 1.0) * o.clamp(0.0, 1.0)).sqrt();
        if score < threshold || cols == 0 {
            continue;
        }
        let (row, col) = ((i / cols) as f32, (i % cols) as f32);
        let b = &bbox[i * 4..i * 4 + 4];
        let (cx, cy) = ((col + b[0]) * s, (row + b[1]) * s);
        let (w, h) = (b[2].exp() * s, b[3].exp() * s);
        let mut points = [0.0f32; 10];
        for (n, p) in kps[i * 10..i * 10 + 10]
            .as_chunks::<2>()
            .0
            .iter()
            .enumerate()
        {
            points[2 * n] = (col + p[0]) * s;
            points[2 * n + 1] = (row + p[1]) * s;
        }
        out.push(([cx - w / 2.0, cy - h / 2.0, w, h, score], points));
    }
    out
}

const fn face_box(b: [f32; 5]) -> FaceBox {
    FaceBox {
        x: b[0],
        y: b[1],
        w: b[2],
        h: b[3],
        score: b[4],
    }
}

/// `faces::normalise`, the same arithmetic.
fn normalise(b: [f32; 5], origin: (u32, u32), scale: f32, size: (u32, u32)) -> FaceBox {
    let (iw, ih) = (size.0 as f32, size.1 as f32);
    let x = (b[0] * scale + origin.0 as f32).clamp(0.0, iw);
    let y = (b[1] * scale + origin.1 as f32).clamp(0.0, ih);
    let x1 = ((b[0] + b[2]) * scale + origin.0 as f32).clamp(0.0, iw);
    let y1 = ((b[1] + b[3]) * scale + origin.1 as f32).clamp(0.0, ih);
    FaceBox {
        x: x / iw,
        y: y / ih,
        w: (x1 - x) / iw,
        h: (y1 - y) / ih,
        score: b[4],
    }
}

/// A pass box and its landmarks in pixels of the upright image.
fn member(
    pass: &'static str,
    b: [f32; 5],
    kps: [f32; 10],
    origin: (u32, u32),
    scale: f32,
) -> Member {
    let (ox, oy) = (origin.0 as f32, origin.1 as f32);
    let mut points = kps;
    for p in points.as_chunks_mut::<2>().0 {
        p[0] = p[0] * scale + ox;
        p[1] = p[1] * scale + oy;
    }
    Member {
        pass,
        rect: [
            b[0] * scale + ox,
            b[1] * scale + oy,
            b[2] * scale,
            b[3] * scale,
        ],
        score: b[4],
        kps: points,
    }
}

/// `faces::union_merge`, step for step, carrying each box's members along.
fn union_merge_members(
    mut boxes: Vec<FaceBox>,
    mut members: Vec<Vec<Member>>,
    iou: f32,
) -> (Vec<FaceBox>, Vec<Vec<Member>>) {
    loop {
        let mut merged = false;
        'outer: for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                if boxes[i].iou(&boxes[j]) > iou || boxes[i].contains_or_inside(&boxes[j]) {
                    let u = boxes[i].union(&boxes[j]);
                    boxes.swap_remove(j);
                    let moved = members.swap_remove(j);
                    boxes[i] = u;
                    members[i].extend(moved);
                    merged = true;
                    break 'outer;
                }
            }
        }
        if !merged {
            return (boxes, members);
        }
    }
}

fn ms(since: Instant) -> f64 {
    (since.elapsed().as_secs_f64() * 10_000.0).round() / 10.0
}

/// One photo through decode, the quality gate, detection, blur, resize and `WebP`.
fn check(
    path: &Path,
    detector: &FaceDetector,
    landmarks: Option<&mut Session>,
    config: &UploadConfig,
) -> Result<Value, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read: {e}"))?;
    let start = Instant::now();
    let decoded =
        imaging::decode_upload(&bytes, config.limits).map_err(|e| format!("decode: {e}"))?;
    let decode_ms = ms(start);
    let view = ImageView::from(&decoded.image);
    let (width, height) = decoded.image.dimensions();

    let t = Instant::now();
    let scores = quality::score(view, &config.quality).map_err(|e| format!("quality: {e}"))?;
    let verdict = quality::verdict(&scores, &config.quality);
    let quality_ms = ms(t);

    let t = Instant::now();
    let found = detector.detect(view).map_err(|e| format!("detect: {e}"))?;
    let detect_ms = ms(t);

    let t = Instant::now();
    let with_landmarks = match landmarks {
        Some(session) => Some(
            detect_with_landmarks(session, view, &config.faces)
                .map_err(|e| format!("landmarks: {e}"))?,
        ),
        None => None,
    };
    let landmarks_ms = ms(t);

    let t = Instant::now();
    let grown: Vec<_> = found.iter().map(|f| f.grown(detector.grow())).collect();
    let mut upright = decoded.image;
    blur_faces(&mut upright, &grown);
    let target = stored_long_side(width.max(height), false, config.default_long_side);
    let stored = imaging::resize_long_side(ImageView::from(&upright), target)
        .map_err(|e| format!("resize: {e}"))?;
    let webp = imaging::encode_webp(ImageView::from(&stored), config.webp_quality);
    let store_ms = ms(t);

    let matched = with_landmarks.as_ref().map(|(boxes, _)| *boxes == found);
    let faces: Vec<Value> = found
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let mut face = json!({"x": f.x, "y": f.y, "w": f.w, "h": f.h, "score": f.score});
            if let (Some(true), Some((_, members))) = (matched, &with_landmarks) {
                face["members"] = members[i].iter().map(|m| m.to_json()).collect();
            }
            face
        })
        .collect();
    let mut result = json!({
        "path": path,
        "width": width,
        "height": height,
        "blur_score": scores.blur_score,
        "exposure": scores.exposure,
        "dark_share": scores.dark_share,
        "bright_share": scores.bright_share,
        "verdict": verdict.map(|p| format!("{p:?}")),
        "faces": faces,
        "stored": {"width": stored.width(), "height": stored.height(), "bytes": webp.len()},
        "ms": {
            "decode": decode_ms,
            "quality": quality_ms,
            "detect": detect_ms,
            "store": store_ms,
            "total": ms(start),
        },
    });
    if let Some(matched) = matched {
        result["landmarks_match"] = json!(matched);
        result["ms"]["landmarks"] = json!(landmarks_ms);
    }
    Ok(result)
}

#[allow(clippy::print_stderr, reason = "a command-line tool's errors")]
fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("face_check: {e}");
            return ExitCode::from(2);
        }
    };
    let (detector, mut landmarks) = match load(&args) {
        Ok(loaded) => loaded,
        Err(e) => {
            eprintln!("face_check: can't load the face detector: {e}");
            return ExitCode::from(2);
        }
    };
    let config = UploadConfig {
        faces: args.faces,
        ..UploadConfig::default()
    };
    let mut out = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let path = Path::new(line.trim());
        if path.as_os_str().is_empty() {
            continue;
        }
        let result = check(path, &detector, landmarks.as_mut(), &config)
            .unwrap_or_else(|e| json!({"path": path, "error": e}));
        // reason: a closed stdout means the caller stopped reading; nothing to report to.
        if writeln!(out, "{result}")
            .and_then(|()| out.flush())
            .is_err()
        {
            break;
        }
    }
    ExitCode::SUCCESS
}
