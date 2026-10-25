//! One alert per spell ([database 4], alert dedupe; architecture 8.3): `create_alert_task` on the
//! embedded `SurrealDB` 3.3.4 engine, one datastore per test. Each call is one scheduler run
//! seeing a spell: its alert key names the spell and its first fired day, `until` its last.

#[path = "embedded/support.rs"]
#[expect(
    dead_code,
    reason = "the shared fixtures; these tests use only some of them"
)]
mod support;

use domain::{
    id::{PlanId, StepId, TaskId},
    ports::{IdGenExt, NewTask, PlanRepo, PlanWrite},
    records::{
        plan::{ActionPlan, PlanReason, TriggerKind},
        task::{AlertKey, TaskSource, TaskTrigger},
    },
    wire::LocalDate,
};
use store::SurrealStore;
use support::{Fixture, exec, fixture, rec, t0};

fn day(s: &str) -> LocalDate {
    s.parse().expect("a date")
}

/// A forecast alert of the fixture's garden for a spell starting on `from`.
fn alert(f: &Fixture, trigger: TriggerKind, from: &str) -> NewTask {
    NewTask {
        id: f.ids.id(),
        plant: None,
        plan: None,
        plan_step: None,
        scan: None,
        title: "Frost expected".into(),
        detail: Some("Cover tender plants.".into()),
        due: Some(day(from)),
        repeat: None,
        source: TaskSource::ForecastAlert,
        trigger: Some(TaskTrigger {
            kind: trigger,
            date: day(from),
        }),
        alert_key: Some(AlertKey::Forecast {
            garden: f.garden,
            trigger,
            date: day(from),
        }),
        created_by: None,
        created_at: t0(),
    }
}

/// One run seeing the spell `from..=until`: whether it created a task.
async fn run(store: &SurrealStore, f: &Fixture, task: NewTask, until: &str) -> Option<TaskId> {
    store
        .create_alert_task(f.garden, task, day(until))
        .await
        .expect("the write")
        .map(|t| t.id)
}

/// Each task of the garden's spells as `(alert_key, spell_until)`, by key.
async fn spells(store: &SurrealStore) -> Vec<(String, String)> {
    let mut r = exec(
        store,
        "SELECT VALUE [alert_key, spell_until] FROM task WHERE spell != NONE ORDER BY alert_key;",
    )
    .await;
    let rows: Vec<Vec<String>> = r.take(0).expect("rows");
    rows.into_iter()
        .map(|mut kv| {
            let until = kv.pop().expect("until");
            (kv.pop().expect("key"), until)
        })
        .collect()
}

#[tokio::test]
async fn a_frost_spell_alerts_once_across_runs_shifts_and_a_dismissal() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let frost = TriggerKind::Frost;
    let key = |from: &str| format!("{}:frost:{from}", f.garden);

    // Frost on the three forecast days: one task, seen to the 20th.
    let first = run(&store, &f, alert(&f, frost, "2026-10-18"), "2026-10-20").await;
    let first = first.expect("the spell's alert");
    // The same spell on the next run: nothing new.
    assert_eq!(
        run(&store, &f, alert(&f, frost, "2026-10-18"), "2026-10-20").await,
        None
    );
    // The next day the 18th has dropped out and the 21st is frost too: the same spell.
    assert_eq!(
        run(&store, &f, alert(&f, frost, "2026-10-19"), "2026-10-21").await,
        None
    );
    assert_eq!(
        spells(&store).await,
        vec![(key("2026-10-18"), "2026-10-21".into())]
    );

    // Dismissed by the user, the alert stays the spell's: the spell going on alerts nothing.
    exec(
        &store,
        &format!(
            "UPDATE {} SET dismissed_at = time::now(), dismissed_reason = 'user';",
            rec("task", first)
        ),
    )
    .await;
    // Seen from its last day only (the cold hour crossed midnight into the 22nd).
    assert_eq!(
        run(&store, &f, alert(&f, frost, "2026-10-22"), "2026-10-22").await,
        None
    );
    // An older forecast seen late never shortens the spell.
    assert_eq!(
        run(&store, &f, alert(&f, frost, "2026-10-19"), "2026-10-20").await,
        None
    );
    assert_eq!(
        spells(&store).await,
        vec![(key("2026-10-18"), "2026-10-22".into())]
    );

    // A day without frost, then frost again: a new spell, a new alert.
    let second = run(&store, &f, alert(&f, frost, "2026-10-24"), "2026-10-25").await;
    assert!(second.is_some_and(|t| t != first));
    // Another trigger in the same garden is its own spell.
    assert!(
        run(
            &store,
            &f,
            alert(&f, TriggerKind::Heat, "2026-10-24"),
            "2026-10-24"
        )
        .await
        .is_some()
    );
    assert_eq!(
        spells(&store).await,
        vec![
            (key("2026-10-18"), "2026-10-22".into()),
            (key("2026-10-24"), "2026-10-25".into()),
            (format!("{}:heat:2026-10-24", f.garden), "2026-10-24".into()),
        ]
    );
}

