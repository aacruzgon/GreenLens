//! RF-DETR-S leaf detector post-processing ([vision models 3]): sigmoid on the `leaf` logit of
//! each of the 300 queries, threshold, a final class-agnostic `NMS` at `IoU` 0.8, boxes in the
//! stored image's normalised `[x, y, w, h]`, small boxes dropped, at most 40 per image.
//!
//! [vision models 3]: ../../../docs/vision/models.md#3-leaf-detector

use domain::wire::BBox;

/// The detector's thresholds, from its manifest `[model.config]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectorConfig {
    /// `τ_det`.
    pub score_threshold: f32,
    /// Boxes under this many pixels on their shorter side (stored image) are dropped.
    pub min_side_px: f32,
    /// At most this many leaves per image.
    pub max_per_image: usize,
    /// The final `NMS`.
    pub nms_iou: f32,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self {
            score_threshold: 0.35,
            min_side_px: 24.0,
            max_per_image: 40,
            nms_iou: 0.8,
        }
    }
}

/// One leaf box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
    /// Normalised to the stored image.
    pub bbox: BBox,
    /// The sigmoid score.
    pub score: f32,
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn iou(a: &BBox, b: &BBox) -> f64 {
    let [ax, ay, aw, ah] = a.to_array();
    let [bx, by, bw, bh] = b.to_array();
    let ix = ((ax + aw).min(bx + bw) - ax.max(bx)).max(0.0);
    let iy = ((ay + ah).min(by + bh) - ay.max(by)).max(0.0);
    let inter = ix * iy;
    let union = aw * ah + bw * bh - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

/// Decodes one image's queries. `dets` is `[queries, 4]` (normalised centre x, centre y, width,
/// height; the squashed resize makes them the stored image's), `logits` is `[queries, classes]`
/// with `leaf` at index 0. `image_size` is the stored image's, for the size rule.
pub fn decode(
    dets: &[f32],
    logits: &[f32],
    classes: usize,
    image_size: (u32, u32),
    cfg: &DetectorConfig,
) -> Vec<Detection> {
    let queries = dets.len() / 4;
    let (iw, ih) = (image_size.0 as f32, image_size.1 as f32);
    let mut found: Vec<Detection> = (0..queries)
        .filter_map(|q| {
            let score = sigmoid(logits[q * classes]);
            if score < cfg.score_threshold {
                return None;
            }
            let [cx, cy, w, h] = [
                dets[q * 4],
                dets[q * 4 + 1],
                dets[q * 4 + 2],
                dets[q * 4 + 3],
            ];
            if (w * iw).min(h * ih) < cfg.min_side_px {
                return None;
            }
            let x0 = (cx - w / 2.0).clamp(0.0, 1.0);
            let y0 = (cy - h / 2.0).clamp(0.0, 1.0);
            let x1 = (cx + w / 2.0).clamp(0.0, 1.0);
            let y1 = (cy + h / 2.0).clamp(0.0, 1.0);
            BBox::new(
                f64::from(x0),
                f64::from(y0),
                f64::from(x1 - x0),
                f64::from(y1 - y0),
            )
            .ok()
            .map(|bbox| Detection { bbox, score })
        })
        .collect();
    found.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Detection> = Vec::new();
    for d in found {
        if kept.len() == cfg.max_per_image {
            break;
        }
        if kept
            .iter()
            .all(|k| iou(&k.bbox, &d.bbox) <= f64::from(cfg.nms_iou))
        {
            kept.push(d);
        }
    }
    kept
}

/// The image's top leaf score, for the plant check (0 without detections).
pub fn top_score(detections: &[Detection]) -> f32 {
    detections.iter().map(|d| d.score).fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_confident_large_boxes_and_drops_duplicates() {
        // three queries: a leaf, its duplicate, a tiny box; one class plus background
        let dets = [
            0.5, 0.5, 0.4, 0.4, 0.5, 0.5, 0.41, 0.4, 0.1, 0.1, 0.005, 0.005,
        ];
        let logits = [3.0, -3.0, 2.0, -3.0, 4.0, -3.0];
        let found = decode(&dets, &logits, 2, (1000, 1000), &DetectorConfig::default());
        assert_eq!(found.len(), 1);
        assert!((found[0].score - sigmoid(3.0)).abs() < 1e-6);
        let [x, _, w, _] = found[0].bbox.to_array();
        assert!((x - 0.3).abs() < 1e-6 && (w - 0.4).abs() < 1e-6);
    }
}
