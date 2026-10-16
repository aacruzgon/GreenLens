//! Plant height and canopy width ([contracts 7], geometry: ground plane and plant points).
//!
//! 1. Ground candidates: confident points whose surface normal is within 15° of gravity.
//! 2. RANSAC repeatedly, removing each plane's inliers; a plane with ≥ 5% of the candidates is a
//!    ground candidate.
//! 3. The base: the lowest 5% along gravity of the confident points in the `plant` mask or box,
//!    or the central third of the frame.
//! 4. The ground: the highest candidate at or below the base (5 cm allowance). Points within
//!    3 cm of it are ground.
//! 5. Plant points: the `plant` mask's, or the connected layer from the centre (neighbours within
//!    5 cm, never through ground, inside the box when there is one).
//! 6. Height along gravity from the ground (or from a pot's top) to the 98th percentile; width
//!    from the 2nd to the 98th percentile along the horizontal across the line of sight. Plant
//!    points on the top or bottom edge cancel the height, on a side edge the width.

use std::collections::VecDeque;

use nalgebra::Vector3;

use super::{
    confidence::PlantRegion,
    config::MeasurementConfig,
    plane::{Plane, percentile, ransac},
    scene::DepthScene,
};
use crate::mask::MaskBuf;

/// The plant's region on the depth grid, from segmentation, when there is one.
#[derive(Debug, Clone, PartialEq)]
pub enum PlantHint {
    /// A `plant` mask (resampled to the depth grid): the plant points are its points.
    Mask(MaskBuf),
    /// Only a `plant` box: the base comes from it and the layer stays inside it.
    Box(MaskBuf),
    /// No `plant` region: the central third of the frame.
    None,
}

/// What the whole-plant shot gave.
#[derive(Debug, Clone, PartialEq)]
pub struct PlantGeometry {
    /// Height, metres; `None` without ground beneath the base, or when the plant touches the
    /// top or bottom edge.
    pub height_m: Option<f64>,
    /// Canopy width, metres; `None` when the plant touches a side edge.
    pub width_m: Option<f64>,
    /// Median camera distance of the plant points, metres.
    pub distance_m: f64,
    /// The coverage criterion: confident pixels over region pixels (the shrunken mask or box, or
    /// the central third).
    pub coverage: f64,
    /// Mask or layer.
    pub region: PlantRegion,
    /// The plant points' bounding box on the depth grid, normalised `[x, y, w, h]`.
    pub bbox: [f64; 4],
}

/// A ground candidate plane and its height along "up" (−gravity).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundCandidate {
    /// The plane.
    pub plane: Plane,
    /// The height of its inliers' centroid along −gravity, metres.
    pub height_m: f64,
}

fn up_height(p: &Vector3<f64>, gravity: &Vector3<f64>) -> f64 {
    -gravity.dot(p)
}

/// The surface normal at a depth pixel from its right and lower neighbours; `None` at the
/// border or next to a pixel without confident depth.
#[allow(
    clippy::many_single_char_names,
    reason = "x, y, w, h, p, n: pixel and vector notation"
)]
fn normal_at(scene: &DepthScene<'_>, x: u32, y: u32) -> Option<Vector3<f64>> {
    let (w, h) = scene.size();
    if x + 1 >= w || y + 1 >= h {
        return None;
    }
    let p = scene.point(x, y)?;
    let right = scene.point(x + 1, y)?;
    let down = scene.point(x, y + 1)?;
    let n = (right - p).cross(&(down - p));
    let len = n.norm();
    (len > 1e-12).then(|| n / len)
}

