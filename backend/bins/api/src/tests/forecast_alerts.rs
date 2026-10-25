//! The `weather` task's forecast alerts and plan triggers, one per spell ([architecture 8.3]):
//! two runs over a fake Open-Meteo, with a store whose `create_alert_task` keeps spells the way
//! the real one does (the store's own tests in `store/tests/alerts.rs` check the query).

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use domain::{
    garden::gardens::TimeZone,
    id::{PlanId, PlanTriggerId, PlantId},
    ports::{CellGardens, NewTask, PlanTriggerRule, PushReport, PushTarget},
    records::{
        plan::TriggerKind,
        task::{Task, TaskSource},
        weather::{
            CellWeather, Coordinates, CurrentWeather, ForecastDay, PastWeather, SoilNow,
            WeatherCell, WeatherProviderName,
        },
    },
    wire::LocalDate,
};

use super::*;

fn garden() -> GardenId {
    GARDEN.parse().unwrap()
}

fn cell() -> WeatherCell {
    WeatherCell::of(Coordinates {
        lat: 43.65,
        lon: -79.38,
    })
}

/// A cell's weather with frost on every forecast day from `first`.
fn frosty(first: &str, days: u32) -> CellWeather {
    let first: LocalDate = first.parse().unwrap();
    CellWeather {
        provider: WeatherProviderName::OpenMeteo,
        fetched_at: NOW.parse().unwrap(),
        current: CurrentWeather {
            temp_c: 3.0,
            rh_pct: 70.0,
            wind_kmh: 5.0,
            precip_mm_last_24h: 0.0,
        },
        past_7_days: PastWeather {
            precip_mm: 3.0,
            mean_rh_pct: 70.0,
            hours_rh_over_90: 0,
            longest_humid_run_days: 0,
        },
        forecast: (0..days)
            .map(|i| ForecastDay {
                date: first.plus_days(i).unwrap(),
                t_min_c: -1.0,
                t_max_c: 9.0,
                precip_mm: 0.0,
                mean_rh_pct: 70.0,
                hours_rh_over_90: 0,
                frost_risk: true,
            })
            .collect(),
        soil: SoilNow {
            moisture_0_9cm_m3m3: 0.3,
            moisture_9_27cm_m3m3: 0.3,
            temp_6cm_c: 6.0,
            et0_mm_last_24h: 1.0,
        },
    }
}

/// The spells the fake store has alerted, by `AlertKey::spell`, and the tasks it created.
#[derive(Default)]
struct Spells {
    until: Mutex<HashMap<String, LocalDate>>,
    created: Mutex<Vec<NewTask>>,
}

impl Spells {
    /// `create_alert_task` as the store answers it: a fired day up to the day after a spell's
    /// last seen day continues that spell.
    fn create(&self, garden: GardenId, task: NewTask, until: LocalDate) -> Option<Task> {
        let key = task.alert_key.clone().unwrap();
        let mut spells = self.until.lock().unwrap();
        if let Some(seen) = spells.get_mut(&key.spell())
            && key.date() <= seen.plus_days(1).unwrap()
        {
            *seen = (*seen).max(until);
            return None;
        }
        spells.insert(key.spell(), until);
        self.created.lock().unwrap().push(task.clone());
        Some(Task {
            id: task.id,
            garden_id: garden,
            plant_id: task.plant,
            plan_id: task.plan,
            plan_step_id: task.plan_step,
            scan_id: None,
            title: task.title,
            detail: task.detail,
            due: task.due,
            repeat: None,
            source: task.source,
            trigger: task.trigger,
            done_at: None,
            dismissed_at: None,
            dismissed_reason: None,
            created_at: task.created_at,
        })
    }
}

/// One cell with the garden. The first run misses the cache and fetches from 18 October (once);
/// the second reads the next forecast, from 19 October, from the cache.
fn two_forecasts(store: &mut FakeStore) -> MockWeatherProvider {
    store
        .weather_cache_repo
        .expect_gardens_by_cell()
        .returning(|| {
            Ok(vec![CellGardens {
                cell: cell(),
                time_zone: TimeZone("America/Toronto".into()),
                gardens: vec![garden()],
            }])
        });
    let reads = Arc::new(AtomicUsize::new(0));
    store
        .weather_cache_repo
        .expect_cached()
        .returning(move |_, _| match reads.fetch_add(1, Ordering::SeqCst) {
            0 => Ok(None),
            _ => Ok(Some(frosty("2026-10-19", 14))),
        });
    store
        .weather_cache_repo
        .expect_put()
        .times(1)
        .returning(|_, _, _| Ok(()));
    let mut weather = MockWeatherProvider::new();
    weather
        .expect_fetch_batch()
        .times(1)
        .returning(|cells| Ok(cells.iter().map(|_| frosty("2026-10-18", 14)).collect()));
    weather
}

