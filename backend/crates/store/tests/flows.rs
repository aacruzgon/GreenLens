//! The rest of the store against the embedded `SurrealDB` 3.3.4 engine: uploads, run starts,
//! the snapshot, job completion, guided checks, answers, reads, chat, memory, tasks, gardens,
//! deletions and the knowledge load. One datastore per test.

#[path = "embedded/support.rs"]
mod support;

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use domain::{
    authz::{Editor, Owner, Viewer, check_conversation, check_membership_of},
    chat::{
        Anchor, Card, ConfirmResult, ConversationQuery, MemoryKind, MemoryQuery, MemoryTopic,
        MessageFeedbackRequest, MessageRole, PendingAction, PendingStatus, Preview, Rating,
        WriteTool,
    },
    events::{RunReason, Stage},
    id::{
        AuthId, ChatImageId, GardenId, ImageId, JobId, LeaseToken, ObservationId, PlantId,
        QuestionId, TaskId,
    },
    jobs::{Capability, JobError, JobKind, JobSpec, PerceptionPayload, PerceptionStage},
    ports::{
        AccessRepo, AnalysisRepo, BeliefSubject, CascadeStep, ChatRepo, ClaimAttempt,
        CompletionOutcome, Conflict, ContentPart, DeletionRepo, DeletionTarget, Embedding,
        EventRepo, GardenRepo, GuidedOutcome, IdGenExt, IdentityRepo, ImageQuality, JobCompletion,
        JobRepo, KnowledgeRepo, Lease, MemoryStore, NewChatImage, NewConversation, NewInvite,
        NewJob, NewMemory, NewMessage, NewPendingAction, NewQuestion, NewScanImage, NewTask,
        ObjectKey, ObservationWrite, PlanRepo, PlantRepo, RunStart, ScanRepo, StepOutput, StepRepo,
        StoreError,
    },
    records::{
        diagnosis::{EvidenceSource, IndicatorEvidence, Weight},
        observation::{Observation, ObservationSource, ObservationStage},
        task::TaskSource,
    },
    rules::{jobs::claim_policy, steps::PipelineStep},
    scan::{
        Answer, AnswerRequest, GuidedCheckStatus, ImageKind, Question, QuestionKind,
        QuestionStatus, ScanStatus,
    },
    url::Stored,
    wire::{BBox, Confidence, PageRequest, Rated, Region, Role, Run, Timestamp},
};
use store::SurrealStore;
use support::{Fixture, exec, fixture, rec, set_status, t0};

fn image(f: &Fixture, kind: ImageKind, at: Timestamp) -> NewScanImage {
    let id: ImageId = f.ids.id();
    NewScanImage {
        id,
        kind,
        question: None,
        width: 1024,
        height: 768,
        storage_key: ObjectKey::scan_image(f.garden, f.scan, id),
        quality: ImageQuality {
            blur_score: 120.0,
            exposure: 0.5,
        },
        depth: None,
        photo_answer_start: None,
        uploaded_at: at,
    }
}

fn perception(f: &Fixture, id: JobId) -> NewJob {
    NewJob {
        id,
        scan: f.scan,
        run: Some(Run(1)),
        question: None,
        spec: JobSpec::Perception(PerceptionPayload {
            images: Vec::new(),
            stages: vec![PerceptionStage::PlantCheck],
            species_id: None,
            skip_species_identification: false,
            regions: None,
            label_set_version: "l1".into(),
            classifier_labels_version: "c1".into(),
            question_id: None,
        }),
        max_runtime: JobKind::Perception.max_runtime(false),
        queued_at: t0(),
    }
}

fn observation(
    f: &Fixture,
    id: ObservationId,
    image: ImageId,
    stage: Stage,
) -> Observation<Stored> {
    Observation {
        id,
        scan_id: f.scan,
        run: Run(1),
        stage: ObservationStage::PlantCheck,
        pipeline_stage: stage,
        label: "Leaf".into(),
        knowledge_ref: None,
        role: None,
        detail: None,
        confidence: Rated::from(Confidence::saturating(0.9)),
        region: Some(Region::unmasked(
            image,
            BBox::new(0.1, 0.1, 0.5, 0.5).expect("a valid box"),
            None,
        )),
        source: ObservationSource::Segmentation,
        model_version: "seg-1".into(),
        created_at: t0(),
    }
}

/// Uploads, the kind limit, a guided check with its job, the run start and the snapshot.
#[tokio::test]
async fn uploads_run_start_and_snapshot() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let whole = image(&f, ImageKind::WholePlant, t0());
    let added = store
        .add_image(f.editor, f.scan, whole.clone())
        .await
        .expect("first shot");
    assert_eq!(added.image.id, whole.id);
    let again = store
        .add_image(f.editor, f.scan, image(&f, ImageKind::WholePlant, t0()))
        .await;
    assert!(
        matches!(
            again,
            Err(StoreError::Rejected(
                domain::error::DomainError::TooManyImages { max: 1, .. }
            ))
        ),
        "{again:?}"
    );
    let squeeze = store
        .add_image(f.editor, f.scan, image(&f, ImageKind::SqueezeTest, t0()))
        .await
        .expect("guided check");
    assert!(squeeze.guided_check.is_some());

    let viewer = check_membership_of::<Viewer>(&*store, f.user, f.garden)
        .await
        .unwrap();
    let scan = store.scan(viewer, f.scan).await.expect("scan");
    assert_eq!(scan.images.len(), 2);
    assert_eq!(scan.guided_checks.len(), 1);
    assert_eq!(scan.guided_checks[0].status, GuidedCheckStatus::Analysing);
    assert_eq!(
        scan.created_by.as_ref().map(|u| u.display_name.as_str()),
        Some("Ada")
    );

    let started = store
        .start_run(
            f.editor,
            f.scan,
            RunStart {
                reason: RunReason::Submit,
                expected_status: ScanStatus::Pending,
                expected_run: Run(0),
                working_species: None,
                first_job: perception(&f, f.ids.id()),
                now: t0(),
            },
        )
        .await
        .expect("submit");
    assert_eq!(started.run, Run(1));
    assert_eq!(started.to, ScanStatus::Queued);
    let stale = store
        .start_run(
            f.editor,
            f.scan,
            RunStart {
                reason: RunReason::Submit,
                expected_status: ScanStatus::Pending,
                expected_run: Run(0),
                working_species: None,
                first_job: perception(&f, f.ids.id()),
                now: t0(),
            },
        )
        .await;
    assert!(
        matches!(stale, Err(StoreError::Rejected(_))),
        "a second submit from pending is refused: {stale:?}"
    );

    let snap = store.snapshot(f.scan).await.expect("snapshot");
    assert_eq!(snap.scan.run, Run(1));
    assert_eq!(snap.scan.status, ScanStatus::Queued);
    assert_eq!(snap.plant.name, "Tomato");
    assert_eq!(snap.garden.name, "Back yard");
    assert_eq!(snap.garden.weather_cell.to_string(), "43.65,-79.38");
    let stream = store.stream_state(f.scan).await.unwrap();
    assert!(stream.guided_check_analysing && !stream.step_claimed && !stream.question_unresolved);
    let detail = store.scan_detail(viewer, f.scan).await.expect("detail");
    assert_eq!(detail.plant.id, f.plant);
}

