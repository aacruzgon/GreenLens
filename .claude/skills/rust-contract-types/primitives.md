# Primitives: version, timestamps, IDs, confidence, regions

Part of [rust-contract-types](SKILL.md). The small types every `domain` shape is built from, in one compiled block with tests that prove the JSON. The rules they implement are [contracts 1, wire format](../../../docs/architecture/contracts.md#wire-format) and [versioning](../../../docs/architecture/contracts.md#versioning). `V1` and `Timestamp` are canonical regions: other blocks carry exact copies, which `check_examples.py` compares ([rust, checking the examples](../rust/SKILL.md#checking-the-examples)).

## Contents

1. [The code](#1-the-code)
2. [Why each type looks like this](#2-why-each-type-looks-like-this)

## 1. The code

The IDs are the design's rule, [backend domain 4](../../../docs/backend/domain.md#4-ids-and-primitives): one generic `Id<T: Table>` with an alias per table. The block shows four of its 22 tables and one slug kind. `Timestamp` is domain 4's newtype too; the one difference [backend domain 1](../../../docs/backend/domain.md#1-purpose-and-boundaries) still notes is with the shared backend brief, which gave timestamps as `chrono::DateTime<Utc>`: the design (and this skill) write whole seconds through the newtype, because chrono writes a fraction for every clock reading and contracts 1's example has none.

```rust
use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ulid::Ulid;
use utoipa::ToSchema;
use utoipa::openapi::RefOr;
use utoipa::openapi::schema::{ArrayBuilder, ObjectBuilder, Schema, Type};

/// A `SurrealDB` table whose records have a ULID key; `NAME` is the table in `schema.surql`,
/// so the store builds `RecordId::new(T::NAME, id.to_string())` (database 2).
pub trait Table: 'static {
    const NAME: &'static str;
}

/// The ULID key of a record in table `T`, sent bare: no `table:` prefix (contracts 1).
///
/// `PhantomData<fn() -> T>`: an id never owns a `T`, so the marker says "produces a `T`",
/// which is always `Send`, `Sync` and `Copy`-friendly whatever `T` is. The trait impls are
/// written by hand, because `#[derive]` would add `T: Clone` (and so on) bounds that the
/// uninhabited markers don't meet.
pub struct Id<T: Table> {
    ulid: Ulid,
    table: PhantomData<fn() -> T>,
}

/// Crockford base32, 26 characters, as `Display` writes it.
pub const ULID_PATTERN: &str = "^[0-9A-HJKMNP-TV-Z]{26}$";

/// Why a string isn't an id of the expected table: domain 4's `IdParseError`, with the table
/// name as its `kind` (domain writes `"scan id"`; here the message adds the word).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid {kind} id: {problem}")]
pub struct IdParseError {
    pub kind: &'static str,
    pub problem: &'static str,
}

/// No `generate()`: new keys come from the `IdGen` port (`IdGenExt::id::<T>()`, domain 9), as
/// time comes from `Clock`, so tests get fixed ids.
impl<T: Table> Id<T> {
    pub const fn from_ulid(ulid: Ulid) -> Self {
        Self {
            ulid,
            table: PhantomData,
        }
    }
}

impl<T: Table> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Table> Copy for Id<T> {}

impl<T: Table> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.ulid == other.ulid
    }
}

impl<T: Table> Eq for Id<T> {}

impl<T: Table> PartialOrd for Id<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Table> Ord for Id<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.ulid.cmp(&other.ulid)
    }
}

impl<T: Table> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ulid.hash(state);
    }
}

/// `scan:01JAX…`, so a log of a mixed list says which table each id is from.
impl<T: Table> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", T::NAME, self.ulid)
    }
}

/// The bare ULID, as on the wire.
impl<T: Table> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.ulid, f)
    }
}

impl<T: Table> FromStr for Id<T> {
    type Err = IdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let problem = |e| match e {
            ulid::DecodeError::InvalidLength => "not 26 characters",
            ulid::DecodeError::InvalidChar => "not Crockford base32",
        };
        Ulid::from_string(s)
            .map(Self::from_ulid)
            .map_err(|e| IdParseError {
                kind: T::NAME,
                problem: problem(e),
            })
    }
}

impl<T: Table> Serialize for Id<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.ulid)
    }
}

/// Parses inside `visit_str`, so axum's path deserializer can name the capture in its
/// error; `Ulid`'s own `Deserialize` parses after its visitor returns, and the name is lost
/// (the `path_errors_name_the_capture` test).
impl<'de, T: Table> Deserialize<'de> for Id<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct IdVisitor<T>(PhantomData<fn() -> T>);

        impl<T: Table> serde::de::Visitor<'_> for IdVisitor<T> {
            type Value = Id<T>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "the 26-character ULID of a `{}` record", T::NAME)
            }

            fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Id<T>, E> {
                s.parse().map_err(E::custom)
            }
        }

        deserializer.deserialize_str(IdVisitor(PhantomData))
    }
}

