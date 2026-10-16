//! Every constant of the measurement stage, with the contract's values as defaults
//! ([contracts 7], measurement). The values marked "ours" are not in the contract; they are
//! starting values for the evaluation set's `LiDAR` cases.

/// The measurement stage's constants.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasurementConfig {
    /// Masks shrink by this many depth pixels before depth is read inside them. Contract: 2.
    pub shrink_depth_px: u32,
    /// Spots and holes under this many pixels across (equivalent diameter, stored-image pixels)
    /// are counted but not sized. Contract: 4.
    pub min_sized_px: f64,
    /// Upper percentile for height and canopy width edges. Contract: 98.
    pub high_percentile: f64,
    /// Lower percentile for canopy width. Contract: 2.
    pub low_percentile: f64,

    /// Ground candidates: surface normal within this many degrees of gravity. Contract: 15.
    pub ground_normal_max_deg: f64,
    /// A RANSAC plane is a ground candidate with at least this share of the candidate points.
    /// Contract: 5%.
    pub ground_min_share: f64,
    /// The base is the lowest share of the plant's points along gravity. Contract: 5%.
    pub base_share: f64,
    /// The ground may sit this far above the base where leaves touch the soil. Contract: 5 cm.
    pub ground_above_base_m: f64,
    /// Points this close to the ground plane are ground. Contract: 3 cm.
    pub ground_band_m: f64,
    /// Neighbouring depth pixels within this distance join the connected layer. Contract: 5 cm.
    pub layer_step_m: f64,
    /// A `plant` mask whose lowest points are this far above the ground (a pot): height starts
    /// at those points. Contract: 5 cm.
    pub pot_gap_m: f64,
    /// At most this many ground planes are tried (ours).
    pub ground_max_planes: usize,
    /// RANSAC inlier distance for the ground (ours: `LiDAR` noise at 1–3 m).
    pub ground_inlier_m: f64,

    /// RANSAC iterations per plane (ours).
    pub ransac_iterations: usize,
    /// RANSAC inlier distance for a leaf plane (ours: about `LiDAR`'s noise at 30 cm).
    pub leaf_inlier_m: f64,
    /// A leaf needs at least this many confident depth points for a plane (ours).
    pub min_plane_points: usize,
    /// The leaf length search samples at most this many outline points (ours; keeps it O(n²)
    /// bounded).
    pub max_outline_points: usize,

    /// Coverage for high / medium confidence. Contract: 60% / 30%.
    pub coverage_high: f64,
    /// See `coverage_high`.
    pub coverage_medium: f64,
    /// Plane residual (RMS) for high / medium. Contract: 5 mm / 10 mm.
    pub residual_high_m: f64,
    /// See `residual_high_m`.
    pub residual_medium_m: f64,
    /// Valid distance for `whole_plant`. Contract: 0.4–4 m.
    pub whole_plant_range_m: (f64, f64),
    /// Valid distance for the leaf kinds. Contract: 0.25–0.6 m (the lower bound moves with the
    /// day-1 check).
    pub leaf_range_m: (f64, f64),
    /// Up to this share beyond the upper end is medium. Contract: 20%.
    pub beyond_upper_medium: f64,
}

impl Default for MeasurementConfig {
    fn default() -> Self {
        Self {
            shrink_depth_px: 2,
            min_sized_px: 4.0,
            high_percentile: 98.0,
            low_percentile: 2.0,
            ground_normal_max_deg: 15.0,
            ground_min_share: 0.05,
            base_share: 0.05,
            ground_above_base_m: 0.05,
            ground_band_m: 0.03,
            layer_step_m: 0.05,
            pot_gap_m: 0.05,
            ground_max_planes: 5,
            ground_inlier_m: 0.015,
            ransac_iterations: 256,
            leaf_inlier_m: 0.004,
            min_plane_points: 30,
            max_outline_points: 1500,
            coverage_high: 0.60,
            coverage_medium: 0.30,
            residual_high_m: 0.005,
            residual_medium_m: 0.010,
            whole_plant_range_m: (0.4, 4.0),
            leaf_range_m: (0.25, 0.6),
            beyond_upper_medium: 0.20,
        }
    }
}
