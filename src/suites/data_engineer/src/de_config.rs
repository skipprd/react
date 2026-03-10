use serde::{Deserialize, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------
// YAML-file structs (deserialized from the `providers:` section of config)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct ProvidersFile {
    pub warehouse: Option<WarehouseFile>,
    pub catalog: Option<CatalogFile>,
    pub dbt: Option<DbtFile>,
    pub vector: Option<VectorFile>,
}

/// Warehouse configuration for a single provider (source or target).
///
/// Keep secrets in env; only non-secret wiring here.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum WarehouseFile {
    Athena {
        workgroup: Option<String>,
        region: Option<String>,
        result_s3: Option<String>,
        max_concurrency: Option<usize>,
        catalog: Option<String>,
        schema: Option<String>,
        discovery_cache_ttl_secs: Option<u64>,
    },
    Postgres {
        database: Option<String>,
        schema: Option<String>,
    },
    Mssql {
        database: Option<String>,
        schema: Option<String>,
    },
    Snowflake {
        database: Option<String>,
        schema: Option<String>,
        warehouse: Option<String>,
        role: Option<String>,
    },
    Bigquery {
        project: Option<String>,
        dataset: Option<String>,
        location: Option<String>,
        max_concurrency: Option<usize>,
        discovery_cache_ttl_secs: Option<u64>,
    },
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct CatalogFile {
    pub enabled: Option<bool>,
    pub refresh_secs: Option<u64>,
    pub max_concurrency: Option<usize>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct DbtNamingFile {
    pub target_schema: Option<String>,
    pub silver_suffix: Option<String>,
    pub gold_suffix: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct DbtFile {
    pub enabled: Option<bool>,
    pub profiles_dir: Option<String>,
    pub target: Option<String>,
    pub naming: Option<DbtNamingFile>,
    pub runner: Option<String>,
    pub docker_image: Option<String>,
    pub docker_platform: Option<String>,
    pub docker_network: Option<String>,
    pub docker_mount_aws_dir: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct VectorFile {
    pub enabled: Option<bool>,
}

// ---------------------------------------------------------------------------
// resolve_providers_from_yaml – builds the normalised suite_config JSON
// ---------------------------------------------------------------------------

use super::env_util::{env_keys, getenv_nonempty};

fn resolve_warehouse(w: WarehouseFile) -> WarehouseResolved {
    match w {
        WarehouseFile::Athena {
            workgroup,
            region,
            result_s3,
            max_concurrency,
            catalog,
            schema,
            discovery_cache_ttl_secs,
        } => WarehouseResolved {
            kind: WarehouseKind::Athena,
            container: catalog.unwrap_or_else(|| "AwsDataCatalog".to_string()),
            namespace: schema.unwrap_or_default(),
            extras: serde_json::json!({
                "workgroup": workgroup,
                "region": region,
                "result_s3": result_s3,
                "max_concurrency": max_concurrency,
                "discovery_cache_ttl_secs": discovery_cache_ttl_secs,
            }),
        },
        WarehouseFile::Postgres { database, schema } => WarehouseResolved {
            kind: WarehouseKind::Postgres,
            container: database.unwrap_or_default(),
            namespace: schema.unwrap_or_default(),
            extras: serde_json::json!({}),
        },
        WarehouseFile::Mssql { database, schema } => WarehouseResolved {
            kind: WarehouseKind::Mssql,
            container: database.unwrap_or_default(),
            namespace: schema.unwrap_or_default(),
            extras: serde_json::json!({}),
        },
        WarehouseFile::Snowflake {
            database,
            schema,
            warehouse,
            role,
        } => WarehouseResolved {
            kind: WarehouseKind::Snowflake,
            container: database.unwrap_or_default(),
            namespace: schema.unwrap_or_default(),
            extras: serde_json::json!({ "warehouse": warehouse, "role": role }),
        },
        WarehouseFile::Bigquery {
            project,
            dataset,
            location,
            max_concurrency,
            discovery_cache_ttl_secs,
        } => WarehouseResolved {
            kind: WarehouseKind::Bigquery,
            container: project.unwrap_or_default(),
            namespace: dataset.unwrap_or_default(),
            extras: serde_json::json!({
                "location": location,
                "max_concurrency": max_concurrency,
                "discovery_cache_ttl_secs": discovery_cache_ttl_secs,
            }),
        },
    }
}

/// Resolve the raw YAML `providers:` value into the normalised suite_config JSON.
pub fn resolve_providers_from_yaml(
    providers_yaml: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let pf: ProvidersFile = serde_json::from_value(providers_yaml)
        .map_err(|e| format!("failed to parse providers config: {}", e))?;

    let wh_f = pf
        .warehouse
        .ok_or_else(|| "missing providers.warehouse in YAML config".to_string())?;
    let cat_f = pf.catalog.unwrap_or_default();
    let dbt_f = pf.dbt.unwrap_or_default();
    let vec_f = pf.vector.unwrap_or_default();

    let dbt_naming_f = dbt_f.naming.clone().unwrap_or_default();
    let naming_target_schema =
        getenv_nonempty(env_keys::DBT_TARGET_SCHEMA).or(dbt_naming_f.target_schema);
    let naming_silver_suffix = getenv_nonempty(env_keys::DBT_SILVER_SUFFIX)
        .or(dbt_naming_f.silver_suffix)
        .or(Some("silver".to_string()));
    let naming_gold_suffix = getenv_nonempty(env_keys::DBT_GOLD_SUFFIX)
        .or(dbt_naming_f.gold_suffix)
        .or(Some("warehouse".to_string()));

    let docker_mount_aws_dir = getenv_nonempty(env_keys::DBT_DOCKER_MOUNT_AWS_DIR)
        .map(|v| {
            let vv = v.trim().to_lowercase();
            vv == "1" || vv == "true" || vv == "yes"
        })
        .or(dbt_f.docker_mount_aws_dir)
        .unwrap_or(false);

    let warehouse_resolved = resolve_warehouse(wh_f);
    let providers = serde_json::json!({
        "warehouse": warehouse_resolved,
        "catalog": {
            "enabled": cat_f.enabled.unwrap_or(true),
            "refresh_secs": cat_f.refresh_secs.unwrap_or(60),
            "max_concurrency": cat_f.max_concurrency.unwrap_or(8),
        },
        "dbt": {
            "enabled": dbt_f.enabled.unwrap_or(true),
            "profiles_dir": getenv_nonempty(env_keys::DBT_PROFILES_DIR).or(dbt_f.profiles_dir),
            "target": getenv_nonempty(env_keys::DBT_TARGET)
                .or(dbt_f.target)
                .unwrap_or_default(),
            "naming": {
                "target_schema": naming_target_schema.unwrap_or_default(),
                "silver_suffix": naming_silver_suffix.unwrap_or_default(),
                "gold_suffix": naming_gold_suffix.unwrap_or_default(),
            },
            "runner": getenv_nonempty(env_keys::DBT_RUNNER)
                .or(dbt_f.runner)
                .unwrap_or_else(|| "host".to_string()),
            "docker_image": getenv_nonempty(env_keys::DBT_DOCKER_IMAGE).or(dbt_f.docker_image),
            "docker_platform": getenv_nonempty(env_keys::DBT_DOCKER_PLATFORM)
                .or(dbt_f.docker_platform),
            "docker_network": getenv_nonempty(env_keys::DBT_DOCKER_NETWORK).or(dbt_f.docker_network),
            "docker_mount_aws_dir": docker_mount_aws_dir,
        },
        "vector": {
            "enabled": vec_f.enabled.unwrap_or(true),
        },
    });

    Ok(providers)
}

// ---------------------------------------------------------------------------
// Resolved types (runtime representation after config is loaded)
// ---------------------------------------------------------------------------

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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProvidersResolved {
    #[serde(default)]
    pub warehouse: WarehouseResolved,
    #[serde(default)]
    pub catalog: CatalogResolved,
    #[serde(default)]
    pub dbt: DbtResolved,
    #[serde(default)]
    pub vector: VectorResolved,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WarehouseResolved {
    #[serde(default)]
    pub kind: WarehouseKind,
    #[serde(default)]
    pub container: String,
    #[serde(default)]
    pub namespace: String,
    #[serde(default)]
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
pub fn de_config_from_resolved(
    cfg: &react_core::resolved_config::ReactResolvedConfig,
) -> Option<ProvidersResolved> {
    serde_json::from_value::<ProvidersResolved>(cfg.suite_config.clone()).ok()
}
