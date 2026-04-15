//! Generic capability-level provider traits.
//!
//! Suite-specific providers live in their respective suite crates.
//! Only truly generic providers remain here.

pub mod secrets;
pub mod state;
pub mod vector;

pub use secrets::{NullSecretsProvider, SecretsProvider};
pub use state::StateStore;
pub use vector::{
    delete_collection, query_typed_documents, upsert_typed_documents, ScoredTypedVectorDocument,
    ScoredVectorRecord, StoredVectorRecord, TypedVectorDocument, VectorCollection, VectorStore,
};
