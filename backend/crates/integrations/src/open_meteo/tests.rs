//! Tests against the live fixture of 2026-10-20, 23:05 UTC (Toronto and Ottawa, `past_days=7`,
//! `forecast_days=16`) and a `wiremock` server.

use std::sync::Arc;

use chrono::{TimeZone as _, Utc};
use domain::{
    garden::gardens::TimeZone,
    ports::{CellRequest, MockWeatherProvider, ProviderError, WeatherProvider},
    records::weather::{Coordinates, WeatherCell},
    testing::FixedClock,
    wire::Timestamp,
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, query_param},
};

use super::*;

const FIXTURE: &str = include_str!("../../fixtures/openmeteo_forecast_multi.json");

/// 23:00 UTC is 19:00 in Toronto (UTC−4 in October), the hour the fixture was fetched.
fn now() -> Timestamp {
    Timestamp::from(Utc.with_ymd_and_hms(2026, 10, 20, 23, 0, 0).unwrap())
}

fn cell(lat: f64, lon: f64) -> CellRequest {
    CellRequest {
        cell: WeatherCell::of(Coordinates::new(lat, lon).unwrap()),
        time_zone: TimeZone("America/Toronto".to_owned()),
    }
}

#[test]
fn summarises_the_live_fixture() {
    let forecasts = serde_json::from_str::<ForecastResponse>(FIXTURE)
        .unwrap()
        .into_vec();
    assert_eq!(forecasts.len(), 2, "two locations come back as an array");
    let toronto = cell_weather(&forecasts[0], now(), 14, &WeatherThresholds::default()).unwrap();
    assert!((toronto.current.temp_c - 11.3).abs() < 1e-9);
    assert!((toronto.current.rh_pct - 69.0).abs() < 1e-9);
    assert!((toronto.current.wind_kmh - 5.6).abs() < 1e-9);
    assert!(toronto.current.precip_mm_last_24h.abs() < 1e-9);
    assert!((toronto.past_7_days.precip_mm - 1.6).abs() < 1e-6);
    assert_eq!(toronto.past_7_days.hours_rh_over_90, 5);
    assert!((toronto.soil.moisture_0_9cm_m3m3 - 0.4058).abs() < 1e-4);
    assert!((toronto.soil.moisture_9_27cm_m3m3 - 0.43).abs() < 1e-9);
    assert!((toronto.soil.temp_6cm_c - 15.6).abs() < 1e-9);
    assert!((toronto.soil.et0_mm_last_24h - 2.38).abs() < 1e-6);
    assert_eq!(toronto.forecast.len(), 14);
    assert_eq!(toronto.forecast[0].date.to_string(), "2026-10-20");
    assert!(
        toronto.forecast.iter().all(|d| !d.frost_risk),
        "lowest 1.0 °C"
    );

    let ottawa = cell_weather(&forecasts[1], now(), 14, &WeatherThresholds::default()).unwrap();
    assert!(!ottawa.forecast[7].frost_risk, "1.1 °C on 2026-10-27");
    assert!(ottawa.forecast[8].frost_risk, "-1.5 °C on 2026-10-28");
    assert_eq!(ottawa.past_7_days.hours_rh_over_90, 25);
}

#[test]
fn the_past_week_keeps_its_longest_humid_run() {
    // Hours over 90 % on 13–19 October: Toronto 6, 0, 0, 5, 0, 0, 0 (no humid day);
    // Ottawa 22, 11, 0, 10, 0, 0, 0 (two in a row: a humid stretch in the past week).
    let forecasts = serde_json::from_str::<ForecastResponse>(FIXTURE)
        .unwrap()
        .into_vec();
    let t = WeatherThresholds::default();
    let toronto = cell_weather(&forecasts[0], now(), 14, &t).unwrap();
    let ottawa = cell_weather(&forecasts[1], now(), 14, &t).unwrap();
    assert_eq!(toronto.past_7_days.longest_humid_run_days, 0);
    assert_eq!(ottawa.past_7_days.longest_humid_run_days, 2);
    assert!(!t.had_humid_stretch(&toronto.past_7_days));
    assert!(t.had_humid_stretch(&ottawa.past_7_days));
}

#[test]
fn depth_weighted_moisture() {
    assert!((moisture_0_9cm(0.3, 0.3, 0.3) - 0.3).abs() < 1e-12);
    assert!((moisture_0_9cm(0.9, 0.0, 0.0) - 0.1).abs() < 1e-12);
}

