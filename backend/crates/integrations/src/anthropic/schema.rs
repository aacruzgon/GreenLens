//! Makes a JSON Schema acceptable to Anthropic's structured output and strict tools.
//!
//! The API compiles the schema into a grammar and refuses, with a 400, keywords it can't
//! enforce: numeric bounds, string lengths, `maxItems`, and `minItems` above 1
//! ([contracts 6], fallback). It also requires `additionalProperties: false` on every object.
//! The contracts' schemas use those keywords because xgrammar on vLLM enforces them, so the
//! fallback worker and the plan writer strip them here and enforce them after decoding
//! (truncate strings and arrays, clamp numbers) from the list this returns.

use std::borrow::Cow;

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
        schema: if stripped.is_empty() && closed.is_empty() {
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
    let is_object = obj.get("type").is_some_and(|t| {
        t == "object"
            || t.as_array()
                .is_some_and(|a| a.iter().any(|x| x == "object"))
    }) || obj.contains_key("properties");
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
    fn borrows_when_clean() {
        let schema = json!({"type": "object", "properties": {}, "additionalProperties": false});
        assert!(matches!(sanitize_schema(&schema).schema, Cow::Borrowed(_)));
    }
}
