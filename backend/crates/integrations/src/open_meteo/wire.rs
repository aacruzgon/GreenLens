//! Open-Meteo's response, as observed live on 2026-10-15 (`fixtures/openmeteo_forecast_multi.json`).

use std::collections::BTreeMap;

use serde::Deserialize;

/// Hourly variables: 14, so a request weighs more than one API call on the fair-use counter.
pub const HOURLY_VARS: &[&str] = &[
    "temperature_2m",
    "relative_humidity_2m",
    "wind_speed_10m",
    "precipitation",
    "soil_moisture_0_to_1cm",
    "soil_moisture_1_to_3cm",
    "soil_moisture_3_to_9cm",
    "soil_moisture_9_to_27cm",
    "soil_moisture_27_to_81cm",
    "soil_temperature_0cm",
    "soil_temperature_6cm",
    "soil_temperature_18cm",
    "soil_temperature_54cm",
    "et0_fao_evapotranspiration",
];

/// Daily variables.
pub const DAILY_VARS: &[&str] = &[
    "temperature_2m_min",
    "temperature_2m_max",
    "precipitation_sum",
];

/// The units `cell_weather` relies on; a response with others is a bad response.
pub(crate) const EXPECTED_UNITS: &[(&str, &str)] = &[
    ("temperature_2m", "°C"),
    ("relative_humidity_2m", "%"),
    ("wind_speed_10m", "km/h"),
    ("precipitation", "mm"),
    ("soil_moisture_0_to_1cm", "m³/m³"),
    ("soil_moisture_9_to_27cm", "m³/m³"),
    ("soil_temperature_6cm", "°C"),
    ("et0_fao_evapotranspiration", "mm"),
];

/// One location.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Forecast {
    /// Snapped to the model grid (43.65 → 43.646603).
    pub latitude: f64,
    /// Snapped to the model grid.
    pub longitude: f64,
    /// The offset at request time, DST-aware: −14,400 for Toronto in October.
    pub utc_offset_seconds: i32,
    /// The IANA zone the times are in.
    pub timezone: String,
    /// Units per hourly variable.
    #[serde(default)]
    pub hourly_units: BTreeMap<String, String>,
    /// Hourly series; `time` is local and naive (`2026-10-15T19:00`).
    #[serde(default)]
    pub hourly: Hourly,
    /// Daily series; `time` is a local date.
    #[serde(default)]
    pub daily: Daily,
}

/// Hourly series, one field per Open-Meteo variable. A value is `null` outside a model's range,
/// hence `Option`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct Hourly {
    pub time: Vec<String>,
    #[serde(default)]
    pub temperature_2m: Vec<Option<f64>>,
    #[serde(default)]
    pub relative_humidity_2m: Vec<Option<f64>>,
    #[serde(default)]
    pub wind_speed_10m: Vec<Option<f64>>,
    #[serde(default)]
    pub precipitation: Vec<Option<f64>>,
    #[serde(default)]
    pub soil_moisture_0_to_1cm: Vec<Option<f64>>,
    #[serde(default)]
    pub soil_moisture_1_to_3cm: Vec<Option<f64>>,
    #[serde(default)]
    pub soil_moisture_3_to_9cm: Vec<Option<f64>>,
    #[serde(default)]
    pub soil_moisture_9_to_27cm: Vec<Option<f64>>,
    #[serde(default)]
    pub soil_moisture_27_to_81cm: Vec<Option<f64>>,
    #[serde(default)]
    pub soil_temperature_6cm: Vec<Option<f64>>,
    #[serde(default)]
    pub et0_fao_evapotranspiration: Vec<Option<f64>>,
}

/// Daily series, one field per Open-Meteo variable.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct Daily {
    pub time: Vec<String>,
    #[serde(default)]
    pub temperature_2m_min: Vec<Option<f64>>,
    #[serde(default)]
    pub temperature_2m_max: Vec<Option<f64>>,
    #[serde(default)]
    pub precipitation_sum: Vec<Option<f64>>,
}

/// An object for one location, an array for several.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum ForecastResponse {
    /// Two or more locations, in request order.
    Many(Vec<Forecast>),
    /// One location.
    One(Box<Forecast>),
}

impl ForecastResponse {
    /// The forecasts in request order.
    pub fn into_vec(self) -> Vec<Forecast> {
        match self {
            Self::Many(v) => v,
            Self::One(f) => vec![*f],
        }
    }
}

/// The 400 body: `{"error":true,"reason":"Forecast days is invalid. …"}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ApiError {
    /// The reason.
    pub reason: String,
}