#[tokio::test]
async fn an_alert_from_before_spells_takes_its_spell_on() {
    let store = support::store().await;
    let f = fixture(&store).await;
    // A task with the key but no spell, as the per-date alerts were stored.
    let legacy = alert(&f, TriggerKind::Frost, "2026-10-18");
    let legacy_id = legacy.id;
    store.create_task(f.editor, legacy).await.expect("legacy");
    assert_eq!(
        run(
            &store,
            &f,
            alert(&f, TriggerKind::Frost, "2026-10-18"),
            "2026-10-20"
        )
        .await,
        None
    );
    assert_eq!(
        run(
            &store,
            &f,
            alert(&f, TriggerKind::Frost, "2026-10-19"),
            "2026-10-21"
        )
        .await,
        None
    );
    let mut r = exec(
        &store,
        &format!("SELECT VALUE spell_until FROM {};", rec("task", legacy_id)),
    )
    .await;
    let until: Option<String> = r.take(0).expect("until");
    assert_eq!(until.as_deref(), Some("2026-10-21"));
}

#[tokio::test]
async fn a_garden_being_deleted_gets_no_alert() {
    let store = support::store().await;
    let f = fixture(&store).await;
    exec(
        &store,
        &format!(
            "UPDATE {} SET deleting_at = time::now();",
            rec("garden", f.garden)
        ),
    )
    .await;
    assert_eq!(
        run(
            &store,
            &f,
            alert(&f, TriggerKind::Frost, "2026-10-18"),
            "2026-10-18"
        )
        .await,
        None
    );
    let mut r = exec(&store, "SELECT VALUE id FROM task;").await;
    let tasks: Vec<surrealdb::types::RecordId> = r.take(0).expect("ids");
    assert!(tasks.is_empty(), "{tasks:?}");
}

#[tokio::test]
async fn a_plan_trigger_creates_one_task_per_spell() {
    let store = support::store().await;
    let f = fixture(&store).await;
    let plan: PlanId = f.ids.id();
    store
        .write_revision(
            f.editor,
            PlanWrite {
                plan: ActionPlan {
                    id: plan,
                    scan_id: f.scan,
                    plant_id: f.plant,
                    version: 1,
                    supersedes: None,
                    provisional: false,
                    diagnosis_id: f.ids.id(),
                    open_question: None,
                    constraints: Vec::new(),
                    summary: "Protect from frost".into(),
                    steps: Vec::new(),
                    watch_for: Vec::new(),
                    citations: Vec::new(),
                    farm_records: Vec::new(),
                    task_ids: Vec::new(),
                    llm_model: "test".into(),
                    prompt_version: "v1".into(),
                    created_at: t0(),
                },
                reason: PlanReason::Initial,
                expected: None,
                tasks: Vec::new(),
                triggers: Vec::new(),
            },
        )
        .await
        .expect("the plan");
    let step: StepId = "s2".parse().expect("a step id");
    let task = |from: &str| NewTask {
        plant: Some(f.plant),
        plan: Some(plan),
        plan_step: Some(step.clone()),
        source: TaskSource::Plan,
        alert_key: Some(AlertKey::PlanTrigger {
            plan,
            step: step.clone(),
            date: day(from),
        }),
        ..alert(&f, TriggerKind::Frost, from)
    };
    assert!(
        run(&store, &f, task("2026-10-18"), "2026-10-19")
            .await
            .is_some()
    );
    assert_eq!(
        run(&store, &f, task("2026-10-19"), "2026-10-20").await,
        None
    );
    assert!(
        run(&store, &f, task("2026-10-23"), "2026-10-23")
            .await
            .is_some()
    );
    assert_eq!(
        spells(&store).await,
        vec![
            (format!("plan:{plan}:s2:2026-10-18"), "2026-10-20".into()),
            (format!("plan:{plan}:s2:2026-10-23"), "2026-10-23".into()),
        ]
    );
}
