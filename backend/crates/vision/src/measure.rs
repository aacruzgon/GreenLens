//! The measurement stage ([contracts 7], measurement; [architecture 6.7]): deterministic
//! geometry over a stored photo, its depth map and its segmentation regions. It runs in `api`
//! on a blocking thread, called by `pipeline`; answering measurable indicators from its output
//! belongs to `pipeline`.
//!
//! [contracts 7]: ../../../docs/architecture/contracts.md#measurement
//! [architecture 6.7]: ../../../docs/architecture/README.md#67-depth-and-measurement-lidar

use std::sync::Arc;

use domain::{
    id::{ImageId, RegionId},
    jobs::{RegionClass, RleMask},
    ports::SpotSegmenter,
    records::measurement::{MeasureQuantity, Measurement},
    scan::{DepthMeta, DepthProblem, ImageKind},
    wire::{BBox, Certainty, Confidence},
};
use nalgebra::Vector3;

use crate::{
    VisionError,
    depth::{self, DepthMap},
    imaging::{self, ImageView, PixelRect},
    mask::{self, MaskBuf},
    rle::{self, RleError},
};

pub mod camera;
pub mod confidence;
pub mod config;
pub mod leaf;
pub mod plane;
pub mod plant;
pub mod scene;
pub mod spots;

pub use camera::stored_fx;
pub use config::MeasurementConfig;

use camera::Camera;
use confidence::{Criteria, band};
use leaf::{LeafGeometry, measure_leaf};
use plant::{PlantHint, measure_plant};
use scene::DepthScene;

/// The algorithm's own version; a measurement's `model_version` is
/// `<Measurer::version()>+<device_model>`.
pub const ALGORITHM_VERSION: &str = "measure-1";

/// A segmentation region of the photo, as the perception result stored it.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasureRegion {
    /// `<image_id>.<n>`.
    pub region_id: RegionId,
    /// Its class.
    pub class: RegionClass,
    /// Its box.
    pub bbox: BBox,
    /// Its mask, when segmentation produced one.
    pub mask: Option<RleMask>,
}

/// One photo to measure, borrowed for the duration of the blocking call. `'a` borrows the
/// caller's buffers: the stored file, the depth parts, the meta and the regions.
#[derive(Debug, Clone, Copy)]
pub struct MeasureInput<'a> {
    /// The photo.
    pub image_id: ImageId,
    /// Its kind: `whole_plant` gives height and width, the leaf kinds the leaf quantities.
    pub kind: ImageKind,
    /// The stored image file (`WebP`).
    pub photo: &'a [u8],
    /// The stored 16-bit depth PNG.
    pub depth_png: &'a [u8],
    /// The stored confidence PNG, when there is one.
    pub confidence_png: Option<&'a [u8]>,
    /// The stored meta (rotated and scaled to the stored image by the upload).
    pub meta: &'a DepthMeta,
    /// The photo's regions.
    pub regions: &'a [MeasureRegion],
}

/// One measured quantity of one image: an observation's `detail.measurement`, its confidence,
/// and its `region` box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasuredQuantity {
    /// The measurement.
    pub measurement: Measurement,
    /// The lowest band of the criteria.
    pub certainty: Certainty,
    /// `certainty.confidence()`: 0.4, 0.65 or 0.85.
    pub confidence: Confidence,
    /// The plant's box, or the box around the measured leaves.
    pub bbox: BBox,
}

/// Why a photo couldn't be measured. Any error skips the stage; the scan goes on.
#[derive(Debug, thiserror::Error)]
pub enum MeasureError {
    /// The stored photo didn't decode.
    #[error("photo: {0}")]
    Photo(#[source] VisionError),
    /// The stored depth or confidence map didn't decode.
    #[error("depth: {0:?}")]
    Depth(DepthProblem),
    /// A region's mask didn't decode, or has another size than the photo.
    #[error("mask: {0}")]
    Mask(#[from] RleError),
    /// The meta doesn't fit the photo.
    #[error("meta: {0}")]
    Meta(&'static str),
}

/// The measurement stage. `Send + Sync`: build one at start and share it in an `Arc`.
#[derive(Clone)]
pub struct Measurer {
    config: MeasurementConfig,
    spots: Arc<dyn SpotSegmenter>,
}

impl std::fmt::Debug for Measurer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Measurer")
            .field("config", &self.config)
            .field("spots", &self.spots.version())
            .finish()
    }
}

impl Measurer {
    /// A measurer with the given constants and spot segmenter.
    pub fn new(config: MeasurementConfig, spots: Arc<dyn SpotSegmenter>) -> Self {
        Self { config, spots }
    }

