//! The weather thresholds: one definition of frost, heat, a humid day, a humid stretch, heavy
//! rain, after rain and dry soil, read by every crate that judges the weather.
//!
//! | Reader | What it reads |
//! |---|---|
//! | `integrations` (Open-Meteo summary) | `frost_risk` of each forecast day; hours over 90% |
//! | `api` scheduler (forecast alerts and plan triggers, [architecture 8.3]) | every trigger rule |
//! | `pipeline` fusion ([architecture 6.1]) | the humid stretch, frost and hard frost |
//! | `knowledge` weather fit ([architecture 6.3]) | the humid hour (its other numbers are the profiles') |
//!
//! The contracts fix the vocabulary (`frost_risk`, `hours_rh_over_90`, `TriggerKind`) but not
//! the numbers, so the defaults are the backend's proposal (domain.md, open questions). `api`
//! holds the one instance in its configuration and hands it to the others, so the weather
//! cache's `frost_risk` and the triggers read from it can't disagree.

use crate::records::weather::{CurrentWeather, ForecastDay, PastWeather, SoilNow};

/// The thresholds of every weather rule. `Copy`: it is a handful of numbers, passed by value
/// into tasks and borrowed in the pure rules below.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeatherThresholds {
    /// Frost: a day whose 2 m minimum is at or below this, °C. Above 0 °C because air at 2 m
    /// runs warmer than the ground on a clear night: ground frost forms with the air at 1–2 °C.
    /// Also `frost_risk` of each forecast day, set by `integrations` when it fetches.
    pub frost_min_c: f64,
    /// Hard frost: a frost day at or below this, °C, is `high` `frost_damage` in fusion
    /// (`medium` otherwise).
    pub frost_hard_c: f64,
    /// Heat: a day's maximum at or above this, °C.
    pub heat_max_c: f64,
    /// A humid day: at least this many hours with RH over
    /// [`HUMID_HOUR_RH_PCT`](Self::HUMID_HOUR_RH_PCT) (`hours_rh_over_90`).
    pub humid_day_hours: u32,
    /// A humid stretch: this many humid days in a row.
    pub humid_stretch_days: usize,
    /// Heavy rain: a day's precipitation at or above this, mm.
    pub heavy_rain_mm: f64,
    /// After rain: the last 24 hours' precipitation at or above this, mm.
    pub after_rain_mm: f64,
    /// Dry soil: 0–9 cm moisture below this, m³/m³, with the last day's ET₀ at or above
    /// [`dry_soil_et0_mm`](Self::dry_soil_et0_mm).
    pub dry_soil_m3m3: f64,
    /// See [`dry_soil_m3m3`](Self::dry_soil_m3m3), mm over 24 hours.
    pub dry_soil_et0_mm: f64,
}

impl Default for WeatherThresholds {
    fn default() -> Self {
        Self {
            frost_min_c: 0.5,
            frost_hard_c: -2.0,
            heat_max_c: 30.0,
            humid_day_hours: 10,
            humid_stretch_days: 2,
            heavy_rain_mm: 20.0,
            after_rain_mm: 5.0,
            dry_soil_m3m3: 0.15,
            dry_soil_et0_mm: 3.0,
        }
    }
}

impl WeatherThresholds {
    /// An hour is humid when its RH is over this, %. Fixed, not configurable: the contracts'
    /// field `hours_rh_over_90` names it.
    pub const HUMID_HOUR_RH_PCT: f64 = 90.0;

    /// Whether an hour's relative humidity counts toward `hours_rh_over_90` (strictly over 90%).
    pub fn is_humid_hour(rh_pct: f64) -> bool {
        rh_pct > Self::HUMID_HOUR_RH_PCT
    }

