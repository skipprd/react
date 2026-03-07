use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tokio::task::JoinSet;
use tracing::info;

use react_core::agent::AgentCtx;
use react_core::keyspace::encode_key_component;
use react_core::tools::Tool;

use crate::data_engineer::dbt_repair::remediate::active_provider_dialect;
use crate::data_engineer::{naming, plan, sql_first};

use super::model_authoring_engine::{
    self as engine, athena_alias_reuse_hint, build_provider_prompt_rules, dedup_notes,
};

fn normalize_folder(folder: Option<&str>) -> String {
    match folder.unwrap_or("marts").trim().to_lowercase().as_str() {
        "core" => "core".to_string(),
        _ => "marts".to_string(),
    }
}

fn gold_model_rel_path(folder: &str, name: &str) -> String {
    format!("models/{}/{}.sql", folder, name)
}

fn staging_rel_path_from_input(input: &str) -> String {
    let t = input.trim();
    if t.contains('/') || t.ends_with(".sql") {
        // Treat as project-relative path.
        return t.to_string();
    }
    format!("models/staging/{}.sql", t)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut out = s[..max].to_string();
    out.push_str("\n-- [truncated]\n");
    out
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GoldModelItem {
    name: String,
    #[serde(default)]
    folder: Option<String>, // "marts" | "core"
    #[serde(default)]
    goal: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    inputs: Vec<String>,
    #[serde(default)]
    instructions: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct GoldModelArgs {
    #[serde(default)]
    items: Vec<GoldModelItem>,
}

fn build_gold_sys_prompt(
    provider: &str,
    dialect: &str,
    max_items: usize,
    provider_rules: &str,
) -> String {
    format!(
        "You are an expert analytics engineer.\n\
         Task: write GOLD model query as PLAIN SQL (no dbt config, no Jinja).\n\
         Provider: {provider}\n\
         Dialect: {dialect}\n\
         Output MUST be valid JSON only: {{\"sql\":\"...\", \"notes\":[...]}}.\n\
         You MUST reference inputs ONLY via the provided placeholders (e.g. __INPUT_0__).\n\
           - Do NOT use ref() / source() / Jinja in this step.\n\
           - The system will replace placeholders with real silver relations for validation, then with dbt ref() for materialization.\n\
         \n\
         CRITICAL gold rules:\n\
         - You MUST write a SELECT-based dbt model.\n\
         - Gold models MUST ONLY read from silver models under models/staging/ using ref('stg_*').\n\
         - Gold models MUST NOT call source() anywhere.\n\
         - IMPORTANT: The user payload may include plan invariants/notes; invariants are hard requirements.\n\
         - Prefer minimal, stable columns for business use; do not invent fields.\n\
         - CRITICAL: Do NOT select or reference any column not present in inputs[].schema_columns for that input.\n\
           If you need a field that does not exist in silver, put it in notes and do NOT guess.\n\
         - Use provided inputs[].schema_columns (from the warehouse/catalog) as ground truth for available columns + types.\n\
\n\
         ANALYST_NOTES_CONTRACT_V1\n\
         Analyst mindset (CRITICAL — include these in `notes` BEFORE writing SQL):\n\
         - Business question: one sentence describing the decision this model supports.\n\
         - Entity definition: what the table represents (e.g., what counts as a “customer/order”), based ONLY on available columns.\n\
         - Grain: one clear sentence. If you dedupe/aggregate, say exactly how and what you might lose.\n\
         - Time axis: which timestamp/date drives analysis (and what it means). If no suitable time column exists, say so.\n\
         - Metric definitions: list 2–4 metrics this table enables (definitions + caveats), grounded in available columns.\n\
         - Assumptions + evidence gaps: list any semantic assumptions you made because the domain isn’t explicit in the data.\n\
           For each gap, recommend the smallest validation probe (e.g., null rate, distinctness, top values) that would confirm/refute it.\n\
\n\
         - IMPORTANT time handling (consistency):\n\
           - If an input column is already typed as timestamp/date/timestamptz/datetime, use it directly; do NOT re-cast it to the same type.\n\
           - Do NOT narrow time zones: never cast timestamptz -> timestamp.\n\
           - If you need parsed timestamps but the input only has string-ish fields, do NOT try_cast in gold; instead note that silver should add a cleaned timestamp column.\n\
         - Dialect/provider compatibility:\n\
{provider_rules}\
         - Batch throughput: you will be asked to create up to {max_items} models per call.\n\
         \n"
        ,
        provider_rules = provider_rules
    )
}

use super::plan_prompt_helpers::{combine_instructions, render_plan_driven_instructions};

#[derive(Clone)]
pub struct GoldModelTool;

#[async_trait]
impl Tool for GoldModelTool {
    fn name(&self) -> &'static str {
        "gold_model"
    }

    async fn call(&self, args: Value, ctx: &AgentCtx) -> Result<Value, String> {
        let parsed_args: GoldModelArgs = serde_json::from_value(args.clone())
            .map_err(|e| format!("gold_model args parse error: {e}"))?;
        if parsed_args.items.is_empty() {
            return Err("gold_model requires args.items (non-empty)".to_string());
        }
        let max_items = crate::data_engineer::plan_progress::MAX_BATCH_SIZE;
        if parsed_args.items.len() > max_items {
            return Err(format!(
                "gold_model supports at most {max_items} items per call (got {}). Split into batches.",
                parsed_args.items.len()
            ));
        }

        let dialect = crate::data_engineer::resolved_config_from_ctx(ctx)
            .map(active_provider_dialect)
            .unwrap_or_else(|| "Unknown SQL dialect".to_string());
        let provider_name = crate::data_engineer::resolved_config_from_ctx(ctx)
            .and_then(|cfg| crate::data_engineer::de_config::de_config_from_resolved(cfg))
            .map(|p| p.warehouse.kind.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let wh = crate::data_engineer::ctx_ext::actx_warehouse(ctx);
        let provider_prompt_rules =
            build_provider_prompt_rules(wh.as_ref().map(|w| w.as_ref()));
        let sys = build_gold_sys_prompt(&provider_name, &dialect, max_items, &provider_prompt_rules);

        // Plan-first authoring: if there is an active model plan, use task invariants/notes as the
        // default authoring instructions (and merge with any explicit item.instructions overrides).
        let plan_opt = plan::load_model_plan(ctx).await;
        let global_semantic_context = ctx
            .storage
            .get_json(&ctx.keyspace.scoped_key(
                &ctx.scope,
                &["semantic", &format!("{}.yaml", encode_key_component(crate::data_engineer::providers::GLOBAL_SEMANTIC_DATASET_ID))],
            ))
            .await
            .ok()
            .unwrap_or(serde_json::Value::Null);

        let base = ctx
            .keyspace
            .scoped_prefix(&ctx.scope, &["dbt"])
            .trim_end_matches('/')
            .to_string();
        let query = crate::data_engineer::ctx_ext::actx_warehouse(ctx)
            .expect("warehouse provider required for gold_model");
        let (target_container, silver_ns) = crate::data_engineer::resolved_config_from_ctx(ctx)
            .and_then(|cfg| crate::data_engineer::de_config::de_config_from_resolved(cfg))
            .map(|p| {
                let container = p.warehouse.container.clone();
                let base_schema = p.dbt.naming.target_schema.clone();
                let silver_suffix = p.dbt.naming.silver_suffix.clone();
                let db = if base_schema.trim().is_empty() {
                    "".to_string()
                } else {
                    format!("{}_{}", base_schema.trim(), silver_suffix.trim())
                };
                (container, db)
            })
            .unwrap_or_else(|| ("".to_string(), "".to_string()));

        let mut written: Vec<String> = Vec::new();
        let mut notes: Vec<String> = Vec::new();
        let mut errors: Vec<String> = Vec::new();
        let mut remediation_hints: Vec<Value> = Vec::new();
        let mut succeeded_item_names: Vec<String> = Vec::new();
        let mut canonical_folder_by_name: HashMap<String, String> = HashMap::new();
        let mut plan_output_field_names: Vec<String> = Vec::new();

        for it in parsed_args.items.iter() {
            let name = it.name.trim();
            if name.is_empty() {
                errors.push("gold_model item.name is required".to_string());
                continue;
            }
            if it.inputs.is_empty() {
                errors.push(format!("{name}: gold_model item.inputs is required (list of stg_* model names or paths)"));
                continue;
            }

            let requested_folder = normalize_folder(it.folder.as_deref());
            let core_rel = gold_model_rel_path("core", name);
            let marts_rel = gold_model_rel_path("marts", name);
            let core_exists = ctx
                .storage
                .get_bytes(&format!("{}/{}", base, core_rel))
                .await
                .is_ok();
            let marts_exists = ctx
                .storage
                .get_bytes(&format!("{}/{}", base, marts_rel))
                .await
                .is_ok();
            if core_exists && marts_exists {
                errors.push(format!(
                    "{name}: model exists in both canonical folders (models/core and models/marts). Keep exactly one canonical location before authoring."
                ));
                continue;
            }
            let folder = if core_exists {
                "core".to_string()
            } else if marts_exists {
                "marts".to_string()
            } else {
                requested_folder
            };
            if let Some(prev) = canonical_folder_by_name.get(name) {
                if prev != &folder {
                    errors.push(format!(
                        "{name}: conflicting target folders in this call ('{}' vs '{}'). Use one canonical folder for this model name.",
                        prev, folder
                    ));
                    continue;
                }
            } else {
                canonical_folder_by_name.insert(name.to_string(), folder.clone());
            }
            let rel_path = gold_model_rel_path(&folder, name);

            // Load the inputs to ground the LLM in actual silver SQL.
            let max_fetch_concurrency = 3usize;
            let storage = ctx.storage.clone();
            let query2 = query.clone();
            let target_container2 = target_container.clone();
            let silver_ns2 = silver_ns.clone();
            let mut set: JoinSet<(usize, String, String, String, String, Vec<(String, String)>)> =
                JoinSet::new();
            // (idx, input, rel_path, content, derived_relation_fqn, schema_cols)
            let mut fetched: Vec<(usize, String, String, String, String, Vec<(String, String)>)> =
                Vec::new();
            for (idx, inp) in it.inputs.iter().cloned().enumerate() {
                while set.len() >= max_fetch_concurrency {
                    if let Some(res) = set.join_next().await {
                        if let Ok(v) = res {
                            fetched.push(v);
                        }
                    }
                }
                let storage2 = storage.clone();
                let rel = staging_rel_path_from_input(&inp);
                let key = format!("{}/{}", base, rel);
                let rel2 = rel.clone();
                let query3 = query2.clone();
                let target_container3 = target_container2.clone();
                let silver_ns3 = silver_ns2.clone();
                set.spawn(async move {
                    let content = storage2
                        .get_bytes(&key)
                        .await
                        .ok()
                        .map(|b| String::from_utf8_lossy(&b).to_string())
                        .unwrap_or_default();

                    let alias = rel2
                        .rsplit('/')
                        .next()
                        .unwrap_or("")
                        .trim_end_matches(".sql")
                        .to_string();
                    let derived_fqn = if !target_container3.trim().is_empty()
                        && !silver_ns3.trim().is_empty()
                        && !alias.trim().is_empty()
                    {
                        format!("{}.{}.{}", target_container3, silver_ns3, alias)
                    } else {
                        "".to_string()
                    };
                    let schema_cols = if !derived_fqn.is_empty() {
                        query3.schema(&derived_fqn).await.unwrap_or_default()
                    } else {
                        vec![]
                    };

                    (idx, inp, rel2, content, derived_fqn, schema_cols)
                });
            }

            while let Some(res) = set.join_next().await {
                if let Ok(v) = res {
                    fetched.push(v);
                }
            }
            fetched.sort_by_key(|(idx, _, _, _, _, _)| *idx);
            let mut input_blocks: Vec<Value> = Vec::new();
            for (_idx, inp, rel, content, derived_fqn, schema_cols) in fetched.into_iter() {
                let cols_json: Vec<Value> = schema_cols
                    .into_iter()
                    .map(|(n, t)| serde_json::json!({"name": n, "type": t}))
                    .collect();
                if content.trim().is_empty() {
                    input_blocks.push(serde_json::json!({
                        "input": inp,
                        "path": rel,
                        "ok": false,
                        "error": "missing or empty input model SQL",
                        "derived_relation_fqn": derived_fqn,
                        "schema_columns": cols_json
                    }));
                } else {
                    input_blocks.push(serde_json::json!({
                        "input": inp,
                        "path": rel,
                        "ok": true,
                        "sql": truncate(&content, 20_000),
                        "derived_relation_fqn": derived_fqn,
                        "schema_columns": cols_json
                    }));
                }
            }

            let goal = if !it.goal.trim().is_empty() {
                it.goal.trim().to_string()
            } else {
                it.description.trim().to_string()
            };
            if goal.is_empty() {
                errors.push(format!("{name}: provide item.goal (or item.description) describing grain + business intent"));
                continue;
            }

            let (
                plan_invariants,
                plan_checklist,
                plan_expected_model_path,
                plan_implementation_spec,
            ) = plan_opt
                .as_ref()
                .and_then(|p| p.tasks.iter().find(|t| t.name.trim() == name))
                .map(|t| {
                    (
                        t.invariants.clone(),
                        t.checklist.clone(),
                        t.expected_model_path.clone().unwrap_or_default(),
                        t.implementation_spec.clone(),
                    )
                })
                .unwrap_or_else(|| (vec![], vec![], String::new(), None));
            if plan_output_field_names.is_empty() {
                if let Some(spec) = plan_implementation_spec.as_ref() {
                    plan_output_field_names = spec.output_fields.iter().map(|f| f.name.clone()).collect();
                }
            }
            let plan_instr = render_plan_driven_instructions(&plan_invariants, &plan_checklist);
            let effective_instructions = combine_instructions(&it.instructions, &plan_instr);

            let user_value = serde_json::json!({
                "model_name": name,
                "model_path": rel_path,
                "goal": goal,
                "global_semantic_context": global_semantic_context,
                "instructions": effective_instructions,
                "plan_invariants": plan_invariants,
                "plan_checklist": plan_checklist,
                "plan_implementation_spec": plan_implementation_spec,
                "plan_expected_model_path": plan_expected_model_path,
                "inputs": input_blocks,
                "existing_model_sql": ctx
                    .storage
                    .get_bytes(&format!("{}/{}", base, rel_path))
                    .await
                    .ok()
                    .map(|b| String::from_utf8_lossy(&b).to_string())
                    .unwrap_or_default()
                ,
                "sql_first": {
                    "input_placeholders": it.inputs.iter().enumerate().map(|(i, inp)| {
                        serde_json::json!({
                            "input": inp,
                            "placeholder": format!("__INPUT_{}__", i),
                            "materialize_ref": if inp.trim().contains('/') || inp.trim().ends_with(".sql") { String::new() } else { format!("{{{{ ref('{}') }}}}", inp.trim()) }
                        })
                    }).collect::<Vec<Value>>()
                }
            });

            let max_tokens = sql_first::sql_first_max_output_tokens(6500);
            let max_attempts = sql_first::sql_first_max_repair_attempts(4);

            // Build placeholder replacement map for validation (placeholders -> quoted silver relations).
            let mut repl_validate: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            let mut repl_materialize: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            for (idx, inp) in it.inputs.iter().enumerate() {
                let ph = format!("__INPUT_{}__", idx);
                // Find derived_relation_fqn for this input (from input_blocks).
                let derived_fqn = input_blocks
                    .iter()
                    .find(|b| b.get("input").and_then(|v| v.as_str()) == Some(inp.as_str()))
                    .and_then(|b| b.get("derived_relation_fqn").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if derived_fqn.is_empty() {
                    errors.push(format!("{name}: cannot validate gold SQL: missing derived_relation_fqn for input '{inp}' (ensure silver models exist and are queryable)"));
                    continue;
                }
                let id = match query.parse_dataset_fqn(&derived_fqn) {
                    Ok(id) => id,
                    Err(e) => {
                        errors.push(format!("{name}: invalid derived_relation_fqn '{derived_fqn}' for input '{inp}': {e}"));
                        continue;
                    }
                };
                repl_validate.insert(ph.clone(), query.quote_fqn(&id));
                // Materialize: ref('stg_*') only for named inputs; path-like inputs are invalid for gold.
                if inp.trim().contains('/') || inp.trim().ends_with(".sql") {
                    errors.push(format!(
                        "{name}: gold inputs must be stg_* names, not paths ('{inp}')"
                    ));
                    continue;
                }
                repl_materialize.insert(ph.clone(), format!("{{{{ ref('{}') }}}}", inp.trim()));
            }
            if errors.iter().any(|e| {
                e.starts_with(&format!("{name}: gold inputs must"))
                    || e.starts_with(&format!("{name}: cannot validate"))
            }) {
                continue;
            }

            let loop_config = engine::AuthorLoopConfig {
                max_tokens: max_tokens as usize,
                max_attempts,
                initial_prompt_id: "data_engineer.tools.gold_model.sql_first",
                repair_prompt_id: "data_engineer.tools.gold_model.sql_first_repair",
                initial_temp: 0.12,
                repair_temp: 0.08,
            };
            let sys2 = sys.clone();
            let loop_result = engine::sql_first_author_loop(
                ctx,
                &loop_config,
                || sys2.clone(),
                &user_value,
                &repl_validate,
                name,
                &rel_path,
                |_d| Ok(()),
            )
            .await;
            let outcome = match loop_result {
                Ok(o) => o,
                Err(errs) => {
                    errors.extend(errs);
                    continue;
                }
            };
            remediation_hints.extend(outcome.remediation_hints);

            let query_for_validate = query.clone();
            let provider_name_for_validate = provider_name.clone();
            let rel_path_for_hint = rel_path.clone();
            let name_for_hint = name.to_string();
            let existing_sql = ctx
                .storage
                .get_bytes(&format!("{}/{}", base, rel_path))
                .await
                .ok()
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_default();

            let write_result = engine::compile_and_write_model(
                ctx,
                &outcome.draft,
                plan_implementation_spec
                    .as_ref()
                    .map(|s| s.output_fields.as_slice())
                    .unwrap_or(&[]),
                &repl_materialize,
                |dbt_sql| {
                    if naming::contains_source_call(dbt_sql) {
                        return Err("invalid gold SQL: contains source(). Gold must only read from silver via ref('stg_*').".to_string());
                    }
                    if !naming::contains_ref_call(dbt_sql) {
                        return Err("invalid gold SQL: must reference at least one silver model via ref('stg_*').".to_string());
                    }
                    if let Some(msg) = query_for_validate.unsupported_sql_reason(dbt_sql) {
                        return Err(format!(
                            "unsupported SQL for provider '{}': {msg}",
                            provider_name_for_validate
                        ));
                    }
                    Ok(())
                },
                &existing_sql,
                &rel_path,
            )
            .await;
            match write_result {
                Ok(result) => {
                    written.push(result.key);
                    succeeded_item_names.push(name.to_string());
                    for n in result.notes {
                        let nt = n.trim();
                        if !nt.is_empty() {
                            notes.push(format!("{name}: {nt}"));
                        }
                    }
                }
                Err(e) => {
                    if let Some(h) = athena_alias_reuse_hint(&e, &rel_path_for_hint, &name_for_hint) {
                        remediation_hints.push(h);
                    }
                    errors.push(format!("{name}: {e}"));
                    continue;
                }
            }
        }

        let out_notes = dedup_notes(notes, 50);

        info!(
            target: "gold_model",
            items = parsed_args.items.len(),
            written = written.len(),
            ok = errors.is_empty(),
            "gold_model finished"
        );

        let mut result = serde_json::json!({
            "ok": errors.is_empty(),
            "batch_failure_kind": if errors.is_empty() { Value::Null } else { Value::String("unknown".to_string()) },
            "written_keys": written,
            "notes": out_notes,
            "remediation_hints": remediation_hints,
            "errors": errors,
            "succeeded_item_names": succeeded_item_names
        });
        if !errors.is_empty() && !plan_output_field_names.is_empty() {
            result["expected_output_fields"] = serde_json::json!(plan_output_field_names);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use react_core::keyspace::{DefaultKeyspace, Keyspace};
    use react_core::llm::ChatMessage;
    use react_core::llm::LargeLanguageModel;
    use crate::data_engineer::providers::{QueryProvider, QueryResult};
    use react_core::scope::RequestScope;
    use react_core::storage::{InMemoryStorageAdapter, StorageAdapter};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct MockWarehouse;

    #[async_trait]
    impl QueryProvider for MockWarehouse {
        async fn query(&self, _sql: &str) -> Result<QueryResult, String> {
            Ok(QueryResult {
                header: vec![],
                rows: vec![],
                meta: None,
            })
        }
        async fn schema(&self, _dataset_fqn: &str) -> Result<Vec<(String, String)>, String> {
            Ok(vec![("order_id".to_string(), "string".to_string())])
        }
        async fn sample(
            &self,
            _dataset_fqn: &str,
            _limit: usize,
        ) -> Result<Vec<Vec<String>>, String> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl crate::data_engineer::providers::DatasetCatalogProvider for MockWarehouse {
        async fn list_datasets(&self) -> Result<Vec<crate::data_engineer::providers::DatasetId>, String> {
            Ok(vec![])
        }
        async fn get_dataset_schema(
            &self,
            dataset: &crate::data_engineer::providers::DatasetId,
        ) -> Result<Vec<(String, String)>, String> {
            self.schema(&dataset.fqn()).await
        }
        async fn get_dataset_stats(
            &self,
            _dataset: &crate::data_engineer::providers::DatasetId,
            _max_fields: usize,
        ) -> Result<
            (
                crate::data_engineer::providers::DatasetFieldStats,
                crate::data_engineer::providers::DatasetStats,
            ),
            String,
        > {
            Err("not used".to_string())
        }
    }

    impl crate::data_engineer::providers::WarehouseNaming for MockWarehouse {
        fn kind(&self) -> crate::data_engineer::de_config::WarehouseKind {
            crate::data_engineer::de_config::WarehouseKind::default()
        }
        fn parse_dataset_fqn(
            &self,
            dataset_fqn: &str,
        ) -> Result<crate::data_engineer::providers::DatasetId, String> {
            let parts: Vec<&str> = dataset_fqn.split('.').collect();
            if parts.len() != 3 {
                return Err("invalid fqn".to_string());
            }
            Ok(crate::data_engineer::providers::DatasetId {
                catalog: parts[0].to_string(),
                database: parts[1].to_string(),
                table: parts[2].to_string(),
            })
        }
        fn quote_ident(&self, ident: &str) -> String {
            format!("\"{}\"", ident.replace('"', "\"\""))
        }
    }

    #[derive(Default)]
    struct MockLlm {
        resp: String,
    }

    impl LargeLanguageModel for MockLlm {
        fn chat(
            &self,
            _messages: &[ChatMessage],
            _options: &react_core::llm::LlmCallOptions,
        ) -> Result<String, String> {
            Ok(self.resp.clone())
        }
        fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
            Ok(vec![])
        }
    }

    fn minimal_cfg() -> Arc<react_core::resolved_config::ReactResolvedConfig> {
        Arc::new(react_core::resolved_config::ReactResolvedConfig {
            server: react_core::resolved_config::ServerResolved { port: 1 },
            storage: react_core::resolved_config::StorageResolved { mode: react_core::resolved_config::StorageMode::Local, bucket: None, path: None },
            scope: RequestScope {
                tenant: "t".to_string(),
                workspace: "w".to_string(),
                project_id: "p".to_string(),
            },
            llm: react_core::resolved_config::LlmResolved::default(),
            suite_config: serde_json::json!({
                "warehouse": { "kind": "athena", "container": "AwsDataCatalog", "namespace": "test_raw", "extras": {"region":"eu-west-1","workgroup":"wg","result_s3":"s3://x/"} },
                "catalog": { "enabled": false, "refresh_secs": 60, "max_concurrency": 8 },
                "dbt": { "enabled": true, "target": "athena", "naming": { "target_schema": "test", "silver_suffix": "silver", "gold_suffix": "warehouse" }, "runner": "host" },
                "vector": { "enabled": false }
            }),
        })
    }

    fn make_ctx(storage: Arc<dyn StorageAdapter>, llm: Arc<dyn LargeLanguageModel>) -> AgentCtx {
        let keyspace: Arc<dyn Keyspace> = Arc::new(DefaultKeyspace::new("b".to_string()));
        let scope = RequestScope {
            tenant: "t".to_string(),
            workspace: "w".to_string(),
            project_id: "p".to_string(),
        };
        let warehouse: Arc<dyn crate::data_engineer::providers::WarehouseProvider> =
            Arc::new(MockWarehouse::default());
        let mut actx = AgentCtx {
            top_k: 1,
            per_step_timeout_secs: 1,
            max_steps: 1,
            thread_id: None,
            progress_tx: None,
            pre_step_tx: None,
            trace_tx: None,
            agent_name: Some("test".to_string()),
            policy: Arc::new(react_core::agent::DefaultPolicy),
            llm,
            storage,
            scope: scope.clone(),
            keyspace,
            vector: None,
            thread_store: None,
            exec_ctx: None,
            resolved_config: Some(minimal_cfg()),
            capabilities: react_core::capability::CapabilityMap::default(),
        };
        actx.set_capability(Arc::new(crate::data_engineer::ctx_ext::WarehouseCap(warehouse)));
        actx
    }

    fn analyst_notes() -> serde_json::Value {
        serde_json::json!([
            "Business question: Provide an orders lens for operational/finance decisions.",
            "Entity definition: One row represents a single order as defined by the available order identifier(s).",
            "Grain: One row per order (no aggregation beyond order grain).",
            "Time axis: Use the best available order timestamp/date column; if missing, note the gap.",
            "Metric definitions: Order count; revenue/amount if a numeric amount column exists; status counts if status exists.",
            "Assumptions & gaps: Column meanings are inferred from names; validate via null rate, distinctness, and top values for key fields."
        ])
    }

    #[test]
    fn gold_sys_prompt_includes_bigquery_alias_scope_rule() {
        let sys = build_gold_sys_prompt(
            "bigquery",
            "Google BigQuery (Standard SQL)",
            3,
            "           - If Provider is bigquery (Google BigQuery Standard SQL), never reference a SELECT-list alias inside another expression in the same SELECT list. If one derived field depends on another, split into CTE/subquery + outer SELECT.\n           - If Provider is bigquery, use SAFE_CAST(...) for tolerant casts (not try_cast).\n",
        );
        assert!(sys.contains("never reference a SELECT-list alias"));
        assert!(sys.contains("SAFE_CAST"));
    }

    #[tokio::test]
    async fn gold_model_writes_mart_and_injects_config() {
        let storage: Arc<dyn StorageAdapter> = Arc::new(InMemoryStorageAdapter::default());
        let base = "t/w/p/dbt";
        // Seed an input staging model so the tool can ground the prompt.
        let stg_key = format!("{}/models/staging/stg_test_raw_raw_orders.sql", base);
        storage
            .put_bytes(
                &stg_key,
                "select * from {{ source('test_raw','raw_orders') }}".as_bytes(),
                "text/sql",
            )
            .await
            .expect("seed staging");

        let llm = Arc::new(MockLlm {
            resp: serde_json::json!({
                "sql": "select * from __INPUT_0__",
                "notes": analyst_notes()
            })
            .to_string(),
        });
        let ctx = make_ctx(storage.clone(), llm);
        let tool = GoldModelTool;

        let out = tool
            .call(
                serde_json::json!({
                    "items": [{
                        "name": "fct_orders",
                        "folder": "marts",
                        "goal": "Orders fact at order grain.",
                        "inputs": ["stg_test_raw_raw_orders"]
                    }]
                }),
                &ctx,
            )
            .await
            .expect("tool call");

        assert!(
            out.get("ok").and_then(|v| v.as_bool()).unwrap_or(false),
            "out={}",
            out
        );
        let written = out
            .get("written_keys")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        assert_eq!(written.len(), 1);
        let key = written[0].as_str().unwrap_or("").to_string();
        let bytes = storage.get_bytes(&key).await.expect("written file exists");
        let content = String::from_utf8_lossy(&bytes).to_string();
        // Hard-cutover portability: do not inject `schema=` into model configs (dbt_project.yml governs schema).
        assert!(!content.contains("config(schema="));
        assert!(content.contains("alias=\"fct_orders\""));
        assert!(content
            .to_ascii_lowercase()
            .contains("ref('stg_test_raw_raw_orders')"));
    }

    #[tokio::test]
    async fn gold_model_rejects_source_calls() {
        let storage: Arc<dyn StorageAdapter> = Arc::new(InMemoryStorageAdapter::default());
        let base = "t/w/p/dbt";
        let stg_key = format!("{}/models/staging/stg_test_raw_raw_orders.sql", base);
        storage
            .put_bytes(
                &stg_key,
                "select * from {{ source('test_raw','raw_orders') }}".as_bytes(),
                "text/sql",
            )
            .await
            .expect("seed staging");

        let llm = Arc::new(MockLlm {
            resp: serde_json::json!({
                "sql": "select * from {{ source('test_raw','raw_orders') }}",
                "notes": analyst_notes()
            })
            .to_string(),
        });
        let ctx = make_ctx(storage.clone(), llm);
        let tool = GoldModelTool;

        let out = tool
            .call(
                serde_json::json!({
                    "items": [{
                        "name": "fct_orders",
                        "folder": "marts",
                        "goal": "Orders fact at order grain.",
                        "inputs": ["stg_test_raw_raw_orders"]
                    }]
                }),
                &ctx,
            )
            .await
            .expect("tool call");

        assert!(
            !out.get("ok").and_then(|v| v.as_bool()).unwrap_or(true),
            "out={}",
            out
        );
        let errs = out
            .get("errors")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        assert!(!errs.is_empty());
    }

    #[tokio::test]
    async fn gold_model_supports_multiple_inputs_and_missing_inputs_do_not_crash() {
        let storage: Arc<dyn StorageAdapter> = Arc::new(InMemoryStorageAdapter::default());
        let base = "t/w/p/dbt";
        // Seed two input staging models; leave one missing to simulate partial availability.
        let stg_orders_key = format!("{}/models/staging/stg_test_raw_raw_orders.sql", base);
        let stg_users_key = format!("{}/models/staging/stg_test_raw_raw_users.sql", base);
        storage
            .put_bytes(
                &stg_orders_key,
                "select 1 as order_id".as_bytes(),
                "text/sql",
            )
            .await
            .expect("seed orders staging");
        storage
            .put_bytes(&stg_users_key, "select 1 as user_id".as_bytes(), "text/sql")
            .await
            .expect("seed users staging");

        let llm = Arc::new(MockLlm {
            resp: serde_json::json!({
                "sql": "select * from __INPUT_0__",
                "notes": analyst_notes()
            })
            .to_string(),
        });
        let ctx = make_ctx(storage.clone(), llm);
        let tool = GoldModelTool;

        let out = tool
            .call(
                serde_json::json!({
                    "items": [{
                        "name": "fct_orders",
                        "folder": "marts",
                        "goal": "Orders fact at order grain.",
                        "inputs": [
                            "stg_test_raw_raw_orders",
                            "stg_test_raw_raw_users",
                            "stg_test_raw_raw_missing"
                        ]
                    }]
                }),
                &ctx,
            )
            .await
            .expect("tool call");

        // Tool may still succeed (missing inputs are passed to the LLM as ok=false blocks).
        let written = out
            .get("written_keys")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        assert_eq!(written.len(), 1);
    }

    #[tokio::test]
    async fn gold_model_uses_model_plan_invariants_and_notes_as_default_instructions() {
        #[derive(Clone)]
        struct CapturingLlm {
            resp: String,
            captured_instructions: Arc<Mutex<Option<String>>>,
        }
        impl LargeLanguageModel for CapturingLlm {
            fn chat(
                &self,
                messages: &[ChatMessage],
                _options: &react_core::llm::LlmCallOptions,
            ) -> Result<String, String> {
                let user = messages
                    .iter()
                    .find(|m| m.role == react_core::llm::ChatRole::User)
                    .map(|m| m.content.clone())
                    .unwrap_or_default();
                if let Ok(v) = serde_json::from_str::<Value>(&user) {
                    let instr = v
                        .get("instructions")
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string());
                    if let Ok(mut g) = self.captured_instructions.lock() {
                        *g = instr;
                    }
                }
                Ok(self.resp.clone())
            }
            fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
                Ok(vec![])
            }
        }

        let storage: Arc<dyn StorageAdapter> = Arc::new(InMemoryStorageAdapter::default());
        let base = "t/w/p/dbt";
        // Seed an input staging model so the tool can ground the prompt.
        let stg_key = format!("{}/models/staging/stg_test_raw_raw_orders.sql", base);
        storage
            .put_bytes(&stg_key, "select 1 as order_id".as_bytes(), "text/sql")
            .await
            .expect("seed staging");

        let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let llm = Arc::new(CapturingLlm {
            resp: serde_json::json!({
                "sql": "select * from __INPUT_0__",
                "notes": analyst_notes()
            })
            .to_string(),
            captured_instructions: captured.clone(),
        });

        let mut ctx = make_ctx(storage.clone(), llm);
        ctx.thread_id = Some("t1".to_string());

        // Seed an approved model plan with invariants/checklist for this model.
        let plan_key = crate::data_engineer::plan::new_model_plan_key(&ctx);
        let plan = crate::data_engineer::plan::ModelPlan {
            plan_key: plan_key.clone(),
            status: crate::data_engineer::plan::PlanStatus::Approved,
            project_snapshot: serde_json::Value::Null,
            tasks: vec![crate::data_engineer::plan::ModelTask {
                name: "fct_orders".to_string(),
                folder: crate::data_engineer::plan::ModelFolder::Marts,
                goal: "Orders fact at order grain.".to_string(),
                inputs: vec!["stg_test_raw_raw_orders".to_string()],
                expected_model_path: Some("models/marts/fct_orders.sql".to_string()),
                invariants: vec!["Grain: exactly 1 row per order_pk.".to_string()],
                implementation_spec: Some(crate::data_engineer::plan::ModelImplementationSpec {
                    spec_version: 1,
                    grain: "1 row per order_id".to_string(),
                    inputs: vec!["stg_test_raw_raw_orders".to_string()],
                    joins: vec![],
                    metrics: vec![crate::data_engineer::plan::MetricSpec {
                        name: "orders".to_string(),
                        definition: "count(*) of orders".to_string(),
                        caveats: vec![],
                    }],
                    output_fields: vec![crate::data_engineer::plan::OutputFieldSpec {
                        name: "order_id".to_string(),
                        kind: crate::data_engineer::plan::FieldKind::Clean,
                        source_columns: vec!["order_id".to_string()],
                        expression: "order_id passthrough from staging".to_string(),
                        data_type: None,
                        nullable: true,
                        description: None,
                    }],
                    assumptions: vec![],
                }),
                status: crate::data_engineer::plan::TaskStatus::Pending,
                checklist: vec![
                    crate::data_engineer::plan::PlanChecklistItem {
                        checklist_item_id: "sql_model".to_string(),
                        label: "Author gold SQL".to_string(),
                        details: Some(
                            "Filter out invalid orders based on silver validity flags.".to_string(),
                        ),
                        status: crate::data_engineer::plan::ChecklistItemStatus::Pending,
                        origin: crate::data_engineer::plan::ChecklistOrigin::Initial,
                        evidence: vec![],
                    },
                    crate::data_engineer::plan::PlanChecklistItem {
                        checklist_item_id: "schema_contract".to_string(),
                        label: "Author schema contract".to_string(),
                        details: None,
                        status: crate::data_engineer::plan::ChecklistItemStatus::Pending,
                        origin: crate::data_engineer::plan::ChecklistOrigin::Initial,
                        evidence: vec![],
                    },
                    crate::data_engineer::plan::PlanChecklistItem {
                        checklist_item_id: "validate".to_string(),
                        label: "Validate model".to_string(),
                        details: None,
                        status: crate::data_engineer::plan::ChecklistItemStatus::Pending,
                        origin: crate::data_engineer::plan::ChecklistOrigin::Initial,
                        evidence: vec![],
                    },
                ],
            }],
            batches: vec![vec!["fct_orders".to_string()]],
            work_groups: vec![
                crate::data_engineer::plan::PlanWorkGroup {
                    group_id: "wg_sql".to_string(),
                    label: "Author SQL".to_string(),
                    kind: crate::data_engineer::plan::WorkGroupKind::AuthorSql,
                    items: vec![crate::data_engineer::plan::WorkGroupItemRef {
                        task_id: "fct_orders".to_string(),
                        checklist_item_id: "sql_model".to_string(),
                    }],
                    depends_on_group_ids: None,
                },
                crate::data_engineer::plan::PlanWorkGroup {
                    group_id: "wg_schema".to_string(),
                    label: "Author schema".to_string(),
                    kind: crate::data_engineer::plan::WorkGroupKind::AuthorSchema,
                    items: vec![crate::data_engineer::plan::WorkGroupItemRef {
                        task_id: "fct_orders".to_string(),
                        checklist_item_id: "schema_contract".to_string(),
                    }],
                    depends_on_group_ids: Some(vec!["wg_sql".to_string()]),
                },
                crate::data_engineer::plan::PlanWorkGroup {
                    group_id: "wg_validate".to_string(),
                    label: "Validate".to_string(),
                    kind: crate::data_engineer::plan::WorkGroupKind::Validate,
                    items: vec![crate::data_engineer::plan::WorkGroupItemRef {
                        task_id: "fct_orders".to_string(),
                        checklist_item_id: "validate".to_string(),
                    }],
                    depends_on_group_ids: Some(vec!["wg_schema".to_string()]),
                },
            ],
            mutations: vec![],
            progress: crate::data_engineer::plan::PlanProgress::default(),
        };
        crate::data_engineer::plan::save_model_plan(&ctx, &plan)
            .await
            .unwrap();

        let tool = GoldModelTool;
        let out = tool
            .call(
                serde_json::json!({
                    "items": [{
                        "name": "fct_orders",
                        "folder": "marts",
                        "goal": "Orders fact at order grain.",
                        "inputs": ["stg_test_raw_raw_orders"]
                    }]
                }),
                &ctx,
            )
            .await
            .expect("tool call");

        assert!(out.get("ok").and_then(|v| v.as_bool()).unwrap_or(false));
        let got = captured
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_default();
        assert!(got.contains("Plan invariants"));
        assert!(got.contains("exactly 1 row"));
        assert!(got.contains("Plan checklist"));
        assert!(got.contains("validity flags"));
        assert!(!plan_key.trim().is_empty());
    }
}
