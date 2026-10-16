//! The pinhole camera of a stored image and of its depth map ([contracts 7], pixels to a
//! plane): rays through K⁻¹ after undistortion, back-projection of depth pixels.

use domain::scan::{DepthMeta, Intrinsics};
use nalgebra::Vector3;

use crate::depth::{DepthMap, scale_intrinsics};

/// Apple's lens-distortion lookup ([`domain::scan::Distortion`]): the table gives, for radii
/// from 0 to the largest distance between the centre and a corner, the relative magnification
/// to apply to a distorted point's offset from the centre. Linear interpolation between entries,
/// as Apple's sample code does (*unverified* against a real capture until the day-1 check).
#[derive(Debug, Clone, PartialEq)]
pub struct Undistort {
    center: [f64; 2],
    max_radius: f64,
    table: Vec<f64>,
}

impl Undistort {
    /// From the stored meta's distortion (already in stored-image pixels).
    pub fn new(center: [f64; 2], table: Vec<f64>, size: (u32, u32)) -> Option<Self> {
        if table.len() < 2 {
            return None;
        }
        let (w, h) = (f64::from(size.0), f64::from(size.1));
        let corners = [[0.0, 0.0], [w, 0.0], [0.0, h], [w, h]];
        let max_radius = corners
            .iter()
            .map(|c| ((c[0] - center[0]).powi(2) + (c[1] - center[1]).powi(2)).sqrt())
            .fold(0.0, f64::max);
        Some(Self {
            center,
            max_radius,
            table,
        })
    }

    /// The undistorted position of a pixel position.
    #[allow(
        clippy::many_single_char_names,
        reason = "u, v, r, t, m: the formula's notation"
    )]
    pub fn apply(&self, u: f64, v: f64) -> (f64, f64) {
        let (dx, dy) = (u - self.center[0], v - self.center[1]);
        let r = (dx * dx + dy * dy).sqrt();
        if r == 0.0 || self.max_radius == 0.0 {
            return (u, v);
        }
        let last = (self.table.len() - 1) as f64;
        let t = (r / self.max_radius * last).clamp(0.0, last);
        let i = t.floor() as usize;
        let frac = t - t.floor();
        let next = self.table[(i + 1).min(self.table.len() - 1)];
        let m = self.table[i] + (next - self.table[i]) * frac;
        (
            self.center[0] + dx * (1.0 + m),
            self.center[1] + dy * (1.0 + m),
        )
    }
}

/// A camera: intrinsics for one image grid, with optional undistortion.
#[derive(Debug, Clone, PartialEq)]
pub struct Camera {
    k: Intrinsics,
    undistort: Option<Undistort>,
}

impl Camera {
    /// The stored image's camera: the stored meta's intrinsics, rescaled when `ref_width`
    /// differs from the image's width.
    pub fn for_image(meta: &DepthMeta, size: (u32, u32)) -> Self {
        let k = scale_intrinsics(meta.intrinsics, size);
        let undistort = meta.distortion.as_ref().and_then(|d| {
            let sx = f64::from(size.0) / f64::from(meta.intrinsics.ref_width);
            let sy = f64::from(size.1) / f64::from(meta.intrinsics.ref_height);
            Undistort::new(
                [d.center[0] * sx, d.center[1] * sy],
                d.inverse_lookup.clone(),
                size,
            )
        });
        Self { k, undistort }
    }

    /// The depth map's camera: the same field of view, scaled to its grid.
    pub fn for_depth(meta: &DepthMeta, depth: DepthMap<'_>) -> Self {
        Self::for_image(meta, (depth.width(), depth.height()))
    }

    /// The intrinsics.
    pub const fn intrinsics(&self) -> &Intrinsics {
        &self.k
    }

    /// The ray K⁻¹ · (u + ½, v + ½, 1) of pixel `(u, v)`, undistorted first; z = 1, so the point
    /// at z-depth `z` is `ray · z`.
    pub fn ray(&self, u: f64, v: f64) -> Vector3<f64> {
        let (pu, pv) = (u + 0.5, v + 0.5);
        let (pu, pv) = self
            .undistort
            .as_ref()
            .map_or((pu, pv), |d| d.apply(pu, pv));
        Vector3::new(
            (pu - self.k.cx) / self.k.fx,
            (pv - self.k.cy) / self.k.fy,
            1.0,
        )
    }

    /// The 3D point of a depth pixel (metres, camera coordinates: x right, y down, z forward),
    /// reading `LiDAR` depth as z-depth.
    pub fn point(&self, u: u32, v: u32, z: f64) -> Vector3<f64> {
        self.ray(f64::from(u), f64::from(v)) * z
    }
}

/// `fx` in stored-image pixels, for the indicator margin (2 × `distance_m` / `fx`). The upload
/// stores intrinsics already scaled to the stored image, so this only rescales when the stored
/// width differs from `ref_width`.
pub fn stored_fx(meta: &DepthMeta, stored_width: u32) -> f64 {
    meta.intrinsics.fx * f64::from(stored_width) / f64::from(meta.intrinsics.ref_width)
}

#[cfg(test)]
mod tests {
    use domain::scan::DepthSource;

    use super::*;

    fn meta() -> DepthMeta {
        DepthMeta {
            source: DepthSource::Lidar,
            intrinsics: Intrinsics {
                fx: 1500.0,
                fy: 1500.0,
                cx: 1000.0,
                cy: 750.0,
                ref_width: 2000,
                ref_height: 1500,
            },
            distortion: None,
            gravity: None,
            device_model: "iPhone16,1".to_owned(),
            filtered: false,
        }
    }

    #[test]
    fn the_centre_ray_looks_straight_ahead_and_depth_cameras_scale() {
        let cam = Camera::for_image(&meta(), (2000, 1500));
        let r = cam.ray(999.5, 749.5);
        assert!(r.x.abs() < 1e-12 && r.y.abs() < 1e-12);
        let small = Camera::for_image(&meta(), (256, 192));
        assert!((small.intrinsics().fx - 192.0).abs() < 1e-9);
        assert!((stored_fx(&meta(), 1000) - 750.0).abs() < 1e-9);
    }

    #[test]
    fn undistortion_moves_points_radially() {
        let d = Undistort::new([50.0, 50.0], vec![0.0, 0.1], (100, 100)).unwrap();
        let (u, v) = d.apply(100.0, 50.0);
        assert!(u > 100.0 && (v - 50.0).abs() < 1e-12);
        assert_eq!(d.apply(50.0, 50.0), (50.0, 50.0));
    }
}
