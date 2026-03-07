//! Provider implementations and runtime-level re-exports.
//!
//! Generic trait definitions live in react_core::providers.
//! Suite-specific traits live in their own suite crate.
//! Concrete implementations (keyspace, secrets, vector store) live here.

pub mod keyspace;
pub mod secrets;
pub mod vector;

pub use react_core::providers::{
    NullSecretsProvider, SecretsProvider, StateStore, VectorStore,
};
pub use react_core::scope::RequestScope;

pub use keyspace::{DefaultKeyspace, Keyspace, LocalKeyspace};
pub use secrets::EnvSecretsProvider;
pub use vector::LanceVectorStore;
