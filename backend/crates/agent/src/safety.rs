//! The safety rules every model-written text passes ([contracts 7], plan validation and the dose
//! check; architecture 11, safety of advice). Pure and synchronous: no I/O, no model, no clock.
//! Shared by the chat turn, the tools, the plan writer and, through the plan writer, `pipeline`.

pub mod dose;
pub mod plan;
pub mod products;
pub mod routes;
pub mod sentences;

pub use dose::{DoseMatch, DoseRule, OwnedDoseMatch, dose_check, dose_check_all};
pub use plan::{
    FALLBACK_SUMMARY, PlanDraft, PlanField, PlanRule, PlanValidation, RuleFailure,
    ValidationContext, step_to_draft, validate_plan,
};
pub use products::{BUILTIN_PRODUCTS, ProductCatalogue, ProductMention};
pub use routes::{
    Calibration, POISON_CONTROL_TEXT, REPLACED_SENTENCE, RouteDecision, SAFETY_POLICY,
    decide as decide_route, mentions_poisoning,
};
pub use sentences::{SentenceSplitter, sentence_spans, sentences};
