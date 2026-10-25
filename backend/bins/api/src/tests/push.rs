//! Push notifications up to the `Notifier` port ([contracts 9], push notifications and
//! account): who `Pusher` sends to, which tokens it forgets, which scan events push, and the
//! token check of `POST /me/push-tokens`.

use std::sync::Mutex;

use domain::{
    events::SupersedeReason,
    garden::{
        account::Platform,
        push::{PushMessage, PushPayload, PushScanReason},
    },
    id::{PlanId, PushToken, QuestionId},
    ports::{PushReport, PushTarget},
    scan::{Question, QuestionKind, QuestionStatus, ScanFailure, ScanFailureCode},
};

use super::*;

const OTHER_USER: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5V";

fn token(s: &str) -> PushToken {
    s.parse().unwrap()
}

fn target(user: &str, t: &str, platform: Platform) -> PushTarget {
    PushTarget {
        user: user.parse().unwrap(),
        token: token(t),
        platform,
    }
}

fn alert() -> PushPayload {
    PushPayload::GardenDeleted {
        v: V1,
        garden_name: "Back yard".to_owned(),
    }
}

/// Every member's iOS and Android Expo tokens get the message; a `web` token and an `android`
/// token stored before the check (a raw FCM token) are skipped; a `DeviceNotRegistered` token
/// is forgotten.
#[tokio::test]
async fn pusher_sends_to_expo_tokens_and_forgets_unregistered_ones() {
    let mut store = FakeStore::default();
    store
        .identity_repo
        .expect_push_targets()
        .times(1)
        .returning(|users| {
            assert_eq!(users.len(), 2);
            Ok(vec![
                target(USER, "ExponentPushToken[ios-a]", Platform::Ios),
                target(USER, "ExponentPushToken[web-b]", Platform::Web),
                target(OTHER_USER, "dVhd7k3HQJ6h7Wq1:APA91bH2k", Platform::Android),
                target(OTHER_USER, "ExpoPushToken[android-d]", Platform::Android),
            ])
        });
    store
        .identity_repo
        .expect_forget_push_tokens()
        .times(1)
        .returning(|tokens| {
            assert_eq!(tokens, [token("ExpoPushToken[android-d]")]);
            Ok(())
        });
    let sent: Arc<Mutex<Vec<PushMessage>>> = Arc::default();
    let mut notifier = MockNotifier::new();
    let seen = Arc::clone(&sent);
    notifier.expect_push().times(1).returning(move |messages| {
        seen.lock().unwrap().extend(messages);
        Ok(PushReport {
            accepted: 1,
            unregistered: vec![token("ExpoPushToken[android-d]")],
            failed: 0,
        })
    });
    let pusher = Pusher::new(Arc::new(store), Arc::new(notifier));
    let users: Vec<UserId> = vec![USER.parse().unwrap(), OTHER_USER.parse().unwrap()];
    pusher
        .send(&users, "Garden deleted", "Back yard was deleted.", alert())
        .await;
    let sent = sent.lock().unwrap();
    let to: Vec<&str> = sent.iter().map(|m| m.to.as_str()).collect();
    assert_eq!(to, ["ExponentPushToken[ios-a]", "ExpoPushToken[android-d]"]);
    assert!(
        sent.iter()
            .all(|m| m.data == alert() && m.title == "Garden deleted")
    );
}

/// Nobody with a deliverable token: nothing is sent and nothing is forgotten.
#[tokio::test]
async fn pusher_sends_nothing_without_a_deliverable_token() {
    let mut store = FakeStore::default();
    store
        .identity_repo
        .expect_push_targets()
        .returning(|_| Ok(vec![target(USER, "ExponentPushToken[web]", Platform::Web)]));
    store.identity_repo.expect_forget_push_tokens().never();
    let mut notifier = MockNotifier::new();
    notifier.expect_push().never();
    let pusher = Pusher::new(Arc::new(store), Arc::new(notifier));
    pusher
        .send(&[USER.parse().unwrap()], "t", "b", alert())
        .await;
}

/// A failed send is logged and dropped: no token is forgotten.
#[tokio::test]
async fn a_failed_send_forgets_nothing() {
    let mut store = FakeStore::default();
    store
        .identity_repo
        .expect_push_targets()
        .returning(|_| Ok(vec![target(USER, "ExponentPushToken[a]", Platform::Ios)]));
    store.identity_repo.expect_forget_push_tokens().never();
    let mut notifier = MockNotifier::new();
    notifier.expect_push().times(1).returning(|_| {
        Err(domain::ports::ProviderError::Unavailable {
            provider: "expo_push",
            source: "HTTP 503".into(),
        })
    });
    let pusher = Pusher::new(Arc::new(store), Arc::new(notifier));
    pusher
        .send(&[USER.parse().unwrap()], "t", "b", alert())
        .await;
}

