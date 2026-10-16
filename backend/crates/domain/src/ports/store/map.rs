//! The garden map ([contracts 9], garden map; [database 4], garden map versions).

use crate::{
    authz::{Editor, Membership, Viewer},
    garden::map::{GardenMap, MapUpload},
    id::{GardenId, PlantId},
    ports::StoreError,
    wire::Timestamp,
};

/// A checked upload with the ids of the plants it creates, in pin order.
#[derive(Debug, Clone, PartialEq)]
pub struct MapWrite {
    /// The checked upload.
    pub upload: MapUpload,
    /// One id per `new_plant` pin, in order.
    pub new_plant_ids: Vec<PlantId>,
    /// Now.
    pub created_at: Timestamp,
}

/// The `garden_map` table and `plant.position`.
#[cfg_attr(feature = "mock", mockall::automock)]
#[async_trait::async_trait]
pub trait MapRepo: Send + Sync {
    /// `GET /gardens/{id}/map`: the current map, leaving out pins of plants being deleted.
    async fn map(&self, member: Membership<Viewer>) -> Result<Option<GardenMap>, StoreError>;

    /// `PUT /gardens/{id}/map`, in one transaction: checks `base_version` against
    /// `garden.map_version` (`Conflict::StaleMap` otherwise), drops pins of plants being
    /// deleted, refuses pins of plants not in the garden (`Rejected(UnknownPlants)` listing every
    /// one), creates the new plants, increments the version, writes the map, deletes older maps,
    /// and sets or unsets every plant's `position`.
    async fn put_map(
        &self,
        editor: Membership<Editor>,
        write: MapWrite,
    ) -> Result<GardenMap, StoreError>;

    /// The current map for the pipeline (neighbours, crowding) and `get_garden_map`.
    async fn current_map(&self, garden: GardenId) -> Result<Option<GardenMap>, StoreError>;
}
