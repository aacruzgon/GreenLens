//! The face check (task M7; [vision evaluation 4], face blur): runs the upload's face detector
//! and quality gate on photos, in the order `UploadProcessor::process` runs them, and prints
//! what they found. `ml/training/eval/face_check.py` drives it to score face recall and false
//! blurs, and to compare upload sizes (issue #41).
//!
//! ```text
//! cd backend
//! ls ~/photos/*.jpg | ORT_DYLIB_PATH=/path/to/libonnxruntime.so.1.31.0 \
//!   cargo run -q --release -p vision --example face_check -- --models-dir ~/.cache/pdm-models \
//!   [--score-threshold 0.3] [--pass-a-long-side 1280] [--tile 1280] [--tile-stride 960] [--threads 2]
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
//! [vision evaluation 4]: ../../../../docs/vision/evaluation.md

use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    time::Instant,
};

use domain::rules::uploads::stored_long_side;
use serde_json::{Value, json};
use vision::{
    imaging::{self, ImageView},
    manifest::Manifest,
    runtime::{OrtRuntime, RuntimeConfig},
    upload::{
        UploadConfig,
        faces::{FaceConfig, FaceDetector, blur_faces},
        quality,
    },
};

/// The command line.
struct Args {
    models_dir: PathBuf,
    faces: FaceConfig,
    threads: usize,
}

fn parse_args() -> Result<Args, String> {
    let mut models_dir = std::env::var_os("MODELS_DIR").map(PathBuf::from);
    let mut faces = FaceConfig {
        sessions: 1,
        ..UploadConfig::default().faces
    };
    let mut threads = RuntimeConfig::cpu("face_check").intra_threads;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
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
    })
}

fn load(args: &Args) -> Result<FaceDetector, vision::VisionError> {
    let runtime = OrtRuntime::init(&RuntimeConfig {
        dylib_path: std::env::var_os("ORT_DYLIB_PATH").map(PathBuf::from),
        intra_threads: args.threads,
        ..RuntimeConfig::cpu("face_check")
    })?;
    let manifest = Manifest::load(&args.models_dir.join("manifest.toml"))?;
    FaceDetector::load(&runtime, &manifest, args.faces)
}

fn ms(since: Instant) -> f64 {
    (since.elapsed().as_secs_f64() * 10_000.0).round() / 10.0
}

/// One photo through decode, the quality gate, detection, blur, resize and `WebP`.
fn check(path: &Path, detector: &FaceDetector, config: &UploadConfig) -> Result<Value, String> {
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
    let grown: Vec<_> = found.iter().map(|f| f.grown(detector.grow())).collect();
    let mut upright = decoded.image;
    blur_faces(&mut upright, &grown);
    let target = stored_long_side(width.max(height), false, config.default_long_side);
    let stored = imaging::resize_long_side(ImageView::from(&upright), target)
        .map_err(|e| format!("resize: {e}"))?;
    let webp = imaging::encode_webp(ImageView::from(&stored), config.webp_quality);
    let store_ms = ms(t);

    let faces: Vec<Value> = found
        .iter()
        .map(|f| json!({"x": f.x, "y": f.y, "w": f.w, "h": f.h, "score": f.score}))
        .collect();
    Ok(json!({
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
    }))
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
    let detector = match load(&args) {
        Ok(detector) => detector,
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
        let result =
            check(path, &detector, &config).unwrap_or_else(|e| json!({"path": path, "error": e}));
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
