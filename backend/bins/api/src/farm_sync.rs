//! The farm connectors' sync ([connectors 6], docs/EON E4): the `connectors` scheduler task's
//! body, vendor-agnostic over the [`FarmSource`] port.
//!
//! Every minute the task takes at most 4 due connectors ([`FarmRecordStore::due_connectors`])
//! and syncs them side by side. For one connector: unseal its token; then for each chosen source,
//! read pages from the source's cursor until the pass ends or the run's budget is spent; for each
//! page, chunk the items ([`chunk`]), hash each chunk, embed only the chunks whose hash changed
//! (in `Document` mode), upsert them, and delete what the vendor reports gone; finally store the
//! cursors, the times and the failure in one [`FarmRecordStore::finish_sync`].
//!
//! - **Passes and cursors** ([`state`]): a source's stored cursor wraps the adapter's own. A pass
//!   with pages left (a throttled first sync, a spent budget) continues from it on a later run,
//!   and records are keyed by `(connector, vendor_id, n)`, so nothing is stored twice.
//! - **The daily re-read**: for a vendor with a [`FarmSource::reread_window`] (Slack's 7 days),
//!   once a day after a finished pass the sync starts a pass from that far back; at its end, the
//!   source's records in the window ([`FarmRecordStore::source_vendor_ids_since`]) that the pass
//!   didn't see are deleted.
//! - **Limits and errors**: a page that would take the garden past 10,000 records stops the sync
//!   with `too_large`; a refused token makes the connector `reauth_needed`, and its records stay;
//!   a `429` keeps the cursor and the next run continues; an outage tries again 15 minutes later
//!   ([`outcome`]). `last_error` is a fixed code, never the vendor's text, and nothing the
//!   vendor sent is logged: lines carry the connector id, the source id and the code.
//!
//! Drive (E4.3) plugs in as another [`FarmSource`] in [`FarmSources`], keyed by kind and
//! endpoint; it returns `None` from `reread_window` and reports deletions in
//! `FarmPage::removed`.
//!
//! [connectors 6]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#6-sync

pub mod chunk;
pub mod state;

#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::Duration,
};

use secrecy::ExposeSecret as _;
use tokio_util::sync::CancellationToken;

use domain::{
    connectors::{
        ConnectorEndpoint, ConnectorKind, ConnectorSource, ConnectorStatus, SyncErrorCode,
    },
    id::{ConnectorId, FarmRecordId, GardenId},
    ports::{
        BoxError, Clock, ConnectorRow, EmbedMode, FarmPage, FarmPageRequest, FarmSource,
        FarmSourceError, FarmVector, NewFarmRecord, Store, StoreError, SyncCursor, SyncOutcome,
        TextEmbedder, VendorToken,
    },
    rules::limits::{FARM_RECORD_KEEP, MAX_FARM_RECORDS_PER_GARDEN},
    wire::Timestamp,
};

use crate::{
    error::{ApiError, chain},
    sealing::{ConnectorKey, SealPurpose},
};

use state::{Reread, SourceState};

/// A connector is synced again this long after a sync that finished, or after an outage
/// ([connectors 6]; the `soil` task's pattern).
///
/// [connectors 6]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#6-sync
pub const SYNC_EVERY: Duration = Duration::from_mins(15);

/// The re-read of a vendor's trailing window runs this long after the last.
pub const REREAD_EVERY: Duration = Duration::from_hours(24);

/// Texts per embedder call.
const EMBED_BATCH: usize = 32;

/// The sync's settings.
#[derive(Debug, Clone, Copy)]
pub struct SyncConfig {
    /// Due connectors taken per run.
    pub batch: u32,
    /// How long one connector reads in a run before it saves its cursor and leaves the rest
    /// for the next run (checked between pages: under Slack's Tier 1 pacing a page can wait a
    /// minute per call).
    pub run_budget: Duration,
    /// Records a garden may hold.
    pub max_records: u32,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            batch: 4,
            run_budget: Duration::from_mins(3),
            max_records: MAX_FARM_RECORDS_PER_GARDEN,
        }
    }
}

/// The adapters, by vendor and endpoint: the real vendor for owners' connectors, Era's
/// simulators for the seeded demo ones. Cheap to clone.
#[derive(Clone, Default)]
pub struct FarmSources(HashMap<(ConnectorKind, ConnectorEndpoint), Arc<dyn FarmSource>>);

impl FarmSources {
    /// Adds `source` for its kind at `endpoint`.
    #[must_use]
    pub fn with(mut self, endpoint: ConnectorEndpoint, source: Arc<dyn FarmSource>) -> Self {
        self.0.insert((source.kind(), endpoint), source);
        self
    }

