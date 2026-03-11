use crate::providers::WarehouseProvider;
use crate::references::DatasetRef;
use react_core::agent::AgentCtx;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RejectedDataset {
    pub dataset_id: String,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GroundedDatasetSet {
    /// Canonical, proven dataset ids: `<catalog>.<schema>.<table>`
    pub allowed: BTreeSet<String>,
    /// All candidate dataset ids considered.
    #[serde(default)]
    pub candidates: Vec<String>,
    /// Subset of candidates that were proven by schema().
    #[serde(default)]
    pub proven_by_schema: Vec<String>,
    /// Candidates rejected with explicit reasons.
    #[serde(default)]
    pub rejected: Vec<RejectedDataset>,
    /// Any warnings encountered while building the set (non-fatal).
    #[serde(default)]
    pub warnings: Vec<String>,
}

fn source_container_from_cfg(ctx: &AgentCtx) -> Option<String> {
    crate::ctx_ext::actx_providers_cfg(ctx).map(|p| p.warehouse.container.clone())
}

fn source_namespace_from_cfg(ctx: &AgentCtx) -> Option<String> {
    crate::ctx_ext::actx_providers_cfg(ctx).map(|p| p.warehouse.namespace.clone())
}

fn is_in_source_namespace(ctx: &AgentCtx, dataset_id: &str) -> bool {
    let Some(parts) = DatasetRef::parse(dataset_id) else {
        return false;
    };
    // If namespace isn't configured (tests / minimal contexts), don't reject candidates on this axis.
    let Some(ns) = source_namespace_from_cfg(ctx) else {
        return true;
    };
    parts.schema == ns
}

fn is_in_source_container(ctx: &AgentCtx, dataset_id: &str) -> bool {
    let Some(parts) = DatasetRef::parse(dataset_id) else {
        return false;
    };
    // If container isn't configured (tests / minimal contexts), don't reject candidates on this axis.
    let Some(cat) = source_container_from_cfg(ctx) else {
        return true;
    };
    parts.catalog == cat
}

async fn schema_proves_dataset(
    wh: &Arc<dyn WarehouseProvider>,
    dataset_id: &str,
) -> Result<(), String> {
    crate::transient_retry::retry_transient_default("schema_proves_dataset", || async {
        wh.schema(dataset_id).await.map(|_cols| ())
    })
    .await
}

/// Build a grounded dataset set for **raw/cleanse** (silver):
/// - only keeps datasets that are in configured raw schema AND proven by schema().
pub async fn build_grounded_raw_dataset_set(
    ctx: &AgentCtx,
    wh: &Arc<dyn WarehouseProvider>,
    candidates: &[String],
) -> GroundedDatasetSet {
    let mut out = GroundedDatasetSet::default();
    let mut uniq: BTreeSet<String> = BTreeSet::new();
    for c in candidates.iter() {
        let id = c.trim();
        if id.is_empty() {
            continue;
        }
        uniq.insert(id.to_string());
    }
    out.candidates = uniq.iter().cloned().collect();

    let cfg_container = source_container_from_cfg(ctx);
    let cfg_namespace = source_namespace_from_cfg(ctx);
    tracing::info!(
        "grounding: configured container={:?}, namespace={:?}, {} unique candidate(s)",
        cfg_container,
        cfg_namespace,
        uniq.len()
    );

    for ds in uniq.into_iter() {
        if DatasetRef::parse(&ds).is_none() {
            out.rejected.push(RejectedDataset {
                dataset_id: ds,
                reason: "invalid dataset_id format (expected <catalog>.<schema>.<table>)"
                    .to_string(),
            });
            continue;
        }
        if !is_in_source_container(ctx, &ds) {
            let parts = DatasetRef::parse(&ds);
            out.rejected.push(RejectedDataset {
                dataset_id: ds,
                reason: format!(
                    "dataset is not in configured source container (got {:?}, expected {:?})",
                    parts.as_ref().map(|p| &p.catalog),
                    cfg_container
                ),
            });
            continue;
        }
        if !is_in_source_namespace(ctx, &ds) {
            let parts = DatasetRef::parse(&ds);
            out.rejected.push(RejectedDataset {
                dataset_id: ds,
                reason: format!(
                    "not in configured raw source namespace (got {:?}, expected {:?})",
                    parts.as_ref().map(|p| &p.schema),
                    cfg_namespace
                ),
            });
            continue;
        }
        match schema_proves_dataset(wh, &ds).await {
            Ok(()) => {
                out.allowed.insert(ds.clone());
                out.proven_by_schema.push(ds);
            }
            Err(e) => {
                out.rejected.push(RejectedDataset {
                    dataset_id: ds,
                    reason: format!("schema lookup failed: {}", e),
                });
            }
        }
    }

    out
}

/// Build a set of available staging model names for **gold/model**:
/// discovered from dbt project files (staging SQL + manifest when present).
#[derive(Clone, Debug, Default)]
pub struct GroundedStagingModelSet {
    pub allowed_models: BTreeSet<String>,
    pub candidates: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn is_staging_model_name(name: &str) -> bool {
    name.trim().to_ascii_lowercase().starts_with("stg_")
}

pub fn is_ref_only_gold_input(input: &str) -> bool {
    // Enforce “gold reads from silver”: only ref stg_* models.
    is_staging_model_name(input)
}

/// Append an IMMUTABLE FACTS block listing the exact staging model names that
/// GOLD models must reference via `ref()`.  Used to ground both the design memo
/// and candidate generation so the LLM never invents abbreviated names.
///
/// When `staging_schemas` is provided, each model name is annotated with its
/// column list so the LLM can reason about structure.
pub fn enrich_query_with_staging_models(
    q: &str,
    staging: &GroundedStagingModelSet,
    staging_schemas: &crate::plan_types::SourceSchema,
) -> String {
    let mut out = q.to_string();
    if !staging.allowed_models.is_empty() {
        out.push_str(
            "\n\nIMMUTABLE FACTS (existing staging models \u{2014} GOLD models MUST reference these exact names via ref()):\n",
        );
        for name in &staging.allowed_models {
            if let Some(cols) = staging_schemas.get(name.as_str()) {
                let cols_str: Vec<String> = cols
                    .iter()
                    .map(|c| format!("{} ({})", c.name, c.data_type))
                    .collect();
                out.push_str(&format!("- {} => [{}]\n", name, cols_str.join(", ")));
            } else {
                out.push_str(&format!("- {name}\n"));
            }
        }
    }
    out
}

pub async fn discover_staging_models_from_storage(ctx: &AgentCtx) -> GroundedStagingModelSet {
    let mut out = GroundedStagingModelSet::default();
    let base = ctx
        .keyspace()
        .scoped_prefix(ctx.scope(), &["dbt"])
        .trim_end_matches('/')
        .to_string()
        + "/";

    // 1) models/staging/*.sql
    let staging_prefix = format!("{}models/staging/", base);
    let keys = match ctx.storage().list_prefix(&staging_prefix).await {
        Ok(k) => k,
        Err(e) => {
            tracing::warn!(
                "discover_staging_models_from_storage: list_prefix({}) failed: {e}",
                staging_prefix
            );
            out.warnings
                .push(format!("list_prefix failed for {staging_prefix}: {e}"));
            Vec::new()
        }
    };
    for k in keys {
        if !k.ends_with(".sql") || k.contains("/_versions/") {
            continue;
        }
        let rel = k.strip_prefix(&base).unwrap_or(&k).to_string();
        let name = std::path::Path::new(&rel)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if is_staging_model_name(&name) {
            out.allowed_models.insert(name.clone());
            out.candidates.push(name);
        }
    }

    // 2) target/manifest.json (best-effort enrichment)
    let manifest_key = format!("{}target/manifest.json", base);
    if let Ok(bytes) = ctx.storage().get_bytes(&manifest_key).await {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            if let Some(nodes) = v.get("nodes").and_then(|n| n.as_object()) {
                for (_uid, node) in nodes.iter() {
                    let rt = node
                        .get("resource_type")
                        .and_then(|x| x.as_str())
                        .unwrap_or("");
                    if rt != "model" {
                        continue;
                    }
                    let name = node
                        .get("name")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .trim();
                    if !name.is_empty() && is_staging_model_name(name) {
                        let fp = node
                            .get("original_file_path")
                            .or_else(|| node.get("path"))
                            .and_then(|x| x.as_str())
                            .unwrap_or("");
                        if fp.starts_with("models/staging/") {
                            out.allowed_models.insert(name.to_string());
                        }
                    }
                }
            }
        }
    }

    out
}

/// Fetch column schemas from the catalog for a set of dataset IDs.
/// Returns both the typed `SourceSchema` map (for compile-time threading) and
/// a rendered prompt block string (for LLM injection).
///
/// This is the single DRY helper reused by both cleanse and model paths.
/// If the catalog is unavailable or a dataset has no catalog entry, that dataset
/// is silently skipped — the grounding gate downstream will catch any gaps.
pub async fn build_catalog_column_context(
    ctx: &AgentCtx,
    dataset_ids: &[String],
) -> (crate::plan_types::SourceSchema, String) {
    use crate::plan_types::{SourceColumnDef, SourceSchema};

    let mut schema_map: SourceSchema = BTreeMap::new();
    let catalog = crate::ctx_ext::actx_catalog(ctx);

    if let Some(cat) = catalog {
        for ds_id in dataset_ids {
            let id = ds_id.trim();
            if id.is_empty() {
                continue;
            }
            match cat.read_catalog(ctx.scope(), id).await {
                Ok(Some(dc)) => {
                    let cols: Vec<SourceColumnDef> = dc
                        .fields
                        .iter()
                        .map(|f| SourceColumnDef {
                            name: f.name.clone(),
                            data_type: f
                                .data_type
                                .clone()
                                .unwrap_or_else(|| "unknown".to_string()),
                        })
                        .collect();
                    if !cols.is_empty() {
                        schema_map.insert(id.to_string(), cols);
                    }
                }
                Ok(None) => {
                    tracing::debug!(
                        "build_catalog_column_context: no catalog entry for {}",
                        id
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        "build_catalog_column_context: read_catalog failed for {}: {}",
                        id,
                        e
                    );
                }
            }
        }
    } else {
        tracing::warn!("build_catalog_column_context: CatalogProvider not available");
    }

    let prompt_block = render_source_schema_prompt_block(&schema_map);
    (schema_map, prompt_block)
}

/// Render a `SourceSchema` map into a bounded prompt block for LLM injection.
pub fn render_source_schema_prompt_block(
    schema: &crate::plan_types::SourceSchema,
) -> String {
    if schema.is_empty() {
        return String::new();
    }
    let mut out =
        String::from("\n\nAUTHORITATIVE SCHEMAS (source columns \u{2014} output_fields.source_columns MUST reference only these exact names):\n");
    for (ds_id, cols) in schema.iter() {
        let cols_str: Vec<String> = cols
            .iter()
            .map(|c| format!("{} ({})", c.name, c.data_type))
            .collect();
        out.push_str(&format!("  {}: [{}]\n", ds_id, cols_str.join(", ")));
    }
    out
}

/// Group dataset fqn strings by (catalog, schema) for schema.yml sources emission.
pub fn group_by_catalog_schema(
    dataset_ids: &BTreeSet<String>,
) -> BTreeMap<(String, String), Vec<String>> {
    let mut out: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for id in dataset_ids.iter() {
        let Some(p) = DatasetRef::parse(id) else {
            continue;
        };
        out.entry((p.catalog, p.schema)).or_default().push(p.table);
    }
    for (_k, v) in out.iter_mut() {
        v.sort();
        v.dedup();
    }
    out
}
