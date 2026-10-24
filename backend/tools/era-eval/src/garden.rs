//! `garden.json`, format 1, and its loader (the fixture's README, `garden.json`): the garden,
//! its members, the plants, the scans with their diagnoses and the memories, written through
//! the store's ports as the routes and the pipeline would leave them.
//!
//! The evaluation's store holds the knowledge seed (`knowledge.surql`, applied as `api` applies
//! it at start; [`crate::bench::store`]), so names resolve against it:
//!
//! - **Species** as `tools/demo/demo_garden.py`'s `species_id` resolves them: the exact
//!   scientific name, the common name deciding between several; else the genus, when exactly
//!   one species recorded by its genus has a name containing the common name (Zucchini's
//!   *Cucurbita pepo* is `squash`). A plant whose species isn't in the seed (kale, garlic) is
//!   created without one and listed in [`Loaded::plants_without_species`].
//! - **Scans** follow a run's path through the ports: created, submitted with the plant's
//!   species as the run's working species, its `perception` job claimed and completed (with an
//!   observation per organism, tied to no image: the fixture has no photos), `score` committed
//!   with the diagnosis, and for a `planned` scan `plan` committed with a stand-in plan: no
//!   steps, the scan's pipeline summary (`MEM_SC_*`) as its summary, `llm_model: "fixture"`.
//!   The format has no plan, and the plan writer is what Q18 measures (`main`'s `ask_plan`).
//!   The primary problem resolves by name and kind among the plant's species' problems, each
//!   organism by name (unknown: an "other" organism). A scan whose primary problem isn't in the
//!   seed (bed 1's green peach aphid) is not written; [`Loaded::skipped_scans`] says why.
//!
//! Each scan's pipeline summary is also loaded with the memories, as before.

use std::collections::BTreeMap;

use anyhow::{Context as _, bail};
use serde::Deserialize;

use domain::{
    authz::{Editor, Owner, check_membership_of},
    chat::{MemoryKind, MemorySource, MemoryTopic},
    events::{ProblemKind, RunReason, Stage},
    garden::gardens::TimeZone,
    garden::plants::UpdatePlantRequest,
    id::{AuthId, GardenId, InviteCode, PlantId, ScanId, SpeciesId, UserId},
    jobs::{Capability, JobKind, JobSpec, PerceptionPayload, PerceptionStage},
    knowledge::{ProblemSeed, SpeciesSeed},
    ports::{
        ClaimAttempt, CompletionOutcome, EmbedMode, IdGen, IdGenExt as _, JobCompletion,
        KnowledgeSnapshot, Lease, MemoryStore, NewGarden, NewInvite, NewJob, NewMemory, NewScan,
        NewScanPlant, ObservationWrite, PlanWrite, ResolvedLocation, RunStart, StepOutput, Store,
        TextEmbedder,
    },
    records::{
        diagnosis::{
            CandidateScore, Citation, Diagnosis, DiagnosisOrganism, DiagnosisStatus, EvidenceBasis,
            WeatherFit,
        },
        observation::{
            KnowledgeRef, Observation, ObservationSource, ObservationStage, OrganismRole,
        },
        plan::{ActionPlan, PlanReason},
        weather::{Coordinates, LocationPrecision},
    },
    rules::{jobs::claim_policy, steps::PipelineStep},
    scan::{PlantStatus, ScanStatus},
    url::Stored,
    wire::{Confidence, Rated, Role, Run, Timestamp},
};

/// The whole file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GardenFile {
    /// `1`.
    pub format: u32,
    /// The time the evaluation runs at: the pinned clock.
    pub as_of: Timestamp,
    /// The garden.
    pub garden: Garden,
    /// Its members; the first owner creates the garden.
    pub members: Vec<Member>,
    /// Its plants.
    pub plants: Vec<Plant>,
    /// Its scans.
    pub scans: Vec<Scan>,
    /// Its memories.
    pub memories: Vec<Memory>,
}

/// `garden`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Garden {
    /// `G_MAPLE`.
    #[expect(
        dead_code,
        reason = "part of the format; the loader mints the garden's id"
    )]
    pub key: String,
    /// Its name.
    pub name: String,
    /// City only.
    pub location: Location,
    /// What the geocoder must derive.
    pub time_zone: String,
}

