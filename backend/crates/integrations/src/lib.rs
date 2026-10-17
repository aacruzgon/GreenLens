//! Every outbound HTTP adapter of the backend: the LLM API, the OpenAI-compatible VLM servers,
//! Open-Meteo, `SoilGrids`, the geocoders, Clerk, Svix webhook verification, Expo Push, the
//! Jev classifier and `SurrealDB` Agent Memory. Each implements a `domain::ports` trait (or, for the two LLM clients, offers
//! a typed client that `agent` and the worker build their ports on) and maps its failures to
//! the port's error enum without exposing `reqwest` types.

pub mod agent_memory;
pub mod anthropic;
pub mod clerk;
pub mod expo_push;
pub mod geocode;
pub mod http;
pub mod jev;
pub mod open_meteo;
pub mod openai_compat;
pub mod soilgrids;
pub mod sse;
pub mod svix;
