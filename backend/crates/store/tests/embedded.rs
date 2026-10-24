//! The store against the embedded `SurrealDB` 3.3.4 engine (`kv-mem`, a dev-dependency only).
//! Each test gets its own in-memory datastore with the schema applied.

#[path = "embedded/support.rs"]
mod support;

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use domain::{
    authz::{Viewer, check_membership_of},
    events::ScanEventBody,
    id::{IdempotencyKey, JobId, PlanId, QuestionId, StepId, TaskId},
    jobs::{Capability, Heartbeat, HeldJob, JobKind, JobSpec, PerceptionPayload, PerceptionStage},
    ports::{
        AnalysisRepo, ClaimAttempt, Conflict, DeletionRepo, EventRepo, IdGenExt, IdempotencyBegin,
        IdempotencyRepo, IdempotencyScope, JobRepo, NewEvent, NewJob, NewTask, PlanRepo, PlanWrite,
        QuestionAdvance, QuestionStepClaim, QuestionStepOutput, StepClaim, StepOutput, StepRepo,
        StoreError, StoredResponse,
    },
    records::{
        plan::{ActionPlan, PlanReason},
        task::TaskSource,
    },
    rules::{jobs::claim_policy, questions::QuestionStep, steps::PipelineStep},
    scan::ScanStatus,
    wire::{EventSeq, Run, Timestamp},
};

use support::{exec, fixture, rec, set_status, t0};

#[tokio::test]
async fn schema_applies_twice() {
    let store = support::store().await;
    store::apply_schema_with(store.db())
        .await
        .expect("second application");
    let mut r = exec(&store, "INFO FOR DB;").await;
    let info: Option<surrealdb::types::Value> = r.take(0).unwrap();
    let info = format!("{info:?}");
    for table in ["indicator_answer", "connector", "farm_record"] {
        assert!(info.contains(table), "{table} missing");
    }
}

/// T-E1.3 ([connectors 4]): a `farm_record` keyed `[connector, vendor_id, n]` with a dotted
/// Slack `ts` round-trips; a plan's `farm_records` (and a step's `farm_citations`) and a
/// message's `farm_citations` are stored by the `SCHEMAFULL` tables; and the garden cascade
/// leaves no connector and no farm record.
#[tokio::test]
#[expect(clippy::too_many_lines, reason = "one fixture per new table and field")]
async fn farm_connectors_store_and_go_with_their_garden() {
    use domain::{
        authz::Owner,
        connectors::{ConnectorKind, FarmCitation},
        id::{ConnectorId, ConversationId, FarmRecordId, MessageId},
        ports::{CascadeStep, DeletionTarget},
        records::plan::{PlanStep, PlanWhen, StepCategory},
    };
    use store::ids::{farm_record_id_of, farm_record_rid, rid};
    use surrealdb::types::RecordId;

    let store = support::store().await;
    let f = fixture(&store).await;
    let connector: ConnectorId = f.ids.id();
    let record = FarmRecordId::new(connector, "C024BE91L/1729000000.000100", 0).unwrap();

    // A connector and one of its records, written as the sync will (E1.6).
    let mut r = store
        .db()
        .query(
            "CREATE $connector SET garden = $garden, kind = 'slack', status = 'active',
                created_by = $user, endpoint = 'vendor', sources = [{ id: 'C024BE91L', name: 'field-reports' }],
                sync_requested_at = $now, last_error = 'rate_limited';
             CREATE $record SET garden = $garden, connector = $connector, kind = 'message',
                vendor_id = 'C024BE91L/1729000000.000100', source = 'C024BE91L', title = '#field-reports', author = 'Sam',
                link = 'https://maplerow.slack.com/archives/C024BE91L/p1729000000000100',
                text = 'Rings on bed 3 again.', text_hash = 'h1', embedding = [0.1, 0.2],
                embedding_model = 'test', at = $now;
             SELECT VALUE id FROM farm_record WHERE connector = $connector;",
        )
        .bind(("connector", rid(&connector)))
        .bind(("record", farm_record_rid(&record)))
        .bind(("garden", rid(&f.garden)))
        .bind(("user", rid(&f.user)))
        .bind(("now", t0().into_inner()))
        .await
        .unwrap();
    assert!(r.take_errors().is_empty());
    let ids: Vec<RecordId> = r.take(2).unwrap();
    assert_eq!(ids.len(), 1);
    assert_eq!(farm_record_id_of(&ids[0]).unwrap(), record);

    // A plan whose step cites the record, and the plan's resolved farm records.
    let citation = FarmCitation {
        record_id: record.clone(),
        system: ConnectorKind::Slack,
        title: "#field-reports".into(),
        author: Some("Sam".into()),
        at: t0(),
        link: "https://maplerow.slack.com/archives/C024BE91L/p1729000000000100".into(),
    };
    let citation_json = serde_json::to_value(&citation).unwrap();
    let v1: PlanId = f.ids.id();
    let mut p = plan(&f, v1, 1, None);
    p.steps = vec![PlanStep {
        id: "s1".parse().unwrap(),
        order: 1,
        title: "Check bed 3 for new rings".into(),
        detail: "The crew saw rings twice.".into(),
        treatment_id: None,
        category: StepCategory::Monitoring,
        when: PlanWhen::Now,
        targets: Vec::new(),
        plant_id: None,
        citations: vec!["example_extension_tomato_diseases".parse().unwrap()],
        farm_citations: vec![record.clone()],
        safety_note: None,
    }];
    store
        .write_revision(
            f.editor,
            PlanWrite {
                plan: p,
                reason: PlanReason::Initial,
                expected: None,
                tasks: Vec::new(),
                triggers: Vec::new(),
            },
        )
        .await
        .unwrap();
    let mut r = store
        .db()
        .query(
            "UPDATE $plan SET farm_records = [$citation];
             SELECT VALUE farm_records FROM ONLY $plan;",
        )
        .bind(("plan", rid(&v1)))
        .bind(("citation", citation_json.clone()))
        .await
        .unwrap();
    assert!(r.take_errors().is_empty());
    // An array takes as `Vec`: as `Option`, an array of one reads as its element.
    let stored: Vec<serde_json::Value> = r.take(1).unwrap();
    assert_eq!(stored, vec![citation_json.clone()]);
    let read = store.current_plan(f.scan).await.unwrap().expect("the plan");
    assert_eq!(read.steps[0].farm_citations, vec![record.clone()]);

    // A message citing the record.
    let conversation: ConversationId = f.ids.id();
    let message: MessageId = f.ids.id();
    let mut r = store
        .db()
        .query(
            "CREATE $conversation SET user = $user, garden = $garden, anchor = $garden;
             CREATE $message SET conversation = $conversation, role = 'assistant',
                text = 'Bed 3 had rings on Oct 12.', farm_citations = [$citation];
             SELECT VALUE farm_citations FROM ONLY $message;",
        )
        .bind(("conversation", rid(&conversation)))
        .bind(("message", rid(&message)))
        .bind(("garden", rid(&f.garden)))
        .bind(("user", rid(&f.user)))
        .bind(("citation", citation_json.clone()))
        .await
        .unwrap();
    assert!(r.take_errors().is_empty());
    let stored: Vec<serde_json::Value> = r.take(2).unwrap();
    let stored: Vec<FarmCitation> = stored
        .into_iter()
        .map(|c| serde_json::from_value(c).unwrap())
        .collect();
    assert_eq!(stored, vec![citation]);

    // A message stored before connectors gets the field's default.
    let old: MessageId = f.ids.id();
    let mut r = store
        .db()
        .query(
            "CREATE $message SET conversation = $conversation, role = 'user', text = 'Hi';
             SELECT VALUE farm_citations FROM ONLY $message;",
        )
        .bind(("conversation", rid(&conversation)))
        .bind(("message", rid(&old)))
        .await
        .unwrap();
    assert!(r.take_errors().is_empty());
    assert_eq!(
        r.take::<Vec<serde_json::Value>>(1).unwrap(),
        Vec::<serde_json::Value>::new()
    );

    // The garden goes, its records before its connector.
    let owner = check_membership_of::<Owner>(&*store, f.user, f.garden)
        .await
        .unwrap();
    store.start_garden_deletion(owner, t0()).await.unwrap();
    let target = DeletionTarget::Garden(f.garden);
    for step in CascadeStep::steps_for(target) {
        let mut guard = 0;
        while store
            .delete_batch(target, *step)
            .await
            .unwrap_or_else(|e| panic!("{step:?}: {e:?}"))
            > 0
        {
            guard += 1;
            assert!(guard < 50, "{step:?} never finished");
        }
    }
    for table in [
        "garden",
        "connector",
        "farm_record",
        "action_plan",
        "message",
    ] {
        let mut r = exec(
            &store,
            &format!("RETURN count(SELECT VALUE id FROM {table});"),
        )
        .await;
        assert_eq!(
            r.take::<Option<i64>>(0).unwrap(),
            Some(0),
            "{table} left behind"
        );
    }
}

