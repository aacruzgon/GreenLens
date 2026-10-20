//! Start-up seeding of the demo garden's `era` connectors ([connectors 9.4], docs/EON E4.6 and
//! gap G4). It runs after `deletions.resume()`, on every start, and only when connectors are on
//! (`CONNECTOR_KEY`) and `ERA_SLACK_BASE`, `ERA_GOOGLE_BASE` and `ERA_DEMO_GARDEN_ID` are all
//! set. Each connector has a fixed id. The store creates it when that id is free; when it exists
//! in the demo garden it is repaired on every start: the token is sealed again under the current
//! `CONNECTOR_KEY` (a rotated key would otherwise leave the demo unable to sync), `status` goes
//! back to `active` and `last_error` is cleared. A connector with that id in another garden is
//! left alone and logged as a warning ([`Seeded::Elsewhere`]). A garden being deleted
//! ([`Seeded::GardenDeleting`]) and a connector being deleted ([`Seeded::ConnectorDeleting`])
//! are left to their deletion and logged; once it finishes, the next start seeds afresh.
//! Their tokens are the fixed strings the simulators accept in open mode, sealed under
//! `CONNECTOR_KEY` like any token. No route can create an `era` connector.
//!
//! The garden is the one `ERA_DEMO_GARDEN_ID` names: E9.0 creates it through the API, so its
//! id is known only after, and the deploy sets the variable. A garden that doesn't exist yet
//! (no members) is logged and skipped; the next start seeds it. The connectors are created by
//! the garden's first owner, so they go if that owner stops being one, like any connector.
//!
//! The sources are the Maple Row Farm fixture's (`backend/tests/fixtures/era/maple-row/`),
//! which E9.1 injects into the simulators' default tenant. The Slack account's workspace URL is
//! the simulator's own base URL, so a record's link points at the simulator (the app shows an
//! `era` record's text instead of opening it, EON decision 10); no call is made at start-up.
//!
//! [connectors 9.4]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#94-the-demo

use domain::{
    connectors::{
        ConnectorAccount, ConnectorEndpoint, ConnectorKind, ConnectorSource, ConnectorStatus,
    },
    id::{ConnectorId, GardenId, UserId},
    ports::{Clock, DeletionTarget, NewConnector, Store, StoreError},
    wire::Role,
};

use crate::{
    error::chain,
    sealing::{ConnectorKey, SealPurpose},
    state::AppState,
};

/// The seeded Slack connector's fixed id.
pub const ERA_SLACK_CONNECTOR: &str = "01K7ERA5EED000000000000001";
/// The seeded Drive connector's fixed id.
pub const ERA_DRIVE_CONNECTOR: &str = "01K7ERA5EED000000000000002";
/// The Slack simulator's token in open mode: any `xoxb-…` works.
pub const ERA_SLACK_TOKEN: &str = "xoxb-era-demo";
/// The Drive simulator's token in open mode: any bearer works.
pub const ERA_DRIVE_TOKEN: &str = "ya29.era-demo";

/// What seeding needs, from the configuration.
#[derive(Debug, Clone)]
pub struct EraSeed {
    /// `ERA_SLACK_BASE`, without a trailing slash.
    pub slack_base: String,
    /// `ERA_DEMO_GARDEN_ID`.
    pub garden: GardenId,
}

/// What seeding did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seeded {
    /// Both connectors exist in the demo garden (created or repaired now).
    Done,
    /// A connector with a fixed id lives in another garden and was left as it was; this is
    /// that garden. The demo garden is missing it.
    Elsewhere(GardenId),
    /// A connector with a fixed id is being deleted in the demo garden and was left so; the
    /// first start after its deletion finishes creates it again.
    ConnectorDeleting,
    /// The garden has no owner (it doesn't exist yet): nothing was created.
    NoGarden,
    /// The garden is being deleted: nothing was created or repaired.
    GardenDeleting,
}

