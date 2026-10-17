//! The read tools ([contracts 8], tool return shapes and scope). Each returns the records the app
//! sees, scoped to the conversation's garden through the session's proof.

use std::io::Cursor;

use domain::{
    chat::{GardenMapView, Memory, MemorySearch, SoilProfileView, ToolName},
    garden::{map::neighbours, plants::GrowthQuantity, tasks::TaskQuery},
    id::{ChatImageId, ImageId, OrganismId, PlantId, ProblemId, ScanId, SourceId, SpeciesId},
    knowledge::ChunkTopic,
    ports::{ChunkFilter, ContentPart, ObjectKey},
    records::task::TaskState,
    wire::PageRequest,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{Tool, ToolCtx, ToolEffect, ToolKind, ToolOutput};
use crate::{beliefs::search_beliefs, context::garden_weather, error::ToolError};

const fn page(limit: u32) -> PageRequest {
    PageRequest {
        cursor: None,
        limit: Some(limit),
    }
}

/// Embeds one query with the pinned text model.
async fn embed(ctx: &ToolCtx<'_>, query: &str) -> Result<domain::ports::Embedding, ToolError> {
    let mut vectors = ctx
        .deps
        .embedder
        .embed(domain::ports::EmbedMode::Query, &[query.to_owned()])
        .await?;
    vectors
        .pop()
        .ok_or_else(|| ToolError::Rejected("The search is unavailable right now.".to_owned()))
}

/// Citation effects for knowledge-base sources.
async fn citations(ctx: &ToolCtx<'_>, sources: &[SourceId]) -> Result<Vec<ToolEffect>, ToolError> {
    Ok(ctx
        .deps
        .knowledge
        .citations(sources)
        .await?
        .into_iter()
        .map(ToolEffect::Citation)
        .collect())
}

// ---------------------------------------------------------------------------------------------

/// `get_scan(scan_id)`.
#[derive(Debug)]
pub struct GetScan;

/// Arguments of `get_scan`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetScanArgs {
    /// A scan of this garden.
    pub scan_id: ScanId,
}

impl Tool for GetScan {
    type Args = GetScanArgs;
    const NAME: ToolName = ToolName::GetScan;
    const DESCRIPTION: &'static str =
        "A scan's findings, confidence, image regions, diagnosis and plan.";
    const LABEL: &'static str = "Looking at the scan";
    const KIND: ToolKind = ToolKind::Read;

    async fn run(&self, ctx: &ToolCtx<'_>, args: GetScanArgs) -> Result<ToolOutput, ToolError> {
        let detail = ctx
            .deps
            .stores
            .scans
            .scan_detail(ctx.owner.membership(), args.scan_id)
            .await?;
        // `ScanDetail<Stored>` carries image ids and no URLs ([contracts 8]: "without image URLs").
        Ok(ToolOutput::json(&detail))
    }
}

// ---------------------------------------------------------------------------------------------

/// `view_image(image_id, region?)`.
#[derive(Debug)]
pub struct ViewImage;

/// A crop of the image, as fractions of its width and height.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegionArg {
    /// Left edge, 0 to 1.
    pub x: f64,
    /// Top edge, 0 to 1.
    pub y: f64,
    /// Width, 0 to 1.
    pub w: f64,
    /// Height, 0 to 1.
    pub h: f64,
}

/// Arguments of `view_image`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViewImageArgs {
    /// A scan image's or this conversation's image's id.
    pub image_id: String,
    /// The scan the image belongs to; omit for an image the user sent in this conversation.
    #[serde(default)]
    pub scan_id: Option<ScanId>,
    /// A crop, for example a region from the scan's findings.
    #[serde(default)]
    pub region: Option<RegionArg>,
}

impl Tool for ViewImage {
    type Args = ViewImageArgs;
    const NAME: ToolName = ToolName::ViewImage;
    const DESCRIPTION: &'static str = "Look at one of this garden's scan photos, or a photo the \
        user sent in this conversation, whole or cropped to a region.";
    const LABEL: &'static str = "Looking at the photo";
    const KIND: ToolKind = ToolKind::Read;

