//! The mirror over mocked ports: what reaches Agent Memory, in what order, and what happens
//! when it fails.

use std::sync::{Arc, Mutex};

use domain::{
    authz::{Editor, check_membership_of},
    chat::{MemoryKind, MemoryTopic},
    ports::{BELIEFS_PROVIDER, MockAccessRepo, MockBeliefStore, MockMemoryStore, ProviderError},
    testing::FixedClock,
    wire::Role,
};

use super::*;
use crate::testing::SlowBeliefs;

const USER: &str = "01JAX3K9T6R2B8V0QZ4M7D1E50";
const GARDEN: &str = "01JAX3K9T6R2B8V0QZ4M7D1E51";
const PLANT: &str = "01JAX3K9T6R2B8V0QZ4M7D1E52";
const MEMORY: &str = "01JAX3K9T6R2B8V0QZ4M7D1E53";
const OTHER_GARDEN: &str = "01JAX3K9T6R2B8V0QZ4M7D1E54";

fn now() -> Timestamp {
    "2026-10-17T15:00:00Z".parse().unwrap()
}

fn garden() -> GardenId {
    GARDEN.parse().unwrap()
}

fn plant() -> PlantId {
    PLANT.parse().unwrap()
}

fn id() -> MemoryId {
    MEMORY.parse().unwrap()
}

async fn member<R: domain::authz::MinRole>() -> Membership<R> {
    let mut access = MockAccessRepo::new();
    access
        .expect_role_in()
        .returning(|_, _| Ok(Some(Role::Owner)));
    check_membership_of::<R>(&access, USER.parse().unwrap(), garden())
        .await
        .unwrap()
}

fn stored(source: MemorySource, shared: bool, plant: Option<PlantId>) -> Memory {
    Memory {
        id: id(),
        garden_id: garden(),
        plant_id: plant,
        kind: MemoryKind::Fact,
        topic: MemoryTopic::Watering,
        text: "Watered every 2 days.".to_owned(),
        source,
        shared,
        created_at: now(),
    }
}

fn plant_path() -> String {
    BeliefSubject {
        garden: garden(),
        plant: Some(plant()),
    }
    .scope_path()
}

fn garden_path() -> String {
    BeliefSubject {
        garden: garden(),
        plant: None,
    }
    .scope_path()
}

fn new_memory() -> NewMemory {
    NewMemory {
        id: id(),
        plant: Some(plant()),
        kind: MemoryKind::Fact,
        topic: MemoryTopic::Watering,
        text: "Watered every 2 days.".to_owned(),
        embedding: Embedding(vec![0.1; 4]),
        embedding_model: "test-embed".to_owned(),
        source_msg: None,
        created_at: now(),
    }
}

fn down() -> ProviderError {
    ProviderError::Unavailable {
        provider: BELIEFS_PROVIDER,
        source: "connection reset".into(),
    }
}

fn enabled() -> MockBeliefStore {
    let mut b = MockBeliefStore::new();
    b.expect_enabled().return_const(true);
    b
}

fn disabled() -> MockBeliefStore {
    let mut b = MockBeliefStore::new();
    b.expect_enabled().return_const(false);
    b
}

fn mirror(inner: MockMemoryStore, beliefs: MockBeliefStore) -> MirroredMemory {
    MirroredMemory::new(
        Arc::new(inner),
        Arc::new(beliefs),
        Arc::new(FixedClock::at(now())),
    )
}

/// Every call, in order, across both mocks.
type Calls = Arc<Mutex<Vec<String>>>;

fn log(calls: &Calls, call: impl Into<String>) {
    calls.lock().unwrap().push(call.into());
}

#[tokio::test]
async fn a_private_chat_memory_is_never_recorded() {
    let mut inner = MockMemoryStore::new();
    inner
        .expect_remember()
        .returning(|_, _| Ok(stored(MemorySource::Chat, false, Some(plant()))));
    inner.expect_mark_synced().never();
    let mut beliefs = enabled();
    beliefs.expect_record().never();
    let m = mirror(inner, beliefs)
        .remember(member::<Viewer>().await, new_memory())
        .await
        .unwrap();
    assert!(!m.shared);
}

