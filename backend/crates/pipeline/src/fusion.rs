//! Fusion ([architecture 6.1], fusion; [contracts 7], `Diagnosis.risks` and
//! `evidence_basis`): risk scores from rules over the verified findings, the weather, the soil
//! and the plant's history. Disease thresholds come from the problem profiles, never from
//! code; weather thresholds (the humid stretch, frost, hard frost) from
//! `domain::rules::weather::WeatherThresholds`, the one definition the scheduler's triggers and
//! Open-Meteo's `frost_risk` read too; the fixed constants below are the contracts' own (growth
//! noise, crowding factor).
//!
//! **Lifetimes.** `'a` in [`FusionInput<'a>`] borrows everything the step read; the risks are
//! owned, because they are stored in the diagnosis.

use domain::{
    events::ProblemKind,
    garden::{map::Pin, plants::MeasurementPoint},
    id::PlantId,
    knowledge::ProblemSeed,
    records::{
        diagnosis::{Risk, RiskLevel},
        observation::Observation,
        weather::WeatherSummary,
    },
    rules::weather::WeatherThresholds,
    url::Stored,
    vlm::SoilAspect,
    wire::{ConfidenceBand, Timestamp},
};

/// The days of forecast fusion looks at.
pub const FORECAST_DAYS: usize = 7;
/// The growth noise floor ([contracts 7], growth): 2 cm or 5%, whichever is larger.
pub const GROWTH_NOISE_CM: f64 = 2.0;
/// The relative part of the noise floor.
pub const GROWTH_NOISE_SHARE: f64 = 0.05;
/// The latest height must be from the past 7 days.
pub const GROWTH_RECENT_DAYS: u64 = 7;
/// The comparison height must be at least 14 days older.
pub const GROWTH_GAP_DAYS: u64 = 14;

/// A humid stretch ([architecture 6.8], spacing; [contracts 9], crowding) forecast in the next
/// [`FORECAST_DAYS`] days or in the past 7: [`WeatherThresholds::humid_stretch_now_or_past`],
/// two humid days in a row, a humid day being 10 or more hours over 90% by default. The past
/// week comes from `past_7_days.longest_humid_run_days`. Returns the driver text, naming the
/// forecast when it holds the stretch. The scheduler's `humid_stretch` trigger reads the
/// forecast rule alone, over its own 3-day horizon.
pub fn humid_stretch(weather: &WeatherSummary, thresholds: &WeatherThresholds) -> Option<String> {
    let days = &weather.forecast[..weather.forecast.len().min(FORECAST_DAYS)];
    if thresholds.has_humid_stretch(days) {
        let n = days.iter().filter(|d| thresholds.is_humid_day(d)).count();
        return Some(format!("{n} humid days in the forecast"));
    }
    thresholds.had_humid_stretch(&weather.past_7_days).then(|| {
        format!(
            "{} humid days in a row in the past week",
            weather.past_7_days.longest_humid_run_days
        )
    })
}

/// Forecast days that meet a profile's favourable conditions: the day's mean temperature in
/// `temp_c`, its mean humidity at or above `rh_pct_min`, and, when `humid_hours_min` is set,
/// at least a seventh of it in hours over 90% that day. A profile with no conditions has none.
pub fn favourable_days(profile: &ProblemSeed, weather: &WeatherSummary) -> usize {
    let c = &profile.conditions;
    if c.temp_c.is_none() && c.rh_pct_min.is_none() && c.humid_hours_min.is_none() {
        return 0;
    }
    weather
        .forecast
        .iter()
        .take(FORECAST_DAYS)
        .filter(|d| {
            let t = f64::midpoint(d.t_min_c, d.t_max_c);
            c.temp_c.is_none_or(|r| t >= r.min && t <= r.max)
                && c.rh_pct_min.is_none_or(|rh| d.mean_rh_pct >= rh)
                && c.humid_hours_min
                    .is_none_or(|h| d.hours_rh_over_90 * 7 >= h)
        })
        .count()
}

