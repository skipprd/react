use std::sync::Arc;

use react_core::keyspace::{DefaultKeyspace, Keyspace, LocalKeyspace};
use react_core::resolved_config as rc;
use react_core::storage::{cached, StorageAdapter};
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

    let (storage, keyspace): (Arc<dyn StorageAdapter>, Arc<dyn Keyspace>) = if cfg.storage.mode
        == rc::StorageMode::Local
    {
        let root = cfg
            .storage
            .path
            .clone()
            .ok_or_else(|| "missing storage.path for local mode".to_string())?;
        let storage = LocalFileStorageAdapter::new(root.clone())
            .map(Arc::new)
            .map_err(|error| error.to_string())? as Arc<dyn StorageAdapter>;
        let keyspace = Arc::new(LocalKeyspace::new(root)) as Arc<dyn Keyspace>;
        (storage, keyspace)
    } else {
        let bucket = cfg
            .storage
            .bucket
            .clone()
            .ok_or_else(|| "missing storage.bucket for s3 mode".to_string())?;
        let storage = if let Some(creds) = s3_creds {
            Arc::new(S3StorageAdapter::from_resolved_credentials(bucket.clone(), creds).await)
                as Arc<dyn StorageAdapter>
        } else {
            Arc::new(S3StorageAdapter::from_env(bucket.clone()).await) as Arc<dyn StorageAdapter>
        };
        let keyspace = Arc::new(DefaultKeyspace::new(bucket)) as Arc<dyn Keyspace>;
        (storage, keyspace)
    };
    let storage = cached(storage);

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

#[cfg(test)]
mod tests {
    use super::*;
    use react_core::resolved_config::{
        LlmResolved, ReactResolvedConfig, ServerResolved, StorageMode, StorageResolved,
    };
    use react_core::scope::RequestScope;

    fn local_cfg(path: String) -> ReactResolvedConfig {
        ReactResolvedConfig {
            server: ServerResolved { port: 1 },
            storage: StorageResolved {
                mode: StorageMode::Local,
                bucket: None,
                path: Some(path),
                s3_credentials: None,
            },
            scope: RequestScope::parse("t", "w", "p").expect("valid test scope"),
            llm: LlmResolved::default(),
            suite_config: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn base_suite_context_wraps_storage_in_cache() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = local_cfg(dir.path().to_string_lossy().to_string());
        let ctx = build_base_suite_ctx(&cfg).await.expect("suite ctx");
        let key = ctx.keyspace().scoped_key(ctx.scope(), &["cache-probe.txt"]);

        ctx.storage()
            .put_bytes(&key, b"cached", "text/plain")
            .await
            .expect("put");
        assert_eq!(
            ctx.storage().get_bytes(&key).await.expect("first get"),
            b"cached"
        );

        std::fs::remove_file(dir.path().join(&key)).expect("remove backing file");
        assert_eq!(
            ctx.storage().get_bytes(&key).await.expect("cached get"),
            b"cached"
        );
    }
}