/// [database 4] scan events: concurrent appends conflict on the scan row, the store retries
/// them, and the stored `seq` is exactly 1…n with no gaps. Conflicts only happen on a
/// multi-thread runtime (research `surrealdb.md` §3).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_appends_are_gap_free_and_replay_by_range() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let mut tasks = tokio::task::JoinSet::new();
    for i in 0..40u64 {
        let store = Arc::clone(&store);
        let scan = f.scan;
        tasks.spawn(async move {
            store
                .append_event(
                    scan,
                    NewEvent {
                        run: Run(1),
                        ts: t0().plus(Duration::from_secs(i)),
                        body: ScanEventBody::StatusChanged {
                            from: ScanStatus::Queued,
                            to: ScanStatus::Analysing,
                        },
                    },
                )
                .await
        });
    }
    let mut seqs = Vec::new();
    while let Some(done) = tasks.join_next().await {
        seqs.push(done.expect("task ran").expect("append committed").seq.0);
    }
    seqs.sort_unstable();
    assert_eq!(seqs, (1..=40).collect::<Vec<_>>());

    let viewer = check_membership_of::<Viewer>(&*store, f.user, f.garden)
        .await
        .unwrap();
    let replay = store
        .events_after(viewer, f.scan, EventSeq(35))
        .await
        .unwrap();
    let replayed: Vec<u64> = replay.iter().map(|e| e.seq.0).collect();
    assert_eq!(replayed, vec![36, 37, 38, 39, 40]);
    assert!(matches!(
        replay[0].body,
        ScanEventBody::StatusChanged {
            to: ScanStatus::Analysing,
            ..
        }
    ));
    assert_eq!(replay[0].scan_id, f.scan);
}

