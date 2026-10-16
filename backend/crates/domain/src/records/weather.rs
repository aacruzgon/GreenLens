//! `WeatherSummary` ([contracts 7]), the weather cache cell and coordinates.

use std::fmt;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::wire::{LocalDate, Timestamp};

/// `WeatherSummary.provider`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WeatherProviderName {
    /// Open-Meteo.
    OpenMeteo,
}

/// `location_precision` of `WeatherSummary` and `Garden`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LocationPrecision {
    /// GPS with consent.
    Exact,
    /// GPS rounded with consent.
    #[serde(rename = "about_1_km")]
    About1Km,
    /// Rounded after consent was withdrawn.
    #[serde(rename = "about_10_km")]
    About10Km,
    /// Geocoded from a postal code.
    PostalCode,
    /// Geocoded from a city.
    City,
}

/// `WeatherSummary.current`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CurrentWeather {
    /// °C.
    pub temp_c: f64,
    /// Relative humidity, %.
    pub rh_pct: f64,
    /// km/h.
    pub wind_kmh: f64,
    /// mm over the last 24 hours.
    pub precip_mm_last_24h: f64,
}

/// `WeatherSummary.past_7_days`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PastWeather {
    /// mm.
    pub precip_mm: f64,
    /// %.
    pub mean_rh_pct: f64,
    /// Hours with RH over 90%: the leaf-wetness stand-in.
    pub hours_rh_over_90: u32,
    /// The longest run of consecutive humid days in the past 7, each a local day with at least
    /// `WeatherThresholds::humid_day_hours` hours over 90% (0–7), computed from the hourly
    /// history when the cell is fetched (`WeatherThresholds::longest_humid_run`), so a humid
    /// stretch in the past week is detectable ([contracts 9], crowding; architecture 6.8).
    /// `#[serde(default)]` reads weather cached or snapshotted before the field existed as 0;
    /// the wire always carries it.
    #[serde(default)]
    #[schema(required = true)]
    pub longest_humid_run_days: u8,
}

/// `WeatherSummary.forecast[]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ForecastDay {
    /// In the garden's time zone.
    pub date: LocalDate,
    /// °C.
    pub t_min_c: f64,
    /// °C.
    pub t_max_c: f64,
    /// mm.
    pub precip_mm: f64,
    /// %.
    pub mean_rh_pct: f64,
    /// Hours with RH over 90% (`WeatherThresholds::humid_hours`).
    pub hours_rh_over_90: u32,
    /// Frost expected: the 2 m minimum at or below `WeatherThresholds::frost_min_c` (0.5 °C by
    /// default), set when the cell is fetched (`domain::rules::weather`).
    pub frost_risk: bool,
}

/// `WeatherSummary.soil`: Open-Meteo's modelled soil conditions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SoilNow {
    /// m³/m³, 0–9 cm.
    pub moisture_0_9cm_m3m3: f64,
    /// m³/m³, 9–27 cm.
    pub moisture_9_27cm_m3m3: f64,
    /// °C at 6 cm.
    pub temp_6cm_c: f64,
    /// FAO reference evapotranspiration, mm over the last 24 hours.
    pub et0_mm_last_24h: f64,
}

/// `WeatherSummary` ([contracts 7]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct WeatherSummary {
    /// Always `open_meteo`.
    pub provider: WeatherProviderName,
    /// When the cell was fetched.
    pub fetched_at: Timestamp,
    /// The garden's location precision.
    pub location_precision: LocationPrecision,
    /// Now.
    pub current: CurrentWeather,
    /// The past 7 days.
    pub past_7_days: PastWeather,
    /// 7–14 days ahead.
    pub forecast: Vec<ForecastDay>,
    /// Soil now.
    pub soil: SoilNow,
}

/// What the weather cache holds for one cell and hour: a `WeatherSummary` without the garden's
/// `location_precision`, which differs between gardens that share a cell
/// ([contracts 7], fetching and caching).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellWeather {
    /// Always `open_meteo`.
    pub provider: WeatherProviderName,
    /// When the cell was fetched.
    pub fetched_at: Timestamp,
    /// Now.
    pub current: CurrentWeather,
    /// The past 7 days.
    pub past_7_days: PastWeather,
    /// 7–14 days ahead, dated in the cell's time zone.
    pub forecast: Vec<ForecastDay>,
    /// Soil now.
    pub soil: SoilNow,
}

impl CellWeather {
    /// The garden's view of its cell.
    pub fn for_garden(self, location_precision: LocationPrecision) -> WeatherSummary {
        WeatherSummary {
            provider: self.provider,
            fetched_at: self.fetched_at,
            location_precision,
            current: self.current,
            past_7_days: self.past_7_days,
            forecast: self.forecast,
            soil: self.soil,
        }
    }
}

/// WGS 84 coordinates. Raw coordinates are never returned to the app ([contracts 9]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Coordinates {
    /// Latitude, degrees, −90 to 90.
    pub lat: f64,
    /// Longitude, degrees, −180 to 180.
    pub lon: f64,
}

impl Coordinates {
    /// Checks the ranges.
    pub fn new(lat: f64, lon: f64) -> Option<Self> {
        ((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon))
            .then_some(Self { lat, lon })
    }

    /// Rounded to 0.1° (about 10 km), for `about_10_km` after consent is withdrawn.
    #[must_use]
    pub fn round_to_10_km(self) -> Self {
        Self {
            lat: (self.lat * 10.0).round() / 10.0,
            lon: (self.lon * 10.0).round() / 10.0,
        }
    }
}

/// A weather cache cell: coordinates rounded to 0.01° (about 1 km), stored on the garden as
/// `"lat,lon"` such as `"43.65,-79.38"` ([contracts 7], [database 3] `garden.weather_cell`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WeatherCell {
    lat_centi: i32,
    lon_centi: i32,
}

impl WeatherCell {
    /// The cell containing a point.
    pub fn of(at: Coordinates) -> Self {
        // reason: coordinates are within ±180°, so ×100 fits easily in i32.
        #[expect(clippy::cast_possible_truncation, reason = "|value| ≤ 18000")]
        let to_centi = |v: f64| (v * 100.0).round() as i32;
        Self {
            lat_centi: to_centi(at.lat),
            lon_centi: to_centi(at.lon),
        }
    }

    /// The cell's centre, sent to Open-Meteo.
    pub fn centre(self) -> Coordinates {
        Coordinates {
            lat: f64::from(self.lat_centi) / 100.0,
            lon: f64::from(self.lon_centi) / 100.0,
        }
    }

    /// Parses `"43.65,-79.38"`.
    pub fn parse(s: &str) -> Option<Self> {
        let (lat, lon) = s.split_once(',')?;
        let lat: f64 = lat.trim().parse().ok()?;
        let lon: f64 = lon.trim().parse().ok()?;
        Coordinates::new(lat, lon).map(Self::of)
    }
}

impl fmt::Display for WeatherCell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let c = self.centre();
        write!(f, "{:.2},{:.2}", c.lat, c.lon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_matches_the_stored_format() {
        let cell = WeatherCell::of(Coordinates::new(43.6532, -79.3832).unwrap());
        assert_eq!(cell.to_string(), "43.65,-79.38");
        assert_eq!(WeatherCell::parse("43.65,-79.38"), Some(cell));
    }

    #[test]
    fn precision_names() {
        assert_eq!(
            serde_json::to_string(&LocationPrecision::About1Km).unwrap(),
            "\"about_1_km\""
        );
    }
}
