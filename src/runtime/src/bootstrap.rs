use std::sync::Arc;

use react_core::resolved_config as rc;
use react_core::suite::SuiteCtx;
use react_module_storage_local::LocalFileStorageAdapter;
use react_module_storage_s3::S3StorageAdapter;

use crate::wiring::{DefaultKeyspace, EnvSecretsProvider, LocalKeyspace};

pub async fn build_suite_ctx(cfg: &rc::ReactResolvedConfig) -> Result<SuiteCtx, String> {
    let s3_creds = cfg.storage.s3_credentials.as_ref();

    match cfg.storage.mode {
        rc::StorageMode::Local => tracing::info!(
            storage_mode = "local",
            storage_path = %cfg.storage.path.as_deref().unwrap_or(""),
            scope_tenant = %cfg.scope.tenant,
            scope_workspace = %cfg.scope.workspace,
            scope_project_id = %cfg.scope.project_id,
            "building suite context"
        ),
        rc::StorageMode::S3 => tracing::info!(
            storage_mode = "s3",
            storage_bucket = %cfg.storage.bucket.as_deref().unwrap_or(""),
            scope_tenant = %cfg.scope.tenant,
            scope_workspace = %cfg.scope.workspace,
            scope_project_id = %cfg.scope.project_id,
            "building suite context"
        ),
    }

    let (storage, keyspace, lance_uri_prefix, lance_storage_opts) =
        if cfg.storage.mode == rc::StorageMode::Local {
            let root = cfg
                .storage
                .path
                .clone()
                .ok_or_else(|| "missing storage.path for local mode".to_string())?;
            let storage = match LocalFileStorageAdapter::new(root.clone()) {
                Ok(s) => Arc::new(s) as Arc<dyn react_core::storage::StorageAdapter>,
                Err(e) => {
                    return Err(format!("{}", e));
                }
            };
            let lance_prefix = format!("file://{}", root);
            let keyspace = Arc::new(LocalKeyspace::new(root));
            (
                storage,
                keyspace as Arc<dyn crate::wiring::Keyspace>,
                lance_prefix,
                Vec::new(),
            )
        } else {
            let b = cfg
                .storage
                .bucket
                .clone()
                .ok_or_else(|| "missing storage.bucket for s3 mode".to_string())?;
            let (storage, lance_prefix, lance_opts) = if let Some(creds) = s3_creds {
                let storage = Arc::new(
                    S3StorageAdapter::from_credentials(
                        b.clone(),
                        &creds.access_key_id,
                        &creds.secret_access_key,
                        creds.session_token.as_deref(),
                        &creds.region,
                    )
                    .await,
                ) as Arc<dyn react_core::storage::StorageAdapter>;
                let prefix = format!("s3://{}", b);
                let mut opts = vec![
                    ("aws_access_key_id".into(), creds.access_key_id.clone()),
                    (
                        "aws_secret_access_key".into(),
                        creds.secret_access_key.clone(),
                    ),
                    ("aws_region".into(), creds.region.clone()),
                ];
                if let Some(tok) = &creds.session_token {
                    opts.push(("aws_session_token".into(), tok.clone()));
                }
                (storage, prefix, opts)
            } else {
                let storage = Arc::new(S3StorageAdapter::from_env(b.clone()).await)
                    as Arc<dyn react_core::storage::StorageAdapter>;
                (storage, format!("s3://{}", b), Vec::new())
            };
            let keyspace = Arc::new(DefaultKeyspace::new(b.clone()));
            (
                storage,
                keyspace as Arc<dyn crate::wiring::Keyspace>,
                lance_prefix,
                lance_opts,
            )
        };

    let secrets = Arc::new(EnvSecretsProvider::default());
    let llm = crate::llm::create_llm(&crate::llm::config_from_resolved(cfg));

    let mut sctx = SuiteCtx::new(storage, secrets, llm, cfg.scope.clone(), keyspace.clone());
    sctx.set_resolved_config(Some(Arc::new(cfg.clone())));

    crate::wiring::data_engineer::wire_providers(
        &mut sctx,
        &keyspace,
        &lance_uri_prefix,
        lance_storage_opts,
    )
    .await?;

    {
        let mut debug_reg = react_core::suite::DebugProviderRegistry::new();
        debug_reg.register(react_suite_data_engineer::debug::DataEngineerDebugProvider);
        debug_reg.register(react_suite_kb::debug::KbDebugProvider);
        sctx.set_capability(Arc::new(debug_reg));
    }

    Ok(sctx)
}