#[test]
fn scan_events_map_to_push_reasons() {
    let plan: PlanId = "01JAX3K9T6R2B8V0QZ4M7D1E5N".parse().unwrap();
    let newer: PlanId = "01JAX3K9T6R2B8V0QZ4M7D1E5M".parse().unwrap();
    let superseded = |reason| ScanEventBody::PlanSuperseded {
        old_plan_id: plan,
        new_plan_id: newer,
        reason,
    };
    let question = Question {
        id: "01JAX3K9T6R2B8V0QZ4M7D1E5Q".parse::<QuestionId>().unwrap(),
        run: Run(1),
        kind: QuestionKind::PhotoRequest,
        text: "Photograph the underside of a leaf.".to_owned(),
        options: None,
        indicator_id: None,
        photo_kind: None,
        photo_hint: None,
        candidates: Vec::new(),
        asked_at: NOW.parse().unwrap(),
        continue_at: NOW.parse().unwrap(),
        status: QuestionStatus::Open,
    };
    let cases = [
        (
            ScanEventBody::PlanReady {
                plan_id: plan,
                provisional: false,
            },
            Some(PushScanReason::PlanReady),
        ),
        (
            superseded(SupersedeReason::LateAnswer),
            Some(PushScanReason::PlanSuperseded),
        ),
        (
            superseded(SupersedeReason::SpeciesChanged),
            Some(PushScanReason::PlanSuperseded),
        ),
        (
            superseded(SupersedeReason::GuidedCheck),
            Some(PushScanReason::PlanSuperseded),
        ),
        // The member who confirmed a chat revision is looking at it.
        (superseded(SupersedeReason::ChatRevision), None),
        (
            ScanEventBody::Question { question },
            Some(PushScanReason::Question),
        ),
        (
            ScanEventBody::Failed {
                failure: ScanFailure::new(ScanFailureCode::NoPlantFound, "No plant found."),
            },
            Some(PushScanReason::Failed),
        ),
        (
            ScanEventBody::StatusChanged {
                from: ScanStatus::Queued,
                to: ScanStatus::Analysing,
            },
            None,
        ),
        (
            ScanEventBody::StageStarted {
                stage: Stage::Species,
            },
            None,
        ),
    ];
    for (event, reason) in cases {
        assert_eq!(Pusher::scan_reason(&event), reason, "{event:?}");
    }
}

async fn register(app: axum::Router, body: &str) -> axum::response::Response {
    app.oneshot(
        Request::post("/me/push-tokens")
            .header(header::AUTHORIZATION, "Bearer session-token")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_owned()))
            .unwrap(),
    )
    .await
    .unwrap()
}

/// An `ios` or `android` token must be an Expo push token: a raw APNs or FCM token is `400
/// invalid_request` on `token` and never stored.
#[tokio::test]
async fn a_raw_device_token_is_refused() {
    for body in [
        r#"{"token":"740f4707bebcf74f9b7c25d48e3358945f6aa01da5ddb387462c7eaf61bb78ad","platform":"ios"}"#,
        r#"{"token":"dVhd7k3HQJ6h7Wq1:APA91bH2k","platform":"android"}"#,
        r#"{"token":"ExponentPushToken[]","platform":"ios"}"#,
    ] {
        let mut store = FakeStore::default();
        let auth = signed_in(&mut store, None);
        store.identity_repo.expect_register_push_token().never();
        let app = crate::router::app(state(store, auth, MockBlobStore::new(), no_images()));
        let response = register(app, body).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
        let error = json(response).await;
        assert_eq!(error["error"]["code"], "invalid_request");
        assert_eq!(error["error"]["details"]["field"], "token");
    }
}

/// Expo push tokens of both spellings are stored, and so is a `web` token as given.
#[tokio::test]
async fn expo_tokens_and_web_tokens_are_stored() {
    for (body, platform) in [
        (
            r#"{"token":"ExponentPushToken[xxxxxxxxxxxxxxxxxxxxxx]","platform":"ios"}"#,
            Platform::Ios,
        ),
        (
            r#"{"token":"ExpoPushToken[xxxxxxxxxxxxxxxxxxxxxx]","platform":"android"}"#,
            Platform::Android,
        ),
        (
            r#"{"token":"https://fcm.googleapis.com/fcm/send/abc","platform":"web"}"#,
            Platform::Web,
        ),
    ] {
        let mut store = FakeStore::default();
        let auth = signed_in(&mut store, None);
        store
            .identity_repo
            .expect_register_push_token()
            .times(1)
            .returning(move |user, request| {
                assert_eq!(user.to_string(), USER);
                assert_eq!(request.platform, platform);
                Ok(())
            });
        let app = crate::router::app(state(store, auth, MockBlobStore::new(), no_images()));
        let response = register(app, body).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT, "{body}");
    }
}
