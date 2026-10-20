//! [`DriveSource`]: the `FarmSource` port over [`DriveClient`] ([connectors 6], docs/EON E4.3).
//!
//! A Drive connector has one chosen folder. Each page does at most one vendor step that reads
//! many files (the listing, or one page of `changes.list`) and then reads the next files of the
//! queue, at most [`FILES_PER_PAGE`], so a throttled pass resumes from its cursor:
//!
//! - **A listing** (the first sync, or a resync): `files.get` on the chosen folder, which is
//!   `source_missing` when gone, trashed or not a folder; then `changes.getStartPageToken`
//!   *before* the folder is listed recursively ([`DriveClient::list_folder`]), so a change made
//!   during the listing is in the next read of `changes.list`. Every file is queued, and the
//!   page reports every file listed in [`FarmPage::listed`], so the sync deletes the source's
//!   records of files no longer there.
//! - **Later syncs** read `changes.list` one page at a time from the cursor's token, Google's
//!   own pattern: the cursor keeps `nextPageToken` while the backlog goes on, and
//!   `newStartPageToken` once it ends, so a long backlog spreads over pages and runs and a `429`
//!   loses at most one page. Only the files under the folder are kept. A file moved out of it,
//!   trashed or deleted is reported in [`FarmPage::removed`]; the sync never deletes by absence
//!   ([`FarmSource::reread_window`] is `None`). The chosen folder trashed or deleted is
//!   `source_missing`. A token Drive no longer accepts (`400`, `404`) starts a resync: the
//!   listing above.
//! - **Text:** a Google Doc exported as `text/plain`, a Sheet as `text/csv` (its first tab only,
//!   a Google fact), a `text/plain` file downloaded, anything else by name only; each cut at
//!   1 MB by the client. A file Drive won't hand over (`403 fileNotDownloadable`, an export over
//!   Google's 10 MB limit), and a "text" file that is mostly bytes no text has, is kept by name;
//!   one gone since the change (`404`) is removed.
//! - **One file that fails** (Drive down for it, a timeout, an answer that doesn't parse) goes
//!   to the back of the queue with its tries counted, and the page goes on with the others; a
//!   page stops after [`FAILURES_PER_PAGE`] such failures. A file is tried once a run, and after
//!   [`MAX_TRIES`] runs it is skipped, its record left as it was. When only files that failed in
//!   this run are left, the page fails with `vendor_unavailable`, so the run stops and the next
//!   one, 15 minutes later, tries them again. A refused token or a `429` fails the page at once.
//!
//! **Under the folder.** Drive gives a file's `parents`, not its path, and moving a folder
//! changes only that folder, never the files inside it. So the cursor keeps the tree: every
//! folder under the chosen one with the folder it sits in, and every file held with its folder.
//! A change is under the chosen folder when one of its parents is the chosen folder or a folder
//! of the tree. Folder changes are applied first: a folder moved, trashed or deleted out of the
//! tree takes its subfolders and their files with it (removed), and a folder moved in is listed
//! recursively, since its files bring no changes of their own. Then each file change adds,
//! re-reads or removes its file. Only files already held are reported removed, so the thousands
//! of changes elsewhere in an account cost nothing but the reading. A change with no file id (a
//! shared drive's own change) or with no file that isn't a removal is ignored.
//!
//! **Limits.** The folder may hold at most 500 files and 500 folders ([`MAX_FILES`],
//! [`MAX_FOLDERS`]). Past either, the page fails with `too_large`, like the garden's 10,000
//! records: nothing more runs until the owner chooses a smaller folder.
//!
//! The cursor is a small JSON object only this module reads: the page token, whether a
//! backlog is in progress, the tree, the files held and the files still to read. A cursor it
//! can't read (none yet, another shape) starts a listing, which deletes nothing it still finds.
//!
//! [connectors 6]: ../../../docs/connectors/README.md#6-sync

use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    time::Duration,
};

use async_trait::async_trait;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use domain::{
    connectors::{ConnectorKind, ConnectorSource, FarmRecordKind},
    ports::{
        FarmItem, FarmPage, FarmPageRequest, FarmSource, FarmSourceError, SyncCursor, VendorToken,
    },
    rules::limits::FARM_RECORD_KEEP,
    wire::Timestamp,
};

