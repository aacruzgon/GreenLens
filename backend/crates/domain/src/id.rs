//! Typed identifiers ([contracts 1, wire format]).
//!
//! - [`Id<T>`]: the ULID key of a `SurrealDB` record, typed by its table, sent as the bare
//!   26-character string (`"01JAX3K9T6R2B8V0QZ4M7D1E5F"`).
//! - [`Slug<K>`]: a knowledge-base id (`"tomato_early_blight"`), typed by its table.
//! - [`IndicatorId`], [`RegionId`], [`ChunkId`], [`FarmRecordId`]: compound string ids with a
//!   checked format.
//! - [`WorkerId`], [`LeaseToken`], [`IdempotencyKey`], [`AuthId`], [`InviteCode`],
//!   [`PushToken`], [`Cursor`], [`StepId`], [`DeviceVerifier`], [`DeviceHash`]: opaque strings
//!   with a checked format.
//!
//! `domain` never depends on `surrealdb`: the `store` crate maps `Id<T>` to
//! `RecordId::new(T::NAME, id.to_string())` and back ([database 2]).

use std::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
    marker::PhantomData,
    str::FromStr,
    sync::Arc,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use ulid::Ulid;

use crate::schema_util::string_schema;

/// Why a string isn't a valid id of the expected kind.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid {kind}: {problem}")]
pub struct IdParseError {
    /// What was being parsed, such as `"scan id"` or `"indicator id"`.
    pub kind: &'static str,
    /// What is wrong with it.
    pub problem: &'static str,
}

impl IdParseError {
    pub(crate) const fn new(kind: &'static str, problem: &'static str) -> Self {
        Self { kind, problem }
    }
}

// ---------------------------------------------------------------------------------------------
// Record ids
// ---------------------------------------------------------------------------------------------

/// A `SurrealDB` table whose records have a ULID key. Implemented by the uninhabited markers in
/// [`table`]; `NAME` is the table name in `schema.surql`.
pub trait Table: 'static {
    /// The table name, as in `DEFINE TABLE` ([database 3]).
    const NAME: &'static str;
}

/// The ULID key of a record in table `T`.
///
/// `PhantomData<fn() -> T>` rather than `PhantomData<T>`: an `Id<T>` never owns or drops a `T`,
/// so the marker should say "produces a `T`", which is covariant in `T` and is always `Send`,
/// `Sync`, `Unpin` and `'static`-friendly whatever `T` is. The markers are uninhabited enums,
/// so this changes nothing today, but it keeps the auto traits independent of the marker type
/// (Rust Reference, `PhantomData` and variance; std `PhantomData` docs, "ownership and the drop
/// check"). The trait impls are written by hand because `#[derive]` would add a `T: Clone`
/// (etc.) bound that the markers don't and needn't satisfy.
pub struct Id<T: Table> {
    ulid: Ulid,
    table: PhantomData<fn() -> T>,
}

impl<T: Table> Id<T> {
    /// Wraps an existing ULID.
    pub const fn from_ulid(ulid: Ulid) -> Self {
        Self {
            ulid,
            table: PhantomData,
        }
    }

    /// The ULID inside.
    pub const fn ulid(self) -> Ulid {
        self.ulid
    }

    /// The table this id belongs to.
    pub const fn table(self) -> &'static str {
        T::NAME
    }

    /// The creation time encoded in the ULID, in milliseconds since the Unix epoch.
    pub const fn timestamp_ms(self) -> u64 {
        self.ulid.timestamp_ms()
    }
}

impl<T: Table> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: Table> Copy for Id<T> {}
impl<T: Table> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.ulid == other.ulid
    }
}
impl<T: Table> Eq for Id<T> {}
impl<T: Table> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ulid.hash(state);
    }
}
impl<T: Table> PartialOrd for Id<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<T: Table> Ord for Id<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.ulid.cmp(&other.ulid)
    }
}
impl<T: Table> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", T::NAME, self.ulid)
    }
}
/// The bare ULID, as on the wire.
impl<T: Table> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.ulid, f)
    }
}
impl<T: Table> FromStr for Id<T> {
    type Err = IdParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != ulid::ULID_LEN {
            return Err(IdParseError::new("id", "expected 26 characters"));
        }
        Ulid::from_string(s)
            .map(Self::from_ulid)
            .map_err(|_| IdParseError::new("id", "not a ULID"))
    }
}
impl<T: Table> Serialize for Id<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut buf = [0u8; ulid::ULID_LEN];
        serializer.serialize_str(self.ulid.array_to_str(&mut buf))
    }
}
impl<'de, T: Table> Deserialize<'de> for Id<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        s.parse().map_err(de::Error::custom)
    }
}
impl<T: Table> utoipa::PartialSchema for Id<T> {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        crate::schema_util::string(
            Some(ULID_PATTERN),
            "ULID record key, without the table prefix",
        )
    }
}
impl<T: Table> utoipa::ToSchema for Id<T> {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Id")
    }
}
impl<T: Table> schemars::JsonSchema for Id<T> {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Id")
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        crate::schema_util::json_string(Some(ULID_PATTERN), "ULID record key")
    }
}

