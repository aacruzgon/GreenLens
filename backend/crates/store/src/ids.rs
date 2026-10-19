//! `Id<T>` and `Slug<K>` ↔ `surrealdb::RecordId` ([database 2]; convention 5: `domain` never
//! depends on `surrealdb`, so the mapping lives here).
//!
//! ULID records are `RecordId::new(T::NAME, ulid_string)`: `RecordIdKey` has no `From<Ulid>`
//! (`surrealdb-types` uses `ulid` 1.x internally), so the key is the 26-character string.
//! Array keys are built with `Array::from(vec![Value::RecordId(..), Value::Number(..)])`
//! ✓; `RecordIdKey` has no `From<Vec<_>>`, and `RecordId::parse_simple` always yields a string
//! key, so it is never used.

use chrono::{DateTime, NaiveDate, Utc};
use domain::{
    id::{
        ConnectorId, FarmRecordId, Id, IdempotencyKey, IndicatorId, MessageId, PushToken, ScanId,
        Slug, SlugKind, Table, UserId, WorkerId,
    },
    wire::{EventSeq, Run},
};
use surrealdb::types::{Array, Number, RecordId, RecordIdKey, SurrealValue, Value};

use crate::error::RowError;

/// A domain id that names one record.
pub trait ToRecordId {
    /// The record id.
    fn record_id(&self) -> RecordId;
}

/// A domain id read back from a record id. Checks the table and the key's shape.
pub trait FromRecordId: Sized {
    /// Parses the record id.
    fn from_record_id(id: &RecordId) -> Result<Self, RowError>;
}

impl<T: Table> ToRecordId for Id<T> {
    fn record_id(&self) -> RecordId {
        RecordId::new(T::NAME, self.to_string())
    }
}

impl<T: Table> FromRecordId for Id<T> {
    fn from_record_id(id: &RecordId) -> Result<Self, RowError> {
        string_key(id, T::NAME)?
            .parse()
            .map_err(|_| bad_id(id, T::NAME))
    }
}

impl<K: SlugKind> ToRecordId for Slug<K> {
    fn record_id(&self) -> RecordId {
        RecordId::new(K::TABLE, self.as_str())
    }
}

impl<K: SlugKind> FromRecordId for Slug<K> {
    fn from_record_id(id: &RecordId) -> Result<Self, RowError> {
        string_key(id, K::TABLE)?
            .parse()
            .map_err(|_| bad_id(id, K::TABLE))
    }
}

/// `rid(id)`: the shortest spelling at a bind site.
pub fn rid<I: ToRecordId + ?Sized>(id: &I) -> RecordId {
    id.record_id()
}

/// `rid` over an `Option`, for `option<record<…>>` fields (`None` binds as `NONE`).
pub fn opt_rid<I: ToRecordId>(id: Option<&I>) -> Option<RecordId> {
    id.map(ToRecordId::record_id)
}

/// The domain id of a record id.
pub fn from_rid<I: FromRecordId>(id: &RecordId) -> Result<I, RowError> {
    I::from_record_id(id)
}

/// The domain id of an optional record id.
pub fn from_opt_rid<I: FromRecordId>(id: Option<&RecordId>) -> Result<Option<I>, RowError> {
    id.map(I::from_record_id).transpose()
}

fn string_key<'a>(id: &'a RecordId, table: &'static str) -> Result<&'a str, RowError> {
    match &id.key {
        RecordIdKey::String(key) if id.table.as_str() == table => Ok(key),
        _ => Err(bad_id(id, table)),
    }
}

fn bad_id(id: &RecordId, expected: &'static str) -> RowError {
    RowError::BadId {
        id: format!("{}:{:?}", id.table.as_str(), id.key),
        expected,
    }
}

fn array_key(table: &'static str, parts: Vec<Value>) -> RecordId {
    RecordId::new(table, Array::from(parts))
}

fn int(n: i64) -> Value {
    Value::Number(Number::Int(n))
}

/// `scan_event:[scan, seq]`.
pub fn scan_event_rid(scan: ScanId, seq: EventSeq) -> RecordId {
    array_key(
        "scan_event",
        vec![Value::RecordId(rid(&scan)), int(seq_i64(seq))],
    )
}

/// The `seq` of a `scan_event:[scan, seq]` key.
pub fn seq_of(id: &RecordId) -> Result<EventSeq, RowError> {
    if let RecordIdKey::Array(parts) = &id.key
        && let Some(Value::Number(Number::Int(seq))) = parts.get(1)
        && let Ok(seq) = u64::try_from(*seq)
    {
        return Ok(EventSeq(seq));
    }
    Err(bad_id(id, "scan_event"))
}

/// `member_of:[user, garden]`.
pub fn member_rid(user: UserId, garden: domain::id::GardenId) -> RecordId {
    array_key(
        "member_of",
        vec![Value::RecordId(rid(&user)), Value::RecordId(rid(&garden))],
    )
}