use super::{
    Change, DriveClient, DriveFile, ExportFormat, FileKind, MAX_FILES, MAX_FOLDERS, MAX_TEXT_BYTES,
    VENDOR,
};
use crate::farm::FarmClientError;

/// Files read on one page. A page also stops once it holds [`TEXT_PER_PAGE`] bytes of text.
pub const FILES_PER_PAGE: usize = 25;

/// The text one page holds before it stops taking files: eight files at the 1 MB cut.
const TEXT_PER_PAGE: usize = 8 * MAX_TEXT_BYTES;

/// Failed file reads after which a page stops: each can take the client's retries and
/// timeouts.
pub const FAILURES_PER_PAGE: usize = 2;

/// Runs a file's read may fail in before it is skipped, its record left as it was: about an
/// hour of an outage at one run every 15 minutes.
pub const MAX_TRIES: u8 = 5;

const KIND: ConnectorKind = ConnectorKind::GoogleDrive;

/// The farm-connector adapter for Google Drive: one per process and per endpoint (Google, or
/// an Era simulator).
#[derive(Debug, Clone)]
pub struct DriveSource {
    client: DriveClient,
}

/// The adapter's cursor, serialised into [`SyncCursor`].
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Cursor {
    /// Where `changes.list` continues: the start token taken before the listing, then each
    /// page's `nextPageToken` or, on the last page, `newStartPageToken`.
    token: String,
    /// `token` is a `nextPageToken`: a backlog of changes is being read.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    paging: bool,
    /// Every folder under the chosen one (not the chosen one itself) → the folder it is in.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    folders: BTreeMap<String, String>,
    /// Every file held, read or still to read → the folder it is in.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    files: BTreeMap<String, String>,
    /// The files this pass still has to read, in order.
    #[serde(default, skip_serializing_if = "VecDeque::is_empty")]
    queue: VecDeque<Queued>,
}

/// A file to read, with what its record needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Queued {
    id: String,
    name: String,
    mime: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    link: Option<String>,
    /// Runs whose read of the file failed.
    #[serde(default, skip_serializing_if = "is_zero")]
    tries: u8,
    /// The start of the run whose read last failed: tried again in a later run only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    failed_at: Option<Timestamp>,
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_zero(n: &u8) -> bool {
    *n == 0
}

impl Queued {
    fn of(file: &DriveFile) -> Self {
        Self {
            id: file.id.clone(),
            name: file.name.clone(),
            mime: file.mime_type.clone(),
            at: file.modified_time.map(Timestamp::from),
            link: file.web_view_link.clone(),
            tries: 0,
            failed_at: None,
        }
    }

    fn kind(&self) -> FileKind {
        FileKind::of(&self.mime)
    }
}

/// What a change page did to the cursor.
#[derive(Debug, Default, PartialEq, Eq)]
struct Applied {
    /// Files held before and no longer under the folder.
    removed: Vec<String>,
    /// Folders that joined the tree, whose files must be listed: only the topmost of each
    /// subtree, since a listing is recursive.
    joined: Vec<String>,
}

/// Why the folder can't be followed.
#[derive(Debug, PartialEq, Eq)]
enum Halt {
    /// The chosen folder was deleted or trashed.
    RootGone,
    /// More than [`MAX_FILES`] files or [`MAX_FOLDERS`] folders under it.
    TooLarge,
}

impl Halt {
    fn into_error(self, root: &str) -> FarmSourceError {
        let source_id = root.to_owned();
        match self {
            Self::RootGone => FarmSourceError::SourceMissing {
                vendor: KIND,
                source_id,
            },
            Self::TooLarge => FarmSourceError::TooLarge {
                vendor: KIND,
                source_id,
            },
        }
    }
}

/// How a later sync's step ended.
enum Step {
    /// A change page applied: the cursor, and the files no longer under the folder.
    Applied(Cursor, Vec<String>),
    /// Drive no longer accepts the cursor's token: list the folder again.
    Resync,
}

impl Cursor {
    /// A cursor this module wrote; `None` for anything else, which starts a listing.
    fn parse(cursor: Option<&SyncCursor>) -> Option<Self> {
        cursor
            .and_then(|c| serde_json::from_str::<Self>(&c.0).ok())
            .filter(|c| !c.token.is_empty())
    }

