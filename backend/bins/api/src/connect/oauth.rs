//! The OAuth `state` of a connect ([connectors 5], roadmap E3.3): an HMAC-SHA256 over the
//! connector id, the nonce and the expiry, keyed with [`ConnectorKey::state_key`].
//!
//! The value is four dot-separated fields, every one URL-safe, so it travels in a query string
//! unchanged:
//!
//! ```text
//! <connector id: 26 ULID chars>.<nonce: 22 base64url chars>.<expiry: unix seconds>.<mac: 43 base64url chars>
//! ```
//!
//! The MAC covers the first three fields exactly as written (`"<id>.<nonce>.<expiry>"`); none
//! of them can hold a dot, so the split is unambiguous. It is checked, in constant time, before
//! anything in the value is believed. The state carries its own connector id and expiry because
//! the public callback has no session: it learns from the state alone which connector the
//! redirect belongs to. That a state works once is not in the value: the pending connector's
//! `oauth.nonce` must still equal the state's nonce, and the exchange clears it.
//!
//! [connectors 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#5-connecting-and-tokens

use std::fmt;

use data_encoding::BASE64URL_NOPAD;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use domain::{id::ConnectorId, wire::Timestamp};

use crate::sealing::ConnectorKey;

/// Random bytes in a nonce: 128 bits, 22 base64url characters.
const NONCE_BYTES: usize = 16;
/// A nonce's length on the wire.
const NONCE_LEN: usize = 22;
/// The MAC's length on the wire: 32 bytes of HMAC-SHA256, base64url without padding.
const MAC_LEN: usize = 43;

/// A connect's signed state, as the pending connector's `oauth` field remembers it.
#[derive(Clone, PartialEq, Eq)]
pub struct OAuthState {
    /// The pending connector.
    pub connector: ConnectorId,
    /// 128 random bits, also stored in the pending connector's `oauth.nonce`.
    pub nonce: String,
    /// When the state stops working: 10 minutes after the start, whole seconds.
    pub expires_at: Timestamp,
}

impl fmt::Debug for OAuthState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OAuthState")
            .field("connector", &self.connector)
            .field("nonce", &"[redacted]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Why a state was refused. Every one is `400 invalid_request` on `state`; the variants exist
/// for the log and the tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StateError {
    /// Not four fields of the expected shapes.
    #[error("the state is malformed")]
    Malformed,
    /// The MAC doesn't verify: tampered, or signed under another `CONNECTOR_KEY`.
    #[error("the state's signature doesn't verify")]
    Signature,
    /// Past its expiry.
    #[error("the state has expired")]
    Expired,
}

impl OAuthState {
    /// A new state for `connector` with a fresh random nonce.
    pub fn new(connector: ConnectorId, expires_at: Timestamp) -> Self {
        let mut bytes = [0_u8; NONCE_BYTES];
        aws_lc_rs::rand::fill(&mut bytes)
            .expect("the system RNG fills 16 bytes unless the platform is broken");
        Self {
            connector,
            nonce: BASE64URL_NOPAD.encode(&bytes),
            expires_at,
        }
    }

    /// The value sent as `state` in the consent URL.
    pub fn encode(&self, key: &ConnectorKey) -> String {
        let signed = format!(
            "{}.{}.{}",
            self.connector,
            self.nonce,
            self.expires_at.unix_seconds()
        );
        let mac = mac(key).chain_update(signed.as_bytes()).finalize();
        format!("{signed}.{}", BASE64URL_NOPAD.encode(&mac.into_bytes()))
    }

