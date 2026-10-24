//! `POST /pending-actions/{id}/confirm` and `/reject` through the real router ([contracts 8],
//! the pending-action table): the mocked store holds the cards, a fake `ChatTurns` applies the
//! revision preview, and a recording pipeline takes the plan write.

use std::sync::Mutex;

use async_trait::async_trait;
use domain::{
    chat::{ConfirmResult, PendingStatus},
    id::{ConversationId, PlanId},
    ports::{ConversationOwnerRow, PendingActionRecord, StoreError},
    records::{
        plan::{PlanReason, PlanStep},
        task::Task,
    },
};

use super::*;
use crate::seams::PreviewError;

const CONV: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5A";
const ACTION: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5B";
const PLANT: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5P";
const TASK: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5T";
const PLAN: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5N";
const NEWER_PLAN: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5M";
const DIAGNOSIS: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5D";

/// Stores a plan revision and remembers what it was asked to write.
#[derive(Default)]
struct RecordingPipeline {
    writes: Mutex<Vec<PlanWrite>>,
}

#[async_trait]
impl PipelineApi for Arc<RecordingPipeline> {
    async fn store_revision(
        &self,
        _: Membership<domain::authz::Editor>,
        _: ScanId,
        write: PlanWrite,
    ) -> Result<ActionPlan, PipelineError> {
        let plan = write.plan.clone();
        self.writes.lock().unwrap().push(write);
        Ok(plan)
    }
    async fn follow_up(&self, _: FollowUp) -> Result<(), PipelineError> {
        unreachable!("not used by these tests")
    }
    async fn submit(
        &self,
        _: Membership<domain::authz::Editor>,
        _: ScanId,
    ) -> Result<SubmitOutcome, PipelineError> {
        unreachable!("not used by these tests")
    }
    async fn species_confirmed(
        &self,
        _: Membership<domain::authz::Editor>,
        _: domain::id::PlantId,
        _: SpeciesChoice,
    ) -> Result<Plant<Stored>, PipelineError> {
        unreachable!("not used by these tests")
    }
    async fn answer(
        &self,
        _: Membership<domain::authz::Editor>,
        _: ScanId,
        _: AnswerRequest,
    ) -> Result<Handled<Scan<Stored>>, PipelineError> {
        unreachable!("not used by these tests")
    }
    async fn photo_answer_start(
        &self,
        _: ScanId,
        _: domain::id::QuestionId,
        _: PhotoInfo,
    ) -> Result<PhotoAnswerStart, PipelineError> {
        unreachable!("not used by these tests")
    }
    fn photo_answer_added(&self, _: domain::ports::QuestionStepClaim) -> Option<FollowUp> {
        unreachable!("not used by these tests")
    }
    async fn worker_events(
        &self,
        _: &Lease,
        _: LeasedJob,
        _: Vec<WorkerEvent>,
    ) -> Result<(), PipelineError> {
        unreachable!("not used by these tests")
    }
    async fn job_completed(
        &self,
        _: &Lease,
        _: LeasedJob,
        _: CompleteJob,
    ) -> Result<Handled<CompletionOutcome>, PipelineError> {
        unreachable!("not used by these tests")
    }
    async fn job_failed_for_good(&self, _: LeasedJob) -> Result<Option<FollowUp>, PipelineError> {
        unreachable!("not used by these tests")
    }
    async fn sweep(&self) -> Result<Vec<FollowUp>, PipelineError> {
        unreachable!("not used by these tests")
    }
}

/// Applies a revision preview with a fixed answer.
struct FixedRevision(Result<RevisedPlan, PreviewError>);

#[async_trait]
impl ChatTurns for FixedRevision {
    async fn start_turn(&self, _: TurnRequest) -> Result<TurnParts, TurnError> {
        unreachable!("not used by these tests")
    }
    fn apply_preview(
        &self,
        _: &ActionPlan,
        _: &Preview,
        _: PendingActionId,
        _: Timestamp,
    ) -> Result<RevisedPlan, PreviewError> {
        self.0.clone()
    }
}