    /// `measure-1+<spot segmenter version>`.
    pub fn version(&self) -> String {
        format!("{ALGORITHM_VERSION}+{}", self.spots.version())
    }

    /// The constants in use.
    pub const fn config(&self) -> &MeasurementConfig {
        &self.config
    }

    /// Measures one photo. Blocking. Returns only the quantities whose needs are met
    /// ([contracts 7], the table of quantities); an empty list is a valid result.
    pub fn measure(&self, input: &MeasureInput<'_>) -> Result<Vec<MeasuredQuantity>, MeasureError> {
        let photo = imaging::decode_stored(input.photo).map_err(MeasureError::Photo)?;
        let size = photo.dimensions();
        let depth = depth::decode_depth_png(input.depth_png).map_err(MeasureError::Depth)?;
        let confidence = input
            .confidence_png
            .map(|c| depth::decode_confidence_png(c, (depth.view().width(), depth.view().height())))
            .transpose()
            .map_err(MeasureError::Depth)?;
        let aspect = |w: u32, h: u32| f64::from(w) / f64::from(h.max(1));
        if (aspect(size.0, size.1) / aspect(depth.view().width(), depth.view().height()) - 1.0)
            .abs()
            > 0.01
        {
            return Err(MeasureError::Meta("depth map and photo differ in aspect"));
        }
        let scene = DepthScene::new(
            depth.view(),
            confidence.as_ref().map(depth::ConfidenceMapBuf::view),
            Camera::for_depth(input.meta, depth.view()),
        );
        let ctx = Context {
            input,
            size,
            scene: &scene,
            image: ImageView::from(&photo),
            image_camera: Camera::for_image(input.meta, size),
            filtered_without_confidence: input.meta.filtered && confidence.is_none(),
        };
        match input.kind {
            ImageKind::WholePlant => self.whole_plant(&ctx),
            ImageKind::LeafCloseup | ImageKind::LeafUnderside => self.leaves(&ctx),
            _ => Ok(Vec::new()),
        }
    }

    /// [contracts 7] organism length: the longer side of the box at the median confident
    /// depth inside it, in millimetres. Not tilt-corrected; shown, never scored.
    pub fn organism_length_mm(
        &self,
        depth: &DepthMap<'_>,
        meta: &DepthMeta,
        bbox: BBox,
    ) -> Option<f64> {
        let rect = PixelRect::from_normalised(bbox.to_array(), (depth.width(), depth.height()));
        let mut z: Vec<f64> = Vec::new();
        for y in rect.y..(rect.y + rect.h).min(depth.height()) {
            for x in rect.x..(rect.x + rect.w).min(depth.width()) {
                z.extend(depth.metres(x, y));
            }
        }
        let z = plane::median(&z)?;
        let k = meta.intrinsics;
        let [_, _, w, h] = bbox.to_array();
        let width_m = w * f64::from(k.ref_width) * z / k.fx;
        let height_m = h * f64::from(k.ref_height) * z / k.fy;
        Some(width_m.max(height_m) * 1000.0)
    }

    fn seed(input: &MeasureInput<'_>, n: u32) -> u64 {
        plane::seed_for(&[input.image_id.to_string().as_bytes(), &n.to_le_bytes()])
    }

