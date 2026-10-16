//! External data providers and push delivery: Open-Meteo, `SoilGrids`, the geocoder, Expo Push.

use crate::{
    garden::{gardens::TimeZone, push::PushMessage},
    id::PushToken,
    knowledge::SoilProfile,
    ports::ProviderError,
    records::weather::{CellWeather, Coordinates, WeatherCell},
};

/// One cell to fetch: its centre is sent to Open-Meteo, and the time zone dates the forecast's
/// `LocalDate`s. Gardens that share a cell share a time zone, since a cell is about 1 km.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellRequest {
    /// The cache cell.
    pub cell: WeatherCell,
    /// Its IANA time zone.
    pub time_zone: TimeZone,
}

/// One cell's result in a batch: a failed multi-location request is retried once, then its
/// cells are fetched one by one, and a cell that still fails skips its gardens until the next
/// run ([contracts 7], fetching and caching).
#[derive(Debug)]
pub struct CellResult {
    /// The cell.
    pub cell: WeatherCell,
    /// Its weather, or why not.
    pub weather: Result<CellWeather, ProviderError>,
}

/// Weather (architecture 6.4 and 8.3): Open-Meteo, cached per ~1 km cell and hour.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait WeatherProvider: Send + Sync {
    /// One multi-location request of at most 50 cells (`latitude` and `longitude` as
    /// comma-separated lists). Fails as a whole when the request fails; the scheduler then
    /// applies the retry rule above.
    async fn fetch_batch(&self, cells: &[CellRequest]) -> Result<Vec<CellWeather>, ProviderError>;

    /// One cell, for a scan's cache miss and the per-cell fallback.
    async fn fetch_cell(&self, cell: &CellRequest) -> Result<CellWeather, ProviderError>;
}

/// `SoilGrids` (architecture 6.4): fetched once per garden, in the background, 5 calls a minute
/// at most.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait SoilProvider: Send + Sync {
    /// The profile at a point, every depth band `SoilGrids` returns.
    async fn soil_profile(&self, at: Coordinates) -> Result<SoilProfile, ProviderError>;
}

/// What the geocoder is asked: coordinates (for the label and time zone), a postal code or a
/// city ([contracts 9], `GardenLocationInput`). Borrowed: the request owns the strings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GeocodeQuery<'a> {
    /// GPS with consent: reverse-geocode the label.
    Coordinates(Coordinates),
    /// A postal code, geocoded once.
    PostalCode(&'a str),
    /// A city, geocoded once.
    City(&'a str),
}

/// A geocoded location.
#[derive(Debug, Clone, PartialEq)]
pub struct Geocoded {
    /// The point stored as `garden.centroid`.
    pub coordinates: Coordinates,
    /// "Toronto, ON".
    pub label: String,
    /// IANA, sets every `LocalDate` of the garden.
    pub time_zone: TimeZone,
}

/// The geocoder (architecture 8.5): coordinates, a label and the IANA time zone.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait Geocoder: Send + Sync {
    /// Resolves the query; an unknown postal code or city is [`ProviderError::NotFound`].
    async fn geocode<'a>(&self, query: GeocodeQuery<'a>) -> Result<Geocoded, ProviderError>;
}

/// What a push batch did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PushReport {
    /// Messages Expo accepted.
    pub accepted: u32,
    /// Tokens Expo reported as `DeviceNotRegistered`: the API removes them.
    pub unregistered: Vec<PushToken>,
}

/// Push delivery through the Expo Push API ([contracts 9], push notifications). A push is a
/// hint, never the only copy, so a failure is logged and dropped.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait Notifier: Send + Sync {
    /// Sends the messages (iOS and Android tokens only), in Expo's batches of 100.
    async fn push(&self, messages: Vec<PushMessage>) -> Result<PushReport, ProviderError>;

    /// Checks the receipts of tickets sent at least 15 minutes ago (Expo's advice) and returns
    /// the tokens Expo reported as `DeviceNotRegistered`, to forget. The default does nothing,
    /// for adapters without receipts. Called by `api`'s scheduler.
    async fn check_receipts(&self) -> Result<Vec<PushToken>, ProviderError> {
        Ok(Vec::new())
    }
}