/// A job's `complete` reads its run's `run_started` and stage events, in `seq` order, and
/// nothing of another run or type (backend roadmap gap 9). Seven events come first, so the
/// stage events' `seq` crosses 10 and a text order of the keys would show.
#[tokio::test]
async fn run_stage_events_reads_one_runs_stage_events_in_order() {
    use domain::events::{RunReason, Stage};
    let store = support::store().await;
    let f = fixture(&store).await;
    for i in 0..7u64 {
        store
            .append_event(
                f.scan,
                NewEvent {
                    run: Run(1),
                    ts: t0().plus(Duration::from_secs(i)),
                    body: ScanEventBody::StatusChanged {
                        from: ScanStatus::Queued,
                        to: ScanStatus::Analysing,
                    },
                },
            )
            .await
            .unwrap();
    }
    let bodies = [
        (
            1,
            ScanEventBody::RunStarted {
                reason: RunReason::Submit,
                rerun_stages: vec![Stage::PlantCheck],
            },
        ),
        (
            1,
            ScanEventBody::StageCompleted {
                stage: Stage::Segmentation,
                duration_ms: 3,
                skipped: None,
            },
        ),
        (
            1,
            ScanEventBody::StatusChanged {
                from: ScanStatus::Queued,
                to: ScanStatus::Analysing,
            },
        ),
        (
            1,
            ScanEventBody::StageStarted {
                stage: Stage::PlantCheck,
            },
        ),
        (
            2,
            ScanEventBody::StageStarted {
                stage: Stage::Species,
            },
        ),
        (
            1,
            ScanEventBody::StageCompleted {
                stage: Stage::PlantCheck,
                duration_ms: 12,
                skipped: None,
            },
        ),
    ];
    for (i, (run, body)) in bodies.iter().enumerate() {
        store
            .append_event(
                f.scan,
                NewEvent {
                    run: Run(*run),
                    ts: t0().plus(Duration::from_secs(10 + i as u64)),
                    body: body.clone(),
                },
            )
            .await
            .unwrap();
    }
    let read = store.run_stage_events(f.scan, Run(1)).await.unwrap();
    let want: Vec<_> = [0, 1, 3, 5].map(|i| bodies[i].1.clone()).into();
    assert_eq!(read, want);
    assert_eq!(store.run_stage_events(f.scan, Run(3)).await.unwrap(), []);
}

fn plan(f: &support::Fixture, id: PlanId, version: u32, supersedes: Option<PlanId>) -> ActionPlan {
    ActionPlan {
        id,
        scan_id: f.scan,
        plant_id: f.plant,
        version,
        supersedes,
        provisional: false,
        diagnosis_id: f.ids.id(),
        open_question: None,
        constraints: Vec::new(),
        summary: "Remove the lowest leaves".into(),
        steps: Vec::new(),
        watch_for: Vec::new(),
        citations: Vec::new(),
        farm_records: Vec::new(),
        task_ids: Vec::new(),
        llm_model: "test".into(),
        prompt_version: "v1".into(),
        created_at: t0(),
    }
}

fn task(f: &support::Fixture, id: TaskId, plan: PlanId) -> NewTask {
    NewTask {
        id,
        plant: Some(f.plant),
        plan: Some(plan),
        plan_step: Some("s1".parse::<StepId>().expect("a step id")),
        scan: None,
        title: "Remove the lowest leaves".into(),
        detail: None,
        due: None,
        repeat: None,
        source: TaskSource::Plan,
        trigger: None,
        alert_key: None,
        created_by: None,
        created_at: t0(),
    }
}

/// [database 4] plan writes: the compare-and-set, the loser's rollback, and the supersede
/// dismissing the old plan's open tasks in the same transaction.
#[tokio::test]
async fn plan_compare_and_set() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let (v1, v1b, v2): (PlanId, PlanId, PlanId) = (f.ids.id(), f.ids.id(), f.ids.id());
    let t1: TaskId = f.ids.id();

    let first = store
        .write_revision(
            f.editor,
            PlanWrite {
                plan: plan(&f, v1, 1, None),
                reason: PlanReason::Initial,
                expected: None,
                tasks: vec![task(&f, t1, v1)],
                triggers: Vec::new(),
            },
        )
        .await
        .expect("first plan");
    assert_eq!(first.task_ids, vec![t1]);

    // A second "first plan" loses: nothing of it is stored.
    let lost = store
        .write_revision(
            f.editor,
            PlanWrite {
                plan: plan(&f, v1b, 1, None),
                reason: PlanReason::Initial,
                expected: None,
                tasks: Vec::new(),
                triggers: Vec::new(),
            },
        )
        .await;
    assert!(
        matches!(lost, Err(StoreError::Conflict(Conflict::StalePlan { current: Some(c) })) if c == v1),
        "{lost:?}"
    );
    let mut r = exec(
        &store,
        &format!("SELECT VALUE id FROM {};", rec("action_plan", v1b)),
    )
    .await;
    let gone: Vec<surrealdb::types::RecordId> = r.take(0).unwrap();
    assert_eq!(gone, Vec::new(), "the THROW rolled the losing plan back");

    store
        .write_revision(
            f.editor,
            PlanWrite {
                plan: plan(&f, v2, 2, Some(v1)),
                reason: PlanReason::ChatRevision,
                expected: Some(v1),
                tasks: Vec::new(),
                triggers: Vec::new(),
            },
        )
        .await
        .expect("second version");
    let mut r = exec(
        &store,
        &format!("SELECT VALUE dismissed_reason FROM {};", rec("task", t1)),
    )
    .await;
    let reason: Vec<String> = r.take(0).unwrap();
    assert_eq!(reason, vec!["plan_superseded".to_owned()]);
}

fn perception_job(f: &support::Fixture, id: JobId, question: Option<QuestionId>) -> NewJob {
    NewJob {
        id,
        scan: f.scan,
        run: Some(Run(1)),
        question,
        spec: JobSpec::Perception(PerceptionPayload {
            images: Vec::new(),
            stages: vec![PerceptionStage::Segmentation],
            species_id: None,
            skip_species_identification: false,
            regions: None,
            label_set_version: "l1".into(),
            classifier_labels_version: "c1".into(),
            question_id: question,
        }),
        max_runtime: JobKind::Perception.max_runtime(false),
        queued_at: t0(),
    }
}

fn heartbeat(worker: &str, caps: Vec<Capability>, held: Vec<HeldJob>) -> Heartbeat {
    Heartbeat {
        worker_id: worker.parse().expect("a worker id"),
        caps,
        slots: 1,
        gpu: None,
        current_jobs: held,
        versions: BTreeMap::from([("label_set_version".to_owned(), "l1".to_owned())]),
    }
}

