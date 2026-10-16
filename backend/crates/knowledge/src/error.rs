//! The crate's own error: a knowledge snapshot that breaks the loader's checks
//! ([contracts 10], checks and generated files). The loader already refuses such data, so this
//! only fires when the database was edited by hand; `api` refuses to start on it.

use domain::{
    id::{OrganismId, ProblemId, SpeciesId},
    ports::StoreError,
};

/// Why a [`crate::KnowledgeIndex`] could not be built.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IndexError {
    /// An id is used twice in one table.
    #[error("duplicate id {0}")]
    Duplicate(String),
    /// A problem doesn't have exactly 3 discriminating indicators.
    #[error("problem {problem} has {count} discriminating indicators, not 3")]
    Discriminating {
        /// The problem.
        problem: ProblemId,
        /// How many it has.
        count: usize,
    },
    /// A problem's `affects` names an unknown species.
    #[error("problem {problem} affects unknown species {species}")]
    UnknownSpecies {
        /// The problem.
        problem: ProblemId,
        /// The species.
        species: SpeciesId,
    },
    /// A `caused_by` or `preys_on` entry names an unknown organism.
    #[error("unknown organism {0}")]
    UnknownOrganism(OrganismId),
    /// A `looks_like` edge names an unknown problem.
    #[error("unknown problem {0}")]
    UnknownProblem(ProblemId),
}

impl From<IndexError> for StoreError {
    fn from(e: IndexError) -> Self {
        Self::Query {
            what: "knowledge index",
            source: Box::new(e),
        }
    }
}
