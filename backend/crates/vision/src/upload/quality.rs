//! The quality gate ([architecture 6.1]): blur (variance of the Laplacian) and exposure (the
//! luma histogram), on a fixed-size downscale so the scores don't depend on the photo's
//! resolution. Thresholds are set from the evaluation set; the defaults are starting values.
//!
//! [architecture 6.1]: ../../../docs/architecture/README.md#61-stages

use domain::error::QualityProblem;
use image::{DynamicImage, RgbImage};

use crate::{
    VisionError,
    imaging::{self, ImageView},
};

/// Thresholds of the quality gate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QualityConfig {
    /// The analysis downscale's long side, pixels.
    pub analysis_long_side: u32,
    /// Below this blur score the shot is `too_blurry`.
    pub min_blur_score: f64,
    /// Below this mean luma (0–1) the shot is `too_dark`.
    pub min_exposure: f64,
    /// Above this mean luma the shot is `too_bright`.
    pub max_exposure: f64,
    /// More than this share of pixels at luma ≤ 15 is `too_dark`.
    pub max_dark_share: f64,
    /// More than this share of pixels at luma ≥ 240 is `too_bright`.
    pub max_bright_share: f64,
}

impl Default for QualityConfig {
    fn default() -> Self {
        Self {
            analysis_long_side: 512,
            min_blur_score: 40.0,
            min_exposure: 0.12,
            max_exposure: 0.90,
            max_dark_share: 0.60,
            max_bright_share: 0.50,
        }
    }
}

/// The two scores of a shot ([contracts 3], `UploadImageResponse.quality`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QualityScores {
    /// Variance of the 3×3 Laplacian of the luma downscale.
    pub blur_score: f64,
    /// Mean luma, 0–1.
    pub exposure: f64,
    /// Share of pixels at luma ≤ 15.
    pub dark_share: f64,
    /// Share of pixels at luma ≥ 240.
    pub bright_share: f64,
}

/// Scores a shot on its downscale.
pub fn score(image: ImageView<'_>, config: &QualityConfig) -> Result<QualityScores, VisionError> {
    let small: RgbImage = imaging::resize_long_side(image, config.analysis_long_side)?;
    let gray = DynamicImage::ImageRgb8(small).into_luma8();
    let lap = imageproc::filter::laplacian_filter(&gray);
    let values = lap.as_raw();
    let n = values.len().max(1) as f64;
    let mean = values.iter().map(|&v| f64::from(v)).sum::<f64>() / n;
    let blur_score = values
        .iter()
        .map(|&v| (f64::from(v) - mean).powi(2))
        .sum::<f64>()
        / n;
    let hist = imageproc::stats::histogram(&gray).channels[0];
    let total = hist.iter().map(|&c| f64::from(c)).sum::<f64>().max(1.0);
    let exposure = hist
        .iter()
        .enumerate()
        .map(|(i, &c)| i as f64 * f64::from(c))
        .sum::<f64>()
        / total
        / 255.0;
    let dark_share = hist[..=15].iter().map(|&c| f64::from(c)).sum::<f64>() / total;
    let bright_share = hist[240..].iter().map(|&c| f64::from(c)).sum::<f64>() / total;
    Ok(QualityScores {
        blur_score,
        exposure,
        dark_share,
        bright_share,
    })
}

/// The first failing check, in the order blur, dark, bright; `None` when the shot passes.
pub fn verdict(scores: &QualityScores, config: &QualityConfig) -> Option<QualityProblem> {
    if scores.blur_score < config.min_blur_score {
        Some(QualityProblem::TooBlurry)
    } else if scores.exposure < config.min_exposure || scores.dark_share > config.max_dark_share {
        Some(QualityProblem::TooDark)
    } else if scores.exposure > config.max_exposure || scores.bright_share > config.max_bright_share
    {
        Some(QualityProblem::TooBright)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sharp_checkerboard_passes_and_its_blur_fails() {
        let sharp = RgbImage::from_fn(256, 256, |x, y| {
            image::Rgb(if (x / 4 + y / 4) % 2 == 0 {
                [200; 3]
            } else {
                [40; 3]
            })
        });
        let config = QualityConfig::default();
        let s = score(ImageView::from(&sharp), &config).unwrap();
        assert_eq!(verdict(&s, &config), None);
        let blurred = image::imageops::blur(&sharp, 6.0);
        let b = score(ImageView::from(&blurred), &config).unwrap();
        assert!(b.blur_score < s.blur_score / 10.0);
        assert_eq!(verdict(&b, &config), Some(QualityProblem::TooBlurry));
    }

    #[test]
    fn a_black_frame_is_too_dark() {
        let s = QualityScores {
            blur_score: 500.0,
            exposure: 0.02,
            dark_share: 1.0,
            bright_share: 0.0,
        };
        assert_eq!(
            verdict(&s, &QualityConfig::default()),
            Some(QualityProblem::TooDark)
        );
    }
}
