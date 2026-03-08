//! Generic capability-level provider traits.
//!
//! Suite-specific providers live in their respective suite crates.
//! Only truly generic providers remain here.

pub mod secrets;
pub mod state;
pub mod vector;

pub use secrets::{NullSecretsProvider, SecretsProvider};
pub use state::StateStore;
pub use vector::{ChunkKind, ScoredVectorChunk, VectorChunk, VectorStore};
