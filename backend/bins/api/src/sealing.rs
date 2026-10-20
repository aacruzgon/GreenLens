//! Sealing a connector's vendor tokens ([connectors 4]): AES-256-GCM under `CONNECTOR_KEY`, a
//! random 96-bit nonce per seal, and the connector's id as the associated data, so a sealed value
//! opens only on the connector it was sealed for. The output, stored in `connector.secret` (and
//! the pending connect's PKCE verifier), is `nonce (12 bytes) || ciphertext || tag (16 bytes)`.
//!
//! It lives in `api` because everything that seals or unseals runs there: the connect routes
//! (E3.3, E3.4), the `connectors` scheduler task that syncs (E4.1) and start-up seeding (E4.6).
//! The `FarmSource` adapters in `integrations` get a token already unsealed ([`VendorToken`]) and
//! never see the key, and `domain` holds only the opaque [`Sealed`] bytes, so neither depends on
//! a cipher.
//!
//! [connectors 4]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#4-data
//! [`VendorToken`]: domain::ports::VendorToken

use std::{fmt, sync::Arc};

use aes_gcm::{
    Aes256Gcm, Key, KeyInit, Nonce,
    aead::{AeadInOut, Generate},
};
use hmac::{Hmac, Mac};
use secrecy::{ExposeSecret, SecretBox, SecretSlice, SecretString, zeroize::Zeroize};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use domain::{id::ConnectorId, ports::Sealed, wire::Timestamp};

/// The nonce's length: 96 bits, GCM's native size.
const NONCE_LEN: usize = 12;
/// The tag's length: 128 bits.
const TAG_LEN: usize = 16;

/// `CONNECTOR_KEY`: 32 bytes, written as 64 hex characters (`openssl rand -hex 32`, the way every
/// other secret is generated, [infra 6]). Holds the expanded AES-256 key, which is wiped on drop;
/// `Debug` never prints it.
///
/// [infra 6]: https://github.com/aacruzgon/GreenLens/blob/main/docs/infra/README.md#6-secrets-and-configuration
#[derive(Clone)]
pub struct ConnectorKey {
    cipher: Arc<Aes256Gcm>,
    /// The OAuth state's HMAC key, derived from the raw key (E3.3, [connectors 5]).
    state_key: Arc<SecretBox<[u8; 32]>>,
}

/// What a sealed value is for. It is part of the associated data, so a value sealed as one
/// purpose never opens as another (the token can't be swapped for the PKCE verifier).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SealPurpose {
    /// The vendor's access token, in `connector.secret`.
    Token,
    /// The pending connect's PKCE verifier.
    Pkce,
}

impl SealPurpose {
    fn prefix(self) -> &'static str {
        match self {
            Self::Token => "token:",
            Self::Pkce => "pkce:",
        }
    }
}

/// The label the OAuth-state key is derived under.
const STATE_KEY_LABEL: &[u8] = b"pdm/connector-oauth-state/v1";

impl fmt::Debug for ConnectorKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ConnectorKey([REDACTED])")
    }
}

/// Why `CONNECTOR_KEY` was refused. The message never quotes the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    /// Not hexadecimal.
    #[error("expected 64 hex characters (openssl rand -hex 32)")]
    NotHex,
    /// Hexadecimal, but not 32 bytes.
    #[error("expected 32 bytes (64 hex characters), got {0} bytes")]
    Length(usize),
}

/// Why a sealed value didn't open. Deliberately opaque: AES-GCM can't tell a wrong key, a wrong
/// connector and a changed byte apart, and saying more would help no one but an attacker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UnsealError {
    /// Shorter than a nonce and a tag: not something [`ConnectorKey::seal`] wrote.
    #[error("the sealed value is too short")]
    TooShort,
    /// The tag didn't verify: another key, another connector, or a changed byte.
    #[error("the sealed value doesn't open under this key for this connector")]
    Rejected,
}

impl ConnectorKey {
    /// Parses the variable's value: 64 hex characters, either case, surrounding space ignored.
    pub fn from_hex(value: &str) -> Result<Self, KeyError> {
        let mut bytes = data_encoding::HEXLOWER_PERMISSIVE
            .decode(value.trim().as_bytes())
            .map_err(|_| KeyError::NotHex)?;
        let key = <[u8; 32]>::try_from(bytes.as_slice()).map(Self::from_bytes);
        let len = bytes.len();
        bytes.zeroize();
        key.map_err(|_| KeyError::Length(len))
    }

