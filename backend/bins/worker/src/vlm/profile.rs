//! The model adapter's properties ([contracts 6]): box format, cell size and pixel budget belong
//! to the adapter **instance** (model, server and preparation together), never to the model
//! alone, so each worker sets them in its environment and reports them in `model_versions`.

use std::collections::BTreeMap;

use domain::{
    jobs::ModelVersionKey,
    vlm::BoxFormat,
    wire::{Certainty, Confidence},
};

/// The prompt versions ([vision vlm 7]), in the contract's `name@n` form.
///
/// [vision vlm 7]: ../../../docs/vision/vlm.md#7-prompts
pub const PROMPTS: &str = "system@2,findings@3,indicators@2,verify@2,guided@1";

/// Certainty words to confidences ([contracts 6], after decoding): 0.4 / 0.65 / 0.85 until
/// calibration moves each value within its band, never above 0.85.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CertaintyMap {
    /// `low`.
    pub low: f64,
    /// `medium`.
    pub medium: f64,
    /// `high`.
    pub high: f64,
}

impl Default for CertaintyMap {
    fn default() -> Self {
        Self {
            low: 0.4,
            medium: 0.65,
            high: 0.85,
        }
    }
}

impl CertaintyMap {
    /// The confidence of a word.
    pub fn confidence(&self, c: Certainty) -> Confidence {
        Confidence::saturating(match c {
            Certainty::Low => self.low,
            Certainty::Medium => self.medium,
            Certainty::High => self.high,
        })
    }

    /// `vlm_certainty`: `0.4/0.65/0.85`.
    pub fn key(&self) -> String {
        format!("{}/{}/{}", self.low, self.medium, self.high)
    }
}

/// Output budgets per call (`max_completion_tokens` on vLLM, `max_tokens` elsewhere).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallBudget {
    /// The findings call.
    pub findings: u32,
    /// The indicators (and verify) call.
    pub indicators: u32,
    /// The guided-check call.
    pub guided: u32,
}

impl CallBudget {
    /// The local VLM ([vision vlm 7], token budget).
    pub const LOCAL: Self = Self {
        findings: 1500,
        indicators: 1500,
        guided: 100,
    };
    /// The hosted fallback: Opus 5.5 always thinks, and thinking counts toward `max_tokens`, so
    /// even the guided check's short answer gets the full budget (contracts 6).
    pub const HOSTED: Self = Self {
        findings: 8000,
        indicators: 8000,
        guided: 8000,
    };
}

/// What `model_versions` says about the VLM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VlmIdentity {
    /// `vlm_model`: repository@revision, or the hosted model id.
    pub model: String,
    /// `vlm_quant`, for local checkpoints.
    pub quant: Option<String>,
    /// `vlm_server`: `vllm@v0.31.0`, `vllm-rocm@v0.31.0`, `llama.cpp@b11669`, `anthropic`.
    pub server: String,
}

/// One adapter instance.
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterProfile {
    /// How the model writes `bbox_2d`.
    pub box_format: BoxFormat,
    /// Each prepared side is a multiple of this (32 for Qwen3-VL; 1 for the hosted model).
    pub cell_px: u32,
    /// `VLM_MAX_PIXELS`.
    pub max_pixels: u32,
    /// A long-side cap (1568 for the hosted model, which resizes larger images itself).
    pub max_long_side: Option<u32>,
    /// JPEG quality of the prepared images.
    pub jpeg_quality: u8,
    /// Output budgets.
    pub budget: CallBudget,
    /// The certainty mapping.
    pub certainty: CertaintyMap,
    /// For `model_versions`.
    pub identity: VlmIdentity,
}

impl AdapterProfile {
    /// The `model_versions` keys of a VLM adapter ([contracts 5]).
    pub fn model_versions(&self) -> BTreeMap<String, String> {
        let mut v = BTreeMap::new();
        v.insert(
            ModelVersionKey::VLM_MODEL.to_owned(),
            self.identity.model.clone(),
        );
        if let Some(q) = &self.identity.quant {
            v.insert(ModelVersionKey::VLM_QUANT.to_owned(), q.clone());
        }
        v.insert(
            ModelVersionKey::VLM_SERVER.to_owned(),
            self.identity.server.clone(),
        );
        v.insert(
            ModelVersionKey::VLM_BOX_FORMAT.to_owned(),
            box_format_name(self.box_format).to_owned(),
        );
        v.insert(
            ModelVersionKey::VLM_MAX_PIXELS.to_owned(),
            self.max_pixels.to_string(),
        );
        v.insert(ModelVersionKey::VLM_PROMPTS.to_owned(), PROMPTS.to_owned());
        v.insert(
            ModelVersionKey::VLM_CERTAINTY.to_owned(),
            self.certainty.key(),
        );
        v
    }
}

/// The contract's name of a box format.
pub const fn box_format_name(f: BoxFormat) -> &'static str {
    match f {
        BoxFormat::Norm1000 => "norm_1000",
        BoxFormat::AbsPxPrepared => "abs_px_prepared",
        BoxFormat::Norm1 => "norm_1",
    }
}
