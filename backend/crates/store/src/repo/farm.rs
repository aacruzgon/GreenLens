//! Farm connectors and their records ([connectors 4]): the `connector` and `farm_record`
//! tables. The schema is in place; the repository methods are roadmap task E1.6 (docs/EON), so
//! until then each answers `StoreError::Query` naming the method, which `api` maps to `500`.
//! No route reaches them yet: the connector routes answer `409 invalid_state` until E3.
//!
//! [connectors 4]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#4-data

use domain::{
    authz::{Membership, Owner, Viewer},
    connectors::{Connector, ConnectorSource, ConnectorStatus, SyncErrorCode},
    id::{ConnectorId, FarmRecordId, GardenId, UserId},
    ports::{
        CompletedConnect, ConnectorRow, Embedding, FarmRecordFilter, FarmRecordMatch,
        FarmRecordStore, NewConnector, NewFarmRecord, StoreError, SyncOutcome,
    },
    wire::Timestamp,
};

use crate::SurrealStore;

/// E1.6: the method isn't implemented yet.
fn e1_6(what: &'static str) -> StoreError {
    StoreError::Query {
        what,
        source: "not implemented until roadmap task E1.6".into(),
    }
}

#[async_trait::async_trait]
impl FarmRecordStore for SurrealStore {
    async fn create_connector(
        &self,
        _owner: Membership<Owner>,
        _connector: NewConnector,
    ) -> Result<Connector, StoreError> {
        Err(e1_6("create connector"))
    }

    async fn seed_connector(
        &self,
        _garden: GardenId,
        _connector: NewConnector,
    ) -> Result<Connector, StoreError> {
        Err(e1_6("seed connector"))
    }

    async fn connectors(&self, _member: Membership<Viewer>) -> Result<Vec<Connector>, StoreError> {
        Err(e1_6("list connectors"))
    }

    async fn connector_row(
        &self,
        _connector: ConnectorId,
    ) -> Result<Option<ConnectorRow>, StoreError> {
        Err(e1_6("read connector"))
    }

    async fn complete_connect(
        &self,
        _owner: Membership<Owner>,
        _connector: ConnectorId,
        _done: CompletedConnect,
    ) -> Result<Connector, StoreError> {
        Err(e1_6("complete connect"))
    }

    async fn set_sources(
        &self,
        _owner: Membership<Owner>,
        _connector: ConnectorId,
        _sources: Vec<ConnectorSource>,
        _now: Timestamp,
    ) -> Result<Connector, StoreError> {
        Err(e1_6("set connector sources"))
    }

    async fn request_sync(
        &self,
        _owner: Membership<Owner>,
        _connector: ConnectorId,
        _now: Timestamp,
    ) -> Result<bool, StoreError> {
        Err(e1_6("request sync"))
    }

    async fn due_connectors(
        &self,
        _now: Timestamp,
        _limit: u32,
    ) -> Result<Vec<ConnectorRow>, StoreError> {
        Err(e1_6("due connectors"))
    }

    async fn finish_sync(
        &self,
        _connector: ConnectorId,
        _outcome: SyncOutcome,
    ) -> Result<(), StoreError> {
        Err(e1_6("finish sync"))
    }

    async fn set_status(
        &self,
        _connector: ConnectorId,
        _status: ConnectorStatus,
        _last_error: Option<SyncErrorCode>,
    ) -> Result<(), StoreError> {
        Err(e1_6("set connector status"))
    }

    async fn start_connector_deletion(
        &self,
        _owner: Membership<Owner>,
        _connector: ConnectorId,
    ) -> Result<ConnectorRow, StoreError> {
        Err(e1_6("start connector deletion"))
    }

    async fn delete_connector_batch(&self, _connector: ConnectorId) -> Result<u32, StoreError> {
        Err(e1_6("delete connector batch"))
    }

    async fn connectors_created_by(
        &self,
        _garden: GardenId,
        _user: UserId,
    ) -> Result<Vec<ConnectorId>, StoreError> {
        Err(e1_6("connectors created by"))
    }

    async fn delete_expired_pending(&self, _before: Timestamp) -> Result<u32, StoreError> {
        Err(e1_6("delete expired pending connectors"))
    }

    async fn record_hashes(
        &self,
        _connector: ConnectorId,
        _vendor_ids: Vec<String>,
    ) -> Result<Vec<(FarmRecordId, String)>, StoreError> {
        Err(e1_6("farm record hashes"))
    }

    async fn upsert_records(
        &self,
        _connector: ConnectorId,
        _records: Vec<NewFarmRecord>,
    ) -> Result<u32, StoreError> {
        Err(e1_6("upsert farm records"))
    }

    async fn delete_records(
        &self,
        _connector: ConnectorId,
        _vendor_ids: Vec<String>,
    ) -> Result<u32, StoreError> {
        Err(e1_6("delete farm records"))
    }

    async fn delete_source_records(
        &self,
        _connector: ConnectorId,
        _source: String,
    ) -> Result<u32, StoreError> {
        Err(e1_6("delete a source's farm records"))
    }

    async fn garden_record_count(&self, _garden: GardenId) -> Result<u32, StoreError> {
        Err(e1_6("garden farm record count"))
    }

    async fn search_records(
        &self,
        _member: Membership<Viewer>,
        _query: &Embedding,
        _filter: FarmRecordFilter,
        _k: usize,
    ) -> Result<Vec<FarmRecordMatch>, StoreError> {
        Err(e1_6("search farm records"))
    }

    async fn delete_records_before(&self, _cutoff: Timestamp) -> Result<u32, StoreError> {
        Err(e1_6("delete old farm records"))
    }
}
