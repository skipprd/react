//! Generic capability-level provider traits.
//!
//! Domain-specific providers (warehouse, catalog, dbt, query) live in their respective suites.
//! Only truly generic providers remain here.

pub mod secrets;
pub mod state;
pub mod vector;

pub use secrets::{NullSecretsProvider, SecretsProvider};
pub use state::StateStore;
pub use vector::{ScoredVectorChunk, VectorChunk, VectorStore};