/// Steps 1–2: every ground candidate, highest first.
pub fn ground_candidates(
    scene: &DepthScene<'_>,
    gravity: &Vector3<f64>,
    cfg: &MeasurementConfig,
    seed: u64,
) -> Vec<GroundCandidate> {
    let cos_max = cfg.ground_normal_max_deg.to_radians().cos();
    let (w, h) = scene.size();
    let mut pool: Vec<Vector3<f64>> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if let (Some(n), Some(p)) = (normal_at(scene, x, y), scene.point(x, y))
                && n.dot(gravity).abs() >= cos_max
            {
                pool.push(p);
            }
        }
    }
    let min_inliers = ((pool.len() as f64) * cfg.ground_min_share).ceil() as usize;
    let mut out = Vec::new();
    for round in 0..cfg.ground_max_planes {
        if pool.len() < min_inliers.max(3) {
            break;
        }
        let Some(fit) = ransac(
            &pool,
            cfg.ground_inlier_m,
            cfg.ransac_iterations,
            seed.wrapping_add(round as u64),
        ) else {
            break;
        };
        if fit.inliers.len() < min_inliers.max(3) {
            break;
        }
        let centroid =
            fit.inliers.iter().map(|&i| pool[i]).sum::<Vector3<f64>>() / fit.inliers.len() as f64;
        out.push(GroundCandidate {
            plane: fit.plane,
            height_m: up_height(&centroid, gravity),
        });
        let inliers: std::collections::BTreeSet<usize> = fit.inliers.into_iter().collect();
        pool = pool
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !inliers.contains(i))
            .map(|(_, p)| p)
            .collect();
    }
    out.sort_by(|a, b| b.height_m.total_cmp(&a.height_m));
    out
}

/// Step 4: the highest candidate at or below the base, with the allowance.
pub fn choose_ground(
    candidates: &[GroundCandidate],
    base_height_m: f64,
    cfg: &MeasurementConfig,
) -> Option<GroundCandidate> {
    candidates
        .iter()
        .find(|c| c.height_m <= base_height_m + cfg.ground_above_base_m)
        .copied()
}

fn central_third(size: (u32, u32)) -> MaskBuf {
    let (w, h) = size;
    MaskBuf::from_rect(
        w,
        h,
        crate::imaging::PixelRect {
            x: w / 3,
            y: h / 3,
            w: w / 3,
            h: h / 3,
        },
    )
}

/// Step 5 without a mask: the connected layer from the confident non-ground pixel nearest the
/// frame's centre (inside the box when given), grown through 4-neighbours whose points lie
/// within `layer_step_m` of each other.
pub fn connected_layer(
    scene: &DepthScene<'_>,
    is_ground: &dyn Fn(&Vector3<f64>) -> bool,
    inside: Option<&MaskBuf>,
    cfg: &MeasurementConfig,
) -> Vec<(u32, u32)> {
    let (w, h) = scene.size();
    let allowed = |x: u32, y: u32| -> Option<Vector3<f64>> {
        if inside.is_some_and(|m| !m.get(x, y)) {
            return None;
        }
        scene.point(x, y).filter(|p| !is_ground(p))
    };
    let (cx, cy) = (f64::from(w) / 2.0, f64::from(h) / 2.0);
    let seed = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| allowed(x, y).is_some())
        .min_by(|a, b| {
            let d = |p: &(u32, u32)| (f64::from(p.0) - cx).powi(2) + (f64::from(p.1) - cy).powi(2);
            d(a).total_cmp(&d(b))
        });
    let Some(seed) = seed else {
        return Vec::new();
    };
    let mut visited = vec![false; w as usize * h as usize];
    let idx = |x: u32, y: u32| y as usize * w as usize + x as usize;
    visited[idx(seed.0, seed.1)] = true;
    let mut queue = VecDeque::from([seed]);
    let mut out = Vec::new();
    while let Some((x, y)) = queue.pop_front() {
        out.push((x, y));
        let Some(p) = allowed(x, y) else { continue };
        for (nx, ny) in [
            (x.wrapping_sub(1), y),
            (x + 1, y),
            (x, y.wrapping_sub(1)),
            (x, y + 1),
        ] {
            if nx >= w || ny >= h || visited[idx(nx, ny)] {
                continue;
            }
            if let Some(q) = allowed(nx, ny)
                && (q - p).norm() <= cfg.layer_step_m
            {
                visited[idx(nx, ny)] = true;
                queue.push_back((nx, ny));
            }
        }
    }
    out
}