/// Crockford base32, 26 characters.
pub const ULID_PATTERN: &str = "^[0-9A-HJKMNP-TV-Z]{26}$";

/// One uninhabited marker per table with a ULID key in `schema.surql`. Tables with natural
/// array keys (`member_of`, `scan_event`, `push_token`, `worker`, `usage_daily`,
/// `idempotency_key`, `weather_cache`, `message_feedback`, `classifier_label`,
/// `soil_condition`) and knowledge tables (slugs) have none.
pub mod table {
    use super::Table;

    macro_rules! tables {
        ($($(#[$doc:meta])* $marker:ident => $name:literal),* $(,)?) => {
            $(
                $(#[$doc])*
                #[derive(Debug)]
                pub enum $marker {}
                impl Table for $marker {
                    const NAME: &'static str = $name;
                }
            )*
        };
    }

    tables! {
        /// `user`
        User => "user",
        /// `garden`
        Garden => "garden",
        /// `invite`
        Invite => "invite",
        /// `plant`
        Plant => "plant",
        /// `garden_map`
        GardenMap => "garden_map",
        /// `scan`
        Scan => "scan",
        /// `scan_image`
        ScanImage => "scan_image",
        /// `mask`
        Mask => "mask",
        /// `observation`
        Observation => "observation",
        /// `question`
        Question => "question",
        /// `guided_check`
        GuidedCheck => "guided_check",
        /// `diagnosis`
        Diagnosis => "diagnosis",
        /// `scan_feedback`
        ScanFeedback => "scan_feedback",
        /// `action_plan`
        ActionPlan => "action_plan",
        /// `plan_trigger`
        PlanTrigger => "plan_trigger",
        /// `task`
        Task => "task",
        /// `conversation`
        Conversation => "conversation",
        /// `chat_image`
        ChatImage => "chat_image",
        /// `message`
        Message => "message",
        /// `pending_action`
        PendingAction => "pending_action",
        /// `memory`
        Memory => "memory",
        /// `job`
        Job => "job",
        /// `connector`
        Connector => "connector",
    }
}

/// `user` record key.
pub type UserId = Id<table::User>;
/// `garden` record key.
pub type GardenId = Id<table::Garden>;
/// `invite` record key.
pub type InviteId = Id<table::Invite>;
/// `plant` record key.
pub type PlantId = Id<table::Plant>;
/// `garden_map` record key.
pub type GardenMapId = Id<table::GardenMap>;
/// `scan` record key.
pub type ScanId = Id<table::Scan>;
/// `scan_image` record key: the `image_id` of contracts 3 and 5.
pub type ImageId = Id<table::ScanImage>;
/// `mask` record key.
pub type MaskId = Id<table::Mask>;
/// `observation` record key.
pub type ObservationId = Id<table::Observation>;
/// `question` record key.
pub type QuestionId = Id<table::Question>;
/// `guided_check` record key.
pub type GuidedCheckId = Id<table::GuidedCheck>;
/// `diagnosis` record key.
pub type DiagnosisId = Id<table::Diagnosis>;
/// `scan_feedback` record key.
pub type FeedbackId = Id<table::ScanFeedback>;
/// `action_plan` record key.
pub type PlanId = Id<table::ActionPlan>;
/// `plan_trigger` record key.
pub type PlanTriggerId = Id<table::PlanTrigger>;
/// `task` record key.
pub type TaskId = Id<table::Task>;
/// `conversation` record key.
pub type ConversationId = Id<table::Conversation>;
/// `chat_image` record key.
pub type ChatImageId = Id<table::ChatImage>;
/// `message` record key.
pub type MessageId = Id<table::Message>;
/// `pending_action` record key.
pub type PendingActionId = Id<table::PendingAction>;
/// `memory` record key.
pub type MemoryId = Id<table::Memory>;
/// `job` record key.
pub type JobId = Id<table::Job>;
/// `connector` record key.
pub type ConnectorId = Id<table::Connector>;

// ---------------------------------------------------------------------------------------------
// Knowledge slugs
// ---------------------------------------------------------------------------------------------

/// A knowledge-base table whose records are keyed by a slug ([contracts 10]).
pub trait SlugKind: 'static {
    /// The table name.
    const TABLE: &'static str;
}

/// Markers for the knowledge tables keyed by slugs.
pub mod kb {
    use super::SlugKind;

    macro_rules! kinds {
        ($($(#[$doc:meta])* $marker:ident => $name:literal),* $(,)?) => {
            $(
                $(#[$doc])*
                #[derive(Debug)]
                pub enum $marker {}
                impl SlugKind for $marker {
                    const TABLE: &'static str = $name;
                }
            )*
        };
    }

    kinds! {
        /// `species`
        Species => "species",
        /// `disease`: every kind of problem (contracts 10)
        Problem => "disease",
        /// `organism`
        Organism => "organism",
        /// `treatment`
        Treatment => "treatment",
        /// `source`
        Source => "source",
        /// An indicator's own slug, after the dot of its [`IndicatorId`]; there is no table.
        Indicator => "indicator",
    }
}

/// A knowledge-base id: `[a-z][a-z0-9_]*`, at most 64 characters, unique across all tables and
/// never reused ([contracts 10]). Cheap to clone (`Arc<str>`), because candidate lists, scores
/// and prompts copy slugs often. The marker works as in [`Id`].
pub struct Slug<K: SlugKind> {
    value: Arc<str>,
    kind: PhantomData<fn() -> K>,
}

/// The longest slug accepted.
pub const SLUG_MAX_LEN: usize = 64;
/// The slug format, as a regular expression.
pub const SLUG_PATTERN: &str = "^[a-z][a-z0-9_]{0,63}$";

fn check_slug(s: &str) -> Result<(), IdParseError> {
    let mut chars = s.chars();
    match chars.next() {
        None => return Err(IdParseError::new("slug", "empty")),
        Some(c) if !c.is_ascii_lowercase() => {
            return Err(IdParseError::new("slug", "must start with a-z"));
        }
        Some(_) => {}
    }
    if s.len() > SLUG_MAX_LEN {
        return Err(IdParseError::new("slug", "longer than 64 characters"));
    }
    if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return Err(IdParseError::new("slug", "only a-z, 0-9 and _ are allowed"));
    }
    Ok(())
}

impl<K: SlugKind> Slug<K> {
    /// The slug text.
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// The table the slug belongs to.
    pub const fn table(&self) -> &'static str {
        K::TABLE
    }
}

impl<K: SlugKind> Clone for Slug<K> {
    fn clone(&self) -> Self {
        Self {
            value: Arc::clone(&self.value),
            kind: PhantomData,
        }
    }
}
impl<K: SlugKind> PartialEq for Slug<K> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}
impl<K: SlugKind> Eq for Slug<K> {}
impl<K: SlugKind> Hash for Slug<K> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}
impl<K: SlugKind> PartialOrd for Slug<K> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<K: SlugKind> Ord for Slug<K> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}
impl<K: SlugKind> fmt::Debug for Slug<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", K::TABLE, self.value)
    }
}
impl<K: SlugKind> fmt::Display for Slug<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.value)
    }
}
impl<K: SlugKind> AsRef<str> for Slug<K> {
    fn as_ref(&self) -> &str {
        &self.value
    }
}
impl<K: SlugKind> FromStr for Slug<K> {
    type Err = IdParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        check_slug(s)?;
        if K::TABLE == "disease" && s == HEALTHY {
            return Err(IdParseError::new("slug", "\"healthy\" is reserved"));
        }
        Ok(Self {
            value: Arc::from(s),
            kind: PhantomData,
        })
    }
}
impl<K: SlugKind> Serialize for Slug<K> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.value)
    }
}
impl<'de, K: SlugKind> Deserialize<'de> for Slug<K> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        s.parse().map_err(de::Error::custom)
    }
}
impl<K: SlugKind> utoipa::PartialSchema for Slug<K> {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        crate::schema_util::string(Some(SLUG_PATTERN), "knowledge-base id")
    }
}
impl<K: SlugKind> utoipa::ToSchema for Slug<K> {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Slug")
    }
}
impl<K: SlugKind> schemars::JsonSchema for Slug<K> {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Slug")
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        crate::schema_util::json_string(Some(SLUG_PATTERN), "knowledge-base id")
    }
}

