//! Open-Meteo forecast: the [`WeatherProvider`] ([contracts 7], `WeatherSummary`, fetching and
//! caching; architecture 6.4 and 8.3).
//!
//! One request fills a whole [`CellWeather`]: hourly air, humidity, wind, rain, soil moisture
//! and temperature, ET₀ for the past 7 days and the forecast, plus daily extremes. Several
//! cells go in one request as comma-separated `latitude`, `longitude` and `timezone` lists;
//! Open-Meteo answers an array for two or more locations and a bare object for one.

mod summary;
mod wire;

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use domain::{
    ports::{CellRequest, CellResult, Clock, ProviderError, WeatherProvider},
    records::weather::CellWeather,
    rules::weather::WeatherThresholds,
};

pub use summary::{SummaryError, cell_weather, moisture_0_9cm};
pub use wire::{ApiError, Daily, Forecast, ForecastResponse, Hourly};

use crate::http::{self, TransportError};

/// The `upstream` of `503 upstream_unavailable` and every log line.
pub const PROVIDER: &str = "open_meteo";

/// Most cells in one request ([contracts 7]). 101 worked live; the URL length is the real
/// ceiling (1,001 cells returned 414).
pub const MAX_CELLS_PER_REQUEST: usize = 50;

/// Settings.
#[derive(Debug, Clone)]
pub struct OpenMeteoConfig {
    /// `https://api.open-meteo.com/v1/forecast`, or the commercial host.
    pub forecast_url: String,
    /// `apikey` for the commercial host; `None` on the free API (non-commercial use only).
    pub api_key: Option<secrecy::SecretString>,
    /// Total deadline of one request.
    pub timeout: Duration,
    /// Hourly history fetched: 7, for `past_7_days`.
    pub past_days: u8,
    /// Forecast days, today included: 7–16. 14 by default.
    pub forecast_days: u8,
    /// The weather thresholds: a forecast day is `frost_risk` when its 2 m minimum is frost
    /// under them. `api` passes the same instance its triggers and the pipeline read.
    pub thresholds: WeatherThresholds,
}

impl Default for OpenMeteoConfig {
    fn default() -> Self {
        Self {
            forecast_url: "https://api.open-meteo.com/v1/forecast".to_owned(),
            api_key: None,
            timeout: Duration::from_secs(20),
            past_days: 7,
            forecast_days: 14,
            thresholds: WeatherThresholds::default(),
        }
    }
}

/// The Open-Meteo adapter. `clock` stamps `fetched_at` and picks the current hour, so tests
/// fix time.
#[derive(Clone)]
pub struct OpenMeteo {
    http: reqwest::Client,
    config: Arc<OpenMeteoConfig>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for OpenMeteo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenMeteo")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl OpenMeteo {
    /// An adapter over the shared client.
    pub fn new(http: reqwest::Client, config: OpenMeteoConfig, clock: Arc<dyn Clock>) -> Self {
        Self {
            http,
            config: Arc::new(config),
            clock,
        }
    }