    /// A key from its 32 bytes. The caller wipes its own copy.
    pub fn from_bytes(mut bytes: [u8; 32]) -> Self {
        let mut key = Key::<Aes256Gcm>::from(bytes);
        let cipher = Aes256Gcm::new(&key);
        key.zeroize();
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(&bytes)
            .expect("HMAC accepts a key of any length");
        mac.update(STATE_KEY_LABEL);
        let mut derived: [u8; 32] = mac.finalize().into_bytes().into();
        bytes.zeroize();
        let state_key = SecretBox::new(Box::new(derived));
        derived.zeroize();
        Self {
            cipher: Arc::new(cipher),
            state_key: Arc::new(state_key),
        }
    }

    /// The key that signs the OAuth `state` (E3.3): HMAC-SHA256 of the raw key under a fixed
    /// label, so it differs from the AES key and rotating `CONNECTOR_KEY` rotates both.
    pub fn state_key(&self) -> &[u8; 32] {
        self.state_key.expose_secret()
    }

    /// Seals `plaintext` for `connector`, with a fresh random nonce, so sealing the same token
    /// twice gives two different values.
    pub fn seal(&self, purpose: SealPurpose, connector: ConnectorId, plaintext: &[u8]) -> Sealed {
        let nonce = Nonce::generate();
        let mut body = Vec::with_capacity(plaintext.len() + TAG_LEN);
        body.extend_from_slice(plaintext);
        self.cipher
            .encrypt_in_place(
                &nonce,
                associated_data(purpose, connector).as_bytes(),
                &mut body,
            )
            .expect(
                "AES-GCM refuses only a message over 64 GiB, and a token is a few hundred bytes",
            );
        let mut sealed = Vec::with_capacity(NONCE_LEN + body.len());
        sealed.extend_from_slice(&nonce);
        sealed.extend_from_slice(&body);
        Sealed(sealed.into())
    }

    /// Opens a value sealed for `connector`. The plaintext comes back as a secret, wiped when
    /// dropped. The deletion runner opens tokens to revoke them (E3.6); the routes (E3.4) and
    /// the sync (E4.1) open them too.
    pub fn unseal(
        &self,
        purpose: SealPurpose,
        connector: ConnectorId,
        sealed: &Sealed,
    ) -> Result<SecretSlice<u8>, UnsealError> {
        if sealed.0.len() < NONCE_LEN + TAG_LEN {
            return Err(UnsealError::TooShort);
        }
        let (nonce, body) = sealed.0.split_at(NONCE_LEN);
        let nonce = Nonce::try_from(nonce).map_err(|_| UnsealError::TooShort)?;
        let mut body = body.to_vec();
        let opened = self.cipher.decrypt_in_place(
            &nonce,
            associated_data(purpose, connector).as_bytes(),
            &mut body,
        );
        if opened.is_err() {
            body.zeroize();
            return Err(UnsealError::Rejected);
        }
        Ok(SecretSlice::from(body))
    }
}

/// The associated data: the purpose, then the connector's id as it is written on the wire.
fn associated_data(purpose: SealPurpose, connector: ConnectorId) -> String {
    format!("{}{connector}", purpose.prefix())
}

/// A connector's vendor tokens, as they are sealed in `connector.secret` with
/// [`SealPurpose::Token`]: the one reader and writer of that value, for the connect routes, the
/// deletion runner's revoke (E3.6) and the sync (E4). Two forms:
///
/// - **A plain string**, the token's own bytes: Slack's `xoxb-…` bot token, which has neither a
///   refresh token nor an expiry, and a seeded connector's fixed token (roadmap E4.6).
/// - **A JSON object** `{"access_token", "refresh_token"?, "expires_at"?}`: Google's grant.
///   `expires_at` is RFC 3339. A revoke prefers `refresh_token`, which ends the whole grant.
///
/// [`seal`](Self::seal) writes the plain string exactly when there is no refresh token and no
/// expiry; [`unseal`](Self::unseal) reads a value that starts with `{` as JSON, anything else
/// as a plain token.
#[derive(Clone)]
pub struct StoredTokens {
    /// Slack's `xoxb-…` bot token, or Google's access token.
    pub access_token: SecretString,
    /// Google's refresh token.
    pub refresh_token: Option<SecretString>,
    /// When Google's access token expires.
    pub expires_at: Option<Timestamp>,
}

