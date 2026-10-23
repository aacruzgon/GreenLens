//! Loaders shared by several repositories: a scan with its images, checks and open question; a
//! plant with its latest scan; observations; questions; a diagnosis with its soil observations;
//! a plan with its open question and task ids; a garden's context. Each projects the stored
//! record into its contract shape and decodes it with [`crate::read`].

use domain::{
    garden::gardens::TimeZone,
    id::{DiagnosisId, GardenId, ObservationId, PlanId, PlantId, QuestionId, ScanId},
    knowledge::SoilProfile,
    ports::{GardenContext, StoreError},
    records::{
        diagnosis::Diagnosis,
        observation::Observation,
        plan::ActionPlan,
        weather::{Coordinates, WeatherCell},
    },
    scan::{GuidedCheck, Plant, Question, Scan, ScanImage},
    url::Stored,
};
use serde_json::Value as Json;
use surrealdb::types::{RecordId, SurrealValue, Value};

use crate::{
    SurrealStore,
    error::{RowError, take_failed},
    ids::rid,
    read::json_of,
    rows::{LocationPrecisionRow, from_json, u32_of},
    tx::run_tx,
};

/// The statements.
pub mod sql {
    use crate::fragments::observation_fields;

    /// The projection of a `scan` into `Scan` without its images, checks and open question.
    macro_rules! scan_fields {
        () => {
            "id, garden AS garden_id, plant AS plant_id, status, run, created_at,
  IF created_by != NONE { { user_id: created_by, display_name: created_by.display_name } } ELSE { NONE } AS created_by,
  submitted_at, diagnosis AS diagnosis_id, plan AS plan_id, failure, last_event_seq, [] AS images, [] AS guided_checks"
        };
    }

    /// The projection of a `scan_image` into `ScanImage` (the depth map itself is never
    /// described beyond its size and gravity, contracts 3).
    macro_rules! image_fields {
        () => {
            "id, kind, answers_question AS answers_question_id, width, height, uploaded_at,
  IF depth_meta != NONE { { width: depth_meta.width, height: depth_meta.height, has_gravity: depth_meta.gravity != NONE } } ELSE { NONE } AS depth"
        };
    }

    /// The projection of a `question` into `Question`. Resolved questions have no wire
    /// status (`QuestionStatus` is `open | answering`); they are read as `open` and are never
    /// the scan's `open_question` (an open-question note in store.md).
    macro_rules! question_fields {
        () => {
            "id, run, kind, text, options, photo_kind, photo_hint, candidates, asked_at, continue_at,
  IF indicator != NONE { string::split(indicator, '.')[1] } ELSE { NONE } AS indicator_id,
  IF status = 'answering' { 'answering' } ELSE { 'open' } AS status"
        };
    }

    /// The projection of a `plant` into `Plant`, with its latest live scan and that scan's
    /// `whole_plant` shot (computed when read, database 3). A `species` link to a record the
    /// knowledge base doesn't hold (a stub worker's pick before any seed is loaded, or a species
    /// a later seed dropped) reads as no species: `option<record<species>>` doesn't check that
    /// the record exists, and `species.scientific_name` is then `NONE`.
    macro_rules! plant_fields {
        () => {
            "id, garden AS garden_id, name,
  IF species.scientific_name != NONE { { id: species, scientific_name: species.scientific_name, common_name: species.common_names[0] ?? species.scientific_name } } ELSE { NONE } AS species,
  custom_species_name, species_status, species_candidates, status, position, created_at,
  (SELECT id AS scan_id, created_at, status,
          (SELECT VALUE id FROM scan_image WHERE scan = $parent.id AND kind = 'whole_plant' LIMIT 1)[0] AS thumbnail_image_id
     FROM scan WHERE plant = $parent.id AND deleting_at = NONE ORDER BY created_at DESC LIMIT 1)[0] AS latest_scan"
        };
    }
    pub(crate) use plant_fields;