    /// One request for up to 50 cells; the forecasts in request order.
    async fn request(&self, cells: &[CellRequest]) -> Result<Vec<Forecast>, ProviderError> {
        let join =
            |f: &dyn Fn(&CellRequest) -> String| cells.iter().map(f).collect::<Vec<_>>().join(",");
        let latitude = join(&|c| format!("{:.2}", c.cell.centre().lat));
        let longitude = join(&|c| format!("{:.2}", c.cell.centre().lon));
        let timezone = join(&|c| c.time_zone.0.clone());
        let mut query: Vec<(&str, String)> = vec![
            ("latitude", latitude),
            ("longitude", longitude),
            ("timezone", timezone),
            ("hourly", wire::HOURLY_VARS.join(",")),
            ("daily", wire::DAILY_VARS.join(",")),
            ("past_days", self.config.past_days.to_string()),
            ("forecast_days", self.config.forecast_days.to_string()),
        ];
        if let Some(key) = &self.config.api_key {
            query.push((
                "apikey",
                secrecy::ExposeSecret::expose_secret(key).to_owned(),
            ));
        }
        let response = self
            .http
            .get(&self.config.forecast_url)
            .query(&query)
            .timeout(self.config.timeout)
            .send()
            .await
            .map_err(|e| unavailable(TransportError::from_reqwest(e)))?;
        let status = response.status();
        let retry_after = http::retry_after(response.headers());
        let bytes = response
            .bytes()
            .await
            .map_err(|e| unavailable(TransportError::from_reqwest(e)))?;
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimited {
                provider: PROVIDER,
                retry_after_s: retry_after.and_then(|d| u32::try_from(d.as_secs()).ok()),
            });
        }
        if !status.is_success() {
            // 400 carries `{"error":true,"reason":…}`: our request is wrong, not the service.
            let reason = serde_json::from_slice::<ApiError>(&bytes).map_or_else(
                |_| http::snippet(&String::from_utf8_lossy(&bytes), 256),
                |e| e.reason,
            );
            let source = format!("HTTP {status}: {reason}").into();
            return Err(if status.is_server_error() {
                ProviderError::Unavailable {
                    provider: PROVIDER,
                    source,
                }
            } else {
                ProviderError::BadResponse {
                    provider: PROVIDER,
                    source,
                }
            });
        }
        let forecasts = serde_json::from_slice::<ForecastResponse>(&bytes)
            .map_err(|e| bad(Box::new(e)))?
            .into_vec();
        if forecasts.len() != cells.len() {
            return Err(bad(format!(
                "asked for {} locations, got {}",
                cells.len(),
                forecasts.len()
            )
            .into()));
        }
        Ok(forecasts)
    }

    fn summarise(&self, forecast: &Forecast) -> Result<CellWeather, ProviderError> {
        cell_weather(
            forecast,
            self.clock.now(),
            usize::from(self.config.forecast_days),
            &self.config.thresholds,
        )
        .map_err(|e| bad(Box::new(e)))
    }
}

#[async_trait]
impl WeatherProvider for OpenMeteo {
    async fn fetch_batch(&self, cells: &[CellRequest]) -> Result<Vec<CellWeather>, ProviderError> {
        let mut out = Vec::with_capacity(cells.len());
        for chunk in cells.chunks(MAX_CELLS_PER_REQUEST) {
            for forecast in self.request(chunk).await? {
                out.push(self.summarise(&forecast)?);
            }
        }
        Ok(out)
    }

    async fn fetch_cell(&self, cell: &CellRequest) -> Result<CellWeather, ProviderError> {
        let forecasts = self.request(std::slice::from_ref(cell)).await?;
        let forecast = forecasts
            .first()
            .ok_or_else(|| bad("empty response".into()))?;
        self.summarise(forecast)
    }
}

/// The scheduler's rule ([contracts 7], fetching and caching): each batch of at most 50 cells
/// is one request; a failed batch is retried once, then its cells are fetched one by one, and a
/// cell that still fails is reported as failed (its gardens are skipped until the next run).
///
/// Takes `&dyn WeatherProvider`, so the rule is tested with `domain`'s mock and the scheduler
/// in `api` uses it with whatever adapter is wired.
pub async fn fetch_cells(provider: &dyn WeatherProvider, cells: &[CellRequest]) -> Vec<CellResult> {
    let mut out = Vec::with_capacity(cells.len());
    for chunk in cells.chunks(MAX_CELLS_PER_REQUEST) {
        let batch = match provider.fetch_batch(chunk).await {
            Ok(v) => Ok(v),
            Err(first) => {
                tracing::warn!(provider = PROVIDER, cells = chunk.len(), error = %first, "batch failed; retrying once");
                provider.fetch_batch(chunk).await
            }
        };
        match batch {
            Ok(weathers) if weathers.len() == chunk.len() => {
                out.extend(chunk.iter().zip(weathers).map(|(c, w)| CellResult {
                    cell: c.cell,
                    weather: Ok(w),
                }));
            }
            _ => {
                for c in chunk {
                    out.push(CellResult {
                        cell: c.cell,
                        weather: provider.fetch_cell(c).await,
                    });
                }
            }
        }
    }
    out
}

fn unavailable(e: TransportError) -> ProviderError {
    ProviderError::Unavailable {
        provider: PROVIDER,
        source: Box::new(e),
    }
}

fn bad(source: domain::ports::BoxError) -> ProviderError {
    ProviderError::BadResponse {
        provider: PROVIDER,
        source,
    }
}

#[cfg(test)]
mod tests;
