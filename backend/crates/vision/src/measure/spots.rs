//! The default `SpotSegmenter` ([architecture 6.7], spots): colour thresholds against the leaf's
//! own green. Brown, black or pale pixels are necrotic; yellow ones are halo. A lesion
//! segmentation model can replace it behind the same port.
//!
//! [architecture 6.7]: ../../../docs/architecture/README.md#67-depth-and-measurement-lidar

use domain::ports::{MaskView, RgbView, SpotClass, SpotMask, SpotSegmenter};

use crate::mask;

/// Thresholds in HSV, relative to the leaf's median value. The defaults were fitted on 2026-10-22
/// (M15) on `PlantSeg`'s lesion masks inside the leaf masks our detector and SAM cut: on its
/// hold-out spot diseases the mean of the lesion and healthy-leaf `IoU`s rose from 0.382 to 0.497
/// ([report]). In short: brown to red hues (0–45°) and greys are necrotic, as is anything darker
/// than a fifth of the leaf; yellow (45–66°) is halo; the rest is leaf ([architecture 15], open
/// decision 17). A necrotic patch under `min_spot_share` of the leaf is leaf too.
///
/// [report]: ../../../ml/training/eval/reports/spots-2026-10-22.md
/// [architecture 15]: ../../../docs/architecture/README.md#15-open-decisions
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpotThresholds {
    /// Darker than this share of the leaf's median value: black or dark brown.
    pub dark_value: f64,
    /// Brown: hue from this many degrees …
    pub brown_hue_min: f64,
    /// … to this many …
    pub brown_hue_max: f64,
    /// … with at least this saturation, and darker than `brown_value` × the leaf's median.
    pub brown_saturation: f64,
    /// See `brown_saturation`.
    pub brown_value: f64,
    /// Pale: saturation under this, and brighter than `pale_value` × the leaf's median.
    pub pale_saturation: f64,
    /// See `pale_saturation`.
    pub pale_value: f64,
    /// Yellow halo: hue from this many degrees …
    pub halo_hue_min: f64,
    /// … to this many, with at least `halo_saturation`.
    pub halo_hue_max: f64,
    /// See `halo_hue_max`.
    pub halo_saturation: f64,
    /// A necrotic patch (4-connected) smaller than this share of the leaf's pixels is leaf:
    /// a speck of glare, grain or edge noise, not a lesion. A share rather than a pixel count,
    /// so it holds at any resolution and distance. 0 keeps every patch.
    pub min_spot_share: f64,
}

/// The default `min_spot_share`, fitted on 2026-10-22 ([report], specks): the largest share at
/// which `PlantSeg`'s diseased fit photos read clean at most a point more often than with no
/// filter (2.5% against 1.7%). It turns the healthy check photos with no spot from 13% to 22%.
///
/// [report]: ../../../ml/training/eval/reports/spots-2026-10-22.md
pub const MIN_SPOT_SHARE: f64 = 2e-5;

impl Default for SpotThresholds {
    fn default() -> Self {
        Self {
            dark_value: 0.2,
            brown_hue_min: 0.0,
            brown_hue_max: 45.0,
            brown_saturation: 0.0,
            brown_value: 2.5,
            pale_saturation: 0.08,
            pale_value: 0.0,
            halo_hue_min: 45.0,
            halo_hue_max: 66.0,
            halo_saturation: 0.0,
            min_spot_share: MIN_SPOT_SHARE,
        }
    }
}

/// The colour-threshold segmenter.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ColourSpotSegmenter {
    /// Its thresholds.
    pub thresholds: SpotThresholds,
}