/// `garden.location`: a `GardenLocationInput` with a city.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Location {
    /// `"Guelph, ON"`.
    pub city: String,
}

/// `members[]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    /// `M_MARIA`.
    pub key: String,
    /// Their name.
    pub display_name: String,
    /// Their role.
    pub role: Role,
    /// The fixture's Slack id; for reading answers only, never stored.
    #[expect(dead_code, reason = "part of the format; never stored")]
    pub slack_user: Option<String>,
}

/// `plants[]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plant {
    /// `P_BED3`.
    pub key: String,
    /// `Bed 3`.
    pub name: String,
    /// The species, as in `SpeciesRef` without the id.
    pub species: SpeciesName,
    /// `confirmed`.
    #[expect(
        dead_code,
        reason = "a plant created with a species has it confirmed, as POST /plants does"
    )]
    pub species_status: String,
    /// Its status.
    pub status: PlantStatus,
    /// When it was created.
    #[expect(dead_code, reason = "the store sets a plant's creation time")]
    pub created_at: Timestamp,
}

/// `plants[].species`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeciesName {
    /// `Solanum lycopersicum`.
    pub scientific_name: String,
    /// `Tomato`.
    pub common_name: String,
}

/// `scans[]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scan {
    /// `SC_BED3`.
    pub key: String,
    /// The plant's key.
    pub plant: String,
    /// The member's key.
    pub created_by: String,
    /// When it was taken.
    pub created_at: Timestamp,
    /// Where its run ended: `diagnosed` or `planned`.
    pub status: ScanStatus,
    /// Its diagnosis.
    pub diagnosis: FixtureDiagnosis,
}

/// `scans[].diagnosis`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureDiagnosis {
    /// Its status.
    pub status: DiagnosisStatus,
    /// The primary problem; absent when healthy.
    #[serde(default)]
    pub primary: Option<FixtureProblem>,
    /// The organisms found.
    pub organisms: Vec<FixtureOrganism>,
}

/// `scans[].diagnosis.primary`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureProblem {
    /// `Early blight`.
    pub name: String,
    /// Its kind.
    pub kind: ProblemKind,
}

/// `scans[].diagnosis.organisms[]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureOrganism {
    /// `Seven-spotted lady beetle`.
    pub name: String,
    /// Its role.
    pub role: OrganismRole,
}

/// `memories[]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Memory {
    /// `MEM_SC_BED3`.
    pub key: String,
    /// The plant's key; absent for the garden.
    #[serde(default)]
    pub plant: Option<String>,
    /// The scan's key, for a pipeline summary; a stand-in plan's summary is its text.
    #[serde(default)]
    pub scan: Option<String>,
    /// Its kind.
    pub kind: MemoryKind,
    /// Its topic.
    pub topic: MemoryTopic,
    /// `pipeline` or `user`.
    pub source: MemorySource,
    /// The member's key, for a `user` memory.
    #[serde(default)]
    pub author: Option<String>,
    /// Shared with the garden.
    #[expect(
        dead_code,
        reason = "pipeline summaries and garden notes are always shared"
    )]
    pub shared: bool,
    /// Its text.
    pub text: String,
    /// When.
    pub created_at: Timestamp,
}

impl GardenFile {
    /// Parses and checks the format.
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let file: Self = serde_json::from_str(text)?;
        anyhow::ensure!(file.format == 1, "garden.json format {}", file.format);
        Ok(file)
    }
}

/// What the loader made, by key.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// The garden.
    pub garden: GardenId,
    /// Each member's user.
    pub users: BTreeMap<String, UserId>,
    /// Each member's auth id, to sign in as them.
    pub auth_ids: BTreeMap<String, AuthId>,
    /// The plants created without a species, as `"P_BED2: Brassica oleracea (Kale)"`.
    pub plants_without_species: Vec<String>,
    /// Each scan written, by key.
    pub scans: BTreeMap<String, ScanId>,
    /// The scans it couldn't write, as `"SC_BED1: why"`.
    pub skipped_scans: Vec<String>,
}