/// [database 4] job claim: slots, the scan's `queued → analysing`, the requeue of an expired
/// lease, and the fallback's hold on a photo answer's segmentation job.
#[tokio::test]
async fn job_claim_slots_requeue_and_fallback_hold() {
    let store = support::store().await;
    let f = fixture(&store).await;
    set_status(&store, f.scan, "queued").await;
    let job: JobId = f.ids.id();
    store
        .enqueue_job(perception_job(&f, job, None))
        .await
        .unwrap();

    let gpu = "rig-3070".parse().unwrap();
    let now = t0().plus(Duration::from_secs(1));
    let policy = claim_policy(&[Capability::Perception], now);
    let claimed = store
        .claim_job(&gpu, 1, policy.clone(), now)
        .await
        .unwrap()
        .expect("a job");
    assert_eq!(claimed.job.job_id, job);
    assert_eq!(claimed.job.attempt, 1);
    assert!(
        claimed.started_analysing,
        "the run's first job starts the scan"
    );
    assert_eq!(claimed.job.max_runtime_s, 120);

    // The worker's one slot is taken.
    let second: JobId = f.ids.id();
    store
        .enqueue_job(perception_job(&f, second, None))
        .await
        .unwrap();
    assert!(
        store
            .claim_job(&gpu, 1, policy, now)
            .await
            .unwrap()
            .is_none()
    );

    // The heartbeat extends the lease, but never past claimed_at + max_runtime.
    let held = HeldJob {
        job_id: job,
        lease_token: claimed.job.lease_token.clone(),
    };
    let hb = store
        .heartbeat(
            heartbeat("rig-3070", vec![Capability::Perception], vec![held]),
            now,
        )
        .await
        .unwrap();
    assert_eq!(hb.cancel_job_ids, Vec::<JobId>::new());

    // Past the lease, the scheduler requeues it with a fresh queued_at; one requeue is free.
    let later = now.plus(Duration::from_secs(150));
    let requeued = store.requeue_stale(later).await.unwrap();
    assert_eq!(requeued.len(), 1);
    assert_eq!(requeued[0].job.job, job);
    assert!(!requeued[0].exhausted);
    assert!(
        !store.perception_alive(later).await.unwrap(),
        "its heartbeat is 150 s old"
    );

    // A photo answer's segmentation job waits for the perception worker while it is alive.
    let q: QuestionId = f.ids.id();
    let held_back: JobId = f.ids.id();
    exec(&store, "DELETE job;").await;
    store
        .enqueue_job(perception_job(&f, held_back, Some(q)))
        .await
        .unwrap();
    let fb = "fallback-1".parse().unwrap();
    let fb_now = later.plus(Duration::from_secs(20));
    store
        .heartbeat(
            heartbeat("rig-3070", vec![Capability::Perception], Vec::new()),
            fb_now,
        )
        .await
        .unwrap();
    let fallback = claim_policy(&[Capability::Fallback], fb_now);
    assert!(
        store
            .claim_job(&fb, 4, fallback, fb_now)
            .await
            .unwrap()
            .is_none(),
        "held back"
    );
    let rig_down = fb_now.plus(Duration::from_secs(31));
    let fallback = claim_policy(&[Capability::Fallback], rig_down);
    let taken = store
        .claim_job(&fb, 4, fallback, rig_down)
        .await
        .unwrap()
        .expect("taken");
    assert_eq!(taken.job.job_id, held_back);
    assert_eq!(taken.job.max_runtime_s, 240, "doubled on the fallback");
}

/// [database 4] steps between jobs: the claim, the busy flag, the hand-over to `score`, the
/// sweep's re-claim and a slow original's lost commit.
#[tokio::test]
async fn step_claim_commit_and_sweep() {
    let store = support::store().await;
    let f = fixture(&store).await;
    set_status(&store, f.scan, "analysing").await;
    let now = t0();

    let ClaimAttempt::Claimed(retrieve) = store
        .claim_step(f.scan, PipelineStep::Retrieve, now)
        .await
        .unwrap()
    else {
        panic!("the first claim wins");
    };
    assert_eq!(retrieve.run, Run(1));
    assert_eq!(
        store
            .claim_step(f.scan, PipelineStep::Score, now)
            .await
            .unwrap(),
        ClaimAttempt::Busy,
        "the second sets rescore_pending instead"
    );

    // Retrieve queues its vlm job; input arrived meanwhile, so it hands over to score.
    let vlm: JobId = f.ids.id();
    let mut job = perception_job(&f, vlm, None);
    job.spec = JobSpec::Perception(match job.spec {
        JobSpec::Perception(p) => p,
        _ => unreachable!(),
    });
    let commit_at = now.plus(Duration::from_secs(5));
    let to_score = store
        .commit_step(
            retrieve,
            StepOutput::Retrieve {
                observations: None,
                evidence: Vec::new(),
                candidates: Vec::new(),
                weather: None,
                job: Some(job.clone()),
            },
            commit_at,
        )
        .await
        .unwrap()
        .expect("rescore_pending hands over to score");
    assert_eq!(to_score.step, PipelineStep::Score);
    assert_eq!(to_score.stamp, commit_at);

    // The same retrieve can't commit twice: its claim is gone.
    let again = store
        .commit_step(retrieve, StepOutput::Release, commit_at)
        .await;
    assert!(
        matches!(again, Err(StoreError::Conflict(Conflict::ClaimLost))),
        "{again:?}"
    );

    // The sweep finds the score claim stale after 2 minutes and re-claims it.
    let sweep_at = commit_at.plus(Duration::from_secs(150));
    let stale = store.stale_steps(sweep_at).await.unwrap();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].seen, commit_at);
    let reclaimed = store
        .reclaim_step(stale[0], sweep_at)
        .await
        .unwrap()
        .expect("re-claimed");
    assert!(
        store
            .reclaim_step(stale[0], sweep_at)
            .await
            .unwrap()
            .is_none(),
        "only once"
    );

    // The slow original's commit loses; the sweep's commits and releases.
    let slow = store
        .commit_step(to_score, StepOutput::Release, sweep_at)
        .await;
    assert!(
        matches!(slow, Err(StoreError::Conflict(Conflict::ClaimLost))),
        "{slow:?}"
    );
    let released = store
        .commit_step(reclaimed, StepOutput::Release, sweep_at)
        .await
        .unwrap();
    assert_eq!(released, None);

    // A claim of an older run never commits.
    let ClaimAttempt::Claimed(old) = store
        .claim_step(f.scan, PipelineStep::Score, sweep_at)
        .await
        .unwrap()
    else {
        panic!("free again");
    };
    exec(
        &store,
        &format!("UPDATE {} SET run = 2;", rec("scan", f.scan)),
    )
    .await;
    let stale_run = StepClaim { ..old };
    let lost = store
        .commit_step(stale_run, StepOutput::Release, sweep_at)
        .await;
    assert!(
        matches!(lost, Err(StoreError::Conflict(Conflict::ClaimLost))),
        "{lost:?}"
    );
}

