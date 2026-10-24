//! `POST /conversations/{id}/messages`: the answer blocks a turn sent reach the stream and the
//! JSON replay stored under the idempotency key ([contracts 8], answer blocks).

use std::sync::Mutex;

use domain::{
    chat::{
        AnswerBlock, AnswerBlockEvent, BlockBody, ChatEvent, Message, MessageCompleted,
        MessageRole, Usage,
    },
    ports::ConversationOwnerRow,
};
use futures::StreamExt as _;

use super::*;

const CONVERSATION: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5C";

/// A turn that sends one block and completes with the message that holds it.
struct OneBlock(Message<Stored>);

#[async_trait::async_trait]
impl ChatTurns for OneBlock {
    async fn start_turn(&self, _: TurnRequest) -> Result<TurnParts, TurnError> {
        let message = self.0.clone();
        let events = vec![
            ChatEvent::AnswerBlock(AnswerBlockEvent {
                v: V1,
                block: message.blocks[0].clone(),
            }),
            ChatEvent::MessageCompleted(MessageCompleted {
                v: V1,
                message_id: message.id,
                usage: Usage::default(),
            }),
        ];
        let (tx, outcome) = tokio::sync::oneshot::channel();
        let _ = tx.send(agent::TurnOutcome::Completed(Box::new(message)));
        Ok(TurnParts {
            events: futures::stream::iter(events).boxed(),
            outcome,
        })
    }
    fn apply_preview(
        &self,
        _: &ActionPlan,
        _: &Preview,
        _: PendingActionId,
        _: Timestamp,
    ) -> Result<RevisedPlan, PreviewError> {
        unreachable!("not used by this test")
    }
}

fn block() -> AnswerBlock {
    AnswerBlock {
        call_id: "toolu_fc".to_owned(),
        body: BlockBody::WeatherHours {
            days: vec![
                serde_json::from_value(serde_json::json!({
                    "date": "2026-10-22", "t_min_c": -1.5, "t_max_c": 9.0, "precip_mm": 0.0,
                    "mean_rh_pct": 80.0, "hours_rh_over_90": 6, "frost_risk": true
                }))
                .unwrap(),
            ],
            scan_id: None,
        },
    }
}

#[tokio::test]
async fn the_stream_and_the_json_replay_carry_the_answer_blocks() {
    let mut store = FakeStore::default();
    let auth = signed_in(&mut store, Some(Role::Viewer));
    store
        .access_repo
        .expect_conversation_owner()
        .returning(|_| {
            Ok(Some(ConversationOwnerRow {
                user: USER.parse().unwrap(),
                garden: GARDEN.parse().unwrap(),
            }))
        });
    store
        .idempotency_repo
        .expect_begin()
        .returning(|_, _, _| Ok(IdempotencyBegin::Started));
    let finished: Arc<Mutex<Option<StoredResponse>>> = Arc::default();
    let seen = Arc::clone(&finished);
    store
        .idempotency_repo
        .expect_finish()
        .times(1)
        .returning(move |_, response| {
            *seen.lock().unwrap() = Some(response);
            Ok(())
        });
    let message = Message::<Stored> {
        id: "01JAX3K9T6R2B8V0QZ4M7D1E5M".parse().unwrap(),
        role: MessageRole::Assistant,
        text: "Thursday night drops below zero.".to_owned(),
        attachments: Vec::new(),
        citations: Vec::new(),
        farm_citations: Vec::new(),
        pending_actions: Vec::new(),
        blocks: vec![block()],
        interrupted: false,
        created_at: NOW.parse().unwrap(),
    };
    let mut state = state(store, auth, MockBlobStore::new(), no_images());
    Arc::get_mut(&mut state).unwrap().chat = Arc::new(OneBlock(message));
    let tasks = state.tasks.clone();
    let app = crate::router::app(state);

    let request = Request::post(format!("/conversations/{CONVERSATION}/messages"))
        .header(header::AUTHORIZATION, "Bearer session-token")
        .header(header::CONTENT_TYPE, "application/json")
        .header("idempotency-key", "7d444840-9dc0-11d1-b245-5ffdce74fad2")
        .body(Body::from(
            r#"{"text":"Will the frost on Thursday hurt my plants?"}"#,
        ))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let sse = String::from_utf8(bytes.to_vec()).unwrap();
    let wire = serde_json::to_string(&block()).unwrap();
    assert!(
        sse.contains(&format!(
            "event: answer_block\ndata: {{\"v\":1,\"block\":{wire}}}"
        )),
        "{sse}"
    );

    tasks.close();
    tasks.wait().await;
    let stored = finished
        .lock()
        .unwrap()
        .take()
        .expect("the replay is stored");
    assert_eq!(stored.status, 200);
    assert_eq!(
        stored.body["message"]["blocks"],
        serde_json::to_value(vec![block()]).unwrap()
    );
}
