//! Scans, their images and masks ([contracts 3]; [database 4], limits ✓).

use chrono::{DateTime, Utc};
use domain::{
    authz::{Editor, Membership, Reviewer, Viewer},
    error::{DomainError, InvalidStateDetails},
    events::RunReason,
    id::{FeedbackId, GardenId, GuidedCheckId, ImageId, JobId, MaskId, PlantId, ScanId},
    jobs::{GuidedPayload, JobImage, JobKind, JobSpec, WorkerImagePath},
    ports::{
        AddedImage, AnalysisRepo, ContentType, FailedScan, IdGenExt, NewFeedback, NewJob, NewMask,
        NewScan, NewScanImage, NewScanPlant, ObjectKey, ObjectRef, PhotoAnswerStart,
        QuestionStepClaim, RunStart, ScanRepo, ScanSnapshot, StartedRun, StoreError, StoredDepth,
    },
    rules::{
        limits::{DAILY_SCAN_LIMIT, check_daily_scans},
        questions::QuestionStep,
        uploads::UploadRoute,
    },
    scan::{DepthInfo, Scan, ScanDetail, ScanFailure, ScanImage, ScanStatus},
    url::{NoUrl, Stored},
    wire::{EventSeq, Run, Timestamp, UserRef},
};
use surrealdb::types::{RecordId, SurrealValue};

use crate::{
    SurrealStore,
    error::{DbFailure, RowError, take_failed, thrown, unknown_species},
    ids::{from_opt_rid, from_rid, opt_rid, rid, usage_rid},
    read::take_all,
    repo::{analysis::enum_str, jobs::JobWriteRow},
    rows::{QuestionStepRow, ScanStatusRow, dt, from_json, to_json, ts, u32_of},
    tx::run_tx,
};

/// The statements.
pub mod sql {
    /// `POST /scans`: the scan (and the new plant), counted in `usage_daily` in the same
    /// transaction ([database 4], limits ✓: `+=` on a missing record starts from the default,
    /// and the `THROW` over 30 rolls the scan back). A new plant's species must be one the
    /// knowledge base holds (`unknown_species`). Statements: 0 `BEGIN`, 1 the plant `IF`, 2
    /// `CREATE` scan, 3 `LET $count`, 4 the limit `IF`, 5 the scan read, 6 `COMMIT`.
    pub const CREATE_SCAN: &str = "BEGIN;
IF $new_plant != NONE {
  IF $new_plant.species != NONE { IF array::len(SELECT VALUE id FROM $new_plant.species) = 0 { THROW 'unknown_species' } };
  CREATE $plant SET garden = $garden, name = $new_plant.name, species = $new_plant.species,
    species_status = IF $new_plant.species != NONE { 'confirmed' } ELSE { 'unknown' }, created_at = $now;
} ELSE IF array::len(SELECT VALUE id FROM $plant WHERE garden = $garden AND deleting_at = NONE) = 0 {
  THROW 'not_found';
};
CREATE $scan SET garden = $garden, plant = $plant, created_by = $user, created_at = $now;
LET $count = (UPSERT ONLY $usage SET scans += 1 RETURN VALUE scans);
IF $count > $limit { THROW 'budget_exceeded' };
SELECT *, created_by.display_name AS creator_name FROM ONLY $scan;
COMMIT;";
    /// The index of the scan read in [`CREATE_SCAN`].
    pub const CREATE_SCAN_RESULT: usize = 5;

    /// Whether the scan is visible to a member of `$garden` (not being deleted, nor its plant).
    pub const SCAN_VISIBLE: &str = "SELECT VALUE id FROM $scan WHERE garden = $garden AND deleting_at = NONE AND plant.deleting_at = NONE AND garden.deleting_at = NONE;";
    /// The scan's weather snapshot, for the detail.
    pub const SCAN_WEATHER: &str = "SELECT VALUE weather FROM ONLY $scan;";

    /// `POST /scans/{id}/images`, after the object is written: re-checks the scan's status and
    /// `deleting_at`, the kind limit before submit and the question's state in the transaction;
    /// a guided check gets its `guided` job; a question photo moves the question to `answering`
    /// with its first step (`segmenting` with its job, or `measuring`). Returns the question's
    /// run.
    pub const ADD_IMAGE: &str = "BEGIN;
LET $sc = (SELECT status, deleting_at, garden, plant.deleting_at AS plant_deleting FROM ONLY $scan);
IF $sc = NONE OR $sc.deleting_at != NONE OR $sc.plant_deleting != NONE OR $sc.garden != $garden { THROW 'not_found' };
IF $route = 'run_shot' AND $sc.status != 'pending' { THROW 'invalid_state' };
IF $route = 'guided_check' AND $sc.status = 'failed' { THROW 'invalid_state' };
IF ($route = 'run_shot' OR ($route = 'guided_check' AND $sc.status = 'pending'))
   AND count(SELECT VALUE id FROM scan_image WHERE scan = $scan AND kind = $kind AND answers_question = NONE) >= $max { THROW 'too_many_images' };
IF $route = 'question_photo' AND array::len(SELECT VALUE id FROM $question WHERE scan = $scan AND status = 'open') = 0 { THROW 'invalid_state' };
CREATE $image CONTENT $row;
IF $check != NONE { CREATE $check.id CONTENT $check.row; CREATE $check_job.id CONTENT $check_job.row; };
IF $route = 'question_photo' {
  UPDATE $question SET status = 'answering', step = $step, step_at = $now;
  IF $step_job != NONE { CREATE $step_job.id CONTENT $step_job.row; };
};
RETURN IF $question != NONE { (SELECT VALUE run FROM ONLY $question) } ELSE { NONE };
COMMIT;";

