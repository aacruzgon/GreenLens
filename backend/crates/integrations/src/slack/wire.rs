//! Slack Web API response shapes, as far as PDM reads them. Unknown fields are ignored, so a
//! field we don't declare (a user's `profile.email`, above all) is never deserialised.

use serde::Deserialize;

/// Every Web API answer: `ok`, and `error` when `ok` is false. A `403 writes_disabled` from
/// Era's simulator has `error` without `ok`.
#[derive(Deserialize)]
pub(super) struct Envelope {
    #[serde(default)]
    pub ok: Option<bool>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Deserialize, Default)]
pub(super) struct ResponseMetadata {
    #[serde(default)]
    pub next_cursor: Option<String>,
}

impl ResponseMetadata {
    /// The cursor of the next page; `None` on the last page (absent or empty).
    pub fn next(self) -> Option<String> {
        self.next_cursor.filter(|c| !c.is_empty())
    }
}

#[derive(Deserialize)]
pub(super) struct AuthTest {
    pub team: String,
    pub team_id: String,
    pub url: String,
    pub user_id: String,
    #[serde(default)]
    pub bot_id: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct Channel {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub is_member: bool,
}

#[derive(Deserialize)]
pub(super) struct ChannelList {
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub response_metadata: Option<ResponseMetadata>,
}

#[derive(Deserialize)]
pub(super) struct Message {
    pub ts: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub thread_ts: Option<String>,
    #[serde(default)]
    pub reply_count: u32,
    #[serde(default)]
    pub latest_reply: Option<String>,
    #[serde(default)]
    pub subtype: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct Messages {
    #[serde(default)]
    pub messages: Vec<Message>,
    #[serde(default)]
    pub response_metadata: Option<ResponseMetadata>,
}

/// `users.info`'s `profile`: the names only. Slack sends `email` with `users:read.email` (which
/// PDM doesn't ask for), and the simulator sends it regardless; it is not a field here.
#[derive(Deserialize, Default)]
pub(super) struct Profile {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub real_name: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct User {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub real_name: Option<String>,
    #[serde(default)]
    pub profile: Option<Profile>,
}

#[derive(Deserialize)]
pub(super) struct UserInfo {
    pub user: User,
}

#[derive(Deserialize)]
pub(super) struct Team {
    pub id: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Deserialize)]
pub(super) struct OAuthAccess {
    pub access_token: String,
    #[serde(default)]
    pub token_type: Option<String>,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub bot_user_id: Option<String>,
    #[serde(default)]
    pub app_id: Option<String>,
    pub team: Team,
}

/// An answer whose payload PDM doesn't read (`auth.revoke`).
#[derive(Deserialize)]
pub(super) struct Empty {}