/// RGB8 to hue (degrees), saturation and value (0–1).
#[allow(
    clippy::float_cmp,
    reason = "`max` is one of r, g, b exactly: it picks which channel is largest"
)]
pub fn hsv(rgb: [u8; 3]) -> (f64, f64, f64) {
    let [r, g, b] = rgb.map(|c| f64::from(c) / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    (hue, saturation, max)
}

impl ColourSpotSegmenter {
    /// Classifies one pixel against the leaf's median value.
    pub fn classify(&self, rgb: [u8; 3], leaf_value: f64) -> SpotClass {
        let t = &self.thresholds;
        let (h, s, v) = hsv(rgb);
        let dark = v < leaf_value * t.dark_value;
        let brown = (t.brown_hue_min..t.brown_hue_max).contains(&h)
            && s >= t.brown_saturation
            && v < leaf_value * t.brown_value;
        let pale = s < t.pale_saturation && v > leaf_value * t.pale_value;
        if dark || brown || pale {
            SpotClass::Necrotic
        } else if (t.halo_hue_min..t.halo_hue_max).contains(&h) && s >= t.halo_saturation {
            SpotClass::Halo
        } else {
            SpotClass::Leaf
        }
    }
}

impl SpotSegmenter for ColourSpotSegmenter {
    fn version(&self) -> String {
        "colour-3".to_owned()
    }

    fn segment<'a>(&self, image: RgbView<'a>, leaf: MaskView<'a>) -> SpotMask {
        let (w, h) = (leaf.width(), leaf.height());
        let pixel = |i: usize| {
            let p = image.pixels();
            [p[i * 3], p[i * 3 + 1], p[i * 3 + 2]]
        };
        let inside: Vec<usize> = (0..w as usize * h as usize)
            .filter(|&i| leaf.data()[i] != 0)
            .collect();
        let mut values: Vec<f64> = inside.iter().map(|&i| hsv(pixel(i)).2).collect();
        values.sort_by(f64::total_cmp);
        let leaf_value = values.get(values.len() / 2).copied().unwrap_or(0.5);
        let mut classes = vec![SpotClass::Outside; w as usize * h as usize];
        for &i in &inside {
            classes[i] = self.classify(pixel(i), leaf_value);
        }
        #[allow(
            clippy::cast_precision_loss,
            reason = "pixel counts are far below 2^52"
        )]
        let min_area = self.thresholds.min_spot_share * inside.len() as f64;
        if min_area > 0.0 {
            let at = |x: u32, y: u32| y as usize * w as usize + x as usize;
            let patches = mask::components(w, h, |x, y| classes[at(x, y)] == SpotClass::Necrotic);
            for patch in patches {
                #[allow(
                    clippy::cast_precision_loss,
                    reason = "pixel counts are far below 2^52"
                )]
                let small = (patch.area() as f64) < min_area;
                if small {
                    for (x, y) in patch.pixels {
                        classes[at(x, y)] = SpotClass::Leaf;
                    }
                }
            }
        }
        SpotMask {
            width: w,
            height: h,
            classes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brown_black_and_yellow_against_green() {
        let s = ColourSpotSegmenter::default();
        let leaf_value = hsv([60, 140, 50]).2;
        assert_eq!(s.classify([60, 140, 50], leaf_value), SpotClass::Leaf);
        assert_eq!(s.classify([110, 70, 30], leaf_value), SpotClass::Necrotic); // brown
        assert_eq!(s.classify([15, 15, 10], leaf_value), SpotClass::Necrotic); // black
        assert_eq!(s.classify([200, 190, 40], leaf_value), SpotClass::Halo); // yellow
    }

    #[test]
    fn patches_under_the_share_of_the_leaf_are_leaf() {
        // A 10 x 10 leaf: one brown pixel at (1, 1) and a 3 x 3 brown patch at (5..8, 5..8).
        let (green, brown) = ([60u8, 140, 50], [110u8, 70, 30]);
        let mut px = Vec::new();
        for y in 0..10 {
            for x in 0..10 {
                let spot = (x, y) == (1, 1) || ((5..8).contains(&x) && (5..8).contains(&y));
                px.extend(if spot { brown } else { green });
            }
        }
        let mask = [255u8; 100];
        let necrotic = |share: f64| {
            let s = ColourSpotSegmenter {
                thresholds: SpotThresholds {
                    min_spot_share: share,
                    ..SpotThresholds::default()
                },
            };
            let out = s.segment(
                RgbView::new(10, 10, &px).unwrap(),
                MaskView::new(10, 10, &mask).unwrap(),
            );
            (0..100)
                .filter(|&i| out.classes[i] == SpotClass::Necrotic)
                .collect::<Vec<_>>()
        };
        assert_eq!(necrotic(0.0).len(), 10);
        // 5% of 100 pixels: the single pixel goes, the 9-pixel patch stays.
        assert_eq!(necrotic(0.05), [55, 56, 57, 65, 66, 67, 75, 76, 77]);
    }

    #[test]
    fn only_pixels_inside_the_leaf_are_classified() {
        let px = [60u8, 140, 50, 110, 70, 30, 60, 140, 50, 60, 140, 50];
        let mask = [255u8, 255, 0, 255];
        let out = ColourSpotSegmenter::default().segment(
            RgbView::new(2, 2, &px).unwrap(),
            MaskView::new(2, 2, &mask).unwrap(),
        );
        assert_eq!(
            out.classes,
            [
                SpotClass::Leaf,
                SpotClass::Necrotic,
                SpotClass::Outside,
                SpotClass::Leaf
            ]
        );
    }
}