    fn whole_plant(&self, ctx: &Context<'_, '_>) -> Result<Vec<MeasuredQuantity>, MeasureError> {
        let cfg = &self.config;
        let Some(g) = ctx.input.meta.gravity else {
            return Ok(Vec::new()); // both quantities need gravity
        };
        let gravity = Vector3::new(g[0], g[1], g[2]).normalize();
        let (dw, dh) = ctx.scene.size();
        let plant = ctx
            .input
            .regions
            .iter()
            .find(|r| r.class == RegionClass::Plant);
        let hint = match plant {
            Some(MeasureRegion { mask: Some(m), .. }) => {
                let m = rle::decode(m)?;
                PlantHint::Mask(mask::resample(m.view(), dw, dh))
            }
            Some(r) => PlantHint::Box(MaskBuf::from_rect(
                dw,
                dh,
                PixelRect::from_normalised(r.bbox.to_array(), (dw, dh)),
            )),
            None => PlantHint::None,
        };
        let Some(geo) = measure_plant(ctx.scene, gravity, &hint, cfg, Self::seed(ctx.input, 0))
        else {
            return Ok(Vec::new());
        };
        let criteria = Criteria {
            coverage: geo.coverage,
            residual_m: None,
            distance_m: geo.distance_m,
            range_m: cfg.whole_plant_range_m,
            plant_region: Some(geo.region),
            filtered_without_confidence: ctx.filtered_without_confidence,
        };
        let certainty = band(&criteria, cfg);
        let bbox = plant
            .map(|r| r.bbox)
            .or_else(|| BBox::new(geo.bbox[0], geo.bbox[1], geo.bbox[2], geo.bbox[3]).ok())
            .unwrap_or(BBox::FULL);
        let one = |quantity, metres: f64| MeasuredQuantity {
            measurement: single(quantity, metres * 100.0, geo.distance_m),
            certainty,
            confidence: certainty.confidence(),
            bbox,
        };
        let mut out = Vec::new();
        out.extend(geo.height_m.map(|h| one(MeasureQuantity::PlantHeightCm, h)));
        out.extend(geo.width_m.map(|w| one(MeasureQuantity::CanopyWidthCm, w)));
        Ok(out)
    }

    /// Measures every leaf region that has a mask, with the region's box.
    fn measure_leaves(
        &self,
        ctx: &Context<'_, '_>,
    ) -> Result<Vec<(LeafGeometry, BBox)>, MeasureError> {
        let cfg = &self.config;
        let mut measured: Vec<(LeafGeometry, BBox)> = Vec::new();
        for region in ctx
            .input
            .regions
            .iter()
            .filter(|r| r.class == RegionClass::Leaf)
        {
            let Some(rle_mask) = &region.mask else {
                continue;
            };
            let leaf = rle::decode(rle_mask)?;
            if (leaf.width(), leaf.height()) != ctx.size {
                return Err(MeasureError::Meta("mask size differs from the photo"));
            }
            if let Some(geo) = measure_leaf(
                ctx.scene,
                &ctx.image_camera,
                ctx.image,
                &leaf,
                self.spots.as_ref(),
                cfg,
                Self::seed(ctx.input, region.region_id.n),
            ) {
                measured.push((geo, region.bbox));
            }
        }
        Ok(measured)
    }

