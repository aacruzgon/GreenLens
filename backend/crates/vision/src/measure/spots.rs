//! The default `SpotSegmenter` ([architecture 6.7], spots): colour thresholds against the leaf's
//! own green. Brown, black or pale pixels are necrotic; yellow ones are halo. A lesion
//! segmentation model can replace it behind the same port.
//!
//! [architecture 6.7]: ../../../docs/architecture/README.md#67-depth-and-measurement-lidar

use domain::ports::{MaskView, RgbView, SpotClass, SpotMask, SpotSegmenter};

/// Thresholds in HSV, relative to the leaf's median value. Starting values, set from the
/// evaluation set's `LiDAR` leaves ([architecture 15], open decision 17).
///
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
}

impl Default for SpotThresholds {
    fn default() -> Self {
        Self {
            dark_value: 0.45,
            brown_hue_min: 10.0,
            brown_hue_max: 45.0,
            brown_saturation: 0.25,
            brown_value: 0.85,
            pale_saturation: 0.15,
            pale_value: 1.25,
            halo_hue_min: 45.0,
            halo_hue_max: 70.0,
            halo_saturation: 0.35,
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
        "colour-1".to_owned()
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
        for i in inside {
            classes[i] = self.classify(pixel(i), leaf_value);
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