    /// The adapter a connector reads through, if one is configured.
    pub fn get(
        &self,
        kind: ConnectorKind,
        endpoint: ConnectorEndpoint,
    ) -> Option<&Arc<dyn FarmSource>> {
        self.0.get(&(kind, endpoint))
    }
}

impl std::fmt::Debug for FarmSources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_set().entries(self.0.keys()).finish()
    }
}

/// What made a connector's run stop before every source was read to its end.
#[derive(Debug)]
enum Stop {
    /// The vendor refused, throttled, failed or lost a source.
    Vendor(FarmSourceError),
    /// The garden would pass its record limit.
    TooLarge,
    /// The store or the embedder failed.
    Internal(BoxError),
}

impl From<StoreError> for Stop {
    fn from(e: StoreError) -> Self {
        Self::Internal(Box::new(e))
    }
}

/// How a source's reading ended in this run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Read {
    /// Its pass reached the last page.
    Done,
    /// The budget ran out, or shutdown came, with pages left.
    PagesLeft,
}

/// How one connector's run ended, for the logs and the tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEnd {
    /// Every source read to its end.
    Synced,
    /// Pages left for the next run.
    PagesLeft,
    /// Stopped with this code.
    Failed(SyncErrorCode),
    /// No adapter for its kind and endpoint here; tried again later.
    NoAdapter,
    /// The store or the embedder failed; tried again after [`SYNC_EVERY`].
    Internal,
}

/// The sync. One per process, built only when connectors are on.
pub struct FarmSync {
    store: Arc<dyn Store>,
    embedder: Arc<dyn TextEmbedder>,
    clock: Arc<dyn Clock>,
    key: ConnectorKey,
    sources: FarmSources,
    config: SyncConfig,
}

/// One connector's fixed facts during a run.
struct Run<'a> {
    id: ConnectorId,
    garden: GardenId,
    source: &'a dyn FarmSource,
    token: VendorToken,
    account: domain::connectors::ConnectorAccount,
    /// The run's start, for the windows.
    now: Timestamp,
    deadline: tokio::time::Instant,
}

impl FarmSync {
    /// The sync over these ports.
    pub fn new(
        store: Arc<dyn Store>,
        embedder: Arc<dyn TextEmbedder>,
        clock: Arc<dyn Clock>,
        key: ConnectorKey,
        sources: FarmSources,
        config: SyncConfig,
    ) -> Self {
        Self {
            store,
            embedder,
            clock,
            key,
            sources,
            config,
        }
    }

    /// The `connectors` task: syncs at most [`SyncConfig::batch`] due connectors, side by side,
    /// and returns how each ended. A connector's failure is logged and stored; only a failure
    /// to list the due connectors is an error.
    pub async fn run_due(
        &self,
        shutdown: &CancellationToken,
    ) -> Result<Vec<(ConnectorId, RunEnd)>, ApiError> {
        let due = self
            .store
            .due_connectors(self.clock.now(), self.config.batch)
            .await?;
        let runs = due.into_iter().map(|row| async move {
            let id = row.connector.id;
            (id, self.sync_connector(row, shutdown).await)
        });
        Ok(futures::future::join_all(runs).await)
    }

    /// One connector's run, from its row as [`FarmRecordStore::due_connectors`] returned it.
    pub async fn sync_connector(&self, row: ConnectorRow, shutdown: &CancellationToken) -> RunEnd {
        let id = row.connector.id;
        let kind = row.connector.kind;
        let span = tracing::info_span!("farm_sync", connector = %id, kind = kind.as_str());
        let end = tracing::Instrument::instrument(self.run(row, shutdown), span.clone()).await;
        let _entered = span.enter();
        match end {
            RunEnd::Synced | RunEnd::PagesLeft | RunEnd::NoAdapter => {
                tracing::info!(end = ?end, "farm sync");
            }
            RunEnd::Failed(code) => {
                tracing::warn!(last_error = code.as_str(), "farm sync stopped");
            }
            RunEnd::Internal => {}
        }
        end
    }

