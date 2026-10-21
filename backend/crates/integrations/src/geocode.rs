//! The [`Geocoder`] (architecture 8.5; [contracts 9], `GardenLocationInput`): coordinates, a
//! label such as "Toronto, ON" and the IANA time zone, from GPS, a postal code or a city.
//!
//! | Input | Coordinates | Label | Time zone |
//! |---|---|---|---|
//! | GPS | the fix, rounded per precision | Nominatim reverse at 0.01°; the time zone's city when it fails | `tzf-rs`, offline |
//! | Canadian postal code | Nominatim search (`postalcode=`), then Zippopotam by FSA | Nominatim / Zippopotam | `tzf-rs` |
//! | US ZIP | Open-Meteo geocoding | Open-Meteo | Open-Meteo |
//! | City | Open-Meteo geocoding | Open-Meteo | Open-Meteo, else `tzf-rs` |
//!
//! Open-Meteo's geocoder does not resolve Canadian postal codes (every form tried on
//! 2026-10-15 returned nothing), hence Nominatim. Nominatim's policy: at most 1 request a
//! second, an identifying user agent, results cached, attribution (`ODbL`). Every answer is
//! cached here for 30 days, keyed by the normalised query.
//!
//! A GPS garden is never refused for its label: when the reverse lookup fails (an outage, a
//! `429`, or no place at the point), the label is the time zone's city, "Toronto time zone",
//! and that answer isn't cached, so the next lookup at the point asks Nominatim again.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use domain::{
    garden::gardens::TimeZone,
    ports::{GeocodeQuery, Geocoded, Geocoder, ProviderError},
    records::weather::{Coordinates, LocationPrecision},
};
use moka::future::Cache;
use serde::Deserialize;

use crate::http::{self, Pacer, TransportError};

/// The `upstream` name.
pub const PROVIDER: &str = "geocoder";

/// Settings.
#[derive(Debug, Clone)]
pub struct GeocoderConfig {
    /// `https://geocoding-api.open-meteo.com/v1/search`.
    pub open_meteo_url: String,
    /// `https://nominatim.openstreetmap.org`.
    pub nominatim_url: String,
    /// `https://api.zippopotam.us`; `None` disables the FSA fallback.
    pub zippopotam_url: Option<String>,
    /// ISO country for city searches: `CA`.
    pub country_code: String,
    /// Total deadline per request.
    pub timeout: Duration,
    /// Nominatim spacing: 1 s.
    pub nominatim_interval: Duration,
    /// How long an answer is cached.
    pub cache_ttl: Duration,
    /// Most cached answers.
    pub cache_capacity: u64,
}

impl Default for GeocoderConfig {
    fn default() -> Self {
        Self {
            open_meteo_url: "https://geocoding-api.open-meteo.com/v1/search".to_owned(),
            nominatim_url: "https://nominatim.openstreetmap.org".to_owned(),
            zippopotam_url: Some("https://api.zippopotam.us".to_owned()),
            country_code: "CA".to_owned(),
            timeout: Duration::from_secs(10),
            nominatim_interval: Duration::from_secs(1),
            cache_ttl: Duration::from_hours(720),
            cache_capacity: 10_000,
        }
    }
}

/// Offline coordinates → IANA zone, from `tzf-rs`'s bundled boundaries. Building it loads the
/// data (a few megabytes), so it is built once at start and shared.
pub struct TimeZoneFinder(tzf_rs::DefaultFinder);

impl std::fmt::Debug for TimeZoneFinder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TimeZoneFinder")
    }
}

impl Default for TimeZoneFinder {
    fn default() -> Self {
        Self(tzf_rs::DefaultFinder::new())
    }
}

impl TimeZoneFinder {
    /// The zone at a point; `None` where no zone covers it (open ocean).
    pub fn zone(&self, at: Coordinates) -> Option<TimeZone> {
        let name = self.0.get_tz_name(at.lon, at.lat); // (lng, lat) order
        (!name.is_empty()).then(|| TimeZone(name.to_owned()))
    }
}