/// The coordinates of the cities a fixture may name (the geocoder isn't called).
fn city(name: &str) -> anyhow::Result<Coordinates> {
    match name {
        "Guelph, ON" => Ok(Coordinates {
            lat: 43.55,
            lon: -80.25,
        }),
        other => bail!("no coordinates for {other}: add the city to era-eval's garden.rs"),
    }
}

/// An invite code: 26 base32 letters, unique per member.
fn invite_code(n: usize) -> anyhow::Result<InviteCode> {
    let letter = char::from(b'A' + u8::try_from(n % 26)?);
    format!("{}{letter}", "A".repeat(25))
        .parse()
        .map_err(|e| anyhow::anyhow!("invite code: {e:?}"))
}

/// Writes the garden, its members, plants, scans and memories.
pub async fn load(
    file: &GardenFile,
    store: &dyn Store,
    memory: &dyn MemoryStore,
    embedder: &dyn TextEmbedder,
    ids: &dyn IdGen,
) -> anyhow::Result<Loaded> {
    let knowledge = store
        .load_knowledge()
        .await
        .context("reading the knowledge base")?;
    let (users, auth_ids) = users(file, store).await?;
    let garden = garden(file, store, ids, &users).await?;
    let mut plants_without_species = Vec::new();
    let mut species = BTreeMap::new();
    for p in &file.plants {
        let id = species_id(&knowledge.species, &p.species)?;
        if id.is_none() {
            plants_without_species.push(format!(
                "{}: {} ({})",
                p.key, p.species.scientific_name, p.species.common_name
            ));
        }
        species.insert(p.key.clone(), id);
    }
    let plants = plants(file, store, ids, &users, garden, &species).await?;
    let pieces = Pieces {
        store,
        memory,
        embedder,
        ids,
        users: &users,
        plants: &plants,
        garden,
    };
    let mut scans = BTreeMap::new();
    let mut skipped_scans = Vec::new();
    for s in &file.scans {
        let working = species.get(&s.plant).cloned().flatten();
        match resolve_diagnosis(&knowledge, s, working.as_ref()) {
            Ok(resolved) => {
                let id = scan(file, &pieces, s, working, &resolved)
                    .await
                    .with_context(|| format!("writing {}", s.key))?;
                scans.insert(s.key.clone(), id);
            }
            Err(why) => skipped_scans.push(format!("{}: {why}", s.key)),
        }
    }
    memories(file, &pieces).await?;
    Ok(Loaded {
        garden,
        users,
        auth_ids,
        plants_without_species,
        scans,
        skipped_scans,
    })
}

