//! Image → tensor, from the manifest's input recipe ([vision models 1]): one tested code path
//! for every model, the same numbers the Python evaluation uses.
//!
//! [vision models 1]: ../../../docs/vision/models.md#1-conventions-for-every-model

use ndarray::{Array3, Array4, ArrayView3, Axis};

use crate::{
    VisionError,
    imaging::{self, ImageView, PixelRect},
    manifest::{Colour, ModelInput, ResizeRule},
};

/// One image as a `[3, side, side]` tensor, normalised by the recipe.
pub fn to_tensor(image: ImageView<'_>, input: &ModelInput) -> Result<Array3<f32>, VisionError> {
    let side = input.side().ok_or_else(|| VisionError::Shape {
        model: "preprocess",
        detail: format!("{} has no fixed side", input.name),
    })?;
    let resized = match input.resize {
        ResizeRule::Squash => imaging::resize(image, side, side)?,
        ResizeRule::ShortestSideCrop => {
            let (w, h) = (image.width(), image.height());
            let scale = f64::from(side) / f64::from(w.min(h).max(1));
            let (rw, rh) = (
                ((f64::from(w) * scale).round() as u32).max(side),
                ((f64::from(h) * scale).round() as u32).max(side),
            );
            let r = imaging::resize(image, rw, rh)?;
            imaging::crop(
                ImageView::from(&r),
                PixelRect {
                    x: (rw - side) / 2,
                    y: (rh - side) / 2,
                    w: side,
                    h: side,
                },
            )
        }
        ResizeRule::LongestSidePad => {
            let fitted = imaging::resize_long_side(image, side)?;
            let mut padded = image::RgbImage::new(side, side);
            image::imageops::replace(&mut padded, &fitted, 0, 0);
            padded
        }
    };
    let mean = input.mean.unwrap_or([0.0; 3]);
    let std = input.std.unwrap_or([1.0; 3]);
    let s = side as usize;
    let mut t = Array3::<f32>::zeros((3, s, s));
    for (x, y, p) in resized.enumerate_pixels() {
        for c in 0..3 {
            let channel = match input.colour {
                Colour::Rgb => c,
                Colour::Bgr => 2 - c,
            };
            let v = f32::from(p.0[channel]) / 255.0;
            t[[c, y as usize, x as usize]] = (v - mean[c]) / std[c];
        }
    }
    Ok(t)
}

/// Stacks `[3, h, w]` tensors into contiguous `[n, 3, h, w]` batches of at most `cap`, the
/// layout `TensorRef::from_array_view` needs.
pub fn batches(tensors: &[Array3<f32>], cap: usize) -> Result<Vec<Array4<f32>>, VisionError> {
    tensors
        .chunks(cap.max(1))
        .map(|chunk| {
            let views: Vec<ArrayView3<'_, f32>> = chunk.iter().map(|t| t.view()).collect();
            ndarray::stack(Axis(0), &views).map_err(|e| VisionError::Shape {
                model: "batch",
                detail: e.to_string(),
            })
        })
        .collect()
}

/// The effective cap of a model: its own cap, never above the job-wide `PERCEPTION_MAX_BATCH`.
pub const fn effective_cap(model_cap: usize, job_cap: usize) -> usize {
    let cap = if model_cap < job_cap {
        model_cap
    } else {
        job_cap
    };
    if cap == 0 { 1 } else { cap }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Dim;

    fn recipe(resize: ResizeRule) -> ModelInput {
        ModelInput {
            name: "input".to_owned(),
            shape: vec![
                Dim::Named("batch".to_owned()),
                Dim::Fixed(3),
                Dim::Fixed(8),
                Dim::Fixed(8),
            ],
            colour: Colour::Rgb,
            resize,
            mean: Some([0.5; 3]),
            std: Some([0.5; 3]),
        }
    }

    #[test]
    fn tensors_are_chw_and_normalised() {
        let img = image::RgbImage::from_pixel(16, 12, image::Rgb([255, 0, 128]));
        for rule in [
            ResizeRule::Squash,
            ResizeRule::ShortestSideCrop,
            ResizeRule::LongestSidePad,
        ] {
            let t = to_tensor(ImageView::from(&img), &recipe(rule)).unwrap();
            assert_eq!(t.shape(), &[3, 8, 8]);
            assert!((t[[0, 0, 0]] - 1.0).abs() < 1e-6);
            assert!((t[[1, 0, 0]] + 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn batches_split_at_the_cap() {
        let t = vec![Array3::<f32>::zeros((3, 4, 4)); 17];
        let sizes: Vec<usize> = batches(&t, effective_cap(8, 16))
            .unwrap()
            .iter()
            .map(|b| b.len_of(Axis(0)))
            .collect();
        assert_eq!(sizes, [8, 8, 1]);
        assert_eq!(effective_cap(32, 16), 16);
    }
}
