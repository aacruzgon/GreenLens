//! The knowledge tables as the `knowledge` crate reads them ([database 3], knowledge;
//! [database 5], vector search). `KnowledgeStore` belongs to the `knowledge` crate, which loads
//! the curated records once through [`KnowledgeRepo::load_knowledge`] and answers lookups from
//! memory; the row types and statements here are public so its loader and migration can use
//! them too.

use domain::{
    chat::KnowledgeHit,
    garden::species::SpeciesPhoto,
    id::{ProblemId, SpeciesId},
    jobs::{OrganismOption, SpeciesOption},
    knowledge::{
        ClassifierLabel, ClassifierLabels, OrganismSeed, ProblemSeed, ProductSeed, ProductsSeed,
        SoilConditionSeed, SourceSeed, SpeciesSeed, TreatmentSeed,
    },
    ports::{
        ChunkFilter, ContentType, Embedding, KnowledgeRepo, KnowledgeSnapshot, ObjectKey,
        ObjectRef, StoreError, VlmOptions,
    },
    records::diagnosis::Citation,
    url::PhotoHash,
};
use surrealdb::types::{RecordId, SurrealValue};

use crate::{
    SurrealStore,
    error::{RowError, take_failed},
    ids::{from_rid, opt_rid, rid},
    read::json_of,
    repo::analysis::enum_str,
    rows::from_json,
    tx::run_tx,
};

/// The statements.
pub mod sql {
    /// The whole curated knowledge base in one read ([database 3]), each table projected into
    /// its seed shape (contracts 10), edges resolved from the graph: `affects` from
    /// `susceptible_to`, `caused_by`, `looks_like` with `how_to_tell`, `treatments` from
    /// `manages`, and `preys_on` on each organism. Statements: 0 species, 1 problems,
    /// 2 look-alikes, 3 organisms, 4 treatments, 5 sources, 6 products, 7 classifier labels,
    /// 8 soil conditions.
    pub const LOAD_KNOWLEDGE: &str = "SELECT id, scientific_name, common_names, profile, care, classifier_supported, bioclip_label,
       sources, reviewed_by, reference_images FROM species ORDER BY id;
SELECT id, name, kind, damage_patterns, prevalence, indicators, conditions, sources, reviewed_by, reviewed_at,
       <-susceptible_to<-species AS affects, ->caused_by->organism AS caused_by,
       (SELECT out AS problem, how_to_tell FROM looks_like WHERE in = $parent.id) AS looks_like,
       <-manages<-treatment AS treatments
FROM disease ORDER BY id;
SELECT in AS problem, out AS lookalike, how_to_tell FROM looks_like;
SELECT id, name, scientific_name, taxon_group, role, recognition, reporting_url, bioclip_label, sources, reviewed_by,
       ->preys_on->organism AS preys_on FROM organism ORDER BY id;
SELECT id, kind, text, sources FROM treatment ORDER BY id;
SELECT id, title, publisher, url, licence, retrieved_at FROM source ORDER BY id;
SELECT name, broad_spectrum FROM product ORDER BY name;
SELECT record::id(id)[0] AS model_version, record::id(id)[1] AS index, label, species AS species_id,
       problem ?? 'healthy' AS problem_id FROM classifier_label ORDER BY id;
SELECT record::id(id)[0] AS aspect, record::id(id)[1] AS value, description, what_to_do, sources
FROM soil_condition ORDER BY id;";

    /// `search_knowledge` ([database 5] ✓): HNSW `<|k, 40|>` with the indexed filters applied
    /// before the graph search (`KnnScan` with the predicate). A `NONE` filter matches all.
    pub const SEARCH_CHUNKS: &str = "SELECT text, source, source.title AS title, source.publisher AS publisher, source.url AS url
FROM knowledge_chunk
WHERE ($topic = NONE OR topic = $topic) AND ($crop = NONE OR crop = $crop)
  AND ($problem = NONE OR problem = $problem) AND ($organism = NONE OR organism = $organism)
  AND embedding <|$k, 40|> $query;";

    /// `GET /species/{id}/images/{hash}`: the photo's key under `knowledge/`.
    pub const SPECIES_PHOTO: &str =
        "SELECT VALUE id FROM $species WHERE $key IN reference_images.storage_key;";
}

