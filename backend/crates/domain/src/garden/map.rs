//! The garden map ([contracts 9], garden map; architecture 6.8): uploads, validation, the
//! stored map, neighbours and crowding geometry.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::DomainError,
    id::{GardenId, PlantId},
    rules::limits::{CROWDING_FACTOR, NEIGHBOUR_RADIUS_M},
    scan::NewPlantInput,
    wire::{Timestamp, UserRef},
};

/// At most 20 beds.
pub const MAX_BEDS: usize = 20;
/// At most 200 pins.
pub const MAX_PINS: usize = 200;
/// A bed polygon has 3–50 points.
pub const BED_POINTS: std::ops::RangeInclusive<usize> = 3..=50;
/// Pins within 200 m of the origin.
pub const MAX_PIN_DISTANCE_M: f64 = 200.0;

/// `GardenMapInput.beds[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BedInput {
    /// The app's own id, unique within the map.
    pub id: String,
    /// Its name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub name: Option<String>,
    /// Ground plane, metres; 3–50 points, a simple polygon.
    pub polygon_m: Vec<[f64; 2]>,
}

/// `GardenMapInput.pins[]`: exactly one of `plant_id` and `new_plant`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PinInput {
    /// An existing plant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_id: Option<PlantId>,
    /// A plant created with the upload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub new_plant: Option<NewPlantInput>,
    /// Within 200 m of the origin.
    pub x_m: f64,
    /// Within 200 m of the origin.
    pub y_m: f64,
    /// From the reconstructed mesh; shown, not used by any rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub canopy_radius_m: Option<f64>,
    /// The bed it stands in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub bed_id: Option<String>,
}

/// `PUT /gardens/{id}/map` ([contracts 9]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GardenMapInput {
    /// The version current when the mapping session started; 0 when the garden had none.
    pub base_version: u32,
    /// At most 20.
    pub beds: Vec<BedInput>,
    /// At most 200; a plant appears at most once.
    pub pins: Vec<PinInput>,
    /// 0–360, clockwise from the map's +y axis to north; only with a compass accuracy of 20° or
    /// better.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub north_deg: Option<f64>,
}

/// `GardenMap.beds[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Bed {
    /// The app's id.
    pub id: String,
    /// Its name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub name: Option<String>,
    /// Ground plane, metres.
    pub polygon_m: Vec<[f64; 2]>,
    /// Computed from the polygon.
    pub area_m2: f64,
}

/// `GardenMap.pins[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Pin {
    /// The plant.
    pub plant_id: PlantId,
    /// Metres.
    pub x_m: f64,
    /// Metres.
    pub y_m: f64,
    /// Shown only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub canopy_radius_m: Option<f64>,
    /// The bed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub bed_id: Option<String>,
}

/// `GardenMap` ([contracts 9]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct GardenMap {
    /// The garden.
    pub garden_id: GardenId,
    /// The version; only the current one is kept.
    pub version: u32,
    /// Beds.
    pub beds: Vec<Bed>,
    /// Pins, leaving out plants being deleted.
    pub pins: Vec<Pin>,
    /// North, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub north_deg: Option<f64>,
    /// Absent once that user is deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub created_by: Option<UserRef>,
    /// When it was uploaded.
    pub created_at: Timestamp,
}

/// `200` of `GET /gardens/{id}/map`: `{ map: GardenMap | null }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MapResponse {
    /// `null` when the garden has no map.
    #[schema(required = true)]
    pub map: Option<GardenMap>,
}

/// A pin's plant, once "exactly one of" is checked.
#[derive(Debug, Clone, PartialEq)]
pub enum PinPlant {
    /// An existing plant; the store checks it belongs to the garden.
    Existing(PlantId),
    /// A plant created in the upload's transaction.
    New(NewPlantInput),
}

/// A checked pin.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidPin {
    /// Its plant.
    pub plant: PinPlant,
    /// Metres.
    pub x_m: f64,
    /// Metres.
    pub y_m: f64,
    /// Shown only.
    pub canopy_radius_m: Option<f64>,
    /// The bed.
    pub bed_id: Option<String>,
}

/// A checked upload, ready for the map transaction ([database 4], garden map versions).
#[derive(Debug, Clone, PartialEq)]
pub struct MapUpload {
    /// Checked against `garden.map_version` in the transaction.
    pub base_version: u32,
    /// Beds with their areas.
    pub beds: Vec<Bed>,
    /// Pins.
    pub pins: Vec<ValidPin>,
    /// North.
    pub north_deg: Option<f64>,
}

