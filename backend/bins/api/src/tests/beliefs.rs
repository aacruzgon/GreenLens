//! `GET /plants/{id}/beliefs` and the deletion runner's Agent Memory step, over
//! `MockBeliefStore`.

use std::sync::Arc;

use domain::{
    chat::{Belief, BeliefHit},
    id::PlantId,
    ports::{BELIEFS_PROVIDER, BeliefSubject, BeliefWrite, MockBeliefStore, ProviderError},
};

use super::*;

const PLANT: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5P";

fn plant() -> PlantId {
    PLANT.parse().unwrap()
}

fn garden() -> GardenId {
    GARDEN.parse().unwrap()
}

fn belief(key: &str, value: &str, superseded: Option<&str>) -> Belief {
    Belief {
        key: key.to_owned(),
        value: value.to_owned(),
        about_plant: true,
        memory_id: None,
        learned_at: "2026-10-14T15:00:00Z".parse().unwrap(),
        superseded_at: superseded.map(|t| t.parse().unwrap()),
    }
}

fn down() -> ProviderError {
    ProviderError::Unavailable {
        provider: BELIEFS_PROVIDER,
        source: "connection reset".into(),
    }
}

async fn get_beliefs(
    role: Option<Role>,
    beliefs: Arc<dyn BeliefStore>,
) -> (StatusCode, serde_json::Value) {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, role);
    let app = crate::router::app(state_with_beliefs(
        store,
        auth,
        MockBlobStore::new(),
        no_images(),
        beliefs,
    ));
    let response = app
        .oneshot(get(&format!("/plants/{PLANT}/beliefs")))
        .await
        .unwrap();
    let status = response.status();
    (status, json(response).await)
}

fn unavailable() -> serde_json::Value {
    serde_json::json!({ "available": false, "current": [], "history": [] })
}

#[tokio::test]
async fn beliefs_without_agent_memory_are_unavailable() {
    let (status, body) = get_beliefs(Some(Role::Viewer), Arc::new(NoBeliefs)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, unavailable());
}

#[tokio::test]
async fn a_member_reads_current_beliefs_and_the_superseded_history() {
    let mut beliefs = MockBeliefStore::new();
    beliefs.expect_enabled().return_const(true);
    beliefs
        .expect_current()
        .withf(|g, p| *g == garden() && *p == Some(plant()))
        .times(1)
        .returning(|_, _| Ok(vec![belief("watering", "Watered every 2 days.", None)]));
    beliefs
        .expect_history()
        .withf(|s| s.garden == garden() && s.plant == Some(plant()))
        .times(1)
        .returning(|_| {
            Ok(vec![
                belief("watering", "Watered every 2 days.", None),
                belief(
                    "watering",
                    "Watered every 3 days.",
                    Some("2026-10-17T15:00:00Z"),
                ),
            ])
        });
    let (status, body) = get_beliefs(Some(Role::Viewer), Arc::new(beliefs)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        serde_json::json!({
            "available": true,
            "current": [{
                "key": "watering", "value": "Watered every 2 days.", "about_plant": true,
                "learned_at": "2026-10-14T15:00:00Z"
            }],
            "history": [{
                "key": "watering", "value": "Watered every 3 days.", "about_plant": true,
                "learned_at": "2026-10-14T15:00:00Z", "superseded_at": "2026-10-17T15:00:00Z"
            }]
        })
    );
}

#[tokio::test]
async fn a_failing_agent_memory_is_unavailable_never_a_5xx() {
    let mut beliefs = MockBeliefStore::new();
    beliefs.expect_enabled().return_const(true);
    beliefs.expect_current().returning(|_, _| Err(down()));
    beliefs.expect_history().returning(|_| Ok(Vec::new()));
    let (status, body) = get_beliefs(Some(Role::Viewer), Arc::new(beliefs)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, unavailable());
}

/// Answers every read after a minute.
struct SlowBeliefs;

#[async_trait::async_trait]
impl BeliefStore for SlowBeliefs {
    fn enabled(&self) -> bool {
        true
    }
    async fn record(&self, _: &BeliefWrite) -> Result<(), ProviderError> {
        Ok(())
    }
    async fn current(&self, _: GardenId, _: Option<PlantId>) -> Result<Vec<Belief>, ProviderError> {
        tokio::time::sleep(Duration::from_secs(60)).await;
        Ok(Vec::new())
    }
    async fn history(&self, _: BeliefSubject) -> Result<Vec<Belief>, ProviderError> {
        tokio::time::sleep(Duration::from_secs(60)).await;
        Ok(Vec::new())
    }
    async fn search(
        &self,
        _: GardenId,
        _: Option<PlantId>,
        _: &str,
        _: usize,
    ) -> Result<Vec<BeliefHit>, ProviderError> {
        Ok(Vec::new())
    }
    async fn forget(&self, _: BeliefSubject) -> Result<(), ProviderError> {
        Ok(())
    }
}

#[tokio::test(start_paused = true)]
async fn a_slow_agent_memory_is_unavailable_after_three_seconds() {
    let start = tokio::time::Instant::now();
    let (status, body) = get_beliefs(Some(Role::Viewer), Arc::new(SlowBeliefs)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, unavailable());
    assert_eq!(start.elapsed(), Duration::from_secs(3));
}

#[tokio::test]
async fn a_non_member_gets_404() {
    let mut beliefs = MockBeliefStore::new();
    beliefs.expect_current().never();
    beliefs.expect_history().never();
    let (status, body) = get_beliefs(None, Arc::new(beliefs)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
}