    async fn run(&self, row: ConnectorRow, shutdown: &CancellationToken) -> RunEnd {
        let id = row.connector.id;
        let due_at = row.next_sync_at;
        let now = self.clock.now();
        let Some(source) = self.sources.get(row.connector.kind, row.connector.endpoint) else {
            let outcome = SyncOutcome {
                due_at,
                cursors: row.cursors.clone(),
                synced_at: None,
                next_sync_at: now.plus(SYNC_EVERY),
                last_error: row.last_error,
            };
            return self.finish(id, outcome, RunEnd::NoAdapter).await;
        };
        let opened = self.token(&row);
        let (Some(token), Some(account)) = (opened, row.connector.account.clone()) else {
            // A token that doesn't open (another key) or a connector without its account:
            // only reconnecting fixes it.
            return self
                .stop(
                    &row,
                    row.cursors.clone(),
                    &Stop::Vendor(refused(row.connector.kind)),
                )
                .await;
        };
        let run = Run {
            id,
            garden: row.connector.garden_id,
            source: source.as_ref(),
            token,
            account,
            now,
            deadline: tokio::time::Instant::now() + self.config.run_budget,
        };
        let mut cursors = row.cursors.clone();
        let mut count = match self.store.garden_record_count(run.garden).await {
            Ok(n) => n,
            Err(e) => return self.internal(&row, cursors, Box::new(e)).await,
        };
        let mut left = false;
        for src in &row.connector.sources {
            let mut state = SourceState::parse(cursors.get(&src.id));
            let read = self
                .read_source(&run, src, &mut state, &mut count, shutdown)
                .await;
            cursors.insert(src.id.clone(), state.to_cursor());
            match read {
                Ok(Read::Done) => {}
                Ok(Read::PagesLeft) => {
                    left = true;
                    break;
                }
                Err(Stop::Internal(e)) => return self.internal(&row, cursors, e).await,
                Err(stop) => return self.stop(&row, cursors, &stop).await,
            }
        }
        let end_now = self.clock.now();
        let (synced_at, next, end) = if left {
            (None, end_now, RunEnd::PagesLeft)
        } else {
            (Some(end_now), end_now.plus(SYNC_EVERY), RunEnd::Synced)
        };
        let outcome = SyncOutcome {
            due_at,
            cursors,
            synced_at,
            next_sync_at: next,
            last_error: None,
        };
        self.finish(id, outcome, end).await
    }

    /// The connector's token, unsealed.
    fn token(&self, row: &ConnectorRow) -> Option<VendorToken> {
        let sealed = row.secret.as_ref()?;
        let opened = self
            .key
            .unseal(SealPurpose::Token, row.connector.id, sealed)
            .ok()?;
        std::str::from_utf8(opened.expose_secret())
            .ok()
            .map(VendorToken::new)
    }

    /// Reads one source's pages until its pass ends, the budget runs out or shutdown comes.
    async fn read_source(
        &self,
        run: &Run<'_>,
        src: &ConnectorSource,
        state: &mut SourceState,
        count: &mut u32,
        shutdown: &CancellationToken,
    ) -> Result<Read, Stop> {
        let window = run.source.reread_window();
        loop {
            if shutdown.is_cancelled() || tokio::time::Instant::now() >= run.deadline {
                return Ok(Read::PagesLeft);
            }
            if state.reread.is_none()
                && !state.more
                && let Some(window) = window
                && state
                    .reread_at
                    .is_some_and(|at| run.now >= at.plus(REREAD_EVERY))
            {
                state.reread = Some(Reread {
                    from: run.now.minus(window),
                    seen: std::collections::BTreeSet::new(),
                });
            }
            let request = FarmPageRequest {
                account: run.account.clone(),
                source: src.clone(),
                cursor: state.vendor_cursor(),
                oldest: run.now.minus(FARM_RECORD_KEEP),
                reread_from: state.reread.as_ref().map(|r| r.from),
            };
            let page = run
                .source
                .read_page(&run.token, request)
                .await
                .map_err(Stop::Vendor)?;
            self.store_page(run, src, &page, count).await?;
            if let Some(reread) = state.reread.as_mut() {
                reread
                    .seen
                    .extend(page.items.iter().map(|i| i.vendor_id.clone()));
                reread.seen.extend(page.kept.iter().cloned());
            }
            let SyncCursor(cursor) = page.cursor;
            state.vendor = Some(cursor);
            state.more = page.more;
            if !page.more {
                if let Some(reread) = state.reread.take() {
                    self.delete_unseen(run, src, &reread, count).await?;
                    state.reread_at = Some(run.now);
                } else if state.reread_at.is_none() {
                    // The first pass read the whole 180 days, the re-read window included.
                    state.reread_at = Some(run.now);
                }
                return Ok(Read::Done);
            }
        }
    }