impl GardenMapInput {
    /// Checks every rule that needs no database: counts, polygons, distances, one pin per plant,
    /// exactly one of `plant_id` and `new_plant`, known bed ids, the north range.
    pub fn validate(self) -> Result<MapUpload, DomainError> {
        if self.beds.len() > MAX_BEDS {
            return Err(DomainError::invalid("beds", "at most 20 beds"));
        }
        if self.pins.len() > MAX_PINS {
            return Err(DomainError::invalid("pins", "at most 200 pins"));
        }
        if let Some(n) = self.north_deg
            && !(0.0..360.0).contains(&n)
        {
            return Err(DomainError::invalid("north_deg", "0 to 360"));
        }
        let mut bed_ids = BTreeSet::new();
        let mut beds = Vec::with_capacity(self.beds.len());
        for (i, bed) in self.beds.into_iter().enumerate() {
            if !bed_ids.insert(bed.id.clone()) {
                return Err(DomainError::invalid(
                    format!("beds[{i}].id"),
                    "duplicate bed id",
                ));
            }
            if !BED_POINTS.contains(&bed.polygon_m.len()) {
                return Err(DomainError::invalid(
                    format!("beds[{i}].polygon_m"),
                    "3 to 50 points",
                ));
            }
            if bed.polygon_m.iter().flatten().any(|v| !v.is_finite()) || !is_simple(&bed.polygon_m)
            {
                return Err(DomainError::invalid(
                    format!("beds[{i}].polygon_m"),
                    "not a simple polygon",
                ));
            }
            beds.push(Bed {
                area_m2: polygon_area(&bed.polygon_m),
                id: bed.id,
                name: bed.name,
                polygon_m: bed.polygon_m,
            });
        }
        let mut seen = BTreeSet::new();
        let mut pins = Vec::with_capacity(self.pins.len());
        for (i, pin) in self.pins.into_iter().enumerate() {
            let field = |f: &str| format!("pins[{i}].{f}");
            if !(pin.x_m.is_finite() && pin.y_m.is_finite())
                || pin.x_m.hypot(pin.y_m) > MAX_PIN_DISTANCE_M
            {
                return Err(DomainError::invalid(
                    field("x_m"),
                    "within 200 m of the origin",
                ));
            }
            if let Some(bed) = &pin.bed_id
                && !bed_ids.contains(bed)
            {
                return Err(DomainError::invalid(field("bed_id"), "unknown bed"));
            }
            let plant = match (pin.plant_id, pin.new_plant) {
                (Some(id), None) => {
                    if !seen.insert(id) {
                        return Err(DomainError::invalid(
                            field("plant_id"),
                            "plant pinned twice",
                        ));
                    }
                    PinPlant::Existing(id)
                }
                (None, Some(new)) => PinPlant::New(new),
                _ => {
                    return Err(DomainError::invalid(
                        field("plant_id"),
                        "exactly one of plant_id and new_plant",
                    ));
                }
            };
            pins.push(ValidPin {
                plant,
                x_m: pin.x_m,
                y_m: pin.y_m,
                canopy_radius_m: pin.canopy_radius_m,
                bed_id: pin.bed_id,
            });
        }
        Ok(MapUpload {
            base_version: self.base_version,
            beds,
            pins,
            north_deg: self.north_deg,
        })
    }
}

/// Shoelace area, m².
pub fn polygon_area(points: &[[f64; 2]]) -> f64 {
    let n = points.len();
    let twice: f64 = (0..n)
        .map(|i| {
            let [x1, y1] = points[i];
            let [x2, y2] = points[(i + 1) % n];
            x1 * y2 - x2 * y1
        })
        .sum();
    (twice / 2.0).abs()
}

/// No two non-adjacent edges intersect and no edge has zero length.
pub fn is_simple(points: &[[f64; 2]]) -> bool {
    let n = points.len();
    let edge = |i: usize| (points[i], points[(i + 1) % n]);
    for i in 0..n {
        let (a, b) = edge(i);
        if a[0].to_bits() == b[0].to_bits() && a[1].to_bits() == b[1].to_bits() {
            return false;
        }
        for j in (i + 1)..n {
            let adjacent = j == i + 1 || (i == 0 && j == n - 1);
            if !adjacent && segments_intersect(a, b, edge(j).0, edge(j).1) {
                return false;
            }
        }
    }
    true
}

