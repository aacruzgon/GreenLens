//! Makes a JSON Schema acceptable to Anthropic's structured output and strict tools.
//!
//! The API compiles the schema into a grammar and refuses, with a 400, keywords it can't
//! enforce: numeric bounds, string lengths, `maxItems`, and `minItems` above 1
//! ([contracts 6], fallback). It also requires `additionalProperties: false` on every object.
//! The contracts' schemas use those keywords because xgrammar on vLLM enforces them, so the
//! fallback worker and the plan writer strip them here and enforce them after decoding
//! (truncate strings and arrays, clamp numbers) from the list this returns.
//!
//! `oneOf` is refused too ("Schema type 'oneOf' is not supported", seen live on 2026-10-16), and
//! `schemars` writes it for every enum with documented variants and every tagged union. A
//! `oneOf` of constants becomes an `enum`, with the variants' descriptions moved into the
//! parent's; any other `oneOf` becomes an `anyOf`, as the official Python SDK does with every
//! `oneOf`. A `format` survives only on a string and only if the API lists it, so `schemars`'s
//! `uint32` and `double` go.
//!
//! Strict schemas may hold at most 16 parameters with a union type (`anyOf` or a type array)
//! across a request (refused live with 20). A nullable enum, `anyOf: [{enum: [...]}, {type:
//! null}]`, becomes one `enum` with `null` among its values: the same answers, and no union. The
//! fallback's indicators call has one such `image` per question, so it no longer grows into the
//! limit.

use std::{borrow::Cow, fmt::Write as _};

use serde_json::{Map, Value};

/// Keywords removed wherever they appear in a schema position.
pub const UNSUPPORTED_KEYWORDS: &[&str] = &[
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "minLength",
    "maxLength",
    "maxItems",
    "minProperties",
    "maxProperties",
];

/// The string formats the API accepts (Anthropic's structured outputs docs, JSON Schema limitations).
pub const SUPPORTED_FORMATS: &[&str] = &[
    "date-time",
    "time",
    "date",
    "duration",
    "email",
    "hostname",
    "uri",
    "ipv4",
    "ipv6",
    "uuid",
];

/// One keyword the sanitiser removed, so the caller can enforce it after decoding.
#[derive(Debug, Clone, PartialEq)]
pub struct Stripped {
    /// JSON Pointer to the schema object that held it, such as `/properties/summary`.
    pub at: String,
    /// The keyword.
    pub keyword: &'static str,
    /// Its value.
    pub value: Value,
}

/// A sanitised schema. `'a` is the input schema: when nothing had to change, `schema` borrows
/// it (`Cow::Borrowed`) and nothing is copied; otherwise it owns a cleaned copy. Either way it
/// drops into [`super::OutputFormat::JsonSchema`] or [`super::Tool::input_schema`], which take a
/// `Cow<'a, Value>`.
#[derive(Debug, Clone, PartialEq)]
pub struct Sanitized<'a> {
    /// The schema to send.
    pub schema: Cow<'a, Value>,
    /// What was removed, in document order.
    pub stripped: Vec<Stripped>,
    /// Objects that had no `additionalProperties` and got `false`.
    pub closed: Vec<String>,
}

/// Sanitises a schema, copying it only if something changes.
pub fn sanitize_schema(schema: &Value) -> Sanitized<'_> {
    let mut copy = schema.clone();
    let (stripped, closed) = sanitize_in_place(&mut copy);
    Sanitized {
        // Rewrites (`oneOf`) change the schema without stripping anything.
        schema: if copy == *schema {
            Cow::Borrowed(schema)
        } else {
            Cow::Owned(copy)
        },
        stripped,
        closed,
    }
}

/// Sanitises a schema in place; returns what was stripped and which objects were closed.
pub fn sanitize_in_place(schema: &mut Value) -> (Vec<Stripped>, Vec<String>) {
    let mut stripped = Vec::new();
    let mut closed = Vec::new();
    walk(schema, &mut String::new(), &mut stripped, &mut closed);
    (stripped, closed)
}

