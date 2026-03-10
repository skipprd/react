use std::sync::Arc;

use react_core::resolved_config as rc;
use react_core::suite::SuiteCtx;
use react_module_storage_local::LocalFileStorageAdapter;
use react_module_storage_s3::S3StorageAdapter;

use crate::wiring::{DefaultKeyspace, EnvSecretsProvider, LocalKeyspace};

pub async fn build_suite_ctx(cfg: &rc::ReactResolvedConfig) -> Result<SuiteCtx, String> {
    let (storage, keyspace, lance_uri_prefix) = if cfg.storage.mode == rc::StorageMode::Local {
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
        )
    } else {
        let b = cfg
            .storage
            .bucket
            .clone()
            .ok_or_else(|| "missing storage.bucket for s3 mode".to_string())?;
        let storage = Arc::new(S3StorageAdapter::from_env(b.clone()).await)
            as Arc<dyn react_core::storage::StorageAdapter>;
        let lance_prefix = format!("s3://{}", b);
        let keyspace = Arc::new(DefaultKeyspace::new(b.clone()));
        (
            storage,
            keyspace as Arc<dyn crate::wiring::Keyspace>,
            lance_prefix,
        )
    };

    let secrets = Arc::new(EnvSecretsProvider::default());
    let llm = crate::llm::create_llm(&crate::llm::config_from_resolved(cfg));

    let mut sctx = SuiteCtx::new(storage, secrets, llm, cfg.scope.clone(), keyspace.clone());
    sctx.set_resolved_config(Some(Arc::new(cfg.clone())));

    crate::wiring::data_engineer::wire_providers(&mut sctx, &keyspace, &lance_uri_prefix).await?;

    Ok(sctx)
}
