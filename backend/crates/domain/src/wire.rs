//! Wire primitives shared by every contract ([contracts 1, conventions]): confidence and its
//! band, the VLM's certainty words, boxes, pages, dates, runs, event sequence numbers, the
//! `v: 1` literal and roles.

use std::{borrow::Cow, fmt};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use utoipa::{
    PartialSchema, ToSchema,
    openapi::{
        RefOr,
        schema::{ObjectBuilder, Schema, SchemaType, Type},
    },
};

use crate::{
    id::{Cursor, ImageId, RegionId, ScanId, UserId},
    url::{ImagePath, Sign, Signed, SignedMask, Signer, Stored, StoredMask, UrlState},
};

/// RFC 3339 in UTC with a `Z` suffix, whole seconds on the wire: `"2026-10-18T14:03:22Z"`
/// ([contracts 1]). chrono's own `Serialize` adds a fraction whenever the value has one, and a
/// clock reading always has one, so the newtype truncates when serialising and keeps full
/// precision in memory (the store binds the inner `DateTime<Utc>`, [database 2]). There is no
/// `Timestamp::now()`: every reading goes through the `Clock` port so tests control time.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, ToSchema, schemars::JsonSchema,
)]
#[schema(value_type = String, format = DateTime, example = "2026-10-18T14:03:22Z")]
#[schemars(transparent)]
pub struct Timestamp(chrono::DateTime<chrono::Utc>);

impl Timestamp {
    /// The instant inside, for the store and for arithmetic.
    pub const fn into_inner(self) -> chrono::DateTime<chrono::Utc> {
        self.0
    }

    /// Unix seconds.
    pub fn unix_seconds(self) -> i64 {
        self.0.timestamp()
    }

    /// The instant `d` later, saturating at chrono's range.
    #[must_use]
    pub fn plus(self, d: std::time::Duration) -> Self {
        chrono::Duration::from_std(d)
            .ok()
            .and_then(|d| self.0.checked_add_signed(d))
            .map_or(self, Self)
    }

    /// The instant `d` earlier, saturating at chrono's range.
    #[must_use]
    pub fn minus(self, d: std::time::Duration) -> Self {
        chrono::Duration::from_std(d)
            .ok()
            .and_then(|d| self.0.checked_sub_signed(d))
            .map_or(self, Self)
    }

    /// How long after `earlier` this is; zero when it isn't later.
    pub fn since(self, earlier: Self) -> std::time::Duration {
        (self.0 - earlier.0).to_std().unwrap_or_default()
    }

    /// The UTC calendar day, for `usage_daily:[user, day]` ([database 4], limits).
    pub fn utc_day(self) -> chrono::NaiveDate {
        self.0.date_naive()
    }
}

impl From<chrono::DateTime<chrono::Utc>> for Timestamp {
    fn from(t: chrono::DateTime<chrono::Utc>) -> Self {
        Self(t)
    }
}
impl std::str::FromStr for Timestamp {
    type Err = chrono::ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        chrono::DateTime::parse_from_rfc3339(s).map(|t| Self(t.with_timezone(&chrono::Utc)))
    }
}
impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
    }
}
impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}
impl<'de> Deserialize<'de> for Timestamp {
    /// Accepts any RFC 3339 offset and converts it to UTC.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        chrono::DateTime::<chrono::Utc>::deserialize(deserializer).map(Self)
    }
}

/// A calendar date in the garden's time zone, `YYYY-MM-DD` ([contracts 1]). A newtype over
/// `chrono::NaiveDate` (whose serde form is already `"2026-10-20"`) so the `OpenAPI` and JSON
/// Schema generators see `format: date` through any alias, and so a UTC day can't be passed
/// where a garden-local one is meant without saying so.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    ToSchema,
    schemars::JsonSchema,
)]
#[serde(transparent)]
#[schema(value_type = String, format = Date, example = "2026-10-20")]
#[schemars(transparent)]
pub struct LocalDate(chrono::NaiveDate);

impl LocalDate {
    /// Wraps a date.
    pub const fn new(date: chrono::NaiveDate) -> Self {
        Self(date)
    }

    /// The date inside.
    pub const fn into_inner(self) -> chrono::NaiveDate {
        self.0
    }

    /// `days` later, or `None` past chrono's range.
    pub fn plus_days(self, days: u32) -> Option<Self> {
        self.0
            .checked_add_days(chrono::Days::new(u64::from(days)))
            .map(Self)
    }
}