/// The classifier's "no problem" label, which takes a problem id's place in
/// `problem_id: Slug | "healthy"` and is therefore never a problem slug.
pub const HEALTHY: &str = "healthy";

/// `species` slug.
pub type SpeciesId = Slug<kb::Species>;
/// `disease` slug: any kind of problem.
pub type ProblemId = Slug<kb::Problem>;
/// `organism` slug.
pub type OrganismId = Slug<kb::Organism>;
/// `treatment` slug.
pub type TreatmentId = Slug<kb::Treatment>;
/// `source` slug.
pub type SourceId = Slug<kb::Source>;
/// An indicator's own slug (`concentric_rings`), without its problem: [`IndicatorId::slug`].
pub type IndicatorSlug = Slug<kb::Indicator>;

/// `Slug | "healthy"` ([contracts 5] disease results, [contracts 10] classifier labels).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProblemOrHealthy {
    /// A problem of the knowledge base.
    Problem(ProblemId),
    /// The classifier's healthy class.
    Healthy,
}

impl ProblemOrHealthy {
    /// The problem, unless healthy.
    pub fn problem(&self) -> Option<&ProblemId> {
        match self {
            Self::Problem(p) => Some(p),
            Self::Healthy => None,
        }
    }
}
impl fmt::Display for ProblemOrHealthy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Problem(p) => fmt::Display::fmt(p, f),
            Self::Healthy => f.write_str(HEALTHY),
        }
    }
}
impl FromStr for ProblemOrHealthy {
    type Err = IdParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == HEALTHY {
            Ok(Self::Healthy)
        } else {
            s.parse().map(Self::Problem)
        }
    }
}
impl Serialize for ProblemOrHealthy {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}
impl<'de> Deserialize<'de> for ProblemOrHealthy {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        s.parse().map_err(de::Error::custom)
    }
}
string_schema!(
    ProblemOrHealthy,
    "ProblemOrHealthy",
    Some(SLUG_PATTERN),
    "a problem slug, or \"healthy\""
);

