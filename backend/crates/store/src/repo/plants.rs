//! Plants, the timeline and the growth series ([contracts 9], plants; [database 5], growth
//! series on `observation_growth` ✓).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use domain::{
    authz::{Editor, Membership, Viewer},
    garden::plants::{GrowthQuantity, MeasurementPoint, TimelineEntry, UpdatePlantRequest},
    id::{GardenId, PlantId, ScanId, SpeciesId},
    ports::{NewPlant, PlantRepo, RecentScan, SpeciesConfirmation, StoreError},
    scan::{Plant, PlantStatus, SpeciesCandidate, SpeciesChoice, SpeciesStatus},
    url::Stored,
    wire::{Confidence, Page, PageRequest},
};
use surrealdb::types::{RecordId, SurrealValue, Value};

use crate::{
    SurrealStore,
    error::{RowError, take_failed, thrown, unknown_species},
    ids::{from_opt_rid, from_rid, opt_rid, rid},
    read::{json_of, page as page_of},
    repo::{analysis::enum_str, load::plant_from_json},
    rows::{ScanStatusRow, ts},
    tx::run_tx,
};

/// The statements. Lists page by a cursor on their sort key and the id, never `START`
/// ([database 5], rules for new queries).
pub mod sql {
    use crate::repo::load::sql::plant_fields;

    /// `GET /plants?garden_id=`, by name (index `plant_garden`), with `latest_scan`.
    pub const PLANTS: &str = concat!(
        "SELECT ",
        plant_fields!(),
        " FROM plant WHERE garden = $garden AND deleting_at = NONE
  AND ($after = NONE OR [name, id] > $after) ORDER BY name, id LIMIT $limit;"
    );
    /// The cursor key of a plant.
    pub const PLANT_CURSOR: &str = "SELECT VALUE [name, id] FROM ONLY $plant;";
    /// Whether the plant is live in the garden.
    pub const PLANT_VISIBLE: &str =
        "SELECT VALUE id FROM $plant WHERE garden = $garden AND deleting_at = NONE;";
    /// Every live plant of a garden, for the plan writer's neighbour list.
    pub const GARDEN_PLANTS: &str = concat!(
        "SELECT ",
        plant_fields!(),
        " FROM plant WHERE garden = $garden AND deleting_at = NONE ORDER BY name;"
    );
    /// `POST /plants`, refusing a species the knowledge base doesn't hold (`unknown_species`).
    /// The port passes no time, so `created_at` takes its default.
    pub const CREATE_PLANT: &str = "BEGIN;
IF $species != NONE { IF array::len(SELECT VALUE id FROM $species) = 0 { THROW 'unknown_species' } };
CREATE $plant SET garden = $garden, name = $name, species = $species,
  species_status = IF $species != NONE { 'confirmed' } ELSE { 'unknown' };
COMMIT;";
    /// `PATCH /plants/{id}`.
    pub const UPDATE_PLANT: &str =
        "UPDATE $plant SET name = $name ?? name, status = $status ?? status
WHERE garden = $garden AND deleting_at = NONE RETURN VALUE id;";
    /// `POST /plants/{id}/species`, with the most recent live scan for the follow-up
    /// ([contracts 3], species confirmation), refusing a species the knowledge base doesn't
    /// hold (`unknown_species`). Statements: 0 `BEGIN`, 1 the species check, 2 the update, 3
    /// the scan, 4 `COMMIT`.
    pub const CONFIRM_SPECIES: &str = "BEGIN;
IF $species != NONE { IF array::len(SELECT VALUE id FROM $species) = 0 { THROW 'unknown_species' } };
UPDATE $plant SET species = $species, custom_species_name = $custom, species_status = 'confirmed'
WHERE garden = $garden AND deleting_at = NONE RETURN VALUE id;
SELECT id, status, created_at, working_species FROM scan WHERE plant = $plant AND deleting_at = NONE ORDER BY created_at DESC LIMIT 1;
COMMIT;";
    /// The pipeline's suggestion, unless the species is already confirmed.
    pub const SUGGEST_SPECIES: &str = "UPDATE $plant SET species_candidates = $candidates,
  species = IF species_status = 'confirmed' { species } ELSE { $working },
  species_status = IF species_status = 'confirmed' { species_status } ELSE { $status };";
    /// The plant's status after a diagnosis.
    pub const SET_PLANT_STATUS: &str = "UPDATE $plant SET status = $status;";
    /// `GET /plants/{id}/timeline`, newest first (index `scan_plant`), with the diagnosis, the
    /// thumbnail and the highest-confidence height and width of medium confidence or more.
    pub const TIMELINE: &str = "SELECT id AS scan_id, created_at, status, diagnosis.status AS diagnosis_status,
  IF diagnosis.primary_problem != NONE { { problem_id: diagnosis.primary_problem, name: diagnosis.primary.name } } ELSE { NONE } AS primary,
  (SELECT VALUE id FROM scan_image WHERE scan = $parent.id AND kind = 'whole_plant' LIMIT 1)[0] AS thumbnail_image_id,
  failure.code AS failure_code, plan AS plan_id,
  { plant_height_cm: (SELECT VALUE measured_value FROM observation WHERE scan = $parent.id AND quantity = 'plant_height_cm'
                        AND confidence >= 0.5 AND retracted_at = NONE ORDER BY confidence DESC LIMIT 1)[0],
    canopy_width_cm: (SELECT VALUE measured_value FROM observation WHERE scan = $parent.id AND quantity = 'canopy_width_cm'
                        AND confidence >= 0.5 AND retracted_at = NONE ORDER BY confidence DESC LIMIT 1)[0] } AS measurements
FROM scan WHERE plant = $plant AND deleting_at = NONE AND ($after = NONE OR [created_at, id] < $after)
ORDER BY created_at DESC, scan_id DESC LIMIT $limit;";
    /// The cursor key of a scan in the timeline.
    pub const SCAN_CURSOR: &str = "SELECT VALUE [created_at, id] FROM ONLY $scan;";
    /// The growth series ([database 5] ✓ `observation_growth`, no sort step): current
    /// measurement observations of medium confidence or more, dated by their scan.
    pub const GROWTH_SERIES: &str = "SELECT scan, taken_at, quantity, measured_value, confidence FROM observation
WHERE plant = $plant AND quantity IN $quantities AND retracted_at = NONE AND confidence >= 0.5 ORDER BY taken_at;";
}

