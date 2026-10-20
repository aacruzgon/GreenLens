//! The garden map ([contracts 9], garden map; [database 4], garden map versions ✓).

use chrono::{DateTime, Utc};
use domain::{
    authz::{Editor, Membership, Viewer},
    error::DomainError,
    garden::map::{Bed, GardenMap, Pin, PinPlant},
    id::{GardenId, PlantId},
    ports::{Conflict, MapRepo, MapWrite, StoreError},
    wire::UserRef,
};
use surrealdb::types::{RecordId, SurrealValue};

use crate::{
    SurrealStore,
    error::{RowError, take_failed, thrown, unknown_species},
    ids::{from_opt_rid, from_rid, opt_rid, rid},
    rows::{dt, from_json, to_json, ts, u32_of},
    tx::run_tx,
};

/// The statements.
pub mod sql {
    /// `PUT /gardens/{id}/map` in one transaction ([database 4] ✓): the `base_version`
    /// check (`THROW` and nothing changes), unknown plants refused, new plants' species the
    /// knowledge base doesn't hold refused (`unknown_species`), pins of plants being
    /// deleted dropped, new plants created, the version incremented, the map written, older
    /// maps deleted, and every plant's `position` set or unset. Two concurrent uploads conflict
    /// on the garden row; the retry then fails the version check.
    pub const PUT_MAP: &str = "BEGIN;
LET $g = (SELECT map_version FROM ONLY $garden WHERE deleting_at = NONE);
IF $g = NONE { THROW 'not_found' };
IF $g.map_version != $base { THROW 'stale_map' };
IF array::len(array::filter($existing, |$p| array::len(SELECT VALUE id FROM $p WHERE garden = $garden) = 0)) > 0 { THROW 'unknown_plants' };
IF array::len(array::filter(array::filter($new_plants, |$n| $n.species != NONE), |$n| array::len(SELECT VALUE id FROM $n.species) = 0)) > 0 { THROW 'unknown_species' };
LET $dropped = (SELECT VALUE id FROM plant WHERE id IN $existing AND deleting_at != NONE);
FOR $n IN $new_plants {
  CREATE $n.id SET garden = $garden, name = $n.name, species = $n.species,
    species_status = IF $n.species != NONE { 'confirmed' } ELSE { 'unknown' }, created_at = $now;
};
LET $version = $g.map_version + 1;
LET $pins = array::filter($pins_in, |$p| !($p.plant IN $dropped));
UPDATE $garden SET map_version = $version;
CREATE garden_map SET garden = $garden, version = $version, beds = $beds, pins = $pins,
  north_deg = $north, created_by = $user, created_at = $now;
DELETE garden_map WHERE garden = $garden AND version < $version;
UPDATE plant SET position = NONE WHERE garden = $garden AND position != NONE AND !(id IN $pins.plant);
FOR $p IN $pins { UPDATE $p.plant SET position = { map_version: $version, x_m: $p.x_m, y_m: $p.y_m }; };
COMMIT;";

    /// After `unknown_plants`: every pinned plant that isn't the garden's, listed at once.
    pub const UNKNOWN_PLANTS: &str = "RETURN array::filter($existing, |$p| array::len(SELECT VALUE id FROM $p WHERE garden = $garden) = 0);";
    /// After `stale_map`: the current version for `409`'s `current_map_version`.
    pub const MAP_VERSION: &str = "SELECT VALUE map_version FROM ONLY $garden;";

    /// The current map ([database 5] ✓ `garden_map_version`), leaving out pins of plants being
    /// deleted.
    pub const CURRENT_MAP: &str = "SELECT garden, version, beds, north_deg, created_by, created_by.display_name AS creator_name,
       created_at, array::filter(pins, |$p| $p.plant.deleting_at = NONE) AS pins
FROM garden_map WHERE garden = $garden AND version = $garden.map_version AND $garden.deleting_at = NONE;";
}

/// One pin as stored (`garden_map.pins[*]`).
#[derive(Debug, Clone, SurrealValue)]
pub struct PinRow {
    /// The plant.
    pub plant: RecordId,
    /// Metres on the ground plane.
    pub x_m: f64,
    /// See `x_m`.
    pub y_m: f64,
    /// Shown on the map; no rule uses it.
    pub canopy_radius_m: Option<f64>,
    /// The bed.
    pub bed_id: Option<String>,
}

/// A plant a pin creates.
#[derive(Debug, Clone, SurrealValue)]
pub struct NewPinPlantRow {
    /// `plant:<ulid>`.
    pub id: RecordId,
    /// Its name.
    pub name: String,
    /// Its species, confirmed when given.
    pub species: Option<RecordId>,
}

/// A `garden_map` row as [`sql::CURRENT_MAP`] reads it.
#[derive(Debug, Clone, SurrealValue)]
pub struct MapRow {
    /// Its garden.
    pub garden: RecordId,
    /// Its version.
    pub version: i64,
    /// `Bed[]`.
    pub beds: Vec<serde_json::Value>,
    /// The pins of live plants.
    pub pins: Vec<PinRow>,
    /// Compass bearing.
    pub north_deg: Option<f64>,
    /// Who mapped it; `NONE` once deleted.
    pub created_by: Option<RecordId>,
    /// Their name.
    pub creator_name: Option<String>,
    /// When.
    pub created_at: DateTime<Utc>,
}

impl TryFrom<MapRow> for GardenMap {
    type Error = RowError;

