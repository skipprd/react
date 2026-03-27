use react::config::{LlmFile, ReactConfigFile, ScopeFile, StorageFile};
use react_core::resolved_config::S3Credentials;

use crate::public_config::{SkipprDbtConfig, SourceConfig, WarehouseConfig};

/// Translate the public `skippr` config into the internal runtime config.
pub fn to_internal(cfg: &SkipprDbtConfig) -> Result<ReactConfigFile, String> {
    let project = cfg.project.trim();
    if project.is_empty() {
        return Err("project name is required in skippr.yaml".to_string());
    }

    let warehouse_json = match &cfg.warehouse {
        Some(WarehouseConfig::Snowflake {
            database,
            schema,
            warehouse,
            role,
        }) => {
            let mut m = serde_json::Map::new();
            m.insert("kind".into(), "snowflake".into());
            if let Some(v) = database {
                m.insert("database".into(), v.clone().into());
            }
            if let Some(v) = schema {
                m.insert("schema".into(), v.clone().into());
            }
            if let Some(v) = warehouse {
                m.insert("warehouse".into(), v.clone().into());
            }
            if let Some(v) = role {
                m.insert("role".into(), v.clone().into());
            }
            serde_json::Value::Object(m)
        }
        Some(WarehouseConfig::Bigquery {
            project,
            dataset,
            location,
        }) => {
            let mut m = serde_json::Map::new();
            m.insert("kind".into(), "bigquery".into());
            if let Some(v) = project {
                m.insert("project".into(), v.clone().into());
            }
            if let Some(v) = dataset {
                m.insert("dataset".into(), v.clone().into());
            }
            if let Some(v) = location {
                m.insert("location".into(), v.clone().into());
            }
            serde_json::Value::Object(m)
        }
        Some(WarehouseConfig::Postgres { database, schema }) => {
            let mut m = serde_json::Map::new();
            m.insert("kind".into(), "postgres".into());
            if let Some(v) = database {
                m.insert("database".into(), v.clone().into());
            }
            if let Some(v) = schema {
                m.insert("schema".into(), v.clone().into());
            }
            serde_json::Value::Object(m)
        }
        None => return Err("warehouse is not configured. Run: skippr connect warehouse <kind>".to_string()),
    };

    let dbt_target = cfg.warehouse_kind_str().unwrap_or_default().to_string();

    let dbt_cfg = cfg.dbt.clone().unwrap_or_default();
    let target_schema = dbt_cfg
        .target_schema
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| project.to_string());
    let silver_suffix = dbt_cfg
        .silver_suffix
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "silver".to_string());
    let gold_suffix = dbt_cfg
        .gold_suffix
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "gold".to_string());

    let el_json = match &cfg.source {
        Some(source) => {
            let skippr_input = match source {
                SourceConfig::Mssql { connection_string } => {
                    let mut m = serde_json::Map::new();
                    m.insert("kind".into(), "mssql".into());
                    if let Some(cs) = connection_string {
                        m.insert("connection_string".into(), cs.clone().into());
                    }
                    serde_json::Value::Object(m)
                }
                SourceConfig::S3 {
                    s3_bucket,
                    s3_prefix,
                    transform,
                } => {
                    let mut m = serde_json::Map::new();
                    m.insert("kind".into(), "s3".into());
                    if let Some(v) = s3_bucket {
                        m.insert("s3_bucket".into(), v.clone().into());
                    }
                    if let Some(v) = s3_prefix {
                        m.insert("s3_prefix".into(), v.clone().into());
                    }
                    if let Some(t) = transform {
                        let mut tm = serde_json::Map::new();
                        if let Some(nf) = &t.namespace_fields {
                            tm.insert("namespace_fields".into(), nf.clone().into());
                        }
                        if !tm.is_empty() {
                            m.insert("transform".into(), serde_json::Value::Object(tm));
                        }
                    }
                    serde_json::Value::Object(m)
                }
            };
            serde_json::json!({
                "enabled": true,
                "skippr_input": skippr_input,
            })
        }
        None => serde_json::json!({ "enabled": false }),
    };

    let providers = serde_json::json!({
        "warehouse": warehouse_json,
        "el": el_json,
        "catalog": {
            "enabled": true,
            "refresh_secs": 3600,
            "max_concurrency": 8,
        },
        "dbt": {
            "enabled": true,
            "runner": "host",
            "target": dbt_target,
            "naming": {
                "target_schema": target_schema,
                "silver_suffix": silver_suffix,
                "gold_suffix": gold_suffix,
            },
        },
        "vector": {
            "enabled": true,
        },
    });

    Ok(ReactConfigFile {
        version: Some(1),
        server: None,
        storage: Some(StorageFile {
            mode: Some("local".into()),
            bucket: None,
            path: Some("./.skippr".into()),
            s3_credentials: None,
        }),
        scope: Some(ScopeFile {
            tenant: Some("_".into()),
            workspace: Some("dev".into()),
            project_id: Some(project.to_string()),
        }),
        llm: Some(LlmFile {
            provider: Some("OPENAI_COMPAT".into()),
            base_url: Some("https://api.openai.com".into()),
            reason_model: Some("gpt-5.4".into()),
            task_model: Some("gpt-5.4".into()),
            embed_model: Some("text-embedding-3-small".into()),
            context_length: Some(8192),
            http_timeout_secs: Some(120),
            max_tokens: Some(8192),
            temperature: Some(0.2),
            top_p: Some(1.0),
            ..Default::default()
        }),
        providers: Some(providers),
    })
}