    /// `DELETE /scans/{id}/images/{image_id}` while `pending`: the image, a guided check on it
    /// with its result, and the keys to delete in the background.
    pub const REMOVE_IMAGE: &str = "BEGIN;
LET $sc = (SELECT status, garden FROM ONLY $scan WHERE deleting_at = NONE);
IF $sc = NONE OR $sc.garden != $garden { THROW 'not_found' };
IF $sc.status != 'pending' { THROW 'invalid_state' };
LET $keys = (SELECT storage_key, depth_key, depth_confidence_key FROM ONLY $image WHERE scan = $scan);
IF $keys = NONE { THROW 'not_found' };
LET $results = (SELECT VALUE result FROM guided_check WHERE image = $image AND result != NONE);
DELETE guided_check WHERE image = $image;
DELETE observation WHERE id IN $results OR image = $image;
DELETE $image;
RETURN $keys;
COMMIT;";

    /// Submit, resubmit and species change ([contracts 3]): one transaction moves the status,
    /// increments `run`, cancels the old run's queued and leased run jobs, supersedes the open
    /// question, releases the step claim, resets the candidate set and queues the first job.
    pub const START_RUN: &str = "BEGIN;
LET $sc = (SELECT status, run, open_question, garden FROM ONLY $scan WHERE deleting_at = NONE AND plant.deleting_at = NONE);
IF $sc = NONE OR $sc.garden != $garden { THROW 'not_found' };
IF $sc.status != $expected_status OR $sc.run != $expected_run { THROW 'invalid_state' };
LET $cancelled = (UPDATE job SET status = 'cancelled', finished_at = $now
                  WHERE scan = $scan AND run = $sc.run AND run != NONE AND status IN ['queued', 'leased'] RETURN VALUE id);
IF $sc.open_question != NONE {
  UPDATE $sc.open_question SET status = 'superseded', resolved_by = 'superseded', resolved_at = $now, step = NONE, step_at = NONE;
};
UPDATE $scan SET status = $to, run = $sc.run + 1, working_species = $species, open_question = NONE, questions_asked = 0,
  pipeline_step = NONE, pipeline_step_run = NONE, pipeline_step_at = NONE, rescore_pending = false, candidates = [],
  submitted_at = submitted_at ?? $now, failure = NONE;
CREATE $job.id CONTENT $job.row;
RETURN { cancelled: $cancelled, superseded: $sc.open_question };
COMMIT;";

    /// Fails the run `$run`: releases the claim and supersedes its question, so a `failed`
    /// scan never holds one ([contracts 3]).
    pub const FAIL_SCAN: &str = "BEGIN;
LET $sc = (SELECT status, run, open_question FROM ONLY $scan);
IF $sc = NONE { THROW 'not_found' };
IF $sc.run != $run OR $sc.status = 'failed' { THROW 'invalid_state' };
IF $sc.open_question != NONE {
  UPDATE $sc.open_question SET status = 'superseded', resolved_by = 'superseded', resolved_at = $now, step = NONE, step_at = NONE;
};
UPDATE $scan SET status = 'failed', failure = $failure, open_question = NONE,
  pipeline_step = NONE, pipeline_step_run = NONE, pipeline_step_at = NONE, rescore_pending = false;
RETURN { from: $sc.status, superseded: $sc.open_question };
COMMIT;";

    /// `POST /scans/{id}/feedback`; refused while `pending` (it would block the scan's deletion).
    pub const RECORD_FEEDBACK: &str = "BEGIN;
LET $st = (SELECT VALUE status FROM ONLY $scan WHERE garden = $garden AND deleting_at = NONE);
IF $st = NONE { THROW 'not_found' };
IF $st = 'pending' { THROW 'invalid_state' };
CREATE $feedback SET scan = $scan, user = $user, verdict = $verdict, corrected_label = $label, notes = $notes;
COMMIT;";

    /// The object behind a signed image URL; depth keys are never returned.
    pub const IMAGE_OBJECT: &str = "SELECT VALUE storage_key FROM $image WHERE scan = $scan;";
    /// The object behind a signed mask URL.
    pub const MASK_OBJECT: &str = "SELECT VALUE storage_key FROM $mask WHERE scan = $scan;";

