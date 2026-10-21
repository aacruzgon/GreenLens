//! Geocoder tests: the live Open-Meteo, Nominatim and Zippopotam answers of 2026-10-20, the
//! offline zone finder, the cache, and the GPS label when the reverse lookup fails.

use std::sync::{Arc, LazyLock};

use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

use super::*;

const TORONTO: &str = include_str!("../../fixtures/geocoding_toronto.json");
const NOMINATIM_M5V3L9: &str = include_str!("../../fixtures/nominatim_search_m5v3l9.json");
const REVERSE_TORONTO: &str = include_str!("../../fixtures/nominatim_reverse_toronto.json");
const REVERSE_NIAGARA: &str = include_str!("../../fixtures/nominatim_reverse_niagara_county.json");
const REVERSE_OCEAN: &str = include_str!("../../fixtures/nominatim_reverse_ocean.json");
const ZIPPOPOTAM_M5V: &str = include_str!("../../fixtures/zippopotam_m5v.json");

/// Loading the boundaries takes a moment; share one finder across tests.
static ZONES: LazyLock<Arc<TimeZoneFinder>> = LazyLock::new(|| Arc::new(TimeZoneFinder::default()));

#[test]
fn postal_codes_are_classified() {
    assert_eq!(
        PostalCode::parse(" m5v 3l9 "),
        Some(PostalCode::Canadian("M5V3L9".into()))
    );
    assert_eq!(
        PostalCode::parse("N1G"),
        Some(PostalCode::CanadianFsa("N1G".into()))
    );
    assert_eq!(
        PostalCode::parse("90210-1234"),
        Some(PostalCode::UsZip("90210".into()))
    );
    assert_eq!(PostalCode::parse("Toronto"), None);
}

#[test]
fn open_meteo_fixture_label() {
    let r: SearchResponse = serde_json::from_str(TORONTO).unwrap();
    assert_eq!(r.results[0].label(), "Toronto, ON");
    assert_eq!(r.results[1].label(), "Toronto, PE");
    let empty: SearchResponse = serde_json::from_str(r#"{"generationtime_ms":0.5}"#).unwrap();
    assert_eq!(empty.results.len(), 0);
}

#[test]
fn offline_time_zones() {
    let z = |lat, lon| ZONES.zone(Coordinates::new(lat, lon).unwrap()).map(|t| t.0);
    assert_eq!(z(43.65, -79.38).as_deref(), Some("America/Toronto"));
    assert_eq!(z(49.28, -123.12).as_deref(), Some("America/Vancouver"));
}

#[test]
fn rounding_per_precision() {
    let c = Coordinates::new(43.65321, -79.38321).unwrap();
    let one = round_for(c, LocationPrecision::About1Km);
    assert!((one.lat - 43.65).abs() < 1e-9 && (one.lon + 79.38).abs() < 1e-9);
    let ten = round_for(c, LocationPrecision::About10Km);
    assert!((ten.lat - 43.7).abs() < 1e-9 && (ten.lon + 79.4).abs() < 1e-9);
    assert_eq!(round_for(c, LocationPrecision::Exact), c);
}

fn geocoder(server: &MockServer) -> CompositeGeocoder {
    CompositeGeocoder::new(
        reqwest::Client::new(),
        GeocoderConfig {
            open_meteo_url: format!("{}/v1/search", server.uri()),
            nominatim_url: server.uri(),
            zippopotam_url: Some(server.uri()),
            nominatim_interval: std::time::Duration::ZERO,
            ..GeocoderConfig::default()
        },
        Arc::clone(&ZONES),
    )
}

#[tokio::test]
async fn city_through_open_meteo_and_cached() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/search"))
        .and(query_param("name", "Toronto"))
        .and(query_param("countryCode", "CA"))
        .respond_with(ResponseTemplate::new(200).set_body_string(TORONTO))
        .expect(1)
        .mount(&server)
        .await;
    let g = geocoder(&server);
    let a = g.geocode(GeocodeQuery::City("Toronto")).await.unwrap();
    let b = g.geocode(GeocodeQuery::City(" toronto ")).await.unwrap();
    assert_eq!(a.label, "Toronto, ON");
    assert_eq!(a.time_zone.0, "America/Toronto");
    assert_eq!(a, b);
}

#[tokio::test]
async fn canadian_postal_code_through_nominatim() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search"))
        .and(query_param("postalcode", "M5V3L9"))
        .respond_with(ResponseTemplate::new(200).set_body_string(NOMINATIM_M5V3L9))
        .mount(&server)
        .await;
    let g = geocoder(&server)
        .geocode(GeocodeQuery::PostalCode("m5v 3l9"))
        .await
        .unwrap();
    assert_eq!(g.label, "Toronto, ON");
    assert_eq!(g.time_zone.0, "America/Toronto");
}

