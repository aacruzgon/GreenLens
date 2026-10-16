//! `classify_body` against [contracts 2]'s error bodies.

use std::time::Duration;

use reqwest::StatusCode;

use super::{CallError, classify_body};

fn body(code: &str, retryable: bool, retry_after_s: Option<u32>) -> String {
    let mut error = serde_json::json!({
        "code": code,
        "message": "m",
        "retryable": retryable,
        "request_id": "req_01JAX4B2Q8",
    });
    if let Some(s) = retry_after_s {
        error["retry_after_s"] = s.into();
    }
    serde_json::json!({ "error": error }).to_string()
}

#[test]
fn lease_lost_drops_the_job() {
    let e = classify_body(StatusCode::CONFLICT, &body("lease_lost", false, None));
    assert!(matches!(e, CallError::LeaseLost));
}

#[test]
fn another_409_is_not_lease_lost() {
    let e = classify_body(StatusCode::CONFLICT, &body("invalid_state", false, None));
    assert!(matches!(e, CallError::Rejected(_)));
}

#[test]
fn a_retryable_body_is_retried_after_its_retry_after_s() {
    let e = classify_body(
        StatusCode::TOO_MANY_REQUESTS,
        &body("rate_limited", true, Some(7)),
    );
    assert!(matches!(e, CallError::Unavailable { .. }));
    assert_eq!(e.retry_after(), Some(Duration::from_secs(7)));
    let e = classify_body(
        StatusCode::SERVICE_UNAVAILABLE,
        &body("upstream_unavailable", true, Some(5)),
    );
    assert_eq!(e.retry_after(), Some(Duration::from_secs(5)));
}

#[test]
fn a_body_that_says_not_retryable_is_not_retried_whatever_the_status() {
    for (status, code) in [
        (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        (StatusCode::TOO_MANY_REQUESTS, "budget_exceeded"),
        (StatusCode::BAD_REQUEST, "invalid_request"),
        (StatusCode::NOT_FOUND, "not_found"),
    ] {
        let e = classify_body(status, &body(code, false, None));
        assert!(matches!(e, CallError::Rejected(_)), "{status} {code}");
    }
}

#[test]
fn without_an_error_body_only_429_and_5xx_are_retried() {
    let e = classify_body(StatusCode::BAD_GATEWAY, ""); // the proxy's own 502
    assert!(matches!(
        e,
        CallError::Unavailable {
            retry_after: None,
            ..
        }
    ));
    let e = classify_body(StatusCode::TOO_MANY_REQUESTS, "slow down");
    assert!(matches!(
        e,
        CallError::Unavailable {
            retry_after: None,
            ..
        }
    ));
    let e = classify_body(StatusCode::NOT_FOUND, "<html>");
    assert!(matches!(e, CallError::Rejected(_)));
}