/// [contracts 3] a photo answer's steps: `measuring`, the sweep, and the lost original.
#[tokio::test]
async fn question_steps_move_and_recover() {
    let store = support::store().await;
    let f = fixture(&store).await;
    set_status(&store, f.scan, "diagnosed").await;
    let q: QuestionId = f.ids.id();
    let start = t0();
    exec(
        &store,
        &format!(
            "CREATE {q} SET scan = {s}, run = 1, kind = 'photo_request', text = 'Photograph the underside',
               candidates = [], status = 'answering', step = 'segmenting', step_at = <datetime> '{start}',
               continue_at = <datetime> '{start}';",
            q = rec("question", q),
            s = rec("scan", f.scan),
            start = start.into_inner().to_rfc3339(),
        ),
    )
    .await;
    let segmenting = QuestionStepClaim {
        question: q,
        scan: f.scan,
        step: QuestionStep::Segmenting,
        run: Run(1),
        stamp: start,
    };
    let now = start.plus(Duration::from_secs(10));
    let QuestionAdvance::Next(measuring) = store
        .advance_question_step(&segmenting, QuestionStepOutput::Measure, now)
        .await
        .unwrap()
    else {
        panic!("segmentation stored: measuring");
    };
    assert_eq!(measuring.step, QuestionStep::Measuring);

    let sweep_at = now.plus(Duration::from_secs(150));
    let stalled = store
        .stalled_question_steps(sweep_at.minus(Duration::from_secs(120)))
        .await
        .unwrap();
    assert_eq!(stalled, vec![measuring]);
    let reclaimed = store
        .reclaim_question_step(&measuring, QuestionStep::Measuring, sweep_at)
        .await
        .unwrap()
        .expect("re-claimed");
    assert_eq!(reclaimed.stamp, sweep_at);
    let slow = store
        .advance_question_step(&measuring, QuestionStepOutput::Measure, sweep_at)
        .await;
    assert!(
        matches!(slow, Err(StoreError::Conflict(Conflict::QuestionStepLost))),
        "{slow:?}"
    );
}

/// [contracts 1] idempotency: first use, in progress, a different body, replay, release.
#[tokio::test]
async fn idempotency_begin_finish_release() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let scope = IdempotencyScope {
        user: f.user,
        route: "POST /scans".into(),
        key: "6f1c2a7e-0b8e-4d55-9a31-2f1f6c3b9d10"
            .parse::<IdempotencyKey>()
            .unwrap(),
    };
    let expires = t0().plus(Duration::from_hours(24));
    assert_eq!(
        store.begin(&scope, "fp-1", expires).await.unwrap(),
        IdempotencyBegin::Started
    );
    assert_eq!(
        store.begin(&scope, "fp-1", expires).await.unwrap(),
        IdempotencyBegin::InProgress
    );
    assert_eq!(
        store.begin(&scope, "fp-2", expires).await.unwrap(),
        IdempotencyBegin::Conflict
    );

    let response = StoredResponse {
        status: 201,
        body: serde_json::json!({ "id": "x" }),
    };
    store.finish(&scope, response.clone()).await.unwrap();
    assert_eq!(
        store.begin(&scope, "fp-1", expires).await.unwrap(),
        IdempotencyBegin::Replay(response)
    );

    store.release(&scope).await.unwrap();
    assert_eq!(
        store.begin(&scope, "fp-1", expires).await.unwrap(),
        IdempotencyBegin::Started
    );

    // Cleanup: expired keys go in batches.
    let after = expires.plus(Duration::from_secs(1));
    assert_eq!(store.delete_expired(after).await.unwrap(), 1);
    assert_eq!(store.delete_expired(after).await.unwrap(), 0);
}