impl From<chrono::NaiveDate> for LocalDate {
    fn from(d: chrono::NaiveDate) -> Self {
        Self(d)
    }
}
impl std::str::FromStr for LocalDate {
    type Err = chrono::ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").map(Self)
    }
}
impl fmt::Display for LocalDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.format("%Y-%m-%d"))
    }
}

// ---------------------------------------------------------------------------------------------
// Confidence
// ---------------------------------------------------------------------------------------------

/// A number from 0 to 1 ([contracts 1]). Finite and in range by construction: deserialising
/// anything else fails.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Confidence(f64);

/// Why a number isn't a confidence.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
#[error("confidence must be a finite number from 0 to 1, got {0}")]
pub struct ConfidenceRangeError(pub f64);

impl Confidence {
    /// 0.
    pub const ZERO: Self = Self(0.0);
    /// The lower bound of the `medium` band.
    pub const MEDIUM: Self = Self(0.5);
    /// The lower bound of the `high` band.
    pub const HIGH: Self = Self(0.8);

    /// Checks the range.
    pub fn new(value: f64) -> Result<Self, ConfidenceRangeError> {
        if value.is_finite() && (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(ConfidenceRangeError(value))
        }
    }

    /// Clamps any finite number into 0..=1; `NaN` becomes 0.
    pub fn saturating(value: f64) -> Self {
        if value.is_nan() {
            Self::ZERO
        } else {
            Self(value.clamp(0.0, 1.0))
        }
    }

    /// The number.
    pub const fn get(self) -> f64 {
        self.0
    }

    /// The band this confidence falls in.
    pub fn band(self) -> ConfidenceBand {
        ConfidenceBand::of(self)
    }

    /// Medium confidence or more, the threshold most rules use (contracts 3, readable plant;
    /// contracts 7, healthy and `low_evidence`).
    pub fn at_least_medium(self) -> bool {
        self >= Self::MEDIUM
    }
}

impl<'de> Deserialize<'de> for Confidence {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(f64::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}
impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.0)
    }
}
impl PartialSchema for Confidence {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::Number)
            .minimum(Some(0.0))
            .maximum(Some(1.0))
            .description(Some("confidence from 0 to 1"))
            .into()
    }
}
impl ToSchema for Confidence {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("Confidence")
    }
}

/// `"high" | "medium" | "low"`, computed by the API so every screen agrees ([contracts 1]):
/// **high** ≥ 0.8, **medium** ≥ 0.5, **low** below that.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceBand {
    /// Below 0.5.
    Low,
    /// 0.5 up to 0.8.
    Medium,
    /// 0.8 or more.
    High,
}

impl ConfidenceBand {
    /// The band rule of contracts 1.
    pub fn of(confidence: Confidence) -> Self {
        if confidence >= Confidence::HIGH {
            Self::High
        } else if confidence >= Confidence::MEDIUM {
            Self::Medium
        } else {
            Self::Low
        }
    }
}

/// `confidence` and `confidence_band` travelling together, flattened into the record that
/// carries them (`Observation`). The band is derived from the number when serialising and
/// recomputed when deserialising, so it can never be set by hand or disagree with the number.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Rated(pub Confidence);

impl Rated {
    /// The confidence.
    pub const fn confidence(self) -> Confidence {
        self.0
    }

    /// Its band.
    pub fn band(self) -> ConfidenceBand {
        self.0.band()
    }
}

impl From<Confidence> for Rated {
    fn from(c: Confidence) -> Self {
        Self(c)
    }
}

#[derive(Serialize, Deserialize)]
struct RatedRepr {
    confidence: Confidence,
    #[serde(default, skip_deserializing)]
    confidence_band: Option<ConfidenceBand>,
}

impl Serialize for Rated {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        RatedRepr {
            confidence: self.0,
            confidence_band: Some(self.band()),
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Rated {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // The incoming band is ignored: the rule of contracts 1 decides it.
        RatedRepr::deserialize(deserializer).map(|r| Self(r.confidence))
    }
}
impl PartialSchema for Rated {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .property("confidence", Confidence::schema())
            .required("confidence")
            .property("confidence_band", ConfidenceBand::schema())
            .required("confidence_band")
            .into()
    }
}
impl ToSchema for Rated {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("Rated")
    }
}

