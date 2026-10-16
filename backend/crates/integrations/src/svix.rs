//! The auth webhook ([contracts 9], auth webhook): Svix's signature scheme, verified by hand,
//! and Clerk's payload mapped to [`AuthWebhookEvent`].
//!
//! The signed content is `{svix-id}.{svix-timestamp}.{raw body}`, HMAC-SHA256 with the bytes of
//! the base64 secret after `whsec_`, base64 output. `svix-signature` holds space-separated
//! `v1,<base64>` entries (several during a secret rotation); any one matching is enough. The
//! check runs on the raw bytes before the body is parsed, compares in constant time, and
//! rejects a timestamp more than 5 minutes away from `now` in either direction. The `svix`
//! crate (the full API client, ~19 extra crates) is only a dev-dependency, as the test oracle.

use aws_lc_rs::{constant_time, hmac};
use domain::{
    garden::webhook::{
        AuthEventData, AuthEventType, AuthWebhookEvent, WEBHOOK_TOLERANCE, WebhookHeaders,
    },
    id::AuthId,
    ports::{AuthError, WebhookVerifier},
    wire::Timestamp,
};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

/// The secret didn't decode.
#[derive(Debug, thiserror::Error)]
#[error("webhook signing secret is not whsec_<base64>")]
pub struct BadSecret;

/// The Svix verifier with Clerk's payload mapping.
pub struct SvixVerifier {
    key: hmac::Key,
}

impl std::fmt::Debug for SvixVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SvixVerifier")
    }
}

impl SvixVerifier {
    /// From the endpoint's signing secret, `whsec_<base64>` (the prefix is optional, as in the
    /// `svix` crate).
    pub fn new(secret: &SecretString) -> Result<Self, BadSecret> {
        let s = secret.expose_secret();
        let b64 = s.strip_prefix("whsec_").unwrap_or(s);
        let raw = data_encoding::BASE64
            .decode(b64.as_bytes())
            .map_err(|_| BadSecret)?;
        if raw.is_empty() {
            return Err(BadSecret);
        }
        Ok(Self {
            key: hmac::Key::new(hmac::HMAC_SHA256, &raw),
        })
    }

    /// Checks the signature and timestamp only. `'a` is the request: the headers and body are
    /// borrowed for the check and nothing outlives it.
    pub fn verify_signature<'a>(
        &self,
        headers: WebhookHeaders<'a>,
        body: &'a [u8],
        now: Timestamp,
    ) -> Result<(), AuthError> {
        let ts: i64 = headers
            .timestamp
            .trim()
            .parse()
            .map_err(|_| AuthError::InvalidWebhook)?;
        let tolerance = i64::try_from(WEBHOOK_TOLERANCE.as_secs()).unwrap_or(300);
        if (now.unix_seconds() - ts).abs() > tolerance {
            return Err(AuthError::InvalidWebhook);
        }
        let mut ctx = hmac::Context::with_key(&self.key);
        ctx.update(headers.id.as_bytes());
        ctx.update(b".");
        ctx.update(headers.timestamp.as_bytes());
        ctx.update(b".");
        ctx.update(body);
        let expected = ctx.sign();
        let matched = headers
            .signature
            .split(' ')
            .filter_map(|entry| entry.strip_prefix("v1,"))
            .filter_map(|b64| data_encoding::BASE64.decode(b64.as_bytes()).ok())
            // `fold`, not `any`: every entry is compared, so timing doesn't reveal which matched.
            .fold(false, |acc, got| {
                acc | constant_time::verify_slices_are_equal(expected.as_ref(), &got).is_ok()
            });
        if matched {
            Ok(())
        } else {
            Err(AuthError::InvalidWebhook)
        }
    }
}

impl WebhookVerifier for SvixVerifier {
    fn verify<'a>(
        &self,
        headers: WebhookHeaders<'a>,
        body: &'a [u8],
        now: Timestamp,
    ) -> Result<Option<AuthWebhookEvent>, AuthError> {
        self.verify_signature(headers, body, now)?;
        Ok(map_clerk_event(body))
    }
}

/// Clerk's event envelope: `{object: "event", type, data, timestamp, instance_id}`. Only `type`
/// and `data.id` are read; for `user.deleted`, `data` is a `DeletedObject` whose `id` is
/// optional in Clerk's `OpenAPI` schema.
#[derive(Debug, Deserialize)]
struct ClerkEvent<'b> {
    #[serde(rename = "type", borrow)]
    kind: std::borrow::Cow<'b, str>,
    #[serde(default)]
    data: ClerkEventData,
}

#[derive(Debug, Default, Deserialize)]
struct ClerkEventData {
    #[serde(default)]
    id: Option<String>,
}