/// The coordinates stored for a precision ([contracts 9]): `exact` as given, `about_1_km`
/// rounded to 0.01°, `about_10_km` to 0.1°. Geocoded postal codes and cities are stored as
/// geocoded.
pub fn round_for(at: Coordinates, precision: LocationPrecision) -> Coordinates {
    match precision {
        LocationPrecision::About1Km => Coordinates {
            lat: (at.lat * 100.0).round() / 100.0,
            lon: (at.lon * 100.0).round() / 100.0,
        },
        LocationPrecision::About10Km => at.round_to_10_km(),
        LocationPrecision::Exact | LocationPrecision::PostalCode | LocationPrecision::City => at,
    }
}

/// A postal code, normalised and classified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostalCode {
    /// `M5V3L9`: a full Canadian code, upper case, no space.
    Canadian(String),
    /// `M5V`: a forward sortation area only.
    CanadianFsa(String),
    /// `90210`.
    UsZip(String),
}

impl PostalCode {
    /// Classifies `" m5v 3l9 "` and friends; `None` for anything else.
    pub fn parse(raw: &str) -> Option<Self> {
        let s: String = raw
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .map(|c| c.to_ascii_uppercase())
            .collect();
        let b = s.as_bytes();
        let ana = |i: usize| {
            b[i].is_ascii_alphabetic()
                && b[i + 1].is_ascii_digit()
                && b[i + 2].is_ascii_alphabetic()
        };
        let nan = |i: usize| {
            b[i].is_ascii_digit() && b[i + 1].is_ascii_alphabetic() && b[i + 2].is_ascii_digit()
        };
        match b.len() {
            6 if ana(0) && nan(3) => Some(Self::Canadian(s)),
            3 if ana(0) => Some(Self::CanadianFsa(s)),
            5 | 9 if b.iter().all(u8::is_ascii_digit) => Some(Self::UsZip(s[..5].to_owned())),
            _ => None,
        }
    }
}

/// "Ontario" → "ON", for labels built from Open-Meteo's full `admin1` names.
pub fn province_code(admin1: &str) -> Option<&'static str> {
    Some(match admin1 {
        "Ontario" => "ON",
        "Quebec" | "Québec" => "QC",
        "British Columbia" => "BC",
        "Alberta" => "AB",
        "Manitoba" => "MB",
        "Saskatchewan" => "SK",
        "Nova Scotia" => "NS",
        "New Brunswick" => "NB",
        "Newfoundland and Labrador" => "NL",
        "Prince Edward Island" => "PE",
        "Northwest Territories" => "NT",
        "Yukon" => "YT",
        "Nunavut" => "NU",
        _ => return None,
    })
}

// ---------------------------------------------------------------------------------------------
// Upstream shapes (observed live 2026-10-15)
// ---------------------------------------------------------------------------------------------

/// Open-Meteo `search`. No `results` key at all when nothing matches.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SearchResponse {
    /// Matches, best first.
    #[serde(default)]
    pub results: Vec<Place>,
}

/// One Open-Meteo match; empty fields are omitted.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Place {
    /// "Toronto".
    pub name: String,
    /// Degrees.
    pub latitude: f64,
    /// Degrees.
    pub longitude: f64,
    /// `CA`.
    #[serde(default)]
    pub country_code: Option<String>,
    /// "Ontario".
    #[serde(default)]
    pub admin1: Option<String>,
    /// "America/Toronto".
    #[serde(default)]
    pub timezone: Option<String>,
}

impl Place {
    /// "Toronto, ON"; the full `admin1` when it isn't a Canadian province.
    pub fn label(&self) -> String {
        match self.admin1.as_deref() {
            Some(a) => format!("{}, {}", self.name, province_code(a).unwrap_or(a)),
            None => self.name.clone(),
        }
    }
}

/// One Nominatim result (`format=jsonv2&addressdetails=1`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct NominatimPlace {
    /// Degrees, as a string.
    pub lat: String,
    /// Degrees, as a string.
    pub lon: String,
    /// The address parts.
    #[serde(default)]
    pub address: NominatimAddress,
}