    fn to_sync_cursor(&self) -> SyncCursor {
        SyncCursor(serde_json::to_string(self).expect("a cursor of strings serialises"))
    }

    /// Whether `id` is the chosen folder or a folder of the tree.
    fn is_folder(&self, root: &str, id: &str) -> bool {
        id == root || self.folders.contains_key(id)
    }

    /// The folder of the tree a file with these parents is in, if any.
    fn home(&self, root: &str, parents: &[String]) -> Option<String> {
        parents.iter().find(|p| self.is_folder(root, p)).cloned()
    }

    /// Adds a folder found by a listing under its parent.
    fn add_folder(&mut self, id: String, parent: String) -> Result<(), Halt> {
        if !self.folders.contains_key(&id) && self.folders.len() >= MAX_FOLDERS {
            return Err(Halt::TooLarge);
        }
        self.folders.entry(id).or_insert(parent);
        Ok(())
    }

    /// Adds a listed file to the files held and to the queue. One held already is left alone,
    /// and so is one whose parent isn't a folder of the tree.
    fn add_listed(&mut self, root: &str, file: &DriveFile) -> Result<(), Halt> {
        if self.files.contains_key(&file.id) {
            return Ok(());
        }
        let Some(home) = self.home(root, &file.parents) else {
            return Ok(());
        };
        if self.files.len() >= MAX_FILES {
            return Err(Halt::TooLarge);
        }
        self.files.insert(file.id.clone(), home);
        self.queue.push_back(Queued::of(file));
        Ok(())
    }

    /// Queues `file` to read, replacing an earlier entry for it.
    fn enqueue(&mut self, file: &DriveFile) {
        self.queue.retain(|q| q.id != file.id);
        self.queue.push_back(Queued::of(file));
    }

    /// Applies one page of changes: the folder tree first, then the files ([module
    /// docs](self)). The page token is the caller's to move.
    fn apply(&mut self, root: &str, changes: &[Change]) -> Result<Applied, Halt> {
        let changes = latest(changes);
        if let Some(change) = changes.get(root)
            && (change.removed || change.file.as_ref().is_some_and(|f| f.trashed))
        {
            return Err(Halt::RootGone);
        }
        let held_before: Vec<String> = self.files.keys().cloned().collect();
        let folders_before = self.folders.clone();

        // The tree as the changes leave it: each changed folder under its new parent, or gone.
        let changed_folders: HashSet<&str> = changes
            .values()
            .filter(|c| {
                c.file.as_ref().map(DriveFile::kind) == Some(FileKind::Folder)
                    || (c.removed && c.file.is_none() && self.folders.contains_key(&c.file_id))
            })
            .map(|c| c.file_id.as_str())
            .filter(|id| *id != root)
            .collect();
        let mut parent_of = self.folders.clone();
        for id in &changed_folders {
            let change = changes[*id];
            let parent = match &change.file {
                Some(f) if !change.removed && !f.trashed => f
                    .parents
                    .iter()
                    .find(|p| {
                        p.as_str() == root
                            || parent_of.contains_key(p.as_str())
                            || changed_folders.contains(p.as_str())
                    })
                    .or_else(|| f.parents.first())
                    .cloned(),
                _ => None,
            };
            match parent {
                Some(p) => parent_of.insert((*id).to_owned(), p),
                None => parent_of.remove(*id),
            };
        }
        let tree: BTreeMap<String, String> = parent_of
            .iter()
            .filter(|(id, _)| reaches(root, id, &parent_of))
            .map(|(id, p)| (id.clone(), p.clone()))
            .collect();
        if tree.len() > MAX_FOLDERS {
            return Err(Halt::TooLarge);
        }
        self.folders = tree;
        let joined: Vec<String> = self
            .folders
            .iter()
            .filter(|(id, p)| {
                !folders_before.contains_key(*id)
                    && (p.as_str() == root || folders_before.contains_key(p.as_str()))
            })
            .map(|(id, _)| id.clone())
            .collect();

        // Files whose folder left the tree are no longer under it.
        let tree_now = self.folders.clone();
        self.files
            .retain(|_, home| home.as_str() == root || tree_now.contains_key(home.as_str()));

        // Then each file change. A change with no file that isn't a removal says nothing about
        // where the file is.
        for (id, change) in &changes {
            if changed_folders.contains(id)
                || *id == root
                || (!change.removed && change.file.is_none())
            {
                continue;
            }
            let file = change
                .file
                .as_ref()
                .filter(|f| !change.removed && !f.trashed);
            match file.and_then(|f| self.home(root, &f.parents).map(|h| (f, h))) {
                Some((f, home)) => {
                    if !self.files.contains_key(*id) && self.files.len() >= MAX_FILES {
                        return Err(Halt::TooLarge);
                    }
                    self.files.insert((*id).to_owned(), home);
                    self.enqueue(f);
                }
                None => {
                    self.files.remove(*id);
                }
            }
        }
        let files = &self.files;
        self.queue.retain(|q| files.contains_key(&q.id));
        Ok(Applied {
            removed: held_before
                .into_iter()
                .filter(|id| !self.files.contains_key(id))
                .collect(),
            joined,
        })
    }
}

