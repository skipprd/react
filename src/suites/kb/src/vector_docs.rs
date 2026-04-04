use react_core::provider_traits::{TypedVectorDocument, VectorCollection};
use serde::{Deserialize, Serialize};

pub struct KbDocCollection;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct KbDocMetadata {
    pub dataset_id: String,
    pub path: String,
    pub chunk_index: usize,
}

impl VectorCollection for KbDocCollection {
    type Metadata = KbDocMetadata;

    const NAMESPACE: &'static str = "kb_doc";
}

pub type KbDocDocument = TypedVectorDocument<KbDocCollection>;