/// `idempotency_key:[user, "METHOD /path", key]`.
pub fn idempotency_rid(user: UserId, route: &str, key: IdempotencyKey) -> RecordId {
    array_key(
        "idempotency_key",
        vec![
            Value::RecordId(rid(&user)),
            Value::String(route.to_owned()),
            Value::String(key.to_string()),
        ],
    )
}

/// `usage_daily:[user, "YYYY-MM-DD"]` (UTC day).
pub fn usage_rid(user: UserId, day: NaiveDate) -> RecordId {
    array_key(
        "usage_daily",
        vec![
            Value::RecordId(rid(&user)),
            Value::String(day.format("%Y-%m-%d").to_string()),
        ],
    )
}

/// `indicator_answer:[scan, run, indicator_id]` (schema v1.4).
pub fn indicator_answer_rid(scan: ScanId, run: Run, indicator: &IndicatorId) -> RecordId {
    array_key(
        "indicator_answer",
        vec![
            Value::RecordId(rid(&scan)),
            int(i64::from(run.0)),
            Value::String(indicator.to_string()),
        ],
    )
}

/// `weather_cache:[hour, cell]`: the hour first, so cleanup is one key-range delete.
pub fn weather_rid(hour: DateTime<Utc>, cell: &str) -> RecordId {
    array_key(
        "weather_cache",
        vec![hour.into_value(), Value::String(cell.to_owned())],
    )
}

/// `deleted_user:[auth_id]`, the account tombstone (schema v1.4).
pub fn deleted_user_rid(auth_id: &domain::id::AuthId) -> RecordId {
    array_key("deleted_user", vec![Value::String(auth_id.to_string())])
}

/// `worker:[worker_id]`.
pub fn worker_rid(worker: &WorkerId) -> RecordId {
    array_key("worker", vec![Value::String(worker.to_string())])
}

/// `push_token:[token]`.
pub fn push_token_rid(token: &PushToken) -> RecordId {
    array_key("push_token", vec![Value::String(token.to_string())])
}

/// `message_feedback:[message]`.
pub fn message_feedback_rid(message: MessageId) -> RecordId {
    array_key("message_feedback", vec![Value::RecordId(rid(&message))])
}

/// `farm_record:[connector, vendor_id, n]` ([connectors 4]): an array key, because a Slack
/// `ts` holds a dot.
///
/// [connectors 4]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#4-data
pub fn farm_record_rid(id: &FarmRecordId) -> RecordId {
    array_key(
        "farm_record",
        vec![
            Value::RecordId(rid(&id.connector)),
            Value::String(id.vendor_id.to_string()),
            int(i64::from(id.n)),
        ],
    )
}

/// The [`FarmRecordId`] of a `farm_record:[connector, vendor_id, n]` key.
pub fn farm_record_id_of(id: &RecordId) -> Result<FarmRecordId, RowError> {
    if id.table.as_str() == "farm_record"
        && let RecordIdKey::Array(parts) = &id.key
        && let [
            Value::RecordId(connector),
            Value::String(vendor_id),
            Value::Number(Number::Int(n)),
        ] = parts.as_slice()
        && let Ok(n) = u32::try_from(*n)
        && let Ok(connector) = from_rid::<ConnectorId>(connector)
        && let Ok(record) = FarmRecordId::new(connector, vendor_id, n)
    {
        return Ok(record);
    }
    Err(bad_id(id, "farm_record"))
}

/// A `seq` as `SurrealDB`'s `int`. Sequences never reach `i64::MAX`.
pub fn seq_i64(seq: EventSeq) -> i64 {
    i64::try_from(seq.0).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::id::{GardenId, SpeciesId};

    #[test]
    fn ids_round_trip() {
        let g: GardenId = "01JAX3K9T6R2B8V0QZ4M7D1E5G".parse().unwrap();
        let r = rid(&g);
        assert_eq!(r.table.as_str(), "garden");
        assert_eq!(from_rid::<GardenId>(&r).unwrap(), g);
        assert!(from_rid::<ScanId>(&r).is_err(), "the table is checked");

        let s: SpeciesId = "tomato".parse().unwrap();
        assert_eq!(from_rid::<SpeciesId>(&rid(&s)).unwrap(), s);
    }

    #[test]
    fn array_keys_keep_their_parts() {
        let scan: ScanId = "01JAX3K9T6R2B8V0QZ4M7D1E5S".parse().unwrap();
        let ev = scan_event_rid(scan, EventSeq(7));
        assert_eq!(seq_of(&ev).unwrap(), EventSeq(7));
        assert!(seq_of(&rid(&scan)).is_err());

        let record: FarmRecordId = "01JAX3K9T6R2B8V0QZ4M7D1E5C.2.C024BE91L/1729000000.000100"
            .parse()
            .unwrap();
        let key = farm_record_rid(&record);
        assert_eq!(key.table.as_str(), "farm_record");
        assert_eq!(farm_record_id_of(&key).unwrap(), record);
        assert!(farm_record_id_of(&ev).is_err());
    }
}