    async fn run(&self, ctx: &ToolCtx<'_>, args: ViewImageArgs) -> Result<ToolOutput, ToolError> {
        let member = ctx.owner.membership();
        let not_found =
            || ToolError::Rejected("No such image in this garden or conversation.".to_owned());
        let key = if let Some(scan_id) = args.scan_id {
            // A scan image: the scan must be the garden's and hold the image.
            let image: ImageId = args.image_id.parse().map_err(|_| not_found())?;
            let scan = ctx.deps.stores.scans.scan(member, scan_id).await?;
            if !scan.images.iter().any(|i| i.id == image) {
                return Err(not_found());
            }
            ObjectKey::scan_image(member.garden(), scan_id, image)
        } else {
            // A chat image: only this conversation's.
            let image: ChatImageId = args.image_id.parse().map_err(|_| not_found())?;
            ctx.deps
                .stores
                .chats
                .chat_image_object(ctx.owner.conversation(), image)
                .await?
                .ok_or_else(not_found)?
                .key
        };
        let bytes = ctx.deps.blob.get_bytes(&key).await?;
        let part = match args.region {
            None => ContentPart::Image {
                media_type: "image/webp",
                data: bytes,
            },
            Some(r) => {
                // Decoding and re-encoding is CPU work: off the async threads.
                let png = tokio::task::spawn_blocking(move || crop_png(&bytes, r))
                    .await
                    .map_err(|_| ToolError::Rejected("The crop failed.".to_owned()))??;
                ContentPart::Image {
                    media_type: "image/png",
                    data: png.into(),
                }
            }
        };
        Ok(ToolOutput {
            content: serde_json::json!({ "ok": true }),
            images: vec![part],
            effects: Vec::new(),
        })
    }
}

/// Crops a stored WebP image to a region and encodes the crop as PNG.
pub fn crop_png(webp: &[u8], region: RegionArg) -> Result<Vec<u8>, ToolError> {
    let bad = |what: &str| ToolError::Rejected(format!("The crop failed: {what}."));
    let RegionArg { x, y, w, h } = region;
    let inside = [x, y, w, h]
        .iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        && w > 0.0
        && h > 0.0
        && x + w <= 1.0 + 1e-6
        && y + h <= 1.0 + 1e-6;
    if !inside {
        return Err(bad("the region must lie inside the image"));
    }
    let image = image::load_from_memory(webp).map_err(|_| bad("the image could not be read"))?;
    let (width, height) = (image.width(), image.height());
    let to_px = |fraction: f64, size: u32| -> u32 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "fraction is in 0..=1, so the product is in 0..=size"
        )]
        let px = (fraction * f64::from(size)).round() as u32;
        px
    };
    let (left, top) = (to_px(x, width), to_px(y, height));
    let crop_width = to_px(w, width).clamp(1, width.saturating_sub(left).max(1));
    let crop_height = to_px(h, height).clamp(1, height.saturating_sub(top).max(1));
    let crop = image.crop_imm(left, top, crop_width, crop_height);
    let mut out = Cursor::new(Vec::new());
    crop.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|_| bad("the crop could not be encoded"))?;
    Ok(out.into_inner())
}

// ---------------------------------------------------------------------------------------------

/// `get_forecast(days)`; the garden is the conversation's.
#[derive(Debug)]
pub struct GetForecast;

/// Arguments of `get_forecast`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetForecastArgs {
    /// Days ahead, 1 to 14.
    pub days: u32,
}

impl Tool for GetForecast {
    type Args = GetForecastArgs;
    const NAME: ToolName = ToolName::GetForecast;
    const DESCRIPTION: &'static str = "The garden's weather forecast, one entry per day.";
    const LABEL: &'static str = "Checking the forecast";
    const KIND: ToolKind = ToolKind::Read;

