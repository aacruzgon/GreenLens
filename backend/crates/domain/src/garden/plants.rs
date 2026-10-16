//! Plants, the timeline and the growth series ([contracts 9], plants).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    chat::ProblemRef,
    id::{GardenId, ImageId, PlanId, ScanId, SpeciesId},
    records::diagnosis::DiagnosisStatus,
    scan::{PlantStatus, ScanFailureCode, ScanStatus},
    url::{ImagePath, Sign, Signed, SignedUrl, Signer, Stored, UrlState, url_absent},
    wire::{ConfidenceBand, Timestamp},
};

/// `GET /plants?garden_id=`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PlantsQuery {
    /// The garden.
    pub garden_id: GardenId,
}

/// `POST /plants`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatePlantRequest {
    /// The garden.
    pub garden_id: GardenId,
    /// Its name.
    pub name: String,
    /// Its species, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub species_id: Option<SpeciesId>,
}

/// `PATCH /plants/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdatePlantRequest {
    /// A new name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub name: Option<String>,
    /// A new status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub status: Option<PlantStatus>,
}

/// `TimelineEntry.measurements`: medium confidence or more.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, ToSchema)]
pub struct TimelineMeasurements {
    /// cm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plant_height_cm: Option<f64>,
    /// cm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub canopy_width_cm: Option<f64>,
}

/// `TimelineEntry` ([contracts 9]): newest first, skipping scans being deleted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = TimelineEntry)]
pub struct TimelineEntry<U: UrlState = Signed> {
    /// The scan.
    pub scan_id: ScanId,
    /// When it was created.
    pub created_at: Timestamp,
    /// Its status.
    pub status: ScanStatus,
    /// The current diagnosis' status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub diagnosis_status: Option<DiagnosisStatus>,
    /// The primary problem.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub primary: Option<ProblemRef>,
    /// The `whole_plant` shot, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub thumbnail_image_id: Option<ImageId>,
    /// Signed URL of that shot.
    #[serde(default, skip_serializing_if = "url_absent")]
    #[schema(nullable = false, value_type = Option<SignedUrl>)]
    pub thumbnail_url: Option<U::Url>,
    /// With status `failed`: "Failed: no plant found".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub failure_code: Option<ScanFailureCode>,
    /// The current plan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub plan_id: Option<PlanId>,
    /// Height and width, medium confidence or more.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub measurements: Option<TimelineMeasurements>,
}

impl Sign for TimelineEntry<Stored> {
    type Signed = TimelineEntry<Signed>;
    type Scope = ();
    fn sign(self, (): (), signer: &Signer<'_>) -> TimelineEntry<Signed> {
        TimelineEntry {
            thumbnail_url: self.thumbnail_image_id.map(|image| {
                signer.url(&ImagePath::ScanImage {
                    scan: self.scan_id,
                    image,
                })
            }),
            scan_id: self.scan_id,
            created_at: self.created_at,
            status: self.status,
            diagnosis_status: self.diagnosis_status,
            primary: self.primary,
            thumbnail_image_id: self.thumbnail_image_id,
            failure_code: self.failure_code,
            plan_id: self.plan_id,
            measurements: self.measurements,
        }
    }
}

/// `MeasurementPoint.quantity`: the growth quantities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GrowthQuantity {
    /// Plant height.
    PlantHeightCm,
    /// Canopy width.
    CanopyWidthCm,
}

/// `MeasurementPoint` ([contracts 9]): one per scan and quantity, oldest first.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MeasurementPoint {
    /// The scan.
    pub scan_id: ScanId,
    /// The scan's `created_at`, when the photos were taken.
    pub created_at: Timestamp,
    /// Which quantity.
    pub quantity: GrowthQuantity,
    /// cm.
    pub value: f64,
    /// Medium or high; low never enters the series.
    pub confidence_band: ConfidenceBand,
}

/// `GET /plants/{id}/measurements?quantity=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MeasurementsQuery {
    /// One quantity, or both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<GrowthQuantity>,
}