    fn try_from(row: MapRow) -> Result<Self, RowError> {
        let created_by = match (from_opt_rid(row.created_by.as_ref())?, row.creator_name) {
            (Some(user_id), Some(display_name)) => Some(UserRef {
                user_id,
                display_name,
            }),
            _ => None,
        };
        Ok(Self {
            garden_id: from_rid(&row.garden)?,
            version: u32_of("version", row.version)?,
            beds: row
                .beds
                .into_iter()
                .map(|b| from_json::<Bed>("beds", b))
                .collect::<Result<_, _>>()?,
            pins: row
                .pins
                .into_iter()
                .map(|p| {
                    Ok(Pin {
                        plant_id: from_rid(&p.plant)?,
                        x_m: p.x_m,
                        y_m: p.y_m,
                        canopy_radius_m: p.canopy_radius_m,
                        bed_id: p.bed_id,
                    })
                })
                .collect::<Result<_, RowError>>()?,
            north_deg: row.north_deg,
            created_by,
            created_at: ts(row.created_at),
        })
    }
}

/// The binds of [`sql::PUT_MAP`] built from a checked upload.
struct MapBinds {
    existing: Vec<RecordId>,
    new_plants: Vec<NewPinPlantRow>,
    pins: Vec<PinRow>,
    beds: Vec<serde_json::Value>,
}

impl MapBinds {
    fn new(write: &MapWrite) -> Result<Self, RowError> {
        let mut new_ids = write.new_plant_ids.iter();
        let mut existing = Vec::new();
        let mut new_plants = Vec::new();
        let mut pins = Vec::new();
        for pin in &write.upload.pins {
            let plant = match &pin.plant {
                PinPlant::Existing(id) => {
                    existing.push(rid(id));
                    rid(id)
                }
                PinPlant::New(input) => {
                    let id: &PlantId = new_ids.next().ok_or(RowError::BadField {
                        field: "new_plant_ids",
                        problem: "fewer ids than new plants".to_owned(),
                    })?;
                    new_plants.push(NewPinPlantRow {
                        id: rid(id),
                        name: input.name.clone(),
                        species: opt_rid(input.species_id.as_ref()),
                    });
                    rid(id)
                }
            };
            pins.push(PinRow {
                plant,
                x_m: pin.x_m,
                y_m: pin.y_m,
                canopy_radius_m: pin.canopy_radius_m,
                bed_id: pin.bed_id.clone(),
            });
        }
        Ok(Self {
            existing,
            new_plants,
            pins,
            beds: write
                .upload
                .beds
                .iter()
                .map(|b| to_json("beds", b))
                .collect::<Result<_, _>>()?,
        })
    }
}

#[async_trait::async_trait]
impl MapRepo for SurrealStore {
    async fn map(&self, member: Membership<Viewer>) -> Result<Option<GardenMap>, StoreError> {
        self.current_map(member.garden()).await
    }

    async fn put_map(
        &self,
        editor: Membership<Editor>,
        write: MapWrite,
    ) -> Result<GardenMap, StoreError> {
        const WHAT: &str = "put map";
        let garden = rid(&editor.garden());
        let binds = MapBinds::new(&write).map_err(|e| e.into_store(WHAT))?;
        let result = run_tx(&self.db, sql::PUT_MAP, |q| {
            q.bind(("garden", garden.clone()))
                .bind(("user", rid(&editor.user())))
                .bind(("base", i64::from(write.upload.base_version)))
                .bind(("existing", binds.existing.clone()))
                .bind(("new_plants", binds.new_plants.clone()))
                .bind(("pins_in", binds.pins.clone()))
                .bind(("beds", binds.beds.clone()))
                .bind(("north", write.upload.north_deg))
                .bind(("now", dt(write.created_at)))
        })
        .await;
        match result {
            Ok(_) => {}
            Err(failure) if failure.thrown() == Some(thrown::STALE_MAP) => {
                let mut r = run_tx(&self.db, sql::MAP_VERSION, |q| {
                    q.bind(("garden", garden.clone()))
                })
                .await
                .map_err(|f| f.into_store(WHAT))?;
                let version: Option<i64> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
                return Err(StoreError::Conflict(Conflict::StaleMap {
                    current_version: u32_of("map_version", version.unwrap_or(0))
                        .map_err(|e| e.into_store(WHAT))?,
                }));
            }
            Err(failure) if failure.thrown() == Some(thrown::UNKNOWN_PLANTS) => {
                let mut r = run_tx(&self.db, sql::UNKNOWN_PLANTS, |q| {
                    q.bind(("garden", garden.clone()))
                        .bind(("existing", binds.existing.clone()))
                })
                .await
                .map_err(|f| f.into_store(WHAT))?;
                let unknown: Vec<RecordId> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
                return Err(StoreError::Rejected(DomainError::UnknownPlants {
                    plant_ids: unknown
                        .iter()
                        .map(from_rid)
                        .collect::<Result<_, _>>()
                        .map_err(|e: RowError| e.into_store(WHAT))?,
                }));
            }
            Err(failure) if failure.thrown() == Some(thrown::UNKNOWN_SPECIES) => {
                return Err(unknown_species("pins"));
            }
            Err(failure) => return Err(failure.into_store(WHAT)),
        }
        self.current_map(editor.garden())
            .await?
            .ok_or(StoreError::NotFound)
    }

    async fn current_map(&self, garden: GardenId) -> Result<Option<GardenMap>, StoreError> {
        const WHAT: &str = "current map";
        let mut results = run_tx(&self.db, sql::CURRENT_MAP, |q| {
            q.bind(("garden", rid(&garden)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<MapRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .next()
            .map(GardenMap::try_from)
            .transpose()
            .map_err(|e| e.into_store(WHAT))
    }
}
