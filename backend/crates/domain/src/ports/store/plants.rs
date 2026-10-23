//! Plants, the timeline and the growth series ([contracts 9], plants; [contracts 3], species
//! confirmation).

use crate::{
    authz::{Editor, Membership, Viewer},
    garden::plants::{GrowthQuantity, MeasurementPoint, TimelineEntry, UpdatePlantRequest},
    id::{GardenId, PlantId, ScanId, SpeciesId},
    ports::StoreError,
    scan::{Plant, PlantStatus, ScanStatus, SpeciesCandidate, SpeciesChoice, SpeciesStatus},
    url::Stored,
    wire::{Page, PageRequest, Run, Timestamp},
};

/// `POST /plants`, with its id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPlant {
    /// Its id.
    pub id: PlantId,
    /// Its name.
    pub name: String,
    /// Its species, confirmed when given.
    pub species: Option<SpeciesId>,
}

/// The plant's most recent scan, which may follow a species change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentScan {
    /// The scan.
    pub scan: ScanId,
    /// Its status.
    pub status: ScanStatus,
    /// When it was created.
    pub created_at: Timestamp,
    /// The species its run works with.
    pub working_species: Option<SpeciesId>,
}

/// What `POST /plants/{id}/species` stored, and the scan that may follow it
/// ([`crate::rules::scan_state::species_follow`]).
#[derive(Debug, Clone, PartialEq)]
pub struct SpeciesConfirmation {
    /// The updated plant.
    pub plant: Plant<Stored>,
    /// Its most recent live scan.
    pub recent_scan: Option<RecentScan>,
}

/// The `plant` table and the reads built on it.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait PlantRepo: Send + Sync {
    /// `GET /plants?garden_id=`, by name, with `latest_scan` computed.
    async fn plants(
        &self,
        member: Membership<Viewer>,
        page: PageRequest,
    ) -> Result<Page<Plant<Stored>>, StoreError>;

    /// `GET /plants/{id}`.
    async fn plant(
        &self,
        member: Membership<Viewer>,
        plant: PlantId,
    ) -> Result<Plant<Stored>, StoreError>;

    /// `POST /plants`.
    async fn create_plant(
        &self,
        editor: Membership<Editor>,
        plant: NewPlant,
    ) -> Result<Plant<Stored>, StoreError>;

    /// `PATCH /plants/{id}`.
    async fn update_plant(
        &self,
        editor: Membership<Editor>,
        plant: PlantId,
        patch: UpdatePlantRequest,
    ) -> Result<Plant<Stored>, StoreError>;

    /// `POST /plants/{id}/species`: sets the species (`confirmed`) or the custom name, and
    /// returns the most recent live scan for the follow-up decision.
    async fn confirm_species(
        &self,
        editor: Membership<Editor>,
        plant: PlantId,
        choice: SpeciesChoice,
    ) -> Result<SpeciesConfirmation, StoreError>;

    /// The pipeline's species suggestion for the scan's run `run`, in one transaction: the
    /// scan's plant gets the candidates and `suggested` with `working` as its species, unless
    /// already confirmed, and the scan's `working_species` becomes `working` unless the run
    /// already has one (a confirmed species, set when the run started). Returns `false`, and
    /// writes nothing, when `run` is no longer the scan's current run, or when the scan, its
    /// plant or its garden is being deleted (the scan is no longer visible).
    async fn suggest_species(
        &self,
        scan: ScanId,
        run: Run,
        candidates: Vec<SpeciesCandidate<Stored>>,
        working: Option<SpeciesId>,
        status: SpeciesStatus,
    ) -> Result<bool, StoreError>;

    /// A species confirmed while the scan is `queued` ([contracts 3]: the run uses the new
    /// species): the scan's run `run`, which started without it, takes the plant's confirmed
    /// species as its working species, read in the same transaction, so the latest
    /// confirmation wins (none for a custom species). Returns `false`, and writes nothing,
    /// under the same conditions as [`PlantRepo::suggest_species`], or when the plant's
    /// species is not confirmed.
    async fn adopt_confirmed_species(&self, scan: ScanId, run: Run) -> Result<bool, StoreError>;

    /// The plant's status after a scan's diagnosis.
    async fn set_plant_status(&self, plant: PlantId, status: PlantStatus)
    -> Result<(), StoreError>;

    /// `GET /plants/{id}/timeline`, newest first, skipping scans being deleted.
    async fn timeline(
        &self,
        member: Membership<Viewer>,
        plant: PlantId,
        page: PageRequest,
    ) -> Result<Page<TimelineEntry<Stored>>, StoreError>;

    /// `GET /plants/{id}/measurements`, oldest first: one point per scan and quantity, the
    /// highest-confidence current measurement of medium confidence or more, dated by the scan
    /// ([contracts 7], growth; index `observation_growth`).
    async fn growth_series(
        &self,
        member: Membership<Viewer>,
        plant: PlantId,
        quantity: Option<GrowthQuantity>,
        page: PageRequest,
    ) -> Result<Page<MeasurementPoint>, StoreError>;

    /// Every live plant of a garden (none being deleted), for the pipeline's neighbour list
    /// ([contracts 7], neighbours for the plan writer): names and species of the pinned plants.
    /// A system call with no membership proof, like `garden_context`.
    async fn garden_plants(&self, garden: GardenId) -> Result<Vec<Plant<Stored>>, StoreError>;

    /// The plant's whole height series, for fusion's `growth_stalled` rule.
    async fn heights(&self, plant: PlantId) -> Result<Vec<MeasurementPoint>, StoreError>;
}