    /// The parts of a step's snapshot the loaders don't cover. Statements: 0 the scan's own
    /// fields, 1 the run's indicator answers (every source, schema v1.4), 2 the masks, 3 the
    /// depth of each image, 4 the open question's step and indicator.
    pub const SNAPSHOT_PARTS: &str = "SELECT run, working_species, candidates, weather, questions_asked, rescore_pending, plant, garden,
       plant.custom_species_name != NONE AS custom_species, open_question, diagnosis, plan, plan.diagnosis AS plan_diagnosis
FROM ONLY $scan WHERE deleting_at = NONE AND plant.deleting_at = NONE AND garden.deleting_at = NONE;
SELECT VALUE evidence FROM indicator_answer WHERE scan = $scan AND run = (SELECT VALUE run FROM ONLY $scan);
SELECT id, image, region_id, storage_key, rle FROM mask WHERE scan = $scan AND rle != NONE;
SELECT id, depth_key, depth_confidence_key, depth_meta FROM scan_image WHERE scan = $scan AND depth_key != NONE;
SELECT id, step, step_at, run, indicator, status FROM question WHERE id = (SELECT VALUE open_question FROM ONLY $scan);";
}

/// A `scan` row, as [`sql::CREATE_SCAN`] reads it.
#[derive(Debug, Clone, SurrealValue)]
pub struct ScanRow {
    /// `scan:<ulid>`.
    pub id: RecordId,
    /// Its garden.
    pub garden: RecordId,
    /// Its plant.
    pub plant: RecordId,
    /// Its creator; `NONE` once deleted.
    pub created_by: Option<RecordId>,
    /// The creator's display name.
    pub creator_name: Option<String>,
    /// The status.
    pub status: ScanStatusRow,
    /// 0 until submitted.
    pub run: i64,
    /// The failure, with status `failed`.
    pub failure: Option<serde_json::Value>,
    /// The current diagnosis.
    pub diagnosis: Option<RecordId>,
    /// The current plan.
    pub plan: Option<RecordId>,
    /// The event counter.
    pub last_event_seq: i64,
    /// When it was submitted.
    pub submitted_at: Option<DateTime<Utc>>,
    /// When it was created.
    pub created_at: DateTime<Utc>,
}

impl TryFrom<ScanRow> for Scan<Stored> {
    type Error = RowError;

    /// The scan without its images, checks and open question, which come from their own tables
    /// (a new scan has none).
    fn try_from(row: ScanRow) -> Result<Self, RowError> {
        let created_by = match (from_opt_rid(row.created_by.as_ref())?, row.creator_name) {
            (Some(user_id), Some(display_name)) => Some(UserRef {
                user_id,
                display_name,
            }),
            _ => None,
        };
        Ok(Self {
            id: from_rid(&row.id)?,
            garden_id: from_rid(&row.garden)?,
            plant_id: from_rid(&row.plant)?,
            status: row.status.into(),
            run: Run(u32_of("run", row.run)?),
            images: Vec::new(),
            guided_checks: Vec::new(),
            created_at: ts(row.created_at),
            created_by,
            submitted_at: row.submitted_at.map(ts),
            open_question: None,
            diagnosis_id: from_opt_rid(row.diagnosis.as_ref())?,
            plan_id: from_opt_rid(row.plan.as_ref())?,
            failure: row
                .failure
                .map(|f| crate::rows::from_json::<ScanFailure>("failure", f))
                .transpose()?,
            last_event_seq: EventSeq(u64::try_from(row.last_event_seq).map_err(|_| {
                RowError::BadField {
                    field: "last_event_seq",
                    problem: "negative".to_owned(),
                }
            })?),
        })
    }
}

/// A `scan_image` row as written ([database 3]; depth from contracts 3).
#[derive(Debug, Clone, SurrealValue)]
pub struct ScanImageRow {
    /// Its scan.
    pub scan: RecordId,
    /// Its garden, denormalised for the storage key and similar-case search.
    pub garden: RecordId,
    /// The shot.
    pub kind: String,
    /// The question it answers.
    pub answers_question: Option<RecordId>,
    /// Stored width.
    pub width: i64,
    /// Stored height.
    pub height: i64,
    /// Its key.
    pub storage_key: String,
    /// `{ blur_score, exposure }`.
    pub quality: serde_json::Value,
    /// The 16-bit depth PNG's key.
    pub depth_key: Option<String>,
    /// The confidence PNG's key.
    pub depth_confidence_key: Option<String>,
    /// `DepthMeta` in the stored frame, with the map's `width` and `height`.
    pub depth_meta: Option<serde_json::Value>,
    /// When.
    pub uploaded_at: DateTime<Utc>,
}

impl ScanImageRow {
    /// The row of an upload.
    pub fn new(scan: ScanId, garden: GardenId, image: &NewScanImage) -> Result<Self, RowError> {
        let depth_meta = image
            .depth
            .as_ref()
            .map(|d| {
                let mut meta = to_json("depth_meta", &d.meta)?;
                if let Some(map) = meta.as_object_mut() {
                    map.insert("width".to_owned(), d.width.into());
                    map.insert("height".to_owned(), d.height.into());
                }
                Ok::<_, RowError>(meta)
            })
            .transpose()?;
        Ok(Self {
            scan: rid(&scan),
            garden: rid(&garden),
            kind: enum_str("kind", &image.kind)?,
            answers_question: opt_rid(image.question.as_ref()),
            width: i64::from(image.width),
            height: i64::from(image.height),
            storage_key: image.storage_key.as_str().to_owned(),
            quality: serde_json::json!({ "blur_score": image.quality.blur_score, "exposure": image.quality.exposure }),
            depth_key: image
                .depth
                .as_ref()
                .map(|d| d.depth_key.as_str().to_owned()),
            depth_confidence_key: image
                .depth
                .as_ref()
                .and_then(|d| d.confidence_key.as_ref())
                .map(|k| k.as_str().to_owned()),
            depth_meta,
            uploaded_at: dt(image.uploaded_at),
        })
    }
}

