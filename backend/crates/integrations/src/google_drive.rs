//! Google Drive API client for the farm connectors (docs/EON E2.2; [connectors 5] and [6]).
//!
//! - **OAuth:** the authorization-code exchange with PKCE ([`Pkce`]) and the refresh at
//!   Google's token endpoint, and the revoke on disconnect. A refused refresh token
//!   (`invalid_grant`) is [`FarmClientError::AuthRefused`].
//! - **The account:** `about.get` for the email. With only `drive.readonly` the token response
//!   has no `id_token`.
//! - **Reading:** a folder listed recursively (at most [`MAX_FILES`] files), Docs exported as
//!   text and Sheets as CSV, plain text downloaded, each cut at [`MAX_TEXT_BYTES`]; the start
//!   page token, and `changes.list` read to its last page.
//!
//! **Errors** come in Google's shape, `{"error":{"code","status","errors":[{"reason"}]}}`:
//! `401` is `AuthRefused`; `429`, and a `403` whose reason is `userRateLimitExceeded` or
//! `rateLimitExceeded`, are retried honouring `Retry-After`, then `RateLimited`; a `403` for a
//! missing scope (`ACCESS_TOKEN_SCOPE_INSUFFICIENT`, or `insufficientPermissions` on a call that
//! reads with the token's scope) is `AuthRefused`, never retried; any other `403` (such as
//! `fileNotDownloadable`) is `Refused` and never retried; `404` is `NotFound`; `408`, 5xx and transport failures are
//! retried, then `Unavailable`. A response body is never logged or kept: the log has the call,
//! the status and the reason ([connectors 8]).
//!
//! [connectors 5]: ../../../docs/connectors/README.md#5-connecting-and-tokens
//! [6]: ../../../docs/connectors/README.md#6-sync
//! [connectors 8]: ../../../docs/connectors/README.md#8-privacy-and-security

mod pkce;
#[cfg(test)]
mod tests;
mod wire;

use std::{collections::HashSet, sync::Arc, time::Duration};

use reqwest::{RequestBuilder, Response, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;

pub use pkce::{InvalidVerifier, Pkce};
pub use wire::{
    Change, ChangeSet, DOCUMENT_MIME, DriveFile, FOLDER_MIME, FileKind, FileText, FolderListing,
    PLAIN_TEXT_MIME, SPREADSHEET_MIME, TokenGrant,
};

use crate::{
    farm::FarmClientError,
    http::{self, Attempt, RetryPolicy, TransportError},
};

/// The `vendor` of every error and log line.
pub const VENDOR: &str = "google_drive";

/// The one scope PDM asks for ([connectors 5]).
///
/// [connectors 5]: ../../../docs/connectors/README.md#5-connecting-and-tokens
pub const SCOPE: &str = "https://www.googleapis.com/auth/drive.readonly";

/// Most files a folder listing returns ([connectors 6]).
///
/// [connectors 6]: ../../../docs/connectors/README.md#6-sync
pub const MAX_FILES: usize = 500;

/// Most folders a listing walks into, the chosen one included, so a tree of empty folders
/// can't run without end.
pub const MAX_FOLDERS: usize = 500;

/// A file's text is cut at 1 MB ([connectors 6]).
///
/// [connectors 6]: ../../../docs/connectors/README.md#6-sync
pub const MAX_TEXT_BYTES: usize = 1_000_000;

/// The file fields every listing and change asks for.
const FILE_FIELDS: &str = "id,name,mimeType,parents,modifiedTime,webViewLink,size,trashed";

/// Drive's largest `pageSize` for `files.list` and `changes.list`.
const MAX_PAGE_SIZE: u32 = 1000;

/// Pages read by one call that reads every page, before the answer is treated as malformed
/// (a page token that cycles, or never ends).
const MAX_PAGES: u32 = 1_000;

/// Settings.
#[derive(Debug, Clone)]
pub struct DriveConfig {
    /// `https://www.googleapis.com`, or a simulator's base URL. No trailing slash.
    pub api_base: String,
    /// `https://oauth2.googleapis.com/token`.
    pub token_url: String,
    /// `https://oauth2.googleapis.com/revoke`.
    pub revoke_url: String,
    /// Total deadline of one request, a download's body included.
    pub timeout: Duration,
    /// Retries of `429`, rate-limit `403`s, `408`, 5xx and transport failures. The code
    /// exchange is never retried: a code works once.
    pub retry: RetryPolicy,
    /// `pageSize` of `files.list` and `changes.list`, 1 to 1000.
    pub page_size: u32,
}

impl Default for DriveConfig {
    fn default() -> Self {
        Self {
            api_base: "https://www.googleapis.com".to_owned(),
            token_url: "https://oauth2.googleapis.com/token".to_owned(),
            revoke_url: "https://oauth2.googleapis.com/revoke".to_owned(),
            timeout: Duration::from_secs(30),
            retry: RetryPolicy::STANDARD,
            page_size: MAX_PAGE_SIZE,
        }
    }
}

/// What `files.export` converts a Google file to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// `text/plain`, for a Google Doc.
    PlainText,
    /// `text/csv`, for a Google Sheet: its first tab only.
    Csv,
}

