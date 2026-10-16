//! Which stages a run lists and repeats ([contracts 3], species change; [contracts 4],
//! `run_started.rerun_stages` and the `measurement` stage).

use crate::{events::Stage, jobs::PerceptionStage};

/// What the earlier run finished, for a species-change run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EarlierRun {
    /// Plant check and segmentation both finished.
    pub perception_base_done: bool,
    /// Measurement finished (or wasn't needed).
    pub measured: bool,
}

/// The stages a run lists in `run_started.rerun_stages`, in pipeline order.
///
/// - Submit and resubmit: every stage; `measurement` only when a photo has depth.
/// - Species change, when the earlier run finished plant check and segmentation: species,
///   disease, the VLM stages and scoring (with fusion and plan), keeping plant check,
///   segmentation, image embeddings and measurements; measuring again only if the earlier run
///   hadn't finished it. Otherwise every perception stage and measurement again.
/// - With "something else" (`custom_species`), species identification is skipped.
pub fn run_stages(
    species_change: Option<EarlierRun>,
    has_depth: bool,
    custom_species: bool,
) -> Vec<Stage> {
    let keep_base = species_change.is_some_and(|e| e.perception_base_done);
    let measure =
        has_depth && species_change.is_none_or(|e| !e.perception_base_done || !e.measured);
    Stage::ALL
        .into_iter()
        .filter(|stage| match stage {
            Stage::PlantCheck | Stage::Segmentation | Stage::ImageEmbedding => !keep_base,
            Stage::Species => !custom_species,
            Stage::Measurement => measure,
            Stage::Guided => false,
            _ => true,
        })
        .filter(|stage| !matches!(stage, Stage::Verify))
        .collect()
}

/// The perception stages the run's `perception` job asks for.
pub fn perception_stages(stages: &[Stage]) -> Vec<PerceptionStage> {
    PerceptionStage::ALL
        .into_iter()
        .filter(|p| stages.contains(&p.stage()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_lists_every_run_stage() {
        let s = run_stages(None, false, false);
        assert!(!s.contains(&Stage::Measurement));
        assert!(!s.contains(&Stage::Guided));
        assert!(!s.contains(&Stage::Verify));
        assert!(s.contains(&Stage::PlantCheck));
        assert!(run_stages(None, true, false).contains(&Stage::Measurement));
    }

    #[test]
    fn species_change_keeps_finished_perception() {
        let done = EarlierRun {
            perception_base_done: true,
            measured: true,
        };
        let s = run_stages(Some(done), true, false);
        assert!(!s.contains(&Stage::PlantCheck));
        assert!(!s.contains(&Stage::Segmentation));
        assert!(!s.contains(&Stage::ImageEmbedding));
        assert!(!s.contains(&Stage::Measurement));
        assert!(s.contains(&Stage::Species) && s.contains(&Stage::Disease));
        assert_eq!(
            perception_stages(&s),
            vec![PerceptionStage::Species, PerceptionStage::Disease]
        );

        let unmeasured = EarlierRun {
            perception_base_done: true,
            measured: false,
        };
        assert!(run_stages(Some(unmeasured), true, false).contains(&Stage::Measurement));
        let custom = run_stages(Some(done), false, true);
        assert!(!custom.contains(&Stage::Species));
        let unfinished = run_stages(Some(EarlierRun::default()), false, false);
        assert!(unfinished.contains(&Stage::PlantCheck));
    }
}
