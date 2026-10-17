//! The sweep over mocked ports: what reaches Agent Memory, in what order, and what happens
//! when it fails.

use std::sync::{Arc, Mutex};

use domain::{
    chat::{Memory, MemoryKind, MemorySource, MemoryTopic},
    id::MemoryId,
    ports::{BELIEFS_PROVIDER, MockBeliefStore, MockMemoryStore, ProviderError},
    testing::FixedClock,
    wire::Timestamp,
};

use super::*;
use crate::testing::SlowBeliefs;

const GARDEN: &str = "01JAX3K9T6R2B8V0QZ4M7D1E51";
const PLANT: &str = "01JAX3K9T6R2B8V0QZ4M7D1E52";
const OTHER_PLANT: &str = "01JAX3K9T6R2B8V0QZ4M7D1E55";

fn now() -> Timestamp {
    "2026-10-17T15:00:00Z".parse().unwrap()
}

fn garden() -> GardenId {
    GARDEN.parse().unwrap()
}

fn plant() -> PlantId {
    PLANT.parse().unwrap()
}

fn other_plant() -> PlantId {
    OTHER_PLANT.parse().unwrap()
}

fn on_plant() -> BeliefSubject {
    BeliefSubject {
        garden: garden(),
        plant: Some(plant()),
    }
}

fn on_garden() -> BeliefSubject {
    BeliefSubject {
        garden: garden(),
        plant: None,
    }
}

fn shared(id: &str, plant: Option<PlantId>) -> Memory {
    Memory {
        id: id.parse().unwrap(),
        garden_id: garden(),
        plant_id: plant,
        kind: MemoryKind::Fact,
        topic: MemoryTopic::Watering,
        text: "Watered every 2 days.".to_owned(),
        source: MemorySource::User,
        shared: true,
        created_at: now(),
    }
}

fn down() -> ProviderError {
    ProviderError::Unavailable {
        provider: BELIEFS_PROVIDER,
        source: "connection reset".into(),
    }
}

/// Every call, in order, across both mocks.
type Calls = Arc<Mutex<Vec<String>>>;

fn log(calls: &Calls, call: impl Into<String>) {
    calls.lock().unwrap().push(call.into());
}

/// A store that queues `forgets` and holds `pending`, and logs finishes and marks.
fn store(forgets: Vec<BeliefSubject>, pending: Vec<Memory>, calls: &Calls) -> MockMemoryStore {
    let mut m = MockMemoryStore::new();
    m.expect_belief_forgets()
        .returning(move |_| Ok(forgets.clone()));
    m.expect_unsynced_shared()
        .returning(move |_| Ok(pending.clone()));
    let c = Arc::clone(calls);
    m.expect_finish_belief_forget().returning(move |s| {
        log(&c, format!("finish({})", s.scope_path()));
        Ok(())
    });
    let c = Arc::clone(calls);
    m.expect_mark_synced().returning(move |id: MemoryId, _| {
        log(&c, format!("synced({id})"));
        Ok(())
    });
    m
}

/// An enabled belief store whose forgets fail for `failing_forget` and whose records fail for
/// `failing_record`, logging every call.
fn beliefs(
    failing_forget: Option<BeliefSubject>,
    failing_record: Option<&'static str>,
    calls: &Calls,
) -> MockBeliefStore {
    let mut b = MockBeliefStore::new();
    b.expect_enabled().return_const(true);
    let c = Arc::clone(calls);
    b.expect_forget().returning(move |s| {
        log(&c, format!("forget({})", s.scope_path()));
        if Some(s) == failing_forget {
            Err(down())
        } else {
            Ok(())
        }
    });
    let c = Arc::clone(calls);
    b.expect_record().returning(move |w| {
        log(&c, format!("record({})", w.memory));
        if failing_record == Some(w.memory.to_string().as_str()) {
            Err(down())
        } else {
            Ok(())
        }
    });
    b
}

async fn sweep(store: &MockMemoryStore, beliefs: &MockBeliefStore) -> SweepReport {
    sync_pending(store, beliefs, &FixedClock::at(now()), 50)
        .await
        .unwrap()
}

const A: &str = "01JAX3K9T6R2B8V0QZ4M7D1E60";
const B: &str = "01JAX3K9T6R2B8V0QZ4M7D1E61";
const C: &str = "01JAX3K9T6R2B8V0QZ4M7D1E62";

#[tokio::test]
async fn queued_erases_run_before_any_record() {
    let calls = Calls::default();
    let store = store(vec![on_plant()], vec![shared(A, Some(plant()))], &calls);
    let report = sweep(&store, &beliefs(None, None, &calls)).await;
    assert_eq!(
        *calls.lock().unwrap(),
        [
            format!("forget({})", on_plant().scope_path()),
            format!("finish({})", on_plant().scope_path()),
            format!("record({A})"),
            format!("synced({A})"),
        ]
    );
    assert_eq!(
        report,
        SweepReport {
            forgotten: 1,
            recorded: 1,
            failed: 0
        }
    );
}

