//! SAM 2.1 box prompts and mask decoding ([vision models 4]): every image is squashed to
//! 1024 × 1024, boxes are corners in that frame, one prompt per box in one decoder call of up to
//! 16 boxes, and the 256 × 256 logits are resized bilinearly to the stored image and thresholded
//! at 0.
//!
//! [vision models 4]: ../../../docs/vision/models.md#4-mask-model

use domain::wire::BBox;
use ndarray::{Array2, ArrayView2};

use crate::mask::MaskBuf;

/// The encoder's square input side.
pub const FRAME: f32 = 1024.0;

/// SAM's settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SamConfig {
    /// `τ_mask`: a mask whose predicted `IoU` is lower is dropped and the box kept.
    pub min_iou: f32,
    /// A `plant` region from the leaves is tried on a whole-plant shot with at least this many
    /// leaf boxes ([vision models 5]).
    ///
    /// [vision models 5]: ../../../docs/vision/models.md#5-plant-soil-and-surroundings-regions
    pub plant_min_leaves: usize,
    /// The enclosing box grows by this share.
    pub plant_grow: f64,
    /// The plant mask is kept only when it covers at least this share of the leaf boxes' area.
    pub plant_min_cover: f64,
}

impl Default for SamConfig {
    fn default() -> Self {
        Self {
            min_iou: 0.6,
            plant_min_leaves: 3,
            plant_grow: 0.05,
            plant_min_cover: 0.6,
        }
    }
}

/// A normalised box as corners in the 1024 frame: the squash scales x and y independently.
pub fn prompt(bbox: BBox) -> [f32; 4] {
    let [x, y, w, h] = bbox.to_array();
    [
        (x * f64::from(FRAME)) as f32,
        (y * f64::from(FRAME)) as f32,
        ((x + w) * f64::from(FRAME)) as f32,
        ((y + h) * f64::from(FRAME)) as f32,
    ]
}

/// The decoder's `boxes` input for a chunk of prompts: `[n, 4]`.
pub fn boxes_input(boxes: &[BBox]) -> Array2<f32> {
    let mut a = Array2::<f32>::zeros((boxes.len(), 4));
    for (i, b) in boxes.iter().enumerate() {
        for (j, v) in prompt(*b).into_iter().enumerate() {
            a[[i, j]] = v;
        }
    }
    a
}

/// Resizes one 256 × 256 logit map bilinearly to the stored image and thresholds it at 0. The
/// squash makes the map cover the whole image, so the resize is direct.
pub fn upsample(logits: &ArrayView2<'_, f32>, size: (u32, u32)) -> MaskBuf {
    let (lh, lw) = logits.dim();
    let mut mask = MaskBuf::empty(size.0, size.1);
    let (sx, sy) = (lw as f32 / size.0 as f32, lh as f32 / size.1 as f32);
    for y in 0..size.1 {
        let fy = ((y as f32 + 0.5) * sy - 0.5).clamp(0.0, (lh - 1) as f32);
        let (y0, ty) = (fy.floor() as usize, fy - fy.floor());
        let y1 = (y0 + 1).min(lh - 1);
        for x in 0..size.0 {
            let fx = ((x as f32 + 0.5) * sx - 0.5).clamp(0.0, (lw - 1) as f32);
            let (x0, tx) = (fx.floor() as usize, fx - fx.floor());
            let x1 = (x0 + 1).min(lw - 1);
            let top = logits[[y0, x0]] * (1.0 - tx) + logits[[y0, x1]] * tx;
            let bottom = logits[[y1, x0]] * (1.0 - tx) + logits[[y1, x1]] * tx;
            if top * (1.0 - ty) + bottom * ty > 0.0 {
                mask.set(x, y, true);
            }
        }
    }
    mask
}

/// The box around every leaf box, grown, for the `plant` prompt.
pub fn enclosing(leaves: &[BBox], grow: f64) -> Option<BBox> {
    let (mut x0, mut y0, mut x1, mut y1) = (1.0f64, 1.0f64, 0.0f64, 0.0f64);
    for b in leaves {
        let [x, y, w, h] = b.to_array();
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x + w);
        y1 = y1.max(y + h);
    }
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (dx, dy) = ((x1 - x0) * grow, (y1 - y0) * grow);
    let (x0, y0) = ((x0 - dx).max(0.0), (y0 - dy).max(0.0));
    let (x1, y1) = ((x1 + dx).min(1.0), (y1 + dy).min(1.0));
    BBox::new(x0, y0, x1 - x0, y1 - y0).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_are_corners_in_the_squashed_frame() {
        let b = BBox::new(0.25, 0.5, 0.5, 0.25).unwrap();
        assert_eq!(prompt(b), [256.0, 512.0, 768.0, 768.0]);
    }

    #[test]
    fn upsampling_keeps_the_positive_half() {
        let mut logits = Array2::<f32>::from_elem((4, 4), -1.0);
        for y in 0..4 {
            logits[[y, 0]] = 1.0;
            logits[[y, 1]] = 1.0;
        }
        let m = upsample(&logits.view(), (8, 8));
        assert!(m.get(0, 0) && m.get(3, 7) && !m.get(5, 0));
    }

    #[test]
    fn the_plant_box_encloses_the_leaves() {
        let leaves = [
            BBox::new(0.2, 0.2, 0.1, 0.1).unwrap(),
            BBox::new(0.5, 0.6, 0.1, 0.1).unwrap(),
        ];
        let e = enclosing(&leaves, 0.0).unwrap().to_array();
        assert!((e[0] - 0.2).abs() < 1e-12 && (e[2] - 0.4).abs() < 1e-12);
    }
}