/// The knowledge base's species for a fixture's, as `tools/demo/demo_garden.py`'s `species_id`
/// finds it: the exact scientific name (several: the one whose first common name is the
/// fixture's); else the one species recorded by the genus alone with a name containing the
/// common name; else none. Several by the scientific name and none by the common name is an
/// error, as there.
pub fn species_id(
    species: &[SpeciesSeed],
    name: &SpeciesName,
) -> anyhow::Result<Option<SpeciesId>> {
    let sci = name.scientific_name.to_lowercase();
    let common = name.common_name.to_lowercase();
    let exact: Vec<&SpeciesSeed> = species
        .iter()
        .filter(|s| s.scientific_name.to_lowercase() == sci)
        .collect();
    if exact.is_empty() {
        let genus = sci.split_whitespace().next().unwrap_or_default();
        let by_genus: Vec<&SpeciesSeed> = species
            .iter()
            .filter(|s| {
                s.scientific_name.to_lowercase() == genus
                    && s.common_names
                        .iter()
                        .any(|n| n.to_lowercase().contains(&common))
            })
            .collect();
        return Ok(match by_genus.as_slice() {
            [one] => Some(one.id.clone()),
            _ => None,
        });
    }
    if let [one] = exact.as_slice() {
        return Ok(Some(one.id.clone()));
    }
    let named: Vec<&&SpeciesSeed> = exact
        .iter()
        .filter(|s| {
            s.common_names
                .first()
                .is_some_and(|n| n.to_lowercase() == common)
        })
        .collect();
    match named.as_slice() {
        [one] => Ok(Some(one.id.clone())),
        _ => bail!(
            "{} matches {}; none or several are named {}",
            name.scientific_name,
            exact
                .iter()
                .map(|s| s.id.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            name.common_name
        ),
    }
}

/// A scan's diagnosis with its names resolved against the knowledge base.
#[derive(Debug, Clone)]
pub struct Resolved {
    /// The primary problem, when there is one.
    pub problem: Option<ProblemSeed>,
    /// Each organism with its knowledge-base id, `None` for "other".
    pub organisms: Vec<(FixtureOrganism, Option<domain::id::OrganismId>)>,
    /// The primary problem's sources.
    pub citations: Vec<Citation>,
}

/// Resolves a scan's primary problem (by name and kind, among the plant's species' problems
/// when it has a species) and its organisms (by name). The reason when the scan can't be
/// written: a status the loader doesn't write, or a primary problem the seed doesn't hold.
pub fn resolve_diagnosis(
    knowledge: &KnowledgeSnapshot,
    scan: &Scan,
    species: Option<&SpeciesId>,
) -> Result<Resolved, String> {
    if !matches!(scan.status, ScanStatus::Diagnosed | ScanStatus::Planned) {
        return Err(format!(
            "status {} isn't a finished run's; the loader writes diagnosed and planned scans",
            scan.status.as_str()
        ));
    }
    let problem = match &scan.diagnosis.primary {
        None => None,
        Some(primary) => Some(
            knowledge
                .problems
                .iter()
                .find(|p| {
                    p.name.eq_ignore_ascii_case(&primary.name)
                        && p.kind == primary.kind
                        && species.is_none_or(|s| p.affects.contains(s))
                })
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "the primary problem \"{}\" isn't in the knowledge base{}",
                        primary.name,
                        species.map_or_else(String::new, |s| format!(" for {s}"))
                    )
                })?,
        ),
    };
    let organisms = scan
        .diagnosis
        .organisms
        .iter()
        .map(|o| {
            let id = knowledge
                .organisms
                .iter()
                .find(|k| k.name.eq_ignore_ascii_case(&o.name))
                .map(|k| k.id.clone());
            (o.clone(), id)
        })
        .collect();
    let citations = problem
        .iter()
        .flat_map(|p| &p.sources)
        .filter_map(|id| knowledge.sources.iter().find(|s| &s.id == id))
        .map(|s| Citation {
            source_id: s.id.clone(),
            title: s.title.clone(),
            publisher: s.publisher.clone(),
            url: s.url.clone(),
        })
        .collect();
    Ok(Resolved {
        problem,
        organisms,
        citations,
    })
}

/// The owner: the first member with that role.
pub fn owner(file: &GardenFile) -> anyhow::Result<&Member> {
    file.members
        .iter()
        .find(|m| m.role == Role::Owner)
        .context("garden.json has no owner")
}

type Users = BTreeMap<String, UserId>;

/// A user for every member, as signing in first creates it.
async fn users(
    file: &GardenFile,
    store: &dyn Store,
) -> anyhow::Result<(Users, BTreeMap<String, AuthId>)> {
    let mut users = BTreeMap::new();
    let mut auth_ids = BTreeMap::new();
    for m in &file.members {
        let auth: AuthId = format!("user_{}", m.key.to_lowercase())
            .parse()
            .map_err(|e| anyhow::anyhow!("auth id for {}: {e:?}", m.key))?;
        let user = store
            .upsert_user(&auth, Some(m.display_name.clone()))
            .await?;
        users.insert(m.key.clone(), user);
        auth_ids.insert(m.key.clone(), auth);
    }
    Ok((users, auth_ids))
}

/// The garden, created by its owner, and the other members through an invite each.
async fn garden(
    file: &GardenFile,
    store: &dyn Store,
    ids: &dyn IdGen,
    users: &Users,
) -> anyhow::Result<GardenId> {
    let owner = owner(file)?;
    let garden: GardenId = ids.id();
    store
        .create_garden(
            users[&owner.key],
            NewGarden {
                id: garden,
                name: file.garden.name.clone(),
                location: ResolvedLocation {
                    coordinates: city(&file.garden.location.city)?,
                    precision: LocationPrecision::City,
                    label: file.garden.location.city.clone(),
                    time_zone: TimeZone(file.garden.time_zone.clone()),
                },
            },
        )
        .await?;
    let owns = check_membership_of::<Owner>(store, users[&owner.key], garden).await?;
    for (n, m) in file.members.iter().enumerate() {
        if m.key == owner.key {
            continue;
        }
        let code = invite_code(n)?;
        store
            .create_invite(
                owns,
                NewInvite {
                    id: ids.id(),
                    code: code.clone(),
                    role: m.role,
                    expires_at: file.as_of.plus(std::time::Duration::from_hours(24)),
                },
            )
            .await?;
        store
            .accept_invite(users[&m.key], &code, file.as_of)
            .await?;
    }
    Ok(garden)
}

