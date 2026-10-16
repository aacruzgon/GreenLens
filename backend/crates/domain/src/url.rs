//! Signed image URLs in the type system ([contracts 1] image URLs, [contracts 4] "image URLs in
//! events are signed as the event is sent", [storage 3]).
//!
//! Every contract type that can carry an image or mask URL takes a URL-state parameter
//! `U: UrlState`, defaulting to [`Signed`]:
//!
//! | | `X<Stored>` | `X<Signed>` (= `X`) |
//! |---|---|---|
//! | URL slots (`url`, `thumbnail_url`, …) | [`NoUrl`]: never serialised, and deserialising a URL into it fails | [`SignedUrl`] |
//! | Region mask | `mask_id` ([`StoredMask`]) | `mask_url` ([`SignedMask`]) |
//! | `utoipa::ToSchema` | not implemented | implemented |
//! | Used by | `store` (records, event payloads), agent tools ("`ScanDetail` without image URLs") | `api` responses and SSE |
//!
//! The only way from `X<Stored>` to `X<Signed>` is [`Sign::sign`], given a [`Signer`] that
//! holds the [`UrlSigner`](crate::ports::UrlSigner) port and the expiry. There is no way back:
//! nothing converts a signed value into a stored one, and the store's repository methods take
//! `X<Stored>` only. `api` renders JSON responses through a wrapper that requires
//! `utoipa::ToSchema` (it needs it for the `OpenAPI` document anyway), and only `X<Signed>`
//! implements it, so a handler that tries to return an unsigned record does not compile:
//!
//! ```compile_fail,E0277
//! fn respond<T: utoipa::ToSchema + serde::Serialize>(_: T) {}
//! fn handler(scan: domain::scan::Scan<domain::url::Stored>) {
//!     respond(scan); // `Stored: ToSchema` is not satisfied
//! }
//! ```
//!
//! and a signed record can't be handed to the store, whose methods take `X<Stored>`:
//!
//! ```compile_fail,E0308
//! fn store(_: domain::events::ScanEventBody<domain::url::Stored>) {}
//! fn send(event: domain::events::ScanEventBody) {
//!     store(event); // expected `ScanEventBody<Stored>`, found `ScanEventBody<Signed>`
//! }
//! ```

use std::{borrow::Cow, fmt, time::Duration};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{
    id::{ChatImageId, ConversationId, MaskId, ScanId, SpeciesId},
    ports::{SpeciesPhotos, UrlSigner},
    wire::Timestamp,
};

mod sealed {
    pub trait Sealed {}
}

/// The state of every URL slot in a value: [`Stored`] or [`Signed`]. Sealed: there are exactly
/// two states.
pub trait UrlState:
    sealed::Sealed + Clone + Copy + fmt::Debug + PartialEq + Default + Send + Sync + 'static
{
    /// What an image URL slot holds.
    type Url: UrlSlot;
    /// What a region's mask slot holds, flattened into the region.
    type Mask: MaskSlot;
}

/// Implemented only by [`Signed`]: the bound on every `utoipa::ToSchema` impl of a type with
/// URL slots, so only signed values can appear in the API's schema and responses.
pub trait WireState: UrlState {}

/// IDs only: what the database stores and what the agent's tools return to the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stored;

/// Fresh, signed, short-lived URLs: what the app receives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Signed;

impl sealed::Sealed for Stored {}
impl sealed::Sealed for Signed {}
impl UrlState for Stored {
    type Url = NoUrl;
    type Mask = StoredMask;
}
impl UrlState for Signed {
    type Url = SignedUrl;
    type Mask = SignedMask;
}
impl WireState for Signed {}

/// `Signed` has a schema and `Stored` has none, so utoipa's default bound on a generic enum
/// (`U: ToSchema`, used where `#[schema(bound)]` isn't accepted) also admits only signed values.
/// The schema itself is never referenced: every generic field overrides its `value_type`.
impl utoipa::PartialSchema for Signed {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new().into()
    }
}
impl utoipa::ToSchema for Signed {}

/// A URL slot's contents.
pub trait UrlSlot:
    Clone + fmt::Debug + PartialEq + Serialize + de::DeserializeOwned + Default + Send + Sync + 'static
{
    /// True when the slot holds nothing to serialise (always, for [`NoUrl`]).
    fn is_absent(&self) -> bool;
}

/// A region's mask slot.
pub trait MaskSlot:
    Clone + fmt::Debug + PartialEq + Serialize + de::DeserializeOwned + Default + Send + Sync + 'static
{
}