/// The last change of each file: the newest `time`, and of equal times the later in the page.
/// A change with no file id (a shared drive's own) is left out.
fn latest(changes: &[Change]) -> BTreeMap<&str, &Change> {
    let mut out: BTreeMap<&str, &Change> = BTreeMap::new();
    for change in changes.iter().filter(|c| !c.file_id.is_empty()) {
        let newer = out
            .get(change.file_id.as_str())
            .is_none_or(|old| change.time >= old.time);
        if newer {
            out.insert(&change.file_id, change);
        }
    }
    out
}

/// A client error as the port's. A `404` here is about a file or a page token: the chosen
/// folder's own is checked by [`DriveSource::list`] and in the changes.
fn source_error(error: FarmClientError) -> FarmSourceError {
    error.into_source_error(KIND, None)
}

/// Whether `id`'s chain of parents reaches `root`; a cycle or a dangling parent doesn't.
fn reaches(root: &str, id: &str, parent_of: &BTreeMap<String, String>) -> bool {
    let mut at = id;
    for _ in 0..=parent_of.len() {
        match parent_of.get(at) {
            Some(p) if p == root => return true,
            Some(p) => at = p,
            None => return false,
        }
    }
    false
}

/// A "text" file that is mostly bytes no text has (a binary file labelled `text/plain`): more
/// than a tenth of its characters are U+FFFD, which stands for invalid UTF-8, or NUL.
fn looks_binary(text: &str) -> bool {
    let (mut total, mut bad) = (0_usize, 0_usize);
    for c in text.chars() {
        total += 1;
        if c == '\u{FFFD}' || c == '\0' {
            bad += 1;
        }
    }
    bad * 10 > total
}

impl DriveSource {
    /// An adapter over `client`.
    pub fn new(client: DriveClient) -> Self {
        Self { client }
    }

    /// The client underneath.
    pub fn client(&self) -> &DriveClient {
        &self.client
    }

    /// One page: a listing when there is no cursor, else, when the queue is empty, the next
    /// page of changes (a resync when Drive refuses the token); then the next files of the
    /// queue.
    async fn page(
        &self,
        token: &SecretString,
        request: &FarmPageRequest,
    ) -> Result<FarmPage, FarmSourceError> {
        let root = request.source.id.as_str();
        let now = request.oldest.plus(FARM_RECORD_KEEP);
        let mut listed = None;
        let mut removed = Vec::new();
        let mut cursor = match Cursor::parse(request.cursor.as_ref()) {
            Some(cursor) if !cursor.queue.is_empty() => cursor,
            Some(cursor) => match self.changes(token, root, cursor).await? {
                Step::Applied(cursor, gone) => {
                    removed = gone;
                    cursor
                }
                Step::Resync => {
                    let (cursor, ids) = self.list(token, root).await?;
                    listed = Some(ids);
                    cursor
                }
            },
            None => {
                let (cursor, ids) = self.list(token, root).await?;
                listed = Some(ids);
                cursor
            }
        };
        let (items, gone) = self.read_queue(token, &mut cursor, now).await?;
        removed.extend(gone);
        Ok(FarmPage {
            items,
            removed,
            kept: Vec::new(),
            listed,
            more: cursor.paging || !cursor.queue.is_empty(),
            cursor: cursor.to_sync_cursor(),
            probes: 0,
        })
    }