/// The plants, created by the owner with their species, when the seed has it, and their status.
async fn plants(
    file: &GardenFile,
    store: &dyn Store,
    ids: &dyn IdGen,
    users: &Users,
    garden: GardenId,
    species: &BTreeMap<String, Option<SpeciesId>>,
) -> anyhow::Result<BTreeMap<String, PlantId>> {
    let editor = check_membership_of::<Editor>(store, users[&owner(file)?.key], garden).await?;
    let mut plants = BTreeMap::new();
    for p in &file.plants {
        let id: PlantId = ids.id();
        store
            .create_plant(
                editor,
                domain::ports::NewPlant {
                    id,
                    name: p.name.clone(),
                    species: species.get(&p.key).cloned().flatten(),
                },
            )
            .await?;
        store
            .update_plant(
                editor,
                id,
                UpdatePlantRequest {
                    name: None,
                    status: Some(p.status),
                },
            )
            .await?;
        plants.insert(p.key.clone(), id);
    }
    Ok(plants)
}

/// The worker id the loader claims its scans' `perception` jobs as.
const WORKER: &str = "era-eval-loader";

/// How long after a scan was taken its run finished.
const RUN_TIME: std::time::Duration = std::time::Duration::from_mins(10);

/// The run's first job, as submit queues it (no images: the fixture has no photos).
fn perception_job(
    job: domain::id::JobId,
    scan: ScanId,
    species: Option<SpeciesId>,
    at: Timestamp,
) -> NewJob {
    NewJob {
        id: job,
        scan,
        run: Some(Run(1)),
        question: None,
        spec: JobSpec::Perception(PerceptionPayload {
            images: Vec::new(),
            stages: vec![PerceptionStage::PlantCheck],
            species_id: species,
            skip_species_identification: false,
            regions: None,
            label_set_version: "fixture".into(),
            classifier_labels_version: "fixture".into(),
            question_id: None,
        }),
        max_runtime: JobKind::Perception.max_runtime(false),
        queued_at: at,
    }
}

/// One scan, along a run's path through the ports (see the module): created by its member,
/// submitted, its `perception` job claimed and completed with the organisms' observations and a
/// hand-over to `score`, the diagnosis committed, and for a `planned` scan the stand-in plan.
async fn scan(
    file: &GardenFile,
    p: &Pieces<'_>,
    s: &Scan,
    working_species: Option<SpeciesId>,
    resolved: &Resolved,
) -> anyhow::Result<ScanId> {
    let plant = *p
        .plants
        .get(&s.plant)
        .with_context(|| format!("no plant {}", s.plant))?;
    let user = *p
        .users
        .get(&s.created_by)
        .with_context(|| format!("no member {}", s.created_by))?;
    let editor = check_membership_of::<Editor>(p.store, user, p.garden).await?;
    let id: ScanId = p.ids.id();
    let at = s.created_at;
    let done = at.plus(RUN_TIME);
    p.store
        .create_scan(
            editor,
            NewScan {
                id,
                plant: NewScanPlant::Existing(plant),
                created_by: user,
                created_at: at,
            },
        )
        .await?;
    p.store
        .start_run(
            editor,
            id,
            RunStart {
                reason: RunReason::Submit,
                expected_status: ScanStatus::Pending,
                expected_run: Run(0),
                working_species: working_species.clone(),
                first_job: perception_job(p.ids.id(), id, working_species, at),
                now: at,
            },
        )
        .await?;

    let (scoring, organisms) = complete_perception(file, p, id, done, resolved).await?;

    // `score`: the diagnosis, then `plan` for a planned scan.
    let primary = resolved.problem.as_ref().map(|problem| CandidateScore {
        problem_id: problem.id.clone(),
        name: problem.name.clone(),
        kind: problem.kind,
        score: 0,
        matched: Vec::new(),
        contradicted: Vec::new(),
        unsure: Vec::new(),
        weather_fit: WeatherFit::Neutral,
    });
    let diagnosis = Diagnosis::<Stored> {
        id: p.ids.id(),
        scan_id: id,
        run: Run(1),
        supersedes: None,
        status: s.diagnosis.status,
        low_evidence: false,
        candidates: primary.iter().cloned().collect(),
        primary,
        organisms,
        soil: Vec::new(),
        risks: Vec::new(),
        evidence_basis: EvidenceBasis::default(),
        citations: resolved.citations.clone(),
        model_versions: BTreeMap::from([("fixture".to_owned(), file.garden.name.clone())]),
        created_at: done,
    };
    let planned = s.status == ScanStatus::Planned;
    let next = p
        .store
        .commit_step(
            scoring,
            StepOutput::Score {
                diagnosis: diagnosis.clone(),
                status: Some(ScanStatus::Diagnosed),
                resolved_question: None,
                needs_plan: planned,
            },
            done,
        )
        .await?;
    if planned {
        let planning = next.context("score didn't hand over to plan")?;
        stand_in_plan(file, p, s, (id, plant), &diagnosis, resolved, planning).await?;
    }
    Ok(id)
}