/// A suggested species the knowledge base doesn't hold (the stub worker's `tomato` with no seed
/// loaded, as on the rig on 2026-10-20) reads as no species, so the snapshot, the plant and the
/// plant lists don't fail on the dangling link, and the Score step that failed on it commits;
/// once the species exists it reads with its names.
#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "one scan's path: the suggestion, the reads, the score commit"
)]
async fn a_species_outside_the_knowledge_base_reads_as_none() {
    use domain::{
        id::SpeciesId,
        scan::{CandidateSource, SpeciesCandidate, SpeciesStatus},
    };
    let store = support::store().await;
    let f = fixture(&store).await;
    set_status(&store, f.scan, "analysing").await;
    let tomato: SpeciesId = "tomato".parse().unwrap();
    // `suggest_species` as the pipeline calls it after the fallback's findings: the stub's
    // BioCLIP pick and the VLM's `unknown`, working species `tomato`.
    store
        .suggest_species(
            f.plant,
            vec![
                SpeciesCandidate {
                    species_id: Some(tomato.clone()),
                    name: "tomato (stub)".into(),
                    score: Confidence::saturating(0.62),
                    source: CandidateSource::Bioclip,
                    reference_image: None,
                },
                SpeciesCandidate {
                    species_id: None,
                    name: "unknown".into(),
                    score: Confidence::saturating(0.4),
                    source: CandidateSource::Vlm,
                    reference_image: None,
                },
            ],
            Some(tomato.clone()),
            SpeciesStatus::Suggested,
        )
        .await
        .expect("suggested");

    let snap = store.snapshot(f.scan).await.expect("snapshot");
    assert_eq!(snap.plant.species, None);
    assert_eq!(snap.plant.species_status, SpeciesStatus::Suggested);
    assert_eq!(snap.plant.species_candidates.len(), 2);
    let viewer = check_membership_of::<Viewer>(&*store, f.user, f.garden)
        .await
        .unwrap();
    assert_eq!(
        store.plant(viewer, f.plant).await.expect("plant").species,
        None
    );
    let page = store
        .plants(viewer, PageRequest::default())
        .await
        .expect("plants");
    assert_eq!(page.items.len(), 1);
    assert_eq!(
        store.garden_plants(f.garden).await.expect("garden").len(),
        1
    );

    // The Score step that follows: no candidates (an empty knowledge base selects none), so an
    // uncertain, low-evidence diagnosis, committed with the hand-over to `plan`.
    let ClaimAttempt::Claimed(scoring) = store
        .claim_step(f.scan, PipelineStep::Score, t0())
        .await
        .expect("claim")
    else {
        panic!("the scan is free");
    };
    let diagnosis = domain::records::diagnosis::Diagnosis {
        id: f.ids.id(),
        scan_id: f.scan,
        run: Run(1),
        supersedes: None,
        status: domain::records::diagnosis::DiagnosisStatus::Uncertain,
        low_evidence: true,
        primary: None,
        candidates: Vec::new(),
        organisms: Vec::new(),
        soil: Vec::new(),
        risks: Vec::new(),
        evidence_basis: domain::records::diagnosis::EvidenceBasis::default(),
        citations: Vec::new(),
        model_versions: BTreeMap::from([
            ("bioclip".to_owned(), "stub".to_owned()),
            ("llm_fallback".to_owned(), "claude-opus-5-5".to_owned()),
        ]),
        created_at: t0(),
    };
    let plan = store
        .commit_step(
            scoring,
            StepOutput::Score {
                diagnosis: diagnosis.clone(),
                status: Some(ScanStatus::Diagnosed),
                resolved_question: None,
                needs_plan: true,
            },
            t0(),
        )
        .await
        .expect("score commits")
        .expect("hands over to plan");
    assert_eq!(plan.step, PipelineStep::Plan);
    let snap = store.snapshot(f.scan).await.expect("snapshot");
    assert_eq!(snap.scan.status, ScanStatus::Diagnosed);
    assert_eq!(snap.diagnosis.map(|d| d.id), Some(diagnosis.id));

    exec(
        &store,
        "CREATE species:tomato SET scientific_name = 'Solanum lycopersicum', common_names = ['Tomato'], profile = 'full',
           classifier_supported = true, bioclip_label = 'tomato', sources = [], reviewed_by = 'ada';",
    )
    .await;
    let species = store
        .snapshot(f.scan)
        .await
        .expect("snapshot")
        .plant
        .species
        .expect("the species exists now");
    assert_eq!(species.id, tomato);
    assert_eq!(species.scientific_name, "Solanum lycopersicum");
    assert_eq!(species.common_name, "Tomato");
}

