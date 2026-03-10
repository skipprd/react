use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    NoFailure,
    InfraTransient,
    WarehouseConfig,
    MissingSource,
    Schema,
    SqlRuntime,
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
        matches!(self, Self::Schema | Self::SqlRuntime)
    }

    fn severity(self) -> u8 {
        match self {
            Self::WarehouseConfig => 5,
            Self::InfraTransient => 4,
            Self::SqlRuntime => 3,
            Self::Schema => 2,
            Self::Unknown => 1,
            Self::MissingSource | Self::NoFailure => 0,
        }
    }

    pub fn merge(self, other: Self) -> Self {
        if other.severity() > self.severity() { other } else { self }
    }
}
