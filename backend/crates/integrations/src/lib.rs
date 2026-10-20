//! Every outbound HTTP adapter of the backend: the LLM API, the OpenAI-compatible VLM servers,
//! Open-Meteo, `SoilGrids`, the geocoders, Clerk, Svix webhook verification, Expo Push, the
//! Jev classifier, `SurrealDB` Agent Memory, and the farm sources Slack and Google Drive. Each implements a `domain::ports` trait (or, for the two LLM clients, offers
//! a typed client that `agent` and the worker build their ports on) and maps its failures to
//! the port's error enum without exposing `reqwest` types.

pub mod agent_memory;
pub mod anthropic;
pub mod clerk;
#[cfg(feature = "era-sim")]
pub mod era_sim;
pub mod expo_push;
pub mod farm;
pub mod geocode;
pub mod google_drive;
pub mod http;
pub mod jev;
pub mod open_meteo;
pub mod openai_compat;
pub mod slack;
pub mod soilgrids;
pub mod sse;
pub mod svix;