/// A user's write that sets a species must name one the knowledge base holds: `POST /plants`,
/// `POST /plants/{id}/species`, a new plant of `POST /scans` and a new plant of a map upload
/// refuse any other with `invalid_request` at the request's field, and store nothing. The
/// pipeline's own `suggest_species` isn't checked (the test above).
#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "the four writes, each refused and then accepted"
)]
async fn user_writes_refuse_a_species_outside_the_knowledge_base() {
    use domain::{
        error::DomainError,
        garden::map::{GardenMapInput, PinInput},
        id::SpeciesId,
        ports::{MapRepo, MapWrite, NewPlant, NewScan, NewScanPlant},
        scan::{NewPlantInput, SpeciesChoice, SpeciesStatus},
    };
    fn refused_at(result: &Result<impl std::fmt::Debug, StoreError>, at: &str) {
        assert!(
            matches!(result, Err(StoreError::Rejected(DomainError::InvalidRequest { field, .. })) if field == at),
            "{result:?}"
        );
    }
    let store = support::store().await;
    let f = fixture(&store).await;
    let nosuch: SpeciesId = "nosuch".parse().unwrap();
    let tomato: SpeciesId = "tomato".parse().unwrap();
    exec(
        &store,
        "CREATE species:tomato SET scientific_name = 'Solanum lycopersicum', common_names = ['Tomato'], profile = 'full',
           classifier_supported = true, bioclip_label = 'tomato', sources = [], reviewed_by = 'ada';",
    )
    .await;
    let count = |table: &'static str| {
        let store = Arc::clone(&store);
        async move {
            let mut r = exec(
                &store,
                &format!("RETURN count(SELECT VALUE id FROM {table});"),
            )
            .await;
            r.take::<Option<i64>>(0).unwrap().unwrap_or(0)
        }
    };

    // POST /plants.
    let plant: PlantId = f.ids.id();
    let new_plant = |species: &SpeciesId| NewPlant {
        id: plant,
        name: "Basil".into(),
        species: Some(species.clone()),
    };
    refused_at(
        &store.create_plant(f.editor, new_plant(&nosuch)).await,
        "species_id",
    );
    assert_eq!(count("plant").await, 1, "only the fixture's plant");
    let created = store
        .create_plant(f.editor, new_plant(&tomato))
        .await
        .expect("a known species");
    assert_eq!(created.species.map(|s| s.id), Some(tomato.clone()));
    assert_eq!(created.species_status, SpeciesStatus::Confirmed);

    // POST /plants/{id}/species.
    refused_at(
        &store
            .confirm_species(f.editor, f.plant, SpeciesChoice::Known(nosuch.clone()))
            .await,
        "species_id",
    );
    let viewer = check_membership_of::<Viewer>(&*store, f.user, f.garden)
        .await
        .unwrap();
    let unchanged = store.plant(viewer, f.plant).await.expect("plant");
    assert_eq!(unchanged.species_status, SpeciesStatus::Unknown);
    let confirmed = store
        .confirm_species(f.editor, f.plant, SpeciesChoice::Known(tomato.clone()))
        .await
        .expect("a known species");
    assert_eq!(confirmed.plant.species.map(|s| s.id), Some(tomato.clone()));
    assert_eq!(confirmed.recent_scan.map(|r| r.scan), Some(f.scan));
    store
        .confirm_species(f.editor, f.plant, SpeciesChoice::Custom("Mystery".into()))
        .await
        .expect("a custom name names no species");

    // POST /scans with a new plant.
    let new_scan = |species: &SpeciesId| NewScan {
        id: f.ids.id(),
        plant: NewScanPlant::New {
            id: f.ids.id(),
            plant: NewPlantInput {
                name: "Pepper".into(),
                species_id: Some(species.clone()),
            },
        },
        created_by: f.user,
        created_at: t0(),
    };
    refused_at(
        &store.create_scan(f.editor, new_scan(&nosuch)).await,
        "new_plant.species_id",
    );
    assert_eq!(count("scan").await, 1, "nothing stored");
    assert_eq!(count("plant").await, 2);
    store
        .create_scan(f.editor, new_scan(&tomato))
        .await
        .expect("a known species");

    // PUT /gardens/{id}/map with a new plant.
    let map = |species: &SpeciesId| MapWrite {
        upload: GardenMapInput {
            base_version: 0,
            beds: Vec::new(),
            pins: vec![PinInput {
                plant_id: None,
                new_plant: Some(NewPlantInput {
                    name: "Squash".into(),
                    species_id: Some(species.clone()),
                }),
                x_m: 1.0,
                y_m: 2.0,
                canopy_radius_m: None,
                bed_id: None,
            }],
            north_deg: None,
        }
        .validate()
        .expect("a valid upload"),
        new_plant_ids: vec![f.ids.id()],
        created_at: t0(),
    };
    refused_at(&store.put_map(f.editor, map(&nosuch)).await, "pins");
    assert_eq!(count("plant").await, 3, "nothing stored");
    assert_eq!(count("garden_map").await, 0);
    let stored = store
        .put_map(f.editor, map(&tomato))
        .await
        .expect("a known species");
    assert_eq!(stored.version, 1);
    assert_eq!(count("plant").await, 4);
}

fn lease_of(claimed: &domain::ports::ClaimedJob, worker: &str) -> Lease {
    Lease {
        job: claimed.job.job_id,
        token: claimed.job.lease_token.clone(),
        worker: worker.parse().expect("a worker id"),
    }
}

/// `complete_job`: stored with its observations, diff and claim; a repeat; a stale lease; a
/// retryable failure requeued; a guided job out of attempts failing its check.
#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one flow, step by step")]
async fn complete_job_paths() {
    let store = support::store().await;
    let f = fixture(&store).await;
    set_status(&store, f.scan, "analysing").await;
    let whole = image(&f, ImageKind::WholePlant, t0());
    exec(
        &store,
        &format!("UPDATE {} SET status = 'pending';", rec("scan", f.scan)),
    )
    .await;
    store
        .add_image(f.editor, f.scan, whole.clone())
        .await
        .unwrap();
    set_status(&store, f.scan, "analysing").await;
    store.enqueue_job(perception(&f, f.ids.id())).await.unwrap();
    let now = t0().plus(Duration::from_secs(1));
    let worker = "rig-3070";
    let claimed = store
        .claim_job(
            &worker.parse().unwrap(),
            1,
            claim_policy(&[Capability::Perception], now),
            now,
        )
        .await
        .unwrap()
        .expect("claimed");
    let lease = lease_of(&claimed, worker);
    let obs_id: ObservationId = f.ids.id();
    let write = ObservationWrite {
        stage: Stage::PlantCheck,
        run: Run(1),
        images: vec![whole.id],
        replace_imageless: true,
        observations: vec![observation(&f, obs_id, whole.id, Stage::PlantCheck)],
        masks: Vec::new(),
    };
    let diff = store
        .store_partial(&lease, write.clone())
        .await
        .expect("partial");
    assert_eq!(diff.added.len(), 1);
    let outcome = store
        .complete_job(
            &lease,
            JobCompletion::Done {
                result: serde_json::json!({ "plant_present": true }),
                observations: vec![write],
                evidence: Vec::new(),
                claim: Some(PipelineStep::Retrieve),
            },
            now,
        )
        .await
        .expect("complete");
    let CompletionOutcome::Stored {
        claim: ClaimAttempt::Claimed(c),
        diff,
    } = outcome
    else {
        panic!("stored with a claim: {outcome:?}");
    };
    assert_eq!(c.step, PipelineStep::Retrieve);
    assert!(
        diff.added.is_empty() && diff.retracted.is_empty(),
        "the final result matched the partial"
    );
    assert_eq!(store.observations(f.scan).await.unwrap().len(), 1);
    assert_eq!(
        store.check_lease(&lease, now).await.unwrap().job,
        lease.job,
        "a finished lease still checks"
    );
    assert_eq!(
        store
            .complete_job(&lease, JobCompletion::Failed { error: err(true) }, now)
            .await
            .unwrap(),
        CompletionOutcome::AlreadyCompleted
    );
    let wrong = Lease {
        token: "01JAX3K9T6R2B8V0QZ4M7D1E5F".parse::<LeaseToken>().unwrap(),
        ..lease.clone()
    };
    assert!(matches!(
        store
            .complete_job(&wrong, JobCompletion::Failed { error: err(true) }, now)
            .await,
        Err(StoreError::Conflict(Conflict::LeaseLost { .. }))
    ));

    // A guided job: a retryable failure is requeued, until it runs out and fails its check.
    let squeeze = image(&f, ImageKind::SqueezeTest, now);
    store.add_image(f.editor, f.scan, squeeze).await.unwrap();
    let vlm = "rig-5070";
    let mut at = now;
    let mut outcomes = Vec::new();
    for _ in 0..3 {
        at = at.plus(Duration::from_secs(1));
        let job = store
            .claim_job(
                &vlm.parse().unwrap(),
                1,
                claim_policy(&[Capability::Vlm], at),
                at,
            )
            .await
            .unwrap()
            .expect("guided job");
        outcomes.push(
            store
                .complete_job(
                    &lease_of(&job, vlm),
                    JobCompletion::Failed { error: err(true) },
                    at,
                )
                .await
                .unwrap(),
        );
    }
    assert!(matches!(outcomes[0], CompletionOutcome::Requeued));
    assert!(
        matches!(outcomes[2], CompletionOutcome::FailedForGood { .. }),
        "{outcomes:?}"
    );
    let mut r = exec(&store, "SELECT VALUE status FROM guided_check;").await;
    assert_eq!(r.take::<Vec<String>>(0).unwrap(), vec!["failed".to_owned()]);
}