    async fn run(&self, ctx: &ToolCtx<'_>, args: GetForecastArgs) -> Result<ToolOutput, ToolError> {
        let weather = garden_weather(ctx.deps, ctx.owner.membership().garden())
            .await
            .ok_or_else(|| {
                ToolError::Rejected("The forecast is unavailable right now.".to_owned())
            })?;
        let days = usize::try_from(args.days.clamp(1, 14)).unwrap_or(14);
        let forecast: Vec<_> = weather.forecast.into_iter().take(days).collect();
        Ok(ToolOutput::json(&forecast))
    }
}

// ---------------------------------------------------------------------------------------------

/// `search_memory(query, plant_id?)`.
#[derive(Debug)]
pub struct SearchMemory;

/// Arguments of `search_memory`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchMemoryArgs {
    /// What to look for.
    pub query: String,
    /// Limit to one plant (garden-wide facts are always included).
    #[serde(default)]
    pub plant_id: Option<PlantId>,
}

impl Tool for SearchMemory {
    type Args = SearchMemoryArgs;
    const NAME: ToolName = ToolName::SearchMemory;
    const DESCRIPTION: &'static str = "Search what is remembered about this garden's plants: \
        shared facts and events, and your own private notes with this user (memories), plus, \
        when available, the matching current shared beliefs (beliefs). A belief is the latest \
        value of its key and wins over an older memory that disagrees.";
    const LABEL: &'static str = "Checking what I remember";
    const KIND: ToolKind = ToolKind::Read;

    async fn run(
        &self,
        ctx: &ToolCtx<'_>,
        args: SearchMemoryArgs,
    ) -> Result<ToolOutput, ToolError> {
        let deps = ctx.deps;
        let member = ctx.owner.membership();
        let local = async {
            let query = embed(ctx, &args.query).await?;
            // The store applies `shared = true OR author = $user` within the proof's garden.
            let hits = deps
                .memory
                .search(member, &query, args.plant_id, deps.config.memory_hits)
                .await?;
            Ok::<_, ToolError>(hits.into_iter().map(|h| h.memory).collect::<Vec<Memory>>())
        };
        // Agent Memory's search takes seconds; past its timeout the local hits answer alone. It
        // is scoped by the proof's garden path, so a plant of another garden finds nothing.
        let beliefs = search_beliefs(
            deps.beliefs.as_ref(),
            member.garden(),
            args.plant_id,
            &args.query,
            deps.config.belief_hits,
            deps.config.beliefs_search_timeout,
        );
        let (memories, beliefs) = tokio::join!(local, beliefs);
        Ok(ToolOutput::json(&MemorySearch {
            memories: memories?,
            beliefs,
        }))
    }
}

// ---------------------------------------------------------------------------------------------

/// `find_similar_cases(image_id)`.
#[derive(Debug)]
pub struct FindSimilarCases;

/// Arguments of `find_similar_cases`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FindSimilarCasesArgs {
    /// A scan image of this garden.
    pub image_id: ImageId,
}

impl Tool for FindSimilarCases {
    type Args = FindSimilarCasesArgs;
    const NAME: ToolName = ToolName::FindSimilarCases;
    const DESCRIPTION: &'static str =
        "Past scans in this garden that look like this photo, most similar first.";
    const LABEL: &'static str = "Looking for similar cases";
    const KIND: ToolKind = ToolKind::Read;

    async fn run(
        &self,
        ctx: &ToolCtx<'_>,
        args: FindSimilarCasesArgs,
    ) -> Result<ToolOutput, ToolError> {
        let cases = ctx
            .deps
            .stores
            .analysis
            .similar_cases(ctx.owner.membership(), args.image_id, 5)
            .await?;
        Ok(ToolOutput::json(&cases))
    }
}

// ---------------------------------------------------------------------------------------------

/// `search_knowledge(query)`.
#[derive(Debug)]
pub struct SearchKnowledge;

/// Arguments of `search_knowledge`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchKnowledgeArgs {
    /// What to look for.
    pub query: String,
    /// Narrow to one kind of passage.
    #[serde(default)]
    pub topic: Option<ChunkTopic>,
}

