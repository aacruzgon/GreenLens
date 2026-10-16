//! Image preparation ([contracts 6], image preparation): each image resized by the worker so
//! that "the image as sent" is "the image as seen": sides multiples of the adapter's cell size,
//! at most `VLM_MAX_PIXELS` pixels (and, for the hosted model, at most 1568 on the long side),
//! then JPEG.

use std::sync::Arc;

use domain::{id::ImageId, ports::LoadedImage, scan::ImageKind};
use tokio::sync::Semaphore;
use vision::{
    VisionError,
    imaging::{self, ImageView},
};

use super::profile::AdapterProfile;

/// One prepared image: what was sent, and its size, which `abs_px_prepared` boxes divide by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedImage {
    /// The stored image it came from.
    pub image_id: ImageId,
    /// Its kind.
    pub kind: ImageKind,
    /// Prepared width.
    pub width: u32,
    /// Prepared height.
    pub height: u32,
    /// JPEG bytes.
    pub jpeg: Vec<u8>,
}

/// The prepared size: scaled (never up) so the pixel count is at most `max_pixels` and the long
/// side at most `max_long_side`, then each side rounded **down** to a multiple of `cell`, never
/// below one cell. Rounding down keeps the count under the budget, as Qwen's `smart_resize` does
/// when it must shrink.
pub fn target_size(
    size: (u32, u32),
    cell: u32,
    max_pixels: u32,
    max_long_side: Option<u32>,
) -> (u32, u32) {
    let (w, h) = (f64::from(size.0), f64::from(size.1));
    let cell = cell.max(1);
    let mut scale = (f64::from(max_pixels) / (w * h)).sqrt().min(1.0);
    if let Some(long) = max_long_side {
        scale = scale.min(f64::from(long) / w.max(h));
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a positive pixel count far below u32::MAX"
    )]
    let snap = |len: f64| -> u32 {
        let scaled = (len * scale).floor() as u32;
        (scaled / cell * cell).max(cell)
    };
    (snap(w), snap(h))
}

/// Prepares one image. Blocking.
pub fn prepare(
    image: &LoadedImage,
    profile: &AdapterProfile,
) -> Result<PreparedImage, VisionError> {
    let pixels = imaging::decode_stored(&image.bytes)?;
    let (w, h) = target_size(
        pixels.dimensions(),
        profile.cell_px,
        profile.max_pixels,
        profile.max_long_side,
    );
    let resized = imaging::resize(ImageView::from(&pixels), w, h)?;
    Ok(PreparedImage {
        image_id: image.image.image_id,
        kind: image.image.kind,
        width: w,
        height: h,
        jpeg: imaging::encode_jpeg(ImageView::from(&resized), profile.jpeg_quality)?,
    })
}

/// Prepares every image on the blocking pool, behind the CPU semaphore. The closure is
/// `'static + Send`: it owns clones of the images (`Bytes` clones are reference counts) and
/// of the profile.
pub async fn prepare_all(
    images: &[LoadedImage],
    profile: &AdapterProfile,
    cpu: &Arc<Semaphore>,
) -> Result<Vec<PreparedImage>, VisionError> {
    let owned = images.to_vec();
    let profile = profile.clone();
    let permit = Arc::clone(cpu)
        .acquire_owned()
        .await
        .map_err(|_| VisionError::InferenceGone)?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        owned.iter().map(|i| prepare(i, &profile)).collect()
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qwen3_cells_and_the_pixel_budget() {
        // a 4:3 stored image at 2048×1536 → about 1 MP in 32-pixel cells
        let (w, h) = target_size((2048, 1536), 32, 1_003_520, None);
        assert_eq!((w % 32, h % 32), (0, 0));
        assert!(w * h <= 1_003_520);
        assert_eq!((w, h), (1152, 864));
        // small images aren't upscaled, only snapped to the grid
        assert_eq!(target_size((500, 300), 32, 1_003_520, None), (480, 288));
    }

    #[test]
    fn the_hosted_model_gets_at_most_1568_pixels_a_side() {
        let (w, h) = target_size((3000, 1000), 1, 1_150_000, Some(1568));
        assert!(w <= 1568 && w * h <= 1_150_000);
        assert_eq!(
            target_size((1200, 900), 1, 1_150_000, Some(1568)),
            (1200, 900)
        );
    }
}