fn err(retryable: bool) -> JobError {
    JobError {
        code: "model_error".into(),
        message: "boom".into(),
        retryable,
    }
}

/// A guided result on a `pending` scan: the check is done, nothing is claimed, and the scan
/// shows the texture observation.
#[tokio::test]
async fn guided_check_finishes() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let squeeze = image(&f, ImageKind::SqueezeTest, t0());
    store
        .add_image(f.editor, f.scan, squeeze.clone())
        .await
        .unwrap();
    let vlm = "rig-5070";
    let now = t0().plus(Duration::from_secs(1));
    let job = store
        .claim_job(
            &vlm.parse().unwrap(),
            1,
            claim_policy(&[Capability::Vlm], now),
            now,
        )
        .await
        .unwrap()
        .expect("guided job");
    let mut texture = observation(&f, f.ids.id(), squeeze.id, Stage::Guided);
    texture.stage = ObservationStage::Soil;
    let finish = store
        .finish_guided_check(
            &lease_of(&job, vlm),
            GuidedOutcome {
                status: GuidedCheckStatus::Done,
                observation: Some(texture.clone()),
            },
            now,
        )
        .await
        .expect("finished");
    assert_eq!(finish.scan_status, ScanStatus::Pending);
    assert_eq!(finish.claim, ClaimAttempt::NotNeeded);
    let viewer = check_membership_of::<Viewer>(&*store, f.user, f.garden)
        .await
        .unwrap();
    let scan = store.scan(viewer, f.scan).await.unwrap();
    assert_eq!(scan.guided_checks[0].status, GuidedCheckStatus::Done);
    assert_eq!(
        scan.guided_checks[0].result.as_ref().map(|o| o.id),
        Some(texture.id)
    );
}

/// `score` asks; the answer stores the user's evidence, resolves the question and claims
/// `score` again; the snapshot reads the evidence.
#[tokio::test]
async fn ask_and_answer() {
    let store = support::store().await;
    let f = fixture(&store).await;
    set_status(&store, f.scan, "analysing").await;
    let ClaimAttempt::Claimed(score_claim) = store
        .claim_step(f.scan, PipelineStep::Score, t0())
        .await
        .unwrap()
    else {
        panic!("claim");
    };
    let q: QuestionId = f.ids.id();
    let indicator: domain::id::IndicatorId =
        "tomato_early_blight.concentric_rings".parse().unwrap();
    let question = Question {
        id: q,
        run: Run(1),
        kind: QuestionKind::Indicator,
        text: "Do the spots have rings?".into(),
        options: None,
        photo_kind: None,
        photo_hint: None,
        candidates: vec!["tomato_early_blight".parse().unwrap()],
        asked_at: t0(),
        continue_at: t0().plus(Duration::from_mins(10)),
        status: QuestionStatus::Open,
    };
    let next = store
        .commit_step(
            score_claim,
            StepOutput::Ask {
                question: NewQuestion {
                    question,
                    indicator: Some(indicator.clone()),
                },
                resolved_question: None,
            },
            t0(),
        )
        .await
        .unwrap();
    assert_eq!(next, None);
    let state = store.stream_state(f.scan).await.unwrap();
    assert!(state.question_unresolved);
    let snap = store.snapshot(f.scan).await.unwrap();
    assert_eq!(snap.scan.status, ScanStatus::AwaitingAnswer);
    assert_eq!(snap.open_question_indicator, Some(indicator.clone()));
    assert_eq!(snap.questions_asked, 1);
    // Due after 10 minutes.
    let due = store
        .questions_due(t0().plus(Duration::from_mins(11)))
        .await
        .unwrap();
    assert_eq!(due.len(), 1);

    let evidence = IndicatorEvidence {
        indicator_id: indicator,
        text: "Concentric rings".into(),
        weight: Weight::new(3).unwrap(),
        answer: Answer::Yes,
        reason: "user".into(),
        source: EvidenceSource::User,
        measured: None,
    };
    let outcome = store
        .answer_question(
            f.editor,
            f.scan,
            AnswerRequest {
                question_id: q,
                option_id: Answer::Yes,
            },
            vec![evidence],
            t0(),
        )
        .await
        .expect("answered");
    assert!(!outcome.late);
    assert!(matches!(outcome.claim, ClaimAttempt::Claimed(_)));
    let again = store
        .answer_question(
            f.editor,
            f.scan,
            AnswerRequest {
                question_id: q,
                option_id: Answer::No,
            },
            Vec::new(),
            t0(),
        )
        .await;
    assert!(
        matches!(again, Err(StoreError::Rejected(_))),
        "an answered question refuses another: {again:?}"
    );
    let snap = store.snapshot(f.scan).await.unwrap();
    assert_eq!(snap.evidence.len(), 1);
    assert!(snap.scan.open_question.is_none());
}

/// A pending scan deletes in order, and a submitted one is refused.
#[tokio::test]
async fn pending_scan_deletion() {
    let store = support::store().await;
    let f = fixture(&store).await;
    store
        .add_image(f.editor, f.scan, image(&f, ImageKind::WholePlant, t0()))
        .await
        .unwrap();
    store
        .add_image(f.editor, f.scan, image(&f, ImageKind::JarTest, t0()))
        .await
        .unwrap();
    let started = store
        .start_scan_deletion(f.editor, f.scan, t0())
        .await
        .expect("started");
    assert_eq!(started.prefixes.len(), 1);
    let target = DeletionTarget::PendingScan(f.scan);
    for step in CascadeStep::steps_for(target) {
        let mut guard = 0;
        while store
            .delete_batch(target, *step)
            .await
            .unwrap_or_else(|e| panic!("{step:?}: {e:?}"))
            > 0
        {
            guard += 1;
            assert!(guard < 20);
        }
    }
    for table in ["scan", "scan_image", "guided_check", "job"] {
        let mut r = exec(
            &store,
            &format!("RETURN count(SELECT VALUE id FROM {table});"),
        )
        .await;
        assert_eq!(r.take::<Option<i64>>(0).unwrap(), Some(0), "{table}");
    }

    let f2 = {
        let scan2 = f.ids.id();
        store
            .create_scan(
                f.editor,
                domain::ports::NewScan {
                    id: scan2,
                    plant: domain::ports::NewScanPlant::Existing(f.plant),
                    created_by: f.user,
                    created_at: t0(),
                },
            )
            .await
            .unwrap();
        scan2
    };
    set_status(&store, f2, "queued").await;
    let refused = store.start_scan_deletion(f.editor, f2, t0()).await;
    assert!(
        matches!(&refused, Err(StoreError::Rejected(domain::error::DomainError::InvalidState(d))) if d.status.as_deref() == Some("queued")),
        "{refused:?}"
    );
}