#[test]
fn missing_hour_and_wrong_units_are_errors() {
    let mut f = serde_json::from_str::<ForecastResponse>(FIXTURE)
        .unwrap()
        .into_vec()
        .remove(0);
    let late = Timestamp::from(Utc.with_ymd_and_hms(2026, 12, 1, 0, 0, 0).unwrap());
    assert!(matches!(
        cell_weather(&f, late, 14, &WeatherThresholds::default()),
        Err(SummaryError::HourMissing(_))
    ));
    f.hourly_units.insert("wind_speed_10m".into(), "m/s".into());
    assert!(matches!(
        cell_weather(&f, now(), 14, &WeatherThresholds::default()),
        Err(SummaryError::Units { .. })
    ));
}

fn adapter(server: &MockServer) -> OpenMeteo {
    OpenMeteo::new(
        reqwest::Client::new(),
        OpenMeteoConfig {
            forecast_url: format!("{}/v1/forecast", server.uri()),
            ..OpenMeteoConfig::default()
        },
        Arc::new(FixedClock::at(now())),
    )
}

#[tokio::test]
async fn batch_sends_lists_and_reads_the_array() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(query_param("latitude", "43.65,45.42"))
        .and(query_param("longitude", "-79.38,-75.70"))
        .and(query_param("timezone", "America/Toronto,America/Toronto"))
        .and(query_param("past_days", "7"))
        .and(query_param("forecast_days", "14"))
        .respond_with(ResponseTemplate::new(200).set_body_string(FIXTURE))
        .mount(&server)
        .await;
    let out = adapter(&server)
        .fetch_batch(&[cell(43.6532, -79.3832), cell(45.4215, -75.6972)])
        .await
        .unwrap();
    assert_eq!(out.len(), 2);
}

#[tokio::test]
async fn one_cell_reads_the_object() {
    let server = MockServer::start().await;
    let first = serde_json::from_str::<serde_json::Value>(FIXTURE).unwrap()[0].clone();
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(first))
        .mount(&server)
        .await;
    let w = adapter(&server)
        .fetch_cell(&cell(43.65, -79.38))
        .await
        .unwrap();
    assert!((w.current.temp_c - 11.3).abs() < 1e-9);
}

#[tokio::test]
async fn statuses_map_to_provider_errors() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "60"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_string(r#"{"error":true,"reason":"Forecast days is invalid."}"#),
        )
        .mount(&server)
        .await;
    let a = adapter(&server);
    let c = cell(43.65, -79.38);
    assert!(matches!(
        a.fetch_cell(&c).await,
        Err(ProviderError::RateLimited {
            retry_after_s: Some(60),
            ..
        })
    ));
    assert!(matches!(
        a.fetch_cell(&c).await,
        Err(ProviderError::BadResponse { .. })
    ));
}

fn down() -> ProviderError {
    ProviderError::Unavailable {
        provider: PROVIDER,
        source: "down".into(),
    }
}

#[tokio::test]
async fn fallback_retries_once_then_goes_cell_by_cell() {
    let forecasts = serde_json::from_str::<ForecastResponse>(FIXTURE)
        .unwrap()
        .into_vec();
    let weather = cell_weather(&forecasts[0], now(), 14, &WeatherThresholds::default()).unwrap();
    let mut mock = MockWeatherProvider::new();
    mock.expect_fetch_batch()
        .times(2)
        .returning(|_| Err(down()));
    let w = weather.clone();
    let mut n = 0;
    mock.expect_fetch_cell().times(2).returning(move |_| {
        n += 1;
        if n == 1 { Ok(w.clone()) } else { Err(down()) }
    });
    let cells = [cell(43.65, -79.38), cell(45.42, -75.70)];
    let out = fetch_cells(&mock, &cells).await;
    assert_eq!(out.len(), 2);
    assert!(out[0].weather.is_ok());
    assert!(out[1].weather.is_err());
}

#[tokio::test]
async fn fallback_keeps_a_good_retry() {
    let forecasts = serde_json::from_str::<ForecastResponse>(FIXTURE)
        .unwrap()
        .into_vec();
    let weather = cell_weather(&forecasts[0], now(), 14, &WeatherThresholds::default()).unwrap();
    let mut mock = MockWeatherProvider::new();
    let mut first = true;
    mock.expect_fetch_batch().times(2).returning(move |cells| {
        if std::mem::take(&mut first) {
            Err(down())
        } else {
            Ok(vec![weather.clone(); cells.len()])
        }
    });
    mock.expect_fetch_cell().never();
    let out = fetch_cells(&mock, &[cell(43.65, -79.38)]).await;
    assert!(out[0].weather.is_ok());
}