/// Walks schema positions only: the values of `properties`, `$defs` and `definitions` are
/// schemas, their keys are names (a property called `maximum` is kept).
fn walk(node: &mut Value, at: &mut String, stripped: &mut Vec<Stripped>, closed: &mut Vec<String>) {
    let Value::Object(obj) = node else {
        return;
    };
    for &keyword in UNSUPPORTED_KEYWORDS {
        if let Some(value) = obj.remove(keyword) {
            stripped.push(Stripped {
                at: at.clone(),
                keyword,
                value,
            });
        }
    }
    if obj
        .get("minItems")
        .and_then(Value::as_u64)
        .is_some_and(|n| n > 1)
        && let Some(value) = obj.remove("minItems")
    {
        stripped.push(Stripped {
            at: at.clone(),
            keyword: "minItems",
            value,
        });
    }
    let format_ok = obj.get("format").is_none_or(|f| {
        has_type(obj, "string") && f.as_str().is_some_and(|f| SUPPORTED_FORMATS.contains(&f))
    });
    if !format_ok && let Some(value) = obj.remove("format") {
        stripped.push(Stripped {
            at: at.clone(),
            keyword: "format",
            value,
        });
    }
    // A `oneOf` that isn't a list is malformed: left for the API to refuse with its reason.
    if obj.get("oneOf").is_some_and(Value::is_array)
        && let Some(Value::Array(variants)) = obj.remove("oneOf")
    {
        replace_one_of(obj, variants);
    }
    collapse_nullable_enum(obj);
    let is_object = has_type(obj, "object") || obj.contains_key("properties");
    if is_object && !obj.contains_key("additionalProperties") {
        obj.insert("additionalProperties".to_owned(), Value::Bool(false));
        closed.push(at.clone());
    }

    for key in ["properties", "$defs", "definitions"] {
        if let Some(Value::Object(map)) = obj.get_mut(key) {
            walk_map(map, key, at, stripped, closed);
        }
    }
    for key in ["items", "additionalProperties", "not", "contains"] {
        if let Some(child) = obj.get_mut(key) {
            with_segment(at, key, |at| walk(child, at, stripped, closed));
        }
    }
    for key in ["anyOf", "allOf", "oneOf", "prefixItems"] {
        if let Some(Value::Array(list)) = obj.get_mut(key) {
            for (i, child) in list.iter_mut().enumerate() {
                with_segment(at, &format!("{key}/{i}"), |at| {
                    walk(child, at, stripped, closed);
                });
            }
        }
    }
}

/// Whether `type` is `name` or a list that holds it.
fn has_type(obj: &Map<String, Value>, name: &str) -> bool {
    obj.get("type")
        .is_some_and(|t| t == name || t.as_array().is_some_and(|a| a.iter().any(|x| x == name)))
}

/// Whether a value may sit in an `enum`: the API refuses objects and arrays there.
const fn is_scalar(v: &Value) -> bool {
    !matches!(v, Value::Object(_) | Value::Array(_))
}

/// Puts the variants of a removed `oneOf` back as an `enum` when each is a documented scalar
/// constant (`{"const": "open", "description": "…", "type": "string"}`), otherwise as an
/// `anyOf`. A schema that already has an `enum` keeps it and gets the `anyOf`, so the two still
/// both apply.
fn replace_one_of(obj: &mut Map<String, Value>, variants: Vec<Value>) {
    let constants: Option<Vec<&Map<String, Value>>> = variants
        .iter()
        .map(|v| {
            v.as_object().filter(|v| {
                v.get("const").is_some_and(is_scalar)
                    && v.keys()
                        .all(|k| matches!(k.as_str(), "const" | "description" | "type" | "title"))
            })
        })
        .collect();
    let constants = constants.filter(|c| !c.is_empty() && !obj.contains_key("enum"));
    let Some(constants) = constants else {
        if obj.contains_key("anyOf") {
            let all_of = obj
                .entry("allOf")
                .or_insert_with(|| Value::Array(Vec::new()));
            if let Value::Array(list) = all_of {
                list.push(serde_json::json!({ "anyOf": variants }));
            }
        } else {
            obj.insert("anyOf".to_owned(), Value::Array(variants));
        }
        return;
    };
    let types: Vec<&Value> = constants.iter().filter_map(|v| v.get("type")).collect();
    if !obj.contains_key("type")
        && types.len() == constants.len()
        && types.windows(2).all(|w| w[0] == w[1])
        && let Some(&t) = types.first()
    {
        obj.insert("type".to_owned(), t.clone());
    }
    let mut notes = String::new();
    for v in &constants {
        if let Some(d) = v.get("description").and_then(Value::as_str) {
            let _ = write!(notes, "\n- {}: {d}", v["const"]);
        }
    }
    if !notes.is_empty() {
        let description = obj
            .get("description")
            .and_then(Value::as_str)
            .map_or_else(|| notes.trim_start().to_owned(), |d| format!("{d}{notes}"));
        obj.insert("description".to_owned(), Value::String(description));
    }
    let values = constants.iter().map(|v| v["const"].clone()).collect();
    obj.insert("enum".to_owned(), Value::Array(values));
}

