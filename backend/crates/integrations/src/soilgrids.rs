//! ISRIC `SoilGrids` v2.0: the [`SoilProvider`] (architecture 6.4; [contracts 10], `SoilProfile`).
//!
//! One query returns eight properties at six depth bands as integers in "mapped units";
//! dividing by each layer's `d_factor` gives the contract's units, which are `SoilGrids`'
//! `target_units`. Urban pixels often have no data at all (`"mean": null` everywhere: central
//! Toronto, Scarborough and Guelph were all null on 2026-10-15), which is a distinct outcome:
//! the adapter probes nearby points with a cheap one-value query, fetches the full profile at
//! the first point that has data, and otherwise answers [`ProviderError::NotFound`].

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use domain::{
    knowledge::{DepthBand, SoilDepth, SoilProfile},
    ports::{Clock, ProviderError, SoilProvider},
    records::weather::Coordinates,
};
use serde::Deserialize;

use crate::http::{self, Pacer, TransportError};

/// The `upstream` name.
pub const PROVIDER: &str = "soilgrids";

/// The properties fetched, `SoilGrids` names.
pub const PROPERTIES: &[&str] = &[
    "phh2o", "clay", "sand", "silt", "soc", "nitrogen", "cec", "bdod",
];

/// The depth bands, `SoilGrids` labels.
pub const DEPTHS: &[&str] = &[
    "0-5cm",
    "5-15cm",
    "15-30cm",
    "30-60cm",
    "60-100cm",
    "100-200cm",
];

/// Settings.
#[derive(Debug, Clone)]
pub struct SoilGridsConfig {
    /// `https://rest.isric.org/soilgrids/v2.0/properties/query`.
    pub query_url: String,
    /// Total deadline: a full query took 9–15 s live, so 60 s.
    pub timeout: Duration,
    /// The least time between two calls: 15 s keeps under the documented 5 calls a minute.
    pub min_interval: Duration,
    /// Radii of the rings probed around a point with no data, km.
    pub rings_km: Vec<f64>,
    /// Points per ring (north, east, south, west for 4).
    pub points_per_ring: u32,
}

impl Default for SoilGridsConfig {
    fn default() -> Self {
        Self {
            query_url: "https://rest.isric.org/soilgrids/v2.0/properties/query".to_owned(),
            timeout: Duration::from_secs(60),
            min_interval: Duration::from_secs(15),
            rings_km: vec![2.0, 5.0],
            points_per_ring: 4,
        }
    }
}

/// The adapter. One per process: the limiter is shared by every garden's fetch.
pub struct SoilGrids {
    http: reqwest::Client,
    config: SoilGridsConfig,
    clock: Arc<dyn Clock>,
    /// Spaces calls by `min_interval`.
    pacer: Pacer,
}

impl std::fmt::Debug for SoilGrids {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SoilGrids")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// `SoilGrids`' response, as observed live.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct QueryResponse {
    /// The layers.
    pub properties: Properties,
}

/// `properties`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Properties {
    /// One per property.
    pub layers: Vec<Layer>,
}

/// One property.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Layer {
    /// `phh2o`, `clay`, …
    pub name: String,
    /// The conversion.
    pub unit_measure: UnitMeasure,
    /// One per depth band.
    pub depths: Vec<LayerDepth>,
}

/// `unit_measure`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UnitMeasure {
    /// Divide the mapped value by this to get `target_units`.
    pub d_factor: f64,
    /// `pH*10`, `g/kg`, …
    pub mapped_units: String,
    /// `%`, `g/kg`, `cmol(c)/kg`, `kg/dm³`.
    pub target_units: String,
}

/// One depth of one property.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LayerDepth {
    /// `0-5cm`.
    pub label: String,
    /// The values requested.
    pub values: LayerValues,
}

/// `values`: only `mean` is requested. `null` where there is no data.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct LayerValues {
    /// Mapped units.
    pub mean: Option<f64>,
}