impl Tool for SearchKnowledge {
    type Args = SearchKnowledgeArgs;
    const NAME: ToolName = ToolName::SearchKnowledge;
    const DESCRIPTION: &'static str =
        "Search the curated gardening guides. Every passage comes with its source to cite.";
    const LABEL: &'static str = "Checking the guides";
    const KIND: ToolKind = ToolKind::Read;

    async fn run(
        &self,
        ctx: &ToolCtx<'_>,
        args: SearchKnowledgeArgs,
    ) -> Result<ToolOutput, ToolError> {
        let query = embed(ctx, &args.query).await?;
        let filter = ChunkFilter {
            topic: args.topic,
            ..ChunkFilter::default()
        };
        let hits = ctx
            .deps
            .knowledge
            .search(&query, filter, ctx.deps.config.knowledge_hits)
            .await?;
        let mut out = ToolOutput::json(&hits);
        out.effects = hits
            .into_iter()
            .map(|h| ToolEffect::Citation(h.citation))
            .collect();
        Ok(out)
    }
}

// ---------------------------------------------------------------------------------------------

/// `get_species_profile(species)`.
#[derive(Debug)]
pub struct GetSpeciesProfile;

/// Arguments of `get_species_profile`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetSpeciesProfileArgs {
    /// A species id from the knowledge base.
    pub species: SpeciesId,
}

impl Tool for GetSpeciesProfile {
    type Args = GetSpeciesProfileArgs;
    const NAME: ToolName = ToolName::GetSpeciesProfile;
    const DESCRIPTION: &'static str =
        "A species' care needs and the problems it is susceptible to.";
    const LABEL: &'static str = "Reading the species profile";
    const KIND: ToolKind = ToolKind::Read;

    async fn run(
        &self,
        ctx: &ToolCtx<'_>,
        args: GetSpeciesProfileArgs,
    ) -> Result<ToolOutput, ToolError> {
        let k = &ctx.deps.knowledge;
        let profile = k
            .species(&args.species)
            .await?
            .ok_or_else(|| ToolError::Rejected(format!("Unknown species {}.", args.species)))?;
        let susceptible: Vec<ProblemId> = k
            .problems_of_species(&args.species)
            .await?
            .into_iter()
            .map(|p| p.id)
            .collect();
        let mut out = ToolOutput::json(&serde_json::json!({
            "profile": profile,
            "susceptible_to": susceptible,
        }));
        out.effects = citations(ctx, &profile.sources).await?;
        Ok(out)
    }
}

/// `get_disease_profile(disease)`.
#[derive(Debug)]
pub struct GetDiseaseProfile;

/// Arguments of `get_disease_profile`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetDiseaseProfileArgs {
    /// A problem id from the knowledge base.
    pub disease: ProblemId,
}

impl Tool for GetDiseaseProfile {
    type Args = GetDiseaseProfileArgs;
    const NAME: ToolName = ToolName::GetDiseaseProfile;
    const DESCRIPTION: &'static str =
        "A problem's symptoms, favourable conditions, management and sources.";
    const LABEL: &'static str = "Reading about the problem";
    const KIND: ToolKind = ToolKind::Read;

    async fn run(
        &self,
        ctx: &ToolCtx<'_>,
        args: GetDiseaseProfileArgs,
    ) -> Result<ToolOutput, ToolError> {
        let k = &ctx.deps.knowledge;
        let profile = k
            .problem(&args.disease)
            .await?
            .ok_or_else(|| ToolError::Rejected(format!("Unknown problem {}.", args.disease)))?;
        let treatments = k.treatments(&profile.treatments).await?;
        let mut out = ToolOutput::json(&serde_json::json!({
            "profile": profile,
            "treatments": treatments,
        }));
        out.effects = citations(ctx, &profile.sources).await?;
        Ok(out)
    }
}

/// `compare_lookalikes(disease)`.
#[derive(Debug)]
pub struct CompareLookalikes;

