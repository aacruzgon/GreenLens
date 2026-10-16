//! How well the weather fits a problem's favourable conditions (`CandidateScore.weather_fit`;
//! [architecture 6.3], "the weather is checked against each profile's favourable conditions").
//!
//! Contracts 7 names the three values but not the arithmetic, so the rule is fixed here (see
//! the open questions in `docs/backend/knowledge.md`): each condition a profile states is
//! checked against the current weather, the past 7 days and the next [`FORECAST_DAYS`] days.
//! All stated conditions met is `favourable`, none met is `unfavourable`, anything between, or
//! a profile that states none, is `neutral`.
//!
//! **Thresholds.** Every number compared here is the profile's own (`Conditions`). The one
//! weather definition it relies on is the humid hour, RH over 90%
//! (`WeatherThresholds::HUMID_HOUR_RH_PCT`, what `hours_rh_over_90` counts); frost, the humid
//! day and the humid stretch are `domain::rules::weather::WeatherThresholds`, read by fusion and
//! the scheduler, not here.

use domain::{
    knowledge::Conditions,
    records::{diagnosis::WeatherFit, weather::WeatherSummary},
};

/// How many forecast days count, from today.
pub const FORECAST_DAYS: usize = 3;

/// One condition's result, for the fusion drivers' wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConditionCheck {
    /// Conditions the profile states (0–3).
    pub stated: u8,
    /// Of those, how many the weather meets.
    pub met: u8,
}

impl ConditionCheck {
    /// The fit this check gives.
    pub const fn fit(self) -> WeatherFit {
        if self.stated == 0 || (self.met > 0 && self.met < self.stated) {
            WeatherFit::Neutral
        } else if self.met == self.stated {
            WeatherFit::Favourable
        } else {
            WeatherFit::Unfavourable
        }
    }
}

/// Checks each stated condition:
/// - `temp_c`: the current temperature, or a forecast day's mean of `t_min_c` and `t_max_c`,
///   inside the range;
/// - `rh_pct_min`: the current humidity, the past 7 days' mean, or a forecast day's mean at or
///   above it;
/// - `humid_hours_min`: the past 7 days' hours over 90%, or the forecast days' sum, at or above
///   it (the leaf-wetness stand-in, [contracts 7], `WeatherSummary`).
pub fn check(conditions: &Conditions, weather: &WeatherSummary) -> ConditionCheck {
    let days = || weather.forecast.iter().take(FORECAST_DAYS);
    let mut stated = 0;
    let mut met = 0;
    if let Some(range) = conditions.temp_c {
        stated += 1;
        let inside = |t: f64| t >= range.min && t <= range.max;
        if inside(weather.current.temp_c)
            || days().any(|d| inside(f64::midpoint(d.t_min_c, d.t_max_c)))
        {
            met += 1;
        }
    }
    if let Some(rh) = conditions.rh_pct_min {
        stated += 1;
        if weather.current.rh_pct >= rh
            || weather.past_7_days.mean_rh_pct >= rh
            || days().any(|d| d.mean_rh_pct >= rh)
        {
            met += 1;
        }
    }
    if let Some(hours) = conditions.humid_hours_min {
        stated += 1;
        let ahead: u32 = days().map(|d| d.hours_rh_over_90).sum();
        if weather.past_7_days.hours_rh_over_90 >= hours || ahead >= hours {
            met += 1;
        }
    }
    ConditionCheck { stated, met }
}

/// The fit of `conditions` under `weather`; `neutral` without weather.
pub fn weather_fit(conditions: &Conditions, weather: Option<&WeatherSummary>) -> WeatherFit {
    weather.map_or(WeatherFit::Neutral, |w| check(conditions, w).fit())
}

/// The ranking order of a fit: favourable first.
pub const fn fit_rank(fit: WeatherFit) -> u8 {
    match fit {
        WeatherFit::Favourable => 0,
        WeatherFit::Neutral => 1,
        WeatherFit::Unfavourable => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use domain::knowledge::Range;

    #[test]
    fn early_blight_in_warm_humid_weather() {
        let blight = fixtures::problem("tomato_early_blight");
        let warm = fixtures::weather(26.0, 95.0, 40);
        assert_eq!(
            weather_fit(&blight.conditions, Some(&warm)),
            WeatherFit::Favourable
        );
        let cold_dry = fixtures::weather(8.0, 40.0, 0);
        assert_eq!(
            weather_fit(&blight.conditions, Some(&cold_dry)),
            WeatherFit::Unfavourable
        );
        let warm_dry = fixtures::weather(26.0, 40.0, 0);
        assert_eq!(
            weather_fit(&blight.conditions, Some(&warm_dry)),
            WeatherFit::Neutral
        );
        assert_eq!(weather_fit(&blight.conditions, None), WeatherFit::Neutral);
    }

    #[test]
    fn no_stated_conditions_is_neutral() {
        let none = Conditions::default();
        assert_eq!(
            weather_fit(&none, Some(&fixtures::weather(20.0, 50.0, 0))),
            WeatherFit::Neutral
        );
        let temp_only = Conditions {
            temp_c: Some(Range {
                min: 15.0,
                max: 25.0,
            }),
            ..Conditions::default()
        };
        assert_eq!(
            check(&temp_only, &fixtures::weather(20.0, 50.0, 0)),
            ConditionCheck { stated: 1, met: 1 }
        );
    }
}
