//! What runs on the worker's CPU on top of `BioCLIP` 2's 768-dimensional embeddings
//! ([vision models 2, 6, 7]): the label matrix (species scoring), the disease head, the optional
//! species head and the plant probe. Each is one JSON file pinned in the manifest, with its
//! matrices as base64 little-endian `f32`.
//!
//! [vision models 2, 6, 7]: ../../../docs/vision/models.md#6-species-bioclip

use std::path::Path;

use data_encoding::BASE64;
use domain::{
    id::{ProblemOrHealthy, SpeciesId},
    jobs::{ProblemScore, SpeciesLabel},
    scan::ImageKind,
    wire::Confidence,
};
use serde::Deserialize;

use crate::VisionError;

/// `BioCLIP` 2's embedding width.
pub const EMBEDDING_DIM: usize = 768;

fn floats(b64: &str, what: &'static str) -> Result<Vec<f32>, VisionError> {
    let bytes = BASE64
        .decode(b64.as_bytes())
        .map_err(|e| VisionError::Shape {
            model: what,
            detail: e.to_string(),
        })?;
    if bytes.len() % 4 != 0 {
        return Err(VisionError::Shape {
            model: what,
            detail: "matrix length is not a multiple of 4 bytes".to_owned(),
        });
    }
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect())
}

fn read_json<T: for<'de> Deserialize<'de>>(
    path: &Path,
    what: &'static str,
) -> Result<T, VisionError> {
    let text = std::fs::read(path).map_err(|e| VisionError::Shape {
        model: what,
        detail: format!("{}: {e}", path.display()),
    })?;
    serde_json::from_slice(&text).map_err(|e| VisionError::Shape {
        model: what,
        detail: e.to_string(),
    })
}

#[derive(Deserialize)]
struct LabelFile {
    label_set_version: String,
    species: Vec<LabelRow>,
    rows: String,
}

#[derive(Deserialize)]
struct LabelRow {
    id: SpeciesId,
    label: String,
}

/// The species rows of one `label_set_version`: `N × 768`, L2-normalised text embeddings of
/// every `bioclip_label`, averaged over the 80 templates offline.
#[derive(Debug, Clone, PartialEq)]
pub struct LabelMatrix {
    /// `label_set_version`.
    pub version: String,
    /// Row `i` is this species.
    pub species: Vec<(SpeciesId, String)>,
    rows: Vec<f32>,
}

impl LabelMatrix {
    /// Reads a label file.
    pub fn load(path: &Path) -> Result<Self, VisionError> {
        let file: LabelFile = read_json(path, "label matrix")?;
        let rows = floats(&file.rows, "label matrix")?;
        if rows.len() != file.species.len() * EMBEDDING_DIM {
            return Err(VisionError::Shape {
                model: "label matrix",
                detail: format!("{} values for {} species", rows.len(), file.species.len()),
            });
        }
        Ok(Self {
            version: file.label_set_version,
            species: file.species.into_iter().map(|r| (r.id, r.label)).collect(),
            rows,
        })
    }

    /// From parts (tests and the fallback).
    pub fn from_parts(version: String, species: Vec<(SpeciesId, String)>, rows: Vec<f32>) -> Self {
        Self {
            version,
            species,
            rows,
        }
    }

    /// Zero-shot probabilities of one image embedding: softmax of `(100 / T) · image · textᵀ`
    /// over the species rows (100 is the model's logit scale).
    pub fn probabilities(&self, image: &[f32], temperature: f32) -> Vec<f32> {
        let scale = 100.0 / temperature.max(1e-6);
        let logits: Vec<f32> = self
            .rows
            .as_chunks::<EMBEDDING_DIM>()
            .0
            .iter()
            .map(|row| scale * row.iter().zip(image).map(|(a, b)| a * b).sum::<f32>())
            .collect();
        softmax(&logits)
    }
}

/// Numerically stable softmax.
pub fn softmax(logits: &[f32]) -> Vec<f32> {
    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exp: Vec<f32> = logits.iter().map(|l| (l - max).exp()).collect();
    let sum: f32 = exp.iter().sum();
    exp.iter().map(|e| e / sum.max(f32::MIN_POSITIVE)).collect()
}

/// Per-shot weights for combining species probabilities: several views beat one, and a single
/// leaf close-up is the weakest ([vision models 6], step 4). Soil shots aren't scored.
pub const fn shot_weight(kind: ImageKind) -> f32 {
    match kind {
        ImageKind::WholePlant | ImageKind::FlowerFruit => 2.0,
        ImageKind::LeafCloseup | ImageKind::LeafUnderside => 1.0,
        _ => 0.0,
    }
}

