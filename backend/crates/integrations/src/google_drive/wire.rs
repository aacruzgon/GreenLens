//! Drive's and Google OAuth's JSON shapes, and the types the client returns.
//!
//! Field names follow the Drive API v3 (`camelCase`). Google sends `int64` values such as a
//! file's `size` as strings.

use std::time::Duration;

use chrono::{DateTime, Utc};
use secrecy::SecretString;
use serde::{Deserialize, Deserializer};

/// A Drive folder's MIME type.
pub const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
/// A Google Doc's MIME type: exported as `text/plain`.
pub const DOCUMENT_MIME: &str = "application/vnd.google-apps.document";
/// A Google Sheet's MIME type: exported as `text/csv` (the first tab only).
pub const SPREADSHEET_MIME: &str = "application/vnd.google-apps.spreadsheet";
/// A plain-text file's MIME type: downloaded with `alt=media`.
pub const PLAIN_TEXT_MIME: &str = "text/plain";

/// What the sync does with a file ([connectors 6]).
///
/// [connectors 6]: ../../../../docs/connectors/README.md#6-sync
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// A folder: listed into, never read.
    Folder,
    /// A Google Doc: [`super::ExportFormat::PlainText`].
    Document,
    /// A Google Sheet: [`super::ExportFormat::Csv`], the first tab.
    Spreadsheet,
    /// `text/plain`: [`super::DriveClient::download_text`].
    PlainText,
    /// Images, PDFs, Office files and the rest: listed by name only, with no text.
    Other,
}

/// One file or folder, with the fields the client asks for.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveFile {
    /// The file id.
    pub id: String,
    /// The file's name.
    pub name: String,
    /// Its MIME type.
    pub mime_type: String,
    /// The folders it sits in.
    #[serde(default)]
    pub parents: Vec<String>,
    /// When it last changed.
    #[serde(default)]
    pub modified_time: Option<DateTime<Utc>>,
    /// The link that opens it in Drive.
    #[serde(default)]
    pub web_view_link: Option<String>,
    /// Bytes of stored content; Google's own files (Docs, Sheets, folders) have none.
    #[serde(default, deserialize_with = "int64_string")]
    pub size: Option<u64>,
    /// In the bin. A listing asks for `trashed=false`; a change can carry a trashed file.
    #[serde(default)]
    pub trashed: bool,
}

impl FileKind {
    /// How the sync treats a file of this MIME type.
    pub fn of(mime_type: &str) -> Self {
        match mime_type {
            FOLDER_MIME => Self::Folder,
            DOCUMENT_MIME => Self::Document,
            SPREADSHEET_MIME => Self::Spreadsheet,
            PLAIN_TEXT_MIME => Self::PlainText,
            _ => Self::Other,
        }
    }
}

impl DriveFile {
    /// How the sync treats the file.
    pub fn kind(&self) -> FileKind {
        FileKind::of(&self.mime_type)
    }
}

/// A folder's files, from [`super::DriveClient::list_folder`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderListing {
    /// Every file under the folder, at any depth, folders excluded; at most
    /// [`super::MAX_FILES`].
    pub files: Vec<DriveFile>,
    /// The folder and every folder found under it, the folder first. A change is under the
    /// chosen folder when one of its parents is in this list.
    pub folders: Vec<String>,
    /// Every folder found under the chosen one, with the folder it was listed in: the tree a
    /// sync keeps to tell whether a later change is still under the chosen folder.
    pub tree: Vec<(String, String)>,
    /// The listing stopped at [`super::MAX_FILES`] files or [`super::MAX_FOLDERS`] folders.
    pub truncated: bool,
}

/// A file's text, cut at [`super::MAX_TEXT_BYTES`].
#[derive(Clone, PartialEq, Eq)]
pub struct FileText {
    /// The text, cut on a character boundary. Invalid UTF-8 is replaced with U+FFFD.
    pub text: String,
    /// The file had more than [`super::MAX_TEXT_BYTES`] bytes.
    pub truncated: bool,
}

/// The text is the farm's own record: never printed by `Debug`.
impl std::fmt::Debug for FileText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileText")
            .field("bytes", &self.text.len())
            .field("truncated", &self.truncated)
            .finish()
    }
}

/// One entry of `changes.list`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// The changed file's id. Empty for a change that isn't a file's: with
    /// `includeItemsFromAllDrives`, Google also lists shared drives' own changes
    /// (`changeType: "drive"`, a `driveId` and no `fileId`).
    #[serde(default)]
    pub file_id: String,
    /// The file was deleted or the token lost access to it; `file` is then absent.
    #[serde(default)]
    pub removed: bool,
    /// When the change happened.
    #[serde(default)]
    pub time: Option<DateTime<Utc>>,
    /// The file as it is now.
    #[serde(default)]
    pub file: Option<DriveFile>,
}

