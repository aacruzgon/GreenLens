//! The weather cache ([contracts 7], fetching and caching; [database 3]
//! `weather_cache:[hour, cell]`).

use crate::{
    garden::gardens::TimeZone,
    id::GardenId,
    ports::StoreError,
    records::weather::{CellWeather, WeatherCell},
    wire::Timestamp,
};

/// The gardens that share a cell, for the scheduler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellGardens {
    /// The cell.
    pub cell: WeatherCell,
    /// Its time zone.
    pub time_zone: TimeZone,
    /// Its gardens.
    pub gardens: Vec<GardenId>,
}

/// The `weather_cache` table and the garden grouping.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait WeatherCacheRepo: Send + Sync {
    /// The cell's weather for `hour` (truncated to the hour), if cached.
    async fn cached(
        &self,
        cell: WeatherCell,
        hour: Timestamp,
    ) -> Result<Option<CellWeather>, StoreError>;

    /// Caches a cell's weather; the first write of the hour wins.
    async fn put(
        &self,
        cell: WeatherCell,
        hour: Timestamp,
        weather: CellWeather,
    ) -> Result<(), StoreError>;

    /// Every live garden grouped by cell (index `garden_cell`).
    async fn gardens_by_cell(&self) -> Result<Vec<CellGardens>, StoreError>;

    /// Cleanup: one key-range delete of hours before `cutoff`.
    async fn delete_before(&self, cutoff: Timestamp) -> Result<u32, StoreError>;
}
