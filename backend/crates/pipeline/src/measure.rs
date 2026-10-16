//! Measurement ([architecture 6.7]; [contracts 7], measurement and measurable indicators):
//! the port the steps call on a blocking thread, and the pure rule that answers a measurable
//! indicator from the best measurement.
//!
//! **Lifetimes.** `'o` in [`MeasuredObs<'o>`] borrows a measurement out of the step's
//! observations; the answer it produces is owned `IndicatorEvidence`, because it is stored.

mod vision_adapter;

pub use vision_adapter::VisionMeasurer;

use bytes::Bytes;
use domain::{
    id::{ImageId, RegionId},
    jobs::{RegionClass, RleMask},
    knowledge::{IndicatorSeed, MeasureStat},
    ports::ObservationWrite,
    records::{
        diagnosis::{EvidenceSource, IndicatorEvidence, MeasuredValue},
        measurement::{MeasureQuantity, MeasureUnit, Measurement},
        observation::{ObservationDetail, ObservationStage},
    },
    scan::{Answer, DepthMeta, ImageKind},
    wire::{BBox, Confidence},
};

/// A region to measure inside: a segmentation region of the photo, with its mask when it has
/// one.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasureRegion {
    /// The region.
    pub region_id: RegionId,
    /// Its class.
    pub class: RegionClass,
    /// Its box.
    pub bbox: BBox,
    /// Its mask.
    pub mask: Option<RleMask>,
}

/// One photo to measure, owned so it can move into `spawn_blocking` (which needs `'static`).
#[derive(Debug, Clone)]
pub struct MeasureRequest {
    /// The photo.
    pub image_id: ImageId,
    /// Its kind.
    pub kind: ImageKind,
    /// The stored image file.
    pub photo: Bytes,
    /// The 16-bit depth PNG.
    pub depth_png: Bytes,
    /// The confidence PNG, when sent.
    pub confidence_png: Option<Bytes>,
    /// The metadata in the stored image's frame.
    pub meta: DepthMeta,
    /// The photo's segmentation regions.
    pub regions: Vec<MeasureRegion>,
}

/// One measured quantity of a photo: becomes a `measurement` observation.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasuredQuantity {
    /// The measurement.
    pub measurement: Measurement,
    /// 0.4, 0.65 or 0.85 from the confidence rules.
    pub confidence: Confidence,
    /// The observation's region box: the plant's box, or the box around the measured leaves.
    pub bbox: BBox,
}

/// Why a photo couldn't be measured: the stage completes as skipped with `error`.
#[derive(Debug, thiserror::Error)]
#[error("measurement failed: {0}")]
pub struct MeasureFailed(pub String);

/// The measurement geometry. Synchronous CPU work, called inside `spawn_blocking`; the
/// implementation is [`VisionMeasurer`] over `vision::measure`, and tests use a fake.
pub trait ImageMeasurer: Send + Sync {
    /// `measure-<n>+<spot segmenter version>`; the observation's `model_version` appends the
    /// device model.
    fn version(&self) -> String;

    /// Every quantity whose needs are met (contracts 7, the quantity table); never panics on
    /// bad input.
    fn measure(&self, request: &MeasureRequest) -> Result<Vec<MeasuredQuantity>, MeasureFailed>;

    /// [contracts 7] organism length (`detail.length_mm`): the longer side of the organism's
    /// box at the median confident depth inside it, in millimetres, from the photo's 16-bit
    /// depth PNG and its metadata in the stored image's frame. `None` when the depth can't be
    /// read or the box holds no confident depth. The default answers `None` (test fakes).
    fn organism_length_mm(&self, depth_png: &[u8], meta: &DepthMeta, bbox: BBox) -> Option<f64> {
        let _ = (depth_png, meta, bbox);
        None
    }
}

