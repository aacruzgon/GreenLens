//! Connecting, signing in, and applying the schema and the migrations ([database 1],
//! [database 6]).

use std::{sync::Arc, time::Duration};

use domain::ports::{IdGen, StoreError};
use surrealdb::{
    Surreal,
    engine::any::{self, Any},
    opt::auth::{Database, Root},
};
use tokio::sync::Notify;

use crate::{
    error::classify,
    migrate::{Applied, KnowledgeMeta, MIGRATIONS, MigrationError, knowledge_meta, migrate_with},
    tx::run_tx,
};

/// The schema, embedded at build time. The file is `backend/migrations/schema.surql` ([database 1],
/// applying the schema). Idempotent: `OVERWRITE`
/// for tables, fields and functions, `IF NOT EXISTS` for indexes.
pub const SCHEMA: &str = include_str!("../../../migrations/schema.surql");

/// The namespace and database the schema defines and selects.
pub const NAMESPACE: &str = "pdm";
/// See [`NAMESPACE`].
pub const DATABASE: &str = "main";

/// How the API reaches `SurrealDB`.
#[derive(Debug, Clone)]
pub struct StoreConfig {
    /// `ws://surrealdb:8000` on the Docker network.
    pub url: String,
    /// The database-level user, `api` ([database 1], users).
    pub username: String,
    /// Its password, from the environment.
    pub password: String,
}

/// The store: one `SurrealDB` client for the process, behind `Arc<dyn Store>` in `api`.
///
/// It is not `Clone`: in 3.x a clone of `Surreal<Any>` opens a new server session and replays
/// the sign-in ([database 6]; research `surrealdb.md` §2), so the store is shared through `Arc`
/// instead. `Surreal<Any>: Send + Sync + 'static` ✓, so `SurrealStore` is too.
pub struct SurrealStore {
    pub(crate) db: Surreal<Any>,
    /// Signalled after every job insert commits; the claim long-poll waits on it
    /// ([database 4], job claim: "an in-process `tokio::sync::Notify`").
    pub(crate) jobs_queued: Arc<Notify>,
    /// Ids for records the store creates on its own (a guided check and its job, from an
    /// upload). Every other id comes from the caller.
    pub(crate) ids: Arc<dyn IdGen>,
}

/// The default id generator: `Ulid::generate()` (ulid 3).
#[derive(Debug, Default, Clone, Copy)]
pub struct UlidIds;

impl IdGen for UlidIds {
    fn ulid(&self) -> ulid::Ulid {
        ulid::Ulid::generate()
    }
}

impl std::fmt::Debug for SurrealStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurrealStore").finish_non_exhaustive()
    }
}

impl SurrealStore {
    /// Connects, signs in as the database user and selects `pdm`/`main`. Run
    /// [`SurrealStore::migrate`] first, as root.
    pub async fn connect(config: &StoreConfig) -> Result<Self, StoreError> {
        let db = any::connect(config.url.as_str())
            .await
            .map_err(|e| classify(e).into_store("connect"))?;
        db.signin(Database {
            namespace: NAMESPACE.to_owned(),
            database: DATABASE.to_owned(),
            username: config.username.clone(),
            password: config.password.clone(),
        })
        .await
        .map_err(|e| classify(e).into_store("sign in"))?;
        Self::from_client(db).await
    }

    /// Wraps a client that is already signed in (or the embedded `mem://` engine in tests,
    /// which has no auth) and selects `pdm`/`main`.
    pub async fn from_client(db: Surreal<Any>) -> Result<Self, StoreError> {
        db.use_ns(NAMESPACE)
            .use_db(DATABASE)
            .await
            .map_err(|e| classify(e).into_store("select database"))?;
        Ok(Self {
            db,
            jobs_queued: Arc::new(Notify::new()),
            ids: Arc::new(UlidIds),
        })
    }

    /// The schema, then the migrations ([`crate::migrate::MIGRATIONS`]), over a short-lived
    /// root connection, before the API serves traffic: `api` can't run the schema's first
    /// statements (`DEFINE NAMESPACE`, `DEFINE DATABASE`). The connection closes when this
    /// returns; then connect as `api` with [`SurrealStore::connect`].
    pub async fn migrate(
        url: &str,
        root_user: &str,
        root_pass: &str,
    ) -> Result<Applied, MigrationError> {
        let store_error = |what: &'static str| {
            move |e| MigrationError::Store {
                what,
                source: Box::new(classify(e).into_store(what)),
            }
        };
        let root = any::connect(url)
            .await
            .map_err(store_error("connect as root"))?;
        root.signin(Root {
            username: root_user.to_owned(),
            password: root_pass.to_owned(),
        })
        .await
        .map_err(store_error("sign in as root"))?;
        migrate_with(&root, MIGRATIONS).await
    }

    /// `knowledge_meta:current`, or `None` before a knowledge file was applied. `api` compares
    /// it with its text embedder at start.
    pub async fn knowledge_meta(&self) -> Result<Option<KnowledgeMeta>, StoreError> {
        knowledge_meta(&self.db).await
    }

    /// Uses `ids` for the records the store creates on its own (tests pass `SequentialIds`).
    #[must_use]
    pub fn with_ids(mut self, ids: Arc<dyn IdGen>) -> Self {
        self.ids = ids;
        self
    }

    /// The client, for the knowledge loader's migration and for tests. Repository code is the
    /// only other user.
    pub fn db(&self) -> &Surreal<Any> {
        &self.db
    }

    /// The `Notify` the job broker's long poll waits on.
    pub fn jobs_queued(&self) -> Arc<Notify> {
        Arc::clone(&self.jobs_queued)
    }

    /// Readiness for `/status`: the health RPC within 2 seconds.
    pub async fn ready(&self) -> bool {
        tokio::time::timeout(Duration::from_secs(2), self.db.health())
            .await
            .is_ok_and(|r| r.is_ok())
    }
}

/// Applies [`SCHEMA`] on a client signed in as root (or the embedded engine). Every statement is
/// checked; one failure fails start-up. [`crate::migrate::migrate_with`] runs it first; tests
/// that need only the schema call it alone.
pub async fn apply_schema_with(root: &Surreal<Any>) -> Result<(), StoreError> {
    run_tx(root, SCHEMA, |q| q)
        .await
        .map(drop)
        .map_err(|f| f.into_store("apply schema"))
}