/// One page of `changes.list`, from [`super::DriveClient::changes_page`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangePage {
    /// The page's changes, in the order Drive sent them.
    pub changes: Vec<Change>,
    /// The token of the next page, while the backlog goes on.
    pub next_page_token: Option<String>,
    /// On the last page: the token to save for the next sync.
    pub new_start_page_token: Option<String>,
}

/// Every change since a page token, from [`super::DriveClient::changes`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeSet {
    /// The changes of every page, in the order Drive sent them (not ordered by time).
    pub changes: Vec<Change>,
    /// The token to save for the next sync.
    pub new_start_page_token: String,
    /// How many pages were read.
    pub pages: u32,
}

/// Tokens from Google's token endpoint.
#[derive(Debug)]
pub struct TokenGrant {
    /// The access token.
    pub access_token: SecretString,
    /// The refresh token: sent on the first exchange with `access_type=offline`, and usually
    /// not on a refresh.
    pub refresh_token: Option<SecretString>,
    /// How long the access token lives.
    pub expires_in: Duration,
    /// The scopes granted, space-separated.
    pub scope: Option<String>,
}

/// The token endpoint's answer. No `Debug`: it holds the tokens in the clear.
#[derive(Deserialize)]
pub(super) struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub scope: Option<String>,
}

impl From<TokenResponse> for TokenGrant {
    fn from(r: TokenResponse) -> Self {
        Self {
            access_token: SecretString::from(r.access_token),
            refresh_token: r.refresh_token.map(SecretString::from),
            // Google's access tokens live an hour; a missing value is read as that.
            expires_in: Duration::from_secs(r.expires_in.unwrap_or(3600)),
            scope: r.scope,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FileList {
    #[serde(default)]
    pub files: Vec<DriveFile>,
    #[serde(default)]
    pub next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ChangeList {
    #[serde(default)]
    pub changes: Vec<Change>,
    #[serde(default)]
    pub next_page_token: Option<String>,
    #[serde(default)]
    pub new_start_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StartPageToken {
    pub start_page_token: String,
}

#[derive(Deserialize)]
pub(super) struct About {
    pub user: AboutUser,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AboutUser {
    pub email_address: String,
}

/// A failure's body: Google's API shape `{"error":{"code","status","errors":[{"reason"}]}}`,
/// or the OAuth endpoints' `{"error":"invalid_grant"}`. Only the short codes are read; the
/// messages may echo the request and are never kept.
#[derive(Deserialize)]
struct ErrorBody {
    error: ErrorField,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ErrorField {
    Api {
        #[serde(default)]
        status: Option<String>,
        #[serde(default)]
        errors: Vec<ErrorItem>,
        /// `google.rpc` details; an `ErrorInfo` carries a reason such as
        /// `ACCESS_TOKEN_SCOPE_INSUFFICIENT`.
        #[serde(default)]
        details: Vec<ErrorItem>,
    },
    OAuth(String),
}

#[derive(Deserialize)]
struct ErrorItem {
    #[serde(default)]
    reason: Option<String>,
}

/// The short codes of a failure's body: the API's reasons, then its details' reasons (or its
/// `status` when it lists none), or the OAuth error. Anything that isn't a short identifier is dropped, so a body
/// that echoes text can't reach an error or a log.
pub(super) fn error_codes(body: &[u8]) -> Vec<String> {
    let Ok(parsed) = serde_json::from_slice::<ErrorBody>(body) else {
        return Vec::new();
    };
    let codes = match parsed.error {
        ErrorField::Api {
            status,
            errors,
            details,
        } => {
            let reasons: Vec<String> = errors
                .into_iter()
                .chain(details)
                .filter_map(|e| e.reason)
                .collect();
            if reasons.is_empty() {
                status.into_iter().collect()
            } else {
                reasons
            }
        }
        ErrorField::OAuth(code) => vec![code],
    };
    codes.into_iter().filter(|c| is_short_code(c)).collect()
}

fn is_short_code(code: &str) -> bool {
    (1..=64).contains(&code.len())
        && code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
}

fn int64_string<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u64>, D::Error> {
    Option::<String>::deserialize(deserializer)?
        .map(|s| s.parse::<u64>().map_err(serde::de::Error::custom))
        .transpose()
}
