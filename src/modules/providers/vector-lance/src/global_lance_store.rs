use crate::lance_store::{Chunk, LanceDbStore, ScoredChunk};

// TODO(item-82): GlobalLanceDbStore largely duplicates LanceDbStore query logic.
// Consider consolidating by delegating `query()` to `LanceDbStore::query()` with
// scope=None, or extracting a shared query-result-parsing helper.
pub struct GlobalLanceDbStore {
    uri: String,
}

impl GlobalLanceDbStore {
    pub fn new(uri: String) -> Self {
        Self { uri }
    }

    pub async fn upsert(&self, items: &[Chunk]) -> Result<(), String> {
        LanceDbStore::new(&self.uri).upsert(items).await
    }

    pub async fn query(&self, query_vec: &[f32], k: usize) -> Result<Vec<ScoredChunk>, String> {
        LanceDbStore::new(&self.uri).query(query_vec, k, None).await
    }
}
