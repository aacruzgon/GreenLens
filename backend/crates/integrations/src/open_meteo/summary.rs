//! From Open-Meteo's series to [`CellWeather`]: a pure function over a borrowed response.

use chrono::{Duration as ChronoDuration, NaiveDate, NaiveDateTime, Timelike};
use domain::{
    records::weather::{
        CellWeather, CurrentWeather, ForecastDay, PastWeather, SoilNow, WeatherProviderName,
    },
    rules::weather::WeatherThresholds,
    wire::{LocalDate, Timestamp},
};

use super::wire::{EXPECTED_UNITS, Forecast};

const HOUR_FORMAT: &str = "%Y-%m-%dT%H:%M";

/// Why a response could not be summarised.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SummaryError {
    /// The current local hour isn't in the hourly series (wrong `past_days`, a stale clock).
    #[error("hour {0} not in the hourly series")]
    HourMissing(String),
    /// A value needed for `current` or `soil` is `null`.
    #[error("{0} is null at the current hour")]
    Null(&'static str),
    /// A variable came back in other units.
    #[error("{variable} in {got}, expected {expected}")]
    Units {
        /// The variable.
        variable: &'static str,
        /// What came back.
        got: String,
        /// What we need.
        expected: &'static str,
    },
    /// A date or hour that didn't parse.
    #[error("unparsable time {0}")]
    Time(String),
}

/// The 0–9 cm soil moisture contracts 7 asks for, which Open-Meteo doesn't have: the
/// depth-weighted mean of its 0–1, 1–3 and 3–9 cm layers, `(1·m₀₋₁ + 2·m₁₋₃ + 6·m₃₋₉) / 9`.
pub fn moisture_0_9cm(m0_1: f64, m1_3: f64, m3_9: f64) -> f64 {
    m0_1.mul_add(1.0, m1_3.mul_add(2.0, m3_9 * 6.0)) / 9.0
}

