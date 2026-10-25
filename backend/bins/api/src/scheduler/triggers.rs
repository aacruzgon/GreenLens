//! The trigger rules shared by forecast alerts and plan triggers ([architecture 8.3]; one
//! vocabulary, contracts 7 `TriggerKind`). Pure: a cell's weather in, the triggers that fire and
//! their local dates out. The thresholds are `domain::rules::weather::WeatherThresholds`, the
//! one definition `integrations` (`frost_risk`) and the pipeline's fusion read too; `api` holds
//! the instance in `Config::weather`.

use domain::{
    records::{plan::TriggerKind, weather::CellWeather},
    rules::weather::WeatherThresholds,
    wire::LocalDate,
};

/// Days of forecast looked at, today included. A scheduling choice, not a weather threshold:
/// alerts are about the next 3 days.
pub const HORIZON_DAYS: usize = 3;

/// Every trigger that fires for a cell, with the local date it is about. The forecast's first day
/// is the garden's local today (Open-Meteo answers in the cell's time zone), so no time-zone
/// database is needed here. A humid stretch fires on the date it starts.
pub fn fired(weather: &CellWeather, t: &WeatherThresholds) -> Vec<(TriggerKind, LocalDate)> {
    let mut out = Vec::new();
    let days = &weather.forecast[..weather.forecast.len().min(HORIZON_DAYS)];
    for day in days {
        if t.is_frost_day(day) {
            out.push((TriggerKind::Frost, day.date));
        }
        if t.is_heat_day(day) {
            out.push((TriggerKind::Heat, day.date));
        }
        if t.is_heavy_rain_day(day) {
            out.push((TriggerKind::HeavyRain, day.date));
        }
    }
    for start in t.humid_stretch_starts(days) {
        out.push((TriggerKind::HumidStretch, days[start].date));
    }
    if let Some(today) = weather.forecast.first() {
        if t.is_after_rain(&weather.current) {
            out.push((TriggerKind::AfterRain, today.date));
        }
        if t.is_dry_soil(&weather.soil) {
            out.push((TriggerKind::DrySoil, today.date));
        }
    }
    out
}

/// A spell: the consecutive days on which one trigger fires, the unit an alert is created for
/// ([architecture 8.3], one alert per spell).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spell {
    /// The rule.
    pub trigger: TriggerKind,
    /// Its first fired day: the alert's date.
    pub from: LocalDate,
    /// Its last fired day in this forecast, the store's `spell_until`.
    pub until: LocalDate,
}

/// The spells of a list of fired triggers: each trigger's dates grouped into runs of
/// consecutive days, by trigger name and then date. A frost on three days is one spell, and so
/// are the overlapping starts of a long humid stretch.
pub fn spells(fired: &[(TriggerKind, LocalDate)]) -> Vec<Spell> {
    let mut sorted = fired.to_vec();
    sorted.sort_by_key(|&(trigger, date)| (trigger.as_str(), date));
    sorted.dedup();
    let mut out: Vec<Spell> = Vec::new();
    for (trigger, date) in sorted {
        match out.last_mut() {
            Some(spell) if spell.trigger == trigger && spell.until.plus_days(1) == Some(date) => {
                spell.until = date;
            }
            _ => out.push(Spell {
                trigger,
                from: date,
                until: date,
            }),
        }
    }
    out
}

/// The task text of a forecast alert.
pub const fn alert_text(trigger: TriggerKind) -> (&'static str, &'static str) {
    match trigger {
        TriggerKind::Frost => (
            "Frost expected",
            "Cover tender plants or bring pots inside tonight.",
        ),
        TriggerKind::Heat => (
            "Heat expected",
            "Water early in the morning and shade young plants.",
        ),
        TriggerKind::HumidStretch => (
            "Humid days ahead",
            "Fungal disease spreads in long humid spells. Check leaves and improve airflow.",
        ),
        TriggerKind::HeavyRain => (
            "Heavy rain expected",
            "Check drainage and stake tall plants.",
        ),
        TriggerKind::AfterRain => ("It rained", "Look for splashed soil on the lower leaves."),
        TriggerKind::DrySoil => ("The soil is drying out", "Water at the base of the plants."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::records::weather::{
        CurrentWeather, ForecastDay, PastWeather, SoilNow, WeatherProviderName,
    };

    fn day(date: &str, t_min: f64, rh_hours: u32) -> ForecastDay {
        ForecastDay {
            date: date.parse().unwrap(),
            t_min_c: t_min,
            t_max_c: 18.0,
            precip_mm: 0.0,
            mean_rh_pct: 80.0,
            hours_rh_over_90: rh_hours,
            frost_risk: false,
        }
    }

    #[test]
    fn frost_and_a_humid_stretch_fire_on_their_dates() {
        let weather = CellWeather {
            provider: WeatherProviderName::OpenMeteo,
            fetched_at: "2026-10-18T14:00:00Z".parse().unwrap(),
            current: CurrentWeather {
                temp_c: 8.0,
                rh_pct: 90.0,
                wind_kmh: 5.0,
                precip_mm_last_24h: 0.0,
            },
            past_7_days: PastWeather {
                precip_mm: 3.0,
                mean_rh_pct: 80.0,
                hours_rh_over_90: 20,
                longest_humid_run_days: 1,
            },
            forecast: vec![
                day("2026-10-18", 4.0, 12),
                day("2026-10-19", -1.0, 11),
                day("2026-10-20", 5.0, 2),
            ],
            soil: SoilNow {
                moisture_0_9cm_m3m3: 0.3,
                moisture_9_27cm_m3m3: 0.3,
                temp_6cm_c: 9.0,
                et0_mm_last_24h: 1.0,
            },
        };
        let got = fired(&weather, &WeatherThresholds::default());
        assert_eq!(
            got,
            vec![
                (TriggerKind::Frost, "2026-10-19".parse().unwrap()),
                (TriggerKind::HumidStretch, "2026-10-18".parse().unwrap()),
            ]
        );
    }

    #[test]
    fn consecutive_days_of_one_trigger_are_one_spell() {
        let d = |s: &str| -> LocalDate { s.parse().unwrap() };
        let fired = [
            (TriggerKind::Frost, d("2026-10-19")),
            (TriggerKind::HumidStretch, d("2026-10-18")),
            (TriggerKind::Frost, d("2026-10-18")),
            (TriggerKind::Frost, d("2026-10-20")),
            (TriggerKind::HumidStretch, d("2026-10-20")),
            (TriggerKind::HumidStretch, d("2026-10-20")),
        ];
        let spell = |trigger, from, until| Spell {
            trigger,
            from: d(from),
            until: d(until),
        };
        assert_eq!(
            spells(&fired),
            vec![
                spell(TriggerKind::Frost, "2026-10-18", "2026-10-20"),
                spell(TriggerKind::HumidStretch, "2026-10-18", "2026-10-18"),
                spell(TriggerKind::HumidStretch, "2026-10-20", "2026-10-20"),
            ]
        );
        assert_eq!(spells(&[]), Vec::new());
    }
}