/// `address`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct NominatimAddress {
    /// City.
    pub city: Option<String>,
    /// Town.
    pub town: Option<String>,
    /// Village.
    pub village: Option<String>,
    /// Municipality.
    pub municipality: Option<String>,
    /// County: the only place name at some rural points ("Niagara County").
    pub county: Option<String>,
    /// `CA-ON`.
    #[serde(rename = "ISO3166-2-lvl4")]
    pub iso_lvl4: Option<String>,
    /// "Ontario".
    pub state: Option<String>,
}

impl NominatimAddress {
    /// "Toronto, ON".
    pub fn label(&self) -> Option<String> {
        let place = self
            .city
            .as_ref()
            .or(self.town.as_ref())
            .or(self.village.as_ref())
            .or(self.municipality.as_ref())
            .or(self.county.as_ref())?;
        let region = self
            .iso_lvl4
            .as_deref()
            .and_then(|c| c.split_once('-'))
            .map(|(_, r)| r.to_owned())
            .or_else(|| self.state.clone());
        Some(match region {
            Some(r) => format!("{place}, {r}"),
            None => place.clone(),
        })
    }
}

/// Nominatim `reverse`: a place, or `{"error": "Unable to geocode"}` with `200` where none
/// is near (open water, observed live 2026-10-20).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum NominatimReverse {
    /// The place at the point.
    Place(NominatimPlace),
    /// No place.
    Error {
        /// "Unable to geocode".
        error: String,
    },
}

/// The label of a garden whose reverse lookup failed: the city that names its time zone,
/// "America/Toronto" → "Toronto time zone". It names the zone, not the town: an Ottawa garden
/// gets "Toronto time zone" too, so it never claims a place it can't vouch for.
pub fn zone_label(zone: &TimeZone) -> String {
    let city = zone
        .0
        .rsplit('/')
        .next()
        .unwrap_or(&zone.0)
        .replace('_', " ");
    format!("{city} time zone")
}

/// Zippopotam `/ca/{fsa}`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ZipResponse {
    /// The places in the area.
    pub places: Vec<ZipPlace>,
}

/// One Zippopotam place.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ZipPlace {
    /// "Downtown Toronto (CN Tower / …)".
    #[serde(rename = "place name")]
    pub place_name: String,
    /// Degrees, as a string.
    pub latitude: String,
    /// Degrees, as a string.
    pub longitude: String,
    /// `ON`.
    #[serde(rename = "state abbreviation")]
    pub state_abbreviation: String,
}

// ---------------------------------------------------------------------------------------------
// The adapter
// ---------------------------------------------------------------------------------------------

/// The composite geocoder.
#[derive(Clone)]
pub struct CompositeGeocoder {
    http: reqwest::Client,
    config: Arc<GeocoderConfig>,
    zones: Arc<TimeZoneFinder>,
    nominatim: Arc<Pacer>,
    cache: Cache<String, Geocoded>,
}