    /// The next page of changes from the cursor's token, applied to it, and the folders that
    /// joined the tree listed. A token Drive refuses (`400`) or doesn't know (`404`) asks for a
    /// resync instead.
    async fn changes(
        &self,
        token: &SecretString,
        root: &str,
        mut cursor: Cursor,
    ) -> Result<Step, FarmSourceError> {
        let page = match self.client.changes_page(token, &cursor.token).await {
            Ok(page) => page,
            Err(
                FarmClientError::Refused { status: 400, .. } | FarmClientError::NotFound { .. },
            ) => {
                tracing::info!(
                    vendor = VENDOR,
                    "the change token is refused; listing again"
                );
                return Ok(Step::Resync);
            }
            Err(e) => return Err(source_error(e)),
        };
        let applied = cursor
            .apply(root, &page.changes)
            .map_err(|h| h.into_error(root))?;
        for folder in &applied.joined {
            let listing = self
                .client
                .list_folder(token, folder)
                .await
                .map_err(source_error)?;
            if listing.truncated {
                return Err(Halt::TooLarge.into_error(root));
            }
            for (id, parent) in listing.tree {
                cursor
                    .add_folder(id, parent)
                    .map_err(|h| h.into_error(root))?;
            }
            for file in &listing.files {
                cursor
                    .add_listed(root, file)
                    .map_err(|h| h.into_error(root))?;
            }
        }
        match (page.new_start_page_token, page.next_page_token) {
            (Some(start), _) => {
                cursor.token = start;
                cursor.paging = false;
            }
            (None, Some(next)) => {
                cursor.token = next;
                cursor.paging = true;
            }
            (None, None) => unreachable!("changes_page refuses a page with neither token"),
        }
        tracing::debug!(
            vendor = VENDOR,
            changes = page.changes.len(),
            paging = cursor.paging,
            queued = cursor.queue.len(),
            removed = applied.removed.len(),
            "change page applied"
        );
        Ok(Step::Applied(cursor, applied.removed))
    }

    /// A listing: the chosen folder checked, the start token first, then the folder listed
    /// recursively and every file queued. Returns the cursor and every file id listed.
    async fn list(
        &self,
        token: &SecretString,
        root: &str,
    ) -> Result<(Cursor, Vec<String>), FarmSourceError> {
        match self.client.file(token, root).await {
            Ok(folder) if folder.kind() == FileKind::Folder && !folder.trashed => {}
            Ok(_) | Err(FarmClientError::NotFound { .. }) => {
                return Err(Halt::RootGone.into_error(root));
            }
            Err(e) => return Err(source_error(e)),
        }
        let start = self
            .client
            .start_page_token(token)
            .await
            .map_err(source_error)?;
        let listing = self
            .client
            .list_folder(token, root)
            .await
            .map_err(source_error)?;
        if listing.truncated {
            return Err(Halt::TooLarge.into_error(root));
        }
        let mut cursor = Cursor {
            token: start,
            ..Cursor::default()
        };
        for (id, parent) in listing.tree {
            cursor
                .add_folder(id, parent)
                .map_err(|h| h.into_error(root))?;
        }
        for file in &listing.files {
            cursor
                .add_listed(root, file)
                .map_err(|h| h.into_error(root))?;
        }
        let ids = cursor.files.keys().cloned().collect();
        Ok((cursor, ids))
    }

