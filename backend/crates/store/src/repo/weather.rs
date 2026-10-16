//! The weather cache ([contracts 7], fetching and caching; [database 4], cleanup ✓
//! `weather_cache:[hour, cell]`). The cache holds the cell's weather without
//! `location_precision`; `api` adds the garden's (contracts v0.9; domain `CellWeather`).

use chrono::{DurationRound, TimeDelta};
use domain::{
    garden::gardens::TimeZone,
    id::GardenId,
    ports::{CellGardens, StoreError, WeatherCacheRepo},
    records::weather::{CellWeather, WeatherCell},
    wire::Timestamp,
};
use surrealdb::types::{RecordId, SurrealValue};

use crate::{
    SurrealStore,
    error::{DbFailure, RowError, take_failed},
    ids::{from_rid, weather_rid},
    rows::{dt, from_json, to_json},
    tx::run_tx,
};

/// The statements.
pub mod sql {
    /// The cell's weather for an hour.
    pub const CACHED: &str = "SELECT VALUE summary FROM ONLY $key;";
    /// Caches a cell's weather; the first write of the hour wins (a second `CREATE` fails with
    /// "already exists", which is success here).
    pub const PUT: &str = "CREATE $key SET summary = $summary;";
    /// Every live garden grouped by cell (index `garden_cell`), for the scheduler and the
    /// orphan sweep's garden list.
    pub const GARDENS_BY_CELL: &str =
        "SELECT weather_cell, time_zone, array::group(id) AS gardens FROM garden
WHERE deleting_at = NONE GROUP BY weather_cell, time_zone;";
    /// Cleanup ([database 4] ✓): one key-range delete, because the hour comes first in the key.
    pub const DELETE_BEFORE: &str =
        "DELETE weather_cache:[NONE, NONE]..[$cutoff, NONE] RETURN VALUE $before.id;";
}

/// One group of [`sql::GARDENS_BY_CELL`].
#[derive(Debug, Clone, SurrealValue)]
pub struct CellGroupRow {
    /// `"43.65,-79.38"`.
    pub weather_cell: String,
    /// IANA time zone.
    pub time_zone: String,
    /// The cell's gardens.
    pub gardens: Vec<RecordId>,
}

/// The hour a timestamp falls in, as the key's first part.
fn hour_of(t: Timestamp) -> chrono::DateTime<chrono::Utc> {
    let t = dt(t);
    t.duration_trunc(TimeDelta::hours(1)).unwrap_or(t)
}

#[async_trait::async_trait]
impl WeatherCacheRepo for SurrealStore {
    async fn cached(
        &self,
        cell: WeatherCell,
        hour: Timestamp,
    ) -> Result<Option<CellWeather>, StoreError> {
        const WHAT: &str = "cached weather";
        let key = weather_rid(hour_of(hour), &cell.to_string());
        let mut results = run_tx(&self.db, sql::CACHED, |q| q.bind(("key", key.clone())))
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let summary: Option<serde_json::Value> =
            results.take(0).map_err(|e| take_failed(WHAT, e))?;
        summary
            .map(|s| from_json("summary", s))
            .transpose()
            .map_err(|e| e.into_store(WHAT))
    }

    async fn put(
        &self,
        cell: WeatherCell,
        hour: Timestamp,
        weather: CellWeather,
    ) -> Result<(), StoreError> {
        const WHAT: &str = "cache weather";
        let key = weather_rid(hour_of(hour), &cell.to_string());
        let summary = to_json("summary", &weather).map_err(|e| e.into_store(WHAT))?;
        match run_tx(&self.db, sql::PUT, |q| {
            q.bind(("key", key.clone()))
                .bind(("summary", summary.clone()))
        })
        .await
        {
            Ok(_) | Err(DbFailure::AlreadyExists(_)) => Ok(()),
            Err(failure) => Err(failure.into_store(WHAT)),
        }
    }

    async fn gardens_by_cell(&self) -> Result<Vec<CellGardens>, StoreError> {
        const WHAT: &str = "gardens by cell";
        let mut results = run_tx(&self.db, sql::GARDENS_BY_CELL, |q| q)
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<CellGroupRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .map(|row| {
                Ok(CellGardens {
                    cell: WeatherCell::parse(&row.weather_cell).ok_or(RowError::BadField {
                        field: "weather_cell",
                        problem: row.weather_cell.clone(),
                    })?,
                    time_zone: TimeZone(row.time_zone),
                    gardens: row
                        .gardens
                        .iter()
                        .map(from_rid::<GardenId>)
                        .collect::<Result<_, _>>()?,
                })
            })
            .collect::<Result<_, RowError>>()
            .map_err(|e| e.into_store(WHAT))
    }

    async fn delete_before(&self, cutoff: Timestamp) -> Result<u32, StoreError> {
        const WHAT: &str = "delete old weather";
        let mut results = run_tx(&self.db, sql::DELETE_BEFORE, |q| {
            q.bind(("cutoff", dt(cutoff)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let ids: Vec<RecordId> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        Ok(u32::try_from(ids.len()).unwrap_or(u32::MAX))
    }
}