    /// At a re-read's end: the source's records in the window that the pass didn't see are
    /// gone at the vendor. The window starts a second late, so a message on the boundary
    /// second, which the vendor's exclusive `oldest` may leave out, is never deleted.
    async fn delete_unseen(
        &self,
        run: &Run<'_>,
        src: &ConnectorSource,
        reread: &Reread,
        count: &mut u32,
    ) -> Result<(), Stop> {
        let stored = self
            .store
            .source_vendor_ids_since(
                run.id,
                src.id.clone(),
                reread.from.plus(Duration::from_secs(1)),
            )
            .await?;
        let gone: Vec<String> = stored
            .into_iter()
            .filter(|v| !reread.seen.contains(v))
            .collect();
        if !gone.is_empty() {
            let deleted = self.store.delete_records(run.id, gone).await?;
            *count = count.saturating_sub(deleted);
            tracing::info!(source = %src.id, deleted, "records gone at the vendor deleted");
        }
        Ok(())
    }

    /// Chunks, hashes, embeds the changed chunks and stores one page; deletes what it reports
    /// gone. `count` follows the garden's records.
    async fn store_page(
        &self,
        run: &Run<'_>,
        src: &ConnectorSource,
        page: &FarmPage,
        count: &mut u32,
    ) -> Result<(), Stop> {
        // One item per vendor id; a later copy wins.
        let mut items: BTreeMap<&str, &domain::ports::FarmItem> = BTreeMap::new();
        for item in &page.items {
            items.insert(item.vendor_id.as_str(), item);
        }
        let mut records: Vec<NewFarmRecord> = Vec::new();
        let mut changed: Vec<usize> = Vec::new();
        let mut shrunk: Vec<(String, u32)> = Vec::new();
        let mut new = 0_u32;
        if !items.is_empty() {
            let stored = self
                .store
                .record_hashes(run.id, items.keys().map(|v| (*v).to_owned()).collect())
                .await?;
            let mut hashes: HashMap<(&str, u32), &str> = HashMap::new();
            let mut held: HashMap<&str, u32> = HashMap::new();
            for (id, hash) in &stored {
                hashes.insert((&id.vendor_id, id.n), hash);
                let n = held.entry(&id.vendor_id).or_default();
                *n = (*n).max(id.n + 1);
            }
            'items: for (vendor_id, item) in &items {
                let texts = chunk::chunks(item.kind, &item.text, &item.title);
                let mut item_records = Vec::with_capacity(texts.len());
                let mut item_changed = Vec::new();
                let mut item_new = 0;
                for (n, text) in texts.into_iter().enumerate() {
                    let n = u32::try_from(n).unwrap_or(u32::MAX);
                    let Ok(id) = FarmRecordId::new(run.id, vendor_id, n) else {
                        tracing::warn!(source = %src.id, "an item with an unusable id skipped");
                        continue 'items;
                    };
                    let hash = chunk::text_hash(&text);
                    let old = hashes.get(&(*vendor_id, n)).copied();
                    if old.is_none() {
                        item_new += 1;
                    }
                    if old != Some(hash.as_str()) {
                        item_changed.push(records.len() + item_records.len());
                    }
                    item_records.push(NewFarmRecord {
                        id,
                        kind: item.kind,
                        source: src.id.clone(),
                        title: item.title.clone(),
                        author: item.author.clone(),
                        link: item.link.clone(),
                        text,
                        text_hash: hash,
                        vector: None,
                        at: item.at,
                    });
                }
                let chunks = u32::try_from(item_records.len()).unwrap_or(u32::MAX);
                if held.get(vendor_id).is_some_and(|&h| h > chunks) {
                    shrunk.push(((*vendor_id).to_owned(), chunks));
                }
                new += item_new;
                changed.extend(item_changed);
                records.extend(item_records);
            }
        }
        if count.saturating_add(new) > self.config.max_records {
            return Err(Stop::TooLarge);
        }
        self.embed(&mut records, &changed).await?;
        if !records.is_empty() {
            self.store.upsert_records(run.id, records).await?;
            *count += new;
        }
        for (vendor_id, from) in shrunk {
            let deleted = self
                .store
                .delete_chunks_from(run.id, vendor_id, from)
                .await?;
            *count = count.saturating_sub(deleted);
        }
        if !page.removed.is_empty() {
            let deleted = self
                .store
                .delete_records(run.id, page.removed.clone())
                .await?;
            *count = count.saturating_sub(deleted);
        }
        Ok(())
    }

    /// Embeds the chunks at `changed` in `Document` mode, in batches; the others keep their
    /// stored vector.
    async fn embed(&self, records: &mut [NewFarmRecord], changed: &[usize]) -> Result<(), Stop> {
        if changed.is_empty() {
            return Ok(());
        }
        let model = self.embedder.model().name;
        for batch in changed.chunks(EMBED_BATCH) {
            let texts: Vec<String> = batch.iter().map(|&i| records[i].text.clone()).collect();
            let vectors = self
                .embedder
                .embed(EmbedMode::Document, &texts)
                .await
                .map_err(|e| Stop::Internal(Box::new(e)))?;
            if vectors.len() != batch.len() {
                return Err(Stop::Internal(
                    "the embedder returned a vector count that differs from its input".into(),
                ));
            }
            for (&i, embedding) in batch.iter().zip(vectors) {
                records[i].vector = Some(FarmVector {
                    embedding,
                    model: model.clone(),
                });
            }
        }
        Ok(())
    }

    /// A run that stopped on the vendor or the limit: the cursors so far, the code, and when to
    /// try again. A refused token also makes the connector `reauth_needed`.
    async fn stop(
        &self,
        row: &ConnectorRow,
        cursors: BTreeMap<String, SyncCursor>,
        stop: &Stop,
    ) -> RunEnd {
        let now = self.clock.now();
        let (code, next) = outcome(stop, now);
        if let Stop::Vendor(error) = stop {
            tracing::info!(error = %chain(error), "vendor stopped the sync");
        }
        let outcome = SyncOutcome {
            due_at: row.next_sync_at,
            cursors,
            synced_at: None,
            next_sync_at: next,
            last_error: Some(code),
        };
        let end = self
            .finish(row.connector.id, outcome, RunEnd::Failed(code))
            .await;
        if code == SyncErrorCode::AuthRefused
            && let Err(e) = self
                .store
                .set_status(row.connector.id, ConnectorStatus::ReauthNeeded, Some(code))
                .await
        {
            tracing::warn!(error = %chain(&e), "reauth_needed not stored");
            return RunEnd::Internal;
        }
        end
    }

    /// A run that the store or the embedder stopped: logged, the cursors so far kept, and tried
    /// again after [`SYNC_EVERY`], so a lasting failure doesn't spend vendor calls every minute.
    async fn internal(
        &self,
        row: &ConnectorRow,
        cursors: BTreeMap<String, SyncCursor>,
        error: BoxError,
    ) -> RunEnd {
        tracing::warn!(error = %chain(&*error), "farm sync failed");
        let outcome = SyncOutcome {
            due_at: row.next_sync_at,
            cursors,
            synced_at: None,
            next_sync_at: self.clock.now().plus(SYNC_EVERY),
            last_error: row.last_error,
        };
        self.finish(row.connector.id, outcome, RunEnd::Internal)
            .await;
        RunEnd::Internal
    }

    async fn finish(&self, id: ConnectorId, outcome: SyncOutcome, end: RunEnd) -> RunEnd {
        match self.store.finish_sync(id, outcome).await {
            Ok(()) => end,
            Err(e) => {
                tracing::warn!(error = %chain(&e), "sync state not stored; the connector stays due");
                RunEnd::Internal
            }
        }
    }
}