/// The organism observations of a write whose image has depth, as `(index, image, box)`: the
/// ones [`ImageMeasurer::organism_length_mm`] can size ([contracts 7], organism length).
pub fn organism_boxes(
    write: &ObservationWrite,
    with_depth: &[ImageId],
) -> Vec<(usize, ImageId, BBox)> {
    write
        .observations
        .iter()
        .enumerate()
        .filter(|(_, o)| o.stage == ObservationStage::Organism)
        .filter_map(|(i, o)| o.region.as_ref().map(|r| (i, r.image_id, r.bbox)))
        .filter(|(_, image, _)| with_depth.contains(image))
        .collect()
}

/// Sets the observation's `detail.length_mm`, rounded to 0.1 mm.
pub fn set_organism_length(write: &mut ObservationWrite, index: usize, length_mm: f64) {
    if let Some(o) = write.observations.get_mut(index) {
        let detail = o.detail.get_or_insert_with(ObservationDetail::default);
        detail.length_mm = Some((length_mm * 10.0).round() / 10.0);
    }
}

/// A measurement of the step's observations, with what the margin rule needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasuredObs<'o> {
    /// The image it came from.
    pub image_id: ImageId,
    /// That image's kind.
    pub kind: ImageKind,
    /// The measurement, borrowed from its observation.
    pub measurement: &'o Measurement,
    /// Its confidence.
    pub confidence: Confidence,
    /// `fx` of that image in stored-image pixels, from its depth metadata.
    pub fx: Option<f64>,
}

/// The margin at a bound ([contracts 7]): 10% of the bound, but at least two stored-image
/// pixels at the measured distance for lengths and diameters (2 × `distance_m` / `fx` metres,
/// × 1000 in mm, × 100 in cm), and at least 1 for counts.
pub fn margin(bound: f64, quantity: MeasureQuantity, distance_m: f64, fx: Option<f64>) -> f64 {
    let tenth = bound.abs() * 0.1;
    match quantity.unit() {
        MeasureUnit::Count => tenth.max(1.0),
        MeasureUnit::Mm | MeasureUnit::Cm if quantity.is_length() => {
            let per_metre = if quantity.unit() == MeasureUnit::Mm {
                1000.0
            } else {
                100.0
            };
            let pixels = fx
                .filter(|f| *f > 0.0)
                .map_or(0.0, |f| 2.0 * distance_m / f * per_metre);
            tenth.max(pixels)
        }
        _ => tenth,
    }
}

/// The value an indicator's `stat` picks.
pub const fn pick(m: &Measurement, stat: MeasureStat) -> f64 {
    match stat {
        MeasureStat::Median => m.value,
        MeasureStat::Max => m.max,
        MeasureStat::Min => m.min,
    }
}

fn unit_label(q: MeasureQuantity) -> &'static str {
    match q.unit() {
        MeasureUnit::Cm => "cm",
        MeasureUnit::Cm2 => "cm²",
        MeasureUnit::Mm => "mm",
        MeasureUnit::Count => "",
        MeasureUnit::Pct => "%",
    }
}

fn stat_label(q: MeasureQuantity, stat: MeasureStat) -> String {
    let what = match q {
        MeasureQuantity::PlantHeightCm => "height",
        MeasureQuantity::CanopyWidthCm => "canopy width",
        MeasureQuantity::LeafLengthCm => "leaf length",
        MeasureQuantity::LeafAreaCm2 => "leaf area",
        MeasureQuantity::LesionDiameterMm => "spot",
        MeasureQuantity::LesionCount => "spot count",
        MeasureQuantity::AffectedAreaPct => "affected area",
        MeasureQuantity::HoleDiameterMm => "hole",
        MeasureQuantity::HoleCount => "hole count",
    };
    let s = match stat {
        MeasureStat::Median => "Median",
        MeasureStat::Max => "Largest",
        MeasureStat::Min => "Smallest",
    };
    format!("{s} {what}")
}

fn number(v: f64) -> String {
    let s = format!("{v:.1}");
    s.strip_suffix(".0").map_or(s.clone(), str::to_owned)
}