/// A string with the ULID pattern. Every instantiation shares the component name `Id`.
impl<T: Table> utoipa::PartialSchema for Id<T> {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .pattern(Some(ULID_PATTERN))
            .into()
    }
}

impl<T: Table> ToSchema for Id<T> {
    fn name() -> Cow<'static, str> {
        "Id".into()
    }
}

impl<T: Table> JsonSchema for Id<T> {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "Id".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "pattern": ULID_PATTERN })
    }
}

/// One uninhabited marker per table with a ULID key (a few of domain 4's 22).
pub mod table {
    macro_rules! tables {
        ($($marker:ident => $name:literal),* $(,)?) => {$(
            #[derive(Debug)]
            pub enum $marker {}

            impl super::Table for $marker {
                const NAME: &'static str = $name;
            }
        )*};
    }

    tables! {
        Plant => "plant",
        Scan => "scan",
        ScanImage => "scan_image",
        Job => "job",
    }
}

pub type PlantId = Id<table::Plant>;
pub type ScanId = Id<table::Scan>;
/// `scan_image` record key: the `image_id` of contracts 3 and 5.
pub type ImageId = Id<table::ScanImage>;
pub type JobId = Id<table::Job>;

/// A knowledge-base table whose records are keyed by a slug (contracts 10).
pub trait SlugKind: 'static {
    const TABLE: &'static str;
}

/// A knowledge-base id such as `"tomato_early_blight"`: `[a-z][a-z0-9_]*`, at most 64
/// characters. `Arc<str>`, because candidate lists, scores and prompts clone slugs often.
pub struct Slug<K: SlugKind> {
    value: Arc<str>,
    kind: PhantomData<fn() -> K>,
}

pub const SLUG_PATTERN: &str = "^[a-z][a-z0-9_]{0,63}$";

impl<K: SlugKind> Slug<K> {
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl<K: SlugKind> FromStr for Slug<K> {
    type Err = IdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut chars = s.chars();
        let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase());
        let rest_ok = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if first_ok && rest_ok && s.len() <= 64 {
            Ok(Self {
                value: s.into(),
                kind: PhantomData,
            })
        } else {
            Err(IdParseError {
                kind: K::TABLE,
                problem: "not a slug: [a-z][a-z0-9_]*, at most 64 characters",
            })
        }
    }
}

impl<K: SlugKind> Clone for Slug<K> {
    fn clone(&self) -> Self {
        Self {
            value: Arc::clone(&self.value),
            kind: PhantomData,
        }
    }
}

impl<K: SlugKind> PartialEq for Slug<K> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<K: SlugKind> Eq for Slug<K> {}

impl<K: SlugKind> Hash for Slug<K> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<K: SlugKind> fmt::Debug for Slug<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", K::TABLE, self.value)
    }
}

impl<K: SlugKind> fmt::Display for Slug<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.value)
    }
}

impl<K: SlugKind> Serialize for Slug<K> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.value)
    }
}

impl<'de, K: SlugKind> Deserialize<'de> for Slug<K> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct SlugVisitor<K>(PhantomData<fn() -> K>);

        impl<K: SlugKind> serde::de::Visitor<'_> for SlugVisitor<K> {
            type Value = Slug<K>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a `{}` slug", K::TABLE)
            }

            fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Slug<K>, E> {
                s.parse().map_err(E::custom)
            }
        }

        deserializer.deserialize_str(SlugVisitor(PhantomData))
    }
}

impl<K: SlugKind> utoipa::PartialSchema for Slug<K> {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .pattern(Some(SLUG_PATTERN))
            .into()
    }
}

impl<K: SlugKind> ToSchema for Slug<K> {
    fn name() -> Cow<'static, str> {
        "Slug".into()
    }
}

impl<K: SlugKind> JsonSchema for Slug<K> {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "Slug".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "pattern": SLUG_PATTERN })
    }
}

/// The species table's slugs (one of domain 4's knowledge kinds).
#[derive(Debug)]
pub enum Species {}

impl SlugKind for Species {
    const TABLE: &'static str = "species";
}

pub type SpeciesId = Slug<Species>;

