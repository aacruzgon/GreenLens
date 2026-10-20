//! [`DriveSource`]: the `FarmSource` port over [`DriveClient`] ([connectors 6], docs/EON E4.3).
//!
//! A Drive connector has one chosen folder. A pass reads the files it has to read, at most
//! [`FILES_PER_PAGE`] a page, so a throttled pass resumes from its cursor without reading a file
//! twice:
//!
//! - **The first sync** takes `changes.getStartPageToken` *before* it lists the folder, then
//!   lists it recursively ([`DriveClient::list_folder`], at most 500 files), so a change made
//!   during the listing is in the next read of `changes.list`.
//! - **Later syncs** read `changes.list` from the saved token to its last page
//!   ([`DriveClient::changes`]; Drive doesn't order its pages by time) and keep only the files
//!   under the folder. A file moved out of it, trashed or deleted is reported in
//!   [`FarmPage::removed`]; the sync never deletes by absence ([`FarmSource::reread_window`] is
//!   `None`). The chosen folder itself deleted or trashed is `source_missing`.
//! - **Text:** a Google Doc exported as `text/plain`, a Sheet as `text/csv` (its first tab only,
//!   a Google fact), a `text/plain` file downloaded, anything else by name only; each cut at
//!   1 MB by the client. A file Drive won't hand over (`403 fileNotDownloadable`, an export over
//!   Google's 10 MB limit) is kept by name only; one gone since the change (`404`) is removed.
//!
//! **Under the folder.** Drive gives a file's `parents`, not its path, and moving a folder
//! changes only that folder, never the files inside it. So the cursor keeps the tree: every
//! folder under the chosen one with the folder it sits in, and every file held with its folder.
//! A change is under the chosen folder when one of its parents is the chosen folder or a folder
//! of the tree. Folder changes are applied first: a folder moved, trashed or deleted out of the
//! tree takes its subfolders and their files with it (removed), and a folder moved in is listed
//! recursively, since its files bring no changes of their own. Then each file change adds,
//! re-reads or removes its file. Only files already held are reported removed, so the thousands
//! of changes elsewhere in an account cost nothing but the reading.
//!
//! The cursor is a small JSON object only this module reads: the page token, the tree, the
//! files held and the files still to read. A cursor it can't read (none yet, another shape)
//! starts a first sync, which reads again but loses no record.
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
    Change, ChangeSet, DriveClient, DriveFile, ExportFormat, FileKind, MAX_FILES, MAX_FOLDERS,
    MAX_TEXT_BYTES, VENDOR,
};
use crate::farm::FarmClientError;

/// Files read on one page. A page also stops once it holds [`TEXT_PER_PAGE`] bytes of text.
pub const FILES_PER_PAGE: usize = 25;

/// The text one page holds before it stops taking files: eight files at the 1 MB cut.
const TEXT_PER_PAGE: usize = 8 * MAX_TEXT_BYTES;

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
    /// Where `changes.list` continues: the start token taken before the first listing, then
    /// each read's `newStartPageToken`.
    token: String,
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
}

impl Queued {
    fn of(file: &DriveFile) -> Self {
        Self {
            id: file.id.clone(),
            name: file.name.clone(),
            mime: file.mime_type.clone(),
            at: file.modified_time.map(Timestamp::from),
            link: file.web_view_link.clone(),
        }
    }

    fn kind(&self) -> FileKind {
        FileKind::of(&self.mime)
    }
}

/// What a change set did to the cursor.
#[derive(Debug, Default, PartialEq, Eq)]
struct Applied {
    /// Files held before and no longer under the folder.
    removed: Vec<String>,
    /// Folders that joined the tree, whose files must be listed: only the topmost of each
    /// subtree, since a listing is recursive.
    joined: Vec<String>,
}

/// The chosen folder was deleted or trashed.
#[derive(Debug, PartialEq, Eq)]
struct RootGone;