/// A species candidate as `plant.species_candidates` stores it.
#[derive(Debug, Clone, SurrealValue)]
pub struct CandidateRow {
    /// The species, `NONE` when the name isn't in the knowledge base.
    pub species: Option<RecordId>,
    /// The name.
    pub name: String,
    /// The score.
    pub score: f64,
    /// `bioclip`, `vlm` or `user`.
    pub source: String,
}

/// The scan [`sql::CONFIRM_SPECIES`] finds.
#[derive(Debug, Clone, SurrealValue)]
pub struct RecentScanRow {
    /// The scan.
    pub id: RecordId,
    /// Its status.
    pub status: ScanStatusRow,
    /// When.
    pub created_at: DateTime<Utc>,
    /// The species its run works with.
    pub working_species: Option<RecordId>,
}

/// One point of [`sql::GROWTH_SERIES`].
#[derive(Debug, Clone, SurrealValue)]
pub struct GrowthRow {
    /// The scan.
    pub scan: RecordId,
    /// The scan's `created_at`.
    pub taken_at: DateTime<Utc>,
    /// The quantity.
    pub quantity: String,
    /// The value.
    pub measured_value: f64,
    /// Its confidence.
    pub confidence: f64,
}

/// One point per scan and quantity, the highest-confidence one, oldest first ([contracts 7],
/// growth).
fn growth_points(rows: Vec<GrowthRow>) -> Result<Vec<MeasurementPoint>, RowError> {
    let mut best: BTreeMap<(DateTime<Utc>, String, String), GrowthRow> = BTreeMap::new();
    for row in rows {
        let key = (
            row.taken_at,
            format!("{:?}", row.scan.key),
            row.quantity.clone(),
        );
        match best.get(&key) {
            Some(b) if b.confidence >= row.confidence => {}
            _ => {
                best.insert(key, row);
            }
        }
    }
    best.into_values()
        .map(|row| {
            Ok(MeasurementPoint {
                scan_id: from_rid(&row.scan)?,
                created_at: ts(row.taken_at),
                quantity: crate::rows::from_json(
                    "quantity",
                    serde_json::Value::String(row.quantity),
                )?,
                value: row.measured_value,
                confidence_band: Confidence::saturating(row.confidence).band(),
            })
        })
        .collect()
}