/// Everything fusion reads.
#[derive(Debug, Clone, Copy)]
pub struct FusionInput<'a> {
    /// The problems the diagnosis points to: the primary, or every remaining candidate of an
    /// `uncertain` diagnosis; empty when healthy.
    pub problems: &'a [&'a ProblemSeed],
    /// The run's weather.
    pub weather: Option<&'a WeatherSummary>,
    /// The soil observations the diagnosis uses (VLM and guided checks).
    pub soil: &'a [Observation<Stored>],
    /// The plant's height series, oldest first.
    pub heights: &'a [MeasurementPoint],
    /// The species' `mature_height_cm`.
    pub mature_height_cm: Option<f64>,
    /// The species' `spacing_cm`.
    pub spacing_cm: Option<f64>,
    /// The plant and the current map's pins, for crowding.
    pub plant: PlantId,
    /// The current map's pins; empty without a map.
    pub pins: &'a [Pin],
    /// Now.
    pub now: Timestamp,
    /// The weather thresholds (`PipelineConfig::weather`).
    pub thresholds: WeatherThresholds,
}

fn level_for_days(days: usize) -> Option<RiskLevel> {
    match days {
        0 => None,
        1 => Some(RiskLevel::Low),
        2 | 3 => Some(RiskLevel::Medium),
        _ => Some(RiskLevel::High),
    }
}

/// `fungal_spread` from the profiles' thresholds, then the crowding rule ([contracts 9],
/// crowding): crowded and a humid stretch raises it one step, or adds it at `low`.
fn fungal_spread(input: &FusionInput<'_>) -> Option<Risk> {
    let weather = input.weather?;
    let mut level: Option<RiskLevel> = None;
    let mut drivers = Vec::new();
    for p in input
        .problems
        .iter()
        .filter(|p| matches!(p.kind, ProblemKind::Fungal | ProblemKind::Bacterial))
    {
        let days = favourable_days(p, weather);
        if let Some(l) = level_for_days(days) {
            if level.is_none_or(|cur| l > cur) {
                level = Some(l);
            }
            drivers.push(format!("{days} days favouring {} in the forecast", p.name));
        }
    }
    let stretch = humid_stretch(weather, &input.thresholds);
    if let (Some(spacing), Some(stretch_text)) = (input.spacing_cm, stretch.as_ref())
        && let Some(nearest) = domain::garden::map::crowded(input.plant, input.pins, spacing)
    {
        drivers.push(format!(
            "Crowded: nearest plant {:.0} cm away, recommended spacing {spacing:.0} cm",
            nearest * 100.0
        ));
        if level.is_none() {
            drivers.push(stretch_text.clone());
        }
        level = Some(level.map_or(RiskLevel::Low, RiskLevel::raised));
    }
    level.map(|level| Risk {
        risk: Risk::FUNGAL_SPREAD.to_owned(),
        level,
        drivers,
    })
}

/// `frost_damage`: a frost day in the next 3 days ([`WeatherThresholds::is_frost_day`], a 2 m
/// minimum at or below 0.5 °C by default); `high` at hard frost
/// ([`WeatherThresholds::is_hard_frost`], −2 °C or colder by default), `medium` otherwise.
fn frost(weather: Option<&WeatherSummary>, thresholds: &WeatherThresholds) -> Option<Risk> {
    let day = weather?
        .forecast
        .iter()
        .take(3)
        .find(|d| thresholds.is_frost_day(d))?;
    Some(Risk {
        risk: Risk::FROST_DAMAGE.to_owned(),
        level: if thresholds.is_hard_frost(day.t_min_c) {
            RiskLevel::High
        } else {
            RiskLevel::Medium
        },
        drivers: vec![format!(
            "Frost forecast on {} (low {:.0} °C)",
            day.date, day.t_min_c
        )],
    })
}

