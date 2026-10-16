//! Reads straight into domain types.
//!
//! Most reads project the stored record into its contract shape in `SurrealQL`
//! (`scan AS scan_id`, `record::id(ref)` …), take the result as a `surrealdb::types::Value`,
//! convert it with [`json_of`] and let the domain type's own serde `Deserialize` build it. The
//! conversion is the contract's wire rules: a record id becomes its bare key (the ULID or the
//! slug, contracts 1), a `datetime` an RFC 3339 string with its full precision, and `NONE` an
//! absent field. So one function replaces a row struct and a hand-written conversion per type,
//! and a field the projection gets wrong fails loudly as a `RowError`, never silently.

use serde::de::DeserializeOwned;
use surrealdb::{
    IndexedResults,
    types::{Number, RecordIdKey, Value},
};

use crate::error::{RowError, take_failed};
use domain::ports::StoreError;

/// A `SurrealDB` value as the JSON the contracts define.
pub fn json_of(value: Value) -> serde_json::Value {
    use serde_json::Value as J;
    match value {
        Value::None | Value::Null => J::Null,
        Value::Bool(b) => J::Bool(b),
        Value::Number(Number::Int(i)) => J::from(i),
        Value::Number(Number::Float(f)) => {
            serde_json::Number::from_f64(f).map_or(J::Null, J::Number)
        }
        Value::Number(Number::Decimal(d)) => d
            .to_string()
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map_or(J::Null, J::Number),
        Value::String(s) => J::String(s),
        Value::Datetime(d) => J::String(
            d.into_inner()
                .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
        ),
        Value::RecordId(id) => match id.key {
            RecordIdKey::String(s) => J::String(s),
            RecordIdKey::Number(n) => J::from(n),
            RecordIdKey::Array(parts) => J::Array(parts.into_iter().map(json_of).collect()),
            other => J::String(format!("{other:?}")),
        },
        Value::Duration(d) => J::from(d.into_inner().as_secs()),
        Value::Array(items) => J::Array(items.into_iter().map(json_of).collect()),
        Value::Object(fields) => J::Object(
            fields
                .into_iter()
                .filter(|(_, v)| !matches!(v, Value::None | Value::Null))
                .map(|(k, v)| (k, json_of(v)))
                .collect(),
        ),
        Value::Uuid(u) => J::String(u.to_string()),
        other => J::String(format!("{other:?}")),
    }
}

/// A value as a domain type.
pub fn decode<T: DeserializeOwned>(what: &'static str, value: Value) -> Result<T, RowError> {
    serde_json::from_value(json_of(value)).map_err(|source| RowError::Json {
        field: what,
        source,
    })
}

/// Statement `index`'s single record (`SELECT … FROM ONLY`), or `None`.
pub fn take_one<T: DeserializeOwned>(
    results: &mut IndexedResults,
    index: usize,
    what: &'static str,
) -> Result<Option<T>, StoreError> {
    let value: Value = results.take(index).map_err(|e| take_failed(what, e))?;
    match value {
        Value::None | Value::Null => Ok(None),
        Value::Array(items) => match items.into_iter().next() {
            Some(first) => decode(what, first)
                .map(Some)
                .map_err(|e| e.into_store(what)),
            None => Ok(None),
        },
        other => decode(what, other)
            .map(Some)
            .map_err(|e| e.into_store(what)),
    }
}

/// Statement `index`'s records.
pub fn take_all<T: DeserializeOwned>(
    results: &mut IndexedResults,
    index: usize,
    what: &'static str,
) -> Result<Vec<T>, StoreError> {
    let value: Value = results.take(index).map_err(|e| take_failed(what, e))?;
    match value {
        Value::None | Value::Null => Ok(Vec::new()),
        Value::Array(items) => items
            .into_iter()
            .map(|v| decode(what, v).map_err(|e| e.into_store(what)))
            .collect(),
        other => decode(what, other)
            .map(|t| vec![t])
            .map_err(|e| e.into_store(what)),
    }
}

/// A page from rows fetched with `LIMIT limit + 1`: the extra row only says there is more,
/// and the cursor is the last returned row's key ([database 5]: page by cursor, never `START`).
pub fn page<T>(
    mut rows: Vec<T>,
    limit: u32,
    cursor_of: impl Fn(&T) -> String,
) -> domain::wire::Page<T> {
    let limit = limit as usize;
    let more = rows.len() > limit;
    rows.truncate(limit);
    let next_cursor = if more {
        rows.last().and_then(|r| cursor_of(r).parse().ok())
    } else {
        None
    };
    domain::wire::Page {
        items: rows,
        next_cursor,
    }
}

#[cfg(test)]
mod tests {
    use surrealdb::types::{Array, Object, RecordId};

    use super::*;

    #[test]
    fn record_ids_become_bare_keys_and_none_is_absent() {
        let mut fields = std::collections::BTreeMap::new();
        fields.insert(
            "scan_id".to_owned(),
            Value::RecordId(RecordId::new("scan", "01JAX3K9T6R2B8V0QZ4M7D1E5S")),
        );
        fields.insert("note".to_owned(), Value::None);
        fields.insert(
            "tags".to_owned(),
            Value::Array(Array::from(vec![Value::String("a".into())])),
        );
        let json = json_of(Value::Object(Object::from(fields)));
        assert_eq!(
            json,
            serde_json::json!({ "scan_id": "01JAX3K9T6R2B8V0QZ4M7D1E5S", "tags": ["a"] })
        );
    }
}
