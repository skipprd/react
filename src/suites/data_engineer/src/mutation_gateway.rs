use std::sync::Arc;

use react_core::agent::AgentCtx;
use crate::providers::DatasetCatalogProvider;

use crate::project_fs;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpectedBase {
    Any,
    MustExist,
    MustNotExist,
}

pub async fn replace_file_content(
    ctx: &AgentCtx,
    datasets: Option<&Arc<dyn DatasetCatalogProvider>>,
    rel_path: &str,
    new_content: &str,
    expected: ExpectedBase,
    content_type: &str,
) -> Result<project_fs::PatchOutcome, String> {
    let key = project_fs::join_storage_key(ctx, rel_path);
    let existing = ctx
        .storage()
        .get_bytes(&key)
        .await
        .ok()
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .unwrap_or_default();
    let expected_existed = match expected {
        ExpectedBase::Any => None,
        ExpectedBase::MustExist => Some(true),
        ExpectedBase::MustNotExist => Some(false),
    };
    let patch_text = project_fs::hunks_only_full_replace_patch(&existing, new_content);
    let out = project_fs::apply_patch(
        ctx,
        datasets,
        rel_path,
        &patch_text,
        None,
        expected_existed,
        project_fs::PatchApplyKind::UnifiedDiff,
    )
    .await?;
    ctx.storage()
        .put_bytes(&out.key, out.content.as_bytes(), content_type)
        .await
        .map_err(|e| e.to_string())?;
    Ok(out)
}

pub async fn apply_hunks_patch(
    ctx: &AgentCtx,
    datasets: Option<&Arc<dyn DatasetCatalogProvider>>,
    rel_path: &str,
    patch_text: &str,
) -> Result<project_fs::PatchOutcome, String> {
    let base_state = crate::patch_protocol::read_patch_base_state(ctx, rel_path).await;
    let out = crate::patch_protocol::apply_single_file_patch_with_base(
        ctx,
        datasets,
        rel_path,
        patch_text,
        &base_state,
    )
    .await?;
    ctx.storage()
        .put_bytes(&out.key, out.content.as_bytes(), "text/plain")
        .await
        .map_err(|e| e.to_string())?;
    Ok(out)
}
