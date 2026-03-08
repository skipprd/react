use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::scope::RequestScope;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChunkKind {
    Entity,
    Field,
    Doc,
    Other(String),
}

impl ChunkKind {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Entity => "entity",
            Self::Field => "field",
            Self::Doc => "doc",
            Self::Other(s) => s.as_str(),
        }
    }
}

impl std::fmt::Display for ChunkKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for ChunkKind {
    fn from(s: &str) -> Self {
        match s {
            "entity" => Self::Entity,
            "field" => Self::Field,
            "doc" => Self::Doc,
            other => Self::Other(other.to_string()),
        }
    }
}

impl From<String> for ChunkKind {
    fn from(s: String) -> Self {
        match s.as_str() {
            "entity" => Self::Entity,
            "field" => Self::Field,
            "doc" => Self::Doc,
            _ => Self::Other(s),
        }
    }
}

impl Serialize for ChunkKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ChunkKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(ChunkKind::from(s))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VectorChunk {
    pub id: String,
    pub kind: ChunkKind,
    pub entity_id: String,
    pub field: Option<String>,
    pub text: String,
    pub vector: Vec<f32>,
    pub meta: Value,
    pub epoch: u64,
}

#[derive(Clone, Debug)]
pub struct ScoredVectorChunk {
    pub item: VectorChunk,
    pub score: f32,
}

/// Vector store is optional but often shared across multiple suite tools.
#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn upsert(&self, scope: &RequestScope, items: &[VectorChunk]) -> Result<(), String>;
    async fn query(
        &self,
        scope: &RequestScope,
        query_vec: &[f32],
        k: usize,
        scope_filter: Option<&str>,
    ) -> Result<Vec<ScoredVectorChunk>, String>;
    async fn delete_thread_embeddings(
        &self,
        scope: &RequestScope,
        thread_id: &str,
    ) -> Result<(), String>;
    async fn delete_project_embeddings(&self, scope: &RequestScope) -> Result<(), String>;
}