// ---------------------------------------------------------------------------------------------
// Compound ids
// ---------------------------------------------------------------------------------------------

/// `<problem_id>.<indicator_slug>` ([contracts 10]), such as
/// `"tomato_early_blight.concentric_rings"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IndicatorId {
    problem: ProblemId,
    slug: Arc<str>,
}

impl IndicatorId {
    /// Builds an indicator id from its problem and its own slug.
    pub fn new(problem: ProblemId, slug: &str) -> Result<Self, IdParseError> {
        check_slug(slug).map_err(|e| IdParseError::new("indicator id", e.problem))?;
        Ok(Self {
            problem,
            slug: Arc::from(slug),
        })
    }

    /// The problem the indicator belongs to.
    pub fn problem(&self) -> &ProblemId {
        &self.problem
    }

    /// The indicator's own slug, after the dot.
    pub fn slug(&self) -> &str {
        &self.slug
    }
}
impl fmt::Display for IndicatorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.problem, self.slug)
    }
}
impl FromStr for IndicatorId {
    type Err = IdParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (problem, slug) = s.split_once('.').ok_or(IdParseError::new(
            "indicator id",
            "expected <problem_id>.<slug>",
        ))?;
        let problem = problem
            .parse()
            .map_err(|e: IdParseError| IdParseError::new("indicator id", e.problem))?;
        Self::new(problem, slug)
    }
}
impl Serialize for IndicatorId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}
impl<'de> Deserialize<'de> for IndicatorId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        s.parse().map_err(de::Error::custom)
    }
}
string_schema!(
    IndicatorId,
    "IndicatorId",
    Some("^[a-z][a-z0-9_]*\\.[a-z][a-z0-9_]*$"),
    "<problem_id>.<indicator_slug>"
);

