//! `garden.json`, format 1, and its loader (the fixture's README, `garden.json`): the garden,
//! its members, the plants and the memories, written through the store's ports as the routes
//! and the pipeline would leave them.
//!
//! Two parts of the format wait on more than the embedded store holds: the plants' species
//! (resolved against the knowledge base, which an evaluation store doesn't load) and the scans
//! with their diagnoses (whose `primary_problem` is a knowledge-base record). The plants are
//! created without a species, and the scans are reported as not loaded; each scan's pipeline
//! summary (`MEM_SC_*`) is loaded with the memories, so the agent still sees what it found.

use std::collections::BTreeMap;

use anyhow::{Context as _, bail};
use serde::Deserialize;

use domain::{
    authz::{Editor, Owner, check_membership_of},
    chat::{MemoryKind, MemorySource, MemoryTopic},
    garden::gardens::TimeZone,
    garden::plants::UpdatePlantRequest,
    id::{AuthId, GardenId, InviteCode, PlantId, UserId},
    ports::{
        EmbedMode, IdGen, IdGenExt as _, MemoryStore, NewGarden, NewInvite, NewMemory,
        ResolvedLocation, Store, TextEmbedder,
    },
    records::weather::{Coordinates, LocationPrecision},
    scan::PlantStatus,
    wire::{Role, Timestamp},
};

/// The whole file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GardenFile {
    /// `1`.
    pub format: u32,
    /// The time the evaluation runs at: the pinned clock.
    pub as_of: Timestamp,
    /// The garden.
    pub garden: Garden,
    /// Its members; the first owner creates the garden.
    pub members: Vec<Member>,
    /// Its plants.
    pub plants: Vec<Plant>,
    /// Its scans.
    pub scans: Vec<serde_json::Value>,
    /// Its memories.
    pub memories: Vec<Memory>,
}

/// `garden`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Garden {
    /// `G_MAPLE`.
    #[expect(
        dead_code,
        reason = "part of the format; the loader mints the garden's id"
    )]
    pub key: String,
    /// Its name.
    pub name: String,
    /// City only.
    pub location: Location,
    /// What the geocoder must derive.
    pub time_zone: String,
}

/// `garden.location`: a `GardenLocationInput` with a city.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Location {
    /// `"Guelph, ON"`.
    pub city: String,
}

/// `members[]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    /// `M_MARIA`.
    pub key: String,
    /// Their name.
    pub display_name: String,
    /// Their role.
    pub role: Role,
    /// The fixture's Slack id; for reading answers only, never stored.
    #[expect(dead_code, reason = "part of the format; never stored")]
    pub slack_user: Option<String>,
}

/// `plants[]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plant {
    /// `P_BED3`.
    pub key: String,
    /// `Bed 3`.
    pub name: String,
    /// The species, as in `SpeciesRef` without the id.
    #[expect(
        dead_code,
        reason = "resolved against a knowledge base the bench doesn't load"
    )]
    pub species: serde_json::Value,
    /// `confirmed`.
    #[expect(
        dead_code,
        reason = "resolved against a knowledge base the bench doesn't load"
    )]
    pub species_status: String,
    /// Its status.
    pub status: PlantStatus,
    /// When it was created.
    #[expect(dead_code, reason = "the store sets a plant's creation time")]
    pub created_at: Timestamp,
}

/// `memories[]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Memory {
    /// `MEM_SC_BED3`.
    pub key: String,
    /// The plant's key; absent for the garden.
    #[serde(default)]
    pub plant: Option<String>,
    /// The scan's key, for a pipeline summary.
    #[serde(default)]
    #[expect(dead_code, reason = "the scans aren't loaded; see the module")]
    pub scan: Option<String>,
    /// Its kind.
    pub kind: MemoryKind,
    /// Its topic.
    pub topic: MemoryTopic,
    /// `pipeline` or `user`.
    pub source: MemorySource,
    /// The member's key, for a `user` memory.
    #[serde(default)]
    pub author: Option<String>,
    /// Shared with the garden.
    #[expect(
        dead_code,
        reason = "pipeline summaries and garden notes are always shared"
    )]
    pub shared: bool,
    /// Its text.
    pub text: String,
    /// When.
    pub created_at: Timestamp,
}

