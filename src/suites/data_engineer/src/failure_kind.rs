use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    InfraTransient,
    #[serde(other)]
    Unknown,
}

impl Default for FailureKind {
    fn default() -> Self {
        Self::Unknown
    }
}

impl FailureKind {
    pub fn is_transient(self) -> bool {
        matches!(self, Self::InfraTransient)
    }

    pub fn is_repairable(self) -> bool {
        !self.is_transient()
    }
}