/// An active frost rule of a plan step in the garden.
fn frost_rule(store: &mut FakeStore) {
    let (plan, plant): (PlanId, PlantId) = (
        "01JAX3K9T6R2B8V0QZ4M7D1E5A".parse().unwrap(),
        "01JAX3K9T6R2B8V0QZ4M7D1E5P".parse().unwrap(),
    );
    store
        .plan_repo
        .expect_active_triggers()
        .returning(move |_| {
            Ok(vec![PlanTriggerRule {
                id: "01JAX3K9T6R2B8V0QZ4M7D1E5R"
                    .parse::<PlanTriggerId>()
                    .unwrap(),
                plan,
                garden: garden(),
                plant,
                step: "s2".parse().unwrap(),
                trigger: TriggerKind::Frost,
                until: None,
                title: "Cover the tomatoes".into(),
                detail: "Use a frost cloth overnight.".into(),
            }])
        });
}

/// One member with one iPhone.
fn one_phone(store: &mut FakeStore) {
    store
        .garden_repo
        .expect_member_ids()
        .returning(|_| Ok(vec![USER.parse().unwrap()]));
    store
        .identity_repo
        .expect_push_targets()
        .returning(|users| {
            Ok(users
                .iter()
                .map(|&user| PushTarget {
                    user,
                    token: "ExponentPushToken[abc]".parse().unwrap(),
                    platform: domain::garden::account::Platform::Ios,
                })
                .collect())
        });
}

/// A frost forecast on three days, then the next hour's forecast shifted a day (the first day
/// dropped out, a new frost day at the end): one alert, one push, one plan-trigger task.
#[tokio::test]
async fn a_frost_forecast_gives_one_alert_not_one_per_sweep() {
    let mut store = FakeStore::default();
    let weather = two_forecasts(&mut store);
    frost_rule(&mut store);
    one_phone(&mut store);
    let spells = Arc::new(Spells::default());
    let seen = Arc::clone(&spells);
    // Per run: the forecast alert's spell and the rule's.
    store
        .plan_repo
        .expect_create_alert_task()
        .times(4)
        .returning(move |garden, task, until| Ok(seen.create(garden, task, until)));
    let mut notifier = MockNotifier::new();
    notifier
        .expect_push()
        .withf(|messages| messages.len() == 1 && messages[0].title == "Frost expected")
        .times(1)
        .returning(|_| {
            Ok(PushReport {
                accepted: 1,
                unregistered: Vec::new(),
                failed: 0,
            })
        });

    let state = with_weather(
        state(
            store,
            MockAuthVerifier::new(),
            MockBlobStore::new(),
            no_images(),
        ),
        weather,
        notifier,
    );
    crate::scheduler::weather_and_triggers(Arc::clone(&state))
        .await
        .unwrap();
    crate::scheduler::weather_and_triggers(state).await.unwrap();

    let created = spells.created.lock().unwrap();
    let kinds: Vec<_> = created
        .iter()
        .map(|t| (t.source, t.due.map(|d| d.to_string())))
        .collect();
    assert_eq!(
        kinds,
        vec![
            (TaskSource::ForecastAlert, Some("2026-10-18".into())),
            (TaskSource::Plan, Some("2026-10-18".into())),
        ]
    );
    let until = spells.until.lock().unwrap();
    assert_eq!(
        until
            .get(&format!("{GARDEN}:frost"))
            .map(ToString::to_string),
        Some("2026-10-21".into()),
        "the second run saw the spell to the end of its 3-day horizon"
    );
}

/// `state` with the weather provider and notifier of the test's.
fn with_weather(
    state: Arc<AppState>,
    weather: MockWeatherProvider,
    notifier: MockNotifier,
) -> Arc<AppState> {
    let state = Arc::try_unwrap(state).ok().expect("the only reference");
    Arc::new(AppState {
        ports: Ports {
            weather: Arc::new(weather),
            notifier: Arc::new(notifier),
            ..state.ports
        },
        ..state
    })
}
