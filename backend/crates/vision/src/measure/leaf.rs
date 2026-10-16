//! Leaves, spots and holes ([contracts 7], geometry): a RANSAC plane through the confident
//! depth inside the shrunken mask, then every photo pixel of the mask projected onto it.

use domain::ports::{SpotClass, SpotSegmenter};
use nalgebra::Vector3;

use super::{
    camera::Camera,
    config::MeasurementConfig,
    plane::{Plane, ransac},
    scene::DepthScene,
};
use crate::{
    imaging::ImageView,
    mask::{self, Component, MaskBuf},
};

/// One sized item (a spot or a hole): its projected area and its size in stored-image pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizedItem {
    /// Projected area, m².
    pub area_m2: f64,
    /// Equivalent diameter in stored-image pixels (2·√(pixels/π)).
    pub diameter_px: f64,
}

impl SizedItem {
    /// The diameter of the circle with the same projected area, metres.
    pub fn diameter_m(&self) -> f64 {
        2.0 * (self.area_m2 / std::f64::consts::PI).sqrt()
    }
}

/// Everything measured on one leaf.
#[derive(Debug, Clone, PartialEq)]
pub struct LeafGeometry {
    /// The fitted plane.
    pub plane: Plane,
    /// Its RMS residual, metres.
    pub rms_m: f64,
    /// Confident pixels over shrunken-mask pixels on the depth grid.
    pub coverage: f64,
    /// Median camera distance of the plane's points, metres.
    pub distance_m: f64,
    /// Longest distance across the projected outline, metres.
    pub length_m: f64,
    /// Projected area, m².
    pub area_m2: f64,
    /// Every spot (necrotic component).
    pub spots: Vec<SizedItem>,
    /// Necrotic and halo area over leaf area, percent.
    pub affected_pct: f64,
    /// Every hole.
    pub holes: Vec<SizedItem>,
}

/// The area on `plane` of the stored-image pixel `(u, v)`: with the ray `dir = K⁻¹(u, v, 1)`
/// meeting the plane at `t·dir`, the footprint is `t² / (fx · fy · |n · dir|)`.
pub fn pixel_area(camera: &Camera, plane: &Plane, u: u32, v: u32) -> Option<(f64, Vector3<f64>)> {
    let dir = camera.ray(f64::from(u), f64::from(v));
    let t = plane.intersect(&dir)?;
    let k = camera.intrinsics();
    let area = t * t / (k.fx * k.fy * plane.n.dot(&dir).abs());
    Some((area, dir * t))
}

fn projected_area(camera: &Camera, plane: &Plane, pixels: &[(u32, u32)]) -> f64 {
    pixels
        .iter()
        .filter_map(|&(u, v)| pixel_area(camera, plane, u, v).map(|(a, _)| a))
        .sum()
}

fn sized(camera: &Camera, plane: &Plane, c: &Component) -> SizedItem {
    SizedItem {
        area_m2: projected_area(camera, plane, &c.pixels),
        diameter_px: 2.0 * (c.area() as f64 / std::f64::consts::PI).sqrt(),
    }
}

/// The longest distance between two outline points projected onto the plane. The outline is
/// sampled to at most `max_points` (every k-th point) so the pairwise search stays bounded.
pub fn longest_extent(points: &[Vector3<f64>], max_points: usize) -> f64 {
    let step = points.len().div_ceil(max_points.max(1)).max(1);
    let sample: Vec<_> = points.iter().step_by(step).collect();
    let mut best = 0.0f64;
    for (i, a) in sample.iter().enumerate() {
        for b in &sample[i + 1..] {
            best = best.max((*a - *b).norm());
        }
    }
    best
}