impl SurrealStore {
    async fn visible_plant(&self, garden: GardenId, plant: PlantId) -> Result<(), StoreError> {
        const WHAT: &str = "plant visible";
        let mut r = run_tx(&self.db, sql::PLANT_VISIBLE, |q| {
            q.bind(("plant", rid(&plant)))
                .bind(("garden", rid(&garden)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let ids: Vec<RecordId> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        if ids.is_empty() {
            Err(StoreError::NotFound)
        } else {
            Ok(())
        }
    }

    async fn plants_of(
        &self,
        statement: &'static str,
        bind: impl Fn(
            surrealdb::method::Query<'_, surrealdb::engine::any::Any>,
        ) -> surrealdb::method::Query<'_, surrealdb::engine::any::Any>
        + Send
        + Sync,
    ) -> Result<Vec<Plant<Stored>>, StoreError> {
        const WHAT: &str = "plants";
        let mut r = run_tx(&self.db, statement, bind)
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let value: Value = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        match json_of(value) {
            serde_json::Value::Array(items) => items
                .into_iter()
                .map(|j| plant_from_json(j, WHAT).map_err(|e| e.into_store(WHAT)))
                .collect(),
            _ => Ok(Vec::new()),
        }
    }

    async fn growth(
        &self,
        plant: PlantId,
        quantities: &[&'static str],
    ) -> Result<Vec<MeasurementPoint>, StoreError> {
        const WHAT: &str = "growth series";
        let quantities: Vec<&'static str> = quantities.to_vec();
        let mut r = run_tx(&self.db, sql::GROWTH_SERIES, |q| {
            q.bind(("plant", rid(&plant)))
                .bind(("quantities", quantities.clone()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<GrowthRow> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        growth_points(rows).map_err(|e| e.into_store(WHAT))
    }
}

#[async_trait::async_trait]
impl PlantRepo for SurrealStore {
    async fn plants(
        &self,
        member: Membership<Viewer>,
        page: PageRequest,
    ) -> Result<Page<Plant<Stored>>, StoreError> {
        let after = self
            .cursor_key(sql::PLANT_CURSOR, "plant", page.cursor.as_ref(), |c| {
                c.parse::<PlantId>().ok().map(|p| rid(&p))
            })
            .await?;
        let limit = page.limit();
        let garden = rid(&member.garden());
        let rows = self
            .plants_of(sql::PLANTS, |q| {
                q.bind(("garden", garden.clone()))
                    .bind(("after", after.clone()))
                    .bind(("limit", i64::from(limit) + 1))
            })
            .await?;
        Ok(page_of(rows, limit, |p| p.id.to_string()))
    }

    async fn plant(
        &self,
        member: Membership<Viewer>,
        plant: PlantId,
    ) -> Result<Plant<Stored>, StoreError> {
        self.visible_plant(member.garden(), plant).await?;
        self.load_plant(plant).await?.ok_or(StoreError::NotFound)
    }

    async fn garden_plants(&self, garden: GardenId) -> Result<Vec<Plant<Stored>>, StoreError> {
        let garden = rid(&garden);
        self.plants_of(sql::GARDEN_PLANTS, |q| q.bind(("garden", garden.clone())))
            .await
    }

    async fn create_plant(
        &self,
        editor: Membership<Editor>,
        plant: NewPlant,
    ) -> Result<Plant<Stored>, StoreError> {
        const WHAT: &str = "create plant";
        run_tx(&self.db, sql::CREATE_PLANT, |q| {
            q.bind(("plant", rid(&plant.id)))
                .bind(("garden", rid(&editor.garden())))
                .bind(("name", plant.name.clone()))
                .bind(("species", opt_rid(plant.species.as_ref())))
        })
        .await
        .map_err(|f| match f.thrown() {
            Some(thrown::UNKNOWN_SPECIES) => unknown_species("species_id"),
            _ => f.into_store(WHAT),
        })?;
        self.load_plant(plant.id).await?.ok_or(StoreError::NotFound)
    }

    async fn update_plant(
        &self,
        editor: Membership<Editor>,
        plant: PlantId,
        patch: UpdatePlantRequest,
    ) -> Result<Plant<Stored>, StoreError> {
        const WHAT: &str = "update plant";
        let status = patch
            .status
            .as_ref()
            .map(|s| enum_str("status", s))
            .transpose()
            .map_err(|e| e.into_store(WHAT))?;
        let mut r = run_tx(&self.db, sql::UPDATE_PLANT, |q| {
            q.bind(("plant", rid(&plant)))
                .bind(("garden", rid(&editor.garden())))
                .bind(("name", patch.name.clone()))
                .bind(("status", status.clone()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let ids: Vec<RecordId> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        if ids.is_empty() {
            return Err(StoreError::NotFound);
        }
        self.load_plant(plant).await?.ok_or(StoreError::NotFound)
    }

    async fn confirm_species(
        &self,
        editor: Membership<Editor>,
        plant: PlantId,
        choice: SpeciesChoice,
    ) -> Result<SpeciesConfirmation, StoreError> {
        const WHAT: &str = "confirm species";
        let (species, custom) = match &choice {
            SpeciesChoice::Known(s) => (Some(rid(s)), None),
            SpeciesChoice::Custom(name) => (None, Some(name.clone())),
        };
        let mut r = run_tx(&self.db, sql::CONFIRM_SPECIES, |q| {
            q.bind(("plant", rid(&plant)))
                .bind(("garden", rid(&editor.garden())))
                .bind(("species", species.clone()))
                .bind(("custom", custom.clone()))
        })
        .await
        .map_err(|f| match f.thrown() {
            Some(thrown::UNKNOWN_SPECIES) => unknown_species("species_id"),
            _ => f.into_store(WHAT),
        })?;
        let updated: Vec<RecordId> = r.take(2).map_err(|e| take_failed(WHAT, e))?;
        if updated.is_empty() {
            return Err(StoreError::NotFound);
        }
        let scans: Vec<RecentScanRow> = r.take(3).map_err(|e| take_failed(WHAT, e))?;
        let recent_scan = scans
            .into_iter()
            .next()
            .map(|s| {
                Ok::<_, RowError>(RecentScan {
                    scan: from_rid::<ScanId>(&s.id)?,
                    status: s.status.into(),
                    created_at: ts(s.created_at),
                    working_species: from_opt_rid::<SpeciesId>(s.working_species.as_ref())?,
                })
            })
            .transpose()
            .map_err(|e| e.into_store(WHAT))?;
        Ok(SpeciesConfirmation {
            plant: self.load_plant(plant).await?.ok_or(StoreError::NotFound)?,
            recent_scan,
        })
    }

    async fn suggest_species(
        &self,
        plant: PlantId,
        candidates: Vec<SpeciesCandidate<Stored>>,
        working: Option<SpeciesId>,
        status: SpeciesStatus,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "suggest species";
        let rows = candidates
            .iter()
            .map(|c| {
                Ok(CandidateRow {
                    species: opt_rid(c.species_id.as_ref()),
                    name: c.name.clone(),
                    score: c.score.get(),
                    source: enum_str("source", &c.source)?,
                })
            })
            .collect::<Result<Vec<_>, RowError>>()
            .map_err(|e| e.into_store(WHAT))?;
        let status = enum_str("species_status", &status).map_err(|e| e.into_store(WHAT))?;
        run_tx(&self.db, sql::SUGGEST_SPECIES, |q| {
            q.bind(("plant", rid(&plant)))
                .bind(("candidates", rows.clone()))
                .bind(("working", opt_rid(working.as_ref())))
                .bind(("status", status.clone()))
        })
        .await
        .map(drop)
        .map_err(|f| f.into_store(WHAT))
    }

    async fn set_plant_status(
        &self,
        plant: PlantId,
        status: PlantStatus,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "set plant status";
        let status = enum_str("status", &status).map_err(|e| e.into_store(WHAT))?;
        run_tx(&self.db, sql::SET_PLANT_STATUS, |q| {
            q.bind(("plant", rid(&plant)))
                .bind(("status", status.clone()))
        })
        .await
        .map(drop)
        .map_err(|f| f.into_store(WHAT))
    }

    async fn timeline(
        &self,
        member: Membership<Viewer>,
        plant: PlantId,
        page: PageRequest,
    ) -> Result<Page<TimelineEntry<Stored>>, StoreError> {
        const WHAT: &str = "timeline";
        self.visible_plant(member.garden(), plant).await?;
        let after = self
            .cursor_key(sql::SCAN_CURSOR, "scan", page.cursor.as_ref(), |c| {
                c.parse::<ScanId>().ok().map(|s| rid(&s))
            })
            .await?;
        let limit = page.limit();
        let mut r = run_tx(&self.db, sql::TIMELINE, |q| {
            q.bind(("plant", rid(&plant)))
                .bind(("after", after.clone()))
                .bind(("limit", i64::from(limit) + 1))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let value: Value = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        let items = match json_of(value) {
            serde_json::Value::Array(items) => items,
            _ => Vec::new(),
        };
        let rows = items
            .into_iter()
            .map(|mut j| {
                // No measurement at all: the field is absent, not `{}`.
                if j.get("measurements")
                    .and_then(serde_json::Value::as_object)
                    .is_some_and(serde_json::Map::is_empty)
                    && let Some(m) = j.as_object_mut()
                {
                    m.remove("measurements");
                }
                serde_json::from_value::<TimelineEntry<Stored>>(j)
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| {
                RowError::Json {
                    field: WHAT,
                    source,
                }
                .into_store(WHAT)
            })?;
        Ok(page_of(rows, limit, |t| t.scan_id.to_string()))
    }

    async fn growth_series(
        &self,
        member: Membership<Viewer>,
        plant: PlantId,
        quantity: Option<GrowthQuantity>,
        page: PageRequest,
    ) -> Result<Page<MeasurementPoint>, StoreError> {
        self.visible_plant(member.garden(), plant).await?;
        let quantities: &[&'static str] = match quantity {
            Some(GrowthQuantity::PlantHeightCm) => &["plant_height_cm"],
            Some(GrowthQuantity::CanopyWidthCm) => &["canopy_width_cm"],
            None => &["plant_height_cm", "canopy_width_cm"],
        };
        let points = self.growth(plant, quantities).await?;
        // The series is small (one point per scan): it is paged in memory by the last point's
        // scan, oldest first.
        let start = page.cursor.as_ref().map_or(0, |c| {
            points
                .iter()
                .rposition(|p| p.scan_id.to_string() == c.as_str())
                .map_or(0, |i| i + 1)
        });
        let rest: Vec<MeasurementPoint> = points.into_iter().skip(start).collect();
        Ok(page_of(rest, page.limit(), |p| p.scan_id.to_string()))
    }

    async fn heights(&self, plant: PlantId) -> Result<Vec<MeasurementPoint>, StoreError> {
        self.growth(plant, &["plant_height_cm"]).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_point_per_scan_and_quantity_keeps_the_most_confident() {
        let at = DateTime::<Utc>::from_timestamp(1_760_000_000, 0).unwrap();
        let scan = RecordId::new("scan", "01JAX3K9T6R2B8V0QZ4M7D1E5S");
        let row = |v: f64, c: f64| GrowthRow {
            scan: scan.clone(),
            taken_at: at,
            quantity: "plant_height_cm".into(),
            measured_value: v,
            confidence: c,
        };
        let points = growth_points(vec![row(40.0, 0.65), row(42.0, 0.85)]).unwrap();
        assert_eq!(points.len(), 1);
        assert!((points[0].value - 42.0).abs() < f64::EPSILON);
    }
}