impl fmt::Debug for StoredTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoredTokens")
            .field("refresh_token", &self.refresh_token.is_some())
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

/// The JSON inside the seal. No `Debug`: it holds the tokens in the clear.
#[derive(Serialize, Deserialize)]
struct TokensJson {
    access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expires_at: Option<Timestamp>,
}

/// Why sealed tokens couldn't be read.
#[derive(Debug, thiserror::Error)]
pub enum TokensError {
    /// The seal didn't open under this key for this connector.
    #[error("the connector's tokens don't open")]
    Unseal(#[source] UnsealError),
    /// It opened, but isn't what [`StoredTokens::seal`] writes.
    #[error("the connector's tokens are unreadable")]
    Format,
}

impl StoredTokens {
    /// A bare access token, with no refresh token or expiry: Slack's bot token.
    pub fn bearer(token: SecretString) -> Self {
        Self {
            access_token: token,
            refresh_token: None,
            expires_at: None,
        }
    }

    /// Seals the tokens for `connector`: a bare bearer token as its plain string, a grant
    /// with a refresh token or an expiry as JSON.
    pub fn seal(&self, key: &ConnectorKey, connector: ConnectorId) -> Sealed {
        if self.refresh_token.is_none() && self.expires_at.is_none() {
            return key.seal(
                SealPurpose::Token,
                connector,
                self.access_token.expose_secret().as_bytes(),
            );
        }
        let json = TokensJson {
            access_token: self.access_token.expose_secret().to_owned(),
            refresh_token: self
                .refresh_token
                .as_ref()
                .map(|t| t.expose_secret().to_owned()),
            expires_at: self.expires_at,
        };
        let mut plain =
            serde_json::to_vec(&json).expect("strings and a timestamp always serialise");
        let TokensJson {
            mut access_token,
            refresh_token,
            ..
        } = json;
        access_token.zeroize();
        if let Some(mut t) = refresh_token {
            t.zeroize();
        }
        let sealed = key.seal(SealPurpose::Token, connector, &plain);
        plain.zeroize();
        sealed
    }