fn card(tool: &str, status: &str, expires_at: &str, preview: &serde_json::Value) -> PendingAction {
    serde_json::from_value(serde_json::json!({
        "id": ACTION,
        "conversation_id": CONV,
        "tool": tool,
        "status": status,
        "card": {
            "title": "Add a task?",
            "body": "Spray the tomato with water",
            "confirm_label": "Add task",
            "reject_label": "Not now",
        },
        "preview": preview,
        "expires_at": expires_at,
    }))
    .unwrap()
}

type PendingAction = domain::chat::PendingAction;

fn task_json() -> serde_json::Value {
    serde_json::json!({
        "id": TASK,
        "garden_id": GARDEN,
        "plant_id": PLANT,
        "title": "Spray the tomato with water",
        "source": "chat",
        "created_at": NOW,
    })
}

fn task_card(status: &str, expires_at: &str) -> PendingAction {
    card(
        "create_task",
        status,
        expires_at,
        &serde_json::json!({ "kind": "task", "task": task_json() }),
    )
}

fn plan_card(status: &str) -> PendingAction {
    card(
        "revise_plan",
        status,
        "2026-10-19T14:00:00Z",
        &serde_json::json!({
            "kind": "plan",
            "base_plan_id": PLAN,
            "plan_summary": "Water less, no copper",
            "changed_steps": [],
            "removed_step_ids": [],
            "constraints_added": ["No copper products"],
        }),
    )
}

fn plan(id: &str, version: u32) -> ActionPlan {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "scan_id": SCAN,
        "plant_id": PLANT,
        "version": version,
        "provisional": false,
        "diagnosis_id": DIAGNOSIS,
        "constraints": [],
        "summary": "Treat the early blight",
        "steps": [],
        "watch_for": [],
        "citations": [],
        "farm_records": [],
        "task_ids": [],
        "llm_model": "big",
        "prompt_version": "plan/test",
        "created_at": NOW,
    }))
    .unwrap()
}

fn revised() -> RevisedPlan {
    RevisedPlan {
        summary: "Water less, no copper".to_owned(),
        steps: Vec::<PlanStep>::new(),
        watch_for: Vec::new(),
        constraints: Vec::new(),
        citation_ids: Vec::new(),
    }
}

/// What a test sets up besides the signed-in caller and the card.
#[derive(Default)]
struct Setup {
    chat: Option<FixedRevision>,
}

/// A store whose conversation `CONV` belongs to the caller, with `action` as its card.
fn store_with_card(role: Role, action: Option<PendingAction>) -> (FakeStore, MockAuthVerifier) {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(role));
    store
        .access_repo
        .expect_pending_action_conversation()
        .returning(|_| Ok(Some(CONV.parse::<ConversationId>().unwrap())));
    store
        .access_repo
        .expect_conversation_owner()
        .returning(|_| {
            Ok(Some(ConversationOwnerRow {
                user: USER.parse().unwrap(),
                garden: GARDEN.parse().unwrap(),
            }))
        });
    if let Some(action) = action {
        store
            .chat_repo
            .expect_pending_action()
            .returning(move |_, _| {
                Ok(PendingActionRecord {
                    action: action.clone(),
                    result: None,
                })
            });
    }
    (store, auth)
}

fn expect_confirmed(store: &mut FakeStore, action: PendingAction) {
    store
        .chat_repo
        .expect_mark_confirmed()
        .times(1)
        .returning(move |_, _, result, _| {
            let mut confirmed = action.clone();
            confirmed.status = PendingStatus::Confirmed;
            // The store keeps the result; the handler answers with its own copy.
            let _: ConfirmResult = result;
            Ok(confirmed)
        });
}