impl Cursor {
    /// A cursor this module wrote; `None` for anything else, which starts a first sync.
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

    /// Adds a listed file to the files held and to the queue; one held already, or over the
    /// 500-file cap, is left alone.
    fn add_listed(&mut self, root: &str, file: &DriveFile) {
        if self.files.contains_key(&file.id) || self.files.len() >= MAX_FILES {
            return;
        }
        let home = self
            .home(root, &file.parents)
            .unwrap_or_else(|| root.to_owned());
        self.files.insert(file.id.clone(), home);
        self.queue.push_back(Queued::of(file));
    }

    /// Queues `file` to read, replacing an earlier entry for it.
    fn enqueue(&mut self, file: &DriveFile) {
        self.queue.retain(|q| q.id != file.id);
        self.queue.push_back(Queued::of(file));
    }

    /// Applies a change set: the folder tree first, then the files ([module docs](self)).
    fn apply(&mut self, root: &str, set: &ChangeSet) -> Result<Applied, RootGone> {
        let changes = latest(&set.changes);
        if let Some(change) = changes.get(root)
            && (change.removed || change.file.as_ref().is_none_or(|f| f.trashed))
        {
            return Err(RootGone);
        }
        let held_before: Vec<String> = self.files.keys().cloned().collect();
        let folders_before = self.folders.clone();

        // The tree as the changes leave it: each changed folder under its new parent, or gone.
        let changed_folders: HashSet<&str> = changes
            .values()
            .filter(|c| {
                c.file.as_ref().map(DriveFile::kind) == Some(FileKind::Folder)
                    || (c.file.is_none() && self.folders.contains_key(&c.file_id))
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
        let mut tree: BTreeMap<String, String> = parent_of
            .iter()
            .filter(|(id, _)| reaches(root, id, &parent_of))
            .map(|(id, p)| (id.clone(), p.clone()))
            .collect();
        if tree.len() > MAX_FOLDERS {
            // Past the cap, no new folder is followed; those known before stay.
            tracing::info!(
                vendor = VENDOR,
                "the folder holds more folders than are followed"
            );
            tree.retain(|id, _| folders_before.contains_key(id));
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

        // Then each file change.
        for (id, change) in &changes {
            // A change with no file that isn't a removal says nothing about where the file is.
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
                    if self.files.contains_key(*id) || self.files.len() < MAX_FILES {
                        self.files.insert((*id).to_owned(), home);
                        self.enqueue(f);
                    }
                }
                None => {
                    self.files.remove(*id);
                }
            }
        }
        let files = &self.files;
        self.queue.retain(|q| files.contains_key(&q.id));
        self.token.clone_from(&set.new_start_page_token);
        Ok(Applied {
            removed: held_before
                .into_iter()
                .filter(|id| !self.files.contains_key(id))
                .collect(),
            joined,
        })
    }
}

/// The last change of each file: the newest `time`, and of equal times the later in the set.
fn latest(changes: &[Change]) -> BTreeMap<&str, &Change> {
    let mut out: BTreeMap<&str, &Change> = BTreeMap::new();
    for change in changes {
        let newer = out
            .get(change.file_id.as_str())
            .is_none_or(|old| change.time >= old.time);
        if newer {
            out.insert(&change.file_id, change);
        }
    }
    out
}

/// A client error as the port's. Every call's `404` is about a file or a page token, never the
/// chosen folder, whose loss shows in the changes instead.
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

impl DriveSource {
    /// An adapter over `client`.
    pub fn new(client: DriveClient) -> Self {
        Self { client }
    }

    /// The client underneath.
    pub fn client(&self) -> &DriveClient {
        &self.client
    }