/// The weighted average of per-shot probabilities, then the top `k` as `species.candidates`.
pub fn combine_species(
    matrix: &LabelMatrix,
    per_shot: &[(ImageKind, Vec<f32>)],
    k: usize,
) -> Vec<SpeciesLabel> {
    let mut sum = vec![0.0f32; matrix.species.len()];
    let mut weights = 0.0f32;
    for (kind, probs) in per_shot {
        let w = shot_weight(*kind);
        if w == 0.0 {
            continue;
        }
        weights += w;
        for (s, p) in sum.iter_mut().zip(probs) {
            *s += w * p;
        }
    }
    if weights == 0.0 {
        return Vec::new();
    }
    let mut ranked: Vec<(usize, f32)> = sum.iter().map(|s| s / weights).enumerate().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    ranked
        .into_iter()
        .take(k)
        .map(|(i, p)| SpeciesLabel {
            species_id: Some(matrix.species[i].0.clone()),
            label: matrix.species[i].1.clone(),
            score: Confidence::saturating(f64::from(p)),
        })
        .collect()
}

#[derive(Deserialize)]
struct HeadFile {
    #[serde(default)]
    classifier_labels_version: Option<String>,
    temperature: f32,
    labels: Vec<HeadLabel>,
    weight: String,
    bias: String,
}

#[derive(Deserialize)]
struct HeadLabel {
    species_id: SpeciesId,
    #[serde(default)]
    problem: Option<ProblemOrHealthy>,
}

/// A linear head on the embedding: `logits = W · e + b`, one row per class.
#[derive(Debug, Clone, PartialEq)]
struct Linear {
    weight: Vec<f32>,
    bias: Vec<f32>,
}

impl Linear {
    fn logits(&self, e: &[f32]) -> Vec<f32> {
        self.weight
            .as_chunks::<EMBEDDING_DIM>()
            .0
            .iter()
            .zip(&self.bias)
            .map(|(row, b)| row.iter().zip(e).map(|(w, x)| w * x).sum::<f32>() + b)
            .collect()
    }
}

/// The disease head: one class per `classifier_labels.yaml` entry (species, problem or healthy).
#[derive(Debug, Clone, PartialEq)]
pub struct DiseaseHead {
    /// `classifier_labels_version`.
    pub version: String,
    temperature: f32,
    labels: Vec<(SpeciesId, ProblemOrHealthy)>,
    linear: Linear,
}

impl DiseaseHead {
    /// Reads a head file.
    pub fn load(path: &Path) -> Result<Self, VisionError> {
        let file: HeadFile = read_json(path, "disease head")?;
        let weight = floats(&file.weight, "disease head")?;
        let bias = floats(&file.bias, "disease head")?;
        let labels: Vec<(SpeciesId, ProblemOrHealthy)> = file
            .labels
            .into_iter()
            .map(|l| (l.species_id, l.problem.unwrap_or(ProblemOrHealthy::Healthy)))
            .collect();
        if weight.len() != labels.len() * EMBEDDING_DIM || bias.len() != labels.len() {
            return Err(VisionError::Shape {
                model: "disease head",
                detail: "weight or bias size differs from the label count".to_owned(),
            });
        }
        Ok(Self {
            version: file.classifier_labels_version.unwrap_or_default(),
            temperature: file.temperature,
            labels,
            linear: Linear { weight, bias },
        })
    }

    /// The classes of one species, in head order: what the fallback worker's schema offers.
    pub fn classes(&self, species: &SpeciesId) -> Vec<ProblemOrHealthy> {
        self.labels
            .iter()
            .filter(|(s, _)| s == species)
            .map(|(_, p)| p.clone())
            .collect()
    }

    /// Whether the head knows the species (`classifier_supported`).
    pub fn supports(&self, species: &SpeciesId) -> bool {
        self.labels.iter().any(|(s, _)| s == species)
    }

    /// The crop's top `k`, restricted to the working species' classes before the softmax, so a
    /// tomato leaf is never labelled with a grape disease ([contracts 5], classifier candidates).
    pub fn top(&self, crop: &[f32], species: &SpeciesId, k: usize) -> Vec<ProblemScore> {
        let logits = self.linear.logits(crop);
        let (idx, restricted): (Vec<usize>, Vec<f32>) = self
            .labels
            .iter()
            .enumerate()
            .filter(|(_, (s, _))| s == species)
            .map(|(i, _)| (i, logits[i] / self.temperature.max(1e-6)))
            .unzip();
        let probs = softmax(&restricted);
        let mut ranked: Vec<(usize, f32)> = idx.into_iter().zip(probs).collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        ranked
            .into_iter()
            .take(k)
            .map(|(i, p)| ProblemScore {
                problem_id: self.labels[i].1.clone(),
                confidence: Confidence::saturating(f64::from(p)),
            })
            .collect()
    }