impl std::fmt::Debug for CompositeGeocoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeGeocoder")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl CompositeGeocoder {
    /// A geocoder over the shared client and the shared zone finder.
    pub fn new(http: reqwest::Client, config: GeocoderConfig, zones: Arc<TimeZoneFinder>) -> Self {
        let cache = Cache::builder()
            .max_capacity(config.cache_capacity)
            .time_to_live(config.cache_ttl)
            .build();
        Self {
            http,
            nominatim: Arc::new(Pacer::new(config.nominatim_interval)),
            config: Arc::new(config),
            zones,
            cache,
        }
    }

    /// GETs JSON. `T: DeserializeOwned` because the bytes are a local buffer.
    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        query: &[(&str, &str)],
    ) -> Result<Option<T>, ProviderError> {
        let response = self
            .http
            .get(url)
            .query(query)
            .timeout(self.config.timeout)
            .send()
            .await
            .map_err(|e| unavailable(TransportError::from_reqwest(e)))?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimited {
                provider: PROVIDER,
                retry_after_s: http::retry_after(response.headers())
                    .and_then(|d| u32::try_from(d.as_secs()).ok()),
            });
        }
        if !status.is_success() {
            let source = format!("HTTP {status} from {url}").into();
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
        let bytes = response
            .bytes()
            .await
            .map_err(|e| unavailable(TransportError::from_reqwest(e)))?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| ProviderError::BadResponse {
                provider: PROVIDER,
                source: Box::new(e),
            })
    }

    fn zone(&self, at: Coordinates) -> Result<TimeZone, ProviderError> {
        self.zones
            .zone(at)
            .ok_or(ProviderError::NotFound { provider: PROVIDER })
    }

    async fn open_meteo(&self, name: &str, country: &str) -> Result<Geocoded, ProviderError> {
        let response: Option<SearchResponse> = self
            .get_json(
                &self.config.open_meteo_url,
                &[
                    ("name", name),
                    ("count", "1"),
                    ("language", "en"),
                    ("format", "json"),
                    ("countryCode", country),
                ],
            )
            .await?;
        let place = response
            .and_then(|r| r.results.into_iter().next())
            .ok_or(ProviderError::NotFound { provider: PROVIDER })?;
        let coordinates = Coordinates::new(place.latitude, place.longitude)
            .ok_or_else(|| bad("coordinates out of range"))?;
        let time_zone = match place.timezone.clone() {
            Some(tz) => TimeZone(tz),
            None => self.zone(coordinates)?,
        };
        Ok(Geocoded {
            coordinates,
            label: place.label(),
            time_zone,
        })
    }

    async fn nominatim_search(&self, code: &str) -> Result<Option<Geocoded>, ProviderError> {
        self.nominatim.wait().await;
        let url = format!("{}/search", self.config.nominatim_url.trim_end_matches('/'));
        let places: Option<Vec<NominatimPlace>> = self
            .get_json(
                &url,
                &[
                    ("postalcode", code),
                    ("country", "CA"),
                    ("format", "jsonv2"),
                    ("addressdetails", "1"),
                    ("limit", "1"),
                ],
            )
            .await?;
        let Some(place) = places.and_then(|p| p.into_iter().next()) else {
            return Ok(None);
        };
        let coordinates = parse_point(&place.lat, &place.lon)?;
        let label = place.address.label().unwrap_or_else(|| code.to_owned());
        Ok(Some(Geocoded {
            coordinates,
            label,
            time_zone: self.zone(coordinates)?,
        }))
    }

    async fn nominatim_reverse(&self, at: Coordinates) -> Result<String, ProviderError> {
        self.nominatim.wait().await;
        let url = format!(
            "{}/reverse",
            self.config.nominatim_url.trim_end_matches('/')
        );
        let (lat, lon) = (format!("{:.2}", at.lat), format!("{:.2}", at.lon));
        let place: Option<NominatimReverse> = self
            .get_json(
                &url,
                &[
                    ("lat", &lat),
                    ("lon", &lon),
                    ("zoom", "10"),
                    ("format", "jsonv2"),
                    ("addressdetails", "1"),
                ],
            )
            .await?;
        match place {
            Some(NominatimReverse::Place(p)) => p.address.label(),
            Some(NominatimReverse::Error { .. }) | None => None,
        }
        .ok_or(ProviderError::NotFound { provider: PROVIDER })
    }

    async fn zippopotam(&self, fsa: &str) -> Result<Option<Geocoded>, ProviderError> {
        let Some(base) = &self.config.zippopotam_url else {
            return Ok(None);
        };
        let url = format!("{}/ca/{fsa}", base.trim_end_matches('/'));
        let response: Option<ZipResponse> = self.get_json(&url, &[]).await?;
        let Some(place) = response.and_then(|r| r.places.into_iter().next()) else {
            return Ok(None);
        };
        let coordinates = parse_point(&place.latitude, &place.longitude)?;
        let name = place
            .place_name
            .split(" (")
            .next()
            .unwrap_or(&place.place_name)
            .to_owned();
        Ok(Some(Geocoded {
            coordinates,
            label: format!("{name}, {}", place.state_abbreviation),
            time_zone: self.zone(coordinates)?,
        }))
    }

    async fn postal_code(&self, code: &PostalCode) -> Result<Geocoded, ProviderError> {
        let not_found = ProviderError::NotFound { provider: PROVIDER };
        match code {
            PostalCode::Canadian(full) => {
                if let Some(g) = self.nominatim_search(full).await? {
                    return Ok(g);
                }
                self.zippopotam(&full[..3]).await?.ok_or(not_found)
            }
            PostalCode::CanadianFsa(fsa) => self.zippopotam(fsa).await?.ok_or(not_found),
            PostalCode::UsZip(zip) => self.open_meteo(zip, "US").await,
        }
    }
}

