use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarehouseKind {
    Athena,
    Postgres,
    Mssql,
    Snowflake,
    Bigquery,
}

impl fmt::Display for WarehouseKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Athena => write!(f, "athena"),
            Self::Postgres => write!(f, "postgres"),
            Self::Mssql => write!(f, "mssql"),
            Self::Snowflake => write!(f, "snowflake"),
            Self::Bigquery => write!(f, "bigquery"),
        }
    }
}

impl Default for WarehouseKind {
    fn default() -> Self {
        Self::Athena
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ProvidersResolved {
    pub warehouse: WarehouseResolved,
    pub catalog: CatalogResolved,
    pub dbt: DbtResolved,
    pub vector: VectorResolved,
}

#[derive(Clone, Debug, Serialize)]
pub struct WarehouseResolved {
    pub kind: WarehouseKind,
    pub container: String,
    pub namespace: String,
    pub extras: serde_json::Value,
}

impl Default for WarehouseResolved {
    fn default() -> Self {
        Self {
            kind: WarehouseKind::default(),
            container: String::new(),
            namespace: String::new(),
            extras: serde_json::Value::Null,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CatalogResolved {
    pub enabled: bool,
    #[serde(default)]
    pub refresh_secs: u64,
    #[serde(default)]
    pub max_concurrency: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VectorResolved {
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DbtNamingResolved {
    #[serde(default)]
    pub target_schema: String,
    #[serde(default)]
    pub silver_suffix: String,
    #[serde(default)]
    pub gold_suffix: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DbtResolved {
    pub enabled: bool,
    #[serde(default)]
    pub profiles_dir: Option<String>,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub naming: DbtNamingResolved,
    #[serde(default)]
    pub runner: String,
    #[serde(default)]
    pub docker_image: Option<String>,
    #[serde(default)]
    pub docker_platform: Option<String>,
    #[serde(default)]
    pub docker_network: Option<String>,
    #[serde(default)]
    pub docker_mount_aws_dir: bool,
}

/// Deserialize the data_engineer-specific config from the suite_config Value.
pub fn de_config_from_resolved(cfg: &react_core::resolved_config::ReactResolvedConfig) -> Option<ProvidersResolved> {
    serde_json::from_value::<ProvidersResolvedSerde>(cfg.suite_config.clone())
        .ok()
        .map(|s| s.into())
}

#[derive(Deserialize)]
struct ProvidersResolvedSerde {
    #[serde(default)]
    warehouse: WarehouseResolvedSerde,
    #[serde(default)]
    catalog: CatalogResolved,
    #[serde(default)]
    dbt: DbtResolved,
    #[serde(default)]
    vector: VectorResolved,
}

#[derive(Deserialize, Default)]
struct WarehouseResolvedSerde {
    #[serde(default)]
    kind: WarehouseKind,
    #[serde(default)]
    container: String,
    #[serde(default)]
    namespace: String,
    #[serde(default)]
    extras: serde_json::Value,
}

impl From<ProvidersResolvedSerde> for ProvidersResolved {
    fn from(s: ProvidersResolvedSerde) -> Self {
        Self {
            warehouse: WarehouseResolved {
                kind: s.warehouse.kind,
                container: s.warehouse.container,
                namespace: s.warehouse.namespace,
                extras: s.warehouse.extras,
            },
            catalog: s.catalog,
            dbt: s.dbt,
            vector: s.vector,
        }
    }
}
