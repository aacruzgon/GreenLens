//! Test fixtures: a small tomato knowledge base in the seed format of contracts 10.

use std::collections::BTreeMap;

use domain::{
    events::SelectionSource,
    id::{IndicatorId, ProblemId},
    knowledge::{OrganismSeed, ProblemSeed, SpeciesSeed},
    ports::KnowledgeSnapshot,
    records::{
        diagnosis::{EvidenceSource, IndicatorEvidence, WeatherFit, Weight},
        weather::{
            CurrentWeather, ForecastDay, LocationPrecision, PastWeather, SoilNow,
            WeatherProviderName, WeatherSummary,
        },
    },
    rules::weather::WeatherThresholds,
    scan::Answer,
};
use serde_json::json;

use crate::candidates::{Candidate, ProfilePool};

fn indicator(problem: &str, slug: &str, weight: u8, kinds: &[&str]) -> serde_json::Value {
    json!({
        "id": format!("{problem}.{slug}"), "text": slug, "question": format!("{slug}?"),
        "weight": weight, "discriminating": true, "where": "lower_leaves", "image_kinds": kinds
    })
}

fn problem_json(id: &str, kind: &str, prevalence: &str) -> serde_json::Value {
    json!({
        "id": id, "name": id, "kind": kind, "affects": ["tomato"], "caused_by": [],
        "damage_patterns": [], "prevalence": prevalence,
        "indicators": [
            indicator(id, "first", 3, &["leaf_closeup"]),
            indicator(id, "second", 2, &["whole_plant"]),
            indicator(id, "third", 1, &["leaf_closeup", "leaf_underside"]),
        ],
        "conditions": {}, "looks_like": [], "treatments": [], "sources": [],
        "reviewed_by": "t", "reviewed_at": "2026-10-20"
    })
}

/// Four tomato problems: early blight (warm and humid), Septoria (its look-alike, with a
/// measurable small-spot indicator), late blight (rare) and flea beetle damage (a pest).
pub fn problems() -> Vec<ProblemSeed> {
    let mut blight = problem_json("tomato_early_blight", "fungal", "common");
    blight["conditions"] =
        json!({"temp_c": {"min": 24, "max": 29}, "rh_pct_min": 90, "humid_hours_min": 24});
    blight["looks_like"] =
        json!([{"problem": "tomato_septoria_leaf_spot", "how_to_tell": "smaller spots"}]);
    blight["affects"] = json!(["tomato"]);
    let mut septoria = problem_json("tomato_septoria_leaf_spot", "fungal", "common");
    septoria["affects"] = json!(["potato"]);
    septoria["indicators"][0] = json!({
        "id": "tomato_septoria_leaf_spot.small_spots", "text": "Spots about 1.5-3 mm across",
        "question": "Are the spots small?", "weight": 3, "discriminating": true,
        "where": "lower_leaves", "image_kinds": ["leaf_closeup"],
        "measure": {"quantity": "lesion_diameter_mm", "stat": "median", "min": 1.5, "max": 3}
    });
    let late = problem_json("tomato_late_blight", "fungal", "rare");
    let mut flea = problem_json("tomato_flea_beetle_damage", "pest", "occasional");
    flea["caused_by"] = json!(["flea_beetle"]);
    flea["damage_patterns"] = json!(["shot_holes"]);
    let mut grey = problem_json("tomato_grey_mould", "fungal", "occasional");
    grey["affects"] = json!(["potato"]);
    [blight, septoria, late, flea, grey]
        .into_iter()
        .map(|v| serde_json::from_value(v).expect("fixture problem"))
        .collect()
}

/// One fixture problem by id.
pub fn problem(id: &str) -> ProblemSeed {
    problems()
        .into_iter()
        .find(|p| p.id.as_str() == id)
        .expect("fixture id")
}

/// A species-sourced candidate with a neutral weather fit.
pub fn candidate<'k>(profiles: &'k [ProblemSeed], id: &str) -> Candidate<'k> {
    let pool = ProfilePool::new(profiles);
    Candidate {
        profile: pool.get(&id.parse().expect("slug")).expect("fixture id"),
        source: SelectionSource::Species,
        classifier: None,
        head: None,
        weather_fit: WeatherFit::Neutral,
    }
}

