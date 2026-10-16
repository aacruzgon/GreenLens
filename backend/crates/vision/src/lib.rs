//! PDM's computer-vision crate: ONNX Runtime sessions and the perception models of the
//! perception worker, the CPU upload pipeline of `api` (decode, quality, face blurring, resize,
//! `WebP`, depth), the `LiDAR` measurement stage, the text embedder and COCO RLE masks.
//!
//! Nothing here does I/O beyond reading model files at start-up: callers hand in bytes and get
//! bytes back. CPU-heavy functions are synchronous and borrow their buffers; the caller runs them
//! on a blocking thread (`tokio::task::spawn_blocking`). The async entry points (the
//! `VisionModel`, `TextEmbedder` and `SpotSegmenter` ports of `domain`) move owned data onto a
//! blocking thread themselves.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "pixel and depth arithmetic converts between u32 sizes, f32 tensors and f64 geometry \
              on values far inside every type's range; each conversion site is a size or a pixel"
)]

pub mod depth;
pub mod embed;
pub mod error;
pub mod imaging;
pub mod manifest;
pub mod mask;
pub mod measure;
pub mod perception;
pub mod rle;
pub mod runtime;
pub mod upload;

pub use error::VisionError;
