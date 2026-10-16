//! Worker authentication ([contracts 1], authentication headers; [architecture 11]): one bearer
//! token per worker process, compared in constant time, and mapped to that worker's id. The id
//! in a claim or heartbeat body must be the token's, so a worker can only act as itself, and the
//! [`Lease`](domain::ports::Lease) of every later call names it.

use std::collections::BTreeMap;

use aws_lc_rs::{constant_time, digest};
use secrecy::{ExposeSecret, SecretString};

use domain::id::WorkerId;

/// The configured tokens, kept only as SHA-256 digests.
#[derive(Clone)]
pub struct WorkerTokens(Vec<(WorkerId, [u8; 32])>);

impl WorkerTokens {
    /// From `WORKER_TOKENS`. Each token is hashed once, here.
    pub fn new(tokens: BTreeMap<WorkerId, SecretString>) -> Self {
        Self(
            tokens
                .into_iter()
                .map(|(id, token)| (id, sha256(token.expose_secret().as_bytes())))
                .collect(),
        )
    }

    /// The worker whose token this is. The bearer is hashed and its digest compared with every
    /// configured digest in constant time. Comparing raw tokens would leak their length:
    /// `verify_slices_are_equal` returns at once when the lengths differ (aws-lc-rs 1.18.2), while
    /// two digests are always 32 bytes. Every token is checked, so the time doesn't tell which
    /// one matched either.
    pub fn identify(&self, bearer: &str) -> Option<WorkerId> {
        let presented = sha256(bearer.as_bytes());
        let mut found = None;
        for (id, expected) in &self.0 {
            if constant_time::verify_slices_are_equal(expected, &presented).is_ok() {
                found = Some(id.clone());
            }
        }
        found
    }
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut out = [0; 32];
    out.copy_from_slice(digest::digest(&digest::SHA256, bytes).as_ref());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_only_exact_tokens() {
        let id: WorkerId = "rig-3070-perception-1".parse().unwrap();
        let other: WorkerId = "rig-5070ti-vlm-1".parse().unwrap();
        let tokens = WorkerTokens::new(BTreeMap::from([
            (id.clone(), SecretString::from("t".repeat(40))),
            (other.clone(), SecretString::from("u".repeat(48))),
        ]));
        assert_eq!(tokens.identify(&"t".repeat(40)), Some(id));
        assert_eq!(tokens.identify(&"u".repeat(48)), Some(other));
        assert_eq!(tokens.identify(&"t".repeat(39)), None);
        assert_eq!(tokens.identify(&"v".repeat(40)), None);
        assert_eq!(tokens.identify(""), None);
    }
}
