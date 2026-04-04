use react_core::provider_traits::{TypedVectorDocument, VectorCollection};
use serde::{Deserialize, Serialize};

pub struct AdminRepoCollection;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AdminRepoMetadata {
    pub repo_root: String,
    pub repo_label: String,
    pub path: String,
    pub chunk_index: usize,
    pub sha256: String,
}

impl VectorCollection for AdminRepoCollection {
    type Metadata = AdminRepoMetadata;

    const NAMESPACE: &'static str = "admin_repo";
}

pub type AdminRepoDocument = TypedVectorDocument<AdminRepoCollection>;
