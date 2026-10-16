//! Test support: a fresh embedded `SurrealDB` 3.3.4 (`kv-mem`) per test, with the schema applied,
//! and fixtures built through the store's own methods.

use std::sync::Arc;

use domain::{
    authz::{Editor, Membership, check_membership_of},
    garden::gardens::TimeZone,
    id::{AuthId, GardenId, PlantId, ScanId, UserId},
    ports::{
        GardenRepo, IdGenExt, IdentityRepo, NewGarden, NewScan, NewScanPlant, ResolvedLocation,
        ScanRepo,
    },
    records::weather::{Coordinates, LocationPrecision},
    scan::NewPlantInput,
    testing::SequentialIds,
    wire::Timestamp,
};
use store::{SurrealStore, apply_schema_with};
use surrealdb::engine::any;

/// A store over a new in-memory datastore with `schema.surql` applied. Each call is its own
/// datastore, so tests never share rows.
pub async fn store() -> Arc<SurrealStore> {
    let db = any::connect("mem://").await.expect("embedded engine");
    apply_schema_with(&db).await.expect("schema applies");
    Arc::new(
        SurrealStore::from_client(db)
            .await
            .expect("select pdm/main"),
    )
}

/// Runs raw `SurrealQL` for a fixture, failing the test on any statement error.
pub async fn exec(store: &SurrealStore, sql: &str) -> surrealdb::IndexedResults {
    let mut results = store.db().query(sql).await.expect("query sent");
    let errors = results.take_errors();
    assert!(errors.is_empty(), "fixture failed: {errors:?}");
    results
}

/// The test clock's start.
pub fn t0() -> Timestamp {
    "2026-10-18T14:00:00Z".parse().expect("a valid timestamp")
}

/// A garden with one editor-or-better member, one plant and one scan.
pub struct Fixture {
    pub ids: SequentialIds,
    pub user: UserId,
    pub garden: GardenId,
    pub plant: PlantId,
    pub scan: ScanId,
    pub editor: Membership<Editor>,
}

/// Builds a [`Fixture`] through `upsert_user`, `create_garden`, the membership check and
/// `create_scan`, so those paths are exercised by every test.
pub async fn fixture(store: &SurrealStore) -> Fixture {
    let ids = SequentialIds::default();
    let auth: AuthId = "user_2abc".parse().expect("auth id");
    let user = store
        .upsert_user(&auth, Some("Ada".into()))
        .await
        .expect("user");
    assert_eq!(
        store.upsert_user(&auth, None).await.expect("again"),
        user,
        "read first"
    );
    let garden: GardenId = ids.id();
    store
        .create_garden(
            user,
            NewGarden {
                id: garden,
                name: "Back yard".into(),
                location: ResolvedLocation {
                    coordinates: Coordinates {
                        lat: 43.65,
                        lon: -79.38,
                    },
                    precision: LocationPrecision::About1Km,
                    label: "Toronto, ON".into(),
                    time_zone: TimeZone("America/Toronto".into()),
                },
            },
        )
        .await
        .expect("garden");
    let editor: Membership<Editor> = check_membership_of(store, user, garden)
        .await
        .expect("owner");
    let plant: PlantId = ids.id();
    let scan: ScanId = ids.id();
    store
        .create_scan(
            editor,
            NewScan {
                id: scan,
                plant: NewScanPlant::New {
                    id: plant,
                    plant: NewPlantInput {
                        name: "Tomato".into(),
                        species_id: None,
                    },
                },
                created_by: user,
                created_at: t0(),
            },
        )
        .await
        .expect("scan");
    Fixture {
        ids,
        user,
        garden,
        plant,
        scan,
        editor,
    }
}

/// Moves the fixture's scan to `status` with run 1, as a submit would have.
pub async fn set_status(store: &SurrealStore, scan: ScanId, status: &str) {
    exec(
        store,
        &format!("UPDATE type::record('scan', '{scan}') SET status = '{status}', run = 1;"),
    )
    .await;
}

/// `type::record('<table>', '<id>')` for raw fixture SQL: sequential test ULIDs are all digits,
/// which a bare `table:000…1` literal would read as a number key.
pub fn rec(table: &str, id: impl std::fmt::Display) -> String {
    format!("type::record('{table}', '{id}')")
}