    /// A scan and its parts. Statements: 0 the scan, 1 its images, 2 its checks, 3 the
    /// checks' result observations, 4 the open question.
    pub const SCAN_PARTS: &str = concat!(
        "SELECT ",
        scan_fields!(),
        ", open_question FROM ONLY $scan;\n",
        "SELECT ",
        image_fields!(),
        " FROM scan_image WHERE scan = $scan ORDER BY uploaded_at;\n",
        "SELECT image AS image_id, kind, status, result FROM guided_check WHERE scan = $scan ORDER BY created_at;\n",
        "SELECT ",
        observation_fields!(),
        " FROM observation WHERE id IN (SELECT VALUE result FROM guided_check WHERE scan = $scan AND result != NONE);\n",
        "SELECT ",
        question_fields!(),
        " FROM question WHERE id = (SELECT VALUE open_question FROM ONLY $scan);"
    );

    /// A plant.
    pub const PLANT: &str = concat!("SELECT ", plant_fields!(), " FROM ONLY $plant;");

    /// Observations by id.
    pub const OBSERVATIONS_BY_ID: &str = concat!(
        "SELECT ",
        observation_fields!(),
        " FROM observation WHERE id IN $ids;"
    );

    /// Questions of a run.
    pub const RUN_QUESTIONS: &str = concat!(
        "SELECT ",
        question_fields!(),
        " FROM question WHERE scan = $scan AND run = $run ORDER BY asked_at;"
    );

    /// A question by id.
    pub const QUESTION: &str = concat!("SELECT ", question_fields!(), " FROM ONLY $question;");

    /// A diagnosis without its soil observations (their ids come separately).
    pub const DIAGNOSIS: &str = "SELECT id, scan AS scan_id, run, supersedes, status, low_evidence, primary, candidates,
       organisms, [] AS soil, risks, evidence_basis, citations, model_versions, created_at, soil AS soil_ids
FROM ONLY $diagnosis;";

    /// A plan, with its task ids computed (index `task_plan`).
    pub const PLAN: &str =
        "SELECT id, scan AS scan_id, plant AS plant_id, version, supersedes, provisional,
       diagnosis AS diagnosis_id, open_question AS open_question_id, constraints, summary, steps,
       watch_for, citations, farm_records, llm_model, prompt_version, created_at,
       (SELECT VALUE id FROM task WHERE plan = $parent.id) AS task_ids
FROM ONLY $plan;";

    /// The garden as the pipeline and scheduler read it.
    pub const GARDEN_CONTEXT: &str =
        "SELECT id, name, centroid.coordinates[0] AS lon, centroid.coordinates[1] AS lat,
       location_precision, weather_cell, time_zone, soil_profile, map_version
FROM ONLY $garden WHERE deleting_at = NONE;";
}

/// A guided check row before its result observation is attached.
#[derive(Debug, serde::Deserialize)]
struct CheckJson {
    image_id: Json,
    kind: Json,
    status: Json,
    result: Option<String>,
}

/// `$garden`'s context row.
#[derive(Debug, Clone, SurrealValue)]
pub struct GardenContextRow {
    /// `garden:<ulid>`.
    pub id: RecordId,
    /// Its name.
    pub name: String,
    /// Longitude.
    pub lon: f64,
    /// Latitude.
    pub lat: f64,
    /// How precise.
    pub location_precision: LocationPrecisionRow,
    /// `"43.65,-79.38"`.
    pub weather_cell: String,
    /// IANA time zone.
    pub time_zone: String,
    /// `SoilGrids`, when fetched.
    pub soil_profile: Option<serde_json::Value>,
    /// The current map version.
    pub map_version: i64,
}

/// Renames a key of a JSON object in place.
pub fn rename(object: &mut Json, from: &str, to: &str) {
    if let Some(map) = object.as_object_mut()
        && let Some(v) = map.remove(from)
    {
        map.insert(to.to_owned(), v);
    }
}

fn take_value(
    results: &mut surrealdb::IndexedResults,
    index: usize,
    what: &'static str,
) -> Result<Json, StoreError> {
    let value: Value = results.take(index).map_err(|e| take_failed(what, e))?;
    Ok(json_of(value))
}

fn items(json: Json) -> Vec<Json> {
    match json {
        Json::Array(items) => items,
        Json::Null => Vec::new(),
        other => vec![other],
    }
}

fn first(json: Json) -> Option<Json> {
    items(json).into_iter().next()
}