/// The best usable measurement for an indicator: its quantity, from an image whose kind the
/// indicator lists, medium confidence or more; highest confidence, then largest `n`.
pub fn best_measurement<'o>(
    ind: &IndicatorSeed,
    measured: &[MeasuredObs<'o>],
) -> Option<MeasuredObs<'o>> {
    let measure = ind.measure?;
    measured
        .iter()
        .filter(|m| {
            m.measurement.quantity == measure.quantity
                && m.confidence.at_least_medium()
                && ind.image_kinds.contains(&m.kind)
        })
        .max_by(|a, b| {
            a.confidence
                .get()
                .total_cmp(&b.confidence.get())
                .then(a.measurement.n.cmp(&b.measurement.n))
        })
        .copied()
}

/// Answers a measurable indicator ([contracts 7], the margin table): inside the range by at
/// least the margin from each bound it has is `yes`; outside by more than the margin is `no`;
/// anything else, or no usable measurement, is `None` and the indicator goes to the VLM.
pub fn answer_indicator(
    ind: &IndicatorSeed,
    measured: &[MeasuredObs<'_>],
) -> Option<IndicatorEvidence> {
    let measure = ind.measure?;
    let best = best_measurement(ind, measured)?;
    let m = best.measurement;
    let v = pick(m, measure.stat);
    let mg = |bound: f64| margin(bound, measure.quantity, m.distance_m, best.fx);
    let inside = measure.min.is_none_or(|lo| v >= lo + mg(lo))
        && measure.max.is_none_or(|hi| v <= hi - mg(hi));
    let outside = measure.min.is_some_and(|lo| v < lo - mg(lo))
        || measure.max.is_some_and(|hi| v > hi + mg(hi));
    let answer = if inside {
        Answer::Yes
    } else if outside {
        Answer::No
    } else {
        return None;
    };
    let unit = unit_label(measure.quantity);
    let with_unit = |x: f64| {
        if unit.is_empty() {
            number(x)
        } else {
            format!("{} {unit}", number(x))
        }
    };
    let range = match (measure.min, measure.max) {
        (Some(lo), Some(hi)) => format!("range {}–{}", number(lo), with_unit(hi)),
        (Some(lo), None) => format!("at least {}", with_unit(lo)),
        (None, Some(hi)) => format!("at most {}", with_unit(hi)),
        (None, None) => String::new(),
    };
    Some(IndicatorEvidence {
        indicator_id: ind.id.clone(),
        text: ind.text.clone(),
        weight: ind.weight,
        answer,
        reason: format!(
            "{} {}; {range}",
            stat_label(measure.quantity, measure.stat),
            with_unit(v)
        ),
        source: EvidenceSource::Measurement,
        measured: Some(MeasuredValue {
            quantity: measure.quantity,
            value: v,
            image_id: best.image_id,
        }),
    })
}

/// The images whose measurements show nothing on any leaf: `lesion_count` and `hole_count`
/// both measured with a `max` of 0 ([contracts 7]: a measured `no` is evidence of health only
/// from such an image).
pub fn clean_images(measured: &[MeasuredObs<'_>]) -> std::collections::BTreeSet<ImageId> {
    let zero = |img: ImageId, q: MeasureQuantity| {
        measured
            .iter()
            .any(|m| m.image_id == img && m.measurement.quantity == q && m.measurement.max == 0.0)
    };
    measured
        .iter()
        .map(|m| m.image_id)
        .filter(|&img| {
            zero(img, MeasureQuantity::LesionCount) && zero(img, MeasureQuantity::HoleCount)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn organism_lengths_go_on_organisms_of_photos_with_depth() {
        use crate::testkit::{image_id, obs};
        use domain::{events::Stage, records::observation::ObservationSource};
        let organism = obs(
            ObservationStage::Organism,
            Stage::VlmFindings,
            ObservationSource::Vlm,
            0.8,
        );
        let soil = obs(
            ObservationStage::Soil,
            Stage::VlmFindings,
            ObservationSource::Vlm,
            0.8,
        );
        let mut write = ObservationWrite {
            stage: Stage::VlmFindings,
            run: domain::wire::Run(1),
            images: vec![image_id()],
            replace_imageless: true,
            observations: vec![soil, organism],
            masks: Vec::new(),
        };
        assert!(
            organism_boxes(&write, &[]).is_empty(),
            "no depth, no length"
        );
        let boxes = organism_boxes(&write, &[image_id()]);
        assert_eq!(boxes, vec![(1, image_id(), BBox::FULL)], "organisms only");
        set_organism_length(&mut write, 1, 12.345);
        let detail = write.observations[1].detail.as_ref().unwrap();
        assert_eq!(detail.length_mm, Some(12.3));
        assert!(write.observations[0].detail.is_none());
    }

    fn septoria_small_spots() -> IndicatorSeed {
        serde_json::from_value(serde_json::json!({
            "id": "tomato_septoria_leaf_spot.small_spots", "text": "Spots about 1.5-3 mm across",
            "question": "Are the spots small?", "weight": 3, "discriminating": true,
            "where": "lower_leaves", "image_kinds": ["leaf_closeup"],
            "measure": {"quantity": "lesion_diameter_mm", "stat": "median", "min": 1.5, "max": 3}
        }))
        .unwrap()
    }

    fn spot(value: f64, n: u32) -> Measurement {
        Measurement {
            quantity: MeasureQuantity::LesionDiameterMm,
            value,
            min: value,
            max: value,
            n,
            distance_m: 0.3,
        }
    }

    fn obs(m: &Measurement, confidence: f64) -> MeasuredObs<'_> {
        MeasuredObs {
            image_id: "01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().unwrap(),
            kind: ImageKind::LeafCloseup,
            measurement: m,
            confidence: Confidence::new(confidence).unwrap(),
            fx: Some(1500.0),
        }
    }

    #[test]
    fn the_margin_table() {
        let ind = septoria_small_spots();
        // 2 px at 0.3 m with fx 1500 is 0.4 mm, more than 10% of 1.5 (0.15) or 3 (0.3).
        assert!(
            (margin(1.5, MeasureQuantity::LesionDiameterMm, 0.3, Some(1500.0)) - 0.4).abs() < 1e-9
        );
        let inside = spot(2.1, 12);
        let e = answer_indicator(&ind, &[obs(&inside, 0.85)]).unwrap();
        assert_eq!(e.answer, Answer::Yes);
        assert_eq!(e.reason, "Median spot 2.1 mm; range 1.5–3 mm");
        let big = spot(9.0, 12);
        assert_eq!(
            answer_indicator(&ind, &[obs(&big, 0.85)]).unwrap().answer,
            Answer::No
        );
        let edge = spot(2.8, 12);
        assert!(
            answer_indicator(&ind, &[obs(&edge, 0.85)]).is_none(),
            "within the margin of 3 mm"
        );
        let low = spot(2.1, 12);
        assert!(
            answer_indicator(&ind, &[obs(&low, 0.4)]).is_none(),
            "low measurements never answer"
        );
    }

    #[test]
    fn best_is_highest_confidence_then_largest_n() {
        let ind = septoria_small_spots();
        let a = spot(2.0, 40);
        let b = spot(9.0, 3);
        let c = spot(2.2, 8);
        let list = [obs(&a, 0.65), obs(&b, 0.85), obs(&c, 0.85)];
        let best = best_measurement(&ind, &list).unwrap();
        assert!((best.measurement.value - 2.2).abs() < 1e-9);
    }

    #[test]
    fn clean_images_need_both_counts_at_zero() {
        let mut lesions = spot(0.0, 3);
        lesions.quantity = MeasureQuantity::LesionCount;
        let mut holes = lesions;
        holes.quantity = MeasureQuantity::HoleCount;
        assert!(clean_images(&[obs(&lesions, 0.85)]).is_empty());
        assert_eq!(
            clean_images(&[obs(&lesions, 0.85), obs(&holes, 0.85)]).len(),
            1
        );
        holes.max = 2.0;
        assert!(clean_images(&[obs(&lesions, 0.85), obs(&holes, 0.85)]).is_empty());
    }
}