/// Gardens, invites, members, roles and the account-deletion plan.
#[tokio::test]
async fn gardens_members_and_account_plan() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let owner = check_membership_of::<Owner>(&*store, f.user, f.garden)
        .await
        .unwrap();
    let invite = store
        .create_invite(
            owner,
            NewInvite {
                id: f.ids.id(),
                code: "ABCDEFGHIJKLMNOPQRSTUVWXYZ".parse().unwrap(),
                role: Role::Editor,
                expires_at: t0().plus(Duration::from_hours(168)),
            },
        )
        .await
        .unwrap();
    let listed = store
        .invites(owner, PageRequest::default(), t0())
        .await
        .unwrap();
    assert_eq!(listed.items, vec![invite.clone()]);
    let bob = store
        .upsert_user(&"user_bob".parse::<AuthId>().unwrap(), Some("Bob".into()))
        .await
        .unwrap();
    let accepted = store.accept_invite(bob, &invite.code, t0()).await.unwrap();
    assert!(accepted.joined);
    assert!(
        matches!(
            store.accept_invite(bob, &invite.code, t0()).await,
            Err(StoreError::NotFound)
        ),
        "used"
    );
    let members = store
        .members(owner.at_least(), PageRequest::default())
        .await
        .unwrap();
    assert_eq!(members.items.len(), 2);
    let gardens = store.gardens(bob, PageRequest::default()).await.unwrap();
    assert_eq!(gardens.items.len(), 1);
    assert_eq!(gardens.items[0].member_count, 2);
    assert_eq!(gardens.items[0].my_role, Role::Editor);
    let demote = store.change_role(owner, f.user, Role::Editor).await;
    assert!(
        matches!(demote, Err(StoreError::Rejected(_))),
        "the last owner stays: {demote:?}"
    );

    let plan = store.plan_account_deletion(f.user).await.unwrap();
    assert_eq!(plan.transfer.len(), 1);
    assert_eq!(plan.transfer[0].new_owner_user_id, bob);
    store
        .transfer_ownership(plan.transfer[0], f.user)
        .await
        .unwrap();
    assert_eq!(
        store.role_in(bob, f.garden).await.unwrap(),
        Some(Role::Owner)
    );
    assert_eq!(store.me(bob).await.unwrap().display_name, "Bob");
}

/// Plants, tasks and the growth series.
#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one flow, step by step")]
async fn plants_tasks_and_growth() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let viewer = check_membership_of::<Viewer>(&*store, f.user, f.garden)
        .await
        .unwrap();
    let plants = store.plants(viewer, PageRequest::default()).await.unwrap();
    assert_eq!(plants.items.len(), 1);
    assert_eq!(
        plants.items[0].latest_scan.as_ref().map(|s| s.scan_id),
        Some(f.scan)
    );
    let timeline = store
        .timeline(viewer, f.plant, PageRequest::default())
        .await
        .unwrap();
    assert_eq!(timeline.items.len(), 1);
    assert!(timeline.items[0].measurements.is_none());

    let t: TaskId = f.ids.id();
    let task = store
        .create_task(
            f.editor,
            NewTask {
                id: t,
                plant: Some(f.plant),
                plan: None,
                plan_step: None,
                scan: Some(f.scan),
                title: "Photograph the jar".into(),
                detail: None,
                due: Some("2026-10-20".parse().unwrap()),
                repeat: None,
                source: TaskSource::User,
                trigger: None,
                alert_key: None,
                created_by: Some(f.user),
                created_at: t0(),
            },
        )
        .await
        .expect("task");
    assert_eq!(task.scan_id, Some(f.scan));
    let listed = store
        .tasks(
            viewer,
            domain::garden::tasks::TaskQuery {
                garden_id: f.garden,
                source: None,
                state: None,
            },
            PageRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(listed.items.len(), 1);
    let done = store
        .update_task(
            f.editor,
            t,
            domain::garden::tasks::UpdateTaskRequest {
                done: Some(true),
                dismissed: None,
                title: None,
                detail: None,
                due: None,
                repeat: None,
            },
            None,
            t0(),
        )
        .await
        .unwrap();
    assert!(done.done_at.is_some());
    store.delete_task(f.editor, t).await.unwrap();
    assert!(matches!(
        store.task(viewer, t).await,
        Err(StoreError::NotFound)
    ));

    exec(
        &store,
        &format!(
            "CREATE observation SET scan = {s}, run = 1, stage = 'measurement', pipeline_stage = 'measurement', label = 'Height',
               confidence = 0.85, source = 'measurement', model_version = 'm1', plant = {p}, quantity = 'plant_height_cm',
               measured_value = 42.0, taken_at = <datetime> '2026-10-18T14:00:00Z';",
            s = rec("scan", f.scan),
            p = rec("plant", f.plant)
        ),
    )
    .await;
    let series = store
        .growth_series(viewer, f.plant, None, PageRequest::default())
        .await
        .unwrap();
    assert_eq!(series.items.len(), 1);
    assert_eq!(store.heights(f.plant).await.unwrap().len(), 1);
    let timeline = store
        .timeline(viewer, f.plant, PageRequest::default())
        .await
        .unwrap();
    assert_eq!(
        timeline.items[0]
            .measurements
            .and_then(|m| m.plant_height_cm),
        Some(42.0)
    );
}