    #[cfg(test)]
    fn from_parts(
        labels: Vec<(SpeciesId, ProblemOrHealthy)>,
        weight: Vec<f32>,
        bias: Vec<f32>,
    ) -> Self {
        Self {
            version: "test".to_owned(),
            temperature: 1.0,
            labels,
            linear: Linear { weight, bias },
        }
    }
}

#[derive(Deserialize)]
struct ProbeFile {
    weight: String,
    bias: f32,
}

/// The plant probe: logistic regression on the embedding, `σ(w · e + b)` ([vision models 2]).
#[derive(Debug, Clone, PartialEq)]
pub struct PlantProbe {
    weight: Vec<f32>,
    bias: f32,
}

impl PlantProbe {
    /// Reads a probe file.
    pub fn load(path: &Path) -> Result<Self, VisionError> {
        let file: ProbeFile = read_json(path, "plant probe")?;
        let weight = floats(&file.weight, "plant probe")?;
        if weight.len() != EMBEDDING_DIM {
            return Err(VisionError::Shape {
                model: "plant probe",
                detail: format!("{} weights", weight.len()),
            });
        }
        Ok(Self {
            weight,
            bias: file.bias,
        })
    }

    /// The plant probability of one image.
    pub fn probability(&self, e: &[f32]) -> f32 {
        let z = self.weight.iter().zip(e).map(|(w, x)| w * x).sum::<f32>() + self.bias;
        1.0 / (1.0 + (-z).exp())
    }
}

/// The plant check over the plant shots ([vision models 2]): present when any plant shot has a
/// top leaf score ≥ `τ_leaf` or a plant probability ≥ `τ_plant`. Soil shots never count.
pub fn plant_present(shots: &[(ImageKind, f32, f32)], tau_leaf: f32, tau_plant: f32) -> bool {
    shots.iter().any(|&(kind, leaf, plant)| {
        shot_weight(kind) > 0.0 && (leaf >= tau_leaf || plant >= tau_plant)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(i: usize) -> Vec<f32> {
        let mut v = vec![0.0; EMBEDDING_DIM];
        v[i] = 1.0;
        v
    }

    #[test]
    fn species_scores_follow_the_closest_row_and_shot_weights() {
        let species: Vec<(SpeciesId, String)> = vec![
            ("tomato".parse().unwrap(), "tomato".to_owned()),
            ("basil".parse().unwrap(), "basil".to_owned()),
        ];
        let rows = [unit(0), unit(1)].concat();
        let m = LabelMatrix::from_parts("v1".to_owned(), species, rows);
        let p_tomato = m.probabilities(&unit(0), 1.0);
        assert!(p_tomato[0] > 0.99);
        let top = combine_species(
            &m,
            &[
                (ImageKind::WholePlant, m.probabilities(&unit(1), 1.0)),
                (ImageKind::LeafCloseup, p_tomato),
                (ImageKind::SoilSurface, m.probabilities(&unit(0), 1.0)),
            ],
            3,
        );
        assert_eq!(top[0].label, "basil"); // the whole-plant shot weighs twice
        assert_eq!(top.len(), 2);
    }

    #[test]
    fn the_disease_head_is_restricted_to_the_working_species() {
        let tomato: SpeciesId = "tomato".parse().unwrap();
        let grape: SpeciesId = "grape".parse().unwrap();
        let labels = vec![
            (tomato.clone(), ProblemOrHealthy::Healthy),
            (tomato.clone(), "tomato_early_blight".parse().unwrap()),
            (grape, "grape_black_rot".parse().unwrap()),
        ];
        let weight = [unit(0), unit(1), unit(2)].concat();
        let head = DiseaseHead::from_parts(labels, weight, vec![0.0; 3]);
        // the crop looks most like the grape class, but grape isn't the working species
        let mut crop = unit(2);
        crop[1] = 0.5;
        let top = head.top(&crop, &tomato, 3);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].problem_id.to_string(), "tomato_early_blight");
    }

    #[test]
    fn soil_shots_never_count_for_the_plant_check() {
        assert!(!plant_present(
            &[(ImageKind::SoilSurface, 0.9, 0.9)],
            0.5,
            0.5
        ));
        assert!(plant_present(
            &[
                (ImageKind::SoilSurface, 0.9, 0.9),
                (ImageKind::WholePlant, 0.1, 0.6)
            ],
            0.5,
            0.5
        ));
    }
}
