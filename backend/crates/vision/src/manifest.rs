//! `ml/models/manifest.toml` ([vision 6]): every model file a worker or `api` loads, with its
//! SHA-256, input recipe and configuration. Checked at start-up: a mismatch refuses to start.
//!
//! [vision 6]: ../../../docs/vision/README.md#6-model-files-versions-and-pinning

use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};

use data_encoding::HEXLOWER;
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Why the manifest can't be used.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    /// The manifest or a model file couldn't be read.
    #[error("reading {path}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// The cause.
        #[source]
        source: std::io::Error,
    },
    /// The manifest isn't valid TOML of the expected shape.
    #[error("manifest: {0}")]
    Parse(#[from] toml::de::Error),
    /// No entry of that name.
    #[error("no model named {0} in the manifest")]
    Missing(String),
    /// A file's hash differs from the manifest's.
    #[error("{name}: sha256 {actual} does not match the manifest's {expected}")]
    HashMismatch {
        /// The entry.
        name: String,
        /// The manifest's hash.
        expected: String,
        /// The file's hash.
        actual: String,
    },
    /// A `[model.config]` value is missing or of the wrong type.
    #[error("{name}: config key {key} is missing or not a {expected}")]
    Config {
        /// The entry.
        name: String,
        /// The key.
        key: &'static str,
        /// The type expected.
        expected: &'static str,
    },
    /// Two entries give different values for a setting the worker holds once.
    #[error("config key {key} is {first_value} in {first} but {second_value} in {second}")]
    Conflict {
        /// The key.
        key: &'static str,
        /// The first entry.
        first: String,
        /// Its value.
        first_value: String,
        /// The entry that differs.
        second: String,
        /// Its value.
        second_value: String,
    },
}

/// Where a model runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunsOn {
    /// The perception worker's GPU.
    Perception,
    /// `api`'s CPU (faces, text embeddings).
    Api,
}

/// How an image becomes the model's input square.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResizeRule {
    /// Squash to the input size, ignoring the aspect ratio (RF-DETR, SAM 2).
    Squash,
    /// Shortest side to the input size, then a centre crop (`BioCLIP` 2).
    ShortestSideCrop,
    /// Longest side to the input size, then pad (`MobileSAM`).
    LongestSidePad,
}

/// Channel order of the input tensor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Colour {
    /// RGB.
    Rgb,
    /// BGR (`YuNet`).
    Bgr,
}

/// One input dimension: fixed, or named and dynamic (`"batch"`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Dim {
    /// A fixed size.
    Fixed(u32),
    /// A dynamic axis.
    Named(String),
}

/// `[model.input]`: the image recipe, applied in Rust ([vision models 1]).
///
/// [vision models 1]: ../../../docs/vision/models.md#1-conventions-for-every-model
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ModelInput {
    /// The input tensor's name.
    pub name: String,
    /// Its shape, NCHW.
    pub shape: Vec<Dim>,
    /// Channel order.
    pub colour: Colour,
    /// The resize rule.
    pub resize: ResizeRule,
    /// Per-channel mean, on 0–1 values.
    #[serde(default)]
    pub mean: Option<[f32; 3]>,
    /// Per-channel standard deviation.
    #[serde(default)]
    pub std: Option<[f32; 3]>,
}

impl ModelInput {
    /// The square side for image models (`[batch, 3, side, side]`), when fixed.
    pub fn side(&self) -> Option<u32> {
        match self.shape.get(3) {
            Some(Dim::Fixed(side)) => Some(*side),
            _ => None,
        }
    }
}

/// One `[[model]]` entry.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ModelEntry {
    /// `leaf_detector`, `sam_encoder`, `bioclip`, `yunet`, `text_embedder` …
    pub name: String,
    /// Reported in `model_versions` and each observation's `model_version`.
    pub version: String,
    /// Relative to the manifest's directory.
    pub file: PathBuf,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: String,
    /// The licence, for the notices.
    pub licence: String,
    /// Where it came from.
    #[serde(default)]
    pub source: String,
    /// Where it runs.
    pub runs_on: RunsOn,
    /// The input recipe, for image models.
    #[serde(default)]
    pub input: Option<ModelInput>,
    /// Logical output → tensor name.
    #[serde(default)]
    pub outputs: BTreeMap<String, String>,
    /// Thresholds, caps and temperatures, set from the evaluation set.
    #[serde(default)]
    pub config: toml::Table,
    /// For label matrices: the `label_set_version` they belong to.
    #[serde(default)]
    pub label_set_version: Option<String>,
    /// For the disease head: its `classifier_labels_version`.
    #[serde(default)]
    pub classifier_labels_version: Option<String>,
    /// `fp16` or `fp32` for ONNX files; the CPU provider runs only `fp32`.
    #[serde(default)]
    pub precision: Option<String>,
}

impl ModelEntry {
    /// A float from `[model.config]`.
    pub fn config_f32(&self, key: &'static str) -> Result<f32, ManifestError> {
        self.config
            .get(key)
            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
            .map(|v| v as f32)
            .ok_or_else(|| self.config_error(key, "number"))
    }

    /// A float from `[model.config]`, at full precision (shares and growth factors).
    pub fn config_f64(&self, key: &'static str) -> Result<f64, ManifestError> {
        self.config
            .get(key)
            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
            .ok_or_else(|| self.config_error(key, "number"))
    }