/// Converts a response; `None` when every value is null (no data at this pixel).
pub fn to_depths(response: &QueryResponse) -> Option<Vec<SoilDepth>> {
    let mut depths: Vec<SoilDepth> = DEPTHS
        .iter()
        .filter_map(|label| band(label))
        .map(|depth_cm| SoilDepth {
            depth_cm,
            ph_h2o: None,
            clay_pct: None,
            sand_pct: None,
            silt_pct: None,
            organic_carbon_g_kg: None,
            nitrogen_g_kg: None,
            cec_cmol_kg: None,
            bulk_density_kg_dm3: None,
        })
        .collect();
    let mut any = false;
    for layer in &response.properties.layers {
        if layer.unit_measure.d_factor == 0.0 {
            continue;
        }
        for d in &layer.depths {
            let Some(target) =
                band(&d.label).and_then(|b| depths.iter_mut().find(|s| s.depth_cm == b))
            else {
                continue;
            };
            let value = d.values.mean.map(|m| m / layer.unit_measure.d_factor);
            any |= value.is_some();
            let slot = match layer.name.as_str() {
                "phh2o" => &mut target.ph_h2o,
                "clay" => &mut target.clay_pct,
                "sand" => &mut target.sand_pct,
                "silt" => &mut target.silt_pct,
                "soc" => &mut target.organic_carbon_g_kg,
                "nitrogen" => &mut target.nitrogen_g_kg,
                "cec" => &mut target.cec_cmol_kg,
                "bdod" => &mut target.bulk_density_kg_dm3,
                _ => continue,
            };
            *slot = value;
        }
    }
    any.then_some(depths)
}

fn band(label: &str) -> Option<DepthBand> {
    Some(match label {
        "0-5cm" => DepthBand::D0To5,
        "5-15cm" => DepthBand::D5To15,
        "15-30cm" => DepthBand::D15To30,
        "30-60cm" => DepthBand::D30To60,
        "60-100cm" => DepthBand::D60To100,
        "100-200cm" => DepthBand::D100To200,
        _ => return None,
    })
}

/// The points probed around `at`, nearest ring first: `points_per_ring` points on each circle,
/// starting north, converted with 111.32 km per degree of latitude.
pub fn nearby_points(at: Coordinates, rings_km: &[f64], points_per_ring: u32) -> Vec<Coordinates> {
    let mut out = Vec::new();
    for &km in rings_km {
        for k in 0..points_per_ring {
            let bearing = std::f64::consts::TAU * f64::from(k) / f64::from(points_per_ring);
            let dlat = km * bearing.cos() / 111.32;
            let dlon = km * bearing.sin() / (111.32 * at.lat.to_radians().cos().max(0.01));
            if let Some(p) = Coordinates::new(at.lat + dlat, at.lon + dlon) {
                out.push(p);
            }
        }
    }
    out
}

impl SoilGrids {
    /// An adapter over the shared client.
    pub fn new(http: reqwest::Client, config: SoilGridsConfig, clock: Arc<dyn Clock>) -> Self {
        Self {
            http,
            pacer: Pacer::new(config.min_interval),
            config,
            clock,
        }
    }

    /// One rate-limited query. `full` asks for every property and depth; otherwise one value
    /// (pH at 0–5 cm), which answers in about 0.3 s and is enough to tell data from no data.
    async fn query(&self, at: Coordinates, full: bool) -> Result<QueryResponse, ProviderError> {
        self.pacer.wait().await;
        let mut query: Vec<(&str, String)> = vec![
            ("lon", format!("{:.5}", at.lon)),
            ("lat", format!("{:.5}", at.lat)),
        ];
        if full {
            query.extend(PROPERTIES.iter().map(|p| ("property", (*p).to_owned())));
            query.extend(DEPTHS.iter().map(|d| ("depth", (*d).to_owned())));
        } else {
            query.push(("property", "phh2o".to_owned()));
            query.push(("depth", "0-5cm".to_owned()));
        }
        query.push(("value", "mean".to_owned()));
        let response = self
            .http
            .get(&self.config.query_url)
            .query(&query)
            .timeout(self.config.timeout)
            .send()
            .await
            .map_err(|e| unavailable(TransportError::from_reqwest(e)))?;
        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimited {
                provider: PROVIDER,
                retry_after_s: http::retry_after(response.headers())
                    .and_then(|d| u32::try_from(d.as_secs()).ok()),
            });
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| unavailable(TransportError::from_reqwest(e)))?;
        if !status.is_success() {
            let source = format!(
                "HTTP {status}: {}",
                http::snippet(&String::from_utf8_lossy(&bytes), 256)
            )
            .into();
            return Err(if status.is_server_error() {
                ProviderError::Unavailable {
                    provider: PROVIDER,
                    source,
                }
            } else {
                ProviderError::BadResponse {
                    provider: PROVIDER,
                    source,
                }
            });
        }
        serde_json::from_slice(&bytes).map_err(|e| ProviderError::BadResponse {
            provider: PROVIDER,
            source: Box::new(e),
        })
    }
}