impl Tool for CompareLookalikes {
    type Args = GetDiseaseProfileArgs;
    const NAME: ToolName = ToolName::CompareLookalikes;
    const DESCRIPTION: &'static str =
        "Problems that look like this one, and how to tell them apart.";
    const LABEL: &'static str = "Comparing look-alikes";
    const KIND: ToolKind = ToolKind::Read;

    async fn run(
        &self,
        ctx: &ToolCtx<'_>,
        args: GetDiseaseProfileArgs,
    ) -> Result<ToolOutput, ToolError> {
        let lookalikes = ctx.deps.knowledge.lookalikes(&args.disease).await?;
        Ok(ToolOutput::json(&lookalikes))
    }
}

/// `get_organism_profile(organism)`.
#[derive(Debug)]
pub struct GetOrganismProfile;

/// Arguments of `get_organism_profile`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetOrganismProfileArgs {
    /// An organism id from the knowledge base.
    pub organism: OrganismId,
}

impl Tool for GetOrganismProfile {
    type Args = GetOrganismProfileArgs;
    const NAME: ToolName = ToolName::GetOrganismProfile;
    const DESCRIPTION: &'static str = "How to recognise an organism, its role (pest, beneficial, \
        invasive), what eats it and where to report it.";
    const LABEL: &'static str = "Reading about the organism";
    const KIND: ToolKind = ToolKind::Read;

    async fn run(
        &self,
        ctx: &ToolCtx<'_>,
        args: GetOrganismProfileArgs,
    ) -> Result<ToolOutput, ToolError> {
        let k = &ctx.deps.knowledge;
        let profile = k
            .organism(&args.organism)
            .await?
            .ok_or_else(|| ToolError::Rejected(format!("Unknown organism {}.", args.organism)))?;
        let prey = k.prey_of(&args.organism).await?;
        let mut out =
            ToolOutput::json(&serde_json::json!({ "profile": profile, "preys_on": prey }));
        out.effects = citations(ctx, &profile.sources).await?;
        Ok(out)
    }
}

// ---------------------------------------------------------------------------------------------

/// `get_soil_profile()`; the garden is the conversation's.
#[derive(Debug)]
pub struct GetSoilProfile;

/// No arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoArgs {}

impl Tool for GetSoilProfile {
    type Args = NoArgs;
    const NAME: ToolName = ToolName::GetSoilProfile;
    const DESCRIPTION: &'static str = "The garden's soil profile from SoilGrids, the latest soil \
        moisture and temperature, and the latest texture check of this scan.";
    const LABEL: &'static str = "Checking the soil";
    const KIND: ToolKind = ToolKind::Read;

    async fn run(&self, ctx: &ToolCtx<'_>, _: NoArgs) -> Result<ToolOutput, ToolError> {
        let garden = ctx.owner.membership().garden();
        let profile = ctx
            .deps
            .stores
            .gardens
            .garden_context(garden)
            .await?
            .soil_profile;
        let latest = garden_weather(ctx.deps, garden)
            .await
            .ok_or_else(|| {
                ToolError::Rejected("Soil readings are unavailable right now.".to_owned())
            })?
            .soil;
        // The texture of a guided check is per scan; only a scan conversation has one at hand.
        let texture = match ctx.anchor {
            domain::chat::Anchor::Scan(scan) => ctx
                .deps
                .stores
                .analysis
                .observations(scan)
                .await?
                .into_iter()
                .filter(|o| o.detail.as_ref().is_some_and(|d| d.guided_kind.is_some()))
                .max_by_key(|o| o.created_at),
            _ => None,
        };
        Ok(ToolOutput::json(&SoilProfileView {
            soil_profile: profile,
            latest,
            texture,
        }))
    }
}

// ---------------------------------------------------------------------------------------------

/// `get_plant_measurements(plant_id, quantity?)`.
#[derive(Debug)]
pub struct GetPlantMeasurements;

/// A measured quantity (`GrowthQuantity`).
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuantityArg {
    /// Plant height.
    PlantHeightCm,
    /// Canopy width.
    CanopyWidthCm,
}

