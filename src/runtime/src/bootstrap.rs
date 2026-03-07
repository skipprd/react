use std::sync::Arc;

use react_core::resolved_config as rc;
use react_core::suite::SuiteCtx;
use react_module_provider_athena::{AthenaQueryProvider, AthenaSettings};
use react_module_provider_bigquery::{BigQueryProvider, BigQuerySettings};
use react_module_provider_catalog::DefaultCatalogProvider;
use react_module_provider_dbt::{DbtProjectProvider, DbtRunnerConfig, DbtRunnerMode};
use react_module_provider_postgres::{PostgresProvider, PostgresSettings};
use react_module_storage::{LocalFileStorageAdapter, S3StorageAdapter};
use react_suites::data_engineer::ctx_ext::{
    CatalogCap, DatasetsCap, DbtCap, ProvidersCfgCap, QueryCap, WarehouseCap,
};
use react_suites::data_engineer::de_config::{self as de_cfg, WarehouseKind};

use crate::providers::{DefaultKeyspace, EnvSecretsProvider, LanceVectorStore, LocalKeyspace};

pub fn getenv_nonempty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn env_usize(key: &str) -> Option<usize> {
    getenv_nonempty(key).and_then(|v| v.parse::<usize>().ok())
}

fn env_u64(key: &str) -> Option<u64> {
    getenv_nonempty(key).and_then(|v| v.parse::<u64>().ok())
}

fn nonempty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() { None } else { Some(t.to_string()) }
}

fn apply_aws_region_fallback_from_warehouse(warehouse_extras: &serde_json::Value) {
    if let Some(region) = warehouse_extras
        .get("region")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let has_default = std::env::var("AWS_DEFAULT_REGION")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .is_some();
        let has_region = std::env::var("AWS_REGION")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .is_some();
        if !has_default {
            std::env::set_var("AWS_DEFAULT_REGION", region);
        }
        if !has_region {
            std::env::set_var("AWS_REGION", region);
        }
    }
}

fn resolve_athena_settings(providers: &de_cfg::ProvidersResolved) -> AthenaSettings {
    let extras = &providers.warehouse.extras;
    let workgroup = getenv_nonempty("ATHENA_WORKGROUP")
        .or_else(|| {
            extras
                .get("workgroup")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });

    let result_output_location = getenv_nonempty("ATHENA_RESULT_S3")
        .or_else(|| {
            extras
                .get("result_s3")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });

    let default_catalog = getenv_nonempty("ATHENA_TARGET_CATALOG")
        .or_else(|| Some(providers.warehouse.container.clone()).filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "AwsDataCatalog".to_string());

    let source_schema = getenv_nonempty("ATHENA_SOURCE_SCHEMA")
        .or_else(|| Some(providers.warehouse.namespace.clone()).filter(|s| !s.is_empty()));

    let max_concurrency = env_usize("ATHENA_MAX_CONCURRENCY")
        .or_else(|| {
            extras
                .get("max_concurrency")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
        })
        .unwrap_or(15);

    let discovery_cache_ttl_secs = env_u64("ATHENA_DISCOVERY_CACHE_TTL_SECS")
        .or_else(|| {
            extras
                .get("discovery_cache_ttl_secs")
                .and_then(|v| v.as_u64())
        })
        .unwrap_or(120);

    AthenaSettings {
        workgroup,
        result_output_location,
        default_catalog,
        source_schema,
        max_concurrency,
        discovery_cache_ttl_secs,
    }
}