/// The `BioCLIP` label set's version: the schema stores no generated files (contracts 10), so it
/// is derived from what the label list is built from (every species' and organism's
/// `bioclip_label`, in id order), as 16 hex digits of an FNV-1a hash. Any change to a label
/// changes it.
pub fn label_set_version(species: &[SpeciesSeed], organisms: &[OrganismSeed]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let labels = species
        .iter()
        .map(|s| (s.id.as_str(), s.bioclip_label.as_str()))
        .chain(
            organisms
                .iter()
                .map(|o| (o.id.as_str(), o.bioclip_label.as_str())),
        );
    for (id, label) in labels {
        for b in id.bytes().chain([0]).chain(label.bytes()).chain([0]) {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

/// A `knowledge_chunk` hit with its source.
#[derive(Debug, Clone, SurrealValue)]
pub struct ChunkHitRow {
    /// The chunk text.
    pub text: String,
    /// `source:<slug>`.
    pub source: RecordId,
    /// The source's title.
    pub title: String,
    /// Its publisher.
    pub publisher: String,
    /// Its URL.
    pub url: String,
}

impl TryFrom<ChunkHitRow> for KnowledgeHit {
    type Error = RowError;

    fn try_from(row: ChunkHitRow) -> Result<Self, RowError> {
        Ok(Self {
            text: row.text,
            citation: Citation {
                source_id: from_rid(&row.source)?,
                title: row.title,
                publisher: row.publisher,
                url: row.url,
            },
        })
    }
}

/// A `species.reference_images[*]` entry.
#[derive(Debug, Clone, SurrealValue)]
pub struct ReferenceImageRow {
    /// `knowledge/species/{species_id}/{hash}.webp`.
    pub storage_key: String,
    /// "Photo: A. Author, CC BY-SA 4.0".
    pub credit: String,
    /// `source:<slug>`.
    pub source: RecordId,
}

/// A `looks_like` edge read both ways.
#[derive(Debug, Clone, SurrealValue)]
pub struct LookalikeRow {
    /// The problem.
    pub problem: RecordId,
    /// Its look-alike.
    pub lookalike: RecordId,
    /// How to tell them apart.
    pub how_to_tell: String,
}

#[async_trait::async_trait]
#[allow(
    clippy::too_many_lines,
    reason = "load_knowledge reads every table of the knowledge base"
)]
impl KnowledgeRepo for SurrealStore {
    async fn load_knowledge(&self) -> Result<KnowledgeSnapshot, StoreError> {
        const WHAT: &str = "load knowledge";
        let mut r = run_tx(&self.db, sql::LOAD_KNOWLEDGE, |q| q)
            .await
            .map_err(|f| f.into_store(WHAT))?;
        let row_err = |e: RowError| e.into_store(WHAT);
        let json = |r: &mut surrealdb::IndexedResults,
                    i: usize|
         -> Result<Vec<serde_json::Value>, StoreError> {
            let v: surrealdb::types::Value = r.take(i).map_err(|e| take_failed(WHAT, e))?;
            Ok(match json_of(v) {
                serde_json::Value::Array(items) => items,
                _ => Vec::new(),
            })
        };

        // Species: the stored photos become `species_photos`; the seed's `file` isn't stored.
        let mut species = Vec::new();
        let mut species_photos = Vec::new();
        for mut s in json(&mut r, 0)? {
            let images = s
                .as_object_mut()
                .and_then(|m| m.remove("reference_images"))
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();
            let seed: SpeciesSeed = from_json("species", s).map_err(row_err)?;
            let photos = images
                .iter()
                .filter_map(|i| {
                    let key = i.get("storage_key")?.as_str()?;
                    let hash = key.rsplit('/').next()?.strip_suffix(".webp")?;
                    Some(SpeciesPhoto {
                        hash: PhotoHash::parse(hash)?,
                        credit: i.get("credit")?.as_str()?.to_owned(),
                    })
                })
                .collect();
            species_photos.push((seed.id.clone(), photos));
            species.push(seed);
        }
        let problems = json(&mut r, 1)?
            .into_iter()
            .map(|v| from_json("disease", v))
            .collect::<Result<Vec<ProblemSeed>, _>>()
            .map_err(row_err)?;
        let lookalikes = json(&mut r, 2)?
            .into_iter()
            .map(|v| {
                from_json::<(ProblemId, ProblemId, String)>(
                    "looks_like",
                    serde_json::json!([v["problem"], v["lookalike"], v["how_to_tell"]]),
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(row_err)?;
        let organisms = json(&mut r, 3)?
            .into_iter()
            .map(|v| from_json("organism", v))
            .collect::<Result<Vec<OrganismSeed>, _>>()
            .map_err(row_err)?;
        let treatments = json(&mut r, 4)?
            .into_iter()
            .map(|v| from_json("treatment", v))
            .collect::<Result<Vec<TreatmentSeed>, _>>()
            .map_err(row_err)?;
        let sources = json(&mut r, 5)?
            .into_iter()
            .map(|v| from_json("source", v))
            .collect::<Result<Vec<SourceSeed>, _>>()
            .map_err(row_err)?;
        let products = ProductsSeed {
            products: json(&mut r, 6)?
                .into_iter()
                .map(|v| from_json("product", v))
                .collect::<Result<Vec<ProductSeed>, _>>()
                .map_err(row_err)?,
        };
        // Labels of the newest model version (`classifier_label:[model_version, index]`).
        let mut labels = json(&mut r, 7)?;
        let current = labels
            .iter()
            .filter_map(|l| {
                l.get("model_version")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .max()
            .unwrap_or_default();
        labels.retain(|l| {
            l.get("model_version").and_then(serde_json::Value::as_str) == Some(current.as_str())
        });
        for l in &mut labels {
            if let Some(m) = l.as_object_mut() {
                m.remove("model_version");
            }
        }
        let classifier_labels = ClassifierLabels {
            model_version: current,
            labels: labels
                .into_iter()
                .map(|v| from_json("classifier_label", v))
                .collect::<Result<Vec<ClassifierLabel>, _>>()
                .map_err(row_err)?,
        };
        let soil_conditions = json(&mut r, 8)?
            .into_iter()
            .map(|v| from_json("soil_condition", v))
            .collect::<Result<Vec<SoilConditionSeed>, _>>()
            .map_err(row_err)?;
        let vlm_options = VlmOptions {
            label_set_version: label_set_version(&species, &organisms),
            species: species
                .iter()
                .map(|s| SpeciesOption {
                    id: s.id.clone(),
                    name: s
                        .common_names
                        .first()
                        .cloned()
                        .unwrap_or_else(|| s.scientific_name.clone()),
                })
                .collect(),
            organisms: organisms
                .iter()
                .map(|o| OrganismOption {
                    id: o.id.clone(),
                    name: o.name.clone(),
                })
                .collect(),
        };
        Ok(KnowledgeSnapshot {
            species,
            species_photos,
            problems,
            lookalikes,
            organisms,
            treatments,
            sources,
            products,
            classifier_labels,
            soil_conditions,
            vlm_options,
        })
    }

    async fn search_chunks(
        &self,
        query: &Embedding,
        filter: ChunkFilter,
        k: usize,
    ) -> Result<Vec<KnowledgeHit>, StoreError> {
        const WHAT: &str = "search chunks";
        let topic = filter
            .topic
            .as_ref()
            .map(|t| enum_str("topic", t))
            .transpose()
            .map_err(|e| e.into_store(WHAT))?;
        let mut results = run_tx(&self.db, sql::SEARCH_CHUNKS, |q| {
            q.bind(("topic", topic.clone()))
                .bind(("crop", opt_rid(filter.crop.as_ref())))
                .bind(("problem", opt_rid(filter.problem.as_ref())))
                .bind(("organism", opt_rid(filter.organism.as_ref())))
                .bind(("k", i64::try_from(k).unwrap_or(5)))
                .bind(("query", query.0.clone()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let rows: Vec<ChunkHitRow> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        rows.into_iter()
            .map(|row| KnowledgeHit::try_from(row).map_err(|e| e.into_store(WHAT)))
            .collect()
    }

    async fn species_photo_object(
        &self,
        species: &SpeciesId,
        hash: PhotoHash,
    ) -> Result<Option<ObjectRef>, StoreError> {
        const WHAT: &str = "species photo";
        let key = ObjectKey::species_photo(species, hash);
        let mut results = run_tx(&self.db, sql::SPECIES_PHOTO, |q| {
            q.bind(("species", rid(species)))
                .bind(("key", key.as_str()))
        })
        .await
        .map_err(|f| f.into_store(WHAT))?;
        let found: Vec<RecordId> = results.take(0).map_err(|e| take_failed(WHAT, e))?;
        Ok(found.first().map(|_| ObjectRef {
            key,
            content_type: ContentType::Webp,
        }))
    }
}