impl From<QuantityArg> for GrowthQuantity {
    fn from(q: QuantityArg) -> Self {
        match q {
            QuantityArg::PlantHeightCm => Self::PlantHeightCm,
            QuantityArg::CanopyWidthCm => Self::CanopyWidthCm,
        }
    }
}

/// Arguments of `get_plant_measurements`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetPlantMeasurementsArgs {
    /// A plant of this garden.
    pub plant_id: PlantId,
    /// One quantity, or both.
    #[serde(default)]
    pub quantity: Option<QuantityArg>,
}

impl Tool for GetPlantMeasurements {
    type Args = GetPlantMeasurementsArgs;
    const NAME: ToolName = ToolName::GetPlantMeasurements;
    const DESCRIPTION: &'static str =
        "A plant's measured height and canopy width over time, from LiDAR scans.";
    const LABEL: &'static str = "Checking the growth curve";
    const KIND: ToolKind = ToolKind::Read;

    async fn run(
        &self,
        ctx: &ToolCtx<'_>,
        args: GetPlantMeasurementsArgs,
    ) -> Result<ToolOutput, ToolError> {
        let points = ctx
            .deps
            .stores
            .plants
            .growth_series(
                ctx.owner.membership(),
                args.plant_id,
                args.quantity.map(Into::into),
                page(50),
            )
            .await?
            .items;
        Ok(ToolOutput::json(&points))
    }
}

// ---------------------------------------------------------------------------------------------

/// `get_garden_map()`; the garden is the conversation's.
#[derive(Debug)]
pub struct GetGardenMap;

impl Tool for GetGardenMap {
    type Args = NoArgs;
    const NAME: ToolName = ToolName::GetGardenMap;
    const DESCRIPTION: &'static str =
        "The garden map: beds, plant positions in metres, neighbours and spacing.";
    const LABEL: &'static str = "Looking at the garden map";
    const KIND: ToolKind = ToolKind::Read;

    async fn run(&self, ctx: &ToolCtx<'_>, _: NoArgs) -> Result<ToolOutput, ToolError> {
        let map = ctx.deps.stores.maps.map(ctx.owner.membership()).await?;
        let neighbours = map
            .as_ref()
            .map(|m| neighbours(&m.pins))
            .unwrap_or_default();
        Ok(ToolOutput::json(&GardenMapView { map, neighbours }))
    }
}

// ---------------------------------------------------------------------------------------------

/// `list_tasks`.
#[derive(Debug)]
pub struct ListTasks;

/// Which tasks (`TaskState`).
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskStateArg {
    /// Not done or dismissed.
    Open,
    /// Done.
    Done,
    /// Dismissed.
    Dismissed,
}

impl From<TaskStateArg> for TaskState {
    fn from(s: TaskStateArg) -> Self {
        match s {
            TaskStateArg::Open => Self::Open,
            TaskStateArg::Done => Self::Done,
            TaskStateArg::Dismissed => Self::Dismissed,
        }
    }
}

/// Arguments of `list_tasks`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListTasksArgs {
    /// Default: open tasks.
    #[serde(default)]
    pub state: Option<TaskStateArg>,
}

impl Tool for ListTasks {
    type Args = ListTasksArgs;
    const NAME: ToolName = ToolName::ListTasks;
    const DESCRIPTION: &'static str = "The garden's tasks, by due date.";
    const LABEL: &'static str = "Checking the tasks";
    const KIND: ToolKind = ToolKind::Read;

    async fn run(&self, ctx: &ToolCtx<'_>, args: ListTasksArgs) -> Result<ToolOutput, ToolError> {
        let member = ctx.owner.membership();
        let query = TaskQuery {
            garden_id: member.garden(),
            source: None,
            state: Some(args.state.map_or(TaskState::Open, Into::into)),
        };
        let tasks = ctx
            .deps
            .stores
            .plans
            .tasks(member, query, page(50))
            .await?
            .items;
        Ok(ToolOutput::json(&tasks))
    }
}