/// The run's `perception` job, claimed and completed with an observation per organism found,
/// handing over to `score`: the claim, and the diagnosis's organisms.
async fn complete_perception(
    file: &GardenFile,
    p: &Pieces<'_>,
    id: ScanId,
    done: Timestamp,
    resolved: &Resolved,
) -> anyhow::Result<(domain::ports::StepClaim, Vec<DiagnosisOrganism>)> {
    let at = done.minus(RUN_TIME);
    let worker: domain::id::WorkerId = WORKER
        .parse()
        .map_err(|e| anyhow::anyhow!("worker id: {e:?}"))?;
    let claimed_at = at.plus(std::time::Duration::from_secs(1));
    let claimed = p
        .store
        .claim_job(
            &worker,
            1,
            claim_policy(&[Capability::Perception], claimed_at),
            claimed_at,
        )
        .await?
        .context("the scan's perception job wasn't claimable")?;
    let lease = Lease {
        job: claimed.job.job_id,
        token: claimed.job.lease_token.clone(),
        worker,
    };
    let mut observations = Vec::new();
    let mut organisms = Vec::new();
    for (o, organism_id) in &resolved.organisms {
        let obs = p.ids.id();
        observations.push(Observation::<Stored> {
            id: obs,
            scan_id: id,
            run: Run(1),
            stage: ObservationStage::Organism,
            pipeline_stage: Stage::VlmFindings,
            label: o.name.clone(),
            knowledge_ref: organism_id.clone().map(KnowledgeRef::Organism),
            role: Some(o.role),
            detail: None,
            confidence: Rated::from(Confidence::saturating(0.9)),
            region: None,
            source: ObservationSource::Vlm,
            model_version: "fixture".into(),
            created_at: done,
        });
        organisms.push(DiagnosisOrganism {
            observation_id: obs,
            organism_id: organism_id.clone(),
            name: o.name.clone(),
            role: o.role,
            reporting_url: None,
            preys_on: None,
        });
    }
    let write = ObservationWrite {
        stage: Stage::VlmFindings,
        run: Run(1),
        images: Vec::new(),
        replace_imageless: true,
        observations,
        masks: Vec::new(),
    };
    let outcome = p
        .store
        .complete_job(
            &lease,
            JobCompletion::Done {
                result: serde_json::json!({ "fixture": file.garden.name }),
                observations: vec![write],
                evidence: Vec::new(),
                claim: Some(PipelineStep::Score),
            },
            // Within the lease: the run's later steps carry `done`.
            claimed_at.plus(std::time::Duration::from_secs(1)),
        )
        .await?;
    let CompletionOutcome::Stored {
        claim: ClaimAttempt::Claimed(scoring),
        ..
    } = outcome
    else {
        bail!("the perception job's completion didn't hand over to score: {outcome:?}");
    };

    Ok((scoring, organisms))
}