/// [contracts 1] idempotency: a `running` key older than `IDEMPOTENCY_RUNNING_STALE` is taken
/// over by a retry, a younger one is in progress, and of two concurrent takeovers one wins.
#[tokio::test]
async fn idempotency_takes_over_a_stale_running_key() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let scope = IdempotencyScope {
        user: f.user,
        route: "POST /gardens/x/connectors/y/complete".into(),
        key: "6f1c2a7e-0b8e-4d55-9a31-2f1f6c3b9d11"
            .parse::<IdempotencyKey>()
            .unwrap(),
    };
    let day = Duration::from_hours(24);
    let expires_at = |minutes: u64| t0().plus(Duration::from_mins(minutes)).plus(day);
    assert_eq!(
        store.begin(&scope, "fp", expires_at(0)).await.unwrap(),
        IdempotencyBegin::Started
    );
    assert_eq!(
        store.begin(&scope, "fp", expires_at(5)).await.unwrap(),
        IdempotencyBegin::InProgress
    );
    // A different body is a conflict even when the key is stale.
    assert_eq!(
        store.begin(&scope, "other", expires_at(11)).await.unwrap(),
        IdempotencyBegin::Conflict
    );
    assert_eq!(
        store.begin(&scope, "fp", expires_at(11)).await.unwrap(),
        IdempotencyBegin::Started
    );
    // The takeover restarted the clock: 5 minutes later it is running again.
    assert_eq!(
        store.begin(&scope, "fp", expires_at(16)).await.unwrap(),
        IdempotencyBegin::InProgress
    );
    // Two concurrent takeovers of the next stale window: one wins.
    let (a, b) = tokio::join!(
        store.begin(&scope, "fp", expires_at(30)),
        store.begin(&scope, "fp", expires_at(30)),
    );
    let mut results = [a.unwrap(), b.unwrap()];
    results.sort_by_key(|r| matches!(r, IdempotencyBegin::InProgress));
    assert_eq!(
        results,
        [IdempotencyBegin::Started, IdempotencyBegin::InProgress]
    );
}

/// [database 4] limits: the 31st scan of a UTC day is refused and nothing is stored.
#[tokio::test]
async fn daily_scan_limit() {
    use domain::ports::{NewScan, NewScanPlant, ScanRepo};
    let store = support::store().await;
    let f = fixture(&store).await; // scan 1 of the day
    for i in 2..=31 {
        let result = store
            .create_scan(
                f.editor,
                NewScan {
                    id: f.ids.id(),
                    plant: NewScanPlant::Existing(f.plant),
                    created_by: f.user,
                    created_at: t0(),
                },
            )
            .await;
        if i <= 30 {
            result.expect("within the limit");
        } else {
            assert!(
                matches!(
                    result,
                    Err(StoreError::Rejected(
                        domain::error::DomainError::BudgetExceeded { .. }
                    ))
                ),
                "{result:?}"
            );
        }
    }
    let mut r = exec(&store, "RETURN count(SELECT VALUE id FROM scan);").await;
    let n: Option<i64> = r.take(0).unwrap();
    assert_eq!(n, Some(30));
}

/// [database 4] deletion: a garden with scans, events, jobs, a plan with a task, a question,
/// indicator answers and a map deletes cleanly in the documented order.
#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "one fixture per table the cascade must empty"
)]
async fn garden_deletion_runs_in_order() {
    use domain::{
        authz::Owner,
        ports::{CascadeStep, DeletionTarget},
        records::diagnosis::{EvidenceSource, IndicatorEvidence, Weight},
        scan::Answer,
    };
    let store = support::store().await;
    let f = fixture(&store).await;
    set_status(&store, f.scan, "analysing").await;
    store
        .append_event(
            f.scan,
            NewEvent {
                run: Run(1),
                ts: t0(),
                body: ScanEventBody::StatusChanged {
                    from: ScanStatus::Queued,
                    to: ScanStatus::Analysing,
                },
            },
        )
        .await
        .unwrap();
    store
        .enqueue_job(perception_job(&f, f.ids.id(), None))
        .await
        .unwrap();
    let v1: PlanId = f.ids.id();
    let t1: TaskId = f.ids.id();
    store
        .write_revision(
            f.editor,
            PlanWrite {
                plan: plan(&f, v1, 1, None),
                reason: PlanReason::Initial,
                expected: None,
                tasks: vec![task(&f, t1, v1)],
                triggers: Vec::new(),
            },
        )
        .await
        .unwrap();
    // A retrieve commit stores a measured answer (schema v1.4 indicator_answer).
    let ClaimAttempt::Claimed(claim) = store
        .claim_step(f.scan, PipelineStep::Retrieve, t0())
        .await
        .unwrap()
    else {
        panic!("claim");
    };
    let evidence = IndicatorEvidence {
        indicator_id: "tomato_early_blight.concentric_rings".parse().unwrap(),
        text: "Concentric rings".into(),
        weight: Weight::new(3).unwrap(),
        answer: Answer::Yes,
        reason: "vlm".into(),
        source: EvidenceSource::Vlm,
        measured: None,
    };
    store
        .commit_step(
            claim,
            StepOutput::Retrieve {
                observations: None,
                evidence: vec![evidence],
                candidates: Vec::new(),
                weather: None,
                job: None,
            },
            t0(),
        )
        .await
        .unwrap();
    let mut r = exec(
        &store,
        "RETURN count(SELECT VALUE id FROM indicator_answer);",
    )
    .await;
    assert_eq!(r.take::<Option<i64>>(0).unwrap(), Some(1));
    exec(
        &store,
        &format!(
            "UPDATE {g} SET map_version = 1; CREATE garden_map SET garden = {g}, version = 1, beds = [], pins = [];",
            g = rec("garden", f.garden)
        ),
    )
    .await;

    let owner = check_membership_of::<Owner>(&*store, f.user, f.garden)
        .await
        .unwrap();
    let started = store.start_garden_deletion(owner, t0()).await.unwrap();
    assert_eq!(started.garden_name.as_deref(), Some("Back yard"));
    let target = DeletionTarget::Garden(f.garden);
    for step in CascadeStep::steps_for(target) {
        let mut guard = 0;
        while store
            .delete_batch(target, *step)
            .await
            .unwrap_or_else(|e| panic!("{step:?}: {e:?}"))
            > 0
        {
            guard += 1;
            assert!(guard < 50, "{step:?} never finished");
        }
    }
    for table in [
        "garden",
        "plant",
        "scan",
        "scan_event",
        "job",
        "task",
        "action_plan",
        "indicator_answer",
        "garden_map",
        "member_of",
    ] {
        let mut r = exec(
            &store,
            &format!("RETURN count(SELECT VALUE id FROM {table});"),
        )
        .await;
        assert_eq!(
            r.take::<Option<i64>>(0).unwrap(),
            Some(0),
            "{table} left behind"
        );
    }
}

