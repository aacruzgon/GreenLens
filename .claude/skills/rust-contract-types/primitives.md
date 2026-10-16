# Primitives: version, timestamps, IDs, confidence, regions

Part of [rust-contract-types](SKILL.md). The small types every `domain` shape is built from, in one compiled block with tests that prove the JSON. The rules they implement are [contracts 1, wire format](../../../docs/architecture/contracts.md#wire-format) and [versioning](../../../docs/architecture/contracts.md#versioning). `V1` and `Timestamp` are canonical regions: other blocks carry exact copies, which `check_examples.py` compares ([rust, checking the examples](../rust/SKILL.md#checking-the-examples)).

## Contents

1. [The code](#1-the-code)
2. [Why each type looks like this](#2-why-each-type-looks-like-this)

## 1. The code

```rust
use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use ulid::Ulid;
use utoipa::ToSchema;
use utoipa::openapi::RefOr;
use utoipa::openapi::schema::{ArrayBuilder, ObjectBuilder, Schema, Type};

/// One newtype per record table. On the wire: the bare 26-character ULID, no `table:` prefix.
/// `Deserialize` parses inside `visit_str`, so axum's path extractor can name the capture
/// in its error; `Ulid`'s own `Deserialize` parses after its visitor returns, and the name
/// is lost (the `path_errors_name_the_capture` test).
macro_rules! ulid_id {
    ($($name:ident),+ $(,)?) => {$(
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
            Serialize, ToSchema, JsonSchema,
        )]
        #[serde(transparent)]
        #[schema(value_type = String, example = "01JAX3K9T6R2B8V0QZ4M7D1E5F")]
        pub struct $name(#[schemars(with = "String")] Ulid);

        impl $name {
            pub fn generate() -> Self {
                Self(Ulid::generate())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }

        impl FromStr for $name {
            type Err = ulid::DecodeError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ulid::from_string(s).map(Self)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct IdVisitor;

                impl serde::de::Visitor<'_> for IdVisitor {
                    type Value = $name;

                    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                        f.write_str("a 26-character ULID")
                    }

                    fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<$name, E> {
                        s.parse().map_err(E::custom)
                    }
                }

                deserializer.deserialize_str(IdVisitor)
            }
        }
    )+};
}

ulid_id!(ScanId, PlantId, ImageId, JobId);

/// A knowledge-base ID such as `"tomato_early_blight"` (contracts 10).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(transparent)]
pub struct Slug(String);

/// A number from 0 to 1. Rejects anything else when deserializing.
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

impl Confidence {
    /// Contracts 1: high >= 0.8, medium >= 0.5, low below that.
    pub fn band(self) -> ConfidenceBand {
        if self.0 >= 0.8 {
            ConfidenceBand::High
        } else if self.0 >= 0.5 {
            ConfidenceBand::Medium
        } else {
            ConfidenceBand::Low
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
    fn schema_name() -> std::borrow::Cow<'static, str> {
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

/// `[x, y, w, h]`, each from 0 to 1, origin top-left (contracts 1, regions).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "[f64; 4]")]
pub struct BBox([f64; 4]);

impl TryFrom<[f64; 4]> for BBox {
    type Error = &'static str;

    fn try_from(v: [f64; 4]) -> Result<Self, Self::Error> {
        if v.iter().all(|c| (0.0..=1.0).contains(c)) {
            Ok(Self(v))
        } else {
            Err("bbox values must be between 0 and 1")
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
/// so the two can't disagree.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, JsonSchema)]
pub struct Finding {
    pub scan_id: ScanId,
    pub image_id: ImageId,
    pub bbox: BBox,
    pub confidence: Confidence,
    pub confidence_band: ConfidenceBand,
    pub due: NaiveDate,
    pub created_at: Timestamp,
}

impl Finding {
    pub fn new(
        scan_id: ScanId,
        image_id: ImageId,
        bbox: BBox,
        confidence: Confidence,
        due: NaiveDate,
        created_at: Timestamp,
    ) -> Self {
        Self {
            scan_id,
            image_id,
            bbox,
            confidence,
            confidence_band: confidence.band(),
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
/// RFC 3339 in UTC, whole seconds, `Z` suffix: `"2026-10-18T14:03:22Z"` (contracts 1).
/// Keeps full precision in memory; only the wire form is truncated. Whole seconds are
/// this skill's choice, not the contracts' (contracts 12, question 12).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, utoipa::ToSchema, schemars::JsonSchema,
)]
#[schema(value_type = String, format = DateTime, example = "2026-10-18T14:03:22Z")]
#[schemars(transparent)]
pub struct Timestamp(chrono::DateTime<chrono::Utc>);

impl Timestamp {
    pub fn now() -> Self {
        Self(chrono::Utc::now())
    }

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
        // Timestamp truncates to whole seconds, as in the contracts example.
        assert_eq!(
            serde_json::to_value(Timestamp::from(at(999))).unwrap(),
            json!("2026-10-18T14:03:22Z")
        );
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
        assert!(
            serde_json::from_value::<ScanId>(json!("scan:01JAX3K9T6R2B8V0QZ4M7D1E5F")).is_err()
        );
        assert_eq!(ScanId::generate().to_string().len(), 26);
    }

    async fn path_error(router: axum::Router) -> String {
        let request = axum::http::Request::get("/scans/nope")
            .body(axum::body::Body::empty())
            .unwrap();
        let response = router.oneshot(request).await.unwrap();
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
            "Invalid URL: Cannot parse `scan_id` with value `nope`: invalid length"
        );
        // `ulid::Ulid`'s own Deserialize: the same failure without the capture's name.
        let plain = axum::Router::new().route(
            "/scans/{scan_id}",
            get(|Path(id): Path<Ulid>| async move { id.to_string() }),
        );
        assert_eq!(path_error(plain).await, "Invalid URL: invalid length");
    }

    #[test]
    fn finding_json() {
        let f = Finding::new(
            "01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().unwrap(),
            "01JAX3KA0000000000000000AB".parse().unwrap(),
            BBox::try_from([0.1, 0.2, 0.3, 0.4]).unwrap(),
            Confidence::try_from(0.71).unwrap(),
            NaiveDate::from_ymd_opt(2026, 10, 20).unwrap(),
            Timestamp::from(at(450)),
        );
        assert_eq!(
            serde_json::to_value(&f).unwrap(),
            json!({
                "scan_id": "01JAX3K9T6R2B8V0QZ4M7D1E5F",
                "image_id": "01JAX3KA0000000000000000AB",
                "bbox": [0.1, 0.2, 0.3, 0.4],
                "confidence": 0.71,
                "confidence_band": "medium",
                "due": "2026-10-20",
                "created_at": "2026-10-18T14:03:22Z"
            })
        );
        assert!(serde_json::from_value::<Confidence>(json!(1.5)).is_err());
        assert!(serde_json::from_value::<BBox>(json!([0.1, 0.2, 1.2, 0.4])).is_err());
    }

    #[test]
    fn band_thresholds() {
        let band = |v: f64| Confidence::try_from(v).unwrap().band();
        assert_eq!(band(0.8), ConfidenceBand::High);
        assert_eq!(band(0.79), ConfidenceBand::Medium);
        assert_eq!(band(0.5), ConfidenceBand::Medium);
        assert_eq!(band(0.49), ConfidenceBand::Low);
    }

    #[test]
    fn schemas() {
        let ut = |s: RefOr<Schema>| serde_json::to_value(s).unwrap();
        assert_eq!(
            ut(<ScanId as utoipa::PartialSchema>::schema()),
            json!({ "type": "string", "example": "01JAX3K9T6R2B8V0QZ4M7D1E5F" })
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
        let sc = serde_json::to_value(schemars::schema_for!(Finding)).unwrap();
        assert_eq!(sc["$defs"]["ScanId"], json!({ "type": "string" }));
        let v1 = serde_json::to_value(schemars::schema_for!(V1)).unwrap();
        assert_eq!(v1["const"], json!(1));
        assert_eq!(
            sc["$defs"]["Confidence"],
            json!({ "type": "number", "minimum": 0, "maximum": 1 })
        );
        assert_eq!(sc["$defs"]["BBox"]["minItems"], json!(4));
        assert_eq!(
            sc["properties"]["due"],
            json!({ "type": "string", "format": "date" })
        );
    }
}
```

## 2. Why each type looks like this

| Type | Decision | Source |
|---|---|---|
| `V1` | A unit type, so a payload can't be built with the wrong version and a `v: 2` fails to deserialize with ``invalid value: integer `2`, expected 1``. The worker reads the version *before* the full job; see [unions.md, version check](unions.md#3-the-worker-checks-v-first) | [contracts 1, versioning](../../../docs/architecture/contracts.md#versioning); [`Deserializer` errors](https://docs.rs/serde/1.0.229/serde/de/trait.Error.html#method.invalid_value) |
| `Timestamp` | chrono 0.4.45's `Serialize` for `DateTime` writes `SecondsFormat::AutoSi` with `Z` (`src/datetime/serde.rs`): a whole second prints as `…:22Z`, anything else as `…:22.120Z` or with 6 or 9 digits. `Utc::now()` is almost never a whole second (it printed 6 digits on macOS). The newtype truncates on the wire with [`to_rfc3339_opts(SecondsFormat::Secs, true)`](https://docs.rs/chrono/0.4.45/chrono/struct.DateTime.html#method.to_rfc3339_opts) and keeps full precision in memory, so ordering and the store's values are unchanged. **Not in the contracts yet:** contracts 1 fixes RFC 3339 in UTC with `Z` but not the precision; whole seconds match every contracts example and are this skill's choice ([contracts 12, question 12](../../../docs/architecture/contracts.md#12-open-questions)) | [contracts 1, wire format](../../../docs/architecture/contracts.md#wire-format); [database 2](../../../docs/database/README.md#2-conventions) (bind `DateTime<Utc>`, so the store calls `into_inner`) |
| `Timestamp` input | chrono's parser is relaxed RFC 3339: besides `T` and `Z` it accepts a space for `T`, lowercase `t` and `z`, `+0200` without the colon, a space before the offset and `UTC`, and converts any offset to UTC. A value without an offset, or with an hour-only offset (`+02`), fails. So input is more lenient than the contracts' format, and output is always the strict form. The `timestamp_input_is_relaxed_rfc_3339` test lists each case | [`FromStr for DateTime<Utc>`](https://docs.rs/chrono/0.4.45/chrono/struct.DateTime.html#impl-FromStr-for-DateTime%3CUtc%3E) ("Accepts a relaxed form of RFC3339"), which chrono's `Deserialize` calls (`src/datetime/serde.rs`, `visit_str`) |
| `ScanId`, `PlantId`, … | One newtype per table over [`ulid::Ulid`](https://docs.rs/ulid/3.0.0/ulid/struct.Ulid.html), serialized `#[serde(transparent)]`, so it is a bare string on the wire and a `ScanId` can't be passed where a `PlantId` is wanted. Parsing is case-insensitive and rejects a `scan:` prefix, so a leaked record ID fails at the boundary. In ulid 3.0.0 the constructor is [`Ulid::generate`](https://docs.rs/ulid/3.0.0/ulid/struct.Ulid.html#method.generate); `Ulid::new` (ulid 1.x, used in the database doc's examples) no longer exists | [contracts 1](../../../docs/architecture/contracts.md#wire-format); [database 2](../../../docs/database/README.md#2-conventions) |
| ID `Deserialize` | Written by hand, parsing inside [`Visitor::visit_str`](https://docs.rs/serde/1.0.229/serde/de/trait.Visitor.html#method.visit_str). axum's path deserializer attaches the capture's name only to an error raised while it drives the visitor (`ErrorKind::DeserializeError { key, value, message }`, `axum-0.8.9/src/extract/path/de.rs`). `Ulid`'s own `Deserialize` takes the string and parses it afterwards, so its error arrives as a bare `ErrorKind::Message` and `details.field` can't name the capture. The `path_errors_name_the_capture` test shows both. [rust-axum-api](../rust-axum-api/error-responses.md#3-extractor-rejections) maps the key to `field` | [axum `ErrorKind`](https://docs.rs/axum/0.8.9/axum/extract/path/enum.ErrorKind.html); the test |
| IDs in the store | The `surrealdb` 3.3.4 SDK depends on ulid 1.2.1, a different crate version, so no `Ulid` crosses into it. The store converts through the string: `RecordId::new("scan", id.to_string())` and back with `FromStr` ([rust-surrealdb](../rust-surrealdb/SKILL.md)) | [database 6](../../../docs/database/README.md#6-rust-sdk); [versions.md](../rust/versions.md#compatibility-checks) |
| IDs in schemas | utoipa doesn't know `Ulid`, so the container says `#[schema(value_type = String)]`; each ID becomes a `string` component (`ScanId`). schemars needs `#[schemars(with = "String")]` on the field | [utoipa `value_type`](https://docs.rs/utoipa/6.0.0/utoipa/derive.ToSchema.html#unnamed-field-struct-optional-configuration-options-for-schema); [schemars `with`](https://graham.cool/schemars/deriving/attributes/#with) |
| A malformed ID | A path or body ID that isn't a ULID fails extraction, which is `400 invalid_request` naming the field, not `404`: nothing can exist under it, so nothing is revealed ([rust-axum-api](../rust-axum-api/SKILL.md) maps the rejection). **Not in the contracts yet:** contracts 2 says only that an unknown ID is `404`; `400` for a malformed one is this skill's choice ([contracts 12, question 14](../../../docs/architecture/contracts.md#12-open-questions)) | [contracts 2](../../../docs/architecture/contracts.md#2-errors) |
| `Slug` | A plain string newtype. Knowledge IDs are slugs, not ULIDs | [contracts 10](../../../docs/architecture/contracts.md#10-knowledge-seed-format) |
| `NaiveDate` for `LocalDate` | chrono serializes it as `"2026-10-20"`; utoipa and schemars render `format: date` | proved by the test |
| `Confidence` | `try_from = "f64"` rejects values outside 0..1 from workers. `confidence_band` is computed by the API, so the band is set in the constructor, never deserialized from a worker. Both schemas are written by hand so they carry `minimum: 0` and `maximum: 1`: utoipa ignores `try_from` ([schema-attrs.md 1](schema-attrs.md#1-which-serde-attributes-each-generator-honours)), and the generated app types and seed checks should know the range | [contracts 1](../../../docs/architecture/contracts.md#wire-format) (`Confidence = number; // 0..1`); [serde `try_from`](https://serde.rs/container-attrs.html#try_from); the `schemas` test |
| `BBox` | `[f64; 4]` with the range check. utoipa renders a bare `[f64; 4]` as an array with no length, so the schema is written by hand; schemars adds `minItems`/`maxItems` 4 on its own | the `schemas` test; [`ArrayBuilder`](https://docs.rs/utoipa/6.0.0/utoipa/openapi/schema/struct.ArrayBuilder.html) |
| Units | The unit is in the field name and the type is a plain number: `duration_ms: u64`, `max_runtime_s: u32`, `temp_c: f64`. Never a `std::time::Duration` on the wire. `Duration::as_millis` returns `u128`; convert with `u64::try_from`, not `as` (clippy `cast_possible_truncation`) | [contracts 1](../../../docs/architecture/contracts.md#wire-format); [`as_millis`](https://doc.rust-lang.org/std/time/struct.Duration.html#method.as_millis); [clippy](https://clippy.rust-lang.org/rust-1.99.0/index.html#cast_possible_truncation) |