/// A `guided_check` row as written.
#[derive(Debug, Clone, SurrealValue)]
pub struct GuidedCheckRow {
    /// Its scan.
    pub scan: RecordId,
    /// Its image.
    pub image: RecordId,
    /// `squeeze_test` or `jar_test`.
    pub kind: String,
    /// When.
    pub created_at: DateTime<Utc>,
}

/// A guided check with its record id.
#[derive(Debug, Clone, SurrealValue)]
pub struct GuidedCheckWriteRow {
    /// `guided_check:<ulid>`.
    pub id: RecordId,
    /// The row.
    pub row: GuidedCheckRow,
}

/// The keys a removed shot leaves.
#[derive(Debug, Clone, SurrealValue)]
pub struct KeysRow {
    /// The photo.
    pub storage_key: String,
    /// Its depth map.
    pub depth_key: Option<String>,
    /// Its confidence map.
    pub depth_confidence_key: Option<String>,
}

/// The result of [`sql::START_RUN`].
#[derive(Debug, Clone, SurrealValue)]
pub struct StartRow {
    /// Jobs cancelled.
    pub cancelled: Vec<RecordId>,
    /// The question superseded.
    pub superseded: Option<RecordId>,
}

/// The result of [`sql::FAIL_SCAN`].
#[derive(Debug, Clone, SurrealValue)]
pub struct FailRow {
    /// The status before.
    pub from: ScanStatusRow,
    /// The question superseded.
    pub superseded: Option<RecordId>,
}

/// Statement 0 of [`sql::SNAPSHOT_PARTS`].
#[derive(Debug, Clone, SurrealValue)]
pub struct SnapshotRow {
    /// The current run.
    pub run: i64,
    /// The species the run works with.
    pub working_species: Option<RecordId>,
    /// The run's candidate set.
    pub candidates: serde_json::Value,
    /// The weather snapshot.
    pub weather: Option<serde_json::Value>,
    /// Questions asked in the run.
    pub questions_asked: i64,
    /// Input arrived while a step held the claim.
    pub rescore_pending: bool,
    /// The plant.
    pub plant: RecordId,
    /// The garden.
    pub garden: RecordId,
    /// "Something else" was chosen.
    pub custom_species: bool,
    /// The current diagnosis.
    pub diagnosis: Option<RecordId>,
    /// The current plan.
    pub plan: Option<RecordId>,
    /// The diagnosis the plan was written from.
    pub plan_diagnosis: Option<RecordId>,
}

/// A stored mask, for measurement.
#[derive(Debug, Clone, SurrealValue)]
pub struct MaskReadRow {
    /// `mask:<ulid>`.
    pub id: RecordId,
    /// Its image.
    pub image: RecordId,
    /// `"<image_id>.<n>"`.
    pub region_id: String,
    /// Its key.
    pub storage_key: String,
    /// The worker's `RleMask`.
    pub rle: serde_json::Value,
}

impl MaskReadRow {
    fn into_mask(self) -> Result<NewMask, RowError> {
        Ok(NewMask {
            id: from_rid(&self.id)?,
            image: from_rid(&self.image)?,
            region_id: self.region_id.parse().map_err(|_| RowError::BadField {
                field: "region_id",
                problem: self.region_id.clone(),
            })?,
            storage_key: ObjectKey::from_stored(&self.storage_key).ok_or(RowError::BadField {
                field: "storage_key",
                problem: self.storage_key.clone(),
            })?,
            rle: from_json("rle", self.rle)?,
        })
    }
}

/// A stored depth map.
#[derive(Debug, Clone, SurrealValue)]
pub struct DepthReadRow {
    /// The image.
    pub id: RecordId,
    /// The depth PNG.
    pub depth_key: String,
    /// The confidence PNG.
    pub depth_confidence_key: Option<String>,
    /// `DepthMeta` with `width` and `height`.
    pub depth_meta: serde_json::Value,
}

impl DepthReadRow {
    fn into_depth(self) -> Result<(ImageId, StoredDepth), RowError> {
        let mut meta = self.depth_meta;
        let size = |m: &mut serde_json::Value, k: &'static str| {
            m.as_object_mut()
                .and_then(|o| o.remove(k))
                .and_then(|v| v.as_u64())
                .and_then(|v| u32::try_from(v).ok())
                .ok_or(RowError::BadField {
                    field: k,
                    problem: "missing".to_owned(),
                })
        };
        let width = size(&mut meta, "width")?;
        let height = size(&mut meta, "height")?;
        let key = |k: &str| {
            ObjectKey::from_stored(k).ok_or(RowError::BadField {
                field: "depth_key",
                problem: k.to_owned(),
            })
        };
        Ok((
            from_rid(&self.id)?,
            StoredDepth {
                depth_key: key(&self.depth_key)?,
                confidence_key: self.depth_confidence_key.as_deref().map(key).transpose()?,
                meta: from_json("depth_meta", meta)?,
                width,
                height,
            },
        ))
    }
}