/// The VLM's certainty as a word ([contracts 6]): a quantized 7–8B model's own probabilities are
/// poorly calibrated, so the worker maps the word to a fixed confidence (open question 2 of
/// contracts 12). Measurements use the same values (contracts 7, measurement confidence).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Certainty {
    /// 0.4.
    Low,
    /// 0.65.
    Medium,
    /// 0.85.
    High,
}

impl Certainty {
    /// The fixed mapping: low 0.4, medium 0.65, high 0.85.
    pub fn confidence(self) -> Confidence {
        Confidence(match self {
            Self::Low => 0.4,
            Self::Medium => 0.65,
            Self::High => 0.85,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Geometry and lists
// ---------------------------------------------------------------------------------------------

/// `[x, y, w, h]`, each from 0 to 1, origin top-left, normalised to the stored image
/// ([contracts 1], regions). Whoever produces a box converts it before sending.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(into = "[f64; 4]")]
pub struct BBox {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Why four numbers aren't a normalised box.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
#[error("bbox values must be finite and from 0 to 1, with x + w and y + h at most 1: {0:?}")]
pub struct BBoxError(pub [f64; 4]);

impl BBox {
    /// Tolerance for `x + w ≤ 1` after a float conversion.
    const EDGE_EPSILON: f64 = 1e-6;

    /// Checks every value and that the box stays inside the image.
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Result<Self, BBoxError> {
        let all = [x, y, w, h];
        let in_unit = all.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v));
        if in_unit && x + w <= 1.0 + Self::EDGE_EPSILON && y + h <= 1.0 + Self::EDGE_EPSILON {
            Ok(Self { x, y, w, h })
        } else {
            Err(BBoxError(all))
        }
    }

    /// The whole image.
    pub const FULL: Self = Self {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };

    /// `[x, y, w, h]`.
    pub const fn to_array(self) -> [f64; 4] {
        [self.x, self.y, self.w, self.h]
    }

    /// The share of the image the box covers.
    pub fn area(self) -> f64 {
        self.w * self.h
    }
}

impl From<BBox> for [f64; 4] {
    fn from(b: BBox) -> Self {
        b.to_array()
    }
}
impl TryFrom<[f64; 4]> for BBox {
    type Error = BBoxError;
    fn try_from([x, y, w, h]: [f64; 4]) -> Result<Self, Self::Error> {
        Self::new(x, y, w, h)
    }
}
impl<'de> Deserialize<'de> for BBox {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = <[f64; 4]>::deserialize(deserializer)?;
        Self::try_from(raw).map_err(de::Error::custom)
    }
}
impl PartialSchema for BBox {
    fn schema() -> RefOr<Schema> {
        utoipa::openapi::schema::ArrayBuilder::new()
            .items(Confidence::schema())
            .min_items(Some(4))
            .max_items(Some(4))
            .description(Some("[x, y, w, h], normalised to the stored image"))
            .into()
    }
}
impl ToSchema for BBox {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("BBox")
    }
}

/// `{ items: T[], next_cursor?: string }` ([contracts 1], lists). Default page size 20, at most
/// 100 ([`PageRequest`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Page<T> {
    /// The items, in the order the endpoint states.
    pub items: Vec<T>,
    /// The cursor of the next page; absent on the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub next_cursor: Option<Cursor>,
}

impl<T> Page<T> {
    /// A last page.
    pub fn last(items: Vec<T>) -> Self {
        Self {
            items,
            next_cursor: None,
        }
    }

    /// Converts every item, keeping the cursor.
    pub fn map<U>(self, f: impl FnMut(T) -> U) -> Page<U> {
        Page {
            items: self.items.into_iter().map(f).collect(),
            next_cursor: self.next_cursor,
        }
    }
}

/// `?cursor=&limit=` ([contracts 1], lists).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageRequest {
    /// From a previous page's `next_cursor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<Cursor>,
    /// 1–100; 20 when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

impl PageRequest {
    /// The default page size.
    pub const DEFAULT_LIMIT: u32 = 20;
    /// The largest page size.
    pub const MAX_LIMIT: u32 = 100;

    /// The limit to use: the default when absent, clamped to 1–100.
    pub fn limit(&self) -> u32 {
        self.limit
            .unwrap_or(Self::DEFAULT_LIMIT)
            .clamp(1, Self::MAX_LIMIT)
    }
}

