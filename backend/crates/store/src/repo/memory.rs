//! Garden memory ([contracts 8], memory; [database 5], vector search: exact cosine within one
//! garden ✓, no HNSW index).

use chrono::{DateTime, Utc};
use domain::{
    authz::{Editor, Membership, Viewer},
    chat::{Memory, MemoryQuery, MemorySource},
    error::DomainError,
    id::{GardenId, MemoryId, PlantId, UserId},
    ports::{Embedding, MemoryHit, MemoryStore, NewMemory, StoreError},
    wire::{Page, PageRequest, Role},
};
use surrealdb::types::{RecordId, SurrealValue};

use crate::{
    SurrealStore,
    error::{RowError, take_failed},
    ids::{from_opt_rid, from_rid, opt_rid, rid},
    read::{page as page_of, take_all, take_one},
    rows::{MemoryKindRow, MemorySourceRow, RoleRow, dt, ts},
    tx::run_tx,
};

/// The statements.
pub mod sql {
    /// A memory with its vector.
    pub const CREATE_MEMORY: &str = "CREATE $memory CONTENT $row;
SELECT * FROM ONLY $memory;";

    /// `search_memory` ([database 5] ✓): exact cosine within one garden, the shared memories
    /// and the caller's own, never another member's private ones; with a plant, that plant's
    /// and the garden-level facts. A garden has at most a few thousand memories, so an exact
    /// search is cheap and always correct, and it avoids the filtered-KNN bug after deletes
    /// (#7423).
    pub const SEARCH: &str = "SELECT *, vector::similarity::cosine(embedding, $query) AS similarity FROM memory
WHERE garden = $garden AND (shared = true OR author = $user) AND ($plant = NONE OR plant = $plant OR plant = NONE)
  AND plant.deleting_at = NONE
ORDER BY similarity DESC LIMIT $k;";

    /// The projection of a `memory` into `Memory`.
    macro_rules! memory_fields {
        () => {
            "id, garden AS garden_id, plant AS plant_id, kind, text, source, shared, created_at"
        };
    }

    /// `GET /memories`: shared memories plus the caller's own chat memories, newest first,
    /// paged by id ([database 5]).
    pub const MEMORIES: &str = concat!(
        "SELECT ", memory_fields!(), " FROM memory WHERE garden = $garden AND (shared = true OR author = $user)
  AND ($plant = NONE OR plant = $plant) AND plant.deleting_at = NONE AND ($after = NONE OR id < $after)
ORDER BY id DESC LIMIT $limit;"
    );
    /// What the plan writer reads: shared memories only (`pipeline` and `user`), the plant's and
    /// the garden-level ones.
    pub const SHARED_FOR_PLAN: &str = concat!(
        "SELECT ",
        memory_fields!(),
        " FROM memory WHERE garden = $garden AND shared = true AND (plant = $plant OR plant = NONE) ORDER BY created_at;"
    );
    /// `POST /memories/{id}/share`: the author's chat memory becomes a shared `user` memory.
    pub const SHARE: &str = concat!(
        "BEGIN;
IF array::len(SELECT VALUE id FROM $memory WHERE garden = $garden AND author = $user AND source = 'chat') = 0 { THROW 'not_found' };
UPDATE $memory SET source = 'user', shared = true;
SELECT ", memory_fields!(), " FROM ONLY $memory;
COMMIT;"
    );
    /// `DELETE /memories/{id}`: the author of a chat memory, or an owner for shared memories
    /// (contracts 8); anyone else's is `not_found` (a private memory) or `forbidden`.
    pub const DELETE: &str = "BEGIN;
LET $m = (SELECT source, author, shared FROM ONLY $memory WHERE garden = $garden);
IF $m = NONE OR ($m.shared = false AND $m.author != $user) { THROW 'not_found' };
IF $m.shared = true AND $role != 'owner' { THROW 'forbidden' };
DELETE $memory;
COMMIT;";
    /// The memory's source and author.
    pub const MEMORY_OWNER: &str = "SELECT source, author FROM ONLY $memory;";
}

/// What [`sql::MEMORY_OWNER`] reads.
#[derive(Debug, Clone, SurrealValue)]
pub struct MemoryOwnerRow {
    /// Its source.
    pub source: MemorySourceRow,
    /// Its author.
    pub author: Option<RecordId>,
}

