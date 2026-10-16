//! The CPU upload pipeline `api` runs for `POST /scans/{id}/images` ([contracts 3]) and chat
//! images: decode with EXIF orientation, the quality gate, face detection and blurring (fail
//! closed), resize, lossy `WebP`, and the depth path. Everything is synchronous and borrows the
//! request's bytes; `api` calls it inside `spawn_blocking`.
//!
//! [contracts 3]: ../../../docs/architecture/contracts.md#post-scansidimages

use bytes::Bytes;
use domain::{
    error::QualityProblem,
    rules::uploads::stored_long_side,
    scan::{DepthProblem, ImageKind, QualityReport},
    wire::BoolLit,
};

use crate::{
    VisionError,
    depth::{DepthContext, process_depth},
    imaging::{self, DecodeError, DecodeLimits, ImageView},
    manifest::Manifest,
    runtime::OrtRuntime,
};

pub mod faces;
pub mod quality;

pub use crate::depth::{DepthInput, ProcessedDepth};

use faces::{FaceConfig, FaceDetector, blur_faces};
use quality::QualityConfig;

/// The pipeline's settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UploadConfig {
    /// Decoder bounds.
    pub limits: DecodeLimits,
    /// The stored long side of a photo without depth. Contracts 3 fixes only the `LiDAR` rule
    /// (at least 2048); 1600 keeps an ordinary photo at 100–500 KB of `WebP`.
    pub default_long_side: u32,
    /// libwebp quality, 0–100.
    pub webp_quality: f32,
    /// The quality gate.
    pub quality: QualityConfig,
    /// The face detector.
    pub faces: FaceConfig,
    /// How much more each blur box grows when it zeroes the depth map ([contracts 3]: 10%).
    pub depth_face_growth: f32,
}

impl Default for UploadConfig {
    fn default() -> Self {
        Self {
            limits: DecodeLimits::default(),
            default_long_side: 1600,
            webp_quality: 82.0,
            quality: QualityConfig::default(),
            faces: FaceConfig::default(),
            depth_face_growth: 0.10,
        }
    }
}

/// One upload, borrowed from the multipart body for the duration of the call. `'a` is that
/// borrow: `api` moves the owned `Bytes` into the blocking closure and borrows them there, so no
/// image is copied to cross onto the blocking thread.
#[derive(Debug, Clone, Copy)]
pub struct UploadInput<'a> {
    /// The shot's kind.
    pub kind: ImageKind,
    /// The `file` part.
    pub file: &'a [u8],
    /// The depth parts, when sent.
    pub depth: Option<DepthInput<'a>>,
}

/// A stored-ready image.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedImage {
    /// Lossy `WebP`, upright, faces blurred, no metadata.
    pub webp: Bytes,
    /// Width of the stored image.
    pub width: u32,
    /// Height of the stored image.
    pub height: u32,
}

/// The result of [`UploadProcessor::process`].
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedUpload {
    /// The image to store.
    pub image: ProcessedImage,
    /// `UploadImageResponse.quality`.
    pub quality: QualityReport,
    /// Present when depth parts were sent: the depth to store, or why it was dropped.
    pub depth: Option<Result<ProcessedDepth, DepthProblem>>,
}