impl ExportFormat {
    /// The `mimeType` parameter.
    pub const fn mime(self) -> &'static str {
        match self {
            Self::PlainText => "text/plain",
            Self::Csv => "text/csv",
        }
    }
}

/// The Drive client. Cheap to clone; every call takes the connector's access token.
#[derive(Clone)]
pub struct DriveClient {
    http: reqwest::Client,
    config: Arc<DriveConfig>,
    /// `Some(config.timeout)`. Only the unit tests clear it (paused time: tokio would otherwise
    /// auto-advance to the deadline while the mock server answers); production never does.
    request_timeout: Option<Duration>,
}

impl std::fmt::Debug for DriveClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DriveClient")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl DriveClient {
    /// A client over the shared `reqwest::Client` ([`crate::http::client`]).
    pub fn new(http: reqwest::Client, config: DriveConfig) -> Self {
        let request_timeout = Some(config.timeout);
        Self {
            http,
            config: Arc::new(config),
            request_timeout,
        }
    }

    // -----------------------------------------------------------------------------------------
    // OAuth
    // -----------------------------------------------------------------------------------------

    /// Exchanges an authorization code with its PKCE verifier (`grant_type=authorization_code`).
    /// Not retried: a code works once, and a reused one is `invalid_grant`.
    pub async fn exchange_code(
        &self,
        code: &str,
        verifier: &SecretString,
        redirect_uri: &str,
        client_id: &str,
        client_secret: &SecretString,
    ) -> Result<TokenGrant, FarmClientError> {
        let body = form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("code_verifier", verifier.expose_secret()),
            ("redirect_uri", redirect_uri),
            ("client_id", client_id),
            ("client_secret", client_secret.expose_secret()),
        ]);
        let response: wire::TokenResponse = self
            .json("oauth.token", RetryPolicy::NONE, || {
                self.form_post(&self.config.token_url, body.clone())
            })
            .await?;
        Ok(response.into())
    }

    /// A new access token from the refresh token (`grant_type=refresh_token`). A refused
    /// refresh token (`invalid_grant`: revoked, expired, or a Testing-mode token over 7 days
    /// old) is [`FarmClientError::AuthRefused`].
    pub async fn refresh(
        &self,
        refresh_token: &SecretString,
        client_id: &str,
        client_secret: &SecretString,
    ) -> Result<TokenGrant, FarmClientError> {
        let body = form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.expose_secret()),
            ("client_id", client_id),
            ("client_secret", client_secret.expose_secret()),
        ]);
        let response: wire::TokenResponse = self
            .json("oauth.refresh", self.config.retry, || {
                self.form_post(&self.config.token_url, body.clone())
            })
            .await?;
        Ok(response.into())
    }

    /// Revokes a token (the refresh token revokes the whole grant). A token Google no longer
    /// knows (`400 invalid_token`) counts as revoked.
    pub async fn revoke(&self, token: &SecretString) -> Result<(), FarmClientError> {
        let body = form(&[("token", token.expose_secret())]);
        match self
            .send("oauth.revoke", self.config.retry, || {
                self.form_post(&self.config.revoke_url, body.clone())
            })
            .await
        {
            Ok(_) => Ok(()),
            Err(FarmClientError::Refused {
                status: 400, code, ..
            }) if code == "invalid_token" => Ok(()),
            Err(e) => Err(e),
        }
    }

    // -----------------------------------------------------------------------------------------
    // Drive
    // -----------------------------------------------------------------------------------------

    /// The account's email, from `about.get`.
    pub async fn about_email(&self, token: &SecretString) -> Result<String, FarmClientError> {
        let about: wire::About = self
            .json("about.get", self.config.retry, || {
                self.get(token, "/drive/v3/about")
                    .query(&[("fields", "user(emailAddress)")])
            })
            .await?;
        Ok(about.user.email_address)
    }

    /// Every file under `folder_id`, at any depth, across `nextPageToken`, stopping at
    /// [`MAX_FILES`] files or [`MAX_FOLDERS`] folders (`truncated` says so). Trashed files are
    /// left out; a shared drive's folder works.
    pub async fn list_folder(
        &self,
        token: &SecretString,
        folder_id: &str,
    ) -> Result<FolderListing, FarmClientError> {
        let mut listing = FolderListing {
            files: Vec::new(),
            folders: vec![folder_id.to_owned()],
            truncated: false,
        };
        let mut seen: HashSet<String> = HashSet::from([folder_id.to_owned()]);
        let mut next = 0;
        let mut pages = 0_u32;
        let fields = format!("nextPageToken,files({FILE_FIELDS})");
        let page_size = self.page_size();
        'folders: while let Some(folder) = listing.folders.get(next).cloned() {
            next += 1;
            let q = format!("'{}' in parents and trashed=false", quote(&folder));
            let mut page_token: Option<String> = None;
            let mut tokens: HashSet<String> = HashSet::new();
            loop {
                pages += 1;
                if pages > MAX_PAGES {
                    return Err(malformed("files.list"));
                }
                let page: wire::FileList = self
                    .json("files.list", self.config.retry, || {
                        let mut query = vec![
                            ("q", q.as_str()),
                            ("fields", fields.as_str()),
                            ("pageSize", page_size.as_str()),
                            ("supportsAllDrives", "true"),
                            ("includeItemsFromAllDrives", "true"),
                        ];
                        if let Some(t) = &page_token {
                            query.push(("pageToken", t));
                        }
                        self.get(token, "/drive/v3/files").query(&query)
                    })
                    .await?;
                for file in page.files {
                    if file.kind() == FileKind::Folder {
                        if !seen.insert(file.id.clone()) {
                            continue;
                        }
                        if listing.folders.len() == MAX_FOLDERS {
                            listing.truncated = true;
                            break 'folders;
                        }
                        listing.folders.push(file.id);
                    } else {
                        if listing.files.len() == MAX_FILES {
                            listing.truncated = true;
                            break 'folders;
                        }
                        listing.files.push(file);
                    }
                }
                match page.next_page_token {
                    // A token seen before in this folder is a cycle.
                    Some(t) if !t.is_empty() => {
                        if !tokens.insert(t.clone()) {
                            return Err(malformed("files.list"));
                        }
                        page_token = Some(t);
                    }
                    _ => break,
                }
            }
        }
        tracing::debug!(
            vendor = VENDOR,
            files = listing.files.len(),
            folders = listing.folders.len(),
            truncated = listing.truncated,
            "folder listed"
        );
        Ok(listing)
    }

    /// A Google Doc or Sheet converted by `files.export`, cut at [`MAX_TEXT_BYTES`].
    pub async fn export(
        &self,
        token: &SecretString,
        file_id: &str,
        format: ExportFormat,
    ) -> Result<FileText, FarmClientError> {
        let path = format!("/drive/v3/files/{}/export", segment(file_id));
        let response = self
            .send("files.export", self.config.retry, || {
                self.get(token, &path).query(&[("mimeType", format.mime())])
            })
            .await?;
        read_text(response).await
    }

    /// A stored file's content (`alt=media`), for `text/plain` files, cut at
    /// [`MAX_TEXT_BYTES`]. A Google file or a file without content is `Refused` with
    /// `fileNotDownloadable`.
    pub async fn download_text(
        &self,
        token: &SecretString,
        file_id: &str,
    ) -> Result<FileText, FarmClientError> {
        let path = format!("/drive/v3/files/{}", segment(file_id));
        let response = self
            .send("files.get", self.config.retry, || {
                self.get(token, &path)
                    .query(&[("alt", "media"), ("supportsAllDrives", "true")])
            })
            .await?;
        read_text(response).await
    }

    /// `changes.getStartPageToken`: the token a first sync saves.
    pub async fn start_page_token(&self, token: &SecretString) -> Result<String, FarmClientError> {
        let start: wire::StartPageToken = self
            .json("changes.getStartPageToken", self.config.retry, || {
                self.get(token, "/drive/v3/changes/startPageToken")
                    .query(&[("supportsAllDrives", "true")])
            })
            .await?;
        Ok(start.start_page_token)
    }

    /// Every change since `page_token`, reading `changes.list` page by page until the page
    /// that carries `newStartPageToken`. Drive doesn't order the pages by time, so the caller
    /// gets them all or an error.
    pub async fn changes(
        &self,
        token: &SecretString,
        page_token: &str,
    ) -> Result<ChangeSet, FarmClientError> {
        let fields = format!(
            "nextPageToken,newStartPageToken,changes(fileId,removed,time,file({FILE_FIELDS}))"
        );
        let page_size = self.page_size();
        let mut current = page_token.to_owned();
        let mut changes = Vec::new();
        let mut pages = 0_u32;
        let mut tokens: HashSet<String> = HashSet::from([page_token.to_owned()]);
        loop {
            if pages == MAX_PAGES {
                return Err(malformed("changes.list"));
            }
            let page: wire::ChangeList = self
                .json("changes.list", self.config.retry, || {
                    self.get(token, "/drive/v3/changes").query(&[
                        ("pageToken", current.as_str()),
                        ("fields", fields.as_str()),
                        ("pageSize", page_size.as_str()),
                        ("supportsAllDrives", "true"),
                        ("includeItemsFromAllDrives", "true"),
                        ("includeRemoved", "true"),
                    ])
                })
                .await?;
            pages += 1;
            changes.extend(page.changes);
            if let Some(new_start) = page.new_start_page_token.filter(|t| !t.is_empty()) {
                tracing::debug!(
                    vendor = VENDOR,
                    changes = changes.len(),
                    pages,
                    "changes read"
                );
                return Ok(ChangeSet {
                    changes,
                    new_start_page_token: new_start,
                    pages,
                });
            }
            match page.next_page_token {
                // A token seen before (a page pointing to itself, or a cycle) would loop for ever.
                Some(next) if !next.is_empty() && tokens.insert(next.clone()) => current = next,
                _ => return Err(malformed("changes.list")),
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Requests
    // -----------------------------------------------------------------------------------------

    fn page_size(&self) -> String {
        self.config.page_size.clamp(1, MAX_PAGE_SIZE).to_string()
    }

    fn get(&self, token: &SecretString, path: &str) -> RequestBuilder {
        self.http
            .get(format!("{}{path}", self.config.api_base))
            .bearer_auth(token.expose_secret())
    }

    fn form_post(&self, url: &str, body: String) -> RequestBuilder {
        self.http
            .post(url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body)
    }

    /// A call whose success is JSON. A body that doesn't parse is `Malformed`, not retried.
    async fn json<T: DeserializeOwned>(
        &self,
        call: &'static str,
        policy: RetryPolicy,
        build: impl Fn() -> RequestBuilder,
    ) -> Result<T, FarmClientError> {
        let response = self.send(call, policy, build).await?;
        let bytes = response
            .bytes()
            .await
            .map_err(|e| unavailable(Some(TransportError::from_reqwest(e.without_url()))))?;
        serde_json::from_slice(&bytes).map_err(|_| malformed(call))
    }

    /// Sends the request `build` makes until it succeeds, fails for good, or `policy` runs out.
    async fn send(
        &self,
        call: &'static str,
        policy: RetryPolicy,
        build: impl Fn() -> RequestBuilder,
    ) -> Result<Response, FarmClientError> {
        http::retry(policy, |_| {
            let mut request = build();
            if let Some(timeout) = self.request_timeout {
                request = request.timeout(timeout);
            }
            async move {
                let response = match request.send().await {
                    Ok(response) => response,
                    Err(e) => {
                        let error = TransportError::from_reqwest(e.without_url());
                        tracing::debug!(
                            vendor = VENDOR,
                            call,
                            timeout = error.timeout,
                            "google drive call failed"
                        );
                        return Attempt::Retry {
                            error: unavailable(Some(error)),
                            retry_after: None,
                        };
                    }
                };
                let status = response.status();
                if status.is_success() {
                    return Attempt::Done(response);
                }
                let retry_after = http::retry_after(response.headers());
                let body = response.bytes().await.unwrap_or_default();
                classify(call, status, retry_after, &wire::error_codes(&body))
            }
        })
        .await
    }
}

/// Maps a failed response to an attempt, from its status and the short codes of its body.
fn classify<T>(
    call: &'static str,
    status: StatusCode,
    retry_after: Option<Duration>,
    codes: &[String],
) -> Attempt<T, FarmClientError> {
    let code = codes
        .first()
        .cloned()
        .unwrap_or_else(|| format!("http_{}", status.as_u16()));
    tracing::debug!(
        vendor = VENDOR,
        call,
        status = status.as_u16(),
        reason = code.as_str(),
        "google drive call refused"
    );
    let rate_limit_reason = codes
        .iter()
        .any(|c| c == "userRateLimitExceeded" || c == "rateLimitExceeded");
    let rate_limited = || Attempt::Retry {
        error: FarmClientError::RateLimited {
            vendor: VENDOR,
            retry_after,
        },
        retry_after,
    };
    match status {
        StatusCode::UNAUTHORIZED => Attempt::Fail(FarmClientError::AuthRefused {
            vendor: VENDOR,
            code,
        }),
        StatusCode::BAD_REQUEST if code == "invalid_grant" => {
            Attempt::Fail(FarmClientError::AuthRefused {
                vendor: VENDOR,
                code,
            })
        }
        StatusCode::TOO_MANY_REQUESTS => rate_limited(),
        StatusCode::FORBIDDEN if rate_limit_reason => rate_limited(),
        StatusCode::FORBIDDEN if scope_refused(call, codes) => {
            Attempt::Fail(FarmClientError::AuthRefused {
                vendor: VENDOR,
                code: codes
                    .iter()
                    .find(|c| is_scope_reason(c))
                    .cloned()
                    .unwrap_or(code),
            })
        }
        StatusCode::NOT_FOUND => Attempt::Fail(FarmClientError::NotFound {
            vendor: VENDOR,
            code,
        }),
        s if s == StatusCode::REQUEST_TIMEOUT || s.is_server_error() => Attempt::Retry {
            error: unavailable(None),
            retry_after,
        },
        s => Attempt::Fail(FarmClientError::Refused {
            vendor: VENDOR,
            status: s.as_u16(),
            code,
        }),
    }
}

/// The calls that read with the token's scope rather than one file's permissions: a `403`
/// `insufficientPermissions` there means the token lacks `drive.readonly`.
const SCOPE_CALLS: [&str; 4] = [
    "about.get",
    "files.list",
    "changes.getStartPageToken",
    "changes.list",
];

fn is_scope_reason(code: &str) -> bool {
    code == "ACCESS_TOKEN_SCOPE_INSUFFICIENT" || code == "insufficientPermissions"
}

/// A `403` the owner fixes by reconnecting: the token lacks the scope. Google's `ErrorInfo`
/// reason `ACCESS_TOKEN_SCOPE_INSUFFICIENT` says so on any call; `insufficientPermissions`
/// says so only on a scope call, since on one file it can be that file's own sharing.
fn scope_refused(call: &str, codes: &[String]) -> bool {
    codes.iter().any(|c| c == "ACCESS_TOKEN_SCOPE_INSUFFICIENT")
        || (SCOPE_CALLS.contains(&call) && codes.iter().any(|c| c == "insufficientPermissions"))
}

/// Reads a text body up to [`MAX_TEXT_BYTES`], then stops reading.
async fn read_text(mut response: Response) -> Result<FileText, FarmClientError> {
    let mut bytes = Vec::new();
    let mut truncated = false;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| unavailable(Some(TransportError::from_reqwest(e.without_url()))))?
    {
        let room = MAX_TEXT_BYTES - bytes.len();
        if chunk.len() > room {
            bytes.extend_from_slice(&chunk[..room]);
            truncated = true;
            break;
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(FileText {
        text: into_text(bytes),
        truncated,
    })
}

/// UTF-8 text from bytes: an incomplete character at the end (a cut) is dropped first, then
/// invalid bytes become U+FFFD. A replacement is 3 bytes for 1 invalid byte, so the text is cut
/// again on a character boundary if it grew past [`MAX_TEXT_BYTES`].
fn into_text(mut bytes: Vec<u8>) -> String {
    // Only the last 3 bytes can start an incomplete character.
    let tail = bytes.len().saturating_sub(3);
    if let Err(e) = std::str::from_utf8(&bytes[tail..])
        && e.error_len().is_none()
    {
        bytes.truncate(tail + e.valid_up_to());
    }
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if text.len() > MAX_TEXT_BYTES {
        let mut end = MAX_TEXT_BYTES;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}

/// A string inside single quotes of a Drive query: `\` and `'` are escaped with `\`.
fn quote(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

/// Percent-encodes everything but RFC 3986's unreserved characters.
fn percent_encode(value: &str, out: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0x0f)]));
        }
    }
}

/// A path segment, percent-encoded.
fn segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    percent_encode(value, &mut out);
    out
}

/// An `application/x-www-form-urlencoded` body. Built here because the workspace's `reqwest`
/// has no `form` feature.
fn form(pairs: &[(&str, &str)]) -> String {
    let mut out = String::new();
    for (i, (key, value)) in pairs.iter().enumerate() {
        if i > 0 {
            out.push('&');
        }
        percent_encode(key, &mut out);
        out.push('=');
        percent_encode(value, &mut out);
    }
    out
}

fn unavailable(source: Option<TransportError>) -> FarmClientError {
    FarmClientError::Unavailable {
        vendor: VENDOR,
        source,
    }
}

fn malformed(call: &'static str) -> FarmClientError {
    tracing::debug!(vendor = VENDOR, call, "google drive answer did not parse");
    FarmClientError::Malformed { vendor: VENDOR }
}