/// The open question's step and indicator.
#[derive(Debug, Clone, SurrealValue)]
pub struct OpenQuestionRow {
    /// The question.
    pub id: RecordId,
    /// Its step while `answering`.
    pub step: Option<QuestionStepRow>,
    /// The step's stamp.
    pub step_at: Option<DateTime<Utc>>,
    /// Its run.
    pub run: i64,
    /// The indicator it asks about.
    pub indicator: Option<String>,
    /// `open`, `answering`, …
    pub status: String,
}

/// `$new_plant` of [`sql::CREATE_SCAN`].
#[derive(Debug, Clone, SurrealValue)]
pub struct NewPlantRow {
    /// Its name.
    pub name: String,
    /// Its species, confirmed when given.
    pub species: Option<RecordId>,
}

#[async_trait::async_trait]
#[allow(
    clippy::too_many_lines,
    reason = "add_image and snapshot assemble many parts"
)]
impl ScanRepo for SurrealStore {
    async fn create_scan(
        &self,
        editor: Membership<Editor>,
        scan: NewScan,
    ) -> Result<Scan<Stored>, StoreError> {
        const WHAT: &str = "create scan";
        let (plant, new_plant) = match &scan.plant {
            NewScanPlant::Existing(id) => (rid(id), None),
            NewScanPlant::New { id, plant } => (
                rid(id),
                Some(NewPlantRow {
                    name: plant.name.clone(),
                    species: opt_rid(plant.species_id.as_ref()),
                }),
            ),
        };
        let result = run_tx(&self.db, sql::CREATE_SCAN, |q| {
            q.bind(("garden", rid(&editor.garden())))
                .bind(("plant", plant.clone()))
                .bind(("new_plant", new_plant.clone()))
                .bind(("scan", rid(&scan.id)))
                .bind(("user", rid(&scan.created_by)))
                .bind((
                    "usage",
                    usage_rid(scan.created_by, scan.created_at.utc_day()),
                ))
                .bind(("limit", i64::from(DAILY_SCAN_LIMIT)))
                .bind(("now", dt(scan.created_at)))
        })
        .await;
        let mut results = match result {
            Ok(results) => results,
            Err(failure) if failure.thrown() == Some(thrown::BUDGET_EXCEEDED) => {
                // reason: the limit was exceeded, so the check fails and yields the error.
                let error = check_daily_scans(DAILY_SCAN_LIMIT + 1, scan.created_at)
                    .err()
                    .map_or(StoreError::NotFound, StoreError::from);
                return Err(error);
            }
            Err(failure) if failure.thrown() == Some(thrown::UNKNOWN_SPECIES) => {
                return Err(unknown_species("new_plant.species_id"));
            }
            Err(failure) => return Err(failure.into_store(WHAT)),
        };
        let row: Option<ScanRow> = results
            .take(sql::CREATE_SCAN_RESULT)
            .map_err(|e| take_failed(WHAT, e))?;
        row.ok_or(StoreError::NotFound)?
            .try_into()
            .map_err(|e: RowError| e.into_store(WHAT))
    }

    async fn scan(
        &self,
        member: Membership<Viewer>,
        scan: ScanId,
    ) -> Result<Scan<Stored>, StoreError> {
        self.visible_scan(member.garden(), scan).await?;
        self.load_scan(scan).await?.ok_or(StoreError::NotFound)
    }

