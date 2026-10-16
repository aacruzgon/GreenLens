//! Gardens and their location ([contracts 9], gardens).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::DomainError,
    id::GardenId,
    records::weather::{Coordinates, LocationPrecision, WeatherSummary},
    wire::{Role, Timestamp},
};

/// `GardenLocationInput.precision`: what the phone's location consent allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConsentPrecision {
    /// Exact GPS.
    Exact,
    /// Rounded GPS.
    #[serde(rename = "about_1_km")]
    About1Km,
}

impl From<ConsentPrecision> for LocationPrecision {
    fn from(p: ConsentPrecision) -> Self {
        match p {
            ConsentPrecision::Exact => Self::Exact,
            ConsentPrecision::About1Km => Self::About1Km,
        }
    }
}

/// `GardenLocationInput` ([contracts 9]): exactly one of `lat` and `lon`, `postal_code`, `city`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GardenLocationInput {
    /// With location consent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub lat: Option<f64>,
    /// With location consent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub lon: Option<f64>,
    /// With `lat` and `lon`; `exact` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub precision: Option<ConsentPrecision>,
    /// Without consent; geocoded once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub postal_code: Option<String>,
    /// Without consent, instead of a postal code: "Guelph, ON"; geocoded once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub city: Option<String>,
}

/// A garden location after "exactly one of" is checked: what the geocoder resolves.
#[derive(Debug, Clone, PartialEq)]
pub enum LocationQuery {
    /// GPS coordinates with consent.
    Coordinates {
        /// Where.
        at: Coordinates,
        /// How precise.
        precision: ConsentPrecision,
    },
    /// A postal code.
    PostalCode(String),
    /// A city.
    City(String),
}

impl LocationQuery {
    /// The `location_precision` the garden gets.
    pub fn precision(&self) -> LocationPrecision {
        match self {
            Self::Coordinates { precision, .. } => (*precision).into(),
            Self::PostalCode(_) => LocationPrecision::PostalCode,
            Self::City(_) => LocationPrecision::City,
        }
    }
}

impl GardenLocationInput {
    /// Checks "exactly one of" and the coordinate ranges.
    pub fn query(self) -> Result<LocationQuery, DomainError> {
        let non_empty =
            |s: Option<String>| s.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
        match (
            self.lat,
            self.lon,
            non_empty(self.postal_code),
            non_empty(self.city),
        ) {
            (Some(lat), Some(lon), None, None) => Coordinates::new(lat, lon)
                .map(|at| LocationQuery::Coordinates {
                    at,
                    precision: self.precision.unwrap_or(ConsentPrecision::Exact),
                })
                .ok_or_else(|| DomainError::invalid("location.lat", "out of range")),
            (None, None, Some(code), None) => Ok(LocationQuery::PostalCode(code)),
            (None, None, None, Some(city)) => Ok(LocationQuery::City(city)),
            _ => Err(DomainError::invalid(
                "location",
                "exactly one of lat and lon, postal_code, city",
            )),
        }
    }
}

/// `Garden.soil_profile_status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SoilProfileStatus {
    /// `SoilGrids` is being fetched in the background.
    Pending,
    /// Fetched.
    Ready,
    /// `SoilGrids` failed; retried later.
    Unavailable,
}

/// An IANA time zone name, such as `"America/Toronto"`: sets every `LocalDate` of the garden.
/// Checked by the geocoder that derives it; `domain` keeps it as text so it needs no time-zone
/// database.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
pub struct TimeZone(pub String);

/// `Garden` ([contracts 9]). Raw coordinates are never returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Garden {
    /// The garden.
    pub id: GardenId,
    /// Its name.
    pub name: String,
    /// The caller's role.
    pub my_role: Role,
    /// "Toronto, ON".
    pub location_label: String,
    /// How precise the stored location is.
    pub location_precision: LocationPrecision,
    /// IANA, derived from the location.
    pub time_zone: TimeZone,
    /// `SoilGrids` progress.
    pub soil_profile_status: SoilProfileStatus,
    /// How many members.
    pub member_count: u32,
    /// When it was created.
    pub created_at: Timestamp,
}

/// `POST /gardens`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateGardenRequest {
    /// Its name.
    pub name: String,
    /// Its location.
    pub location: GardenLocationInput,
}

/// `PATCH /gardens/{id}`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateGardenRequest {
    /// A new name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub name: Option<String>,
    /// A new location.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub location: Option<GardenLocationInput>,
}

/// `200` of `GET /gardens/{id}/weather`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct GardenWeather {
    /// The garden's cache cell for the current hour.
    pub weather: WeatherSummary,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_one_location() {
        let both = GardenLocationInput {
            lat: Some(43.6),
            lon: Some(-79.4),
            precision: None,
            postal_code: Some("M5V".to_owned()),
            city: None,
        };
        assert!(both.query().is_err());
        let city = GardenLocationInput {
            lat: None,
            lon: None,
            precision: None,
            postal_code: None,
            city: Some("Guelph, ON".to_owned()),
        };
        assert_eq!(city.query().unwrap().precision(), LocationPrecision::City);
        let unknown = r#"{"lat":1,"lon":2,"altitude":3}"#;
        assert!(serde_json::from_str::<GardenLocationInput>(unknown).is_err());
    }
}