    /// Opens the tokens sealed for `connector`.
    pub fn unseal(
        key: &ConnectorKey,
        connector: ConnectorId,
        sealed: &Sealed,
    ) -> Result<Self, TokensError> {
        let plain = key
            .unseal(SealPurpose::Token, connector, sealed)
            .map_err(TokensError::Unseal)?;
        let bytes = plain.expose_secret();
        if bytes.first() != Some(&b'{') {
            let token = std::str::from_utf8(bytes).map_err(|_| TokensError::Format)?;
            if token.is_empty() {
                return Err(TokensError::Format);
            }
            return Ok(Self::bearer(SecretString::from(token)));
        }
        let json: TokensJson = serde_json::from_slice(bytes).map_err(|_| TokensError::Format)?;
        Ok(Self {
            access_token: SecretString::from(json.access_token),
            refresh_token: json.refresh_token.map(SecretString::from),
            expires_at: json.expires_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        sync::{Arc, Mutex},
    };

    use secrecy::ExposeSecret;
    use tracing_subscriber::fmt::MakeWriter;

    use super::*;

    const KEY_HEX: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
    const T: SealPurpose = SealPurpose::Token;
    const TOKEN: &[u8] = b"xoxb-1234-5678-the-bot-token";

    fn key() -> ConnectorKey {
        ConnectorKey::from_hex(KEY_HEX).unwrap()
    }

    fn connector(n: u8) -> ConnectorId {
        format!("01JAX3K9T6R2B8V0QZ4M7D1E5{}", char::from(b'A' + n))
            .parse()
            .unwrap()
    }

    #[test]
    fn a_sealed_token_opens_for_its_connector() {
        let sealed = key().seal(T, connector(0), TOKEN);
        assert_eq!(sealed.0.len(), NONCE_LEN + TOKEN.len() + TAG_LEN);
        let opened = key().unseal(T, connector(0), &sealed).unwrap();
        assert_eq!(opened.expose_secret(), TOKEN);
        let empty = key().seal(T, connector(0), b"");
        assert_eq!(
            key()
                .unseal(T, connector(0), &empty)
                .unwrap()
                .expose_secret(),
            b""
        );
    }

    #[test]
    fn a_sealed_token_never_opens_for_another_connector() {
        let sealed = key().seal(T, connector(0), TOKEN);
        assert_eq!(
            key().unseal(T, connector(1), &sealed).unwrap_err(),
            UnsealError::Rejected
        );
    }

    #[test]
    fn a_sealed_token_never_opens_under_another_key() {
        let sealed = key().seal(T, connector(0), TOKEN);
        let other = ConnectorKey::from_bytes([7; 32]);
        assert_eq!(
            other.unseal(T, connector(0), &sealed).unwrap_err(),
            UnsealError::Rejected
        );
    }

    #[test]
    fn any_changed_byte_is_refused() {
        let sealed = key().seal(T, connector(0), TOKEN);
        for at in 0..sealed.0.len() {
            let mut bytes = sealed.0.to_vec();
            bytes[at] ^= 0x01;
            assert_eq!(
                key()
                    .unseal(T, connector(0), &Sealed(bytes.into()))
                    .unwrap_err(),
                UnsealError::Rejected,
                "byte {at}"
            );
        }
        let cut = Sealed(sealed.0.slice(..NONCE_LEN + TAG_LEN - 1));
        assert_eq!(
            key().unseal(T, connector(0), &cut).unwrap_err(),
            UnsealError::TooShort
        );
    }

    #[test]
    fn a_value_sealed_for_one_purpose_never_opens_for_the_other() {
        let sealed = key().seal(SealPurpose::Token, connector(0), TOKEN);
        assert_eq!(
            key()
                .unseal(SealPurpose::Pkce, connector(0), &sealed)
                .unwrap_err(),
            UnsealError::Rejected
        );
        let pkce = key().seal(SealPurpose::Pkce, connector(0), TOKEN);
        assert!(key().unseal(SealPurpose::Pkce, connector(0), &pkce).is_ok());
        assert!(
            key()
                .unseal(SealPurpose::Token, connector(0), &pkce)
                .is_err()
        );
    }

    #[test]
    fn the_state_key_is_derived_and_kept_secret() {
        let a = key();
        assert_eq!(a.state_key(), key().state_key());
        assert_ne!(a.state_key(), ConnectorKey::from_bytes([7; 32]).state_key());
        let raw = data_encoding::HEXLOWER.decode(KEY_HEX.as_bytes()).unwrap();
        assert_ne!(a.state_key().as_slice(), raw.as_slice());
        let derived_hex = data_encoding::HEXLOWER.encode(a.state_key());
        assert!(!format!("{a:?}").contains(&derived_hex));
        assert_eq!(format!("{a:?}"), "ConnectorKey([REDACTED])");
    }

    #[test]
    fn two_seals_of_one_token_differ() {
        let first = key().seal(T, connector(0), TOKEN);
        let second = key().seal(T, connector(0), TOKEN);
        assert_ne!(first.0[..NONCE_LEN], second.0[..NONCE_LEN]);
        assert_ne!(first, second);
    }

    #[test]
    fn the_key_must_be_32_bytes_of_hex() {
        assert!(ConnectorKey::from_hex(&KEY_HEX.to_uppercase()).is_ok());
        assert!(ConnectorKey::from_hex(&format!("  {KEY_HEX}\n")).is_ok());
        assert_eq!(
            ConnectorKey::from_hex(&KEY_HEX[..62]).unwrap_err(),
            KeyError::Length(31)
        );
        assert_eq!(
            ConnectorKey::from_hex(&format!("{KEY_HEX}00")).unwrap_err(),
            KeyError::Length(33)
        );
        assert_eq!(
            ConnectorKey::from_hex(&"zz".repeat(32)).unwrap_err(),
            KeyError::NotHex
        );
        // base64 of 32 bytes is refused rather than read as something else
        assert_eq!(
            ConnectorKey::from_hex("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=").unwrap_err(),
            KeyError::NotHex
        );
    }

    /// Collects everything a subscriber writes.
    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl io::Write for Captured {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Captured {
        type Writer = Self;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[test]
    fn neither_the_key_nor_a_token_reaches_a_log_line() {
        let key = key();
        let sealed = key.seal(T, connector(0), TOKEN);
        let opened = key.unseal(T, connector(0), &sealed).unwrap();
        let error = key.unseal(T, connector(1), &sealed).unwrap_err();
        let logs = Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_writer(logs.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(?key, ?sealed, ?opened, %error, "connector token");
        });
        let line = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
        assert!(line.contains("connector token"), "{line}");
        assert!(line.contains("ConnectorKey([REDACTED])"), "{line}");
        assert!(
            line.contains(&format!("Sealed({} bytes)", sealed.0.len())),
            "{line}"
        );
        let token = std::str::from_utf8(TOKEN).unwrap();
        for secret in [token, KEY_HEX, &KEY_HEX.to_uppercase()] {
            assert!(!line.contains(secret), "{secret} in {line}");
        }
        let sealed_hex = data_encoding::HEXLOWER.encode(&sealed.0);
        assert!(!line.contains(&sealed_hex[..24]), "{line}");
        assert!(!format!("{key:?}{sealed:?}{opened:?}{error}").contains(token));
    }

    #[test]
    fn tokens_round_trip_through_the_seal() {
        let tokens = StoredTokens {
            access_token: SecretString::from("ya29.access"),
            refresh_token: Some(SecretString::from("1//refresh")),
            expires_at: Some("2026-10-19T10:00:00Z".parse().unwrap()),
        };
        let sealed = tokens.seal(&key(), connector(0));
        let back = StoredTokens::unseal(&key(), connector(0), &sealed).unwrap();
        assert_eq!(back.access_token.expose_secret(), "ya29.access");
        assert_eq!(
            back.refresh_token.as_ref().map(ExposeSecret::expose_secret),
            Some("1//refresh")
        );
        assert_eq!(back.expires_at, tokens.expires_at);
        assert!(!format!("{back:?}").contains("ya29"));
        let other = connector(1);
        assert!(matches!(
            StoredTokens::unseal(&key(), other, &sealed),
            Err(TokensError::Unseal(_))
        ));
    }

    #[test]
    fn a_slack_token_is_sealed_as_its_plain_string() {
        let sealed =
            StoredTokens::bearer(SecretString::from("xoxb-1234-bot")).seal(&key(), connector(0));
        let plain = key()
            .unseal(SealPurpose::Token, connector(0), &sealed)
            .unwrap();
        assert_eq!(plain.expose_secret(), b"xoxb-1234-bot");
    }

    #[test]
    fn a_google_grant_is_sealed_as_json() {
        let tokens = StoredTokens {
            access_token: SecretString::from("ya29.access"),
            refresh_token: Some(SecretString::from("1//refresh")),
            expires_at: Some("2026-10-19T10:00:00Z".parse().unwrap()),
        };
        let sealed = tokens.seal(&key(), connector(0));
        let plain = key()
            .unseal(SealPurpose::Token, connector(0), &sealed)
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(plain.expose_secret()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "access_token": "ya29.access",
                "refresh_token": "1//refresh",
                "expires_at": "2026-10-19T10:00:00Z"
            })
        );
    }

    #[test]
    fn a_bare_sealed_token_reads_as_a_bearer_token() {
        let sealed = key().seal(SealPurpose::Token, connector(0), b"xoxb-era-demo");
        let tokens = StoredTokens::unseal(&key(), connector(0), &sealed).unwrap();
        assert_eq!(tokens.access_token.expose_secret(), "xoxb-era-demo");
        assert!(tokens.refresh_token.is_none() && tokens.expires_at.is_none());
        let pkce = key().seal(SealPurpose::Pkce, connector(0), b"xoxb-era-demo");
        assert!(StoredTokens::unseal(&key(), connector(0), &pkce).is_err());
    }
}
