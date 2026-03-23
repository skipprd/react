use serde::{Deserialize, Serialize};
use std::path::Path;

/// Public `skippr-dbt` config schema.
///
/// This is the only config surface exposed to product users.
/// It maps to the internal runtime config shape silently.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SkipprDbtConfig {
    pub project: String,

    #[serde(default)]
    pub warehouse: Option<WarehouseConfig>,

    #[serde(default)]
    pub source: Option<SourceConfig>,

    #[serde(default)]
    pub dbt: Option<DbtConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WarehouseConfig {
    Snowflake {
        #[serde(default)]
        database: Option<String>,
        #[serde(default)]
        schema: Option<String>,
        #[serde(default)]
        warehouse: Option<String>,
        #[serde(default)]
        role: Option<String>,
    },
    Bigquery {
        #[serde(default)]
        project: Option<String>,
        #[serde(default)]
        dataset: Option<String>,
        #[serde(default)]
        location: Option<String>,
    },
    Postgres {
        #[serde(default)]
        database: Option<String>,
        #[serde(default)]
        schema: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceConfig {
    Mssql {
        #[serde(default)]
        connection_string: Option<String>,
    },
    S3 {
        #[serde(default)]
        s3_bucket: Option<String>,
        #[serde(default)]
        s3_prefix: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transform: Option<S3Transform>,
    },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct S3Transform {
    #[serde(default)]
    pub namespace_fields: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DbtConfig {
    #[serde(default)]
    pub target_schema: Option<String>,
    #[serde(default)]
    pub silver_suffix: Option<String>,
    #[serde(default)]
    pub gold_suffix: Option<String>,
}

impl SkipprDbtConfig {
    pub fn load_from(path: &Path) -> Result<Self, String> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
        serde_yaml::from_slice::<Self>(&bytes)
            .map_err(|e| format!("failed to parse {}: {}", path.display(), e))
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        let yaml = serde_yaml::to_string(self)
            .map_err(|e| format!("failed to serialize config: {}", e))?;
        std::fs::write(path, yaml.as_bytes())
            .map_err(|e| format!("failed to write {}: {}", path.display(), e))
    }

    pub fn warehouse_kind_str(&self) -> Option<&'static str> {
        match &self.warehouse {
            Some(WarehouseConfig::Snowflake { .. }) => Some("snowflake"),
            Some(WarehouseConfig::Bigquery { .. }) => Some("bigquery"),
            Some(WarehouseConfig::Postgres { .. }) => Some("postgres"),
            None => None,
        }
    }

    pub fn source_kind_str(&self) -> Option<&'static str> {
        match &self.source {
            Some(SourceConfig::Mssql { .. }) => Some("mssql"),
            Some(SourceConfig::S3 { .. }) => Some("s3"),
            None => None,
        }
    }

    pub fn uses_postgres(&self) -> bool {
        matches!(&self.warehouse, Some(WarehouseConfig::Postgres { .. }))
    }
}

impl WarehouseConfig {
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::Snowflake { .. } => "snowflake",
            Self::Bigquery { .. } => "bigquery",
            Self::Postgres { .. } => "postgres",
        }
    }
}
