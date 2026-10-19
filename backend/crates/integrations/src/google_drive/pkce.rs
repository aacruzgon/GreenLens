//! PKCE for Google's authorization-code flow ([RFC 7636]), with the `S256` method only.
//!
//! The API keeps the verifier sealed in the pending connector's `oauth` field and sends the
//! challenge in the consent URL ([connectors 5]).
//!
//! [RFC 7636]: https://datatracker.ietf.org/doc/html/rfc7636
//! [connectors 5]: ../../../../docs/connectors/README.md#5-connecting-and-tokens

use aws_lc_rs::digest;
use data_encoding::BASE64URL_NOPAD;
use secrecy::{ExposeSecret, SecretString};

/// A PKCE verifier and its `S256` challenge.
#[derive(Debug)]
pub struct Pkce {
    verifier: SecretString,
    challenge: String,
}

/// A verifier that RFC 7636 4.1 doesn't allow: 43 to 128 characters of
/// `[A-Z] / [a-z] / [0-9] / "-" / "." / "_" / "~"`.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("PKCE verifier must be 43 to 128 unreserved characters")]
pub struct InvalidVerifier;

impl Pkce {
    /// The `code_challenge_method` sent with the challenge.
    pub const METHOD: &'static str = "S256";

    /// A new pair: 32 random bytes from the system's RNG, base64url without padding, give a
    /// 43-character verifier (RFC 7636 4.1's recommendation).
    pub fn generate() -> Self {
        let mut bytes = [0_u8; 32];
        aws_lc_rs::rand::fill(&mut bytes)
            .expect("the system RNG fills 32 bytes unless the platform is broken");
        let verifier = BASE64URL_NOPAD.encode(&bytes);
        let challenge = challenge(&verifier);
        Self {
            verifier: SecretString::from(verifier),
            challenge,
        }
    }

    /// The pair for a verifier read back from storage.
    pub fn from_verifier(verifier: SecretString) -> Result<Self, InvalidVerifier> {
        let v = verifier.expose_secret();
        let allowed = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~');
        if !(43..=128).contains(&v.len()) || !v.bytes().all(allowed) {
            return Err(InvalidVerifier);
        }
        let challenge = challenge(v);
        Ok(Self {
            verifier,
            challenge,
        })
    }

    /// The verifier, sent with the code exchange.
    pub fn verifier(&self) -> &SecretString {
        &self.verifier
    }

    /// `BASE64URL(SHA256(verifier))`, sent in the consent URL.
    pub fn challenge(&self) -> &str {
        &self.challenge
    }
}

fn challenge(verifier: &str) -> String {
    BASE64URL_NOPAD.encode(digest::digest(&digest::SHA256, verifier.as_bytes()).as_ref())
}