impl GardenFile {
    /// Parses and checks the format.
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let file: Self = serde_json::from_str(text)?;
        anyhow::ensure!(file.format == 1, "garden.json format {}", file.format);
        Ok(file)
    }
}

/// What the loader made, by key.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// The garden.
    pub garden: GardenId,
    /// Each member's user.
    pub users: BTreeMap<String, UserId>,
    /// Each member's auth id, to sign in as them.
    pub auth_ids: BTreeMap<String, AuthId>,
    /// The scans it couldn't write, by key.
    pub skipped_scans: Vec<String>,
}

/// The coordinates of the cities a fixture may name (the geocoder isn't called).
fn city(name: &str) -> anyhow::Result<Coordinates> {
    match name {
        "Guelph, ON" => Ok(Coordinates {
            lat: 43.55,
            lon: -80.25,
        }),
        other => bail!("no coordinates for {other}: add the city to era-eval's garden.rs"),
    }
}

/// An invite code: 26 base32 letters, unique per member.
fn invite_code(n: usize) -> anyhow::Result<InviteCode> {
    let letter = char::from(b'A' + u8::try_from(n % 26)?);
    format!("{}{letter}", "A".repeat(25))
        .parse()
        .map_err(|e| anyhow::anyhow!("invite code: {e:?}"))
}

/// Writes the garden, its members, plants and memories.
pub async fn load(
    file: &GardenFile,
    store: &dyn Store,
    memory: &dyn MemoryStore,
    embedder: &dyn TextEmbedder,
    ids: &dyn IdGen,
) -> anyhow::Result<Loaded> {
    let (users, auth_ids) = users(file, store).await?;
    let garden = garden(file, store, ids, &users).await?;
    let plants = plants(file, store, ids, &users, garden).await?;
    let pieces = Pieces {
        store,
        memory,
        embedder,
        ids,
        users: &users,
        plants: &plants,
        garden,
    };
    memories(file, &pieces).await?;
    let skipped_scans = file
        .scans
        .iter()
        .filter_map(|s| s["key"].as_str().map(str::to_owned))
        .collect();
    Ok(Loaded {
        garden,
        users,
        auth_ids,
        skipped_scans,
    })
}

/// The owner: the first member with that role.
pub fn owner(file: &GardenFile) -> anyhow::Result<&Member> {
    file.members
        .iter()
        .find(|m| m.role == Role::Owner)
        .context("garden.json has no owner")
}

type Users = BTreeMap<String, UserId>;

/// A user for every member, as signing in first creates it.
async fn users(
    file: &GardenFile,
    store: &dyn Store,
) -> anyhow::Result<(Users, BTreeMap<String, AuthId>)> {
    let mut users = BTreeMap::new();
    let mut auth_ids = BTreeMap::new();
    for m in &file.members {
        let auth: AuthId = format!("user_{}", m.key.to_lowercase())
            .parse()
            .map_err(|e| anyhow::anyhow!("auth id for {}: {e:?}", m.key))?;
        let user = store
            .upsert_user(&auth, Some(m.display_name.clone()))
            .await?;
        users.insert(m.key.clone(), user);
        auth_ids.insert(m.key.clone(), auth);
    }
    Ok((users, auth_ids))
}

