//! Client tests against a `wiremock` server.

use std::time::Duration;

use futures::StreamExt;
use secrecy::SecretString;
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_partial_json, header, method, path},
};

use super::*;
use crate::http::RetryPolicy;

fn client(server: &MockServer, retry: RetryPolicy) -> AnthropicClient {
    let mut config = AnthropicConfig::new(SecretString::from("sk-test"));
    config.base_url = server.uri();
    config.retry = RetryPolicy {
        base: Duration::from_millis(1),
        cap: Duration::from_millis(2),
        ..retry
    };
    AnthropicClient::new(reqwest::Client::new(), config)
}

fn request() -> MessagesRequest<'static> {
    MessagesRequest::new(
        "claude-opus-5-5",
        100,
        vec![InputMessage {
            role: Role::User,
            content: vec![ContentBlock::text("Hi")],
        }],
    )
}

fn message_body() -> serde_json::Value {
    json!({
        "id": "msg_1", "type": "message", "role": "assistant", "model": "claude-opus-5-5",
        "content": [{"type": "text", "text": "Hello"}],
        "stop_reason": "end_turn", "usage": {"input_tokens": 3, "output_tokens": 1}
    })
}

#[tokio::test]
async fn create_sends_headers_and_retries_overload() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(529).set_body_json(json!({
            "type": "error", "error": {"type": "overloaded_error", "message": "Overloaded"}
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "sk-test"))
        .and(header("anthropic-version", API_VERSION))
        .respond_with(ResponseTemplate::new(200).set_body_json(message_body()))
        .mount(&server)
        .await;
    let m = client(&server, RetryPolicy::STANDARD)
        .create(&request())
        .await
        .unwrap();
    assert_eq!(m.text(), "Hello");
    assert_eq!(m.stop_reason, Some(StopReason::EndTurn));
}

#[tokio::test]
async fn spend_cap_is_never_retried() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({
            "type": "error",
            "error": {"type": "rate_limit_error", "message": "cap",
                      "details": {"error_code": "enforced_spend_limit_reached"}}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let err = client(&server, RetryPolicy::STANDARD)
        .create(&request())
        .await
        .unwrap_err();
    assert!(matches!(err, AnthropicError::SpendCap { .. }));
    assert!(!err.retryable());
    assert!(matches!(
        err.into_model_error("claude"),
        domain::ports::ModelError::Unavailable { .. }
    ));
}

#[tokio::test]
async fn rate_limit_carries_retry_after() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "3")
                .set_body_json(json!({"type": "error",
                    "error": {"type": "rate_limit_error", "message": "slow down"}})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let err = client(&server, RetryPolicy::NONE)
        .create(&request())
        .await
        .unwrap_err();
    assert!(err.retryable());
    assert_eq!(err.retry_after(), Some(Duration::from_secs(3)));
}

#[tokio::test]
async fn stream_ends_with_error_event_after_200() {
    let server = MockServer::start().await;
    let body = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-5-5\",\"content\":[],\"stop_reason\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":1}}}\n\n",
        "event: ping\n",
        "data: {\"type\":\"ping\"}\n\n",
        "event: error\n",
        "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n",
    );
    Mock::given(method("POST"))
        .and(body_partial_json(json!({"stream": true})))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .insert_header("request-id", "req_1")
                .set_body_string(body),
        )
        .mount(&server)
        .await;
    let stream = client(&server, RetryPolicy::NONE)
        .stream(&request())
        .await
        .unwrap();
    assert_eq!(stream.request_id(), Some("req_1"));
    let items: Vec<_> = stream.collect().await;
    assert_eq!(items.len(), 2, "ping filtered, nothing after the error");
    assert!(matches!(items[0], Ok(StreamEvent::MessageStart { .. })));
    assert!(matches!(
        &items[1],
        Err(AnthropicError::Stream { kind, .. }) if kind == "overloaded_error"
    ));
}
