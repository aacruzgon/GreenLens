//! Building the question an ambiguous run asks ([contracts 3], `Question`; [contracts 7]:
//! one targeted question, or a request for a specific photo). The choice itself is
//! `knowledge::status::choose_question`; this module only words it.

use domain::{
    id::QuestionId,
    rules::questions::CONTINUE_AFTER,
    scan::{Answer, ImageKind, Question, QuestionKind, QuestionOption, QuestionStatus},
    wire::{Run, Timestamp},
};
use knowledge::status::QuestionChoice;

/// The photo hint for each kind of shot.
pub const fn photo_hint(kind: ImageKind) -> &'static str {
    match kind {
        ImageKind::WholePlant => "Step back and photograph the whole plant, top to bottom.",
        ImageKind::LeafCloseup => "Photograph one affected leaf up close, filling the frame.",
        ImageKind::LeafUnderside => "Turn the leaf over and photograph the underside",
        ImageKind::SoilSurface => "Photograph the soil around the base of the plant.",
        ImageKind::FlowerFruit => "Photograph a flower or fruit up close.",
        ImageKind::SqueezeTest | ImageKind::JarTest => "Follow the soil check's steps.",
    }
}

/// The question text of a photo request.
pub const fn photo_text(kind: ImageKind) -> &'static str {
    match kind {
        ImageKind::WholePlant => "Can you send a photo of the whole plant?",
        ImageKind::LeafUnderside => "Can you send a photo of the underside of an affected leaf?",
        ImageKind::SoilSurface => "Can you send a photo of the soil around the plant?",
        ImageKind::FlowerFruit => "Can you send a photo of a flower or fruit?",
        ImageKind::LeafCloseup | ImageKind::SqueezeTest | ImageKind::JarTest => {
            "Can you send a closer photo of an affected leaf?"
        }
    }
}

/// The question as the app sees it, `open`, continuing after 10 minutes.
pub fn build_question(
    choice: &QuestionChoice<'_>,
    id: QuestionId,
    run: Run,
    now: Timestamp,
) -> Question {
    let (kind, text, options, indicator_id, photo_kind, photo_hint_text, candidates) = match choice
    {
        QuestionChoice::Indicator {
            indicator,
            candidates,
        } => (
            QuestionKind::Indicator,
            indicator.question.clone(),
            Some(vec![
                QuestionOption {
                    id: Answer::Yes,
                    label: "Yes".to_owned(),
                },
                QuestionOption {
                    id: Answer::No,
                    label: "No".to_owned(),
                },
                QuestionOption {
                    id: Answer::Unsure,
                    label: "Not sure".to_owned(),
                },
            ]),
            indicator.id.slug().parse().ok(),
            None,
            None,
            candidates.clone(),
        ),
        QuestionChoice::Photo { kind, candidates } => (
            QuestionKind::PhotoRequest,
            photo_text(*kind).to_owned(),
            None,
            None,
            Some(*kind),
            Some(photo_hint(*kind).to_owned()),
            candidates.clone(),
        ),
    };
    Question {
        id,
        run,
        kind,
        text,
        options,
        indicator_id,
        photo_kind,
        photo_hint: photo_hint_text,
        candidates,
        asked_at: now,
        continue_at: now.plus(CONTINUE_AFTER),
        status: QuestionStatus::Open,
    }
}
