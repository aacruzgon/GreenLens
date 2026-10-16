//! The PDM `knowledge` crate: the curated knowledge base as the pipeline, the agent and `api`
//! read it ([architecture 5, knowledge base]), and the pure rules of retrieve-then-verify
//! ([architecture 6.3], [contracts 7, diagnosis]).
//!
//! - [`SurrealKnowledgeStore`] implements `domain::ports::KnowledgeStore`. It reads every
//!   curated record once through `KnowledgeRepo::load_knowledge` into an in-memory
//!   [`KnowledgeIndex`] (a few hundred records that change only with a migration) and answers
//!   lookups from memory; chunk search goes to the HNSW index ([database 5]).
//! - [`candidates`]: the pre-VLM candidate set, about 9 profiles.
//! - [`weather_fit`]: the weather against a profile's favourable conditions.
//! - [`scoring`]: matched minus contradicted weights, one answer per indicator.
//! - [`status`]: `healthy`, `confirmed`, `uncertain`, `low_evidence`, and the question to ask.
//!
//! The rules borrow the profiles (`&'k ProblemSeed`): they run synchronously inside one step,
//! over records the step already holds, so nothing is cloned on the hot path.
//!
//! [architecture 5, knowledge base]: https://github.com/aacruzgon/GreenLens/blob/main/docs/architecture/README.md#knowledge-base
//! [architecture 6.3]: https://github.com/aacruzgon/GreenLens/blob/main/docs/architecture/README.md#63-retrieve-then-verify
//! [contracts 7, diagnosis]: https://github.com/aacruzgon/GreenLens/blob/main/docs/architecture/contracts.md#diagnosis
//! [database 5]: https://github.com/aacruzgon/GreenLens/blob/main/docs/database/README.md#5-reads-indexes-and-vector-search

pub mod candidates;
mod error;
mod index;
pub mod scoring;
pub mod status;
mod store;
pub mod weather_fit;

pub use error::IndexError;
pub use index::KnowledgeIndex;
pub use store::SurrealKnowledgeStore;

#[cfg(test)]
pub(crate) mod fixtures;