pub async fn build_suite_ctx(cfg: &rc::ReactResolvedConfig) -> SuiteCtx {
    let (storage, keyspace, lance_uri_prefix) = if cfg.storage.mode == rc::StorageMode::Local {
        let root = cfg
            .storage
            .path
            .clone()
            .unwrap_or_else(|| {
                eprintln!("ERROR: missing storage.path for local mode");
                std::process::exit(1);
            });
        let storage = match LocalFileStorageAdapter::new(root.clone()) {
            Ok(s) => Arc::new(s) as Arc<dyn react_core::storage::StorageAdapter>,
            Err(e) => {
                eprintln!("ERROR: {}", e);
                std::process::exit(1);
            }
        };
        let lance_prefix = format!("file://{}", root);
        let keyspace = Arc::new(LocalKeyspace::new(root));
        (
            storage,
            keyspace as Arc<dyn crate::providers::Keyspace>,
            lance_prefix,
        )
    } else {
        let b = cfg
            .storage
            .bucket
            .clone()
            .unwrap_or_else(|| {
                eprintln!("ERROR: missing storage.bucket for s3 mode");
                std::process::exit(1);
            });
        let storage = Arc::new(S3StorageAdapter::from_env(b.clone()).await)
            as Arc<dyn react_core::storage::StorageAdapter>;
        let lance_prefix = format!("s3://{}", b);
        let keyspace = Arc::new(DefaultKeyspace::new(b.clone()));
        (storage, keyspace as Arc<dyn crate::providers::Keyspace>, lance_prefix)
    };

    let secrets = Arc::new(EnvSecretsProvider::default());
    let llm = crate::llm::create_llm(&crate::llm::config_from_resolved(cfg));

    let mut suite_ctx =
        SuiteCtx::new(storage, secrets, llm, cfg.scope.clone(), keyspace.clone());
    suite_ctx.resolved_config = Some(Arc::new(cfg.clone()));

    let providers = de_cfg::de_config_from_resolved(cfg).unwrap_or_default();
    let wh_kind = providers.warehouse.kind;
    if wh_kind == WarehouseKind::Athena {
        apply_aws_region_fallback_from_warehouse(&providers.warehouse.extras);
        let athena = Arc::new(
            AthenaQueryProvider::from_settings(resolve_athena_settings(&providers)).await,
        );
        suite_ctx.set_capability(Arc::new(WarehouseCap(athena.clone())));
        suite_ctx.set_capability(Arc::new(QueryCap(athena.clone())));
        suite_ctx.set_capability(Arc::new(DatasetsCap(athena.clone())));
    } else if wh_kind == WarehouseKind::Postgres {
        let dbname = nonempty(&providers.warehouse.container);
        let default_schema = nonempty(&providers.warehouse.namespace);
        let pg = Arc::new(PostgresProvider::from_settings(PostgresSettings {
            dbname,
            default_schema,
            ..Default::default()
        }));
        suite_ctx.set_capability(Arc::new(WarehouseCap(pg.clone())));
        suite_ctx.set_capability(Arc::new(QueryCap(pg.clone())));
        suite_ctx.set_capability(Arc::new(DatasetsCap(pg.clone())));
    } else if wh_kind == WarehouseKind::Bigquery {
        let project = nonempty(&providers.warehouse.container);
        let dataset = nonempty(&providers.warehouse.namespace);
        let location = providers
            .warehouse
            .extras
            .get("location")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let max_conc = providers
            .warehouse
            .extras
            .get("max_concurrency")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(15);
        let ttl_secs = providers
            .warehouse
            .extras
            .get("discovery_cache_ttl_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(120);
        let bq = Arc::new(
            BigQueryProvider::from_settings(BigQuerySettings {
                project,
                dataset,
                location,
                max_concurrency: max_conc,
                discovery_cache_ttl_secs: ttl_secs,
            })
            .await
            .map_err(|e| {
                eprintln!("ERROR: BigQuery provider init failed: {}", e);
                std::process::exit(1);
            })
            .unwrap(),
        );
        suite_ctx.set_capability(Arc::new(WarehouseCap(bq.clone())));
        suite_ctx.set_capability(Arc::new(QueryCap(bq.clone())));
        suite_ctx.set_capability(Arc::new(DatasetsCap(bq.clone())));
    } else {
        eprintln!(
            "ERROR: unsupported providers.warehouse.kind '{}'",
            providers.warehouse.kind
        );
        std::process::exit(1);
    }

    if providers.catalog.enabled {
        let cat = Arc::new(DefaultCatalogProvider::new(
            suite_ctx.storage.clone(),
            keyspace.clone(),
            suite_ctx.llm.clone(),
            30,
            6,
        ).with_thread_id_fn(Arc::new(crate::llm::thread_ctx::current_thread_id)));
        suite_ctx.set_capability(Arc::new(CatalogCap(cat)));
    }

    if providers.vector.enabled {
        suite_ctx.vector = Some(Arc::new(LanceVectorStore::new(
            keyspace.clone(),
            cfg.scope.clone(),
            lance_uri_prefix,
        )));
    }

    if providers.dbt.enabled {
        let runner_mode = match DbtRunnerMode::parse(&providers.dbt.runner) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("ERROR: {}", e);
                std::process::exit(1);
            }
        };
        let runner = DbtRunnerConfig {
            mode: runner_mode,
            docker_image: providers.dbt.docker_image.clone(),
            docker_platform: providers.dbt.docker_platform.clone(),
            docker_network: providers.dbt.docker_network.clone(),
            docker_mount_aws_dir: providers.dbt.docker_mount_aws_dir,
        };
        suite_ctx.set_capability(Arc::new(DbtCap(Arc::new(DbtProjectProvider::new(
            suite_ctx.storage.clone(),
            keyspace.clone(),
            runner,
        )))));
    }

    suite_ctx.set_capability(Arc::new(ProvidersCfgCap(providers)));

    suite_ctx
}