#[test]
fn timestamps_keep_precision_for_stamps() {
    let t: Timestamp = "2026-10-18T14:00:00Z".parse().unwrap();
    assert_eq!(
        t.plus(Duration::from_millis(1)).since(t),
        Duration::from_millis(1)
    );
}

/// Database caveats 16–17 and the indicator-answer ranking (schema v1.4), as raw statements.
#[tokio::test]
async fn engine_facts_behind_the_v1_4_notes() {
    let store = support::store().await;
    for name in ["auth", "token", "access", "session"] {
        let sent = store.db().query("RETURN 1;").bind((name, "x")).await;
        let error = sent
            .err()
            .map(|e| e.message().to_owned())
            .unwrap_or_default();
        assert!(error.contains("protected variable"), "{name}: {error}");
    }
    let mut r = store
        .db()
        .query("RETURN type::point([$lon, $lat]);")
        .bind(("lon", -79.38))
        .bind(("lat", 43.65))
        .await
        .unwrap();
    assert!(r.take_errors().is_empty());
    let f = fixture(&store).await;
    let s = rec("scan", f.scan);
    let write = |source: &str, rank: i64, x: i64| {
        format!(
            "LET $answers = [{{ id: indicator_answer:[{s}, 1, 'a.b'], rank: {rank}, row: {{ scan: {s}, run: 1, source: '{source}', evidence: {{ x: {x} }}, answered_at: time::now() }} }}];
             FOR $a IN $answers {{ IF ((SELECT VALUE rank FROM ONLY $a.id) ?? 0) <= $a.rank {{ UPSERT $a.id CONTENT $a.row; }}; }};
             SELECT VALUE evidence.x FROM indicator_answer;"
        )
    };
    let x = |mut r: surrealdb::IndexedResults| r.take::<Vec<i64>>(2).unwrap();
    assert_eq!(x(exec(&store, &write("vlm", 1, 1)).await), vec![1]);
    assert_eq!(
        x(exec(&store, &write("vlm", 1, 2)).await),
        vec![2],
        "same rank replaces"
    );
    assert_eq!(
        x(exec(&store, &write("measurement", 2, 3)).await),
        vec![3],
        "measurement beats vlm"
    );
    assert_eq!(
        x(exec(&store, &write("user", 3, 4)).await),
        vec![4],
        "user beats measurement"
    );
    assert_eq!(
        x(exec(&store, &write("measurement", 2, 5)).await),
        vec![4],
        "user is kept"
    );
}

/// A one-hot 384-dimension vector, the index's dimension.
fn one_hot(i: usize) -> Vec<f32> {
    let mut v = vec![0.0; 384];
    v[i] = 1.0;
    v
}

/// The knowledge loader's output on the real schema: `knowledge_fixture.surql` is what
/// `ml/knowledge/build_migration.py` writes for the fixture seed in `ml/tests/fixtures/` (its
/// test keeps the copy current), with one-hot vectors in chunk order.
#[tokio::test]
async fn the_knowledge_loaders_file_loads_and_reads_back() {
    use domain::ports::{ChunkFilter, Embedding, KnowledgeRepo};

    const FILE: &str = include_str!("embedded/knowledge_fixture.surql");
    let store = support::store().await;
    // Twice: the file replaces the whole set, so a second run changes nothing.
    exec(&store, FILE).await;
    exec(&store, FILE).await;

    let k = store.load_knowledge().await.expect("the knowledge loads");
    let ids = |v: Vec<String>| v.join(",");
    assert_eq!(
        ids(k.species.iter().map(|s| s.id.to_string()).collect()),
        "strawberry,tomato"
    );
    assert_eq!(k.problems.len(), 3);
    let blight = k
        .problems
        .iter()
        .find(|p| p.id.as_str() == "tomato_early_blight")
        .unwrap();
    assert_eq!(blight.discriminating().count(), 3);
    assert_eq!(
        blight.affects.len(),
        1,
        "affects comes back from susceptible_to"
    );
    assert_eq!(
        blight.treatments.len(),
        3,
        "treatments come back from manages"
    );
    assert!(
        (blight.conditions.temp_c.unwrap().min - 24.0).abs() < 1e-9,
        "an integer in the seed reads as f64"
    );
    let septoria = k
        .problems
        .iter()
        .find(|p| p.id.as_str() == "tomato_septoria_leaf_spot")
        .unwrap();
    assert!(septoria.indicators[0].measure.is_some());
    assert_eq!(k.lookalikes.len(), 2, "looks_like is written both ways");
    let aphids = k
        .problems
        .iter()
        .find(|p| p.id.as_str() == "tomato_aphids")
        .unwrap();
    assert_eq!(aphids.caused_by.len(), 1);
    let larva = k
        .organisms
        .iter()
        .find(|o| o.id.as_str() == "ladybug_larva")
        .unwrap();
    assert_eq!(larva.preys_on.as_ref().map(Vec::len), Some(1));
    let tomato_photos = &k
        .species_photos
        .iter()
        .find(|(s, _)| s.as_str() == "tomato")
        .unwrap()
        .1;
    assert_eq!(tomato_photos.len(), 1);
    assert_eq!(
        tomato_photos[0].credit,
        "Generated test image, not a real photo"
    );
    assert_eq!(k.treatments.len(), 4);
    assert!(
        k.treatments
            .iter()
            .any(|t| t.text.contains("it's an 'easy' first step")),
        "quotes survive the escaping"
    );
    assert_eq!(k.products.products.len(), 2);
    assert_eq!(k.classifier_labels.model_version, "fixture_v1");
    assert_eq!(k.classifier_labels.labels.len(), 2);
    assert_eq!(k.soil_conditions.len(), 1);

    // The loader and the store compute label_set_version the same way.
    let mut meta = exec(
        &store,
        "SELECT VALUE label_set_version FROM ONLY knowledge_meta:current;",
    )
    .await;
    let written: Option<String> = meta.take(0).unwrap();
    assert_eq!(
        written.as_deref(),
        Some(k.vlm_options.label_set_version.as_str())
    );

    // Chunk i has the one-hot vector i, in id order.
    let hits = store
        .search_chunks(&Embedding(one_hot(2)), ChunkFilter::default(), 1)
        .await
        .expect("the vector search runs");
    assert_eq!(
        hits[0].text,
        "Water at the base of the plant and mulch the soil.\nRemove infected leaves."
    );
    assert_eq!(hits[0].citation.title, "Tomato diseases (example)");
}