#[tokio::test]
async fn a_note_is_recorded_then_marked_synced() {
    let calls = Calls::default();
    let mut inner = MockMemoryStore::new();
    inner
        .expect_add_note()
        .returning(|_, _| Ok(stored(MemorySource::User, true, Some(plant()))));
    let c = calls.clone();
    inner
        .expect_mark_synced()
        .withf(|m, at| *m == id() && *at == now())
        .times(1)
        .returning(move |_, _| {
            log(&c, "mark_synced");
            Ok(())
        });
    let mut beliefs = enabled();
    let c = calls.clone();
    beliefs
        .expect_record()
        .withf(|w| {
            w.key == "watering"
                && w.subject.plant == Some(plant())
                && w.value == "Watered every 2 days."
        })
        .times(1)
        .returning(move |_| {
            log(&c, "record");
            Ok(())
        });
    mirror(inner, beliefs)
        .add_note(member::<Editor>().await, new_memory())
        .await
        .unwrap();
    assert_eq!(*calls.lock().unwrap(), ["record", "mark_synced"]);
}

#[tokio::test]
async fn a_failed_record_leaves_the_memory_unsynced_and_the_write_succeeds() {
    let mut inner = MockMemoryStore::new();
    inner
        .expect_add_pipeline_memory()
        .returning(|_, _| Ok(stored(MemorySource::Pipeline, true, Some(plant()))));
    inner.expect_mark_synced().never();
    let mut beliefs = enabled();
    beliefs.expect_record().times(1).returning(|_| Err(down()));
    let m = mirror(inner, beliefs)
        .add_pipeline_memory(garden(), new_memory())
        .await
        .unwrap();
    assert_eq!(m.id, id());
}

#[tokio::test]
async fn sharing_a_memory_records_it() {
    let mut inner = MockMemoryStore::new();
    inner
        .expect_share()
        .returning(|_, _| Ok(stored(MemorySource::User, true, None)));
    inner.expect_mark_synced().times(1).returning(|_, _| Ok(()));
    let mut beliefs = enabled();
    beliefs
        .expect_record()
        .withf(|w| w.subject.plant.is_none())
        .times(1)
        .returning(|_| Ok(()));
    mirror(inner, beliefs)
        .share(member::<Editor>().await, id())
        .await
        .unwrap();
}

/// A delete's mocks: `memory` returns `found`; forget, `mark_unsynced` and delete log their
/// calls.
fn delete_mocks(
    found: Memory,
    forget: Result<(), fn() -> ProviderError>,
    calls: &Calls,
) -> (MockMemoryStore, MockBeliefStore) {
    let mut inner = MockMemoryStore::new();
    inner
        .expect_memory()
        .returning(move |_| Ok(Some(found.clone())));
    let c = calls.clone();
    inner.expect_mark_unsynced().returning(move |g, p| {
        assert_eq!(g, garden());
        log(
            &c,
            format!("mark_unsynced({})", p.map_or("none", |_| "plant")),
        );
        Ok(())
    });
    let c = calls.clone();
    inner.expect_delete().returning(move |_, _| {
        log(&c, "delete");
        Ok(())
    });
    let mut beliefs = enabled();
    let c = calls.clone();
    beliefs.expect_forget().returning(move |s| {
        log(&c, format!("forget({})", s.scope_path()));
        forget.map_err(|e| e())
    });
    (inner, beliefs)
}

#[tokio::test]
async fn deleting_a_shared_plant_memory_forgets_then_unsyncs_then_deletes() {
    let calls = Calls::default();
    let (inner, beliefs) = delete_mocks(
        stored(MemorySource::User, true, Some(plant())),
        Ok(()),
        &calls,
    );
    mirror(inner, beliefs)
        .delete(member::<Viewer>().await, id())
        .await
        .unwrap();
    assert_eq!(
        *calls.lock().unwrap(),
        [
            format!("forget({})", plant_path()),
            "delete".to_owned(),
            "mark_unsynced(plant)".to_owned(),
        ]
    );
}

#[tokio::test]
async fn deleting_a_garden_memory_unsyncs_the_whole_garden() {
    let calls = Calls::default();
    let (inner, beliefs) = delete_mocks(stored(MemorySource::Pipeline, true, None), Ok(()), &calls);
    mirror(inner, beliefs)
        .delete(member::<Viewer>().await, id())
        .await
        .unwrap();
    assert_eq!(
        *calls.lock().unwrap(),
        [
            format!("forget({})", garden_path()),
            "delete".to_owned(),
            "mark_unsynced(none)".to_owned(),
        ]
    );
}

#[tokio::test]
async fn a_failed_forget_is_a_503_and_keeps_the_row() {
    let calls = Calls::default();
    let (inner, beliefs) = delete_mocks(
        stored(MemorySource::User, true, Some(plant())),
        Err(down),
        &calls,
    );
    let err = mirror(inner, beliefs)
        .delete(member::<Viewer>().await, id())
        .await
        .unwrap_err();
    assert!(matches!(err, StoreError::Unavailable(_)), "{err:?}");
    assert_eq!(
        *calls.lock().unwrap(),
        [format!("forget({})", plant_path())]
    );
}