/// A box on an image ([contracts 1], `Region`). `region_id` is present for segmentation regions
/// (`"<image_id>.<n>"`) and referenced by later stages.
//
// The mask slot is `mask_id` when stored and `mask_url` (signed) on the wire: see `crate::url`.
// (A plain comment, so it stays out of the OpenAPI description.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(bound = "")]
#[schema(bound = "U: crate::url::WireState", as = Region)]
pub struct Region<U: UrlState = Signed> {
    /// Present for segmentation regions, unique within the scan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub region_id: Option<RegionId>,
    /// The image the box is on.
    pub image_id: ImageId,
    /// The box.
    pub bbox: BBox,
    // `mask_id` (stored) or `mask_url` (signed), when segmentation produced a mask.
    #[serde(flatten)]
    #[schema(inline, value_type = SignedMask)]
    pub mask: U::Mask,
}

impl Region<Stored> {
    /// A region without a mask, as every region from the VLM is.
    pub fn unmasked(image_id: ImageId, bbox: BBox, region_id: Option<RegionId>) -> Self {
        Self {
            region_id,
            image_id,
            bbox,
            mask: StoredMask::default(),
        }
    }
}

impl Sign for Region<Stored> {
    type Signed = Region<Signed>;
    type Scope = ScanId;
    fn sign(self, scan: ScanId, signer: &Signer<'_>) -> Region<Signed> {
        Region {
            region_id: self.region_id,
            image_id: self.image_id,
            bbox: self.bbox,
            mask: SignedMask {
                mask_url: self
                    .mask
                    .mask_id
                    .map(|mask| signer.url(&ImagePath::Mask { scan, mask })),
            },
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Runs, sequence numbers, literals
// ---------------------------------------------------------------------------------------------

/// A scan's run: 0 until submitted, 1 on submit, +1 on a resubmit or a species change
/// ([contracts 3], lifecycle). Jobs, events, observations and diagnoses carry it.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Default,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[serde(transparent)]
pub struct Run(pub u32);

impl Run {
    /// Not yet submitted.
    pub const NONE: Self = Self(0);

    /// The run a new pass starts.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}
impl fmt::Display for Run {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// A scan event's `seq` and SSE `id`: per scan, from 1, strictly increasing, without gaps,
/// across all runs ([contracts 4]).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Default,
    Serialize,
    Deserialize,
    ToSchema,
)]
#[serde(transparent)]
pub struct EventSeq(pub u64);

impl EventSeq {
    /// The value before the first event (`scan.last_event_seq` of a new scan).
    pub const ZERO: Self = Self(0);

    /// The following sequence number.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}
impl fmt::Display for EventSeq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// The literal `v: 1` on every SSE event, worker payload and result, chat event and push
/// payload ([contracts 1], versioning). Serialises as `1`; deserialising any other number fails
/// with [`UnsupportedVersion`], which a worker turns into the job error `unsupported_version`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct V1;

/// A payload's `v` isn't one this build understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("unsupported_version: v = {0}")]
pub struct UnsupportedVersion(pub u64);

impl Serialize for V1 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(1)
    }
}
impl<'de> Deserialize<'de> for V1 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u64::deserialize(deserializer)? {
            1 => Ok(Self),
            other => Err(de::Error::custom(UnsupportedVersion(other))),
        }
    }
}
impl PartialSchema for V1 {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::Integer)
            .enum_values(Some([1]))
            .into()
    }
}
impl ToSchema for V1 {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("V1")
    }
}

/// A boolean literal type, for `passed: true` ([contracts 3], upload response) and the
/// `stored: true | false` union tag of `UploadImageResponse.depth`. Deserialising the other
/// value fails, which is what lets an untagged enum pick the right branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BoolLit<const B: bool>;

/// `true` as a type.
pub type True = BoolLit<true>;
/// `false` as a type.
pub type False = BoolLit<false>;

impl<const B: bool> Serialize for BoolLit<B> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bool(B)
    }
}
impl<'de, const B: bool> Deserialize<'de> for BoolLit<B> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if bool::deserialize(deserializer)? == B {
            Ok(Self)
        } else {
            Err(de::Error::custom(format_args!("expected {B}")))
        }
    }
}
impl<const B: bool> PartialSchema for BoolLit<B> {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(SchemaType::new(Type::Boolean))
            .enum_values(Some([B]))
            .into()
    }
}
impl<const B: bool> ToSchema for BoolLit<B> {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed(if B { "True" } else { "False" })
    }
}