#[async_trait]
impl SoilProvider for SoilGrids {
    async fn soil_profile(&self, at: Coordinates) -> Result<SoilProfile, ProviderError> {
        if let Some(depths) = to_depths(&self.query(at, true).await?) {
            return Ok(SoilProfile {
                fetched_at: self.clock.now(),
                depths,
            });
        }
        for point in nearby_points(at, &self.config.rings_km, self.config.points_per_ring) {
            if to_depths(&self.query(point, false).await?).is_some()
                && let Some(depths) = to_depths(&self.query(point, true).await?)
            {
                tracing::info!(
                    provider = PROVIDER,
                    "soil profile taken from a nearby point"
                );
                return Ok(SoilProfile {
                    fetched_at: self.clock.now(),
                    depths,
                });
            }
        }
        Err(ProviderError::NotFound { provider: PROVIDER })
    }
}

fn unavailable(e: TransportError) -> ProviderError {
    ProviderError::Unavailable {
        provider: PROVIDER,
        source: Box::new(e),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone as _, Utc};
    use domain::{testing::FixedClock, wire::Timestamp};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, query_param},
    };

    use super::*;

    const IOWA: &str = include_str!("../fixtures/soilgrids_iowa.json");
    const TORONTO: &str = include_str!("../fixtures/soilgrids_toronto.json");
    const GUELPH: &str = include_str!("../fixtures/soilgrids_guelph.json");

    #[test]
    fn converts_with_d_factor() {
        let r: QueryResponse = serde_json::from_str(IOWA).unwrap();
        let d = to_depths(&r).unwrap();
        let top = d[0];
        assert_eq!(top.depth_cm, DepthBand::D0To5);
        assert!((top.ph_h2o.unwrap() - 6.5).abs() < 1e-9);
        assert!((top.clay_pct.unwrap() - 29.1).abs() < 1e-9);
        assert!((top.nitrogen_g_kg.unwrap() - 3.39).abs() < 1e-9);
        assert!((top.bulk_density_kg_dm3.unwrap() - 1.35).abs() < 1e-9);
        assert!(d[5].ph_h2o.is_none(), "the fixture asked for three depths");
    }

    #[test]
    fn null_pixels_are_no_data() {
        for f in [TORONTO, GUELPH] {
            let r: QueryResponse = serde_json::from_str(f).unwrap();
            assert!(to_depths(&r).is_none());
        }
    }

    #[test]
    fn rings_are_about_the_right_distance() {
        let at = Coordinates::new(43.65, -79.38).unwrap();
        let pts = nearby_points(at, &[2.0], 4);
        assert_eq!(pts.len(), 4);
        assert!((pts[0].lat - at.lat - 2.0 / 111.32).abs() < 1e-9);
        let east_km = (pts[1].lon - at.lon) * 111.32 * at.lat.to_radians().cos();
        assert!((east_km - 2.0).abs() < 1e-6);
    }

    fn adapter(server: &MockServer) -> SoilGrids {
        SoilGrids::new(
            reqwest::Client::new(),
            SoilGridsConfig {
                query_url: server.uri(),
                min_interval: Duration::ZERO,
                rings_km: vec![2.0],
                ..SoilGridsConfig::default()
            },
            Arc::new(FixedClock::at(Timestamp::from(
                Utc.with_ymd_and_hms(2026, 10, 15, 12, 0, 0).unwrap(),
            ))),
        )
    }

    #[tokio::test]
    async fn falls_back_to_a_nearby_point() {
        let server = MockServer::start().await;
        // The garden's own pixel: no data.
        Mock::given(method("GET"))
            .and(query_param("lat", "43.65000"))
            .respond_with(ResponseTemplate::new(200).set_body_string(TORONTO))
            .mount(&server)
            .await;
        // Every other point has data.
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(IOWA))
            .mount(&server)
            .await;
        let p = adapter(&server)
            .soil_profile(Coordinates::new(43.65, -79.38).unwrap())
            .await
            .unwrap();
        assert_eq!(p.depths.len(), 6);
        assert!(p.depths[0].ph_h2o.is_some());
    }

    #[tokio::test]
    async fn all_null_everywhere_is_not_found() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(TORONTO))
            .expect(5) // the point, then 4 probes
            .mount(&server)
            .await;
        let err = adapter(&server)
            .soil_profile(Coordinates::new(43.65, -79.38).unwrap())
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            ProviderError::NotFound {
                provider: "soilgrids"
            }
        ));
    }

    #[tokio::test]
    async fn server_errors_are_unavailable() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(502))
            .mount(&server)
            .await;
        let err = adapter(&server)
            .soil_profile(Coordinates::new(42.0, -93.6).unwrap())
            .await
            .unwrap_err();
        assert!(matches!(err, ProviderError::Unavailable { .. }));
    }
}