/// Maps a verified body to the domain event. Deterministic, so a redelivered event (same
/// `svix-id`) maps to the same value and the handler's upsert/cascade keeps it idempotent.
/// `None` for other event types, for a `user.*` event without `data.id`, and for a body that
/// doesn't parse (logged: Svix would only redeliver the same bytes).
///
/// `ClerkEvent<'b>` borrows its `type` from the body (`'b`) instead of allocating it.
pub fn map_clerk_event(body: &[u8]) -> Option<AuthWebhookEvent> {
    let event: ClerkEvent<'_> = match serde_json::from_slice(body) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, "signed auth webhook body did not parse; ignored");
            return None;
        }
    };
    let event_type = match event.kind.as_ref() {
        "user.created" => AuthEventType::UserCreated,
        "user.deleted" => AuthEventType::UserDeleted,
        _ => return None,
    };
    let id: AuthId = event.data.id?.parse().ok()?;
    Some(AuthWebhookEvent {
        event_type,
        data: AuthEventData { id },
    })
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone as _, Utc};

    use super::*;

    /// A made-up secret: the test signs with the svix crate and verifies with the same one.
    const SECRET: &str = "whsec_cGRtLXRlc3Qtd2ViaG9vay1zZWNyZXQtbm90LXJlYWw=";
    const NOW: i64 = 1_792_000_000;

    fn now() -> Timestamp {
        Timestamp::from(Utc.timestamp_opt(NOW, 0).unwrap())
    }

    fn verifier() -> SvixVerifier {
        SvixVerifier::new(&SecretString::from(SECRET)).unwrap()
    }

    fn oracle_sign(id: &str, ts: i64, body: &[u8]) -> String {
        svix::webhooks::Webhook::new(SECRET)
            .unwrap()
            .sign(id, ts, body)
            .unwrap()
    }

    const CREATED: &[u8] =
        br#"{"object":"event","type":"user.created","data":{"id":"user_29w83sxmDNGwOuEthce5gg56FcC","first_name":"A"},"timestamp":1654012591835,"instance_id":"ins_1"}"#;

    fn headers<'a>(id: &'a str, ts: &'a str, sig: &'a str) -> WebhookHeaders<'a> {
        WebhookHeaders {
            id,
            timestamp: ts,
            signature: sig,
        }
    }

    #[test]
    fn accepts_what_the_svix_crate_signs() {
        let ts = NOW.to_string();
        let sig = oracle_sign("msg_1", NOW, CREATED);
        let event = verifier()
            .verify(headers("msg_1", &ts, &sig), CREATED, now())
            .unwrap()
            .unwrap();
        assert_eq!(event.event_type, AuthEventType::UserCreated);
        assert_eq!(event.data.id.as_str(), "user_29w83sxmDNGwOuEthce5gg56FcC");
    }

    #[test]
    fn any_v1_entry_may_match() {
        let ts = NOW.to_string();
        let good = oracle_sign("msg_1", NOW, CREATED);
        let rotated = format!("v1,AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA= v2,xyz {good}");
        assert!(
            verifier()
                .verify(headers("msg_1", &ts, &rotated), CREATED, now())
                .is_ok()
        );
    }

    #[test]
    fn rejects_tampering_and_stale_timestamps() {
        let v = verifier();
        let ts = NOW.to_string();
        let sig = oracle_sign("msg_1", NOW, CREATED);
        let tampered = br#"{"object":"event","type":"user.deleted","data":{"id":"user_x"}}"#;
        assert!(matches!(
            v.verify(headers("msg_1", &ts, &sig), tampered, now()),
            Err(AuthError::InvalidWebhook)
        ));
        assert!(
            v.verify(headers("msg_2", &ts, &sig), CREATED, now())
                .is_err()
        );
        for skew in [-301, 301] {
            let t = NOW + skew;
            let s = oracle_sign("msg_1", t, CREATED);
            assert!(
                v.verify(headers("msg_1", &t.to_string(), &s), CREATED, now())
                    .is_err()
            );
        }
        let t = NOW - 299;
        let s = oracle_sign("msg_1", t, CREATED);
        assert!(
            v.verify(headers("msg_1", &t.to_string(), &s), CREATED, now())
                .is_ok()
        );
    }

    #[test]
    fn maps_clerk_events() {
        let deleted = br#"{"object":"event","type":"user.deleted","data":{"deleted":true,"id":"user_1","object":"user"}}"#;
        assert_eq!(
            map_clerk_event(deleted).map(|e| e.event_type),
            Some(AuthEventType::UserDeleted)
        );
        assert_eq!(
            map_clerk_event(br#"{"type":"user.updated","data":{"id":"user_1"}}"#),
            None
        );
        assert_eq!(
            map_clerk_event(br#"{"type":"user.deleted","data":{"deleted":true}}"#),
            None
        );
        assert_eq!(map_clerk_event(b"not json"), None);
        assert_eq!(
            map_clerk_event(CREATED),
            map_clerk_event(CREATED),
            "deterministic"
        );
    }
}