/// `root_rot`: the photo shows waterlogged soil (medium confidence or more), raised a step
/// when the modelled moisture in the top 9 cm is also at least 0.45 m³/m³. Photo first, then
/// modelled data (architecture 6.4, evidence priority).
fn root_rot(soil: &[Observation<Stored>], weather: Option<&WeatherSummary>) -> Option<Risk> {
    let wet = soil.iter().find(|o| {
        o.confidence.confidence().at_least_medium()
            && o.detail.as_ref().is_some_and(|d| {
                d.aspect == Some(SoilAspect::Moisture) && d.value.as_deref() == Some("waterlogged")
            })
    })?;
    let mut level = RiskLevel::Low;
    let mut drivers = vec![format!("Photo: {}", wet.label)];
    if let Some(w) = weather
        && w.soil.moisture_0_9cm_m3m3 >= 0.45
    {
        level = level.raised();
        drivers.push(format!(
            "Modelled soil moisture {:.2} m³/m³ in the top 9 cm",
            w.soil.moisture_0_9cm_m3m3
        ));
    }
    Some(Risk {
        risk: Risk::ROOT_ROT.to_owned(),
        level,
        drivers,
    })
}

/// `growth_stalled` ([contracts 7], growth): the species has a mature height; the latest
/// height is from the past 7 days and below it; and the most recent height at least 14 days
/// before it differs by less than 2 cm or 5% of the earlier height, whichever is larger. Only
/// medium and high points count.
pub fn growth_stalled(
    heights: &[MeasurementPoint],
    mature_height_cm: Option<f64>,
    now: Timestamp,
) -> Option<Risk> {
    let mature = mature_height_cm?;
    let usable: Vec<&MeasurementPoint> = heights
        .iter()
        .filter(|p| p.confidence_band >= ConfidenceBand::Medium)
        .collect();
    let latest = usable.iter().max_by_key(|p| p.created_at)?;
    let day = std::time::Duration::from_hours(24);
    if now.since(latest.created_at) > day * u32::try_from(GROWTH_RECENT_DAYS).unwrap_or(7)
        || latest.value >= mature
    {
        return None;
    }
    let cutoff = latest
        .created_at
        .minus(day * u32::try_from(GROWTH_GAP_DAYS).unwrap_or(14));
    let earlier = usable
        .iter()
        .filter(|p| p.created_at <= cutoff)
        .max_by_key(|p| p.created_at)?;
    let noise = GROWTH_NOISE_CM.max(earlier.value * GROWTH_NOISE_SHARE);
    ((latest.value - earlier.value).abs() < noise).then(|| Risk {
        risk: Risk::GROWTH_STALLED.to_owned(),
        level: RiskLevel::Low,
        drivers: vec![format!(
            "Height {:.0} cm, {:.0} cm two weeks earlier; mature height {mature:.0} cm",
            latest.value, earlier.value
        )],
    })
}