/// Builds a cell's weather at `now`.
///
/// The current hour is `now` shifted by the response's `utc_offset_seconds` and floored to the
/// hour, matched against the local `time` strings. `current` is that hour's row; the 24-hour
/// sums and the 7-day figures end with it (168 rows). `forecast` holds the daily rows from the
/// local date of `now`, at most `forecast_days`, each with its hours' mean humidity and hours
/// above 90 % ([`WeatherThresholds::humid_hours`]). `past_7_days.longest_humid_run_days` is
/// the longest run of humid days among the 7 whole local days before today
/// ([`WeatherThresholds::longest_humid_run`]). A day is `frost_risk` when its 2 m minimum
/// is frost under `thresholds` ([`WeatherThresholds::is_frost`]: at or below 0.5 °C by
/// default; a design choice, since Open-Meteo has no frost field).
pub fn cell_weather(
    forecast: &Forecast,
    now: Timestamp,
    forecast_days: usize,
    thresholds: &WeatherThresholds,
) -> Result<CellWeather, SummaryError> {
    for &(variable, expected) in EXPECTED_UNITS {
        if let Some(got) = forecast.hourly_units.get(variable)
            && got != expected
        {
            return Err(SummaryError::Units {
                variable,
                got: got.clone(),
                expected,
            });
        }
    }
    let h = &forecast.hourly;
    let local = now.into_inner().naive_utc()
        + ChronoDuration::seconds(i64::from(forecast.utc_offset_seconds));
    let local_hour = local
        .with_minute(0)
        .and_then(|t| t.with_second(0))
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(local);
    let key = local_hour.format(HOUR_FORMAT).to_string();
    let i = h
        .time
        .iter()
        .position(|t| *t == key)
        .ok_or(SummaryError::HourMissing(key))?;
    let last_24 = i.saturating_sub(23)..i + 1;
    let last_168 = i.saturating_sub(167)..i + 1;
    let need =
        |series: &[Option<f64>], name: &'static str| at(series, i).ok_or(SummaryError::Null(name));

    let today = local_hour.date();
    let mut days = Vec::new();
    for (d, date) in forecast.daily.time.iter().enumerate() {
        let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|_| SummaryError::Time(date.clone()))?;
        if date < today {
            continue;
        }
        if days.len() == forecast_days {
            break;
        }
        let (Some(t_min_c), Some(t_max_c)) = (
            at(&forecast.daily.temperature_2m_min, d),
            at(&forecast.daily.temperature_2m_max, d),
        ) else {
            break; // the model's range ended
        };
        let rh: Vec<f64> = h
            .time
            .iter()
            .enumerate()
            .filter(|(_, t)| hour_date(t) == Some(date))
            .filter_map(|(j, _)| at(&h.relative_humidity_2m, j))
            .collect();
        days.push(ForecastDay {
            date: LocalDate::new(date),
            t_min_c,
            t_max_c,
            precip_mm: at(&forecast.daily.precipitation_sum, d).unwrap_or(0.0),
            mean_rh_pct: mean(&rh),
            hours_rh_over_90: WeatherThresholds::humid_hours(rh.iter().copied()),
            frost_risk: thresholds.is_frost(t_min_c),
        });
    }

    let past_rh: Vec<f64> = last_168
        .clone()
        .filter_map(|j| at(&h.relative_humidity_2m, j))
        .collect();
    let daily_humid = past_daily_humid_hours(h, today);
    Ok(CellWeather {
        provider: WeatherProviderName::OpenMeteo,
        fetched_at: now,
        current: CurrentWeather {
            temp_c: need(&h.temperature_2m, "temperature_2m")?,
            rh_pct: need(&h.relative_humidity_2m, "relative_humidity_2m")?,
            wind_kmh: need(&h.wind_speed_10m, "wind_speed_10m")?,
            precip_mm_last_24h: sum(&h.precipitation, last_24.clone()),
        },
        past_7_days: PastWeather {
            precip_mm: sum(&h.precipitation, last_168),
            mean_rh_pct: mean(&past_rh),
            hours_rh_over_90: WeatherThresholds::humid_hours(past_rh.iter().copied()),
            longest_humid_run_days: thresholds.longest_humid_run(daily_humid),
        },
        forecast: days,
        soil: SoilNow {
            moisture_0_9cm_m3m3: moisture_0_9cm(
                need(&h.soil_moisture_0_to_1cm, "soil_moisture_0_to_1cm")?,
                need(&h.soil_moisture_1_to_3cm, "soil_moisture_1_to_3cm")?,
                need(&h.soil_moisture_3_to_9cm, "soil_moisture_3_to_9cm")?,
            ),
            moisture_9_27cm_m3m3: need(&h.soil_moisture_9_to_27cm, "soil_moisture_9_to_27cm")?,
            temp_6cm_c: need(&h.soil_temperature_6cm, "soil_temperature_6cm")?,
            et0_mm_last_24h: sum(&h.et0_fao_evapotranspiration, last_24),
        },
    })
}

/// The 7 whole local days before `today`, oldest first: each day's hours over 90 %, for the
/// longest humid run (a humid stretch in the past week, contracts 9 crowding).
fn past_daily_humid_hours(h: &super::wire::Hourly, today: NaiveDate) -> Vec<u32> {
    (1..=7_i64)
        .rev()
        .filter_map(|back| today.checked_sub_signed(ChronoDuration::days(back)))
        .map(|date| {
            WeatherThresholds::humid_hours(
                h.time
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| hour_date(t) == Some(date))
                    .filter_map(|(j, _)| at(&h.relative_humidity_2m, j)),
            )
        })
        .collect()
}

fn at(series: &[Option<f64>], i: usize) -> Option<f64> {
    series.get(i).copied().flatten()
}

fn sum(series: &[Option<f64>], range: std::ops::Range<usize>) -> f64 {
    range.filter_map(|i| at(series, i)).sum()
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    // reason: at most a few hundred hourly values, exact in f64.
    #[expect(clippy::cast_precision_loss, reason = "len ≤ 552")]
    let n = values.len() as f64;
    values.iter().sum::<f64>() / n
}

fn hour_date(t: &str) -> Option<NaiveDate> {
    NaiveDateTime::parse_from_str(t, HOUR_FORMAT)
        .ok()
        .map(|t| t.date())
}
