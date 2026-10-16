# utoipa and schemars: which serde attributes they honour

Part of [rust-contract-types](SKILL.md). `utoipa` 6.0.0 (derive `utoipa-gen` 6.0.1) builds the OpenAPI spec, from which the app's TypeScript types are generated; `schemars` 1.2.3 builds JSON Schema for the fixed shapes ([contracts 1, source of truth](../../../docs/architecture/contracts.md#source-of-truth)). Both read `#[serde(...)]` attributes, but not the same ones. Where a generator ignores an attribute, the schema describes a shape the JSON doesn't have, and the app's types are wrong without any error. Every cell below was read in the derive source and, where marked, generated in a test.

## Contents

1. [Which serde attributes each generator honours](#1-which-serde-attributes-each-generator-honours)
2. [Optional and nullable](#2-optional-and-nullable)
3. [The proof](#3-the-proof)

## 1. Which serde attributes each generator honours

Sources: utoipa's list, [Partial `#[serde(...)]` attributes support](https://docs.rs/utoipa/6.0.0/utoipa/derive.ToSchema.html#partial-serde-attributes-support), and its parser, `utoipa-gen-6.0.1/src/component/serde.rs` (`SerdeValue::parse`, `SerdeContainer::parse_attribute`); schemars' list, [Supported Serde Attributes](https://graham.cool/schemars/deriving/attributes/#supported-serde-attributes), and `schemars_derive-1.2.3/src/attr/schemars_to_serde.rs` (`SERDE_KEYWORDS`), which hands them to `serde_derive_internals`. ✓ = generated in [the proof](#3-the-proof) or in [unions.md](unions.md).

| serde attribute | utoipa 6 | schemars 1.2.3 | What to do |
|---|---|---|---|
| `rename_all` (container) | Honoured ✓ | Honoured ✓ | Use on every enum (`"snake_case"`). Structs don't need it: Rust field names are already `snake_case` |
| `rename_all_fields` | **Ignored** (no arm in the parser) | Honoured | Don't use it; it isn't needed |
| `tag` | Honoured: `oneOf`, the tag as a one-value `enum` ✓ | Honoured: `oneOf`, the tag as `const` ✓ | Use for events, `PartialResult`, `status` |
| `tag` + `content` | Honoured ✓ | Honoured ✓ | Use for `kind` + `payload` |
| `untagged` | Honoured: `oneOf` ✓ | Honoured: `anyOf` ✓ | Avoid for anything decoded: [unions.md 4](unions.md#4-complete-tagged-by-status-result-decoded-by-the-jobs-kind) |
| `flatten` | Honoured: `allOf` of the flattened `$ref` and the struct's own object ✓ | Honoured: properties merged, the enum's `oneOf` beside them ✓ | Fine on outbound and tolerant types; never with `deny_unknown_fields` ([unions.md 6](unions.md#6-deny_unknown_fields-breaks-flatten)) |
| `default` (field or container) | Field becomes not required ✓ | Not required in the deserialize contract, required in the serialize contract; adds a `default` value ✓ | Only where the wire really allows the field to be missing |
| `skip_serializing_if` | Field becomes not required, **for input too** ✓ | Not required in the serialize contract; required in the deserialize contract unless it also has `default` or is an `Option` ✓ | Pair it with `default` (or use `Option`) so serde, utoipa and schemars agree |
| `skip`, `skip_deserializing` | Field removed | Field removed / `readOnly` | |
| `skip_serializing` (field) | **Treated as `skip`**: removed from the schema even though it is still read | `writeOnly` | Avoid on fields of types with a schema |
| `with`, `serialize_with` | **Ignored**: the schema is the field's Rust type (only `serde_with`'s `double_option` is recognised) | `with` must name a **type** that implements `JsonSchema`; a module path fails to compile (E0573) ✓. `serialize_with` is ignored | Prefer a newtype with its own `Serialize` and schema impls, like `Timestamp` ([primitives.md](primitives.md)). With `with`, add `#[schemars(with = "Type")]` |
| `from`, `try_from`, `into` | **Ignored**: renders the struct's own fields ✓ | Honoured: `from`/`try_from` set the deserialize schema, `into` the serialize schema ✓ | Add `#[schema(value_type = ...)]` or a hand-written `PartialSchema` ([primitives.md](primitives.md), `BBox`) |
| `deny_unknown_fields` | Honoured: `additionalProperties: false` ✓ | Honoured: `additionalProperties: false` ✓ | Only on inbound app requests and config. **Not in the contracts yet** for app requests ([contracts 12, question 13](../../../docs/architecture/contracts.md#12-open-questions)) |
| `transparent` | **Ignored**. A tuple newtype still renders as its field (unnamed single-field structs always do); a braced one-field struct renders as an object ✓ | Honoured ✓ | Only on tuple newtypes (`ScanId(Ulid)`) |
| `other` (variant) | **Ignored**: listed as an ordinary value (`"other"` in a string enum) ✓; hidden only if the variant is also `skip_serializing` ✓ | **Ignored**: listed as an ordinary variant (`{"type": "unknown"}`) ✓ | Only on types the API reads from workers (`WorkerEvent`), which need no JSON Schema. Add `#[schemars(skip)]` if one ever does |

To make a *schema* strict without making the *type* strict, as for the plan writer's output, use `#[schemars(extend("additionalProperties" = false))]` ([`extend`](https://graham.cool/schemars/deriving/attributes/#extend)): the model is told no extra keys are allowed, and serde still drops one if it appears ✓.

utoipa has one schema per type and no notion of direction; schemars has a [contract](https://graham.cool/schemars/generating/#serialize-vs-deserialize-contract), `Deserialize` by default. The fixed shapes are both *read* (seed files, the plan writer's output), so generate their schemas with the default contract.

## 2. Optional and nullable

[Contracts 1, notation](../../../docs/architecture/contracts.md#notation): `field?: T` is omitted when absent and never `null`; `field: T | null` is always present and may be `null`. Rust spells both as `Option<T>`; the attributes make the difference.

| Contract | Rust | serde out | utoipa | schemars (deserialize contract) |
|---|---|---|---|---|
| `field?: T` | `#[serde(default, skip_serializing_if = "Option::is_none")]` `#[schema(nullable = false)]` `field: Option<T>` | omitted when `None` | not required, not nullable ✓ | not required, nullable (`["T", "null"]`), since serde accepts `null` for an `Option` ✓ |
| `field: T \| null` | `#[schema(required = true)]` `field: Option<T>` | `"field": null` ✓ | required, nullable ✓ | not required, nullable: serde itself accepts a missing `Option` as `None` ✓ |

- utoipa makes every `Option` field nullable *and* not required ([field nullability and required rules](https://docs.rs/utoipa/6.0.0/utoipa/derive.ToSchema.html#field-nullability-and-required-rules)). Without `#[schema(required = true)]`, a `T | null` field comes out as optional; without `#[schema(nullable = false)]`, a `T?` field comes out as `T | null`, and the generated TypeScript says `field?: T | null`.
- `#[schemars(required)]` on an `Option<T>` makes the field required **and drops `null`** ("this will create a schema as though the field were a `T`", [schemars `required`](https://graham.cool/schemars/deriving/attributes/#required)). It is not the way to spell `T | null`. None of the fixed shapes has a `| null` field today ([contracts 7, action plan](../../../docs/architecture/contracts.md#actionplan); [contracts 10](../../../docs/architecture/contracts.md#10-knowledge-seed-format)).
- serde fills a missing `Option` field with `None` even without `default`, but not when the field has `deserialize_with` or `with`: then the field is required unless it also has `default` ✓. That is why `default` stays on every `?` field.
- **A JSON `null` on input also reads as `None`**, the same as a missing field ✓, so with a plain `Option<T>` a `PATCH` body can't say "clear this field". No contract `PATCH` body has a `| null` field today: `PATCH /gardens/{id}`, `/plants/{id}`, `/tasks/{id}` and `/me` take `?` fields only ([contracts 9](../../../docs/architecture/contracts.md#9-gardens-plants-tasks-and-account)), so none can clear a value. If one gains `field?: T | null` meaning "absent: leave it, `null`: clear it", type it `Option<Option<T>>` with `default` and a `deserialize_with` that wraps whatever is present, `null` included, in `Some` ([`default`](https://serde.rs/field-attrs.html#default), [`deserialize_with`](https://serde.rs/field-attrs.html#deserialize_with)). Neither generator is shown handling this pattern here, so assert the field's schema in a test when you add one ([section 1](#1-which-serde-attributes-each-generator-honours)).

```rust
use chrono::NaiveDate;
use serde::{Deserialize, Deserializer};

/// Whatever is present, `null` included, becomes `Some`; `default` keeps absent as `None`.
fn present<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// A hypothetical body whose `due` could be cleared: `due?: LocalDate | null`.
#[derive(Debug, Deserialize)]
pub struct Clearable {
    #[serde(default, deserialize_with = "present")]
    pub due: Option<Option<NaiveDate>>,
}

/// What a plain `Option` reads.
#[derive(Debug, Deserialize)]
pub struct Plain {
    #[serde(default)]
    pub due: Option<NaiveDate>,
}

#[test]
fn null_and_absent_differ_only_with_a_double_option() {
    let clearable = |s: &str| serde_json::from_str::<Clearable>(s).unwrap().due;
    assert_eq!(clearable("{}"), None);
    assert_eq!(clearable(r#"{"due":null}"#), Some(None));
    assert_eq!(
        clearable(r#"{"due":"2026-10-20"}"#),
        Some(NaiveDate::from_ymd_opt(2026, 10, 20))
    );
    let plain = |s: &str| serde_json::from_str::<Plain>(s).unwrap().due;
    assert_eq!(plain("{}"), None);
    assert_eq!(plain(r#"{"due":null}"#), None);
}
```

## 3. The proof

```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, ToSchema, JsonSchema)]
pub struct Shapes {
    // T | null
    #[schema(required = true)]
    pub nullable: Option<String>,
    // T?
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub optional: Option<String>,
    // bare Option: what you get by default
    pub bare: Option<String>,
    // skip_serializing_if without default: serde requires it on input
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub only_skip_if: Vec<String>,
    #[serde(default)]
    pub only_default: Vec<String>,
}

#[derive(Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(transparent)]
pub struct Braced {
    pub inner: String,
}

#[derive(Clone, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct Checked {
    raw: String,
}

impl TryFrom<String> for Checked {
    type Error = &'static str;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        if raw.is_empty() {
            Err("empty")
        } else {
            Ok(Self { raw })
        }
    }
}

impl From<Checked> for String {
    fn from(c: Checked) -> Self {
        c.raw
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    Yes,
    No,
    #[serde(other)]
    Other,
}

#[derive(Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Internal {
    A { x: u32 },
}

#[derive(Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum Adjacent {
    A { x: u32 },
}

#[derive(Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(untagged)]
pub enum Untagged {
    N(u32),
    S(String),
}

#[derive(Serialize, Deserialize, ToSchema, JsonSchema)]
pub struct Flattened {
    pub seq: u64,
    #[serde(flatten)]
    pub body: Internal,
}

#[derive(Deserialize, ToSchema, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub name: String,
}

/// Model output: tolerant in serde, strict in the schema the model is given.
#[derive(Deserialize, JsonSchema)]
#[schemars(extend("additionalProperties" = false))]
pub struct ModelOutput {
    pub summary: String,
}

fn utoipa<T: utoipa::PartialSchema>() -> Value {
    serde_json::to_value(T::schema()).unwrap_or_default()
}

fn schemars_de<T: JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T)).unwrap_or_default()
}

fn schemars_ser<T: JsonSchema>() -> Value {
    let generator = schemars::generate::SchemaSettings::default()
        .for_serialize()
        .into_generator();
    serde_json::to_value(generator.into_root_schema_for::<T>()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utoipa_required_and_nullable() {
        let s = utoipa::<Shapes>();
        // only `nullable`: `only_skip_if` is not required here although serde requires it on input
        assert_eq!(s["required"], json!(["nullable"]));
        assert_eq!(
            s["properties"]["nullable"]["type"],
            json!(["string", "null"])
        );
        assert_eq!(s["properties"]["optional"]["type"], json!("string"));
        assert_eq!(s["properties"]["bare"]["type"], json!(["string", "null"]));
    }

    #[test]
    fn schemars_contracts_differ() {
        assert_eq!(schemars_de::<Shapes>()["required"], json!(["only_skip_if"]));
        assert_eq!(
            schemars_de::<Shapes>()["properties"]["only_default"]["default"],
            json!([])
        );
        assert_eq!(
            schemars_ser::<Shapes>()["required"],
            json!(["nullable", "bare", "only_default"])
        );
        // serde agrees with the deserialize contract: only `only_skip_if` must be present.
        assert!(serde_json::from_value::<Shapes>(json!({ "only_skip_if": [] })).is_ok());
        assert!(serde_json::from_value::<Shapes>(json!({})).is_err());
    }

    #[test]
    fn transparent_and_try_from() {
        assert_eq!(utoipa::<Braced>()["type"], json!("object"));
        assert_eq!(schemars_de::<Braced>()["type"], json!("string"));
        assert_eq!(
            utoipa::<Checked>()["properties"]["raw"]["type"],
            json!("string")
        );
        assert_eq!(schemars_de::<Checked>()["type"], json!("string"));
    }

    #[test]
    fn other_is_just_another_value() {
        assert_eq!(
            serde_json::from_value::<Answer>(json!("maybe")).unwrap(),
            Answer::Other
        );
        assert_eq!(utoipa::<Answer>()["enum"], json!(["yes", "no", "other"]));
        assert_eq!(
            schemars_de::<Answer>()["enum"],
            json!(["yes", "no", "other"])
        );
    }

    #[test]
    fn enum_representations() {
        let ut = utoipa::<Internal>();
        assert_eq!(
            ut["oneOf"][0]["properties"]["type"],
            json!({ "type": "string", "enum": ["a"] })
        );
        let sc = schemars_de::<Internal>();
        assert_eq!(
            sc["oneOf"][0]["properties"]["type"],
            json!({ "type": "string", "const": "a" })
        );
        assert_eq!(
            utoipa::<Adjacent>()["oneOf"][0]["required"],
            json!(["payload", "kind"])
        );
        assert_eq!(
            schemars_de::<Adjacent>()["oneOf"][0]["required"],
            json!(["kind", "payload"])
        );
        assert!(utoipa::<Untagged>()["oneOf"].is_array());
        assert!(schemars_de::<Untagged>()["anyOf"].is_array());
        let ut = utoipa::<Flattened>();
        assert_eq!(
            ut["allOf"][0],
            json!({ "$ref": "#/components/schemas/Internal" })
        );
        assert_eq!(ut["allOf"][1]["required"], json!(["seq"]));
        let sc = schemars_de::<Flattened>();
        assert_eq!(sc["required"], json!(["seq"]));
        assert!(sc["oneOf"].is_array());
    }

    #[test]
    fn strict_schema_tolerant_type() {
        assert_eq!(
            schemars_de::<ModelOutput>()["additionalProperties"],
            json!(false)
        );
        let extra = json!({ "summary": "Remove the lowest leaves", "confidence": "high" });
        assert!(serde_json::from_value::<ModelOutput>(extra).is_ok());
    }

    #[test]
    fn deny_unknown_fields_in_both() {
        assert_eq!(utoipa::<Request>()["additionalProperties"], json!(false));
        assert_eq!(
            schemars_de::<Request>()["additionalProperties"],
            json!(false)
        );
    }
}
```

`#[serde(other)]` on a plain string enum, as in `Answer`, compiles and works with serde_derive 1.0.229, which only checks that the variant is the last one, a unit variant, and not in an untagged enum (`serde_derive-1.0.229/src/internals/check.rs`). serde's own docs restrict it to internally and adjacently tagged enums ([`other`](https://serde.rs/variant-attrs.html#other)), so the project relies on it only in tagged enums.

schemars rejects a `with` module at compile time:

```rust,compile_fail,E0573
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::Serialize;

mod whole_seconds {
    use chrono::{DateTime, SecondsFormat, Utc};
    use serde::Serializer;

    pub fn serialize<S: Serializer>(t: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&t.to_rfc3339_opts(SecondsFormat::Secs, true))
    }
}

#[derive(Serialize, JsonSchema)]
pub struct Event {
    #[serde(with = "whole_seconds")]
    pub ts: DateTime<Utc>,
}
```
