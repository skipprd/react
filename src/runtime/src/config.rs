use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use crate::wiring::RequestScope;
use rc::{LlmProvider, StorageMode};
use react_core::resolved_config as rc;

/// # `react` configuration
///
/// `react serve` loads a **YAML** config file via `--config <PATH>`.
///
/// ## Precedence
/// - CLI flag (if provided)
/// - Environment variable (if set and non-empty)
/// - YAML config file
/// - Hardcoded default
///
/// ## Example YAML
///
/// ```yaml
/// version: 1
///
/// server:
///   port: 8787
///
/// storage:
///   # Local-first (default)
///   mode: local
///   path: ./.react
///   # For S3:
///   # mode: s3
///   # bucket: my-react-bucket
///
/// scope:
///   tenant: default
///   workspace: default
///   project_id: default
///
/// llm:
///   provider: OPENAI_COMPAT
///   base_url: https://api.openai.com
///   reason_model: gpt-5.1
///   embed_model: text-embedding-3-small
///   context_length: 4096
///   http_timeout_secs: 30
///   max_tokens: 1024
///   temperature: 0.2
///   top_p: 1.0
///
/// providers:
///   athena:
///     enabled: true
///     workgroup: my_wg
///     # Athena Data Catalog (Glue).
///     target_catalog: AwsDataCatalog
///     # Bronze/raw schema (Glue database) for discovery + dbt sources.
///     source_schema: raw
///     result_s3: s3://my-query-results/
///     discovery_cache_ttl_secs: 120
///   catalog:
///     enabled: true
///     refresh_secs: 60
///     max_concurrency: 8
///   dbt:
///     enabled: true
///     runner: host
///     target: athena
///     naming:
///       # dbt target.schema (base) and tier suffixes. Example schemas: raw=test_raw, silver=test_silver, gold=test_warehouse
///       target_schema: test
///       silver_suffix: silver
///       gold_suffix: gold
///   vector:
///     enabled: true
/// ```
///
/// Notes:
/// - Secrets remain env-driven (e.g. `LLM_API_KEY`).
/// - Local-first: `storage.mode` defaults to `local` (stores artifacts under `storage.path`).
/// - For S3: set `storage.mode: s3` and provide `storage.bucket` (or env `SKIPPR_S3_BUCKET` / CLI `--bucket`).