    async fn scan_detail(
        &self,
        member: Membership<Viewer>,
        scan: ScanId,
    ) -> Result<ScanDetail<Stored>, StoreError> {
        const WHAT: &str = "scan detail";
        self.visible_scan(member.garden(), scan).await?;
        let scan_value = self.load_scan(scan).await?.ok_or(StoreError::NotFound)?;
        let plant = self
            .load_plant(scan_value.plant_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        let observations = self.observations(scan).await?;
        let mut r = run_tx(&self.db, sql::SCAN_WEATHER, |q| {
            q.bind(("scan", rid(&scan)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let weather: Option<serde_json::Value> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        let weather = weather
            .map(|w| from_json("weather", w))
            .transpose()
            .map_err(|e| e.into_store(WHAT))?;
        let diagnosis = match scan_value.diagnosis_id {
            Some(d) => self.load_diagnosis(d).await?,
            None => None,
        };
        let plan = match scan_value.plan_id {
            Some(p) => self.load_plan(p).await?,
            None => None,
        };
        Ok(ScanDetail {
            scan: scan_value,
            plant,
            observations,
            weather,
            diagnosis,
            plan,
        })
    }

    async fn add_image(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        image: NewScanImage,
    ) -> Result<AddedImage, StoreError> {
        const WHAT: &str = "add image";
        let route = if image.question.is_some() {
            UploadRoute::QuestionPhoto
        } else if image.kind.guided().is_some() {
            UploadRoute::GuidedCheck
        } else {
            UploadRoute::RunShot
        };
        let route_name = match route {
            UploadRoute::RunShot => "run_shot",
            UploadRoute::GuidedCheck => "guided_check",
            UploadRoute::QuestionPhoto => "question_photo",
        };
        let row =
            ScanImageRow::new(scan, editor.garden(), &image).map_err(|e| e.into_store(WHAT))?;
        let kind = enum_str("kind", &image.kind).map_err(|e| e.into_store(WHAT))?;
        // A guided check and its job are the only records the store creates ids for.
        let (check, check_job, check_id) = if route == UploadRoute::GuidedCheck {
            let check_id: GuidedCheckId = self.ids.id();
            let job_id: JobId = self.ids.id();
            let job = NewJob {
                id: job_id,
                scan,
                run: None,
                question: None,
                spec: JobSpec::Guided(GuidedPayload {
                    image: JobImage {
                        image_id: image.id,
                        kind: image.kind,
                        width: image.width,
                        height: image.height,
                        url: WorkerImagePath::new(job_id, image.id),
                    },
                }),
                max_runtime: JobKind::Guided.max_runtime(false),
                queued_at: image.uploaded_at,
            };
            let check = GuidedCheckWriteRow {
                id: rid(&check_id),
                row: GuidedCheckRow {
                    scan: rid(&scan),
                    image: rid(&image.id),
                    kind: kind.clone(),
                    created_at: dt(image.uploaded_at),
                },
            };
            (
                Some(check),
                Some(JobWriteRow::new(&job).map_err(|e| e.into_store(WHAT))?),
                Some(check_id),
            )
        } else {
            (None, None, None)
        };
        let (step, step_job) = match &image.photo_answer_start {
            Some(PhotoAnswerStart::Segment(job)) => (
                Some(QuestionStep::Segmenting),
                Some(JobWriteRow::new(job).map_err(|e| e.into_store(WHAT))?),
            ),
            Some(PhotoAnswerStart::Measure) => (Some(QuestionStep::Measuring), None),
            None if route == UploadRoute::QuestionPhoto => (Some(QuestionStep::Measuring), None),
            None => (None, None),
        };
        let result = run_tx(&self.db, sql::ADD_IMAGE, |q| {
            q.bind(("scan", rid(&scan)))
                .bind(("garden", rid(&editor.garden())))
                .bind(("image", rid(&image.id)))
                .bind(("row", row.clone()))
                .bind(("route", route_name))
                .bind(("kind", kind.clone()))
                .bind(("max", i64::from(image.kind.max_before_submit())))
                .bind(("check", check.clone()))
                .bind(("check_job", check_job.clone()))
                .bind(("question", opt_rid(image.question.as_ref())))
                .bind(("step", step.map(QuestionStepRow::from)))
                .bind(("step_job", step_job.clone()))
                .bind(("now", dt(image.uploaded_at)))
        })
        .await;
        let mut results = match result {
            Ok(r) => r,
            Err(f) => {
                return Err(match f.thrown() {
                    Some(thrown::INVALID_STATE) => StoreError::Rejected(DomainError::InvalidState(
                        InvalidStateDetails::default(),
                    )),
                    Some("too_many_images") => StoreError::Rejected(DomainError::TooManyImages {
                        kind: image.kind,
                        max: image.kind.max_before_submit(),
                    }),
                    _ => f.into_store(WHAT),
                });
            }
        };
        if check_job.is_some() || step_job.is_some() {
            self.jobs_queued.notify_waiters();
        }
        let last = results.num_statements().saturating_sub(2);
        let run: Option<i64> = results.take(last).map_err(|e| take_failed(WHAT, e))?;
        let photo_answer = match (image.question, step, run) {
            (Some(question), Some(step), Some(run)) => Some(QuestionStepClaim {
                question,
                scan,
                step,
                run: Run(u32_of("run", run).map_err(|e| e.into_store(WHAT))?),
                stamp: image.uploaded_at,
            }),
            _ => None,
        };
        Ok(AddedImage {
            image: ScanImage {
                id: image.id,
                kind: image.kind,
                answers_question_id: image.question,
                width: image.width,
                height: image.height,
                url: NoUrl,
                uploaded_at: image.uploaded_at,
                depth: image.depth.as_ref().map(|d| DepthInfo {
                    width: d.width,
                    height: d.height,
                    has_gravity: d.meta.gravity.is_some(),
                }),
            },
            route,
            guided_check: check_id,
            photo_answer,
        })
    }

    async fn remove_image(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        image: ImageId,
    ) -> Result<Vec<ObjectKey>, StoreError> {
        const WHAT: &str = "remove image";
        let mut results = run_tx(&self.db, sql::REMOVE_IMAGE, |q| {
            q.bind(("scan", rid(&scan)))
                .bind(("garden", rid(&editor.garden())))
                .bind(("image", rid(&image)))
        })
        .await
        .map_err(|f| invalid_state(f, WHAT))?;
        let last = results.num_statements().saturating_sub(2);
        let keys: Option<KeysRow> = results.take(last).map_err(|e| take_failed(WHAT, e))?;
        let keys = keys.ok_or(StoreError::NotFound)?;
        Ok([
            Some(keys.storage_key),
            keys.depth_key,
            keys.depth_confidence_key,
        ]
        .into_iter()
        .flatten()
        .filter_map(|k| ObjectKey::from_stored(&k))
        .collect())
    }

    async fn start_run(
        &self,
        editor: Membership<Editor>,
        scan: ScanId,
        start: RunStart,
    ) -> Result<StartedRun, StoreError> {
        const WHAT: &str = "start run";
        let to = match start.reason {
            RunReason::SpeciesChanged => ScanStatus::Analysing,
            RunReason::Submit | RunReason::Resubmit => ScanStatus::Queued,
        };
        let job = JobWriteRow::new(&start.first_job).map_err(|e| e.into_store(WHAT))?;
        let result = run_tx(&self.db, sql::START_RUN, |q| {
            q.bind(("scan", rid(&scan)))
                .bind(("garden", rid(&editor.garden())))
                .bind((
                    "expected_status",
                    ScanStatusRow::from(start.expected_status),
                ))
                .bind(("expected_run", i64::from(start.expected_run.0)))
                .bind(("to", ScanStatusRow::from(to)))
                .bind(("species", opt_rid(start.working_species.as_ref())))
                .bind(("job", job.clone()))
                .bind(("now", dt(start.now)))
        })
        .await;
        let mut results = match result {
            Ok(r) => r,
            Err(f) if f.thrown() == Some(thrown::INVALID_STATE) => {
                return Err(StoreError::Rejected(DomainError::InvalidState(
                    InvalidStateDetails {
                        status: Some(start.expected_status.as_str().to_owned()),
                        ..InvalidStateDetails::default()
                    },
                )));
            }
            Err(f) => return Err(f.into_store(WHAT)),
        };
        self.jobs_queued.notify_waiters();
        let last = results.num_statements().saturating_sub(2);
        let row: Option<StartRow> = results.take(last).map_err(|e| take_failed(WHAT, e))?;
        let row = row.ok_or(StoreError::NotFound)?;
        Ok(StartedRun {
            run: start.expected_run.next(),
            from: start.expected_status,
            to,
            superseded_question: from_opt_rid(row.superseded.as_ref())
                .map_err(|e| e.into_store(WHAT))?,
            cancelled_jobs: row
                .cancelled
                .iter()
                .map(from_rid)
                .collect::<Result<_, _>>()
                .map_err(|e: RowError| e.into_store(WHAT))?,
        })
    }

    async fn fail_scan(
        &self,
        scan: ScanId,
        run: Run,
        failure: ScanFailure,
        now: Timestamp,
    ) -> Result<FailedScan, StoreError> {
        const WHAT: &str = "fail scan";
        let failure = to_json("failure", &failure).map_err(|e| e.into_store(WHAT))?;
        let mut results = run_tx(&self.db, sql::FAIL_SCAN, |q| {
            q.bind(("scan", rid(&scan)))
                .bind(("run", i64::from(run.0)))
                .bind(("failure", failure.clone()))
                .bind(("now", dt(now)))
        })
        .await
        .map_err(|f| invalid_state(f, WHAT))?;
        let last = results.num_statements().saturating_sub(2);
        let row: Option<FailRow> = results.take(last).map_err(|e| take_failed(WHAT, e))?;
        let row = row.ok_or(StoreError::NotFound)?;
        Ok(FailedScan {
            from: row.from.into(),
            superseded_question: from_opt_rid(row.superseded.as_ref())
                .map_err(|e| e.into_store(WHAT))?,
        })
    }

    async fn record_feedback(
        &self,
        reviewer: Membership<Reviewer>,
        scan: ScanId,
        feedback: NewFeedback,
    ) -> Result<FeedbackId, StoreError> {
        const WHAT: &str = "record feedback";
        let verdict =
            enum_str("verdict", &feedback.request.verdict).map_err(|e| e.into_store(WHAT))?;
        run_tx(&self.db, sql::RECORD_FEEDBACK, |q| {
            q.bind(("scan", rid(&scan)))
                .bind(("garden", rid(&reviewer.garden())))
                .bind(("feedback", rid(&feedback.id)))
                .bind(("user", rid(&feedback.user)))
                .bind(("verdict", verdict.clone()))
                .bind(("label", feedback.request.corrected_label.clone()))
                .bind(("notes", feedback.request.notes.clone()))
        })
        .await
        .map_err(|f| invalid_state(f, WHAT))?;
        Ok(feedback.id)
    }

    async fn image_object(
        &self,
        scan: ScanId,
        image: ImageId,
    ) -> Result<Option<ObjectRef>, StoreError> {
        self.object_ref(
            sql::IMAGE_OBJECT,
            "image",
            rid(&image),
            scan,
            ContentType::Webp,
        )
        .await
    }

    async fn mask_object(
        &self,
        scan: ScanId,
        mask: MaskId,
    ) -> Result<Option<ObjectRef>, StoreError> {
        self.object_ref(sql::MASK_OBJECT, "mask", rid(&mask), scan, ContentType::Png)
            .await
    }

    async fn snapshot(&self, scan: ScanId) -> Result<ScanSnapshot, StoreError> {
        const WHAT: &str = "snapshot";
        let mut r = run_tx(&self.db, sql::SNAPSHOT_PARTS, |q| {
            q.bind(("scan", rid(&scan)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let parts: Option<SnapshotRow> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        // A scan, plant or garden being deleted reads as missing.
        let parts = parts.ok_or(StoreError::NotFound)?;
        let evidence: Vec<serde_json::Value> = r.take(1).map_err(|e| take_failed(WHAT, e))?;
        let masks: Vec<MaskReadRow> = r.take(2).map_err(|e| take_failed(WHAT, e))?;
        let depth: Vec<DepthReadRow> = r.take(3).map_err(|e| take_failed(WHAT, e))?;
        let open: Vec<OpenQuestionRow> = r.take(4).map_err(|e| take_failed(WHAT, e))?;
        let row_err = |e: RowError| e.into_store(WHAT);

        let scan_value = self.load_scan(scan).await?.ok_or(StoreError::NotFound)?;
        let plant_id: PlantId = from_rid(&parts.plant).map_err(row_err)?;
        let plant = self
            .load_plant(plant_id)
            .await?
            .ok_or(StoreError::NotFound)?;
        let garden = self
            .load_garden_context(from_rid(&parts.garden).map_err(row_err)?)
            .await?;
        let run = Run(u32_of("run", parts.run).map_err(row_err)?);
        let questions = {
            let mut q = run_tx(&self.db, crate::repo::load::sql::RUN_QUESTIONS, |q| {
                q.bind(("scan", rid(&scan))).bind(("run", i64::from(run.0)))
            })
            .await
            .map_err(|f| f.into_store(WHAT))?;
            take_all(&mut q, 0, WHAT)?
        };
        let diagnosis = match parts.diagnosis.as_ref() {
            Some(d) => self.load_diagnosis(from_rid(d).map_err(row_err)?).await?,
            None => None,
        };
        let plan = match parts.plan.as_ref() {
            Some(p) => self.load_plan(from_rid(p).map_err(row_err)?).await?,
            None => None,
        };
        let plan_diagnosis = match parts.plan_diagnosis.as_ref() {
            Some(d) => self.load_diagnosis(from_rid(d).map_err(row_err)?).await?,
            None => None,
        };
        let open = open.into_iter().next();
        let open_question_claim = match &open {
            Some(OpenQuestionRow {
                id,
                step: Some(step),
                step_at: Some(at),
                run,
                status,
                ..
            }) if status == "answering" => Some(QuestionStepClaim {
                question: from_rid(id).map_err(row_err)?,
                scan,
                step: (*step).into(),
                run: Run(u32_of("run", *run).map_err(row_err)?),
                stamp: ts(*at),
            }),
            _ => None,
        };
        let open_question_indicator = open
            .as_ref()
            .and_then(|o| o.indicator.as_deref())
            .map(str::parse)
            .transpose()
            .map_err(|_| RowError::BadField {
                field: "indicator",
                problem: "not an indicator id".to_owned(),
            })
            .map_err(row_err)?;
        Ok(ScanSnapshot {
            scan: scan_value,
            plant,
            garden,
            working_species: from_opt_rid(parts.working_species.as_ref()).map_err(row_err)?,
            custom_species: parts.custom_species,
            observations: self.observations(scan).await?,
            evidence: evidence
                .into_iter()
                .map(|e| from_json("evidence", e))
                .collect::<Result<_, _>>()
                .map_err(row_err)?,
            candidates: from_json("candidates", parts.candidates).map_err(row_err)?,
            weather: parts
                .weather
                .map(|w| from_json("weather", w))
                .transpose()
                .map_err(row_err)?,
            open_question_claim,
            open_question_indicator,
            masks: masks
                .into_iter()
                .map(MaskReadRow::into_mask)
                .collect::<Result<_, _>>()
                .map_err(row_err)?,
            depth: depth
                .into_iter()
                .map(DepthReadRow::into_depth)
                .collect::<Result<_, _>>()
                .map_err(row_err)?,
            questions_asked: u32_of("questions_asked", parts.questions_asked).map_err(row_err)?,
            questions,
            diagnosis,
            plan,
            plan_diagnosis,
            rescore_pending: parts.rescore_pending,
        })
    }
}

/// `invalid_state` as `Rejected(InvalidState)`, anything else by the default mapping.
fn invalid_state(f: DbFailure, what: &'static str) -> StoreError {
    if f.thrown() == Some(thrown::INVALID_STATE) {
        StoreError::Rejected(DomainError::InvalidState(InvalidStateDetails::default()))
    } else {
        f.into_store(what)
    }
}

impl SurrealStore {
    /// `NotFound` unless the scan is visible in `garden` ([`sql::SCAN_VISIBLE`]).
    pub(crate) async fn visible_scan(
        &self,
        garden: GardenId,
        scan: ScanId,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "scan visible";
        let mut r = run_tx(&self.db, sql::SCAN_VISIBLE, |q| {
            q.bind(("scan", rid(&scan))).bind(("garden", rid(&garden)))
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

    /// The storage key of an image or mask of `scan`, behind a signed URL.
    async fn object_ref(
        &self,
        statement: &'static str,
        name: &'static str,
        id: RecordId,
        scan: ScanId,
        content_type: ContentType,
    ) -> Result<Option<ObjectRef>, StoreError> {
        const WHAT: &str = "object key";
        let mut r = run_tx(&self.db, statement, |q| {
            q.bind((name, id.clone())).bind(("scan", rid(&scan)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let keys: Vec<String> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        Ok(keys
            .first()
            .and_then(|k| ObjectKey::from_stored(k))
            .map(|key| ObjectRef { key, content_type }))
    }
}