/// Steps 1–6. `gravity` is the stored meta's unit vector towards the ground.
pub fn measure_plant(
    scene: &DepthScene<'_>,
    gravity: Vector3<f64>,
    hint: &PlantHint,
    cfg: &MeasurementConfig,
    seed: u64,
) -> Option<PlantGeometry> {
    let size = scene.size();
    let base_region = match hint {
        PlantHint::Mask(m) | PlantHint::Box(m) => m.clone(),
        PlantHint::None => central_third(size),
    };
    let shrunk = crate::mask::erode(base_region.view(), cfg.shrink_depth_px);
    let region = scene.points_in(&shrunk);
    let heights: Vec<f64> = region
        .points
        .iter()
        .map(|p| up_height(p, &gravity))
        .collect();
    let base = percentile(&heights, cfg.base_share * 100.0)?;
    let ground = choose_ground(&ground_candidates(scene, &gravity, cfg, seed), base, cfg);
    let is_ground =
        |p: &Vector3<f64>| ground.is_some_and(|g| g.plane.distance(p) <= cfg.ground_band_m);

    let (pixels, region_kind) = match hint {
        PlantHint::Mask(m) => {
            let px: Vec<(u32, u32)> = region
                .pixels
                .iter()
                .copied()
                .filter(|&(x, y)| m.get(x, y) && scene.point(x, y).is_some_and(|p| !is_ground(&p)))
                .collect();
            (px, PlantRegion::Mask)
        }
        PlantHint::Box(b) => (
            connected_layer(scene, &is_ground, Some(b), cfg),
            PlantRegion::Layer,
        ),
        PlantHint::None => (
            connected_layer(scene, &is_ground, None, cfg),
            PlantRegion::Layer,
        ),
    };
    let points: Vec<Vector3<f64>> = pixels
        .iter()
        .filter_map(|&(x, y)| scene.point(x, y))
        .collect();
    if points.is_empty() {
        return None;
    }
    let (w, h) = size;
    let touches_vertical = pixels.iter().any(|&(_, y)| y == 0 || y + 1 == h);
    let touches_side = pixels.iter().any(|&(x, _)| x == 0 || x + 1 == w);

    let up: Vec<f64> = points.iter().map(|p| up_height(p, &gravity)).collect();
    let top = percentile(&up, cfg.high_percentile)?;
    let height_m = ground.filter(|_| !touches_vertical).map(|g| {
        let mut floor = g.height_m;
        if region_kind == PlantRegion::Mask
            && let Some(lowest) = percentile(&up, cfg.low_percentile)
            && lowest - g.height_m > cfg.pot_gap_m
        {
            floor = lowest; // a pot or planter: start at the plant's own lowest points
        }
        top - floor
    });

    let centroid = points.iter().sum::<Vector3<f64>>() / points.len() as f64;
    let across = gravity.cross(&centroid.normalize());
    let width_m = (!touches_side && across.norm() > 1e-9).then(|| {
        let axis = across.normalize();
        let along: Vec<f64> = points.iter().map(|p| axis.dot(p)).collect();
        match (
            percentile(&along, cfg.high_percentile),
            percentile(&along, cfg.low_percentile),
        ) {
            (Some(hi), Some(lo)) => hi - lo,
            _ => 0.0,
        }
    });
    let distances: Vec<f64> = points.iter().map(Vector3::norm).collect();
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for &(x, y) in &pixels {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x + 1);
        y1 = y1.max(y + 1);
    }
    Some(PlantGeometry {
        height_m,
        width_m,
        distance_m: super::plane::median(&distances)?,
        coverage: region.coverage(),
        region: region_kind,
        bbox: [
            f64::from(x0) / f64::from(w),
            f64::from(y0) / f64::from(h),
            f64::from(x1 - x0) / f64::from(w),
            f64::from(y1 - y0) / f64::from(h),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(h: f64) -> GroundCandidate {
        GroundCandidate {
            plane: Plane {
                n: Vector3::new(0.0, -1.0, 0.0),
                d: 0.0,
            },
            height_m: h,
        }
    }

    #[test]
    fn the_raised_bed_wins_over_the_path_and_a_bench_above_loses() {
        let cfg = MeasurementConfig::default();
        // highest first: a bench at 0.9 m, the bed's soil at 0.3 m, the path at 0.0 m
        let candidates = [cand(0.9), cand(0.3), cand(0.0)];
        let ground = choose_ground(&candidates, 0.32, &cfg).unwrap();
        assert!((ground.height_m - 0.3).abs() < 1e-12);
        // a base below every candidate but the path
        assert!((choose_ground(&candidates, 0.1, &cfg).unwrap().height_m).abs() < 1e-12);
        // nothing at or below the base: no height
        assert!(choose_ground(&[cand(0.9)], 0.1, &cfg).is_none());
    }
}
