//! The depth map as 3D points: which pixels are confident, and back-projection.

use nalgebra::Vector3;

use super::camera::Camera;
use crate::{
    depth::{ConfidenceMap, DepthConfidence, DepthMap},
    mask::MaskBuf,
};

/// A depth map with its confidence and camera. `'a` borrows the decoded depth and confidence
/// buffers, which the caller owns for the whole measurement.
#[derive(Debug, Clone)]
pub struct DepthScene<'a> {
    depth: DepthMap<'a>,
    confidence: Option<ConfidenceMap<'a>>,
    camera: Camera,
}

/// The confident depth points inside a region.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RegionPoints {
    /// The 3D points, metres.
    pub points: Vec<Vector3<f64>>,
    /// Their depth pixels.
    pub pixels: Vec<(u32, u32)>,
    /// The region's pixel count on the depth grid (the coverage denominator).
    pub region_pixels: usize,
}

impl RegionPoints {
    /// The contract's coverage: confident pixels over region pixels.
    pub fn coverage(&self) -> f64 {
        self.points.len() as f64 / self.region_pixels.max(1) as f64
    }

    /// The median camera distance of the points.
    pub fn median_distance(&self) -> Option<f64> {
        let d: Vec<f64> = self.points.iter().map(Vector3::norm).collect();
        super::plane::median(&d)
    }
}

impl<'a> DepthScene<'a> {
    /// A scene; the confidence map, when present, has the depth map's size.
    pub fn new(depth: DepthMap<'a>, confidence: Option<ConfidenceMap<'a>>, camera: Camera) -> Self {
        Self {
            depth,
            confidence,
            camera,
        }
    }

    /// The depth grid's size.
    pub const fn size(&self) -> (u32, u32) {
        (self.depth.width(), self.depth.height())
    }

    /// The depth map.
    pub const fn depth(&self) -> DepthMap<'a> {
        self.depth
    }

    /// The depth camera.
    pub const fn camera(&self) -> &Camera {
        &self.camera
    }

    /// Medium or high confidence with depth. Without a confidence map, every non-zero pixel
    /// counts as medium ([contracts 7]).
    pub fn confident(&self, x: u32, y: u32) -> bool {
        self.depth.metres(x, y).is_some()
            && self
                .confidence
                .is_none_or(|c| c.level(x, y) >= DepthConfidence::Medium)
    }

    /// The 3D point of a confident pixel.
    pub fn point(&self, x: u32, y: u32) -> Option<Vector3<f64>> {
        if !self.confident(x, y) {
            return None;
        }
        self.depth.metres(x, y).map(|z| self.camera.point(x, y, z))
    }

    /// The confident points inside `region`, a mask on the depth grid.
    pub fn points_in(&self, region: &MaskBuf) -> RegionPoints {
        let mut out = RegionPoints::default();
        let (w, h) = self.size();
        for y in 0..h {
            for x in 0..w {
                if !region.get(x, y) {
                    continue;
                }
                out.region_pixels += 1;
                if let Some(p) = self.point(x, y) {
                    out.points.push(p);
                    out.pixels.push((x, y));
                }
            }
        }
        out
    }
}