/// A calendar date in the garden's time zone, `"2026-10-20"` (contracts 1). A newtype so the
/// generators see `format: date` through it, and a UTC day can't stand in for a local one.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    ToSchema,
    JsonSchema,
)]
#[serde(transparent)]
#[schema(value_type = String, format = Date, example = "2026-10-20")]
#[schemars(transparent)]
pub struct LocalDate(NaiveDate);

/// A number from 0 to 1. Rejects anything else, `NaN` included, when deserializing.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "f64")]
pub struct Confidence(f64);

impl TryFrom<f64> for Confidence {
    type Error = &'static str;

    fn try_from(v: f64) -> Result<Self, Self::Error> {
        if (0.0..=1.0).contains(&v) {
            Ok(Self(v))
        } else {
            Err("confidence must be between 0 and 1")
        }
    }
}

/// Written by hand: a derive would describe the struct, not the 0..1 range.
impl utoipa::PartialSchema for Confidence {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::Number)
            .minimum(Some(0))
            .maximum(Some(1))
            .into()
    }
}

impl ToSchema for Confidence {}

impl JsonSchema for Confidence {
    fn schema_name() -> Cow<'static, str> {
        "Confidence".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "number", "minimum": 0, "maximum": 1 })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceBand {
    High,
    Medium,
    Low,
}

impl ConfidenceBand {
    /// Contracts 1: high >= 0.8, medium >= 0.5, low below that.
    pub fn of(confidence: Confidence) -> Self {
        if confidence.0 >= 0.8 {
            Self::High
        } else if confidence.0 >= 0.5 {
            Self::Medium
        } else {
            Self::Low
        }
    }
}

/// `[x, y, w, h]`, each from 0 to 1, origin top-left, inside the image: `x + w <= 1` and
/// `y + h <= 1` (contracts 1, regions; domain 4).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "[f64; 4]")]
pub struct BBox([f64; 4]);

impl TryFrom<[f64; 4]> for BBox {
    type Error = &'static str;

    fn try_from(values: [f64; 4]) -> Result<Self, Self::Error> {
        let [x, y, w, h] = values;
        if !values.iter().all(|value| (0.0..=1.0).contains(value)) {
            Err("bbox values must be between 0 and 1")
        } else if x + w > 1.0 || y + h > 1.0 {
            Err("bbox must lie inside the image")
        } else {
            Ok(Self(values))
        }
    }
}

impl utoipa::PartialSchema for BBox {
    fn schema() -> RefOr<Schema> {
        let item = ObjectBuilder::new()
            .schema_type(Type::Number)
            .minimum(Some(0))
            .maximum(Some(1));
        ArrayBuilder::new()
            .items(item)
            .min_items(Some(4))
            .max_items(Some(4))
            .into()
    }
}

impl ToSchema for BBox {}

/// A shape that uses them all. `confidence_band` is set from `confidence` in the constructor,
/// so the two can't disagree (domain's `Rated` does the same when flattened into a record).
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema)]
pub struct Finding {
    pub scan_id: ScanId,
    pub image_id: ImageId,
    pub species_id: SpeciesId,
    pub bbox: BBox,
    pub confidence: Confidence,
    pub confidence_band: ConfidenceBand,
    pub due: LocalDate,
    pub created_at: Timestamp,
}

impl Finding {
    pub fn new(
        scan_id: ScanId,
        image_id: ImageId,
        species_id: SpeciesId,
        bbox: BBox,
        confidence: Confidence,
        due: LocalDate,
        created_at: Timestamp,
    ) -> Self {
        Self {
            scan_id,
            image_id,
            species_id,
            bbox,
            confidence,
            confidence_band: ConfidenceBand::of(confidence),
            due,
            created_at,
        }
    }
}

// canonical: rust-contract-types/primitives.md V1
/// The `"v": 1` on every SSE event, job payload and result (contracts 1, versioning).
/// Serializes as `1`; any other number fails to deserialize.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct V1;

impl serde::Serialize for V1 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(1)
    }
}

impl<'de> serde::Deserialize<'de> for V1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = <u64 as serde::Deserialize>::deserialize(deserializer)?;
        if v == 1 {
            Ok(Self)
        } else {
            let unexpected = serde::de::Unexpected::Unsigned(v);
            Err(serde::de::Error::invalid_value(unexpected, &"1"))
        }
    }
}

impl utoipa::PartialSchema for V1 {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::Integer)
            .enum_values(Some([1]))
            .into()
    }
}

impl utoipa::ToSchema for V1 {}

impl schemars::JsonSchema for V1 {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "V1".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "integer", "const": 1 })
    }
}
// end: V1