/// Turns `anyOf: [{enum: [...]}, {type: "null"}]` (either order) into one `enum` holding `null`,
/// so a nullable enum isn't a union-typed parameter. The enum branch may carry only `type`,
/// `description` and `title` besides its values; its description moves up if the parent has
/// none.
fn collapse_nullable_enum(obj: &mut Map<String, Value>) {
    if obj.contains_key("type") || obj.contains_key("enum") {
        return;
    }
    let Some(Value::Array(branches)) = obj.get("anyOf") else {
        return;
    };
    let [a, b] = branches.as_slice() else {
        return;
    };
    let is_null = |v: &Value| {
        v.as_object()
            .is_some_and(|o| o.len() == 1 && o["type"] == "null")
    };
    let choices = if is_null(b) {
        a
    } else if is_null(a) {
        b
    } else {
        return;
    };
    let Some(choices) = choices.as_object().filter(|o| {
        o.get("enum")
            .and_then(Value::as_array)
            .is_some_and(|e| !e.is_empty() && e.iter().all(is_scalar))
            && o.keys()
                .all(|k| matches!(k.as_str(), "enum" | "type" | "description" | "title"))
    }) else {
        return;
    };
    let mut values = choices["enum"].as_array().cloned().unwrap_or_default();
    if !values.contains(&Value::Null) {
        values.push(Value::Null);
    }
    let description = choices.get("description").cloned();
    obj.remove("anyOf");
    if let Some(d) = description
        && !obj.contains_key("description")
    {
        obj.insert("description".to_owned(), d);
    }
    obj.insert("enum".to_owned(), Value::Array(values));
}

fn walk_map(
    map: &mut Map<String, Value>,
    key: &str,
    at: &mut String,
    stripped: &mut Vec<Stripped>,
    closed: &mut Vec<String>,
) {
    for (name, child) in map.iter_mut() {
        let escaped = name.replace('~', "~0").replace('/', "~1");
        with_segment(at, &format!("{key}/{escaped}"), |at| {
            walk(child, at, stripped, closed);
        });
    }
}