#[tokio::test]
async fn fsa_falls_back_to_zippopotam() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/ca/M5V"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ZIPPOPOTAM_M5V))
        .mount(&server)
        .await;
    let g = geocoder(&server)
        .geocode(GeocodeQuery::PostalCode("M5V 9Z9"))
        .await
        .unwrap();
    assert_eq!(g.label, "Downtown Toronto, ON");
}

#[tokio::test]
async fn unknown_city_is_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"generationtime_ms":0.4}"#))
        .mount(&server)
        .await;
    let err = geocoder(&server)
        .geocode(GeocodeQuery::City("Nowhereville"))
        .await
        .unwrap_err();
    assert!(matches!(err, ProviderError::NotFound { .. }));
}

#[tokio::test]
async fn gps_reverse_sends_a_rounded_point() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/reverse"))
        .and(query_param("lat", "43.65"))
        .and(query_param("lon", "-79.38"))
        .respond_with(ResponseTemplate::new(200).set_body_string(REVERSE_TORONTO))
        .mount(&server)
        .await;
    let at = Coordinates::new(43.653_21, -79.383_21).unwrap();
    let g = geocoder(&server)
        .geocode(GeocodeQuery::Coordinates(at))
        .await
        .unwrap();
    assert_eq!(g.label, "Toronto, ON");
    assert_eq!(
        g.coordinates, at,
        "the caller's point is kept, only the lookup is rounded"
    );
}

#[test]
fn live_reverse_shapes() {
    let NominatimReverse::Place(p) = serde_json::from_str(REVERSE_TORONTO).unwrap() else {
        panic!("a place")
    };
    assert_eq!(p.address.label().as_deref(), Some("Toronto, ON"));
    // A point in Lake Ontario: only a county, across the border.
    let NominatimReverse::Place(p) = serde_json::from_str(REVERSE_NIAGARA).unwrap() else {
        panic!("a place")
    };
    assert_eq!(p.address.label().as_deref(), Some("Niagara County, NY"));
    assert_eq!(
        serde_json::from_str::<NominatimReverse>(REVERSE_OCEAN).unwrap(),
        NominatimReverse::Error {
            error: "Unable to geocode".into()
        }
    );
}

#[test]
fn zone_labels() {
    let l = |z: &str| zone_label(&TimeZone(z.to_owned()));
    assert_eq!(l("America/Toronto"), "Toronto time zone");
    assert_eq!(
        l("America/Argentina/Buenos_Aires"),
        "Buenos Aires time zone"
    );
    assert_eq!(l("UTC"), "UTC time zone");
}

/// The reverse lookup fails four ways; each time the garden is labelled by its zone, and the
/// fallback isn't cached.
#[tokio::test]
async fn gps_reverse_failure_falls_back_to_the_zone() {
    for failure in [
        ResponseTemplate::new(503),
        ResponseTemplate::new(429).insert_header("retry-after", "60"),
        ResponseTemplate::new(200).set_body_string(REVERSE_OCEAN),
        ResponseTemplate::new(200).set_body_string("<html>"),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/reverse"))
            .respond_with(failure)
            .expect(2)
            .mount(&server)
            .await;
        let g = geocoder(&server);
        let at = Coordinates::new(45.42, -75.70).unwrap(); // Ottawa
        for _ in 0..2 {
            let found = g.geocode(GeocodeQuery::Coordinates(at)).await.unwrap();
            assert_eq!(found.label, "Toronto time zone");
            assert_eq!(found.time_zone.0, "America/Toronto");
            assert_eq!(found.coordinates, at);
        }
    }
}

#[tokio::test]
async fn gps_reverse_success_is_cached() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/reverse"))
        .respond_with(ResponseTemplate::new(200).set_body_string(REVERSE_TORONTO))
        .expect(1)
        .mount(&server)
        .await;
    let g = geocoder(&server);
    let a = Coordinates::new(43.651, -79.381).unwrap();
    let b = Coordinates::new(43.652, -79.382).unwrap();
    let first = g.geocode(GeocodeQuery::Coordinates(a)).await.unwrap();
    assert_eq!(first.label, "Toronto, ON");
    let again = g.geocode(GeocodeQuery::Coordinates(b)).await.unwrap();
    assert_eq!(again.label, "Toronto, ON");
    assert_eq!(again.coordinates, b, "the cache keeps the caller's point");
}