/// `<image_id>.<n>` ([contracts 5], region ids): numbered per image by the worker that segments
/// it, unique within the scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RegionId {
    /// The image the region is on.
    pub image: ImageId,
    /// The region's number on that image.
    pub n: u32,
}
impl fmt::Display for RegionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.image, self.n)
    }
}
impl FromStr for RegionId {
    type Err = IdParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (image, n) = s
            .split_once('.')
            .ok_or(IdParseError::new("region id", "expected <image_id>.<n>"))?;
        Ok(Self {
            image: image
                .parse()
                .map_err(|_| IdParseError::new("region id", "image part is not a ULID"))?,
            n: n.parse()
                .map_err(|_| IdParseError::new("region id", "number part is not an integer"))?,
        })
    }
}
impl Serialize for RegionId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}
impl<'de> Deserialize<'de> for RegionId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        s.parse().map_err(de::Error::custom)
    }
}
string_schema!(
    RegionId,
    "RegionId",
    Some("^[0-9A-HJKMNP-TV-Z]{26}\\.[0-9]+$"),
    "<image_id>.<n>"
);

/// `<source_id>.<n>`: a knowledge chunk ([contracts 10], `ChunkSeed.id`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkId {
    /// The source the chunk was cut from.
    pub source: SourceId,
    /// Its number within the source.
    pub n: u32,
}
impl fmt::Display for ChunkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.source, self.n)
    }
}
impl FromStr for ChunkId {
    type Err = IdParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (source, n) = s
            .rsplit_once('.')
            .ok_or(IdParseError::new("chunk id", "expected <source_id>.<n>"))?;
        Ok(Self {
            source: source
                .parse()
                .map_err(|e: IdParseError| IdParseError::new("chunk id", e.problem))?,
            n: n.parse()
                .map_err(|_| IdParseError::new("chunk id", "number part is not an integer"))?,
        })
    }
}
impl Serialize for ChunkId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}
impl<'de> Deserialize<'de> for ChunkId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        s.parse().map_err(de::Error::custom)
    }
}
string_schema!(
    ChunkId,
    "ChunkId",
    Some("^[a-z][a-z0-9_]*\\.[0-9]+$"),
    "<source_id>.<n>"
);

/// A farm record ([connectors 4]): the record key `farm_record:[connector, vendor_id, n]`, sent
/// as `"<connector_id>.<n>.<vendor_id>"`. The vendor id goes last because it may hold dots and
/// slashes (a Slack `"<channel>/<ts>"`, whose `ts` is `"1729000000.000100"`), so the first two
/// dots split the string and the rest is the vendor id.
///
/// [connectors 4]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#4-data
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FarmRecordId {
    /// The connector that synced it.
    pub connector: ConnectorId,
    /// Slack `"<channel>/<ts>"` or a Drive file id.
    pub vendor_id: Arc<str>,
    /// The chunk's number: 0 for a message; 0, 1, … for a document's chunks.
    pub n: u32,
}
impl FarmRecordId {
    /// Builds a farm record id, checking the vendor id (printable, 1–255 bytes).
    pub fn new(connector: ConnectorId, vendor_id: &str, n: u32) -> Result<Self, IdParseError> {
        printable(vendor_id, 255)
            .map_err(|problem| IdParseError::new("farm record id", problem))?;
        Ok(Self {
            connector,
            vendor_id: Arc::from(vendor_id),
            n,
        })
    }
}
impl fmt::Display for FarmRecordId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.connector, self.n, self.vendor_id)
    }
}
impl FromStr for FarmRecordId {
    type Err = IdParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        const SHAPE: &str = "expected <connector_id>.<n>.<vendor_id>";
        let (connector, rest) = s
            .split_once('.')
            .ok_or(IdParseError::new("farm record id", SHAPE))?;
        let (n, vendor_id) = rest
            .split_once('.')
            .ok_or(IdParseError::new("farm record id", SHAPE))?;
        let connector = connector
            .parse()
            .map_err(|_| IdParseError::new("farm record id", "connector part is not a ULID"))?;
        // Canonical digits only: a leading zero would give one record two spellings.
        if n.len() > 1 && n.starts_with('0') {
            return Err(IdParseError::new(
                "farm record id",
                "number part has a leading zero",
            ));
        }
        let n = n
            .parse()
            .map_err(|_| IdParseError::new("farm record id", "number part is not an integer"))?;
        Self::new(connector, vendor_id, n)
    }
}
impl Serialize for FarmRecordId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}
impl<'de> Deserialize<'de> for FarmRecordId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        s.parse().map_err(de::Error::custom)
    }
}
string_schema!(
    FarmRecordId,
    "FarmRecordId",
    Some("^[0-9A-HJKMNP-TV-Z]{26}\\.[0-9]+\\..+$"),
    "<connector_id>.<n>.<vendor_id>"
);

