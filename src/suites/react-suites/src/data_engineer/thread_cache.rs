use dashmap::DashMap;
use once_cell::sync::OnceCell;
use std::time::Instant;

#[derive(Clone, Debug, Default)]
pub struct ThreadCache {
    pub published_relations: Vec<String>,
    pub published_manifest_sha256: Option<String>,
    pub updated_at: Option<Instant>,
}

static THREAD_CTX_CACHE: OnceCell<DashMap<String, ThreadCache>> = OnceCell::new();
fn ctx_cache() -> &'static DashMap<String, ThreadCache> {
    THREAD_CTX_CACHE.get_or_init(DashMap::new)
}

impl ThreadCache {
    pub fn ttl_fresh(&self, secs: u64) -> bool {
        match self.updated_at {
            Some(t) => t.elapsed().as_secs() < secs,
            None => false,
        }
    }
}

pub struct ThreadCacheStore;

impl ThreadCacheStore {
    pub fn get(thread_id: &str) -> Option<ThreadCache> {
        ctx_cache().get(thread_id).map(|c| c.clone())
    }

    pub fn update_published(thread_id: &str, manifest_sha256: &str, relations: Vec<String>) {
        let mut entry = ctx_cache()
            .get(thread_id)
            .map(|e| e.clone())
            .unwrap_or_default();
        entry.published_relations = relations;
        entry.published_manifest_sha256 = Some(manifest_sha256.to_string());
        entry.updated_at = Some(Instant::now());
        ctx_cache().insert(thread_id.to_string(), entry);
    }
}