/// The empty URL slot of a stored value. It is skipped when serialising, and deserialising
/// anything into it fails, so a stored record can't hold a URL even through JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NoUrl;

impl UrlSlot for NoUrl {
    fn is_absent(&self) -> bool {
        true
    }
}
impl Serialize for NoUrl {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_unit()
    }
}
impl<'de> Deserialize<'de> for NoUrl {
    fn deserialize<D: Deserializer<'de>>(_: D) -> Result<Self, D::Error> {
        Err(de::Error::custom(
            "a stored record carries image ids, never URLs",
        ))
    }
}

/// `skip_serializing_if` for optional URL slots.
pub fn url_absent<T: UrlSlot>(slot: &Option<T>) -> bool {
    slot.as_ref().is_none_or(UrlSlot::is_absent)
}

/// An API-signed, short-lived image URL: `{base}{path}?exp={unix seconds}&sig={hex}`
/// ([storage 3]). Built only by [`Signer::url`] in this crate; it deserialises (for clients and
/// tests), but nothing turns it back into a stored value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize)]
#[serde(transparent)]
pub struct SignedUrl(String);

impl SignedUrl {
    /// The URL text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl UrlSlot for SignedUrl {
    fn is_absent(&self) -> bool {
        false
    }
}
impl fmt::Display for SignedUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl<'de> Deserialize<'de> for SignedUrl {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        if s.contains("sig=") && s.contains("exp=") {
            Ok(Self(s))
        } else {
            Err(de::Error::custom("not a signed URL"))
        }
    }
}
impl utoipa::PartialSchema for SignedUrl {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        crate::schema_util::string(None, "signed, short-lived image URL (15 minutes)")
    }
}
impl utoipa::ToSchema for SignedUrl {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("SignedUrl")
    }
}

/// The mask slot of a stored region: the mask record's id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StoredMask {
    /// The `mask` record, when segmentation produced one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_id: Option<MaskId>,
}
impl MaskSlot for StoredMask {}

/// The mask slot of a signed region: `mask_url?` of contracts 1.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SignedMask {
    /// Signed PNG mask, when segmentation produced one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub mask_url: Option<SignedUrl>,
}
impl MaskSlot for SignedMask {}

// ---------------------------------------------------------------------------------------------
// Paths and signing
// ---------------------------------------------------------------------------------------------

/// The first 16 hex digits of a species photo's WebP SHA-256, in its storage key and URL
/// ([contracts 10], [storage 2]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PhotoHash([u8; 16]);

impl PhotoHash {
    /// Parses 16 lowercase hex digits.
    pub fn parse(s: &str) -> Option<Self> {
        let bytes: [u8; 16] = s.as_bytes().try_into().ok()?;
        bytes
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
            .then_some(Self(bytes))
    }

    /// The 16 hex digits.
    pub fn as_str(&self) -> &str {
        // reason: `parse` admits only ASCII hex digits, which are valid UTF-8.
        std::str::from_utf8(&self.0).unwrap_or_default()
    }
}
impl fmt::Display for PhotoHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every route that serves an image through a signed URL ([storage 3]). Depth maps have none.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ImagePath {
    /// `GET /scans/{scan}/images/{image}`
    ScanImage {
        /// The scan.
        scan: ScanId,
        /// The image.
        image: crate::id::ImageId,
    },
    /// `GET /scans/{scan}/masks/{mask}`
    Mask {
        /// The scan.
        scan: ScanId,
        /// The mask.
        mask: MaskId,
    },
    /// `GET /conversations/{conversation}/images/{image}`
    ChatImage {
        /// The conversation.
        conversation: ConversationId,
        /// The image.
        image: ChatImageId,
    },
    /// `GET /species/{species}/images/{hash}`
    SpeciesPhoto {
        /// The species.
        species: SpeciesId,
        /// The photo's content hash.
        hash: PhotoHash,
    },
}

impl fmt::Display for ImagePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ScanImage { scan, image } => write!(f, "/scans/{scan}/images/{image}"),
            Self::Mask { scan, mask } => write!(f, "/scans/{scan}/masks/{mask}"),
            Self::ChatImage {
                conversation,
                image,
            } => write!(f, "/conversations/{conversation}/images/{image}"),
            Self::SpeciesPhoto { species, hash } => write!(f, "/species/{species}/images/{hash}"),
        }
    }
}

/// What the HMAC covers: the method, the path and `exp` ([storage 3]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningInput {
    /// Always `"GET"`.
    pub method: &'static str,
    /// The route path, without query string.
    pub path: String,
    /// Expiry, Unix seconds.
    pub exp: i64,
}