/// CLI overrides for `react serve`.
///
/// Any `Some` value takes precedence over env and file config.
#[derive(Clone, Debug, Default)]
pub struct ServeOverrides {
    pub port: Option<u16>,
    pub storage_mode: Option<String>,
    pub bucket: Option<String>,
    pub storage_path: Option<String>,
    pub tenant: Option<String>,
    pub workspace: Option<String>,
    pub project_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ReactConfigFile {
    pub version: Option<u32>,
    pub server: Option<ServerFile>,
    pub storage: Option<StorageFile>,
    pub scope: Option<ScopeFile>,
    pub llm: Option<LlmFile>,
    pub providers: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ServerFile {
    pub port: Option<u16>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct StorageFile {
    pub mode: Option<String>,
    pub bucket: Option<String>,
    pub path: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ScopeFile {
    pub tenant: Option<String>,
    pub workspace: Option<String>,
    pub project_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct LlmFile {
    pub provider: Option<String>,
    pub base_url: Option<String>,
    pub reason_model: Option<String>,
    pub task_model: Option<String>,
    pub embed_model: Option<String>,
    pub context_length: Option<usize>,
    pub gpu_layers: Option<usize>,

    pub http_timeout_secs: Option<u64>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
}

pub use rc::LlmResolved;
pub use rc::ReactResolvedConfig;
pub use rc::ServerResolved;
pub use rc::StorageResolved;

use crate::runtime_settings::getenv_nonempty;

fn ensure_safe_segment(name: &str, v: &str) -> Result<(), String> {
    let t = v.trim();
    if t.is_empty() {
        return Err(format!("scope.{} is empty", name));
    }
    if t.contains('/') || t.contains('\\') {
        return Err(format!("scope.{} must not contain path separators", name));
    }
    if t.contains("..") {
        return Err(format!("scope.{} must not contain '..'", name));
    }
    Ok(())
}

impl ReactConfigFile {
    pub fn load_yaml(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|e| format!("failed to read config file: {}", e))?;
        serde_yaml::from_slice::<Self>(&bytes)
            .map_err(|e| format!("failed to parse YAML config: {}", e))
    }
}

const DEFAULT_SERVER_PORT: u16 = 8787;
const DEFAULT_LOCAL_STORAGE_PATH: &str = "./.react";

/// Suite-specific provider resolution. Currently dispatches to data_engineer.
///
/// TODO: Replace this hard-coded dispatch with a proper suite registry lookup
/// (keyed by `suite_id` from the config file) so adding a new suite does not
/// require changing this function.
fn resolve_suite_providers(providers_yaml: serde_json::Value) -> Result<serde_json::Value, String> {
    react_suite_data_engineer::de_config::resolve_providers_from_yaml(providers_yaml)
}

pub fn resolve_config(
    file: ReactConfigFile,
    ov: ServeOverrides,
) -> Result<ReactResolvedConfig, String> {
    let server_port = ov
        .port
        .or_else(|| file.server.as_ref().and_then(|s| s.port))
        .unwrap_or(DEFAULT_SERVER_PORT);

    // Storage mode: CLI > env > YAML > default(local)
    let mode_str = ov
        .storage_mode
        .or_else(|| getenv_nonempty("REACT_STORAGE_MODE"))
        .or_else(|| file.storage.as_ref().and_then(|s| s.mode.clone()))
        .unwrap_or_else(|| "local".to_string());
    let mode = match mode_str.trim().to_ascii_lowercase().as_str() {
        "local" => StorageMode::Local,
        "s3" => StorageMode::S3,
        other => {
            return Err(format!(
                "unsupported storage.mode '{other}' (expected local|s3)"
            ))
        }
    };

    fn abs_path(p: &str) -> Result<String, String> {
        let t = p.trim();
        if t.is_empty() {
            return Err("empty storage.path".to_string());
        }
        let pb = PathBuf::from(t);
        if pb.is_absolute() {
            return Ok(pb.to_string_lossy().to_string());
        }
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        Ok(cwd.join(pb).to_string_lossy().to_string())
    }

    // Resolve storage fields based on mode.
    let (bucket, path) = if mode == StorageMode::S3 {
        // Bucket: CLI > env > YAML
        let b = ov
                .bucket
                .or_else(|| getenv_nonempty("SKIPPR_S3_BUCKET"))
                .or_else(|| file.storage.as_ref().and_then(|s| s.bucket.clone()))
                .ok_or_else(|| {
                    "missing storage bucket for s3 mode (set --bucket, env SKIPPR_S3_BUCKET, or storage.bucket in YAML)".to_string()
                })?;
        (Some(b), None)
    } else {
        // Path: CLI > env > YAML > default(./.react)
        let p = ov
            .storage_path
            .or_else(|| getenv_nonempty("REACT_STORAGE_PATH"))
            .or_else(|| file.storage.as_ref().and_then(|s| s.path.clone()))
            .unwrap_or_else(|| DEFAULT_LOCAL_STORAGE_PATH.to_string());
        (None, Some(abs_path(&p)?))
    };

    let tenant = ov
        .tenant
        .or_else(|| file.scope.as_ref().and_then(|s| s.tenant.clone()))
        .unwrap_or_else(|| "default".to_string());
    let workspace = ov
        .workspace
        .or_else(|| file.scope.as_ref().and_then(|s| s.workspace.clone()))
        .unwrap_or_else(|| "default".to_string());
    let project_id = ov
        .project_id
        .or_else(|| file.scope.as_ref().and_then(|s| s.project_id.clone()))
        .unwrap_or_else(|| "default".to_string());

    ensure_safe_segment("tenant", &tenant)?;
    ensure_safe_segment("workspace", &workspace)?;
    ensure_safe_segment("project_id", &project_id)?;

    // Providers – delegate to suite-specific resolver
    let providers_yaml = file.providers.unwrap_or(serde_json::json!({}));
    let providers = resolve_suite_providers(providers_yaml)?;

    // LLM env surface
    let llmf = file.llm.unwrap_or_default();
    let llm_provider_raw = getenv_nonempty("LLM_PROVIDER").or(llmf.provider);
    let provider = match llm_provider_raw {
        Some(raw) => raw.parse::<LlmProvider>().map_err(|e| e.to_string())?,
        None => LlmProvider::default(),
    };
    let llm = LlmResolved {
        provider,
        base_url: getenv_nonempty("LLM_BASE_URL").or(llmf.base_url),
        reason_model: getenv_nonempty("LLM_REASON_MODEL").or(llmf.reason_model),
        task_model: getenv_nonempty("LLM_TASK_MODEL").or(llmf.task_model),
        embed_model: getenv_nonempty("LLM_EMBED_MODEL").or(llmf.embed_model),
        context_length: getenv_nonempty("LLM_CONTEXT_LENGTH")
            .and_then(|v| v.parse::<usize>().ok())
            .or(llmf.context_length),
        gpu_layers: getenv_nonempty("LLM_GPU_LAYERS")
            .and_then(|v| v.parse::<usize>().ok())
            .or(llmf.gpu_layers),
        http_timeout_secs: getenv_nonempty("LLM_HTTP_TIMEOUT_SECS")
            .and_then(|v| v.parse::<u64>().ok())
            .or(llmf.http_timeout_secs),
        max_tokens: getenv_nonempty("LLM_MAX_TOKENS")
            .and_then(|v| v.parse::<u32>().ok())
            .or(llmf.max_tokens),
        temperature: getenv_nonempty("LLM_TEMPERATURE")
            .and_then(|v| v.parse::<f32>().ok())
            .or(llmf.temperature),
        top_p: getenv_nonempty("LLM_TOP_P")
            .and_then(|v| v.parse::<f32>().ok())
            .or(llmf.top_p),
    };

    let cfg = ReactResolvedConfig {
        server: ServerResolved { port: server_port },
        storage: StorageResolved {
            mode: mode.clone(),
            bucket: bucket.clone(),
            path: path.clone(),
        },
        scope: RequestScope::parse(tenant, workspace, project_id).map_err(|e| e.to_string())?,
        llm,
        suite_config: providers,
    };

    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use once_cell::sync::Lazy;
    use std::sync::Mutex;

    static ENV_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    const ALL_TEST_ENV_KEYS: &[&str] = &[
        "SKIPPR_S3_BUCKET",
        "REACT_STORAGE_MODE",
        "REACT_STORAGE_PATH",
        "LLM_PROVIDER",
        "LLM_REASON_MODEL",
        "LLM_TASK_MODEL",
        "LLM_BASE_URL",
        "LLM_EMBED_MODEL",
        "LLM_CONTEXT_LENGTH",
        "LLM_GPU_LAYERS",
        "LLM_HTTP_TIMEOUT_SECS",
        "LLM_MAX_TOKENS",
        "LLM_TEMPERATURE",
        "LLM_TOP_P",
        "DBT_TARGET",
        "DBT_SILVER_SUFFIX",
    ];

    fn save_env<'a>(keys: &'a [&'a str]) -> Vec<(&'a str, Option<String>)> {
        keys.iter().map(|k| (*k, std::env::var(k).ok())).collect()
    }

    fn restore_env(saved: &[(&str, Option<String>)]) {
        for (k, v) in saved {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
    }

    fn minimal_providers_json() -> serde_json::Value {
        serde_json::json!({
            "warehouse": { "kind": "postgres" }
        })
    }

    /// Runs the body under ENV_LOCK and guarantees env var cleanup regardless of panics.
    fn with_clean_env<F: FnOnce() + std::panic::UnwindSafe>(body: F) {
        let _g = ENV_LOCK.lock().unwrap();
        let saved = save_env(ALL_TEST_ENV_KEYS);
        let result = std::panic::catch_unwind(body);
        restore_env(&saved);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    fn clear_env(keys: &[&str]) {
        for k in keys {
            std::env::remove_var(k);
        }
    }

    #[test]
    fn resolve_bucket_precedence_cli_over_env_over_yaml() {
        with_clean_env(|| {
            clear_env(ALL_TEST_ENV_KEYS);
            std::env::set_var("SKIPPR_S3_BUCKET", "env-bucket");

            let file = ReactConfigFile {
                storage: Some(StorageFile {
                    mode: Some("s3".into()),
                    bucket: Some("yaml-bucket".into()),
                    path: None,
                }),
                providers: Some(minimal_providers_json()),
                ..Default::default()
            };
            let ov = ServeOverrides {
                storage_mode: Some("s3".into()),
                bucket: Some("cli-bucket".into()),
                ..Default::default()
            };

            let cfg = resolve_config(file, ov).expect("resolve");
            assert_eq!(cfg.storage.mode, StorageMode::S3);
            assert_eq!(cfg.storage.bucket, Some("cli-bucket".to_string()));
        });
    }

    #[test]
    fn resolve_bucket_errors_if_missing_everywhere() {
        with_clean_env(|| {
            clear_env(ALL_TEST_ENV_KEYS);
            let file = ReactConfigFile {
                providers: Some(minimal_providers_json()),
                ..Default::default()
            };
            let ov = ServeOverrides {
                storage_mode: Some("s3".into()),
                ..Default::default()
            };
            let err = resolve_config(file, ov).err().unwrap_or_default();
            assert!(err.contains("missing storage bucket for s3 mode"));
        });
    }

    #[test]
    fn resolve_rejects_unsafe_scope_segments() {
        with_clean_env(|| {
            clear_env(ALL_TEST_ENV_KEYS);
            let file = ReactConfigFile {
                storage: Some(StorageFile {
                    mode: Some("s3".into()),
                    bucket: Some("b".into()),
                    path: None,
                }),
                scope: Some(ScopeFile {
                    tenant: Some("a/b".into()),
                    workspace: Some("w".into()),
                    project_id: Some("p".into()),
                }),
                providers: Some(minimal_providers_json()),
                ..Default::default()
            };
            let err = resolve_config(file, ServeOverrides::default())
                .err()
                .unwrap_or_default();
            assert!(err.contains("must not contain path separators"));
        });
    }

    #[test]
    fn resolve_defaults_to_local_storage_without_bucket() {
        with_clean_env(|| {
            clear_env(ALL_TEST_ENV_KEYS);
            let file = ReactConfigFile {
                providers: Some(minimal_providers_json()),
                ..Default::default()
            };
            let cfg = resolve_config(file, ServeOverrides::default()).expect("resolve");
            assert_eq!(cfg.storage.mode, StorageMode::Local);
            assert!(cfg.storage.bucket.is_none());
            assert!(cfg.storage.path.as_ref().is_some());
        });
    }

    #[test]
    fn resolve_rejects_unknown_llm_provider() {
        with_clean_env(|| {
            clear_env(ALL_TEST_ENV_KEYS);
            let file = ReactConfigFile {
                llm: Some(LlmFile {
                    provider: Some("SOME_UNKNOWN_PROVIDER".to_string()),
                    ..Default::default()
                }),
                providers: Some(minimal_providers_json()),
                ..Default::default()
            };
            let err = resolve_config(file, ServeOverrides::default()).unwrap_err();
            assert!(err.contains("unsupported llm provider"));
        });
    }

    #[test]
    fn resolve_config_does_not_mutate_llm_or_dbt_env() {
        with_clean_env(|| {
            clear_env(ALL_TEST_ENV_KEYS);
            std::env::set_var("LLM_PROVIDER", "NULL");
            std::env::set_var("LLM_REASON_MODEL", "preset-reason-model");
            std::env::set_var("DBT_TARGET", "preset-target");
            std::env::set_var("DBT_SILVER_SUFFIX", "preset-silver");
            let file = ReactConfigFile {
                llm: Some(LlmFile {
                    provider: Some("OPENAI_COMPAT".to_string()),
                    reason_model: Some("gpt-5.1".to_string()),
                    ..Default::default()
                }),
                providers: Some(serde_json::json!({
                    "warehouse": { "kind": "postgres" },
                    "dbt": {
                        "target": "athena",
                        "naming": { "silver_suffix": "silver" }
                    }
                })),
                ..Default::default()
            };
            let _ = resolve_config(file, ServeOverrides::default()).expect("resolve");
            assert_eq!(std::env::var("LLM_PROVIDER").ok().as_deref(), Some("NULL"));
            assert_eq!(
                std::env::var("LLM_REASON_MODEL").ok().as_deref(),
                Some("preset-reason-model")
            );
            assert_eq!(
                std::env::var("DBT_TARGET").ok().as_deref(),
                Some("preset-target")
            );
            assert_eq!(
                std::env::var("DBT_SILVER_SUFFIX").ok().as_deref(),
                Some("preset-silver")
            );
        });
    }
}
