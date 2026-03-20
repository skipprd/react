use crate::progress_controller::PhaseTransition;
use crate::{control_flow, DataEngineerSuite, PhaseError, PhaseOutcome};
use react_core::session::ThreadStore;
use react_core::suite::SuiteCtx;

use crate::providers::{SkipprOutputConfig, SkipprPipelineConfig};

impl DataEngineerSuite {
    pub(super) async fn execute_el_discover_phase(
        thread_store: &ThreadStore,
        thread_id: &str,
        sctx: &SuiteCtx,
    ) -> Result<PhaseOutcome, PhaseError> {
        let skippr = crate::ctx_ext::sctx_skippr(sctx).ok_or_else(|| {
            "skippr provider not configured. enable providers.el and restart.".to_string()
        })?;

        let cfg = sctx
            .resolved_config()
            .as_ref()
            .and_then(|c| crate::de_config::de_config_from_resolved(c))
            .ok_or_else(|| "resolved config missing for EL discover".to_string())?;

        let pipeline_name = sctx.scope().project_id.as_str();

        let output_config = warehouse_to_output_config(&cfg.warehouse);

        let pipeline_config = SkipprPipelineConfig {
            pipeline_name: pipeline_name.to_string(),
            skippr_input: cfg.el.skippr_input.clone(),
            output_plugin: output_config,
        };

        // 1. Write skippr.yml
        skippr
            .write_pipeline_config(sctx.scope(), &pipeline_config)
            .await
            .map_err(|e| format!("failed to write skippr pipeline config: {}", e))?;

        // 2. Run skippr discover
        let discover_result = skippr
            .discover_pipeline(sctx.scope(), pipeline_name)
            .await
            .map_err(|e| format!("skippr discover failed: {}", e))?;

        if !discover_result.ok {
            let errs = discover_result.errors.join("; ");
            return Err(format!("skippr discover reported errors: {}", errs).into());
        }

        // 3. Read back schemas via SHOW PIPELINE
        let pipeline_status = skippr
            .show_pipeline(sctx.scope(), pipeline_name)
            .await
            .map_err(|e| format!("SHOW PIPELINE failed after discover: {}", e))?;

        let namespaces_count = pipeline_status.namespaces.len();
        if namespaces_count == 0 {
            return Err(
                "skippr discover completed but found no namespaces (tables)".to_string().into(),
            );
        }

        tracing::info!(
            namespaces = namespaces_count,
            pipeline = pipeline_name,
            "EL discover complete"
        );

        // 4. Persist discovered schemas to storage
        let schemas_json = serde_json::to_value(&pipeline_status)
            .map_err(|e| format!("failed to serialize pipeline status: {}", e))?;
        let schemas_key = sctx
            .keyspace()
            .scoped_key(sctx.scope(), &["el", "discovered_schemas"]);
        sctx.storage()
            .put_json(&schemas_key, &schemas_json)
            .await
            .map_err(|e| format!("failed to persist discovered schemas: {}", e))?;

        crate::phase_contract::commit_phase_decision(
            thread_store,
            thread_id,
            Some(control_flow::Phase::ElDiscover),
            crate::phase_contract::PhaseDecision::forward(
                control_flow::Phase::ElSync,
                Some(PhaseTransition::ElDiscoverOk { namespaces_count }),
            ),
        )
        .await?;

        Ok(PhaseOutcome::TransitionCommitted)
    }
}

fn warehouse_to_output_config(
    wh: &crate::de_config::WarehouseResolved,
) -> SkipprOutputConfig {
    use crate::de_config::WarehouseKind;
    let kind = match wh.kind {
        WarehouseKind::Snowflake => "snowflake",
        WarehouseKind::Athena => "athena",
        WarehouseKind::Postgres => "postgres",
        WarehouseKind::Bigquery => "bigquery",
        WarehouseKind::Mssql => "mssql",
    };
    SkipprOutputConfig {
        kind: kind.to_string(),
        account: None,
        database: Some(wh.container.clone()).filter(|s| !s.is_empty()),
        schema: Some(wh.namespace.clone()).filter(|s| !s.is_empty()),
        warehouse: wh
            .extras
            .get("warehouse")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        role: wh
            .extras
            .get("role")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    }
}