// ---------------------------------------------------------------------------------------------
// Opaque string ids
// ---------------------------------------------------------------------------------------------

/// Declares an opaque string newtype with a checking constructor, serde as a bare string and a
/// string schema.
///
/// `secret` before the doc comment gives the type a `Debug` that prints `"[redacted]"`.
macro_rules! opaque_string {
    (
        secret
        $(#[$doc:meta])*
        $name:ident, $kind:literal, $pattern:expr, $check:expr
    ) => {
        opaque_string!(@build () $(#[$doc])* $name, $kind, $pattern, $check);
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "([redacted])"))
            }
        }
    };
    (
        $(#[$doc:meta])*
        $name:ident, $kind:literal, $pattern:expr, $check:expr
    ) => {
        opaque_string!(@build (Debug,) $(#[$doc])* $name, $kind, $pattern, $check);
    };
    (
        @build ($($derive:ident,)*)
        $(#[$doc:meta])*
        $name:ident, $kind:literal, $pattern:expr, $check:expr
    ) => {
        $(#[$doc])*
        #[derive($($derive,)* Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Arc<str>);

        impl $name {
            /// The text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
        impl FromStr for $name {
            type Err = IdParseError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let check: fn(&str) -> Result<(), &'static str> = $check;
                check(s).map_err(|problem| IdParseError::new($kind, problem))?;
                Ok(Self(Arc::from(s)))
            }
        }
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let s = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
                s.parse().map_err(de::Error::custom)
            }
        }
        string_schema!($name, stringify!($name), $pattern, $kind);
    };
}

fn printable(s: &str, max: usize) -> Result<(), &'static str> {
    if s.is_empty() {
        Err("empty")
    } else if s.len() > max {
        Err("too long")
    } else if s.chars().any(char::is_control) {
        Err("contains control characters")
    } else {
        Ok(())
    }
}

opaque_string!(
    /// A worker's stable id, such as `"rig-3070-perception-1"` ([contracts 5], heartbeat).
    /// `[A-Za-z0-9._-]`, 1–64 characters.
    WorkerId,
    "worker id",
    Some("^[A-Za-z0-9._-]{1,64}$"),
    |s| {
        if s.is_empty() || s.len() > 64 {
            Err("expected 1-64 characters")
        } else if !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        {
            Err("only A-Z, a-z, 0-9, '.', '_' and '-' are allowed")
        } else {
            Ok(())
        }
    }
);

opaque_string!(
    /// The lease token returned with a claimed job and sent as `X-Lease-Token`
    /// ([contracts 5]). The claim generates it in `SurrealDB` with `rand::ulid()` ([database 4],
    /// job claim), so it is a ULID string; a requeue invalidates it.
    LeaseToken,
    "lease token",
    Some(ULID_PATTERN),
    |s| Ulid::from_string(s).map(|_| ()).map_err(|_| "not a ULID")
);

opaque_string!(
    /// The auth provider's user id (Clerk's `sub` claim and webhook `data.id`), stored as
    /// `user.auth_id` ([contracts 9], auth webhook). Never sent to the app.
    AuthId,
    "auth id",
    None,
    |s| printable(s, 255)
);

opaque_string!(
    /// An invite code: 26 random base32 characters, 128 bits ([contracts 9], invites).
    InviteCode,
    "invite code",
    Some("^[A-Z2-7]{26}$"),
    |s| {
        if s.len() == 26 && s.chars().all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c)) {
            Ok(())
        } else {
            Err("expected 26 base32 characters (A-Z, 2-7)")
        }
    }
);

opaque_string!(
    /// A registered push token, URL-encoded in the path of `DELETE /me/push-tokens/{token}`
    /// ([contracts 9], account). Any printable string of at most 255 bytes parses, so a stored
    /// token, a `web` one or one registered before the check always reads back and can be
    /// removed; `POST /me/push-tokens` takes only Expo push tokens for `ios` and `android`
    /// ([`PushToken::is_expo`], `PushTokenRequest::check`).
    PushToken,
    "push token",
    None,
    |s| printable(s, 255)
);

