//! `Measurement` ([contracts 7], measurement): `LiDAR` sizes in real units.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::scan::ImageKind;

/// `MeasureQuantity` ([contracts 7]). The unit is in the name.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    ToSchema,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MeasureQuantity {
    /// From `whole_plant`: one value; needs gravity and a ground plane.
    PlantHeightCm,
    /// From `whole_plant`: one value; needs gravity.
    CanopyWidthCm,
    /// Per leaf with a mask.
    LeafLengthCm,
    /// Per leaf with a mask.
    LeafAreaCm2,
    /// Per spot at least 4 pixels across.
    LesionDiameterMm,
    /// Spots per leaf with a mask.
    LesionCount,
    /// Share of each leaf covered by spots and their yellowing.
    AffectedAreaPct,
    /// Per hole at least 4 pixels across.
    HoleDiameterMm,
    /// Holes per leaf with a mask, of any size.
    HoleCount,
}

/// A quantity's unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeasureUnit {
    /// Centimetres.
    Cm,
    /// Square centimetres.
    Cm2,
    /// Millimetres.
    Mm,
    /// A count.
    Count,
    /// A percentage.
    Pct,
}

impl MeasureQuantity {
    /// Every quantity.
    pub const ALL: [Self; 9] = [
        Self::PlantHeightCm,
        Self::CanopyWidthCm,
        Self::LeafLengthCm,
        Self::LeafAreaCm2,
        Self::LesionDiameterMm,
        Self::LesionCount,
        Self::AffectedAreaPct,
        Self::HoleDiameterMm,
        Self::HoleCount,
    ];

    /// The unit.
    pub const fn unit(self) -> MeasureUnit {
        match self {
            Self::PlantHeightCm | Self::CanopyWidthCm | Self::LeafLengthCm => MeasureUnit::Cm,
            Self::LeafAreaCm2 => MeasureUnit::Cm2,
            Self::LesionDiameterMm | Self::HoleDiameterMm => MeasureUnit::Mm,
            Self::LesionCount | Self::HoleCount => MeasureUnit::Count,
            Self::AffectedAreaPct => MeasureUnit::Pct,
        }
    }

    /// Whether a photo of this kind can measure the quantity (contracts 7, the table of
    /// quantities). The knowledge loader rejects a `measure` that none of its indicator's
    /// `image_kinds` can measure (contracts 10).
    pub const fn measurable_from(self, kind: ImageKind) -> bool {
        match self {
            Self::PlantHeightCm | Self::CanopyWidthCm => matches!(kind, ImageKind::WholePlant),
            Self::LeafLengthCm
            | Self::LeafAreaCm2
            | Self::LesionDiameterMm
            | Self::LesionCount
            | Self::AffectedAreaPct
            | Self::HoleDiameterMm
            | Self::HoleCount => matches!(kind, ImageKind::LeafCloseup | ImageKind::LeafUnderside),
        }
    }

    /// Height and canopy width form the growth series ([contracts 7], growth).
    pub const fn is_growth(self) -> bool {
        matches!(self, Self::PlantHeightCm | Self::CanopyWidthCm)
    }

    /// Lengths and diameters, whose indicator margin is at least two stored-image pixels.
    pub const fn is_length(self) -> bool {
        matches!(
            self,
            Self::PlantHeightCm
                | Self::CanopyWidthCm
                | Self::LeafLengthCm
                | Self::LesionDiameterMm
                | Self::HoleDiameterMm
        )
    }
}

/// `Measurement` ([contracts 7]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Measurement {
    /// What was measured.
    pub quantity: MeasureQuantity,
    /// The median over the items, or the single value.
    pub value: f64,
    /// Over the items; equal to `value` when `n` = 1. Always sent.
    pub min: f64,
    /// Over the items; equal to `value` when `n` = 1. Always sent.
    pub max: f64,
    /// The number of items; 1 for height and canopy width.
    pub n: u32,
    /// Median camera distance over what was measured, metres.
    pub distance_m: f64,
}