/// A run's stopping code and its next attempt: a `429` continues when the vendor allows (the
/// next run at the earliest); everything else 15 minutes later. After `auth_refused`,
/// `too_large` and `source_missing` the due query skips the connector until the owner acts.
fn outcome(stop: &Stop, now: Timestamp) -> (SyncErrorCode, Timestamp) {
    match stop {
        Stop::Vendor(FarmSourceError::RateLimited { retry_after_s, .. }) => (
            SyncErrorCode::RateLimited,
            now.plus(Duration::from_secs(u64::from(retry_after_s.unwrap_or(0)))),
        ),
        Stop::Vendor(e) => (e.code(), now.plus(SYNC_EVERY)),
        Stop::TooLarge => (SyncErrorCode::TooLarge, now.plus(SYNC_EVERY)),
        Stop::Internal(_) => (SyncErrorCode::VendorUnavailable, now.plus(SYNC_EVERY)),
    }
}

/// The refusal for a token that can't be used.
const fn refused(vendor: ConnectorKind) -> FarmSourceError {
    FarmSourceError::AuthRefused { vendor }
}

/// The cleanup's part ([connectors 6]): Slack messages older than 180 days, 500 a batch until
/// none is left. Drive documents stay while they are under the folder.
///
/// [connectors 6]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#6-sync
pub async fn delete_old_messages(store: &dyn Store, now: Timestamp) -> Result<u32, ApiError> {
    let cutoff = now.minus(FARM_RECORD_KEEP);
    let mut total = 0;
    loop {
        let deleted = store.delete_records_before(cutoff).await?;
        total += deleted;
        if deleted == 0 {
            return Ok(total);
        }
    }
}