/// Measures one leaf. `mask` is on the stored image's grid; `None` when the leaf has too few
/// confident depth points for a plane.
pub fn measure_leaf(
    scene: &DepthScene<'_>,
    image_camera: &Camera,
    image: ImageView<'_>,
    leaf: &MaskBuf,
    spots: &dyn SpotSegmenter,
    cfg: &MeasurementConfig,
    seed: u64,
) -> Option<LeafGeometry> {
    let (dw, dh) = scene.size();
    let on_depth = mask::resample(leaf.view(), dw, dh);
    let shrunk = mask::erode(on_depth.view(), cfg.shrink_depth_px);
    let region = scene.points_in(&shrunk);
    if region.points.len() < cfg.min_plane_points {
        return None;
    }
    let fit = ransac(
        &region.points,
        cfg.leaf_inlier_m,
        cfg.ransac_iterations,
        seed,
    )?;
    let distance_m = region.median_distance()?;
    let plane = fit.plane;

    let leaf_pixels: Vec<(u32, u32)> = (0..leaf.height())
        .flat_map(|y| (0..leaf.width()).map(move |x| (x, y)))
        .filter(|&(x, y)| leaf.get(x, y))
        .collect();
    let area_m2 = projected_area(image_camera, &plane, &leaf_pixels);
    let outline: Vec<Vector3<f64>> = mask::outline(leaf.view())
        .into_iter()
        .filter_map(|(u, v)| pixel_area(image_camera, &plane, u, v).map(|(_, p)| p))
        .collect();
    let length_m = longest_extent(&outline, cfg.max_outline_points);

    let classes = spots.segment(image.as_domain(), leaf.view());
    let class = |x: u32, y: u32| classes.classes[y as usize * classes.width as usize + x as usize];
    let necrotic = mask::components(leaf.width(), leaf.height(), |x, y| {
        class(x, y) == SpotClass::Necrotic
    });
    let spot_items: Vec<SizedItem> = necrotic
        .iter()
        .map(|c| sized(image_camera, &plane, c))
        .collect();
    let affected: Vec<(u32, u32)> = leaf_pixels
        .iter()
        .copied()
        .filter(|&(x, y)| matches!(class(x, y), SpotClass::Necrotic | SpotClass::Halo))
        .collect();
    let affected_pct = if area_m2 > 0.0 {
        100.0 * projected_area(image_camera, &plane, &affected) / area_m2
    } else {
        0.0
    };
    let holes = mask::holes(leaf.view())
        .iter()
        .map(|c| sized(image_camera, &plane, c))
        .collect();
    Some(LeafGeometry {
        plane,
        rms_m: fit.rms,
        coverage: region.coverage(),
        distance_m,
        length_m,
        area_m2,
        spots: spot_items,
        affected_pct,
        holes,
    })
}

#[cfg(test)]
mod tests {
    use domain::scan::{DepthMeta, DepthSource, Intrinsics};

    use super::*;

    fn camera(size: (u32, u32)) -> Camera {
        let meta = DepthMeta {
            source: DepthSource::Lidar,
            intrinsics: Intrinsics {
                fx: 1000.0,
                fy: 1000.0,
                cx: 500.0,
                cy: 500.0,
                ref_width: 1000,
                ref_height: 1000,
            },
            distortion: None,
            gravity: None,
            device_model: "test".to_owned(),
            filtered: false,
        };
        Camera::for_image(&meta, size)
    }

    #[test]
    fn a_fronto_parallel_pixel_at_30_cm_is_0_3_mm_square() {
        let plane = Plane {
            n: Vector3::new(0.0, 0.0, -1.0),
            d: 0.3,
        };
        let (area, p) = pixel_area(&camera((1000, 1000)), &plane, 499, 499).unwrap();
        assert!((area - 0.0003f64.powi(2)).abs() < 1e-12);
        assert!((p.z - 0.3).abs() < 1e-12);
    }

    #[test]
    fn a_tilted_plane_has_a_larger_footprint() {
        let tilted = Plane {
            n: Vector3::new(0.0, -0.5f64.sqrt(), -0.5f64.sqrt()),
            d: 0.3 * 0.5f64.sqrt(),
        };
        let flat = Plane {
            n: Vector3::new(0.0, 0.0, -1.0),
            d: 0.3,
        };
        let cam = camera((1000, 1000));
        let (a, _) = pixel_area(&cam, &tilted, 499, 499).unwrap();
        let (b, _) = pixel_area(&cam, &flat, 499, 499).unwrap();
        assert!((a / b - 2f64.sqrt()).abs() < 1e-2); // half a pixel off the axis
    }

    #[test]
    fn longest_extent_of_a_square_is_its_diagonal() {
        let pts = [
            Vector3::new(0.0, 0.0, 0.3),
            Vector3::new(0.1, 0.0, 0.3),
            Vector3::new(0.0, 0.1, 0.3),
            Vector3::new(0.1, 0.1, 0.3),
        ];
        assert!((longest_extent(&pts, 100) - 0.02f64.sqrt()).abs() < 1e-12);
    }
}
