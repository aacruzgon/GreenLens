//! Synchronous ports every crate shares: time, ids, URL signing and the species photo index.

use ulid::Ulid;

use crate::{
    garden::species::SpeciesPhoto,
    id::{Id, SpeciesId, Table},
    url::SigningInput,
    wire::Timestamp,
};

/// Wall-clock time. Every reading goes through this port so tests control time
/// (convention 10). Monotonic waits use tokio's clock, which tests pause.
#[cfg_attr(feature = "mock", mockall::automock)]
pub trait Clock: Send + Sync {
    /// Now, in UTC.
    fn now(&self) -> Timestamp;
}

/// Record keys ([database 2]: ULID string keys generated in Rust).
#[cfg_attr(feature = "mock", mockall::automock)]
pub trait IdGen: Send + Sync {
    /// A new ULID. `ulid` 3 spells it `Ulid::generate()`.
    fn ulid(&self) -> Ulid;
}

/// Typed ids from any [`IdGen`], including `dyn IdGen`. A generic method can't be called
/// through `dyn` (it isn't dispatchable, Rust Reference, dyn compatibility), so it lives in a
/// blanket extension trait over `?Sized` generators instead of in `IdGen` itself.
pub trait IdGenExt: IdGen {
    /// A new id of table `T`.
    fn id<T: Table>(&self) -> Id<T> {
        Id::from_ulid(self.ulid())
    }
}
impl<G: IdGen + ?Sized> IdGenExt for G {}

/// The HMAC behind signed image URLs ([storage 3]). Implemented in `api` with the server
/// secret; `domain` assembles and checks the URL around it ([`crate::url::Signer`],
/// [`crate::url::verify_signed`]).
#[cfg_attr(feature = "mock", mockall::automock)]
pub trait UrlSigner: Send + Sync {
    /// HMAC-SHA256 of `input.canonical()`, lowercase hex.
    fn signature(&self, input: &SigningInput) -> String;
    /// Compares `sig` with the expected signature in constant time.
    fn verify(&self, input: &SigningInput, sig: &str) -> bool;
    /// The API's public origin, such as `"https://api.example.org"`, prefixed to every URL.
    fn public_base(&self) -> &str;
}

/// The species reference photos, in seed order ([contracts 9], species). Loaded once at start
/// from the knowledge base and held in memory, because signing is synchronous and runs for
/// every candidate in every response and replayed event.
#[cfg_attr(feature = "mock", mockall::automock)]
pub trait SpeciesPhotos: Send + Sync {
    /// The species' photos, at most 3; empty when it has none.
    fn photos(&self, species: &SpeciesId) -> Vec<SpeciesPhoto>;
}