    /// An unsigned integer from `[model.config]`.
    pub fn config_usize(&self, key: &'static str) -> Result<usize, ManifestError> {
        self.config
            .get(key)
            .and_then(toml::Value::as_integer)
            .and_then(|v| usize::try_from(v).ok())
            .ok_or_else(|| self.config_error(key, "non-negative integer"))
    }

    fn config_error(&self, key: &'static str, expected: &'static str) -> ManifestError {
        ManifestError::Config {
            name: self.name.clone(),
            key,
            expected,
        }
    }
}

#[derive(Deserialize)]
struct ManifestFile {
    #[serde(rename = "model")]
    models: Vec<ModelEntry>,
}

/// The parsed manifest, with the directory its relative paths start from.
#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    root: PathBuf,
    models: Vec<ModelEntry>,
}

impl Manifest {
    /// Reads and parses the manifest. Hashes are checked separately ([`Manifest::verify`]),
    /// for only the files this process loads.
    pub fn load(path: &Path) -> Result<Self, ManifestError> {
        let text = std::fs::read_to_string(path).map_err(|source| ManifestError::Io {
            path: path.to_owned(),
            source,
        })?;
        let mut manifest = Self::parse(&text)?;
        manifest.root = path.parent().map(Path::to_owned).unwrap_or_default();
        Ok(manifest)
    }

    /// Parses manifest text; paths are relative to the current directory.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let file: ManifestFile = toml::from_str(text)?;
        Ok(Self {
            root: PathBuf::new(),
            models: file.models,
        })
    }

    /// Every entry.
    pub fn models(&self) -> &[ModelEntry] {
        &self.models
    }

    /// The entry named `name`.
    pub fn entry(&self, name: &str) -> Result<&ModelEntry, ManifestError> {
        self.models
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| ManifestError::Missing(name.to_owned()))
    }

    /// Every entry whose name starts with `prefix` (the label matrices, one per version).
    pub fn entries_with_prefix<'m>(
        &'m self,
        prefix: &'m str,
    ) -> impl Iterator<Item = &'m ModelEntry> + 'm {
        self.models
            .iter()
            .filter(move |m| m.name.starts_with(prefix))
    }

    /// The absolute path of an entry's file.
    pub fn path(&self, entry: &ModelEntry) -> PathBuf {
        self.root.join(&entry.file)
    }

    /// Checks the SHA-256 of each named entry's file against the manifest.
    pub fn verify(&self, names: &[&str]) -> Result<(), ManifestError> {
        for name in names {
            let entry = self.entry(name)?;
            let actual = sha256_file(&self.path(entry))?;
            if !actual.eq_ignore_ascii_case(&entry.sha256) {
                return Err(ManifestError::HashMismatch {
                    name: entry.name.clone(),
                    expected: entry.sha256.clone(),
                    actual,
                });
            }
        }
        Ok(())
    }

    /// `perception_config` ([contracts 5], `model_versions` keys): the first 16 hex digits of
    /// the SHA-256 over every entry's name and `[model.config]` table, in manifest order. TOML
    /// tables serialise with sorted keys, so the hash depends only on the values.
    pub fn perception_config(&self) -> String {
        let mut hasher = Sha256::new();
        for entry in &self.models {
            hasher.update(entry.name.as_bytes());
            hasher.update(b"\n");
            hasher.update(entry.config.to_string().as_bytes());
            hasher.update(b"\n");
        }
        let digest = HEXLOWER.encode(&hasher.finalize());
        digest[..16].to_owned()
    }
}

/// The lowercase hex SHA-256 of a file, streamed in 64 KiB blocks.
pub fn sha256_file(path: &Path) -> Result<String, ManifestError> {
    let io = |source| ManifestError::Io {
        path: path.to_owned(),
        source,
    };
    let mut file = std::fs::File::open(path).map_err(io)?;
    let mut hasher = Sha256::new();
    let mut block = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut block).map_err(io)?;
        if n == 0 {
            break;
        }
        hasher.update(&block[..n]);
    }
    Ok(HEXLOWER.encode(&hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[[model]]
name      = "leaf_detector"
version   = "rfdetr-s-leaf-2026-10-18"
file      = "leaf_detector/rfdetr.onnx"
sha256    = "00"
licence   = "Apache-2.0"
runs_on   = "perception"

  [model.input]
  name   = "input"
  shape  = ["batch", 3, 512, 512]
  colour = "rgb"
  resize = "squash"
  mean   = [0.485, 0.456, 0.406]
  std    = [0.229, 0.224, 0.225]

  [model.outputs]
  boxes  = "dets"
  logits = "labels"

  [model.config]
  score_threshold  = 0.35
  max_batch        = 8
"#;

    #[test]
    fn parses_an_entry_and_reads_its_config() {
        let m = Manifest::parse(SAMPLE).unwrap();
        let e = m.entry("leaf_detector").unwrap();
        assert_eq!(e.input.as_ref().unwrap().side(), Some(512));
        assert_eq!(e.outputs["boxes"], "dets");
        assert_eq!(e.config_usize("max_batch").unwrap(), 8);
        assert!((e.config_f32("score_threshold").unwrap() - 0.35).abs() < 1e-6);
        assert!(e.config_f32("missing").is_err());
    }

    #[test]
    fn perception_config_follows_the_values() {
        let a = Manifest::parse(SAMPLE).unwrap().perception_config();
        let b = Manifest::parse(&SAMPLE.replace("0.35", "0.4"))
            .unwrap()
            .perception_config();
        assert_eq!(a.len(), 16);
        assert_ne!(a, b);
    }
}