/// Set the skippr binary path in the EL provider config.
pub fn set_skippr_binary(cfg: &mut ReactConfigFile, binary_path: &str) {
    if let Some(ref mut providers) = cfg.providers {
        if let Some(el) = providers.get_mut("el") {
            el["skippr_binary"] = serde_json::Value::String(binary_path.to_string());
        }
    }
}

/// Overlay authenticated mode onto an existing config:
/// - Switch storage to S3 with STS credentials
/// - Set server-provided LLM API key (if user hasn't set their own)
/// - Initialize metering client
pub fn apply_authenticated_overlay(
    cfg: &mut ReactConfigFile,
    creds: &crate::api_client::CredentialsResponse,
    auth_token: &str,
    initial_balance: f64,
) {
    cfg.storage = Some(StorageFile {
        mode: Some("s3".into()),
        bucket: Some(creds.bucket.clone()),
        path: Some(creds.key_prefix.clone()),
        s3_credentials: Some(S3Credentials {
            access_key_id: creds.credentials.access_key_id.clone(),
            secret_access_key: creds.credentials.secret_access_key.clone(),
            session_token: Some(creds.credentials.session_token.clone()),
            region: "us-east-1".to_string(),
            key_prefix: creds.key_prefix.clone(),
        }),
    });

    if let Some(scope) = cfg.scope.as_mut() {
        if !creds.tenant_id.trim().is_empty() {
            scope.tenant = Some(creds.tenant_id.clone());
        }
    }

    if !creds.llm_api_key.is_empty() {
        let existing = std::env::var("LLM_API_KEY").ok().filter(|v| !v.trim().is_empty());
        if existing.is_none() {
            std::env::set_var("LLM_API_KEY", &creds.llm_api_key);
        }
    }

    let accounting_url = if creds.accounting_url.is_empty() {
        None
    } else {
        Some(creds.accounting_url.clone())
    };

    react_suite_data_engineer::metering::init_metering(
        accounting_url,
        Some(auth_token.to_string()),
        initial_balance,
    );

    react::llm::set_llm_usage_handler(Box::new(|usage: react::llm::LlmUsage| {
        react_suite_data_engineer::metering::report_llm_usage(
            usage.input_tokens,
            usage.output_tokens,
            usage.model,
        );
    }));

    react::llm::set_llm_pre_call_guard(Box::new(|| {
        react_suite_data_engineer::metering::check_budget()
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public_config::*;

    #[test]
    fn translate_minimal_snowflake() {
        let cfg = SkipprDbtConfig {
            project: "my_project".into(),
            warehouse: Some(WarehouseConfig::Snowflake {
                database: Some("ANALYTICS".into()),
                schema: Some("RAW".into()),
                warehouse: Some("COMPUTE_WH".into()),
                role: Some("ACCOUNTADMIN".into()),
            }),
            source: Some(SourceConfig::Mssql {
                connection_string: Some("${MSSQL_CONNECTION_STRING}".into()),
            }),
            dbt: None,
        };

        let internal = to_internal(&cfg).unwrap();
        assert_eq!(
            internal.scope.as_ref().unwrap().project_id.as_deref(),
            Some("my_project")
        );
        let p = internal.providers.unwrap();
        assert_eq!(p["warehouse"]["kind"], "snowflake");
        assert_eq!(p["el"]["enabled"], true);
        assert_eq!(p["dbt"]["naming"]["target_schema"], "my_project");
        assert_eq!(p["dbt"]["naming"]["silver_suffix"], "silver");
        assert_eq!(p["dbt"]["naming"]["gold_suffix"], "gold");
        assert_eq!(p["dbt"]["target"], "snowflake");
    }

    #[test]
    fn translate_missing_warehouse_errors() {
        let cfg = SkipprDbtConfig {
            project: "test".into(),
            warehouse: None,
            source: None,
            dbt: None,
        };
        assert!(to_internal(&cfg).is_err());
    }

    #[test]
    fn translate_postgres_warehouse() {
        let cfg = SkipprDbtConfig {
            project: "pg_project".into(),
            warehouse: Some(WarehouseConfig::Postgres {
                database: Some("analytics".into()),
                schema: Some("public".into()),
            }),
            source: Some(SourceConfig::Mssql {
                connection_string: Some("${MSSQL_CONNECTION_STRING}".into()),
            }),
            dbt: None,
        };

        let internal = to_internal(&cfg).unwrap();
        assert_eq!(
            internal.scope.as_ref().unwrap().project_id.as_deref(),
            Some("pg_project")
        );
        let p = internal.providers.unwrap();
        assert_eq!(p["warehouse"]["kind"], "postgres");
        assert_eq!(p["warehouse"]["database"], "analytics");
        assert_eq!(p["dbt"]["target"], "postgres");
        assert_eq!(p["dbt"]["naming"]["target_schema"], "pg_project");
    }

    #[test]
    fn translate_empty_project_errors() {
        let cfg = SkipprDbtConfig {
            project: "".into(),
            warehouse: Some(WarehouseConfig::Snowflake {
                database: None,
                schema: None,
                warehouse: None,
                role: None,
            }),
            source: None,
            dbt: None,
        };
        assert!(to_internal(&cfg).is_err());
    }

    #[test]
    fn authenticated_overlay_sets_scope_tenant_from_credentials() {
        let cfg = SkipprDbtConfig {
            project: "tes".into(),
            warehouse: Some(WarehouseConfig::Snowflake {
                database: Some("ANALYTICS".into()),
                schema: Some("RAW".into()),
                warehouse: Some("COMPUTE_WH".into()),
                role: Some("ACCOUNTADMIN".into()),
            }),
            source: Some(SourceConfig::Mssql {
                connection_string: Some("${MSSQL_CONNECTION_STRING}".into()),
            }),
            dbt: None,
        };

        let mut internal = to_internal(&cfg).unwrap();
        let creds = crate::api_client::CredentialsResponse {
            credentials: crate::api_client::StsCreds {
                access_key_id: "ak".into(),
                secret_access_key: "sk".into(),
                session_token: "st".into(),
                expiration: "2099-01-01T00:00:00Z".into(),
            },
            bucket: "skippr-prod".into(),
            key_prefix: "c3471188-8965-4c52-b486-7dbbd7a2d329/".into(),
            tenant_id: "c3471188-8965-4c52-b486-7dbbd7a2d329".into(),
            llm_api_key: String::new(),
            accounting_url: String::new(),
        };

        apply_authenticated_overlay(&mut internal, &creds, "token", 0.0);

        assert_eq!(
            internal.scope.as_ref().unwrap().tenant.as_deref(),
            Some("c3471188-8965-4c52-b486-7dbbd7a2d329")
        );
    }
}