/// A `planned` scan's stand-in plan (see the module), committed with the `plan` claim.
async fn stand_in_plan(
    file: &GardenFile,
    p: &Pieces<'_>,
    s: &Scan,
    (id, plant): (ScanId, PlantId),
    diagnosis: &Diagnosis<Stored>,
    resolved: &Resolved,
    planning: domain::ports::StepClaim,
) -> anyhow::Result<()> {
    let done = diagnosis.created_at;
    let summary = file
        .memories
        .iter()
        .find(|m| m.scan.as_deref() == Some(s.key.as_str()))
        .map_or_else(
            || {
                resolved
                    .problem
                    .as_ref()
                    .map_or_else(|| "Looked healthy.".to_owned(), |p| p.name.clone())
            },
            |m| m.text.clone(),
        );
    let plan = ActionPlan {
        id: p.ids.id(),
        scan_id: id,
        plant_id: plant,
        version: 1,
        supersedes: None,
        provisional: diagnosis.status == DiagnosisStatus::Uncertain,
        diagnosis_id: diagnosis.id,
        open_question: None,
        constraints: Vec::new(),
        summary,
        steps: Vec::new(),
        watch_for: Vec::new(),
        citations: resolved.citations.clone(),
        farm_records: Vec::new(),
        task_ids: Vec::new(),
        llm_model: "fixture".into(),
        prompt_version: "fixture".into(),
        created_at: done,
    };
    p.store
        .commit_step(
            planning,
            StepOutput::Plan(Box::new(PlanWrite {
                plan,
                reason: PlanReason::Initial,
                expected: None,
                tasks: Vec::new(),
                triggers: Vec::new(),
            })),
            done,
        )
        .await?;
    Ok(())
}

/// What the scans and memories are written with.
struct Pieces<'a> {
    store: &'a dyn Store,
    memory: &'a dyn MemoryStore,
    embedder: &'a dyn TextEmbedder,
    ids: &'a dyn IdGen,
    users: &'a Users,
    plants: &'a BTreeMap<String, PlantId>,
    garden: GardenId,
}

/// The memories through the `MemoryStore` port, which keeps their kind and time: pipeline
/// summaries as the pipeline writes them, the rest as their author's garden notes.
async fn memories(file: &GardenFile, p: &Pieces<'_>) -> anyhow::Result<()> {
    let model = p.embedder.model().name;
    for m in &file.memories {
        let plant = match &m.plant {
            Some(key) => Some(
                *p.plants
                    .get(key)
                    .with_context(|| format!("no plant {key}"))?,
            ),
            None => None,
        };
        let embedding = p
            .embedder
            .embed(EmbedMode::Document, std::slice::from_ref(&m.text))
            .await?
            .pop()
            .context("no embedding")?;
        let new = NewMemory {
            id: p.ids.id(),
            plant,
            kind: m.kind,
            topic: m.topic,
            text: m.text.clone(),
            embedding,
            embedding_model: model.clone(),
            source_msg: None,
            created_at: m.created_at,
        };
        match m.source {
            MemorySource::Pipeline => {
                p.memory.add_pipeline_memory(p.garden, new).await?;
            }
            MemorySource::User => {
                let author = m
                    .author
                    .as_ref()
                    .and_then(|a| p.users.get(a))
                    .with_context(|| format!("{}: no author", m.key))?;
                let proof = check_membership_of::<Editor>(p.store, *author, p.garden).await?;
                p.memory.add_note(proof, new).await?;
            }
            MemorySource::Chat => bail!("{}: a chat memory isn't part of the format", m.key),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixture_parses() {
        let file = GardenFile::parse(include_str!(
            "../../../tests/fixtures/era/maple-row/garden.json"
        ))
        .unwrap();
        assert_eq!(file.plants.len(), 6);
        assert_eq!(file.memories.len(), 5);
        assert!(city(&file.garden.location.city).is_ok());
    }

    #[test]
    fn invite_codes_are_valid_and_distinct() {
        let a = invite_code(0).unwrap();
        let b = invite_code(1).unwrap();
        assert_ne!(a, b);
    }
}