/// Chat: a conversation, messages with tool calls, a card confirmed twice, context, feedback.
#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one flow, step by step")]
async fn chat_flow() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let viewer = check_membership_of::<Viewer>(&*store, f.user, f.garden)
        .await
        .unwrap();
    let conv = store
        .create_conversation(
            viewer,
            NewConversation {
                id: f.ids.id(),
                user: f.user,
                anchor: Anchor::Plant(f.plant),
                created_at: t0(),
            },
        )
        .await
        .expect("conversation");
    assert_eq!(conv.anchor, Anchor::Plant(f.plant));
    // A `CurrentUser` only comes from `authenticate`, with a verified token.
    let mut verifier = domain::ports::MockAuthVerifier::new();
    verifier.expect_verify().returning(|_, now| {
        Ok(domain::ports::VerifiedToken {
            auth_id: "user_2abc".parse().unwrap(),
            display_name: None,
            expires_at: now,
        })
    });
    let user = domain::authz::authenticate(&verifier, &*store, "token", t0())
        .await
        .unwrap();
    assert_eq!(user.id(), f.user);
    let owner = check_conversation::<Editor>(&*store, &user, conv.id)
        .await
        .unwrap();
    let img: ChatImageId = f.ids.id();
    store
        .add_chat_image(
            owner.at_least(),
            NewChatImage {
                id: img,
                width: 10,
                height: 10,
                storage_key: ObjectKey::chat_image(f.garden, conv.id, img),
                uploaded_at: t0(),
            },
        )
        .await
        .unwrap();
    let msg = |role, text: &str, at: u64, parts: Vec<ContentPart>| NewMessage {
        id: f.ids.id(),
        role,
        text: text.into(),
        attachments: Vec::new(),
        citations: Vec::new(),
        farm_citations: Vec::new(),
        tool_calls: parts,
        route: None,
        interrupted: false,
        model: None,
        prompt_version: None,
        tokens_in: 1,
        tokens_out: 2,
        created_at: t0().plus(Duration::from_secs(at)),
    };
    let mut first = msg(MessageRole::User, "Why the spots?", 1, Vec::new());
    first.attachments = vec![img];
    store
        .store_message(owner.at_least(), first)
        .await
        .expect("user message");
    let answer = store
        .store_message(
            owner.at_least(),
            msg(
                MessageRole::Assistant,
                "Likely early blight.",
                2,
                vec![ContentPart::ToolCall {
                    call_id: "c1".into(),
                    name: "get_scan".into(),
                    arguments: serde_json::json!({}),
                }],
            ),
        )
        .await
        .unwrap();
    let card = PendingAction {
        id: f.ids.id(),
        conversation_id: conv.id,
        tool: WriteTool::CreateTask,
        status: PendingStatus::Proposed,
        card: Card {
            title: "Add a task?".into(),
            body: "Remove leaves".into(),
            confirm_label: "Add".into(),
            reject_label: "Not now".into(),
        },
        preview: Preview::Task {
            task: Box::new(domain::records::task::Task {
                id: f.ids.id(),
                garden_id: f.garden,
                plant_id: Some(f.plant),
                plan_id: None,
                plan_step_id: None,
                scan_id: None,
                title: "Remove leaves".into(),
                detail: None,
                due: None,
                repeat: None,
                source: TaskSource::Chat,
                trigger: None,
                done_at: None,
                dismissed_at: None,
                dismissed_reason: None,
                created_at: t0(),
            }),
        },
        expires_at: t0().plus(Duration::from_hours(24)),
    };
    store
        .create_pending_action(
            owner,
            NewPendingAction {
                action: card.clone(),
                message: answer.id,
            },
        )
        .await
        .unwrap();
    let confirmed = store
        .mark_confirmed(
            owner,
            card.id,
            ConfirmResult {
                task: None,
                plan: None,
            },
            t0(),
        )
        .await
        .unwrap();
    assert_eq!(confirmed.status, PendingStatus::Confirmed);
    let retry = store
        .mark_confirmed(
            owner,
            card.id,
            ConfirmResult {
                task: None,
                plan: None,
            },
            t0(),
        )
        .await
        .unwrap();
    assert_eq!(
        retry.status,
        PendingStatus::Confirmed,
        "a confirm retry keeps the result"
    );
    assert!(matches!(
        store.reject_pending_action(owner, card.id, t0()).await,
        Err(StoreError::Rejected(_))
    ));
    let (summary, context) = store.context(owner.at_least(), 10).await.unwrap();
    assert!(summary.is_none());
    assert_eq!(context.len(), 2);
    assert_eq!(
        context[1].tool_calls.len(),
        1,
        "oldest first, with the tool call"
    );
    assert_eq!(context[0].message.attachments.len(), 1);
    assert_eq!(store.message_count(owner.at_least()).await.unwrap(), 2);
    let detail = store
        .conversation_detail(owner.at_least(), PageRequest::default())
        .await
        .unwrap();
    assert_eq!(detail.messages.items.len(), 2);
    assert_eq!(detail.messages.items[0].pending_actions.len(), 1);
    store
        .rate_message(
            owner.at_least(),
            answer.id,
            MessageFeedbackRequest {
                rating: Rating::Up,
                notes: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        store.message_conversation(answer.id).await.unwrap(),
        Some(conv.id)
    );
    let list = store
        .conversations(
            viewer,
            ConversationQuery {
                garden_id: f.garden,
                anchor_type: None,
                anchor_id: None,
            },
            PageRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(list.items.len(), 1);
}

/// Memory: private chat memories stay private; notes are shared; search, share and delete.
#[tokio::test]
async fn memory_visibility() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let viewer = check_membership_of::<Viewer>(&*store, f.user, f.garden)
        .await
        .unwrap();
    let mem = |text: &str, v: Vec<f32>| NewMemory {
        id: f.ids.id(),
        plant: Some(f.plant),
        kind: MemoryKind::Fact,
        topic: MemoryTopic::Other,
        text: text.into(),
        embedding: Embedding(v),
        embedding_model: "e5".into(),
        source_msg: None,
        created_at: t0(),
    };
    let private = store
        .remember(viewer, mem("Waters on Sundays", vec![1.0, 0.0]))
        .await
        .unwrap();
    let note = store
        .add_note(f.editor, mem("Raised bed", vec![0.0, 1.0]))
        .await
        .unwrap();
    assert!(note.shared && !private.shared);
    let hits = store
        .search(viewer, &Embedding(vec![1.0, 0.0]), None, 5)
        .await
        .unwrap();
    assert_eq!(hits.first().map(|h| h.memory.id), Some(private.id));
    let page = store
        .memories(
            viewer,
            MemoryQuery {
                garden_id: f.garden,
                plant_id: None,
            },
            PageRequest::default(),
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(
        store
            .shared_for_plan(f.garden, f.plant)
            .await
            .unwrap()
            .len(),
        1
    );
    let shared = store.share(f.editor, private.id).await.unwrap();
    assert!(shared.shared);
    store
        .delete(viewer, note.id)
        .await
        .expect("an owner deletes a shared memory");
}

/// The belief sweep's rows: only shared, unsynced memories, oldest first, and the erase queue.
#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one flow, step by step")]
async fn memory_belief_sync() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let viewer = check_membership_of::<Viewer>(&*store, f.user, f.garden)
        .await
        .unwrap();
    let mem = |plant: Option<PlantId>, topic: MemoryTopic, text: &str, secs: u64| NewMemory {
        id: f.ids.id(),
        plant,
        kind: MemoryKind::Fact,
        topic,
        text: text.into(),
        embedding: Embedding(vec![1.0, 0.0]),
        embedding_model: "e5".into(),
        source_msg: None,
        created_at: t0().plus(Duration::from_secs(secs)),
    };
    let chat = store
        .remember(
            viewer,
            mem(Some(f.plant), MemoryTopic::Watering, "Waters on Sundays", 0),
        )
        .await
        .unwrap();
    assert_eq!(chat.topic, MemoryTopic::Watering, "topic round-trips");
    let pipeline = store
        .add_pipeline_memory(
            f.garden,
            mem(Some(f.plant), MemoryTopic::Health, "Scan: blight", 20),
        )
        .await
        .unwrap();
    let note = store
        .add_note(f.editor, mem(None, MemoryTopic::Location, "Raised bed", 10))
        .await
        .unwrap();
    assert_eq!(note.topic, MemoryTopic::Location);
    let ids = |v: Vec<domain::chat::Memory>| v.into_iter().map(|m| m.id).collect::<Vec<_>>();

    // A private chat memory never appears; the others come oldest first.
    assert_eq!(
        ids(store.unsynced_shared(10).await.unwrap()),
        vec![note.id, pipeline.id]
    );
    assert_eq!(ids(store.unsynced_shared(1).await.unwrap()), vec![note.id]);

    store.mark_synced(note.id, t0()).await.unwrap();
    assert_eq!(
        ids(store.unsynced_shared(10).await.unwrap()),
        vec![pipeline.id]
    );

    // `share` makes the chat memory appear, unsynced.
    store.share(f.editor, chat.id).await.unwrap();
    assert_eq!(
        ids(store.unsynced_shared(10).await.unwrap()),
        vec![chat.id, pipeline.id]
    );

    store.mark_synced(chat.id, t0()).await.unwrap();
    store.mark_synced(pipeline.id, t0()).await.unwrap();
    assert_eq!(ids(store.unsynced_shared(10).await.unwrap()), vec![]);

    assert_eq!(
        store.belief_forgets(10).await.unwrap(),
        Vec::<BeliefSubject>::new()
    );

    // Deleting a private memory queues nothing.
    let private = store
        .remember(viewer, mem(Some(f.plant), MemoryTopic::Other, "Secret", 30))
        .await
        .unwrap();
    store.delete(viewer, private.id).await.unwrap();
    assert_eq!(
        store.belief_forgets(10).await.unwrap(),
        Vec::<BeliefSubject>::new()
    );

    // Deleting a shared plant memory queues the plant, once, in the delete's transaction.
    let on_plant = BeliefSubject {
        garden: f.garden,
        plant: Some(f.plant),
    };
    store.delete(viewer, pipeline.id).await.unwrap();
    let extra = store
        .add_pipeline_memory(
            f.garden,
            mem(Some(f.plant), MemoryTopic::Health, "Scan: fine", 40),
        )
        .await
        .unwrap();
    store.mark_synced(extra.id, t0()).await.unwrap();
    store.delete(viewer, extra.id).await.unwrap();
    assert_eq!(store.belief_forgets(10).await.unwrap(), vec![on_plant]);

    // Finishing the erase brings back the plant's remaining shared memories, not the garden
    // note, and empties the queue.
    store.finish_belief_forget(on_plant).await.unwrap();
    assert_eq!(ids(store.unsynced_shared(10).await.unwrap()), vec![chat.id]);
    assert_eq!(
        store.belief_forgets(10).await.unwrap(),
        Vec::<BeliefSubject>::new()
    );
    store.mark_synced(chat.id, t0()).await.unwrap();

    // A garden-level memory's subject is the garden: finishing it brings back every shared
    // memory of the garden, plants included.
    let on_garden = BeliefSubject {
        garden: f.garden,
        plant: None,
    };
    let note2 = store
        .add_note(f.editor, mem(None, MemoryTopic::Light, "Full sun", 50))
        .await
        .unwrap();
    store.delete(viewer, note2.id).await.unwrap();
    assert_eq!(store.belief_forgets(10).await.unwrap(), vec![on_garden]);
    store.finish_belief_forget(on_garden).await.unwrap();
    assert_eq!(
        ids(store.unsynced_shared(10).await.unwrap()),
        vec![chat.id, note.id]
    );
    assert_eq!(
        store.belief_forgets(10).await.unwrap(),
        Vec::<BeliefSubject>::new()
    );
}