/// Why an upload is refused. Depth problems are never here: they ride in
/// [`ProcessedUpload::depth`].
#[derive(Debug, thiserror::Error)]
pub enum UploadError {
    /// `422 image_quality`, with `hint = problem.hint()`.
    #[error("image quality: {problem:?}")]
    Quality {
        /// The failing check.
        problem: QualityProblem,
        /// The blur score.
        blur_score: f64,
        /// The mean exposure.
        exposure: f64,
    },
    /// `415 unsupported_media_type`.
    #[error("unsupported media")]
    UnsupportedMedia(#[source] DecodeError),
    /// Face detection or blurring failed: the image isn't stored (`503 processing_unavailable`).
    #[error("face blurring failed")]
    FaceBlur(#[source] VisionError),
    /// Resizing or encoding failed (`503 processing_unavailable`).
    #[error("encoding failed")]
    Encode(#[source] VisionError),
}

/// The upload pipeline: the face detector and the settings. `Send + Sync`; build one at start
/// and share it in an `Arc`.
#[derive(Debug)]
pub struct UploadProcessor {
    faces: FaceDetector,
    config: UploadConfig,
}

impl UploadProcessor {
    /// Loads the face detector (hash checked).
    pub fn load(
        runtime: &OrtRuntime,
        manifest: &Manifest,
        config: UploadConfig,
    ) -> Result<Self, VisionError> {
        Ok(Self {
            faces: FaceDetector::load(runtime, manifest, config.faces)?,
            config,
        })
    }

    /// The face detector's model version.
    pub fn face_model_version(&self) -> &str {
        self.faces.version()
    }

    /// A scan upload. Blocking.
    ///
    /// Order: decode → quality gate (every scan kind) → faces on the full-resolution upright
    /// image → blur → resize (2048+ when the depth survives validation) → `WebP` → depth.
    pub fn process(&self, input: UploadInput<'_>) -> Result<ProcessedUpload, UploadError> {
        let decoded = imaging::decode_upload(input.file, self.config.limits)
            .map_err(UploadError::UnsupportedMedia)?;
        let view = ImageView::from(&decoded.image);
        let scores = quality::score(view, &self.config.quality).map_err(UploadError::Encode)?;
        if let Some(problem) = quality::verdict(&scores, &self.config.quality) {
            return Err(UploadError::Quality {
                problem,
                blur_score: scores.blur_score,
                exposure: scores.exposure,
            });
        }
        let faces: Vec<_> = self
            .faces
            .detect(view)
            .map_err(UploadError::FaceBlur)?
            .into_iter()
            .map(|f| f.grown(self.faces.grow()))
            .collect();
        let mut upright = decoded.image;
        blur_faces(&mut upright, &faces);

        let size = upright.dimensions();
        let long = size.0.max(size.1);
        // Validate the depth against the photo first (it needs only sizes), so the stored size
        // knows whether a depth map will be kept.
        let depth_ctx = |stored_size| DepthContext {
            kind: input.kind,
            orientation: decoded.orientation.value(),
            sensor_size: decoded.sensor_size,
            stored_size,
            faces: &faces,
            face_growth: self.config.depth_face_growth,
        };
        let with_depth_size = imaging::fit_long_side(
            size,
            stored_long_side(long, true, self.config.default_long_side),
        );
        let depth = input
            .depth
            .map(|d| process_depth(&d, &depth_ctx(with_depth_size)));
        let keeps_depth = matches!(depth, Some(Ok(_)));
        let target = stored_long_side(long, keeps_depth, self.config.default_long_side);
        let stored = imaging::resize_long_side(ImageView::from(&upright), target)
            .map_err(UploadError::Encode)?;
        let webp = imaging::encode_webp(ImageView::from(&stored), self.config.webp_quality);
        Ok(ProcessedUpload {
            image: ProcessedImage {
                webp: Bytes::from(webp),
                width: stored.width(),
                height: stored.height(),
            },
            quality: QualityReport {
                blur_score: scores.blur_score,
                exposure: scores.exposure,
                passed: BoolLit,
            },
            depth,
        })
    }

    /// A chat image: the same without the quality gate and without depth. Blocking.
    pub fn process_chat(&self, file: &[u8]) -> Result<ProcessedImage, UploadError> {
        let decoded = imaging::decode_upload(file, self.config.limits)
            .map_err(UploadError::UnsupportedMedia)?;
        let faces: Vec<_> = self
            .faces
            .detect(ImageView::from(&decoded.image))
            .map_err(UploadError::FaceBlur)?
            .into_iter()
            .map(|f| f.grown(self.faces.grow()))
            .collect();
        let mut upright = decoded.image;
        blur_faces(&mut upright, &faces);
        let stored =
            imaging::resize_long_side(ImageView::from(&upright), self.config.default_long_side)
                .map_err(UploadError::Encode)?;
        Ok(ProcessedImage {
            webp: Bytes::from(imaging::encode_webp(
                ImageView::from(&stored),
                self.config.webp_quality,
            )),
            width: stored.width(),
            height: stored.height(),
        })
    }
}
