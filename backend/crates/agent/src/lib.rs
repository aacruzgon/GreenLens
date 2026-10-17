//! The agent core (architecture 7): the chat turn, its tools, the router and safety gate, the
//! plan writer, memory extraction and the model adapters. Runs inside `api`; `pipeline` calls the
//! plan writer. Every boundary it crosses is a `domain` port.

pub mod beliefs;
pub mod config;
pub mod context;
pub mod deps;
pub mod error;
pub mod memory;
pub mod model;
pub mod plan_writer;
pub mod prompt;
pub mod safety;
pub mod tools;
pub mod turn;

pub use beliefs::MirroredMemory;
pub use config::AgentConfig;
pub use deps::{AgentDeps, AgentStores};
pub use error::{
    BeliefSyncError, MemoryError, PlanWriterError, PrivateMemory, ToolError, TurnError,
};
pub use plan_writer::{PlanInput, PlanWriter, SharedMemories, WrittenPlan};
pub use turn::{ChatAgent, TurnOutcome, TurnRequest, TurnStream};

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod testing;