/// A plant's JSON as `Plant`: stored candidates name their species `species`.
pub fn plant_from_json(mut json: Json, what: &'static str) -> Result<Plant<Stored>, RowError> {
    if let Some(candidates) = json
        .get_mut("species_candidates")
        .and_then(Json::as_array_mut)
    {
        for c in candidates {
            rename(c, "species", "species_id");
        }
    }
    serde_json::from_value(json).map_err(|source| RowError::Json {
        field: what,
        source,
    })
}

impl SurrealStore {
    /// The scan with its images, guided checks and open question, if it exists.
    pub(crate) async fn load_scan(&self, scan: ScanId) -> Result<Option<Scan<Stored>>, StoreError> {
        const WHAT: &str = "load scan";
        let mut r = run_tx(&self.db, sql::SCAN_PARTS, |q| q.bind(("scan", rid(&scan))))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let Some(mut base) = first(take_value(&mut r, 0, WHAT)?) else {
            return Ok(None);
        };
        if let Some(map) = base.as_object_mut() {
            map.remove("open_question");
        }
        let mut scan_value: Scan<Stored> = serde_json::from_value(base).map_err(|source| {
            RowError::Json {
                field: WHAT,
                source,
            }
            .into_store(WHAT)
        })?;
        scan_value.images = items(take_value(&mut r, 1, WHAT)?)
            .into_iter()
            .map(serde_json::from_value::<ScanImage<Stored>>)
            .collect::<Result<_, _>>()
            .map_err(|source| {
                RowError::Json {
                    field: "images",
                    source,
                }
                .into_store(WHAT)
            })?;
        let checks: Vec<CheckJson> =
            serde_json::from_value(take_value(&mut r, 2, WHAT)?).map_err(|source| {
                RowError::Json {
                    field: "guided_checks",
                    source,
                }
                .into_store(WHAT)
            })?;
        let results: Vec<Observation<Stored>> =
            serde_json::from_value(take_value(&mut r, 3, WHAT)?).map_err(|source| {
                RowError::Json {
                    field: "guided check results",
                    source,
                }
                .into_store(WHAT)
            })?;
        scan_value.guided_checks = checks
            .into_iter()
            .map(|c| {
                let result = c
                    .result
                    .and_then(|id| results.iter().find(|o| o.id.to_string() == id).cloned());
                let mut json = serde_json::json!({ "image_id": c.image_id, "kind": c.kind, "status": c.status });
                if let Some(result) = result
                    && let Ok(value) = serde_json::to_value(result)
                {
                    json["result"] = value;
                }
                serde_json::from_value::<GuidedCheck<Stored>>(json)
            })
            .collect::<Result<_, _>>()
            .map_err(|source| RowError::Json { field: "guided_checks", source }.into_store(WHAT))?;
        scan_value.open_question = first(take_value(&mut r, 4, WHAT)?)
            .map(serde_json::from_value::<Question>)
            .transpose()
            .map_err(|source| {
                RowError::Json {
                    field: "open_question",
                    source,
                }
                .into_store(WHAT)
            })?;
        Ok(Some(scan_value))
    }

    /// A plant, if it exists.
    pub(crate) async fn load_plant(
        &self,
        plant: PlantId,
    ) -> Result<Option<Plant<Stored>>, StoreError> {
        const WHAT: &str = "load plant";
        let mut r = run_tx(&self.db, sql::PLANT, |q| q.bind(("plant", rid(&plant))))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        first(take_value(&mut r, 0, WHAT)?)
            .map(|j| plant_from_json(j, WHAT))
            .transpose()
            .map_err(|e| e.into_store(WHAT))
    }