/// Several candidates, in the order named.
pub fn candidates<'k>(profiles: &'k [ProblemSeed], ids: &[&str]) -> Vec<Candidate<'k>> {
    ids.iter().map(|id| candidate(profiles, id)).collect()
}

/// `looks_like` in both directions, from the profiles.
pub fn lookalike_map(profiles: &[ProblemSeed]) -> BTreeMap<ProblemId, Vec<ProblemId>> {
    let mut map: BTreeMap<ProblemId, Vec<ProblemId>> = BTreeMap::new();
    for p in profiles {
        for l in &p.looks_like {
            map.entry(p.id.clone()).or_default().push(l.problem.clone());
            map.entry(l.problem.clone()).or_default().push(p.id.clone());
        }
    }
    map
}

/// An answer.
pub fn evidence(
    id: &IndicatorId,
    weight: u8,
    answer: Answer,
    source: EvidenceSource,
) -> IndicatorEvidence {
    IndicatorEvidence {
        indicator_id: id.clone(),
        text: id.slug().to_owned(),
        weight: Weight::new(weight).expect("1-3"),
        answer,
        reason: String::new(),
        source,
        measured: None,
    }
}

/// Weather at one temperature and humidity, now and for 3 forecast days, with `humid_hours`
/// over 90% in the past week. A forecast day at 90% mean humidity or more is a humid day (12
/// hours over 90%); `frost_risk` follows the shared rule (`WeatherThresholds::is_frost`).
pub fn weather(temp_c: f64, rh_pct: f64, humid_hours: u32) -> WeatherSummary {
    let thresholds = WeatherThresholds::default();
    let day = |d: &str| ForecastDay {
        date: d.parse().expect("date"),
        t_min_c: temp_c - 2.0,
        t_max_c: temp_c + 2.0,
        precip_mm: 0.0,
        mean_rh_pct: rh_pct,
        hours_rh_over_90: if rh_pct >= WeatherThresholds::HUMID_HOUR_RH_PCT {
            12
        } else {
            0
        },
        frost_risk: thresholds.is_frost(temp_c - 2.0),
    };
    WeatherSummary {
        provider: WeatherProviderName::OpenMeteo,
        fetched_at: "2026-10-18T14:00:00Z".parse().expect("ts"),
        location_precision: LocationPrecision::About1Km,
        current: CurrentWeather {
            temp_c,
            rh_pct,
            wind_kmh: 5.0,
            precip_mm_last_24h: 0.0,
        },
        past_7_days: PastWeather {
            precip_mm: 10.0,
            mean_rh_pct: rh_pct,
            hours_rh_over_90: humid_hours,
            longest_humid_run_days: 0,
        },
        forecast: vec![day("2026-10-18"), day("2026-10-19"), day("2026-10-20")],
        soil: SoilNow {
            moisture_0_9cm_m3m3: 0.25,
            moisture_9_27cm_m3m3: 0.3,
            temp_6cm_c: 15.0,
            et0_mm_last_24h: 2.0,
        },
    }
}

/// The fixture knowledge base as the store returns it.
pub fn snapshot() -> KnowledgeSnapshot {
    let species = |id: &str| -> SpeciesSeed {
        serde_json::from_value(json!({
            "id": id, "scientific_name": id, "common_names": [id], "profile": "full",
            "care": {"water": "medium", "light": "full_sun", "soil_ph": {"min": 6, "max": 7}, "spacing_cm": 60, "mature_height_cm": 150},
            "classifier_supported": true, "bioclip_label": id, "sources": [], "reviewed_by": "t"
        }))
        .expect("fixture species")
    };
    let organism = |id: &str, role: &str| -> OrganismSeed {
        serde_json::from_value(json!({
            "id": id, "name": id, "scientific_name": id, "taxon_group": "insect", "role": role,
            "recognition": "", "bioclip_label": id, "sources": [], "reviewed_by": "t"
        }))
        .expect("fixture organism")
    };
    let mut ladybug = organism("ladybug_larva", "beneficial");
    ladybug.preys_on = Some(vec!["flea_beetle".parse().expect("slug")]);
    KnowledgeSnapshot {
        species: vec![species("tomato"), species("potato")],
        problems: problems(),
        organisms: vec![organism("flea_beetle", "pest"), ladybug],
        ..KnowledgeSnapshot::default()
    }
}