#[async_trait]
impl Geocoder for CompositeGeocoder {
    async fn geocode<'a>(&self, query: GeocodeQuery<'a>) -> Result<Geocoded, ProviderError> {
        let key = match query {
            GeocodeQuery::Coordinates(c) => format!("gps:{:.2},{:.2}", c.lat, c.lon),
            GeocodeQuery::PostalCode(p) => match PostalCode::parse(p) {
                Some(
                    PostalCode::Canadian(s) | PostalCode::CanadianFsa(s) | PostalCode::UsZip(s),
                ) => {
                    format!("postal:{s}")
                }
                None => return Err(ProviderError::NotFound { provider: PROVIDER }),
            },
            GeocodeQuery::City(c) => format!("city:{}", c.trim().to_lowercase()),
        };
        if let Some(hit) = self.cache.get(&key).await {
            return Ok(match query {
                // The cache keys GPS at 0.01°; the caller's own point is returned.
                GeocodeQuery::Coordinates(c) => Geocoded {
                    coordinates: c,
                    ..hit
                },
                _ => hit,
            });
        }
        let found = match query {
            GeocodeQuery::Coordinates(c) => {
                let time_zone = self.zone(c)?;
                // Only a 0.01° point leaves the server: the label is city-level anyway.
                match self
                    .nominatim_reverse(round_for(c, LocationPrecision::About1Km))
                    .await
                {
                    Ok(label) => Geocoded {
                        coordinates: c,
                        label,
                        time_zone,
                    },
                    Err(error) => {
                        // No location in the log: the error names the provider and the status.
                        tracing::warn!(provider = PROVIDER, error = %error, "reverse geocoding failed; labelled by the time zone");
                        // Not cached, so the next lookup here asks Nominatim again.
                        return Ok(Geocoded {
                            coordinates: c,
                            label: zone_label(&time_zone),
                            time_zone,
                        });
                    }
                }
            }
            GeocodeQuery::PostalCode(p) => {
                let code =
                    PostalCode::parse(p).ok_or(ProviderError::NotFound { provider: PROVIDER })?;
                self.postal_code(&code).await?
            }
            GeocodeQuery::City(c) => self.open_meteo(c.trim(), &self.config.country_code).await?,
        };
        self.cache.insert(key, found.clone()).await;
        Ok(found)
    }
}

fn parse_point(lat: &str, lon: &str) -> Result<Coordinates, ProviderError> {
    let lat = lat.parse::<f64>().map_err(|e| bad_source(Box::new(e)))?;
    let lon = lon.parse::<f64>().map_err(|e| bad_source(Box::new(e)))?;
    Coordinates::new(lat, lon).ok_or_else(|| bad("coordinates out of range"))
}

fn unavailable(e: TransportError) -> ProviderError {
    ProviderError::Unavailable {
        provider: PROVIDER,
        source: Box::new(e),
    }
}

fn bad(msg: &'static str) -> ProviderError {
    bad_source(msg.into())
}

fn bad_source(source: domain::ports::BoxError) -> ProviderError {
    ProviderError::BadResponse {
        provider: PROVIDER,
        source,
    }
}

#[cfg(test)]
mod tests;
