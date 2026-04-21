use std::sync::Arc;

use react_core::keyspace::{DefaultKeyspace, Keyspace, LocalKeyspace};
use react_core::resolved_config as rc;
use react_core::suite::SuiteCtx;
use react_module_storage_local::LocalFileStorageAdapter;
use react_module_storage_s3::S3StorageAdapter;

use crate::host::HostComposition;
use crate::secrets::EnvSecretsProvider;

pub async fn build_base_suite_ctx(cfg: &rc::ReactResolvedConfig) -> Result<SuiteCtx, String> {
    let s3_creds = cfg.storage.s3_credentials.as_ref();

    match cfg.storage.mode {
        rc::StorageMode::Local => tracing::info!(
            storage_mode = "local",
            storage_path = %cfg.storage.path.as_deref().unwrap_or(""),
            scope_tenant = %cfg.scope.tenant,
            scope_workspace = %cfg.scope.workspace,
            scope_project_id = %cfg.scope.project_id,
            "building base suite context"
        ),
        rc::StorageMode::S3 => tracing::info!(
            storage_mode = "s3",
            storage_bucket = %cfg.storage.bucket.as_deref().unwrap_or(""),
            scope_tenant = %cfg.scope.tenant,
            scope_workspace = %cfg.scope.workspace,
            scope_project_id = %cfg.scope.project_id,
            "building base suite context"
        ),
    }

    let (storage, keyspace): (
        Arc<dyn react_core::storage::StorageAdapter>,
        Arc<dyn Keyspace>,
    ) = if cfg.storage.mode == rc::StorageMode::Local {
        let root = cfg
            .storage
            .path
            .clone()
            .ok_or_else(|| "missing storage.path for local mode".to_string())?;
        let storage = LocalFileStorageAdapter::new(root.clone())
            .map(Arc::new)
            .map_err(|error| error.to_string())? as Arc<dyn react_core::storage::StorageAdapter>;
        let keyspace = Arc::new(LocalKeyspace::new(root)) as Arc<dyn Keyspace>;
        (storage, keyspace)
    } else {
        let bucket = cfg
            .storage
            .bucket
            .clone()
            .ok_or_else(|| "missing storage.bucket for s3 mode".to_string())?;
        let storage = if let Some(creds) = s3_creds {
            Arc::new(
                S3StorageAdapter::from_credentials(
                    bucket.clone(),
                    &creds.access_key_id,
                    &creds.secret_access_key,
                    creds.session_token.as_deref(),
                    &creds.region,
                )
                .await,
            ) as Arc<dyn react_core::storage::StorageAdapter>
        } else {
            Arc::new(S3StorageAdapter::from_env(bucket.clone()).await)
                as Arc<dyn react_core::storage::StorageAdapter>
        };
        let keyspace = Arc::new(DefaultKeyspace::new(bucket)) as Arc<dyn Keyspace>;
        (storage, keyspace)
    };

    let secrets = Arc::new(EnvSecretsProvider::default());
    let llm = crate::llm::create_llm(&crate::llm::config_from_resolved(cfg));

    let mut sctx = SuiteCtx::new(storage, secrets, llm, cfg.scope.clone(), keyspace.clone());
    sctx.set_resolved_config(Some(Arc::new(cfg.clone())));
    Ok(sctx)
}

pub async fn build_suite_ctx(cfg: &rc::ReactResolvedConfig) -> Result<SuiteCtx, String> {
    build_base_suite_ctx(cfg).await
}

pub async fn build_suite_ctx_with(
    cfg: &rc::ReactResolvedConfig,
    host: &dyn HostComposition,
) -> Result<SuiteCtx, String> {
    let mut sctx = build_base_suite_ctx(cfg).await?;
    host.configure_suite_ctx(cfg, &mut sctx).await?;
    Ok(sctx)
}
