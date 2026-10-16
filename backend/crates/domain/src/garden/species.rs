//! The species list ([contracts 9], species): read by any signed-in user.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    id::SpeciesId,
    scan::{ReferenceImage, SpeciesRef},
    url::{ImagePath, PhotoHash, Signed, Signer},
};

/// `SpeciesSummary.profile` and `SpeciesSeed.profile`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SpeciesProfileKind {
    /// Care profile and problems.
    Full,
    /// Recognised by `BioCLIP`, no care profile or problems yet.
    LabelOnly,
}

/// `SpeciesSummary extends SpeciesRef` ([contracts 9]). Only exists on the wire: it is built
/// from the knowledge record, whose photos are storage keys, so it has no stored form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SpeciesSummary {
    /// `id`, `scientific_name`, `common_name`.
    #[serde(flatten)]
    pub species: SpeciesRef,
    /// `full` or `label_only`.
    pub profile: SpeciesProfileKind,
    /// The disease classifier covers it.
    pub classifier_supported: bool,
    /// At most 3, in seed order.
    pub reference_images: Vec<ReferenceImage<Signed>>,
}

/// A species photo as the knowledge base records it: the content hash in its key and its
/// credit ([contracts 10], `SpeciesSeed.reference_images`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeciesPhoto {
    /// The first 16 hex digits of the WebP's SHA-256.
    pub hash: PhotoHash,
    /// "Photo: A. Author, CC BY-SA 4.0".
    pub credit: String,
}

impl SpeciesSummary {
    /// Signs a species' photos for the list and detail endpoints.
    pub fn signed_images(
        species: &SpeciesId,
        photos: &[SpeciesPhoto],
        signer: &Signer<'_>,
    ) -> Vec<ReferenceImage<Signed>> {
        photos
            .iter()
            .take(3)
            .map(|p| ReferenceImage {
                url: signer.url(&ImagePath::SpeciesPhoto {
                    species: species.clone(),
                    hash: p.hash,
                }),
                credit: p.credit.clone(),
            })
            .collect()
    }
}

/// `GET /species?q=`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SpeciesQuery {
    /// Matches common and scientific names, case-insensitively, anywhere in the name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
}