    fn leaves(&self, ctx: &Context<'_, '_>) -> Result<Vec<MeasuredQuantity>, MeasureError> {
        let cfg = &self.config;
        let measured = self.measure_leaves(ctx)?;
        if measured.is_empty() {
            return Ok(Vec::new());
        }
        let bbox = union_bbox(measured.iter().map(|(_, b)| *b));
        let leaves: Vec<&LeafGeometry> = measured.iter().map(|(g, _)| g).collect();
        let rate = |used: &[&LeafGeometry], distance_m: f64| {
            band(
                &Criteria {
                    coverage: pooled_coverage(used),
                    residual_m: plane::median(&used.iter().map(|l| l.rms_m).collect::<Vec<_>>()),
                    distance_m,
                    range_m: cfg.leaf_range_m,
                    plant_region: None,
                    filtered_without_confidence: ctx.filtered_without_confidence,
                },
                cfg,
            )
        };
        let mut out = Vec::new();
        let mut push = |quantity, items: &[f64], used: &[&LeafGeometry]| {
            let distances: Vec<f64> = used.iter().map(|l| l.distance_m).collect();
            if let Some(m) = aggregate(quantity, items, &distances) {
                let certainty = rate(used, m.distance_m);
                out.push(MeasuredQuantity {
                    measurement: m,
                    certainty,
                    confidence: certainty.confidence(),
                    bbox,
                });
            }
        };
        let per_leaf =
            |f: fn(&LeafGeometry) -> f64| leaves.iter().map(|l| f(l)).collect::<Vec<_>>();
        push(
            MeasureQuantity::LeafLengthCm,
            &per_leaf(|l| l.length_m * 100.0),
            &leaves,
        );
        push(
            MeasureQuantity::LeafAreaCm2,
            &per_leaf(|l| l.area_m2 * 1e4),
            &leaves,
        );
        push(
            MeasureQuantity::LesionCount,
            &per_leaf(|l| l.spots.len() as f64),
            &leaves,
        );
        push(
            MeasureQuantity::AffectedAreaPct,
            &per_leaf(|l| l.affected_pct),
            &leaves,
        );
        push(
            MeasureQuantity::HoleCount,
            &per_leaf(|l| l.holes.len() as f64),
            &leaves,
        );
        let sized = |items: fn(&LeafGeometry) -> &[leaf::SizedItem]| {
            let used: Vec<&LeafGeometry> = leaves
                .iter()
                .copied()
                .filter(|l| items(l).iter().any(|s| s.diameter_px >= cfg.min_sized_px))
                .collect();
            let values: Vec<f64> = used
                .iter()
                .flat_map(|l| items(l).iter())
                .filter(|s| s.diameter_px >= cfg.min_sized_px)
                .map(|s| s.diameter_m() * 1000.0)
                .collect();
            (values, used)
        };
        let (spots, with_spots) = sized(|l| &l.spots);
        push(MeasureQuantity::LesionDiameterMm, &spots, &with_spots);
        let (holes, with_holes) = sized(|l| &l.holes);
        push(MeasureQuantity::HoleDiameterMm, &holes, &with_holes);
        Ok(out)
    }
}

/// What every quantity of one photo shares. `'i` borrows the input, `'s` the scene (which itself
/// borrows the decoded depth buffers for `'s`).
struct Context<'i, 's> {
    input: &'i MeasureInput<'i>,
    size: (u32, u32),
    scene: &'s DepthScene<'s>,
    image: ImageView<'s>,
    image_camera: Camera,
    filtered_without_confidence: bool,
}

fn single(quantity: MeasureQuantity, value: f64, distance_m: f64) -> Measurement {
    Measurement {
        quantity,
        value,
        min: value,
        max: value,
        n: 1,
        distance_m,
    }
}

/// `value` = nearest-rank median of the items, `min`, `max`, `n`; `distance_m` = median of the
/// contributing leaves' distances. `None` without items.
pub fn aggregate(
    quantity: MeasureQuantity,
    items: &[f64],
    distances: &[f64],
) -> Option<Measurement> {
    let value = plane::median(items)?;
    Some(Measurement {
        quantity,
        value,
        min: items.iter().copied().fold(f64::INFINITY, f64::min),
        max: items.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        n: u32::try_from(items.len()).unwrap_or(u32::MAX),
        distance_m: plane::median(distances)?,
    })
}

fn pooled_coverage(leaves: &[&LeafGeometry]) -> f64 {
    let n = leaves.len().max(1) as f64;
    leaves.iter().map(|l| l.coverage).sum::<f64>() / n
}

fn union_bbox(boxes: impl Iterator<Item = BBox>) -> BBox {
    let (mut x0, mut y0, mut x1, mut y1) = (1.0f64, 1.0f64, 0.0f64, 0.0f64);
    for b in boxes {
        let [x, y, w, h] = b.to_array();
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x + w);
        y1 = y1.max(y + h);
    }
    BBox::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)).unwrap_or(BBox::FULL)
}

#[cfg(test)]
mod tests {
    use domain::scan::{DepthSource, Intrinsics};

    use super::*;
    use crate::{depth::DepthMapBuf, measure::spots::ColourSpotSegmenter};