fn with_segment(at: &mut String, segment: &str, f: impl FnOnce(&mut String)) {
    let len = at.len();
    at.push('/');
    at.push_str(segment);
    f(at);
    at.truncate(len);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strips_bounds_and_closes_objects() {
        let schema = json!({
            "type": "object",
            "properties": {
                "summary": {"type": "string", "maxLength": 280},
                "maximum": {"type": "integer", "minimum": 0, "maximum": 1000},
                "boxes": {"type": "array", "minItems": 4, "maxItems": 4, "items": {"type": "integer"}},
                "nested": {"anyOf": [{"type": "object", "properties": {"a": {"type": "number", "multipleOf": 0.5}}}, {"type": "null"}]}
            },
            "required": ["summary"],
            "additionalProperties": false
        });
        let out = sanitize_schema(&schema);
        assert!(matches!(out.schema, Cow::Owned(_)));
        let s = out.schema.as_ref();
        assert!(s["properties"]["summary"].get("maxLength").is_none());
        assert!(
            s["properties"].get("maximum").is_some(),
            "a property name is kept"
        );
        assert!(s["properties"]["maximum"].get("maximum").is_none());
        assert!(s["properties"]["boxes"].get("minItems").is_none());
        assert_eq!(
            s["properties"]["nested"]["anyOf"][0]["additionalProperties"],
            json!(false)
        );
        assert_eq!(out.closed, vec!["/properties/nested/anyOf/0".to_owned()]);
        let kw: Vec<_> = out
            .stripped
            .iter()
            .map(|s| (s.at.as_str(), s.keyword))
            .collect();
        assert!(kw.contains(&("/properties/summary", "maxLength")));
        assert!(kw.contains(&("/properties/boxes", "minItems")));
        assert!(kw.contains(&("/properties/nested/anyOf/0/properties/a", "multipleOf")));
    }

    #[test]
    fn one_of_becomes_an_enum_or_an_any_of() {
        let schema = json!({
            "type": "object",
            "properties": {
                "state": {"description": "Which tasks.", "oneOf": [
                    {"const": "open", "description": "Not done.", "type": "string"},
                    {"const": "done", "description": "Done.", "type": "string"}
                ]},
                "when": {"oneOf": [
                    {"type": "object", "properties": {"kind": {"const": "now", "type": "string"}}},
                    {"type": "object", "properties": {"kind": {"const": "on", "type": "string"}}}
                ]}
            },
            "additionalProperties": false
        });
        let out = sanitize_schema(&schema);
        let s = out.schema.as_ref();
        let state = &s["properties"]["state"];
        assert_eq!(state["enum"], json!(["open", "done"]));
        assert_eq!(state["type"], json!("string"));
        assert_eq!(
            state["description"],
            json!("Which tasks.\n- \"open\": Not done.\n- \"done\": Done.")
        );
        assert!(state.get("oneOf").is_none());
        let when = &s["properties"]["when"];
        assert!(when.get("oneOf").is_none());
        assert_eq!(when["anyOf"][1]["additionalProperties"], json!(false));
    }

    #[test]
    fn formats_survive_only_on_strings_the_api_lists() {
        let schema = json!({
            "type": "object",
            "properties": {
                "days": {"type": "integer", "format": "uint32"},
                "due": {"type": "string", "format": "date"},
                "id": {"type": "string", "format": "ulid"}
            },
            "additionalProperties": false
        });
        let out = sanitize_schema(&schema);
        let p = &out.schema["properties"];
        assert!(p["days"].get("format").is_none());
        assert_eq!(p["due"]["format"], json!("date"));
        assert!(p["id"].get("format").is_none());
        assert_eq!(out.stripped.len(), 2);
    }

    #[test]
    fn a_rewrite_alone_is_not_borrowed() {
        let schema = json!({"oneOf": [{"const": 1}, {"const": 2}]});
        let out = sanitize_schema(&schema);
        assert!(matches!(out.schema, Cow::Owned(_)));
        assert_eq!(out.schema["enum"], json!([1, 2]));
    }

    #[test]
    fn a_nullable_enum_becomes_one_enum_with_null() {
        let schema = json!({
            "type": "object",
            "properties": {
                "image": {"anyOf": [{"type": "string", "enum": ["img1", "img2"]}, {"type": "null"}]},
                "note": {"anyOf": [{"type": "string"}, {"type": "null"}]}
            },
            "required": ["image", "note"],
            "additionalProperties": false
        });
        let out = sanitize_schema(&schema);
        let p = &out.schema["properties"];
        assert_eq!(p["image"], json!({"enum": ["img1", "img2", null]}));
        assert!(
            p["note"].get("anyOf").is_some(),
            "a nullable string stays a union"
        );
    }

    #[test]
    fn one_of_keeps_an_existing_enum_and_refuses_complex_constants() {
        let both = json!({"enum": ["a", "b"], "oneOf": [{"const": "a"}, {"const": "z"}]});
        let both = sanitize_schema(&both);
        assert_eq!(both.schema["enum"], json!(["a", "b"]));
        assert_eq!(
            both.schema["anyOf"],
            json!([{"const": "a"}, {"const": "z"}])
        );
        let complex = json!({"oneOf": [{"const": {"k": 1}}]});
        let complex = sanitize_schema(&complex);
        assert!(complex.schema.get("enum").is_none());
        assert_eq!(complex.schema["anyOf"], json!([{"const": {"k": 1}}]));
    }

    #[test]
    fn borrows_when_clean() {
        let schema = json!({"type": "object", "properties": {}, "additionalProperties": false});
        assert!(matches!(sanitize_schema(&schema).schema, Cow::Borrowed(_)));
    }
}