async fn post_to(app: axum::Router, action: &str, verb: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(
            Request::post(format!("/pending-actions/{action}/{verb}"))
                .header(header::AUTHORIZATION, "Bearer session-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    (status, json(response).await)
}

fn app(
    store: FakeStore,
    auth: MockAuthVerifier,
    setup: Setup,
    pipeline: &Arc<RecordingPipeline>,
) -> axum::Router {
    let pipeline = Arc::clone(pipeline);
    let mut state = state_with_pipeline(store, auth, config(), move |_| Arc::new(pipeline));
    let owned = Arc::get_mut(&mut state).expect("the state is not shared yet");
    if let Some(chat) = setup.chat {
        owned.chat = Arc::new(chat);
    }
    let mut knowledge = domain::ports::MockKnowledgeStore::new();
    knowledge.expect_citations().returning(|_| Ok(Vec::new()));
    owned.ports.knowledge = Arc::new(knowledge);
    crate::router::app(state)
}

async fn confirm(
    store: FakeStore,
    auth: MockAuthVerifier,
    setup: Setup,
) -> (StatusCode, serde_json::Value, Arc<RecordingPipeline>) {
    let pipeline = Arc::new(RecordingPipeline::default());
    let (status, body) = post_to(app(store, auth, setup, &pipeline), ACTION, "confirm").await;
    (status, body, pipeline)
}

#[tokio::test]
async fn confirming_a_create_task_card_creates_the_task_with_source_chat() {
    let action = task_card("proposed", "2026-10-19T14:00:00Z");
    let (mut store, auth) = store_with_card(Role::Editor, Some(action.clone()));
    store
        .plan_repo
        .expect_create_task()
        .times(1)
        .withf(|_, new| {
            new.source == domain::records::task::TaskSource::Chat
                && new.plant == Some(PLANT.parse().unwrap())
                && new.title == "Spray the tomato with water"
                && new.created_by == Some(USER.parse().unwrap())
        })
        .returning(|_, _| Ok(serde_json::from_value::<Task>(task_json()).unwrap()));
    expect_confirmed(&mut store, action);

    let (status, body, _) = confirm(store, auth, Setup::default()).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["pending_action"]["status"], "confirmed");
    assert_eq!(body["result"]["task"]["id"], TASK);
    assert_eq!(body["result"]["task"]["source"], "chat");
    assert!(body["result"].get("plan").is_none());
}

#[tokio::test]
async fn confirming_a_revise_plan_card_stores_a_new_version_with_reason_chat_revision() {
    let action = plan_card("proposed");
    let (mut store, auth) = store_with_card(Role::Editor, Some(action.clone()));
    store
        .plan_repo
        .expect_plan()
        .returning(|_, _| Ok(plan(PLAN, 1)));
    expect_confirmed(&mut store, action);
    let setup = Setup {
        chat: Some(FixedRevision(Ok(revised()))),
    };

    let (status, body, pipeline) = confirm(store, auth, setup).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["plan"]["version"], 2);
    assert_eq!(body["result"]["plan"]["supersedes"], PLAN);
    assert_eq!(body["result"]["plan"]["summary"], "Water less, no copper");
    let writes = pipeline.writes.lock().unwrap();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].reason, PlanReason::ChatRevision);
    assert_eq!(writes[0].expected, Some(PLAN.parse::<PlanId>().unwrap()));
    assert_eq!(writes[0].plan.version, 2);
}

#[tokio::test]
async fn rejecting_a_card_answers_200_with_the_rejected_card() {
    let action = task_card("proposed", "2026-10-19T14:00:00Z");
    let (mut store, auth) = store_with_card(Role::Editor, None);
    store
        .chat_repo
        .expect_reject_pending_action()
        .times(1)
        .returning(move |_, _, _| {
            let mut rejected = action.clone();
            rejected.status = PendingStatus::Rejected;
            Ok(rejected)
        });
    let pipeline = Arc::new(RecordingPipeline::default());

    let (status, body) = post_to(
        app(store, auth, Setup::default(), &pipeline),
        ACTION,
        "reject",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["pending_action"]["status"], "rejected");
    assert!(body.get("result").is_none());
}