    /// A flat leaf at 30 cm facing the camera, 120 px square with a brown disk 20 px across,
    /// on a 200 px photo with fx = 200: one pixel is 1.5 mm, so the spot is 30 mm across, the
    /// leaf 18 cm a side (25.5 cm across the diagonal) and 324 cm².
    #[test]
    fn a_synthetic_leaf_measures_to_its_true_size() {
        let photo = image::RgbImage::from_fn(200, 200, |x, y| {
            let (dx, dy) = (f64::from(x) - 100.0, f64::from(y) - 100.0);
            image::Rgb(if dx * dx + dy * dy < 100.0 {
                [110, 70, 30]
            } else {
                [60, 140, 50]
            })
        });
        // PNG, not lossy WebP: chroma subsampling would soften the spot's rim and the test is
        // about geometry, not the colour thresholds.
        let mut webp = Vec::new();
        photo
            .write_to(
                &mut std::io::Cursor::new(&mut webp),
                image::ImageFormat::Png,
            )
            .unwrap();
        let leaf = MaskBuf::from_rect(
            200,
            200,
            PixelRect {
                x: 40,
                y: 40,
                w: 120,
                h: 120,
            },
        );
        let depth_png = depth::encode_depth_png(
            DepthMapBuf::from_raw(50, 50, vec![300; 2500])
                .unwrap()
                .view(),
        )
        .unwrap();
        let meta = DepthMeta {
            source: DepthSource::Lidar,
            intrinsics: Intrinsics {
                fx: 200.0,
                fy: 200.0,
                cx: 100.0,
                cy: 100.0,
                ref_width: 200,
                ref_height: 200,
            },
            distortion: None,
            gravity: None,
            device_model: "test".to_owned(),
            filtered: false,
        };
        let image_id: ImageId = "01JAX6BBBBBBBBBBBBBBBBBBBB".parse().unwrap();
        let regions = [MeasureRegion {
            region_id: RegionId {
                image: image_id,
                n: 1,
            },
            class: RegionClass::Leaf,
            bbox: BBox::new(0.2, 0.2, 0.6, 0.6).unwrap(),
            mask: Some(rle::encode(leaf.view())),
        }];
        let measurer = Measurer::new(
            MeasurementConfig::default(),
            Arc::new(ColourSpotSegmenter::default()),
        );
        let out = measurer
            .measure(&MeasureInput {
                image_id,
                kind: ImageKind::LeafCloseup,
                photo: &webp,
                depth_png: &depth_png,
                confidence_png: None,
                meta: &meta,
                regions: &regions,
            })
            .unwrap();
        let get = |q| out.iter().find(|m| m.measurement.quantity == q).unwrap();
        let spot = get(MeasureQuantity::LesionDiameterMm).measurement;
        assert!((spot.value - 30.0).abs() < 2.0, "spot {}", spot.value);
        assert_eq!(get(MeasureQuantity::LesionCount).measurement.value, 1.0);
        let area = get(MeasureQuantity::LeafAreaCm2).measurement.value;
        assert!((area - 324.0).abs() < 3.0, "area {area}");
        let length = get(MeasureQuantity::LeafLengthCm).measurement.value;
        assert!((length - 25.46).abs() < 0.5, "length {length}");
        assert_eq!(get(MeasureQuantity::HoleCount).measurement.value, 0.0);
        assert!(out.iter().all(|m| m.certainty == Certainty::High));
        // the same input measures the same twice
        assert_eq!(
            out,
            measurer
                .measure(&MeasureInput {
                    image_id,
                    kind: ImageKind::LeafCloseup,
                    photo: &webp,
                    depth_png: &depth_png,
                    confidence_png: None,
                    meta: &meta,
                    regions: &regions,
                })
                .unwrap()
        );
    }

    #[test]
    fn aggregate_takes_the_nearest_rank_median() {
        let m = aggregate(
            MeasureQuantity::LesionDiameterMm,
            &[2.0, 1.5, 3.0, 2.4],
            &[0.3, 0.32],
        )
        .unwrap();
        assert_eq!((m.value, m.min, m.max, m.n), (2.0, 1.5, 3.0, 4));
        assert!((m.distance_m - 0.3).abs() < 1e-12);
        assert!(aggregate(MeasureQuantity::LeafAreaCm2, &[], &[0.3]).is_none());
    }

    #[test]
    fn an_organism_box_at_30_cm_has_its_real_length() {
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
        let depth = DepthMapBuf::from_raw(10, 10, vec![300; 100]).unwrap();
        let measurer = Measurer::new(
            MeasurementConfig::default(),
            Arc::new(ColourSpotSegmenter::default()),
        );
        // 0.1 of 1000 px at 0.3 m with fx 1000 → 30 mm
        let len = measurer
            .organism_length_mm(
                &depth.view(),
                &meta,
                BBox::new(0.4, 0.4, 0.1, 0.05).unwrap(),
            )
            .unwrap();
        assert!((len - 30.0).abs() < 1e-9);
        assert_eq!(measurer.version(), "measure-1+colour-3");
    }
}