    /// Observations by id, in no particular order.
    pub(crate) async fn load_observations(
        &self,
        ids: &[ObservationId],
    ) -> Result<Vec<Observation<Stored>>, StoreError> {
        const WHAT: &str = "load observations";
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<RecordId> = ids.iter().map(rid).collect();
        let mut r = run_tx(&self.db, sql::OBSERVATIONS_BY_ID, |q| {
            q.bind(("ids", ids.clone()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        crate::read::take_all(&mut r, 0, WHAT)
    }

    /// A question, if it exists.
    pub(crate) async fn load_question(
        &self,
        question: QuestionId,
    ) -> Result<Option<Question>, StoreError> {
        const WHAT: &str = "load question";
        let mut r = run_tx(&self.db, sql::QUESTION, |q| {
            q.bind(("question", rid(&question)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        crate::read::take_one(&mut r, 0, WHAT)
    }

    /// A diagnosis with its soil observations, if it exists.
    pub(crate) async fn load_diagnosis(
        &self,
        diagnosis: DiagnosisId,
    ) -> Result<Option<Diagnosis<Stored>>, StoreError> {
        const WHAT: &str = "load diagnosis";
        let mut r = run_tx(&self.db, sql::DIAGNOSIS, |q| {
            q.bind(("diagnosis", rid(&diagnosis)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let Some(mut json) = first(take_value(&mut r, 0, WHAT)?) else {
            return Ok(None);
        };
        let soil_ids: Vec<ObservationId> = json
            .as_object_mut()
            .and_then(|m| m.remove("soil_ids"))
            .map(serde_json::from_value)
            .transpose()
            .map_err(|source| {
                RowError::Json {
                    field: "soil",
                    source,
                }
                .into_store(WHAT)
            })?
            .unwrap_or_default();
        let mut d: Diagnosis<Stored> = serde_json::from_value(json).map_err(|source| {
            RowError::Json {
                field: WHAT,
                source,
            }
            .into_store(WHAT)
        })?;
        let soil = self.load_observations(&soil_ids).await?;
        d.soil = soil_ids
            .iter()
            .filter_map(|id| soil.iter().find(|o| o.id == *id).cloned())
            .collect();
        Ok(Some(d))
    }

    /// A plan with its open question and task ids, if it exists.
    pub(crate) async fn load_plan(&self, plan: PlanId) -> Result<Option<ActionPlan>, StoreError> {
        const WHAT: &str = "load plan";
        let mut r = run_tx(&self.db, sql::PLAN, |q| q.bind(("plan", rid(&plan))))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let Some(mut json) = first(take_value(&mut r, 0, WHAT)?) else {
            return Ok(None);
        };
        if let Some(constraints) = json.get_mut("constraints").and_then(Json::as_array_mut) {
            for c in constraints {
                rename(c, "from_pending_action", "from_pending_action_id");
            }
        }
        let question: Option<QuestionId> = json
            .as_object_mut()
            .and_then(|m| m.remove("open_question_id"))
            .map(serde_json::from_value)
            .transpose()
            .map_err(|source| {
                RowError::Json {
                    field: "open_question",
                    source,
                }
                .into_store(WHAT)
            })?;
        let mut p: ActionPlan = serde_json::from_value(json).map_err(|source| {
            RowError::Json {
                field: WHAT,
                source,
            }
            .into_store(WHAT)
        })?;
        if let Some(q) = question {
            p.open_question = self.load_question(q).await?;
        }
        Ok(Some(p))
    }

    /// The garden as the pipeline and scheduler read it.
    pub(crate) async fn load_garden_context(
        &self,
        garden: GardenId,
    ) -> Result<GardenContext, StoreError> {
        const WHAT: &str = "garden context";
        let mut r = run_tx(&self.db, sql::GARDEN_CONTEXT, |q| {
            q.bind(("garden", rid(&garden)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let row: Option<GardenContextRow> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        let row = row.ok_or(StoreError::NotFound)?;
        let ctx = (|| {
            Ok::<_, RowError>(GardenContext {
                id: crate::ids::from_rid(&row.id)?,
                name: row.name,
                coordinates: Coordinates {
                    lat: row.lat,
                    lon: row.lon,
                },
                precision: row.location_precision.into(),
                weather_cell: WeatherCell::parse(&row.weather_cell).ok_or(RowError::BadField {
                    field: "weather_cell",
                    problem: row.weather_cell.clone(),
                })?,
                time_zone: TimeZone(row.time_zone),
                soil_profile: row
                    .soil_profile
                    .map(|p| from_json::<SoilProfile>("soil_profile", p))
                    .transpose()?,
                map_version: u32_of("map_version", row.map_version)?,
            })
        })();
        ctx.map_err(|e| e.into_store(WHAT))
    }
}