fn segments_intersect(p1: [f64; 2], p2: [f64; 2], p3: [f64; 2], p4: [f64; 2]) -> bool {
    fn orient(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    }
    fn on_segment(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> bool {
        c[0] >= a[0].min(b[0])
            && c[0] <= a[0].max(b[0])
            && c[1] >= a[1].min(b[1])
            && c[1] <= a[1].max(b[1])
    }
    let d1 = orient(p3, p4, p1);
    let d2 = orient(p3, p4, p2);
    let d3 = orient(p1, p2, p3);
    let d4 = orient(p1, p2, p4);
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    (d1 == 0.0 && on_segment(p3, p4, p1))
        || (d2 == 0.0 && on_segment(p3, p4, p2))
        || (d3 == 0.0 && on_segment(p1, p2, p3))
        || (d4 == 0.0 && on_segment(p1, p2, p4))
}

/// Each pinned plant's neighbours: the plants within [`NEIGHBOUR_RADIUS_M`], centre to centre.
pub fn neighbours(pins: &[Pin]) -> BTreeMap<PlantId, Vec<PlantId>> {
    pins.iter()
        .map(|p| {
            let near = pins
                .iter()
                .filter(|q| q.plant_id != p.plant_id && distance_m(p, q) <= NEIGHBOUR_RADIUS_M)
                .map(|q| q.plant_id)
                .collect();
            (p.plant_id, near)
        })
        .collect()
}

/// Centre-to-centre distance, metres.
pub fn distance_m(a: &Pin, b: &Pin) -> f64 {
    (a.x_m - b.x_m).hypot(a.y_m - b.y_m)
}

/// The crowding rule's geometric half ([contracts 9], crowding): the nearest other pin is
/// closer than 0.75 × the species' spacing. Returns that distance in metres when crowded.
pub fn crowded(plant: PlantId, pins: &[Pin], spacing_cm: f64) -> Option<f64> {
    let me = pins.iter().find(|p| p.plant_id == plant)?;
    let nearest = pins
        .iter()
        .filter(|q| q.plant_id != plant)
        .map(|q| distance_m(me, q))
        .min_by(f64::total_cmp)?;
    (nearest < CROWDING_FACTOR * spacing_cm / 100.0).then_some(nearest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(id: &str, x: f64, y: f64) -> Pin {
        Pin {
            plant_id: id.parse().unwrap(),
            x_m: x,
            y_m: y,
            canopy_radius_m: None,
            bed_id: None,
        }
    }

    #[test]
    fn polygons() {
        let square = [[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [0.0, 1.0]];
        assert!(is_simple(&square));
        assert!((polygon_area(&square) - 2.0).abs() < 1e-9);
        let bowtie = [[0.0, 0.0], [2.0, 1.0], [2.0, 0.0], [0.0, 1.0]];
        assert!(!is_simple(&bowtie));
    }

    #[test]
    fn validation() {
        let plant: PlantId = "01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().unwrap();
        let pin_input = |plant_id| PinInput {
            plant_id,
            new_plant: None,
            x_m: 1.0,
            y_m: 1.0,
            canopy_radius_m: None,
            bed_id: None,
        };
        let input = GardenMapInput {
            base_version: 0,
            beds: vec![],
            pins: vec![pin_input(Some(plant)), pin_input(Some(plant))],
            north_deg: None,
        };
        assert!(input.validate().is_err());
        let input = GardenMapInput {
            base_version: 0,
            beds: vec![],
            pins: vec![pin_input(None)],
            north_deg: Some(360.0),
        };
        assert!(input.validate().is_err());
    }

    #[test]
    fn neighbours_and_crowding() {
        let first = "01JAX3K9T6R2B8V0QZ4M7D1E5A";
        let close = "01JAX3K9T6R2B8V0QZ4M7D1E5B";
        let far = "01JAX3K9T6R2B8V0QZ4M7D1E5C";
        let pins = [
            pin(first, 0.0, 0.0),
            pin(close, 0.3, 0.0),
            pin(far, 3.0, 0.0),
        ];
        let near = neighbours(&pins);
        assert_eq!(near[&first.parse().unwrap()], vec![close.parse().unwrap()]);
        assert_eq!(near[&far.parse().unwrap()], Vec::<PlantId>::new());
        let distance = crowded(first.parse().unwrap(), &pins, 60.0).unwrap();
        assert!((distance - 0.3).abs() < 1e-9);
        assert!(crowded(far.parse().unwrap(), &pins, 60.0).is_none());
    }
}