/// A `memory` row as written.
#[derive(Debug, Clone, SurrealValue)]
pub struct MemoryRow {
    /// Its garden.
    pub garden: RecordId,
    /// Its plant; `NONE` for a garden-level fact.
    pub plant: Option<RecordId>,
    /// `fact`, `event` or `summary`.
    pub kind: MemoryKindRow,
    /// The text.
    pub text: String,
    /// `pipeline`, `chat` or `user`.
    pub source: MemorySourceRow,
    /// Visible to every member.
    pub shared: bool,
    /// Its author (chat memories and notes).
    pub author: Option<RecordId>,
    /// The chat message it came from (a plain link).
    pub source_msg: Option<RecordId>,
    /// The vector of `text`.
    pub embedding: Vec<f32>,
    /// The embedding model.
    pub embedding_model: String,
    /// When.
    pub created_at: DateTime<Utc>,
}

/// A memory as read.
#[derive(Debug, Clone, SurrealValue)]
pub struct MemoryReadRow {
    /// `memory:<ulid>`.
    pub id: RecordId,
    /// Its garden.
    pub garden: RecordId,
    /// Its plant.
    pub plant: Option<RecordId>,
    /// Its kind.
    pub kind: MemoryKindRow,
    /// The text.
    pub text: String,
    /// Its source.
    pub source: MemorySourceRow,
    /// Shared with the garden.
    pub shared: bool,
    /// When.
    pub created_at: DateTime<Utc>,
    /// The search's cosine similarity, when searched.
    pub similarity: Option<f64>,
}

impl TryFrom<MemoryReadRow> for Memory {
    type Error = RowError;

    fn try_from(row: MemoryReadRow) -> Result<Self, RowError> {
        Ok(Self {
            id: from_rid(&row.id)?,
            garden_id: from_rid(&row.garden)?,
            plant_id: from_opt_rid(row.plant.as_ref())?,
            kind: row.kind.into(),
            text: row.text,
            source: row.source.into(),
            shared: row.shared,
            created_at: ts(row.created_at),
        })
    }
}

impl MemoryRow {
    fn new(
        garden: GardenId,
        memory: &NewMemory,
        source: MemorySource,
        shared: bool,
        author: Option<UserId>,
    ) -> Self {
        Self {
            garden: rid(&garden),
            plant: opt_rid(memory.plant.as_ref()),
            kind: memory.kind.into(),
            text: memory.text.clone(),
            source: source.into(),
            shared,
            author: opt_rid(author.as_ref()),
            source_msg: opt_rid(memory.source_msg.as_ref()),
            embedding: memory.embedding.0.clone(),
            embedding_model: memory.embedding_model.clone(),
            created_at: dt(memory.created_at),
        }
    }
}