impl SigningInput {
    /// The canonical bytes to sign: `"{method}\n{path}\n{exp}"`.
    pub fn canonical(&self) -> String {
        format!("{}\n{}\n{}", self.method, self.path, self.exp)
    }
}

/// How long a signed URL lives ([contracts 1]; open question 5 of contracts 12).
pub const SIGNED_URL_TTL: Duration = Duration::from_mins(15);

/// Signs URLs for one response or one event send. Borrows the signer and the species photo
/// index for `'a`, the time it takes to build the response, so nothing is cloned per URL.
#[derive(Clone, Copy)]
pub struct Signer<'a> {
    port: &'a dyn UrlSigner,
    photos: &'a dyn SpeciesPhotos,
    exp: i64,
}

impl fmt::Debug for Signer<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Signer")
            .field("exp", &self.exp)
            .finish_non_exhaustive()
    }
}

impl<'a> Signer<'a> {
    /// A signer whose URLs expire [`SIGNED_URL_TTL`] after `now`.
    pub fn new(signer: &'a dyn UrlSigner, photos: &'a dyn SpeciesPhotos, now: Timestamp) -> Self {
        let ttl = i64::try_from(SIGNED_URL_TTL.as_secs()).unwrap_or(i64::MAX);
        Self {
            port: signer,
            photos,
            exp: now.unix_seconds().saturating_add(ttl),
        }
    }

    /// The signed URL of an image route.
    pub fn url(&self, path: &ImagePath) -> SignedUrl {
        let input = SigningInput {
            method: "GET",
            path: path.to_string(),
            exp: self.exp,
        };
        let sig = self.port.signature(&input);
        SignedUrl(format!(
            "{}{}?exp={}&sig={sig}",
            self.port.public_base(),
            input.path,
            self.exp
        ))
    }

    /// The species photo index, for `SpeciesCandidate.reference_image`.
    pub fn photos(&self) -> &'a dyn SpeciesPhotos {
        self.photos
    }
}

/// Why a signed URL was refused: `403 invalid_signature` ([contracts 2]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SignatureError {
    /// `exp` is in the past.
    #[error("signed URL expired")]
    Expired,
    /// `exp` is further ahead than any URL the API issues.
    #[error("signed URL expiry too far ahead")]
    TooFarAhead,
    /// The signature doesn't match.
    #[error("signature mismatch")]
    Mismatch,
}

/// Checks an incoming signed request ([storage 3]): `exp` not past, not beyond the TTL, and the
/// signature matching (compared by the port in constant time).
pub fn verify_signed(
    signer: &dyn UrlSigner,
    path: &ImagePath,
    exp: i64,
    sig: &str,
    now: Timestamp,
) -> Result<(), SignatureError> {
    let now = now.unix_seconds();
    let ttl = i64::try_from(SIGNED_URL_TTL.as_secs()).unwrap_or(i64::MAX);
    if exp < now {
        return Err(SignatureError::Expired);
    }
    if exp > now.saturating_add(ttl) {
        return Err(SignatureError::TooFarAhead);
    }
    let input = SigningInput {
        method: "GET",
        path: path.to_string(),
        exp,
    };
    if signer.verify(&input, sig) {
        Ok(())
    } else {
        Err(SignatureError::Mismatch)
    }
}

/// Turns a stored value into its signed wire form. `Scope` is what the value doesn't know about
/// itself but its URLs need (the scan of a region's mask, the conversation of a message's
/// attachment); `()` when the value carries it.
pub trait Sign {
    /// The signed form.
    type Signed;
    /// The parent id the URLs need.
    type Scope: Copy;
    /// Signs every URL slot.
    fn sign(self, scope: Self::Scope, signer: &Signer<'_>) -> Self::Signed;
}

impl<T: Sign> Sign for Vec<T> {
    type Signed = Vec<T::Signed>;
    type Scope = T::Scope;
    fn sign(self, scope: Self::Scope, signer: &Signer<'_>) -> Self::Signed {
        self.into_iter().map(|v| v.sign(scope, signer)).collect()
    }
}
impl<T: Sign> Sign for Option<T> {
    type Signed = Option<T::Signed>;
    type Scope = T::Scope;
    fn sign(self, scope: Self::Scope, signer: &Signer<'_>) -> Self::Signed {
        self.map(|v| v.sign(scope, signer))
    }
}
impl<T: Sign> Sign for crate::wire::Page<T> {
    type Signed = crate::wire::Page<T::Signed>;
    type Scope = T::Scope;
    fn sign(self, scope: Self::Scope, signer: &Signer<'_>) -> Self::Signed {
        self.map(|v| v.sign(scope, signer))
    }
}