impl PushToken {
    /// Whether it is an Expo push token, the only kind Expo Push delivers to: `ExponentPushToken[…]`
    /// or `ExpoPushToken[…]`, as `expo-notifications`' `getExpoPushTokenAsync` returns them and
    /// Expo's server SDK checks them (`isExpoPushToken`), with a non-empty inside of printable
    /// ASCII without spaces or brackets. A raw APNs or FCM device token isn't one. The SDK's
    /// other form, a bare UUID from Expo's early days, is not accepted.
    pub fn is_expo(&self) -> bool {
        let inner = ["ExponentPushToken[", "ExpoPushToken["]
            .iter()
            .find_map(|prefix| self.as_str().strip_prefix(*prefix))
            .and_then(|rest| rest.strip_suffix(']'));
        inner.is_some_and(|i| {
            !i.is_empty()
                && i.bytes()
                    .all(|b| b.is_ascii_graphic() && b != b'[' && b != b']')
        })
    }
}

opaque_string!(
    /// A list cursor (`?cursor=`, `next_cursor`), opaque to the app ([contracts 1], lists).
    Cursor,
    "cursor",
    None,
    |s| printable(s, 512)
);

opaque_string!(
    /// A plan step's id, unique only within its plan ([contracts 7], `PlanStep.id`), such as
    /// `"s3"`.
    StepId,
    "step id",
    None,
    |s| printable(s, 64)
);

/// 43 base64url characters without padding: 32 bytes, or a SHA-256 digest.
fn base64url_43(s: &str) -> Result<(), &'static str> {
    if s.len() == 43
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        Ok(())
    } else {
        Err("expected 43 base64url characters without padding")
    }
}

opaque_string!(
    secret
    /// A connect's device binding ([connectors 5]): the app's 32 random bytes, base64url without
    /// padding. Never leaves the device that started the connect until `complete`.
    ///
    /// [connectors 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#5-connecting-and-tokens
    DeviceVerifier,
    "device verifier",
    Some("^[A-Za-z0-9_-]{43}$"),
    base64url_43
);

opaque_string!(
    /// `SHA-256(device_verifier)`, base64url without padding, as PKCE's `S256` encodes a
    /// challenge ([connectors 5]): sent when a connect starts and kept in the pending
    /// connector's `oauth` field.
    ///
    /// [connectors 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/connectors/README.md#5-connecting-and-tokens
    DeviceHash,
    "device hash",
    Some("^[A-Za-z0-9_-]{43}$"),
    base64url_43
);

/// The `Idempotency-Key` header: a UUID the app generates per user action ([contracts 1],
/// idempotency). Parsed with `uuid` (`v4`, `serde`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IdempotencyKey(pub uuid::Uuid);