/// Every risk, in a fixed order.
pub fn risks(input: &FusionInput<'_>) -> Vec<Risk> {
    [
        fungal_spread(input),
        root_rot(input.soil, input.weather),
        frost(input.weather, &input.thresholds),
        growth_stalled(input.heights, input.mature_height_cm, input.now),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::garden::plants::GrowthQuantity;

    fn weather(rh: f64, frost: bool) -> WeatherSummary {
        serde_json::from_value(serde_json::json!({
            "provider": "open_meteo", "fetched_at": "2026-10-18T14:00:00Z", "location_precision": "about_1_km",
            "current": {"temp_c": 20, "rh_pct": rh, "wind_kmh": 3, "precip_mm_last_24h": 0},
            "past_7_days": {"precip_mm": 3, "mean_rh_pct": rh, "hours_rh_over_90": 0},
            "forecast": (0..4).map(|i| serde_json::json!({
                "date": format!("2026-10-{}", 18 + i), "t_min_c": if frost { -3 } else { 16 }, "t_max_c": 24,
                "precip_mm": 0, "mean_rh_pct": rh, "hours_rh_over_90": if rh >= 90.0 { 14 } else { 0 }, "frost_risk": frost
            })).collect::<Vec<_>>(),
            "soil": {"moisture_0_9cm_m3m3": 0.2, "moisture_9_27cm_m3m3": 0.2, "temp_6cm_c": 12, "et0_mm_last_24h": 1}
        }))
        .unwrap()
    }

    fn point(days_ago: u64, cm: f64, now: Timestamp) -> MeasurementPoint {
        MeasurementPoint {
            scan_id: "01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().unwrap(),
            created_at: now.minus(std::time::Duration::from_hours(24 * days_ago)),
            quantity: GrowthQuantity::PlantHeightCm,
            value: cm,
            confidence_band: ConfidenceBand::High,
        }
    }

    #[test]
    fn humid_stretch_needs_two_days_in_a_row() {
        let t = WeatherThresholds::default();
        assert_eq!(
            humid_stretch(&weather(95.0, false), &t).as_deref(),
            Some("4 humid days in the forecast")
        );
        assert!(humid_stretch(&weather(60.0, false), &t).is_none());
        let mut apart = weather(95.0, false);
        apart.forecast[1].hours_rh_over_90 = 9;
        apart.forecast[3].hours_rh_over_90 = 9;
        assert!(
            humid_stretch(&apart, &t).is_none(),
            "humid days apart are no stretch"
        );
    }

    #[test]
    fn a_humid_stretch_in_the_past_week_counts() {
        let t = WeatherThresholds::default();
        let mut w = weather(60.0, false);
        w.past_7_days.longest_humid_run_days = 1;
        assert!(
            humid_stretch(&w, &t).is_none(),
            "one humid day is no stretch"
        );
        w.past_7_days.longest_humid_run_days = 3;
        assert_eq!(
            humid_stretch(&w, &t).as_deref(),
            Some("3 humid days in a row in the past week")
        );
    }

    #[test]
    fn frost_uses_the_shared_thresholds() {
        let t = WeatherThresholds::default();
        let mut w = weather(60.0, false);
        w.forecast[1].t_min_c = 0.5;
        let risk = frost(Some(&w), &t).unwrap();
        assert_eq!(
            risk.level,
            RiskLevel::Medium,
            "0.5 °C at 2 m is frost, not hard frost"
        );
        w.forecast[1].t_min_c = -2.0;
        assert_eq!(frost(Some(&w), &t).unwrap().level, RiskLevel::High);
        w.forecast[1].t_min_c = 0.6;
        assert!(frost(Some(&w), &t).is_none());
    }

    #[test]
    fn growth_stalled_rule() {
        let now: Timestamp = "2026-10-20T12:00:00Z".parse().unwrap();
        let stalled = [point(20, 40.0, now), point(2, 41.0, now)];
        assert!(growth_stalled(&stalled, Some(150.0), now).is_some());
        let growing = [point(20, 40.0, now), point(2, 55.0, now)];
        assert!(growth_stalled(&growing, Some(150.0), now).is_none());
        let old = [point(30, 40.0, now), point(10, 41.0, now)];
        assert!(
            growth_stalled(&old, Some(150.0), now).is_none(),
            "latest older than a week"
        );
        assert!(
            growth_stalled(&stalled, None, now).is_none(),
            "no mature height"
        );
        assert!(
            growth_stalled(&stalled, Some(41.0), now).is_none(),
            "reached mature height"
        );
    }

    #[test]
    fn frost_and_crowding() {
        let now: Timestamp = "2026-10-20T12:00:00Z".parse().unwrap();
        let w = weather(95.0, true);
        let plant: PlantId = "01JAX3K9T6R2B8V0QZ4M7D1E5A".parse().unwrap();
        let pins: Vec<Pin> = serde_json::from_value(serde_json::json!([
            {"plant_id": "01JAX3K9T6R2B8V0QZ4M7D1E5A", "x_m": 0.0, "y_m": 0.0},
            {"plant_id": "01JAX3K9T6R2B8V0QZ4M7D1E5B", "x_m": 0.3, "y_m": 0.0}
        ]))
        .unwrap();
        let r = risks(&FusionInput {
            problems: &[],
            weather: Some(&w),
            soil: &[],
            heights: &[],
            mature_height_cm: None,
            spacing_cm: Some(60.0),
            plant,
            pins: &pins,
            now,
            thresholds: WeatherThresholds::default(),
        });
        let fungal = r.iter().find(|r| r.risk == Risk::FUNGAL_SPREAD).unwrap();
        assert_eq!(
            fungal.level,
            RiskLevel::Low,
            "crowding alone adds it at low"
        );
        assert!(fungal.drivers[0].starts_with("Crowded: nearest plant 30 cm away"));
        let frost = r.iter().find(|r| r.risk == Risk::FROST_DAMAGE).unwrap();
        assert_eq!(frost.level, RiskLevel::High);
    }
}