/// The garden, created by its owner, and the other members through an invite each.
async fn garden(
    file: &GardenFile,
    store: &dyn Store,
    ids: &dyn IdGen,
    users: &Users,
) -> anyhow::Result<GardenId> {
    let owner = owner(file)?;
    let garden: GardenId = ids.id();
    store
        .create_garden(
            users[&owner.key],
            NewGarden {
                id: garden,
                name: file.garden.name.clone(),
                location: ResolvedLocation {
                    coordinates: city(&file.garden.location.city)?,
                    precision: LocationPrecision::City,
                    label: file.garden.location.city.clone(),
                    time_zone: TimeZone(file.garden.time_zone.clone()),
                },
            },
        )
        .await?;
    let owns = check_membership_of::<Owner>(store, users[&owner.key], garden).await?;
    for (n, m) in file.members.iter().enumerate() {
        if m.key == owner.key {
            continue;
        }
        let code = invite_code(n)?;
        store
            .create_invite(
                owns,
                NewInvite {
                    id: ids.id(),
                    code: code.clone(),
                    role: m.role,
                    expires_at: file.as_of.plus(std::time::Duration::from_hours(24)),
                },
            )
            .await?;
        store
            .accept_invite(users[&m.key], &code, file.as_of)
            .await?;
    }
    Ok(garden)
}

/// The plants, created by the owner with their status (and no species: see the module).
async fn plants(
    file: &GardenFile,
    store: &dyn Store,
    ids: &dyn IdGen,
    users: &Users,
    garden: GardenId,
) -> anyhow::Result<BTreeMap<String, PlantId>> {
    let editor = check_membership_of::<Editor>(store, users[&owner(file)?.key], garden).await?;
    let mut plants = BTreeMap::new();
    for p in &file.plants {
        let id: PlantId = ids.id();
        store
            .create_plant(
                editor,
                domain::ports::NewPlant {
                    id,
                    name: p.name.clone(),
                    species: None,
                },
            )
            .await?;
        store
            .update_plant(
                editor,
                id,
                UpdatePlantRequest {
                    name: None,
                    status: Some(p.status),
                },
            )
            .await?;
        plants.insert(p.key.clone(), id);
    }
    Ok(plants)
}

/// What the memories are written with.
struct Pieces<'a> {
    store: &'a dyn Store,
    memory: &'a dyn MemoryStore,
    embedder: &'a dyn TextEmbedder,
    ids: &'a dyn IdGen,
    users: &'a Users,
    plants: &'a BTreeMap<String, PlantId>,
    garden: GardenId,
}

/// The memories through the `MemoryStore` port, which keeps their kind and time: pipeline
/// summaries as the pipeline writes them, the rest as their author's garden notes.
async fn memories(file: &GardenFile, p: &Pieces<'_>) -> anyhow::Result<()> {
    let model = p.embedder.model().name;
    for m in &file.memories {
        let plant = match &m.plant {
            Some(key) => Some(
                *p.plants
                    .get(key)
                    .with_context(|| format!("no plant {key}"))?,
            ),
            None => None,
        };
        let embedding = p
            .embedder
            .embed(EmbedMode::Document, std::slice::from_ref(&m.text))
            .await?
            .pop()
            .context("no embedding")?;
        let new = NewMemory {
            id: p.ids.id(),
            plant,
            kind: m.kind,
            topic: m.topic,
            text: m.text.clone(),
            embedding,
            embedding_model: model.clone(),
            source_msg: None,
            created_at: m.created_at,
        };
        match m.source {
            MemorySource::Pipeline => {
                p.memory.add_pipeline_memory(p.garden, new).await?;
            }
            MemorySource::User => {
                let author = m
                    .author
                    .as_ref()
                    .and_then(|a| p.users.get(a))
                    .with_context(|| format!("{}: no author", m.key))?;
                let proof = check_membership_of::<Editor>(p.store, *author, p.garden).await?;
                p.memory.add_note(proof, new).await?;
            }
            MemorySource::Chat => bail!("{}: a chat memory isn't part of the format", m.key),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixture_parses() {
        let file = GardenFile::parse(include_str!(
            "../../../tests/fixtures/era/maple-row/garden.json"
        ))
        .unwrap();
        assert_eq!(file.plants.len(), 6);
        assert_eq!(file.memories.len(), 5);
        assert!(city(&file.garden.location.city).is_ok());
    }

    #[test]
    fn invite_codes_are_valid_and_distinct() {
        let a = invite_code(0).unwrap();
        let b = invite_code(1).unwrap();
        assert_ne!(a, b);
    }
}