    /// Checks a state's MAC, then its shape, then its expiry against `now`.
    pub fn decode(value: &str, key: &ConnectorKey, now: Timestamp) -> Result<Self, StateError> {
        let (signed, tag) = value.rsplit_once('.').ok_or(StateError::Malformed)?;
        if tag.len() != MAC_LEN {
            return Err(StateError::Malformed);
        }
        let tag = BASE64URL_NOPAD
            .decode(tag.as_bytes())
            .map_err(|_| StateError::Malformed)?;
        mac(key)
            .chain_update(signed.as_bytes())
            .verify_slice(&tag)
            .map_err(|_| StateError::Signature)?;
        let mut fields = signed.split('.');
        let (Some(connector), Some(nonce), Some(expires), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(StateError::Malformed);
        };
        let connector: ConnectorId = connector.parse().map_err(|_| StateError::Malformed)?;
        let nonce_ok = nonce.len() == NONCE_LEN
            && nonce
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'));
        if !nonce_ok {
            return Err(StateError::Malformed);
        }
        let expires_at: Timestamp = expires
            .parse::<i64>()
            .ok()
            .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
            .map(Timestamp::from)
            .ok_or(StateError::Malformed)?;
        if now >= expires_at {
            return Err(StateError::Expired);
        }
        Ok(Self {
            connector,
            nonce: nonce.to_owned(),
            expires_at,
        })
    }
}

/// HMAC-SHA256 under the state key.
fn mac(key: &ConnectorKey) -> Hmac<Sha256> {
    <Hmac<Sha256> as KeyInit>::new_from_slice(key.state_key())
        .expect("HMAC accepts a key of any length")
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_HEX: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
    const CONNECTOR: &str = "01JAX3K9T6R2B8V0QZ4M7D1E5C";

    fn key() -> ConnectorKey {
        ConnectorKey::from_hex(KEY_HEX).unwrap()
    }

    fn at(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    fn state() -> OAuthState {
        OAuthState::new(CONNECTOR.parse().unwrap(), at("2026-10-19T09:10:00Z"))
    }

    #[test]
    fn a_state_round_trips_until_it_expires() {
        let s = state();
        let value = s.encode(&key());
        assert_eq!(value.len(), 26 + 1 + 22 + 1 + 10 + 1 + 43);
        assert!(
            value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        );
        assert!(value.starts_with(&format!("{CONNECTOR}.")));
        let back = OAuthState::decode(&value, &key(), at("2026-10-19T09:09:59Z")).unwrap();
        assert_eq!(back, s);
        assert_eq!(
            OAuthState::decode(&value, &key(), at("2026-10-19T09:10:00Z")),
            Err(StateError::Expired)
        );
    }

    #[test]
    fn two_states_have_different_nonces() {
        assert_ne!(state().nonce, state().nonce);
    }

    #[test]
    fn a_changed_character_anywhere_is_refused() {
        let value = state().encode(&key());
        let now = at("2026-10-19T09:00:00Z");
        for at in 0..value.len() {
            let mut bytes = value.clone().into_bytes();
            bytes[at] = if bytes[at] == b'A' { b'B' } else { b'A' };
            let tampered = String::from_utf8(bytes).unwrap();
            assert!(
                OAuthState::decode(&tampered, &key(), now).is_err(),
                "byte {at}"
            );
        }
    }

    #[test]
    fn a_later_expiry_with_the_old_mac_is_a_bad_signature() {
        let s = state();
        let value = s.encode(&key());
        let (_, mac) = value.rsplit_once('.').unwrap();
        let forged = format!(
            "{CONNECTOR}.{}.{}.{mac}",
            s.nonce,
            s.expires_at.unix_seconds() + 3600
        );
        assert_eq!(
            OAuthState::decode(&forged, &key(), at("2026-10-19T09:00:00Z")),
            Err(StateError::Signature)
        );
    }

    #[test]
    fn a_state_signed_under_another_key_is_refused() {
        let value = state().encode(&ConnectorKey::from_bytes([7; 32]));
        assert_eq!(
            OAuthState::decode(&value, &key(), at("2026-10-19T09:00:00Z")),
            Err(StateError::Signature)
        );
    }

    #[test]
    fn malformed_states_are_refused() {
        let now = at("2026-10-19T09:00:00Z");
        for value in [
            "",
            "x",
            "a.b.c.d",
            &format!("{CONNECTOR}.n.1.{}", "A".repeat(43)),
        ] {
            assert!(OAuthState::decode(value, &key(), now).is_err(), "{value}");
        }
    }

    #[test]
    fn the_nonce_never_prints() {
        let s = state();
        assert!(!format!("{s:?}").contains(&s.nonce));
    }
}