    /// Reads the next files of the queue into items, up to [`FILES_PER_PAGE`],
    /// [`TEXT_PER_PAGE`] or [`FAILURES_PER_PAGE`], skipping those that failed in this run
    /// (`now`, the run's start). Returns the items and the files gone since they were queued.
    async fn read_queue(
        &self,
        token: &SecretString,
        cursor: &mut Cursor,
        now: Timestamp,
    ) -> Result<(Vec<FarmItem>, Vec<String>), FarmSourceError> {
        let mut items = Vec::new();
        let mut gone = Vec::new();
        let (mut bytes, mut reads, mut failures) = (0, 0, 0);
        while reads < FILES_PER_PAGE && bytes < TEXT_PER_PAGE && failures < FAILURES_PER_PAGE {
            let Some(at) = cursor.queue.iter().position(|q| q.failed_at != Some(now)) else {
                break;
            };
            let file = cursor.queue[at].clone();
            reads += 1;
            match self.text(token, &file).await {
                Ok(text) => {
                    cursor.queue.remove(at);
                    bytes += text.len();
                    items.push(FarmItem {
                        link: file.link.clone().unwrap_or_else(|| {
                            format!("https://drive.google.com/open?id={}", file.id)
                        }),
                        vendor_id: file.id,
                        kind: FarmRecordKind::Document,
                        title: file.name,
                        author: None,
                        text,
                        at: file.at.unwrap_or(now),
                    });
                }
                Err(FarmClientError::NotFound { .. }) => {
                    cursor.queue.remove(at);
                    cursor.files.remove(&file.id);
                    gone.push(file.id);
                }
                Err(
                    e @ (FarmClientError::AuthRefused { .. } | FarmClientError::RateLimited { .. }),
                ) => {
                    return Err(source_error(e));
                }
                Err(e) => {
                    failures += 1;
                    let mut failed = cursor.queue.remove(at).expect("the file at its position");
                    failed.tries = failed.tries.saturating_add(1);
                    failed.failed_at = Some(now);
                    tracing::info!(
                        vendor = VENDOR,
                        tries = failed.tries,
                        error = %e,
                        "a file's read failed"
                    );
                    if failed.tries >= MAX_TRIES {
                        tracing::warn!(
                            vendor = VENDOR,
                            "a file failed in {MAX_TRIES} runs; skipped, its record left as it was"
                        );
                    } else {
                        cursor.queue.push_back(failed);
                    }
                }
            }
        }
        // Only files that failed in this run are left, and none was tried on this page: the
        // run stops here, and the next one tries them again.
        if reads == 0 && !cursor.queue.is_empty() {
            return Err(FarmSourceError::Unavailable {
                vendor: KIND,
                source: "the files left failed in this run".into(),
            });
        }
        Ok((items, gone))
    }

    /// A file's text: a Doc exported as text, a Sheet as CSV, plain text downloaded, anything
    /// else none. A file Drive refuses to hand over, or a "text" file that isn't text, is kept
    /// by name only.
    async fn text(&self, token: &SecretString, file: &Queued) -> Result<String, FarmClientError> {
        let read = match file.kind() {
            FileKind::Document => {
                self.client
                    .export(token, &file.id, ExportFormat::PlainText)
                    .await
            }
            FileKind::Spreadsheet => self.client.export(token, &file.id, ExportFormat::Csv).await,
            FileKind::PlainText => self.client.download_text(token, &file.id).await,
            FileKind::Folder | FileKind::Other => return Ok(String::new()),
        };
        match read {
            Ok(text) if file.kind() == FileKind::PlainText && looks_binary(&text.text) => {
                tracing::debug!(vendor = VENDOR, "a binary file kept by name only");
                Ok(String::new())
            }
            Ok(text) => Ok(text.text),
            Err(FarmClientError::Refused { status, code, .. }) => {
                tracing::debug!(
                    vendor = VENDOR,
                    status,
                    reason = code.as_str(),
                    "file kept by name only"
                );
                Ok(String::new())
            }
            Err(e) => Err(e),
        }
    }
}

#[async_trait]
impl FarmSource for DriveSource {
    fn kind(&self) -> ConnectorKind {
        KIND
    }

    fn reread_window(&self) -> Option<Duration> {
        None
    }

    async fn available_sources(
        &self,
        token: &VendorToken,
    ) -> Result<Vec<ConnectorSource>, FarmSourceError> {
        let token = SecretString::from(token.expose());
        let folders = self
            .client
            .folders(&token)
            .await
            .map_err(|e| e.into_source_error(KIND, None))?;
        Ok(folders
            .into_iter()
            .map(|f| ConnectorSource {
                id: f.id,
                name: f.name,
            })
            .collect())
    }

    async fn read_page(
        &self,
        token: &VendorToken,
        request: FarmPageRequest,
    ) -> Result<FarmPage, FarmSourceError> {
        let secret = SecretString::from(token.expose());
        self.page(&secret, &request).await
    }

    async fn revoke(&self, token: &VendorToken) -> Result<(), FarmSourceError> {
        let token = SecretString::from(token.expose());
        self.client
            .revoke(&token)
            .await
            .map_err(|e| e.into_source_error(KIND, None))
    }
}

#[cfg(test)]
mod tests;