/// A belief subject: the plant, or the garden when `plant` is none.
const fn subject(garden: GardenId, plant: Option<PlantId>) -> BeliefSubject {
    BeliefSubject { garden, plant }
}

/// The knowledge load resolves edges into seed shapes.
#[tokio::test]
async fn knowledge_loads() {
    let store = support::store().await;
    exec(
        &store,
        "CREATE source:umn SET title = 'UMN', publisher = 'UMN Extension', url = 'https://extension.umn.edu', retrieved_at = '2026-10-01';
         CREATE species:tomato SET scientific_name = 'Solanum lycopersicum', common_names = ['Tomato'], profile = 'full',
           classifier_supported = true, bioclip_label = 'tomato', sources = [source:umn], reviewed_by = 'ada',
           reference_images = [{ storage_key: 'knowledge/species/tomato/0123456789abcdef.webp', credit: 'Photo: A', source: source:umn }];
         CREATE organism:aphid SET name = 'Aphid', scientific_name = 'Aphidoidea', taxon_group = 'insect', role = 'pest',
           recognition = 'small', bioclip_label = 'aphid', sources = [source:umn], reviewed_by = 'ada';
         CREATE organism:ladybug SET name = 'Ladybug', scientific_name = 'Coccinellidae', taxon_group = 'insect', role = 'beneficial',
           recognition = 'red', bioclip_label = 'ladybug', sources = [source:umn], reviewed_by = 'ada';
         RELATE organism:ladybug->preys_on->organism:aphid;
         CREATE disease:tomato_early_blight SET name = 'Early blight', kind = 'fungal', damage_patterns = [], prevalence = 'common',
           indicators = [], conditions = {}, sources = [source:umn], reviewed_by = 'ada', reviewed_at = '2026-10-01';
         RELATE species:tomato->susceptible_to->disease:tomato_early_blight;",
    )
    .await;
    let kb = store.load_knowledge().await.expect("loaded");
    assert_eq!(kb.species.len(), 1);
    assert_eq!(kb.species_photos[0].1.len(), 1);
    assert_eq!(kb.problems[0].affects.len(), 1);
    let ladybug = kb
        .organisms
        .iter()
        .find(|o| o.id.as_str() == "ladybug")
        .unwrap();
    assert_eq!(ladybug.preys_on.as_ref().map(Vec::len), Some(1));
    assert_eq!(kb.vlm_options.species.len(), 1);
    assert_eq!(kb.vlm_options.label_set_version.len(), 16);
}

#[allow(dead_code, reason = "keeps the shared imports used across tests")]
fn _types(_: Arc<SurrealStore>, _: GardenId, _: BTreeMap<u8, u8>) {}

/// Removing a shot, feedback refused while pending, failing a run, and a plant's cascade.
#[tokio::test]
async fn remove_fail_feedback_and_plant_deletion() {
    use domain::{
        id::FeedbackId,
        ports::NewFeedback,
        scan::{FeedbackRequest, ScanFailure, ScanFailureCode, Verdict},
    };
    let store = support::store().await;
    let f = fixture(&store).await;
    let shot = image(&f, ImageKind::LeafCloseup, t0());
    store
        .add_image(f.editor, f.scan, shot.clone())
        .await
        .unwrap();
    let keys = store
        .remove_image(f.editor, f.scan, shot.id)
        .await
        .expect("removed");
    assert_eq!(keys, vec![shot.storage_key.clone()]);

    let reviewer = check_membership_of::<domain::authz::Reviewer>(&*store, f.user, f.garden)
        .await
        .unwrap();
    let feedback = NewFeedback {
        id: f.ids.id::<domain::id::table::ScanFeedback>(),
        user: f.user,
        request: FeedbackRequest {
            verdict: Verdict::Correct,
            corrected_label: None,
            notes: None,
        },
    };
    assert!(matches!(
        store
            .record_feedback(reviewer, f.scan, feedback.clone())
            .await,
        Err(StoreError::Rejected(_))
    ));

    set_status(&store, f.scan, "analysing").await;
    let failed = store
        .fail_scan(
            f.scan,
            Run(1),
            ScanFailure {
                code: ScanFailureCode::ProcessingError,
                message: "attempts".into(),
                retryable: true,
            },
            t0(),
        )
        .await
        .unwrap();
    assert_eq!(failed.from, ScanStatus::Analysing);
    assert_eq!(
        store.stream_state(f.scan).await.unwrap().status,
        ScanStatus::Failed
    );
    let id: FeedbackId = store
        .record_feedback(reviewer, f.scan, feedback)
        .await
        .expect("feedback after submit");
    let _ = id;

    let owner = check_membership_of::<Owner>(&*store, f.user, f.garden)
        .await
        .unwrap();
    let started = store
        .start_plant_deletion(owner, f.plant, t0())
        .await
        .unwrap();
    assert_eq!(started.prefixes.len(), 1, "one scan prefix");
    let queued = vec![subject(f.garden, Some(f.plant))];
    assert_eq!(store.belief_forgets(10).await.unwrap(), queued, "queued");
    let target = DeletionTarget::Plant(f.plant);
    for step in CascadeStep::steps_for(target) {
        while store
            .delete_batch(target, *step)
            .await
            .unwrap_or_else(|e| panic!("{step:?}: {e:?}"))
            > 0
        {}
    }
    for table in ["plant", "scan", "scan_feedback"] {
        let mut r = exec(
            &store,
            &format!("RETURN count(SELECT VALUE id FROM {table});"),
        )
        .await;
        assert_eq!(r.take::<Option<i64>>(0).unwrap(), Some(0), "{table}");
    }
    let mut r = exec(&store, "RETURN count(SELECT VALUE id FROM garden);").await;
    assert_eq!(
        r.take::<Option<i64>>(0).unwrap(),
        Some(1),
        "the garden stays"
    );
    assert_eq!(store.belief_forgets(10).await.unwrap(), queued, "kept");
}