    /// One page: the listing on a first sync, else the changes when no pass is in progress;
    /// then the next files of the queue.
    async fn page(
        &self,
        token: &SecretString,
        request: &FarmPageRequest,
    ) -> Result<FarmPage, FarmSourceError> {
        let root = request.source.id.as_str();
        let (mut cursor, mut removed) = match Cursor::parse(request.cursor.as_ref()) {
            None => (
                self.first(token, root).await.map_err(source_error)?,
                Vec::new(),
            ),
            Some(cursor) if !cursor.queue.is_empty() => (cursor, Vec::new()),
            Some(cursor) => self.changes(token, root, cursor).await?,
        };
        let now = request.oldest.plus(FARM_RECORD_KEEP);
        let (items, gone) = self
            .read_queue(token, &mut cursor, now)
            .await
            .map_err(source_error)?;
        removed.extend(gone);
        Ok(FarmPage {
            items,
            removed,
            kept: Vec::new(),
            more: !cursor.queue.is_empty(),
            cursor: cursor.to_sync_cursor(),
            probes: 0,
        })
    }

    /// A later sync with no pass in progress: every change since the cursor's token, applied
    /// to it, and the folders that joined the tree listed. Returns the cursor and the files
    /// removed.
    async fn changes(
        &self,
        token: &SecretString,
        root: &str,
        mut cursor: Cursor,
    ) -> Result<(Cursor, Vec<String>), FarmSourceError> {
        let set = self
            .client
            .changes(token, &cursor.token)
            .await
            .map_err(source_error)?;
        let applied =
            cursor
                .apply(root, &set)
                .map_err(|RootGone| FarmSourceError::SourceMissing {
                    vendor: KIND,
                    source_id: root.to_owned(),
                })?;
        for folder in &applied.joined {
            let listing = self
                .client
                .list_folder(token, folder)
                .await
                .map_err(source_error)?;
            for (id, parent) in listing.tree {
                if cursor.folders.len() < MAX_FOLDERS {
                    cursor.folders.entry(id).or_insert(parent);
                }
            }
            for file in &listing.files {
                cursor.add_listed(root, file);
            }
        }
        tracing::debug!(
            vendor = VENDOR,
            changes = set.changes.len(),
            pages = set.pages,
            queued = cursor.queue.len(),
            removed = applied.removed.len(),
            "changes applied"
        );
        Ok((cursor, applied.removed))
    }

    /// A first sync's cursor: the start token first, then the folder listed recursively, every
    /// file queued.
    async fn first(&self, token: &SecretString, root: &str) -> Result<Cursor, FarmClientError> {
        let start = self.client.start_page_token(token).await?;
        let listing = self.client.list_folder(token, root).await?;
        if listing.truncated {
            tracing::info!(
                vendor = VENDOR,
                files = listing.files.len(),
                "the folder holds more than the listing reads; the rest is left out"
            );
        }
        let mut cursor = Cursor {
            token: start,
            folders: listing.tree.into_iter().collect(),
            ..Cursor::default()
        };
        for file in &listing.files {
            cursor.add_listed(root, file);
        }
        Ok(cursor)
    }

    /// Reads the next files of the queue into items, up to [`FILES_PER_PAGE`] or
    /// [`TEXT_PER_PAGE`]. Returns the items and the files gone since they were queued.
    async fn read_queue(
        &self,
        token: &SecretString,
        cursor: &mut Cursor,
        now: Timestamp,
    ) -> Result<(Vec<FarmItem>, Vec<String>), FarmClientError> {
        let mut items = Vec::new();
        let mut gone = Vec::new();
        let mut bytes = 0;
        let mut files = 0;
        while files < FILES_PER_PAGE && bytes < TEXT_PER_PAGE {
            let Some(file) = cursor.queue.front().cloned() else {
                break;
            };
            files += 1;
            match self.text(token, &file).await {
                Ok(text) => {
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
                    cursor.files.remove(&file.id);
                    gone.push(file.id);
                }
                Err(e) => return Err(e),
            }
            cursor.queue.pop_front();
        }
        Ok((items, gone))
    }

    /// A file's text: a Doc exported as text, a Sheet as CSV, plain text downloaded, anything
    /// else none. A file Drive refuses to hand over is kept by name only.
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