impl SurrealStore {
    async fn create_memory(&self, id: MemoryId, row: MemoryRow) -> Result<Memory, StoreError> {
        const WHAT: &str = "create memory";
        let mut results = run_tx(&self.db, sql::CREATE_MEMORY, |q| {
            q.bind(("memory", rid(&id))).bind(("row", row.clone()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let read: Option<MemoryReadRow> = results.take(1).map_err(|e| take_failed(WHAT, e))?;
        Memory::try_from(read.ok_or(StoreError::NotFound)?).map_err(|e| e.into_store(WHAT))
    }
}

#[async_trait::async_trait]
impl MemoryStore for SurrealStore {
    async fn remember(
        &self,
        member: Membership<Viewer>,
        memory: NewMemory,
    ) -> Result<Memory, StoreError> {
        let row = MemoryRow::new(
            member.garden(),
            &memory,
            MemorySource::Chat,
            false,
            Some(member.user()),
        );
        self.create_memory(memory.id, row).await
    }

    async fn add_note(
        &self,
        member: Membership<Editor>,
        memory: NewMemory,
    ) -> Result<Memory, StoreError> {
        let row = MemoryRow::new(
            member.garden(),
            &memory,
            MemorySource::User,
            true,
            Some(member.user()),
        );
        self.create_memory(memory.id, row).await
    }

    async fn add_pipeline_memory(
        &self,
        garden: GardenId,
        memory: NewMemory,
    ) -> Result<Memory, StoreError> {
        let row = MemoryRow::new(garden, &memory, MemorySource::Pipeline, true, None);
        self.create_memory(memory.id, row).await
    }

    async fn memories(
        &self,
        member: Membership<Viewer>,
        query: MemoryQuery,
        page: PageRequest,
    ) -> Result<Page<Memory>, StoreError> {
        const WHAT: &str = "memories";
        if query.garden_id != member.garden() {
            return Err(StoreError::NotFound);
        }
        let after = page
            .cursor
            .as_ref()
            .map(|c| c.as_str().parse::<MemoryId>().map(|m| rid(&m)))
            .transpose()
            .map_err(|_| StoreError::NotFound)?;
        let limit = page.limit();
        let mut r = run_tx(&self.db, sql::MEMORIES, |q| {
            q.bind(("garden", rid(&member.garden())))
                .bind(("user", rid(&member.user())))
                .bind(("plant", opt_rid(query.plant_id.as_ref())))
                .bind(("after", after.clone()))
                .bind(("limit", i64::from(limit) + 1))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<Memory> = take_all(&mut r, 0, WHAT)?;
        Ok(page_of(rows, limit, |m| m.id.to_string()))
    }

    async fn search(
        &self,
        member: Membership<Viewer>,
        query: &Embedding,
        plant: Option<PlantId>,
        k: usize,
    ) -> Result<Vec<MemoryHit>, StoreError> {
        const WHAT: &str = "search memory";
        let mut results = run_tx(&self.db, sql::SEARCH, |q| {
            q.bind(("garden", rid(&member.garden())))
                .bind(("user", rid(&member.user())))
                .bind(("plant", opt_rid(plant.as_ref())))
                .bind(("query", query.0.clone()))
                .bind(("k", i64::try_from(k).unwrap_or(5)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<MemoryReadRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .map(|row| {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "a cosine similarity is in -1..=1"
                )]
                let similarity = row.similarity.unwrap_or(0.0) as f32;
                Ok(MemoryHit {
                    memory: Memory::try_from(row)?,
                    similarity,
                })
            })
            .collect::<Result<_, RowError>>()
            .map_err(|e| e.into_store(WHAT))
    }

    async fn shared_for_plan(
        &self,
        garden: GardenId,
        plant: PlantId,
    ) -> Result<Vec<Memory>, StoreError> {
        const WHAT: &str = "shared memories";
        let mut r = run_tx(&self.db, sql::SHARED_FOR_PLAN, |q| {
            q.bind(("garden", rid(&garden)))
                .bind(("plant", rid(&plant)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        take_all(&mut r, 0, WHAT)
    }

    async fn share(
        &self,
        member: Membership<Editor>,
        memory: MemoryId,
    ) -> Result<Memory, StoreError> {
        const WHAT: &str = "share memory";
        let mut r = run_tx(&self.db, sql::SHARE, |q| {
            q.bind(("memory", rid(&memory)))
                .bind(("garden", rid(&member.garden())))
                .bind(("user", rid(&member.user())))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let last = r.num_statements().saturating_sub(2);
        take_one(&mut r, last, WHAT)?.ok_or(StoreError::NotFound)
    }

    async fn delete(&self, member: Membership<Viewer>, memory: MemoryId) -> Result<(), StoreError> {
        const WHAT: &str = "delete memory";
        let role = RoleRow::from(member.role());
        run_tx(&self.db, sql::DELETE, |q| {
            q.bind(("memory", rid(&memory)))
                .bind(("garden", rid(&member.garden())))
                .bind(("user", rid(&member.user())))
                .bind(("role", role))
        })
        .await
        .map(drop)
        .map_err(|f| {
            if f.thrown() == Some("forbidden") {
                StoreError::Rejected(DomainError::Forbidden {
                    required: Role::Owner,
                })
            } else {
                f.into_store(WHAT)
            }
        })
    }

    async fn memory_owner(
        &self,
        memory: MemoryId,
    ) -> Result<Option<(MemorySource, Option<UserId>)>, StoreError> {
        const WHAT: &str = "memory owner";
        let mut r = run_tx(&self.db, sql::MEMORY_OWNER, |q| {
            q.bind(("memory", rid(&memory)))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let row: Option<MemoryOwnerRow> = r.take(0).map_err(|e| take_failed(WHAT, e))?;
        row.map(|row| Ok((row.source.into(), from_opt_rid(row.author.as_ref())?)))
            .transpose()
            .map_err(|e: RowError| e.into_store(WHAT))
    }
}