/// The account tombstone: one cascade, no sign-in back, the markers, finishing and cleanup.
#[tokio::test]
async fn account_tombstone() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let auth: AuthId = "user_2abc".parse().unwrap();
    assert!(
        store
            .start_account_deletion(f.user, &auth, t0())
            .await
            .unwrap(),
        "the first call starts it"
    );
    assert!(
        !store
            .start_account_deletion(f.user, &auth, t0())
            .await
            .unwrap(),
        "a second doesn't"
    );
    assert!(
        matches!(
            store.upsert_user(&auth, None).await,
            Err(StoreError::NotFound)
        ),
        "an existing user is refused"
    );
    let markers = store.unfinished_account_deletions().await.unwrap();
    assert_eq!(markers.len(), 1);
    assert!(!markers[0].user_deleted);
    // The cascade's last step deletes the user (its gardens first, as in the real order).
    let owner = check_membership_of::<Owner>(&*store, f.user, f.garden)
        .await
        .unwrap();
    store.start_garden_deletion(owner, t0()).await.unwrap();
    let target = DeletionTarget::Garden(f.garden);
    for step in CascadeStep::steps_for(target) {
        while store.delete_batch(target, *step).await.unwrap() > 0 {}
    }
    assert_eq!(
        store.belief_forgets(10).await.unwrap(),
        vec![BeliefSubject {
            garden: f.garden,
            plant: None
        }],
        "the garden's erase stays queued after its records are gone"
    );
    store.delete_user(f.user).await.unwrap();
    assert!(store.unfinished_account_deletions().await.unwrap()[0].user_deleted);
    assert!(
        matches!(
            store.upsert_user(&auth, None).await,
            Err(StoreError::NotFound)
        ),
        "no new user either"
    );
    store.finish_account_deletion(&auth, t0()).await.unwrap();
    assert_eq!(
        store.unfinished_account_deletions().await.unwrap(),
        [] as [domain::ports::AccountDeletionMarker; 0]
    );
    assert_eq!(
        store.delete_finished_tombstones(t0()).await.unwrap(),
        0,
        "kept for 24 hours"
    );
    assert_eq!(
        store
            .delete_finished_tombstones(t0().plus(Duration::from_hours(25)))
            .await
            .unwrap(),
        1
    );
}

/// `SoilGrids` retries: a failure waits until its retry time; `ready` clears it.
#[tokio::test]
async fn soil_profile_retries() {
    let store = support::store().await;
    let f = fixture(&store).await;
    assert_eq!(
        store
            .gardens_due_soil_profile(t0(), 10)
            .await
            .unwrap()
            .len(),
        1,
        "pending is due"
    );
    store
        .soil_profile_failed(f.garden, t0().plus(Duration::from_mins(15)))
        .await
        .unwrap();
    assert!(
        store
            .gardens_due_soil_profile(t0(), 10)
            .await
            .unwrap()
            .is_empty(),
        "not before its retry time"
    );
    let due = store
        .gardens_due_soil_profile(t0().plus(Duration::from_mins(15)), 10)
        .await
        .unwrap();
    assert_eq!(due.len(), 1);
    assert!((due[0].1.lat - 43.65).abs() < 1e-9);
    store
        .set_soil_profile(
            f.garden,
            domain::garden::gardens::SoilProfileStatus::Unavailable,
            None,
        )
        .await
        .unwrap();
    let mut r = exec(&store, "SELECT VALUE soil_retry_at FROM garden;").await;
    assert_eq!(
        r.take::<Vec<Option<chrono::DateTime<chrono::Utc>>>>(0)
            .unwrap()
            .len(),
        1
    );
}

/// `live_lease`: a live lease, a finished one (a late event, dropped), and a lost one; a
/// requeue wakes the claim long poll.
#[tokio::test]
async fn live_lease_and_requeue_wake() {
    let store = support::store().await;
    let f = fixture(&store).await;
    set_status(&store, f.scan, "analysing").await;
    store.enqueue_job(perception(&f, f.ids.id())).await.unwrap();
    let now = t0().plus(Duration::from_secs(1));
    let worker = "rig-3070";
    let claimed = store
        .claim_job(
            &worker.parse().unwrap(),
            1,
            claim_policy(&[Capability::Perception], now),
            now,
        )
        .await
        .unwrap()
        .expect("claimed");
    let lease = lease_of(&claimed, worker);
    assert!(store.live_lease(&lease, now).await.unwrap().is_some());

    // Expired: the scheduler requeues it and the long poll wakes.
    let notify = store.jobs_queued();
    let woken = tokio::spawn(async move { notify.notified().await });
    tokio::task::yield_now().await;
    let later = now.plus(Duration::from_secs(150));
    assert_eq!(store.requeue_stale(later).await.unwrap().len(), 1);
    tokio::time::timeout(Duration::from_secs(2), woken)
        .await
        .expect("woken")
        .unwrap();
    assert!(matches!(
        store.live_lease(&lease, later).await,
        Err(StoreError::Conflict(Conflict::LeaseLost { .. }))
    ));

    let again = store
        .claim_job(
            &worker.parse().unwrap(),
            1,
            claim_policy(&[Capability::Perception], later),
            later,
        )
        .await
        .unwrap()
        .expect("claimed again");
    let lease = lease_of(&again, worker);
    store
        .complete_job(
            &lease,
            JobCompletion::Done {
                result: serde_json::json!({}),
                observations: Vec::new(),
                evidence: Vec::new(),
                claim: None,
            },
            later,
        )
        .await
        .unwrap();
    assert_eq!(
        store.live_lease(&lease, later).await.unwrap(),
        None,
        "a late event after complete"
    );
}