/// The start-up step: seeds when connectors are on and the three variables are set, and logs
/// what it did. A failure is logged, not fatal: the API serves without the demo connectors, and
/// the next start tries again.
pub async fn at_start(state: &AppState) {
    let c = &state.config.connectors;
    // Both simulators, as gap G4 says: the demo has a Slack and a Drive connector.
    let (Some(key), Some(slack), Some(_)) = (
        state.connector_key(),
        c.era_slack_base.as_ref(),
        c.era_google_base.as_ref(),
    ) else {
        return;
    };
    let Some(garden) = c.era_demo_garden else {
        tracing::info!("era connectors not seeded: ERA_DEMO_GARDEN_ID is not set");
        return;
    };
    let seed = EraSeed {
        slack_base: slack.clone(),
        garden,
    };
    let p = &state.ports;
    match seed_era_connectors(p.store.as_ref(), key, p.clock.as_ref(), &seed).await {
        Ok(Seeded::Done) => tracing::info!(%garden, "era connectors seeded"),
        Ok(Seeded::Elsewhere(other)) => tracing::warn!(
            %garden,
            %other,
            "era connectors not seeded: a connector with a fixed id lives in another garden"
        ),
        Ok(Seeded::ConnectorDeleting) => tracing::info!(
            %garden,
            "era connectors not all seeded: one is being deleted; a start after its deletion seeds it"
        ),
        Ok(Seeded::NoGarden) => tracing::info!(
            %garden,
            "era connectors not seeded: the demo garden has no owner yet; the next start seeds it"
        ),
        Ok(Seeded::GardenDeleting) => tracing::warn!(
            %garden,
            "era connectors not seeded: the demo garden is being deleted; point ERA_DEMO_GARDEN_ID at another"
        ),
        Err(e) => tracing::warn!(%garden, error = %chain(&e), "era connectors not seeded"),
    }
}

/// Creates the two `era` connectors if their ids are free, and repairs those that exist in the
/// demo garden (token sealed under `key`, `status` active, no `last_error`).
pub async fn seed_era_connectors(
    store: &dyn Store,
    key: &ConnectorKey,
    clock: &dyn Clock,
    seed: &EraSeed,
) -> Result<Seeded, StoreError> {
    let Some(owner) = first_owner(store, seed.garden).await? else {
        // A garden being deleted has no owner to the store either.
        let deleting = store
            .unfinished_deletions()
            .await?
            .contains(&DeletionTarget::Garden(seed.garden));
        return Ok(if deleting {
            Seeded::GardenDeleting
        } else {
            Seeded::NoGarden
        });
    };
    let now = clock.now();
    let slack: ConnectorId = ERA_SLACK_CONNECTOR.parse().expect("a fixed, valid ULID");
    let drive: ConnectorId = ERA_DRIVE_CONNECTOR.parse().expect("a fixed, valid ULID");
    let connectors = [
        (
            slack,
            ConnectorKind::Slack,
            ConnectorAccount::Slack {
                team_id: "T_MAPLE".to_owned(),
                team_name: "Maple Row Farm".to_owned(),
                url: format!("{}/", seed.slack_base),
            },
            vec![source("C_FIELD", "field-reports"), source("C_CREW", "crew")],
            ERA_SLACK_TOKEN,
        ),
        (
            drive,
            ConnectorKind::GoogleDrive,
            ConnectorAccount::GoogleDrive {
                email: "demo@maplerowfarm.example".to_owned(),
            },
            vec![source("F_FARM", "Farm records")],
            ERA_DRIVE_TOKEN,
        ),
    ];
    let mut elsewhere = None;
    let mut deleting = false;
    for (id, kind, account, sources, token) in connectors {
        let connector = NewConnector {
            id,
            kind,
            status: ConnectorStatus::Active,
            endpoint: ConnectorEndpoint::Era,
            created_by: owner,
            account: Some(account),
            secret: Some(key.seal(SealPurpose::Token, id, token.as_bytes())),
            oauth: None,
            sources,
            created_at: now,
        };
        let stored = store.seed_connector(seed.garden, connector).await?;
        if stored.garden_id != seed.garden {
            elsewhere = Some(stored.garden_id);
        } else if stored.status == ConnectorStatus::Deleting {
            deleting = true;
        }
    }
    Ok(match (elsewhere, deleting) {
        (Some(other), _) => Seeded::Elsewhere(other),
        (None, true) => Seeded::ConnectorDeleting,
        (None, false) => Seeded::Done,
    })
}

/// The garden's first owner, in the store's member order; `None` when it has none.
async fn first_owner(store: &dyn Store, garden: GardenId) -> Result<Option<UserId>, StoreError> {
    for user in store.member_ids(garden).await? {
        if store.role_in(user, garden).await? == Some(Role::Owner) {
            return Ok(Some(user));
        }
    }
    Ok(None)
}

fn source(id: &str, name: &str) -> ConnectorSource {
    ConnectorSource {
        id: id.to_owned(),
        name: name.to_owned(),
    }
}