impl FromStr for IdempotencyKey {
    type Err = IdParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        uuid::Uuid::parse_str(s)
            .map(Self)
            .map_err(|_| IdParseError::new("idempotency key", "not a UUID"))
    }
}
impl fmt::Display for IdempotencyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0.hyphenated(), f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_round_trips_as_bare_ulid() {
        let id: ScanId = "01JAX3K9T6R2B8V0QZ4M7D1E5F".parse().unwrap();
        assert_eq!(id.to_string(), "01JAX3K9T6R2B8V0QZ4M7D1E5F");
        assert_eq!(format!("{id:?}"), "scan:01JAX3K9T6R2B8V0QZ4M7D1E5F");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"01JAX3K9T6R2B8V0QZ4M7D1E5F\"");
        assert_eq!(serde_json::from_str::<ScanId>(&json).unwrap(), id);
        assert!("scan:01JAX3K9T6R2B8V0QZ4M7D1E5F".parse::<ScanId>().is_err());
        assert!("01JAX3K9T6".parse::<ScanId>().is_err());
    }

    #[test]
    fn id_is_send_sync_copy_whatever_the_marker() {
        fn assert_traits<T: Send + Sync + Copy + Unpin + 'static>() {}
        assert_traits::<ScanId>();
        assert_traits::<JobId>();
    }

    #[test]
    fn slugs_are_checked() {
        assert!("tomato_early_blight".parse::<ProblemId>().is_ok());
        assert!("Tomato".parse::<ProblemId>().is_err());
        assert!("1tomato".parse::<SpeciesId>().is_err());
        assert!("tomato-blight".parse::<SpeciesId>().is_err());
        assert!("healthy".parse::<ProblemId>().is_err());
        assert!("healthy".parse::<SpeciesId>().is_ok());
        assert_eq!(
            "healthy".parse::<ProblemOrHealthy>().unwrap(),
            ProblemOrHealthy::Healthy
        );
    }

    #[test]
    fn compound_ids_parse_their_parts() {
        let ind: IndicatorId = "tomato_early_blight.concentric_rings".parse().unwrap();
        assert_eq!(ind.problem().as_str(), "tomato_early_blight");
        assert_eq!(ind.slug(), "concentric_rings");
        assert!("tomato_early_blight".parse::<IndicatorId>().is_err());

        let region: RegionId = "01JAX3K9T6R2B8V0QZ4M7D1E5F.3".parse().unwrap();
        assert_eq!(region.n, 3);
        assert_eq!(region.to_string(), "01JAX3K9T6R2B8V0QZ4M7D1E5F.3");

        let chunk: ChunkId = "example_extension_tomato_diseases.12".parse().unwrap();
        assert_eq!(chunk.n, 12);
    }

    #[test]
    fn farm_record_ids_keep_the_dots_of_a_slack_ts() {
        let text = "01JAX3K9T6R2B8V0QZ4M7D1E5C.0.C024BE91L/1729000000.000100";
        let id: FarmRecordId = text.parse().unwrap();
        assert_eq!(id.connector.to_string(), "01JAX3K9T6R2B8V0QZ4M7D1E5C");
        assert_eq!(id.n, 0);
        assert_eq!(&*id.vendor_id, "C024BE91L/1729000000.000100");
        assert_eq!(serde_json::to_value(&id).unwrap(), serde_json::json!(text));
        assert_eq!(
            serde_json::from_value::<FarmRecordId>(serde_json::json!(text)).unwrap(),
            id
        );
        assert!(
            "01JAX3K9T6R2B8V0QZ4M7D1E5C.0"
                .parse::<FarmRecordId>()
                .is_err()
        );
        for spelled in [".01.", ".00.", ".007."] {
            let text = format!("01JAX3K9T6R2B8V0QZ4M7D1E5C{spelled}file");
            assert!(text.parse::<FarmRecordId>().is_err(), "{text}");
        }
        assert!(
            "01JAX3K9T6R2B8V0QZ4M7D1E5C.0.file"
                .parse::<FarmRecordId>()
                .is_ok()
        );
        assert!(
            "01JAX3K9T6R2B8V0QZ4M7D1E5C.x.file"
                .parse::<FarmRecordId>()
                .is_err()
        );
        assert!(
            "01JAX3K9T6R2B8V0QZ4M7D1E5C.1."
                .parse::<FarmRecordId>()
                .is_err()
        );
    }

    #[test]
    fn opaque_strings_check_their_format() {
        assert!("rig-3070-perception-1".parse::<WorkerId>().is_ok());
        assert!("rig 1".parse::<WorkerId>().is_err());
        assert!("ABCDEFGHIJKLMNOPQRSTUVWXYZ".parse::<InviteCode>().is_ok());
        assert!("ABCDEFGHIJKLMNOPQRSTUVWXY1".parse::<InviteCode>().is_err());
        assert!("01JAX3K9T6R2B8V0QZ4M7D1E5F".parse::<LeaseToken>().is_ok());
        let key: IdempotencyKey = "6f1c2c1e-5b8a-4c39-9f43-2d3b1f6e7a10".parse().unwrap();
        assert_eq!(key.to_string(), "6f1c2c1e-5b8a-4c39-9f43-2d3b1f6e7a10");
    }

    #[test]
    fn expo_push_tokens_are_recognised() {
        let expo = |s: &str| s.parse::<PushToken>().unwrap().is_expo();
        assert!(expo("ExponentPushToken[xxxxxxxxxxxxxxxxxxxxxx]"));
        assert!(expo("ExpoPushToken[AbC-dEf_123]"));
        for not_expo in [
            "ExponentPushToken[]",
            "ExponentPushToken[abc",
            "ExponentPushToken[a b]",
            "ExponentPushToken[a]]",
            "exponentpushtoken[abc]",
            " ExponentPushToken[abc]",
            // A raw APNs device token, and a raw FCM registration token.
            "740f4707bebcf74f9b7c25d48e3358945f6aa01da5ddb387462c7eaf61bb78ad",
            "dVhd7k3HQJ6h7Wq1:APA91bH2k_example",
            "6f1c2c1e-5b8a-4c39-9f43-2d3b1f6e7a10",
        ] {
            assert!(!expo(not_expo), "{not_expo}");
        }
    }
}
