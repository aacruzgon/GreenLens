//! The confidence criteria of a measurement ([contracts 7], confidence): each applicable
//! criterion gives a band, and the measurement takes the lowest.

use domain::wire::Certainty;

use super::config::MeasurementConfig;

/// Where a whole-plant measurement's points came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlantRegion {
    /// A `plant` mask: high.
    Mask,
    /// The connected depth layer: medium (a pot counts as plant).
    Layer,
}

/// The inputs of the criteria table. `None` marks a criterion that doesn't apply.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Criteria {
    /// Share of the shrunken region's pixels with medium or high depth confidence.
    pub coverage: f64,
    /// Plane residual (RMS), metres: leaf, spot and hole quantities.
    pub residual_m: Option<f64>,
    /// Median distance, metres.
    pub distance_m: f64,
    /// The kind's valid range.
    pub range_m: (f64, f64),
    /// Height and canopy width only.
    pub plant_region: Option<PlantRegion>,
    /// `filtered: true` without a confidence map.
    pub filtered_without_confidence: bool,
}

/// The band: the lowest any applicable criterion reaches.
pub fn band(c: &Criteria, cfg: &MeasurementConfig) -> Certainty {
    let tier = |high: bool, medium: bool| {
        if high {
            Certainty::High
        } else if medium {
            Certainty::Medium
        } else {
            Certainty::Low
        }
    };
    let mut bands = vec![tier(
        c.coverage >= cfg.coverage_high,
        c.coverage >= cfg.coverage_medium,
    )];
    if let Some(r) = c.residual_m {
        bands.push(tier(r <= cfg.residual_high_m, r <= cfg.residual_medium_m));
    }
    let (lo, hi) = c.range_m;
    bands.push(if c.distance_m < lo {
        Certainty::Low
    } else {
        tier(
            c.distance_m <= hi,
            c.distance_m <= hi * (1.0 + cfg.beyond_upper_medium),
        )
    });
    if let Some(region) = c.plant_region {
        bands.push(match region {
            PlantRegion::Mask => Certainty::High,
            PlantRegion::Layer => Certainty::Medium,
        });
    }
    bands.push(if c.filtered_without_confidence {
        Certainty::Medium
    } else {
        Certainty::High
    });
    bands.into_iter().min().unwrap_or(Certainty::Low)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(coverage: f64, residual: f64, distance: f64) -> Criteria {
        Criteria {
            coverage,
            residual_m: Some(residual),
            distance_m: distance,
            range_m: (0.25, 0.6),
            plant_region: None,
            filtered_without_confidence: false,
        }
    }

    #[test]
    fn the_lowest_criterion_wins() {
        let cfg = MeasurementConfig::default();
        assert_eq!(band(&leaf(0.7, 0.004, 0.3), &cfg), Certainty::High);
        assert_eq!(band(&leaf(0.7, 0.008, 0.3), &cfg), Certainty::Medium);
        assert_eq!(band(&leaf(0.2, 0.004, 0.3), &cfg), Certainty::Low);
        assert_eq!(band(&leaf(0.7, 0.004, 0.7), &cfg), Certainty::Medium); // ≤ 20% beyond
        assert_eq!(band(&leaf(0.7, 0.004, 0.2), &cfg), Certainty::Low); // below the range
        let layer = Criteria {
            residual_m: None,
            range_m: (0.4, 4.0),
            plant_region: Some(PlantRegion::Layer),
            ..leaf(0.9, 0.0, 1.0)
        };
        assert_eq!(band(&layer, &cfg), Certainty::Medium);
    }
}
