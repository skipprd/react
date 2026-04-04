use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::marker::PhantomData;

use crate::scope::RequestScope;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StoredVectorRecord {
    pub id: String,
    pub namespace: String,
    pub text: String,
    pub vector: Vec<f32>,
    pub metadata_json: String,
    pub epoch: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScoredVectorRecord {
    pub item: StoredVectorRecord,
    pub score: f32,
}

pub trait VectorCollection: Send + Sync + 'static {
    type Metadata: Clone + std::fmt::Debug + PartialEq + Send + Sync + Serialize + DeserializeOwned + 'static;

    const NAMESPACE: &'static str;
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypedVectorDocument<C: VectorCollection> {
    id: String,
    text: String,
    vector: Vec<f32>,
    epoch: u64,
    metadata: C::Metadata,
    _collection: PhantomData<C>,
}

impl<C: VectorCollection> TypedVectorDocument<C> {
    pub fn new(
        id: impl Into<String>,
        text: impl Into<String>,
        vector: Vec<f32>,
        epoch: u64,
        metadata: C::Metadata,
    ) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            vector,
            epoch,
            metadata,
            _collection: PhantomData,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn vector(&self) -> &[f32] {
        &self.vector
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn metadata(&self) -> &C::Metadata {
        &self.metadata
    }

    pub fn into_record(&self) -> Result<StoredVectorRecord, String> {
        Ok(StoredVectorRecord {
            id: self.id.clone(),
            namespace: C::NAMESPACE.to_string(),
            text: self.text.clone(),
            vector: self.vector.clone(),
            metadata_json: serde_json::to_string(&self.metadata).map_err(|e| e.to_string())?,
            epoch: self.epoch,
        })
    }

    pub fn from_record(record: StoredVectorRecord) -> Result<Self, String> {
        if record.namespace != C::NAMESPACE {
            return Err(format!(
                "namespace mismatch: expected '{}' but found '{}'",
                C::NAMESPACE,
                record.namespace
            ));
        }
        let metadata =
            serde_json::from_str::<C::Metadata>(&record.metadata_json).map_err(|e| e.to_string())?;
        Ok(Self::new(
            record.id,
            record.text,
            record.vector,
            record.epoch,
            metadata,
        ))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScoredTypedVectorDocument<C: VectorCollection> {
    pub item: TypedVectorDocument<C>,
    pub score: f32,
}

pub async fn upsert_typed_documents<C: VectorCollection>(
    store: &dyn VectorStore,
    scope: &RequestScope,
    docs: &[TypedVectorDocument<C>],
) -> Result<(), String> {
    let mut records = Vec::with_capacity(docs.len());
    for doc in docs {
        records.push(doc.into_record()?);
    }
    store.upsert(scope, &records).await
}

pub async fn query_typed_documents<C: VectorCollection>(
    store: &dyn VectorStore,
    scope: &RequestScope,
    query_vec: &[f32],
    k: usize,
) -> Result<Vec<ScoredTypedVectorDocument<C>>, String> {
    let hits = store.query(scope, query_vec, k, Some(C::NAMESPACE)).await?;
    let mut out = Vec::with_capacity(hits.len());
    for hit in hits {
        let item = TypedVectorDocument::<C>::from_record(hit.item)?;
        out.push(ScoredTypedVectorDocument {
            item,
            score: hit.score,
        });
    }
    Ok(out)
}

pub async fn delete_collection<C: VectorCollection>(
    store: &dyn VectorStore,
    scope: &RequestScope,
) -> Result<(), String> {
    store.delete_namespace(scope, C::NAMESPACE).await
}

/// Vector store is optional but often shared across multiple suite tools.
#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn upsert(&self, scope: &RequestScope, items: &[StoredVectorRecord]) -> Result<(), String>;
    async fn query(
        &self,
        scope: &RequestScope,
        query_vec: &[f32],
        k: usize,
        namespace: Option<&str>,
    ) -> Result<Vec<ScoredVectorRecord>, String>;
    async fn delete_thread_embeddings(
        &self,
        scope: &RequestScope,
        thread_id: &str,
    ) -> Result<(), String>;
    async fn delete_project_embeddings(&self, scope: &RequestScope) -> Result<(), String>;
    async fn delete_namespace(&self, scope: &RequestScope, namespace: &str) -> Result<(), String>;
    async fn delete_ids_with_prefix(&self, scope: &RequestScope, prefix: &str) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCollection;

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    struct TestMetadata {
        label: String,
    }

    impl VectorCollection for TestCollection {
        type Metadata = TestMetadata;

        const NAMESPACE: &'static str = "test_collection";
    }

    #[test]
    fn typed_document_round_trips_through_stored_record() {
        let doc = TypedVectorDocument::<TestCollection>::new(
            "doc-1",
            "hello",
            vec![0.1, 0.2],
            42,
            TestMetadata {
                label: "example".to_string(),
            },
        );
        let record = doc.into_record().expect("record");
        assert_eq!(record.namespace, "test_collection");

        let decoded = TypedVectorDocument::<TestCollection>::from_record(record).expect("decode");
        assert_eq!(decoded.id(), "doc-1");
        assert_eq!(decoded.text(), "hello");
        assert_eq!(decoded.vector(), &[0.1, 0.2]);
        assert_eq!(
            decoded.metadata(),
            &TestMetadata {
                label: "example".to_string()
            }
        );
    }
}