// canonical: rust-contract-types/primitives.md Timestamp
/// RFC 3339 in UTC, whole seconds, `Z` suffix: `"2026-10-18T14:03:22Z"` (contracts 1,
/// wire format; contracts 12, question 12). Keeps full precision in memory and in the
/// database; only the wire form is truncated. No `now()`: every reading comes from the
/// `Clock` port, so tests control time (backend domain 4).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, utoipa::ToSchema, schemars::JsonSchema,
)]
#[schema(value_type = String, format = DateTime, example = "2026-10-18T14:03:22Z")]
#[schemars(transparent)]
pub struct Timestamp(chrono::DateTime<chrono::Utc>);

impl Timestamp {
    /// For the store, which binds `chrono::DateTime<Utc>` (database 2).
    pub const fn into_inner(self) -> chrono::DateTime<chrono::Utc> {
        self.0
    }
}

impl From<chrono::DateTime<chrono::Utc>> for Timestamp {
    fn from(t: chrono::DateTime<chrono::Utc>) -> Self {
        Self(t)
    }
}

impl serde::Serialize for Timestamp {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let text = self.0.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        serializer.serialize_str(&text)
    }
}

impl<'de> serde::Deserialize<'de> for Timestamp {
    /// Accepts what chrono's RFC 3339 parser accepts and converts it to UTC.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <chrono::DateTime<chrono::Utc> as serde::Deserialize>::deserialize(deserializer).map(Self)
    }
}
// end: Timestamp

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use serde_json::json;
    use tower::ServiceExt as _;

    fn at(ms: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 18, 14, 3, 22).unwrap() + chrono::Duration::milliseconds(ms)
    }

    #[test]
    fn chrono_emits_subseconds_and_timestamp_does_not() {
        // chrono's own Serialize uses SecondsFormat::AutoSi with `Z`.
        assert_eq!(
            serde_json::to_value(at(0)).unwrap(),
            json!("2026-10-18T14:03:22Z")
        );
        assert_eq!(
            serde_json::to_value(at(120)).unwrap(),
            json!("2026-10-18T14:03:22.120Z")
        );
        // Timestamp truncates to whole seconds, as contracts 1 says.
        assert_eq!(
            serde_json::to_value(Timestamp::from(at(999))).unwrap(),
            json!("2026-10-18T14:03:22Z")
        );
        // In memory the fraction is still there, so two stamps in one second differ.
        assert!(Timestamp::from(at(100)) < Timestamp::from(at(900)));
    }

    #[test]
    fn timestamp_input_is_relaxed_rfc_3339() {
        let read = |s: &str| serde_json::from_value::<Timestamp>(json!(s));
        // Accepted, and equal to 14:03:22 UTC: strict RFC 3339 and chrono's relaxations.
        for text in [
            "2026-10-18T14:03:22Z",
            "2026-10-18T16:03:22+02:00",
            "2026-10-18 14:03:22Z",
            "2026-10-18t14:03:22z",
            "2026-10-18T16:03:22+0200",
            "2026-10-18T14:03:22UTC",
        ] {
            assert_eq!(read(text).unwrap(), Timestamp::from(at(0)), "{text}");
        }
        // Fractions are kept in memory.
        assert_eq!(read("2026-10-18T14:03:22.5Z").unwrap(), at(500).into());
        // Rejected: no offset, or an hour-only offset.
        assert!(read("2026-10-18T14:03:22").is_err());
        assert!(read("2026-10-18T14:03:22+02").is_err());
    }

    #[test]
    fn version_other_than_one_fails() {
        assert_eq!(serde_json::to_value(V1).unwrap(), json!(1));
        let err = serde_json::from_value::<V1>(json!(2)).unwrap_err();
        assert_eq!(err.to_string(), "invalid value: integer `2`, expected 1");
    }

    #[test]
    fn ids_are_bare_ulids() {
        let id: ScanId = "01jax3k9t6r2b8v0qz4m7d1e5f".parse().unwrap();
        assert_eq!(
            serde_json::to_value(id).unwrap(),
            json!("01JAX3K9T6R2B8V0QZ4M7D1E5F")
        );
        assert_eq!(
            serde_json::from_value::<ScanId>(json!("01JAX3K9T6R2B8V0QZ4M7D1E5F")).unwrap(),
            id
        );
        let prefixed = serde_json::from_value::<ScanId>(json!("scan:01JAX3K9T6R2B8V0QZ4M7D1E5F"));
        assert_eq!(
            prefixed.unwrap_err().to_string(),
            "invalid scan id: not 26 characters"
        );
        assert_eq!(format!("{id:?}"), "scan:01JAX3K9T6R2B8V0QZ4M7D1E5F");
        let fresh = ScanId::from_ulid(Ulid::generate()); // what an `IdGen` does
        assert_eq!(fresh.to_string().len(), 26);
    }

    /// A marker that is neither `Send`, `Sync` nor `Copy`.
    struct NotSendTable(std::rc::Rc<()>);

    impl Table for NotSendTable {
        const NAME: &'static str = "probe";
    }

    fn send_sync_copy<X: Send + Sync + Copy + 'static>() {}

    #[test]
    fn an_id_is_send_sync_and_copy_whatever_its_marker() {
        // Compiles only because the marker is `PhantomData<fn() -> T>` and the impls are by hand.
        send_sync_copy::<Id<NotSendTable>>();
        send_sync_copy::<ScanId>();
        let _unused = NotSendTable(std::rc::Rc::new(()));
    }

    #[test]
    fn slugs_are_checked_and_cheap_to_clone() {
        let s: SpeciesId = "tomato".parse().unwrap();
        let c = s.clone();
        assert!(Arc::ptr_eq(&s.value, &c.value), "a clone shares the string");
        assert_eq!(serde_json::to_value(&s).unwrap(), json!("tomato"));
        for bad in ["Tomato", "1tomato", "tomato-early", "", &*"a".repeat(65)] {
            assert!(bad.parse::<SpeciesId>().is_err(), "{bad}");
        }
        assert!("a".repeat(64).parse::<SpeciesId>().is_ok());
        assert!(serde_json::from_value::<SpeciesId>(json!("Tomato")).is_err());
    }

    async fn path_error(router: axum::Router) -> String {
        let request = axum::http::Request::get("/scans/nope")
            .body(axum::body::Body::empty())
            .unwrap();
        let response = router.oneshot(request).await.unwrap();
        // axum's own rejection; `ApiPath` answers 404 for it (rust-axum-api).
        assert_eq!(response.status(), 400);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        String::from_utf8(body.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn path_errors_name_the_capture() {
        use axum::extract::Path;
        use axum::routing::get;

        let ours = axum::Router::new().route(
            "/scans/{scan_id}",
            get(|Path(id): Path<ScanId>| async move { id.to_string() }),
        );
        assert_eq!(
            path_error(ours).await,
            "Invalid URL: Cannot parse `scan_id` with value `nope`: invalid scan id: not 26 characters"
        );
        // `ulid::Ulid`'s own Deserialize: the same failure without the capture's name.
        let plain = axum::Router::new().route(
            "/scans/{scan_id}",
            get(|Path(id): Path<Ulid>| async move { id.to_string() }),
        );
        assert_eq!(path_error(plain).await, "Invalid URL: invalid length");
    }

    #[test]
    fn a_body_error_names_the_field_path() {
        #[derive(Debug, Deserialize)]
        struct Answer {
            image_id: ImageId,
        }
        let de = &mut serde_json::Deserializer::from_str(r#"{"image_id":"nope"}"#);
        let err = serde_path_to_error::deserialize::<_, Answer>(de).unwrap_err();
        assert_eq!(err.path().to_string(), "image_id");
    }

    fn finding() -> Finding {
        Finding::new(
            "01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().unwrap(),
            "01JAX3KA0000000000000000AB".parse().unwrap(),
            "tomato".parse().unwrap(),
            BBox::try_from([0.1, 0.2, 0.3, 0.4]).unwrap(),
            Confidence::try_from(0.71).unwrap(),
            LocalDate(NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()),
            Timestamp::from(at(450)),
        )
    }

    #[test]
    fn finding_json() {
        assert_eq!(
            serde_json::to_value(finding()).unwrap(),
            json!({
                "scan_id": "01JAX3K9T6R2B8V0QZ4M7D1E5F",
                "image_id": "01JAX3KA0000000000000000AB",
                "species_id": "tomato",
                "bbox": [0.1, 0.2, 0.3, 0.4],
                "confidence": 0.71,
                "confidence_band": "medium",
                "due": "2026-10-20",
                "created_at": "2026-10-18T14:03:22Z"
            })
        );
        assert!(serde_json::from_value::<Confidence>(json!(1.5)).is_err());
        assert!(serde_json::from_value::<BBox>(json!([0.1, 0.2, 1.2, 0.4])).is_err());
        // Each value is in range, but the box runs off the right edge.
        assert!(serde_json::from_value::<BBox>(json!([0.5, 0.2, 0.6, 0.4])).is_err());
    }

    #[test]
    fn band_thresholds() {
        let band = |v: f64| ConfidenceBand::of(Confidence::try_from(v).unwrap());
        assert_eq!(band(0.8), ConfidenceBand::High);
        assert_eq!(band(0.79), ConfidenceBand::Medium);
        assert_eq!(band(0.5), ConfidenceBand::Medium);
        assert_eq!(band(0.49), ConfidenceBand::Low);
        assert!(Confidence::try_from(f64::NAN).is_err());
    }

    #[test]
    fn schemas() {
        let ut = |s: RefOr<Schema>| serde_json::to_value(s).unwrap();
        let ulid = json!({ "type": "string", "pattern": ULID_PATTERN });
        assert_eq!(ut(<ScanId as utoipa::PartialSchema>::schema()), ulid);
        assert_eq!(<ScanId as ToSchema>::name(), "Id");
        assert_eq!(
            <JobId as ToSchema>::name(),
            "Id",
            "one component for every table"
        );
        assert_eq!(
            ut(<SpeciesId as utoipa::PartialSchema>::schema()),
            json!({ "type": "string", "pattern": SLUG_PATTERN })
        );
        assert_eq!(
            ut(<V1 as utoipa::PartialSchema>::schema()),
            json!({ "type": "integer", "enum": [1] })
        );
        assert_eq!(
            ut(<Confidence as utoipa::PartialSchema>::schema()),
            json!({ "type": "number", "minimum": 0, "maximum": 1 })
        );
        assert_eq!(
            ut(<BBox as utoipa::PartialSchema>::schema()),
            json!({
                "type": "array",
                "items": { "type": "number", "minimum": 0, "maximum": 1 },
                "minItems": 4,
                "maxItems": 4
            })
        );
        let finding = ut(<Finding as utoipa::PartialSchema>::schema());
        assert_eq!(
            finding["properties"]["scan_id"],
            json!({ "$ref": "#/components/schemas/Id" })
        );
        assert_eq!(
            ut(<LocalDate as utoipa::PartialSchema>::schema())["format"],
            json!("date")
        );
        let sc = serde_json::to_value(schemars::schema_for!(Finding)).unwrap();
        assert_eq!(sc["properties"]["scan_id"], ulid, "inlined");
        let v1 = serde_json::to_value(schemars::schema_for!(V1)).unwrap();
        assert_eq!(v1["const"], json!(1));
        assert_eq!(
            sc["$defs"]["Confidence"],
            json!({ "type": "number", "minimum": 0, "maximum": 1 })
        );
        assert_eq!(sc["$defs"]["BBox"]["minItems"], json!(4));
        assert_eq!(sc["$defs"]["LocalDate"]["format"], json!("date"));
    }
}
```

## 2. Why each type looks like this

| Type | Decision | Source |
|---|---|---|
| `V1` | A unit type, so a payload can't be built with the wrong version and a `v: 2` fails to deserialize with ``invalid value: integer `2`, expected 1``. The worker reads the version *before* the full job; see [unions.md, version check](unions.md#3-the-worker-checks-v-first) | [contracts 1, versioning](../../../docs/architecture/contracts.md#versioning); [`Deserializer` errors](https://docs.rs/serde/1.0.229/serde/de/trait.Error.html#method.invalid_value) |
| `Timestamp` | chrono 0.4.45's `Serialize` for `DateTime` writes `SecondsFormat::AutoSi` with `Z` (`src/datetime/serde.rs`): a whole second prints as `…:22Z`, anything else as `…:22.120Z` or with 6 or 9 digits. `Utc::now()` is almost never a whole second (it printed 6 digits on macOS). The newtype truncates on the wire with [`to_rfc3339_opts(SecondsFormat::Secs, true)`](https://docs.rs/chrono/0.4.45/chrono/struct.DateTime.html#method.to_rfc3339_opts) and keeps full precision in memory, so ordering and the store's values are unchanged. Whole seconds on the wire and full precision in the database are settled ([contracts 12, question 12](../../../docs/architecture/contracts.md#12-open-questions); [contracts 1, wire format](../../../docs/architecture/contracts.md#wire-format); [backend api 22](../../../docs/backend/api.md#22-decisions-on-the-contracts-open-questions)); the store compares step stamps exactly in the database, never through the truncated wire form ([backend store 11](../../../docs/backend/store.md#11-steps-between-jobs), [21](../../../docs/backend/store.md#21-decisions-on-open-questions)). There is no `Timestamp::now()`: every reading comes from the `Clock` port ([backend domain 4](../../../docs/backend/domain.md#wire-primitives)) | [contracts 1, wire format](../../../docs/architecture/contracts.md#wire-format); [database 2](../../../docs/database/README.md#2-conventions) (bind `DateTime<Utc>`, so the store calls `into_inner`) |
| `Timestamp` input | chrono's parser is relaxed RFC 3339: besides `T` and `Z` it accepts a space for `T`, lowercase `t` and `z`, `+0200` without the colon, a space before the offset and `UTC`, and converts any offset to UTC. A value without an offset, or with an hour-only offset (`+02`), fails. So input is more lenient than the contracts' format, and output is always the strict form. The `timestamp_input_is_relaxed_rfc_3339` test lists each case | [`FromStr for DateTime<Utc>`](https://docs.rs/chrono/0.4.45/chrono/struct.DateTime.html#impl-FromStr-for-DateTime%3CUtc%3E) ("Accepts a relaxed form of RFC3339"), which chrono's `Deserialize` calls (`src/datetime/serde.rs`, `visit_str`) |
| `Id<T>`, `ScanId`, `PlantId`, … | **One generic `Id<T: Table>` over [`ulid::Ulid`](https://docs.rs/ulid/3.0.0/ulid/struct.Ulid.html) with a `PhantomData<fn() -> T>` marker, and an alias per table** (`ScanId = Id<table::Scan>`): one implementation of parsing, serde and schemas, and `T::NAME` gives the store the table name ([backend domain 4](../../../docs/backend/domain.md#idt); [domain-types 1, identifiers](../../../docs/backend/domain-types.md#identifiers)). The markers are uninhabited enums (`pub enum Scan {}`) implementing `Table`. A `ScanId` still can't be passed where a `PlantId` is wanted. `fn() -> T` is covariant and always `Send + Sync`, so the id's auto traits never depend on the marker ([Nomicon, `PhantomData` patterns](https://doc.rust-lang.org/nomicon/phantom-data.html#table-of-phantomdata-patterns); [backend domain 10, markers](../../../docs/backend/domain.md#markers-and-phantomdata)); the `an_id_is_send_sync_and_copy_whatever_its_marker` test uses a `!Send` marker. `Clone`, `Copy`, `PartialEq`, `Eq`, `Ord`, `Hash` are written by hand, because `#[derive]` adds a `T: Clone` (and so on) bound the markers don't meet (the `compile_fail` block below). On the wire it is the bare string; `Debug` is `scan:01JAX…`. Parsing is case-insensitive and rejects a `scan:` prefix, so a leaked record ID fails at the boundary. New keys come from the `IdGen` port (`IdGen::ulid`, `IdGenExt::id::<T>()`), never from a constructor on `Id<T>`, so tests can fix them ([backend domain 9](../../../docs/backend/domain.md#time-ids-url-signing-and-species-photos)); the production generator calls [`Ulid::generate`](https://docs.rs/ulid/3.0.0/ulid/struct.Ulid.html#method.generate), the ulid 3.0.0 name (`Ulid::new` was ulid 1.x) | [contracts 1](../../../docs/architecture/contracts.md#wire-format); [database 2](../../../docs/database/README.md#2-conventions) |
| ID `Deserialize` | Written by hand, parsing inside [`Visitor::visit_str`](https://docs.rs/serde/1.0.229/serde/de/trait.Visitor.html#method.visit_str). axum's path deserializer attaches the capture's name only to an error raised while it drives the visitor (`ErrorKind::DeserializeError { key, value, message }`, `axum-0.8.9/src/extract/path/de.rs`). `Ulid`'s own `Deserialize` takes the string and parses it afterwards, so its error arrives as a bare `ErrorKind::Message` without the capture's name. The `path_errors_name_the_capture` test shows both. A malformed path value is answered `404` whatever the error says (below), so the name only helps the logs; in a JSON body the field's path comes from `serde_path_to_error` ([backend api 8](../../../docs/backend/api.md#8-errors); the `a_body_error_names_the_field_path` test) | [axum `ErrorKind`](https://docs.rs/axum/0.8.9/axum/extract/path/enum.ErrorKind.html); the test |
| IDs in the store | The `surrealdb` 3.3.4 SDK depends on ulid 1.2.1, a different crate version, so no `Ulid` crosses into it. The store converts through the string: `RecordId::new("scan", id.to_string())` and back with `FromStr` ([rust-surrealdb](../rust-surrealdb/SKILL.md)) | [database 6](../../../docs/database/README.md#6-rust-sdk); [versions.md](../rust/versions.md#compatibility-checks) |
| IDs in schemas | `Id<T>` and `Slug<K>` implement `PartialSchema`/`ToSchema` and `JsonSchema` by hand as a string with a pattern (`ULID_PATTERN`, `SLUG_PATTERN`). Every instantiation shares the OpenAPI component `Id` (or `Slug`), which the backend flags for a check of the generated TypeScript ([backend api 23](../../../docs/backend/api.md#23-open-questions), item 5); schemars inlines them | [utoipa `ToSchema::name`](https://docs.rs/utoipa/6.0.0/utoipa/trait.ToSchema.html#method.name); [schemars `inline_schema`](https://docs.rs/schemars/1.2.3/schemars/trait.JsonSchema.html#method.inline_schema); the `schemas` test |
| A malformed ID | Settled: in a **path**, `404 not_found`, exactly like an unknown ID, because it can name no record and `404` reveals nothing (`ApiPath`, [rust-axum-api](../rust-axum-api/error-responses.md#3-extractor-rejections)); in a **body or query**, `400 invalid_request` naming the field, like any wrong-typed value ([contracts 12, question 14](../../../docs/architecture/contracts.md#12-open-questions); [backend api 22](../../../docs/backend/api.md#22-decisions-on-the-contracts-open-questions)) | [contracts 2](../../../docs/architecture/contracts.md#2-errors) |
| `Slug<K>` | Generic over a `SlugKind` marker like `Id<T>`, holding an `Arc<str>` so cloning is a reference count. Format `[a-z][a-z0-9_]*`, at most 64 characters, checked when parsed. `"healthy"` is reserved: it can't be a problem slug, because it takes a problem ID's place in `Slug \| "healthy"` (`ProblemOrHealthy`) | [contracts 10](../../../docs/architecture/contracts.md#10-knowledge-seed-format); [backend domain 4](../../../docs/backend/domain.md#slugk-compound-and-opaque-ids); [domain-types 1](../../../docs/backend/domain-types.md#identifiers) |
| `LocalDate` | A newtype over `chrono::NaiveDate`, `"2026-10-20"`, with `format: date` in both schemas, so a UTC day can't stand in for a garden-local one | [backend domain-types 1](../../../docs/backend/domain-types.md#timestamps-dates-and-confidence); proved by the test |
| `Confidence` | `try_from = "f64"` rejects values outside 0..1 from workers. `confidence_band` is computed by the API, so the band is set in the constructor (`ConfidenceBand::of`), never deserialized from a worker; domain's records carry the pair as a flattened `Rated`, which writes the band from the number and recomputes it when read ([backend domain-types 1](../../../docs/backend/domain-types.md#timestamps-dates-and-confidence)). `NaN` fails the range check. Both schemas are written by hand so they carry `minimum: 0` and `maximum: 1`: utoipa ignores `try_from` ([schema-attrs.md 1](schema-attrs.md#1-which-serde-attributes-each-generator-honours)), and the generated app types and seed checks should know the range | [contracts 1](../../../docs/architecture/contracts.md#wire-format) (`Confidence = number; // 0..1`); [serde `try_from`](https://serde.rs/container-attrs.html#try_from); the `schemas` test |
| `BBox` | `[f64; 4]` with the range check and `x + w <= 1`, `y + h <= 1` ([backend domain 4](../../../docs/backend/domain.md#wire-primitives)). utoipa renders a bare `[f64; 4]` as an array with no length, so the schema is written by hand; schemars adds `minItems`/`maxItems` 4 on its own | the `schemas` test; [`ArrayBuilder`](https://docs.rs/utoipa/6.0.0/utoipa/openapi/schema/struct.ArrayBuilder.html) |
| Units | The unit is in the field name and the type is a plain number: `duration_ms: u64`, `max_runtime_s: u32`, `temp_c: f64`. Never a `std::time::Duration` on the wire. `Duration::as_millis` returns `u128`; convert with `u64::try_from`, not `as` (clippy `cast_possible_truncation`) | [contracts 1](../../../docs/architecture/contracts.md#wire-format); [`as_millis`](https://doc.rust-lang.org/std/time/struct.Duration.html#method.as_millis); [clippy](https://clippy.rust-lang.org/rust-1.99.0/index.html#cast_possible_truncation) |

Why `Id<T>`'s impls are written by hand: a derive bounds its impl on the parameter, so `#[derive(Clone, Copy)]` gives `impl<T: Copy> Copy for Id<T>`, and an uninhabited marker isn't `Copy` ([std, `Copy`: how can I implement `Copy`?](https://doc.rust-lang.org/std/marker/trait.Copy.html#how-can-i-implement-copy), "the derive strategy will also place a `Copy` bound on type parameters"):

```rust,compile_fail,E0277
use std::marker::PhantomData;

#[derive(Clone, Copy)]
pub struct Id<T> {
    key: u128,
    table: PhantomData<fn() -> T>,
}

pub enum Scan {}

fn copies<X: Copy>(_: X) {}

pub fn check(id: Id<Scan>) {
    copies(id);
}
```