// ---------------------------------------------------------------------------------------------
// Roles and users
// ---------------------------------------------------------------------------------------------

/// A garden member's role ([contracts 1], roles and access), ordered
/// `viewer < reviewer < editor < owner` so "editor or above" is `role >= Role::Editor`. The
/// wire order in contracts (`"owner" | "editor" | "viewer" | "reviewer"`) is not the rank.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Reads and chats.
    Viewer,
    /// A viewer who can also give scan feedback.
    Reviewer,
    /// Scans, answers, manages tasks and plants, maps the garden.
    Editor,
    /// Manages the garden and its members. A garden may have several.
    Owner,
}

impl Role {
    /// Every role, lowest first.
    pub const ALL: [Self; 4] = [Self::Viewer, Self::Reviewer, Self::Editor, Self::Owner];

    /// The wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Reviewer => "reviewer",
            Self::Editor => "editor",
            Self::Owner => "owner",
        }
    }
}
impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `{ user_id: Id; display_name: string }`: `Scan.created_by` and `GardenMap.created_by`,
/// absent once that user is deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UserRef {
    /// The user.
    pub user_id: UserId,
    /// Their display name.
    pub display_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_rule() {
        let band = |v| Confidence::new(v).unwrap().band();
        assert_eq!(band(0.8), ConfidenceBand::High);
        assert_eq!(band(0.79), ConfidenceBand::Medium);
        assert_eq!(band(0.5), ConfidenceBand::Medium);
        assert_eq!(band(0.49), ConfidenceBand::Low);
        assert!(Confidence::new(1.01).is_err());
        assert!(Confidence::new(f64::NAN).is_err());
    }

    #[test]
    fn rated_writes_the_band_and_recomputes_it() {
        let r = Rated(Confidence::new(0.71).unwrap());
        assert_eq!(
            serde_json::to_value(r).unwrap(),
            serde_json::json!({"confidence": 0.71, "confidence_band": "medium"})
        );
        let back: Rated =
            serde_json::from_str(r#"{"confidence":0.9,"confidence_band":"low"}"#).unwrap();
        assert_eq!(back.band(), ConfidenceBand::High);
    }

    #[test]
    fn certainty_mapping() {
        assert!((Certainty::Low.confidence().get() - 0.4).abs() < f64::EPSILON);
        assert!((Certainty::Medium.confidence().get() - 0.65).abs() < f64::EPSILON);
        assert!((Certainty::High.confidence().get() - 0.85).abs() < f64::EPSILON);
        assert_eq!(Certainty::High.confidence().band(), ConfidenceBand::High);
    }

    #[test]
    fn bbox_is_checked() {
        assert!(BBox::new(0.1, 0.2, 0.3, 0.4).is_ok());
        assert!(BBox::new(0.9, 0.0, 0.2, 0.1).is_err());
        assert!(serde_json::from_str::<BBox>("[0.1, 0.2, 1.5, 0.1]").is_err());
        let b: BBox = serde_json::from_str("[0.1,0.2,0.3,0.4]").unwrap();
        assert_eq!(serde_json::to_string(&b).unwrap(), "[0.1,0.2,0.3,0.4]");
    }

    #[test]
    fn literals() {
        assert_eq!(serde_json::to_string(&V1).unwrap(), "1");
        let err = serde_json::from_str::<V1>("2").unwrap_err();
        assert!(err.to_string().contains("unsupported_version"));
        assert!(serde_json::from_str::<True>("false").is_err());
        assert!(serde_json::from_str::<False>("false").is_ok());
    }

    #[test]
    fn timestamps_use_z_and_whole_seconds() {
        let ts: Timestamp = "2026-10-18T14:03:22.381Z".parse().unwrap();
        assert_eq!(
            serde_json::to_string(&ts).unwrap(),
            "\"2026-10-18T14:03:22Z\""
        );
        let offset: Timestamp = serde_json::from_str("\"2026-10-18T16:03:22+02:00\"").unwrap();
        assert_eq!(offset.to_string(), "2026-10-18T14:03:22Z");
    }

    #[test]
    fn roles_rank() {
        assert!(Role::Owner > Role::Editor);
        assert!(Role::Editor > Role::Reviewer);
        assert!(Role::Reviewer > Role::Viewer);
    }
}