#[tokio::test]
async fn a_failed_record_does_not_hold_up_the_rest() {
    let calls = Calls::default();
    let pending = vec![
        shared(A, Some(plant())),
        shared(B, Some(plant())),
        shared(C, None),
    ];
    let store = store(Vec::new(), pending, &calls);
    let report = sweep(&store, &beliefs(None, Some(A), &calls)).await;
    assert_eq!(
        *calls.lock().unwrap(),
        [
            format!("record({A})"),
            format!("record({B})"),
            format!("synced({B})"),
            format!("record({C})"),
            format!("synced({C})"),
        ],
        "A stays unsynced for the next run; B and C go on"
    );
    assert_eq!(report.failed, 1);
    assert_eq!(report.recorded, 2);
}

#[tokio::test]
async fn a_failed_erase_stays_queued_and_holds_back_its_subject() {
    let calls = Calls::default();
    let pending = vec![
        shared(A, Some(plant())),
        shared(B, Some(other_plant())),
        shared(C, None),
    ];
    let store = store(vec![on_plant()], pending, &calls);
    let report = sweep(&store, &beliefs(Some(on_plant()), None, &calls)).await;
    assert_eq!(
        *calls.lock().unwrap(),
        [
            format!("forget({})", on_plant().scope_path()),
            format!("record({B})"),
            format!("synced({B})"),
            format!("record({C})"),
            format!("synced({C})"),
        ],
        "no finish for the failed erase, and nothing recorded under its plant"
    );
    assert_eq!(report.failed, 1);
}

#[tokio::test]
async fn a_failed_garden_erase_holds_back_its_plants_too() {
    let calls = Calls::default();
    let pending = vec![shared(A, Some(plant())), shared(B, None)];
    let store = store(vec![on_garden()], pending, &calls);
    let report = sweep(&store, &beliefs(Some(on_garden()), None, &calls)).await;
    assert_eq!(
        *calls.lock().unwrap(),
        [format!("forget({})", on_garden().scope_path())]
    );
    assert_eq!(
        report,
        SweepReport {
            forgotten: 0,
            recorded: 0,
            failed: 1
        }
    );
}

#[tokio::test]
async fn a_private_memory_is_never_recorded() {
    let calls = Calls::default();
    let mut private = shared(A, Some(plant()));
    private.shared = false;
    private.source = MemorySource::Chat;
    let store = store(Vec::new(), vec![private], &calls);
    let report = sweep(&store, &beliefs(None, None, &calls)).await;
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(report, SweepReport::default());
}

#[tokio::test]
async fn with_agent_memory_off_the_queue_is_emptied_and_nothing_is_called() {
    let calls = Calls::default();
    let mut store = store(vec![on_plant()], Vec::new(), &calls);
    store.checkpoint();
    store
        .expect_belief_forgets()
        .returning(|_| Ok(vec![on_plant()]));
    let c = Arc::clone(&calls);
    store.expect_finish_belief_forget().returning(move |s| {
        log(&c, format!("finish({})", s.scope_path()));
        Ok(())
    });
    store.expect_unsynced_shared().never();
    let mut b = MockBeliefStore::new();
    b.expect_enabled().return_const(false);
    b.expect_forget().never();
    b.expect_record().never();
    let report = sweep(&store, &b).await;
    assert_eq!(
        *calls.lock().unwrap(),
        [format!("finish({})", on_plant().scope_path())]
    );
    assert_eq!(report, SweepReport::default());
}

#[tokio::test]
async fn current_beliefs_are_empty_on_error() {
    let mut beliefs = MockBeliefStore::new();
    beliefs.expect_enabled().return_const(true);
    beliefs.expect_current().returning(|_, _| Err(down()));
    assert_eq!(
        current_beliefs(&beliefs, garden(), Some(plant()), Duration::from_secs(1)).await,
        []
    );
}

#[tokio::test(start_paused = true)]
async fn a_slow_belief_store_times_out_empty() {
    let start = tokio::time::Instant::now();
    let found = current_beliefs(&SlowBeliefs, garden(), None, Duration::from_millis(1_500)).await;
    assert_eq!(found, []);
    assert_eq!(start.elapsed(), Duration::from_millis(1_500));
    let hits = search_beliefs(
        &SlowBeliefs,
        garden(),
        None,
        "watering",
        5,
        Duration::from_secs(4),
    )
    .await;
    assert_eq!(hits, []);
}