#[tokio::test]
async fn confirming_a_rejected_or_expired_card_is_409_invalid_state_and_stores_nothing() {
    for status_name in ["rejected", "expired"] {
        let action = task_card(status_name, "2026-10-19T14:00:00Z");
        let (mut store, auth) = store_with_card(Role::Editor, Some(action));
        store.plan_repo.expect_create_task().never();
        store.chat_repo.expect_mark_confirmed().never();

        let (status, body, _) = confirm(store, auth, Setup::default()).await;

        assert_eq!(status, StatusCode::CONFLICT, "{status_name}");
        assert_eq!(body["error"]["code"], "invalid_state");
        assert_eq!(body["error"]["details"]["status"], status_name);
    }
}

#[tokio::test]
async fn confirming_a_proposed_card_past_its_expiry_is_409_expired() {
    // `NOW` is 2026-10-18T14:00:00Z; the card ran out an hour before.
    let action = task_card("proposed", "2026-10-18T13:00:00Z");
    let (mut store, auth) = store_with_card(Role::Editor, Some(action));
    store.plan_repo.expect_create_task().never();
    store.chat_repo.expect_mark_confirmed().never();

    let (status, body, _) = confirm(store, auth, Setup::default()).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "invalid_state");
    assert_eq!(body["error"]["details"]["status"], "expired");
}

#[tokio::test]
async fn confirming_a_revision_whose_base_is_no_longer_current_is_409_with_current_plan_id() {
    let action = plan_card("proposed");
    let (mut store, auth) = store_with_card(Role::Editor, Some(action));
    store
        .plan_repo
        .expect_plan()
        .returning(|_, _| Ok(plan(PLAN, 1)));
    store.chat_repo.expect_mark_confirmed().never();
    let setup = Setup {
        chat: Some(FixedRevision(Err(PreviewError::BaseMismatch {
            base: PLAN.parse().unwrap(),
            current: NEWER_PLAN.parse().unwrap(),
        }))),
    };

    let (status, body, pipeline) = confirm(store, auth, setup).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "invalid_state");
    assert_eq!(body["error"]["details"]["current_plan_id"], NEWER_PLAN);
    assert!(pipeline.writes.lock().unwrap().is_empty());
}

#[tokio::test]
async fn confirming_a_create_task_for_a_deleted_plant_is_409_with_hint_plant_removed() {
    let action = task_card("proposed", "2026-10-19T14:00:00Z");
    let (mut store, auth) = store_with_card(Role::Editor, Some(action));
    store
        .plan_repo
        .expect_create_task()
        .times(1)
        .returning(|_, _| Err(StoreError::NotFound));
    store.chat_repo.expect_mark_confirmed().never();

    let (status, body, _) = confirm(store, auth, Setup::default()).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "invalid_state");
    assert_eq!(body["error"]["details"]["hint"], "plant_removed");
}

#[tokio::test]
async fn a_viewer_can_neither_confirm_nor_reject_a_card() {
    for verb in ["confirm", "reject"] {
        let (mut store, auth) = store_with_card(Role::Viewer, None);
        store.chat_repo.expect_pending_action().never();
        store.chat_repo.expect_mark_confirmed().never();
        store.chat_repo.expect_reject_pending_action().never();
        store.plan_repo.expect_create_task().never();
        let pipeline = Arc::new(RecordingPipeline::default());

        let (status, body) =
            post_to(app(store, auth, Setup::default(), &pipeline), ACTION, verb).await;

        assert_eq!(status, StatusCode::FORBIDDEN, "{verb}");
        assert_eq!(body["error"]["code"], "forbidden", "{verb}");
    }
}