    /// `hours_rh_over_90` of a run of hourly readings.
    pub fn humid_hours(rh_pct: impl IntoIterator<Item = f64>) -> u32 {
        let n = rh_pct
            .into_iter()
            .filter(|&v| Self::is_humid_hour(v))
            .count();
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    /// Whether a 2 m minimum is frost: `frost_risk`.
    pub fn is_frost(&self, t_min_c: f64) -> bool {
        t_min_c <= self.frost_min_c
    }

    /// Whether a 2 m minimum is hard frost.
    pub fn is_hard_frost(&self, t_min_c: f64) -> bool {
        t_min_c <= self.frost_hard_c
    }

    /// A frost day: its stored `frost_risk` (set by this rule when fetched), or its minimum
    /// under this rule, so a cached cell fetched under other settings still counts.
    pub fn is_frost_day(&self, day: &ForecastDay) -> bool {
        day.frost_risk || self.is_frost(day.t_min_c)
    }

    /// A heat day.
    pub fn is_heat_day(&self, day: &ForecastDay) -> bool {
        day.t_max_c >= self.heat_max_c
    }

    /// A heavy-rain day.
    pub fn is_heavy_rain_day(&self, day: &ForecastDay) -> bool {
        day.precip_mm >= self.heavy_rain_mm
    }

    /// A humid day: at least `humid_day_hours` hours over 90%.
    pub fn is_humid_day(&self, day: &ForecastDay) -> bool {
        day.hours_rh_over_90 >= self.humid_day_hours
    }

    /// The index of each day that starts a humid stretch: `humid_stretch_days` humid days in a
    /// row from it. Overlapping stretches each count (three humid days give two starts).
    ///
    /// `'d` is the forecast's borrow. `use<'d>` keeps the iterator from also capturing `&self`'s
    /// lifetime, which edition 2024 would do by default (Edition Guide, RPIT lifetime capture
    /// rules): the thresholds are copied into the closure, so a caller can drop them while it
    /// still holds the iterator.
    pub fn humid_stretch_starts<'d>(
        &self,
        days: &'d [ForecastDay],
    ) -> impl Iterator<Item = usize> + use<'d> {
        let t = *self;
        days.windows(t.humid_stretch_days.max(1))
            .enumerate()
            .filter(move |(_, run)| run.iter().all(|d| t.is_humid_day(d)))
            .map(|(i, _)| i)
    }

    /// Whether the forecast holds a humid stretch.
    pub fn has_humid_stretch(&self, days: &[ForecastDay]) -> bool {
        self.humid_stretch_starts(days).next().is_some()
    }

    /// `past_7_days.longest_humid_run_days`: the longest run of consecutive humid days, from
    /// each past local day's count of hours over 90%, oldest first. Saturates at 255.
    pub fn longest_humid_run(&self, daily_humid_hours: impl IntoIterator<Item = u32>) -> u8 {
        let (mut best, mut run) = (0_u8, 0_u8);
        for hours in daily_humid_hours {
            run = if hours >= self.humid_day_hours {
                run.saturating_add(1)
            } else {
                0
            };
            best = best.max(run);
        }
        best
    }

    /// Whether the past week held a humid stretch.
    pub fn had_humid_stretch(&self, past: &PastWeather) -> bool {
        usize::from(past.longest_humid_run_days) >= self.humid_stretch_days.max(1)
    }

    /// A humid stretch forecast or in the past 7 days: the fungal-spread rule of crowding and
    /// fusion ([contracts 9], crowding; architecture 6.8).
    pub fn humid_stretch_now_or_past(&self, past: &PastWeather, forecast: &[ForecastDay]) -> bool {
        self.had_humid_stretch(past) || self.has_humid_stretch(forecast)
    }

    /// After rain: the last 24 hours' precipitation.
    pub fn is_after_rain(&self, current: &CurrentWeather) -> bool {
        current.precip_mm_last_24h >= self.after_rain_mm
    }

    /// Dry soil: low 0–9 cm moisture while the air is drawing water out.
    pub fn is_dry_soil(&self, soil: &SoilNow) -> bool {
        soil.moisture_0_9cm_m3m3 < self.dry_soil_m3m3
            && soil.et0_mm_last_24h >= self.dry_soil_et0_mm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(t_min: f64, humid_hours: u32) -> ForecastDay {
        ForecastDay {
            date: "2026-10-18".parse().unwrap(),
            t_min_c: t_min,
            t_max_c: 18.0,
            precip_mm: 0.0,
            mean_rh_pct: 80.0,
            hours_rh_over_90: humid_hours,
            frost_risk: false,
        }
    }

    /// The defaults are a decision recorded in domain.md; changing one is a design change, not
    /// a refactor, so this test pins every number.
    #[test]
    fn defaults_are_pinned() {
        let t = WeatherThresholds::default();
        assert_eq!(
            t,
            WeatherThresholds {
                frost_min_c: 0.5,
                frost_hard_c: -2.0,
                heat_max_c: 30.0,
                humid_day_hours: 10,
                humid_stretch_days: 2,
                heavy_rain_mm: 20.0,
                after_rain_mm: 5.0,
                dry_soil_m3m3: 0.15,
                dry_soil_et0_mm: 3.0,
            }
        );
        assert!((WeatherThresholds::HUMID_HOUR_RH_PCT - 90.0).abs() < f64::EPSILON);
    }

    #[test]
    fn frost_includes_its_threshold_and_hard_frost_is_colder() {
        let t = WeatherThresholds::default();
        assert!(t.is_frost(0.5) && t.is_frost(-0.0) && !t.is_frost(0.6));
        assert!(t.is_hard_frost(-2.0) && !t.is_hard_frost(-1.9));
        let mut warm = day(4.0, 0);
        assert!(!t.is_frost_day(&warm));
        warm.frost_risk = true;
        assert!(t.is_frost_day(&warm), "the stored flag counts");
    }

    #[test]
    fn humid_hours_are_strictly_over_90() {
        assert_eq!(WeatherThresholds::humid_hours([90.0, 90.1, 95.0, 40.0]), 2);
    }

    #[test]
    fn the_past_week_counts_its_longest_humid_run() {
        let t = WeatherThresholds::default();
        assert_eq!(t.longest_humid_run([12, 3, 10, 11, 9, 14, 0]), 2);
        assert_eq!(t.longest_humid_run([]), 0);
        let past = |days| PastWeather {
            precip_mm: 0.0,
            mean_rh_pct: 80.0,
            hours_rh_over_90: 30,
            longest_humid_run_days: days,
        };
        assert!(!t.had_humid_stretch(&past(1)));
        assert!(t.had_humid_stretch(&past(2)));
        let dry_forecast = [day(5.0, 0), day(5.0, 0)];
        assert!(
            t.humid_stretch_now_or_past(&past(2), &dry_forecast),
            "a past stretch counts"
        );
        assert!(!t.humid_stretch_now_or_past(&past(1), &dry_forecast));
    }

    #[test]
    fn a_humid_stretch_is_two_humid_days_in_a_row() {
        let t = WeatherThresholds::default();
        let apart = [day(5.0, 12), day(5.0, 3), day(5.0, 12)];
        assert!(!t.has_humid_stretch(&apart));
        let run = [day(5.0, 9), day(5.0, 10), day(5.0, 14), day(5.0, 11)];
        assert_eq!(t.humid_stretch_starts(&run).collect::<Vec<_>>(), vec![1, 2]);
    }
}