#[tokio::test]
async fn knowledge_search_returns_the_nearest_chunks_first() {
    use domain::ports::{ChunkFilter, Embedding, KnowledgeRepo};

    let store = support::store().await;
    exec(
        &store,
        "CREATE source:guide SET title = 'Guide', publisher = 'Extension', url = 'https://example.org/guide', retrieved_at = '2026-10-15';",
    )
    .await;
    for i in 0..3 {
        let v = serde_json::to_string(&one_hot(i)).unwrap();
        exec(
            &store,
            &format!(
                "CREATE knowledge_chunk:⟨guide.{i}⟩ SET source = source:guide, topic = 'symptoms', text = 'chunk {i}', text_hash = 'h{i}', embedding = {v}, embedding_model = 'test';"
            ),
        )
        .await;
    }
    let mut query = one_hot(1);
    query[2] = 0.5;
    let hits = store
        .search_chunks(&Embedding(query), ChunkFilter::default(), 2)
        .await
        .expect("the vector search runs");
    let texts: Vec<_> = hits.iter().map(|h| h.text.as_str()).collect();
    assert_eq!(texts, ["chunk 1", "chunk 2"], "nearest first, k of them");
}

/// A species finding of run 1, as the pipeline converts a worker's result.
fn species_finding(
    f: &support::Fixture,
    id: domain::id::ObservationId,
    label: &str,
    model_version: &str,
) -> domain::records::observation::Observation<domain::url::Stored> {
    use domain::records::observation::{Observation, ObservationSource, ObservationStage};
    Observation {
        id,
        scan_id: f.scan,
        run: Run(1),
        stage: ObservationStage::Species,
        pipeline_stage: domain::events::Stage::Species,
        label: label.to_owned(),
        knowledge_ref: None,
        role: None,
        detail: None,
        confidence: domain::wire::Rated(domain::wire::Confidence::new(0.6).expect("a confidence")),
        region: None,
        source: ObservationSource::Bioclip,
        model_version: model_version.to_owned(),
        created_at: t0(),
    }
}

/// [contracts 5] partial results: a write that keeps a stored finding's id (the pipeline's
/// `keep_partial_ids`, for a `complete` that repeats a partial result) replaces its record with
/// the new content and reports nothing; a new id is added and an id it dropped is retracted.
#[tokio::test]
async fn a_write_that_keeps_a_partial_findings_id_reports_no_change() {
    let store = support::store().await;
    let f = fixture(&store).await;
    set_status(&store, f.scan, "queued").await;
    let job: JobId = f.ids.id();
    store
        .enqueue_job(perception_job(&f, job, None))
        .await
        .unwrap();
    let worker: domain::id::WorkerId = "rig-3070".parse().unwrap();
    let now = t0().plus(Duration::from_secs(1));
    let claimed = store
        .claim_job(
            &worker,
            1,
            claim_policy(&[Capability::Perception], now),
            now,
        )
        .await
        .unwrap()
        .expect("a job");
    let lease = domain::ports::Lease {
        job,
        token: claimed.job.lease_token,
        worker,
    };
    let write = |observations| domain::ports::ObservationWrite {
        stage: domain::events::Stage::Species,
        run: Run(1),
        images: Vec::new(),
        replace_imageless: true,
        observations,
        masks: Vec::new(),
    };
    let (tomato, potato) = (f.ids.id(), f.ids.id());
    let partial = store
        .store_partial(
            &lease,
            write(vec![
                species_finding(&f, tomato, "Tomato", "partial"),
                species_finding(&f, potato, "Potato", "partial"),
            ]),
        )
        .await
        .unwrap();
    assert_eq!(partial.added.len(), 2);

    // The same findings under the same ids, from the job's own versions: no change to report.
    let repeat = store
        .store_partial(
            &lease,
            write(vec![
                species_finding(&f, tomato, "Tomato", "bioclip-2"),
                species_finding(&f, potato, "Potato", "bioclip-2"),
            ]),
        )
        .await
        .unwrap();
    assert!(repeat.added.is_empty(), "{repeat:?}");
    assert!(repeat.retracted.is_empty(), "{repeat:?}");
    let stored = store.observations(f.scan).await.unwrap();
    assert_eq!(stored.len(), 2);
    assert!(
        stored.iter().all(|o| o.model_version == "bioclip-2"),
        "the records hold the new content"
    );

    // One kept, one changed under a new id.
    let pepper = f.ids.id();
    let changed = store
        .store_partial(
            &lease,
            write(vec![
                species_finding(&f, tomato, "Tomato", "bioclip-2"),
                species_finding(&f, pepper, "Pepper", "bioclip-2"),
            ]),
        )
        .await
        .unwrap();
    assert_eq!(
        changed.added.iter().map(|o| o.id).collect::<Vec<_>>(),
        [pepper]
    );
    assert_eq!(changed.retracted, [potato]);
}