#[tokio::test]
async fn deleting_a_private_memory_never_forgets() {
    let calls = Calls::default();
    let (inner, mut beliefs) = delete_mocks(
        stored(MemorySource::Chat, false, Some(plant())),
        Ok(()),
        &calls,
    );
    beliefs.checkpoint();
    beliefs.expect_enabled().return_const(true);
    beliefs.expect_forget().never();
    mirror(inner, beliefs)
        .delete(member::<Viewer>().await, id())
        .await
        .unwrap();
    assert_eq!(*calls.lock().unwrap(), ["delete"]);
}

#[tokio::test]
async fn a_memory_of_another_garden_is_never_forgotten() {
    let calls = Calls::default();
    let mut other = stored(MemorySource::User, true, Some(plant()));
    other.garden_id = OTHER_GARDEN.parse().unwrap();
    let (inner, mut beliefs) = delete_mocks(other, Ok(()), &calls);
    beliefs.checkpoint();
    beliefs.expect_enabled().return_const(true);
    beliefs.expect_forget().never();
    mirror(inner, beliefs)
        .delete(member::<Viewer>().await, id())
        .await
        .unwrap();
    assert_eq!(*calls.lock().unwrap(), ["delete"]);
}

#[tokio::test]
async fn disabled_beliefs_are_never_called() {
    let mut inner = MockMemoryStore::new();
    inner
        .expect_add_note()
        .returning(|_, _| Ok(stored(MemorySource::User, true, Some(plant()))));
    inner.expect_memory().never();
    inner.expect_unsynced_shared().never();
    inner.expect_mark_synced().never();
    inner.expect_mark_unsynced().never();
    inner.expect_delete().times(1).returning(|_, _| Ok(()));
    let mut beliefs = disabled();
    beliefs.expect_record().never();
    beliefs.expect_forget().never();
    beliefs.expect_current().never();
    beliefs.expect_search().never();
    let clock = FixedClock::at(now());
    let m = mirror(inner, beliefs);
    m.add_note(member::<Editor>().await, new_memory())
        .await
        .unwrap();
    m.delete(member::<Viewer>().await, id()).await.unwrap();
    assert_eq!(
        sync_pending(&m, m.beliefs.as_ref(), &clock, 50)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        current_beliefs(m.beliefs.as_ref(), garden(), None, Duration::from_secs(1)).await,
        []
    );
}

#[tokio::test]
async fn the_sweep_records_oldest_first_and_stops_at_the_first_failure() {
    let ids = [
        "01JAX3K9T6R2B8V0QZ4M7D1E60",
        "01JAX3K9T6R2B8V0QZ4M7D1E61",
        "01JAX3K9T6R2B8V0QZ4M7D1E62",
    ];
    let pending: Vec<Memory> = ids
        .iter()
        .map(|i| Memory {
            id: i.parse().unwrap(),
            ..stored(MemorySource::User, true, Some(plant()))
        })
        .collect();
    let mut inner = MockMemoryStore::new();
    inner
        .expect_unsynced_shared()
        .withf(|limit| *limit == 50)
        .returning(move |_| Ok(pending.clone()));
    let synced = Calls::default();
    let s = synced.clone();
    inner.expect_mark_synced().returning(move |m, _| {
        log(&s, m.to_string());
        Ok(())
    });
    let mut beliefs = enabled();
    let failing: MemoryId = ids[1].parse().unwrap();
    beliefs.expect_record().times(2).returning(move |w| {
        if w.memory == failing {
            Err(down())
        } else {
            Ok(())
        }
    });
    let err = sync_pending(&inner, &beliefs, &FixedClock::at(now()), 50)
        .await
        .unwrap_err();
    assert!(
        matches!(err, BeliefSyncError::Record { synced: 1, .. }),
        "{err:?}"
    );
    assert_eq!(*synced.lock().unwrap(), [ids[0]]);
}

#[tokio::test]
async fn the_sweep_counts_what_it_recorded() {
    let mut inner = MockMemoryStore::new();
    inner
        .expect_unsynced_shared()
        .returning(|_| Ok(vec![stored(MemorySource::User, true, None)]));
    inner.expect_mark_synced().times(1).returning(|_, _| Ok(()));
    let mut beliefs = enabled();
    beliefs.expect_record().times(1).returning(|_| Ok(()));
    let n = sync_pending(&inner, &beliefs, &FixedClock::at(now()), 50)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn current_beliefs_are_empty_on_error() {
    let mut beliefs = enabled();
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
