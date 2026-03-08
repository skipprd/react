use react_core::session::ControlStateStore;
use tracing::warn;

use crate::progress_controller::{
    DataEngineerEvent, ExecutionState, EXECUTION_STATE_SCHEMA_VERSION,
};

pub const DATA_ENGINEER_SUITE_ID: &str = "data_engineer";

fn validate_loaded_state(parsed: &ExecutionState) -> Result<(), String> {
    if parsed.schema_version != EXECUTION_STATE_SCHEMA_VERSION {
        return Err(format!(
            "execution_state schema_version mismatch: expected {}, got {}",
            EXECUTION_STATE_SCHEMA_VERSION, parsed.schema_version
        ));
    }
    parsed
        .validate_invariants()
        .map_err(|e| format!("execution_state invariant check failed on load: {e}"))
}

pub async fn load_execution_state(control: &ControlStateStore, thread_id: &str) -> Option<ExecutionState> {
    let parsed: ExecutionState = control
        .load::<ExecutionState>(thread_id, DATA_ENGINEER_SUITE_ID)
        .await
        .ok()??;
    if let Err(e) = validate_loaded_state(&parsed) {
        warn!(thread_id, error = %e, "dropping execution_state due to schema mismatch or corruption");
        return None;
    }
    Some(parsed)
}

pub async fn load_execution_state_strict(
    control: &ControlStateStore,
    thread_id: &str,
) -> Result<Option<ExecutionState>, String> {
    let loaded = match control
        .load::<ExecutionState>(thread_id, DATA_ENGINEER_SUITE_ID)
        .await
    {
        Ok(loaded) => loaded,
        Err(e) if e.to_string().to_ascii_lowercase().contains("not found") => return Ok(None),
        Err(e) => {
            return Err(format!(
                "failed to load control state for execution_state: {e}"
            ))
        }
    };
    let Some(parsed) = loaded else {
        return Ok(None);
    };
    validate_loaded_state(&parsed)?;
    Ok(Some(parsed))
}

async fn persist_execution_state(
    control: &ControlStateStore,
    thread_id: &str,
    state: &ExecutionState,
) -> Result<(), String> {
    if state.schema_version != EXECUTION_STATE_SCHEMA_VERSION {
        return Err(format!(
            "execution_state schema_version mismatch: expected {}, got {}",
            EXECUTION_STATE_SCHEMA_VERSION, state.schema_version
        ));
    }
    state
        .validate_invariants()
        .map_err(|e| format!("execution_state invariant check failed on save: {e}"))?;
    control
        .save(thread_id, DATA_ENGINEER_SUITE_ID, state)
        .await
        .map_err(|e| e.to_string())
}

pub async fn mutate_execution_state(
    control: &ControlStateStore,
    thread_id: &str,
    mutate: impl FnOnce(&mut ExecutionState),
) -> Result<ExecutionState, String> {
    let mut st = load_execution_state_strict(control, thread_id)
        .await?
        .unwrap_or_else(ExecutionState::new);
    mutate(&mut st);
    st.validate_invariants()
        .map_err(|e| format!("execution_state invariant check failed after mutation: {e}"))?;
    persist_execution_state(control, thread_id, &st).await?;
    Ok(st)
}

pub async fn replace_execution_state(
    control: &ControlStateStore,
    thread_id: &str,
    next_state: ExecutionState,
) -> Result<ExecutionState, String> {
    mutate_execution_state(control, thread_id, |st| {
        *st = next_state.clone();
    })
    .await
}

pub async fn apply_execution_event(
    control: &ControlStateStore,
    thread_id: &str,
    event: DataEngineerEvent,
) -> Result<ExecutionState, String> {
    mutate_execution_state(control, thread_id, |st| st.apply_event(event))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use react_core::keyspace::DefaultKeyspace;
    use react_core::scope::RequestScope;
    use react_module_storage_memory::InMemoryStorageAdapter;
    use std::sync::Arc;

    fn test_control_store() -> ControlStateStore {
        let storage = Arc::new(InMemoryStorageAdapter::default());
        let scope = RequestScope::parse("t", "w", "p").expect("valid test scope");
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string()));
        ControlStateStore::new(storage, scope, keyspace)
    }

    #[tokio::test]
    async fn replace_execution_state_rejects_invariant_violations() {
        let control = test_control_store();
        let tid = "tid-state-manager-invariant-save";
        let mut st = ExecutionState::new();
        st.repair.repair_mode =
            crate::progress_controller::RepairModeState::SqlTarget(
                crate::progress_controller::SqlTargetRepairMode {
                    target_path: crate::progress_controller::SqlModelPath::parse("models/staging/stg_x.sql".to_string()).expect("valid sql model path"),
                    core: crate::progress_controller::RepairModeCore {
                        ladder_step: crate::progress_controller::RepairLadderStep::Stop,
                        attempt_count: 1,
                        repair_started_mutation_epoch: None,
                        consecutive_noop_patches: 0,
                    },
                },
            );
        let err = replace_execution_state(&control, tid, st)
            .await
            .expect_err("invalid state must fail save");
        assert!(err.contains("invariant check failed"));
    }

    #[tokio::test]
    async fn mutate_execution_state_rejects_invalid_mutator_result() {
        let control = test_control_store();
        let tid = "tid-state-manager-invariant-mutate";
        let err = mutate_execution_state(&control, tid, |st| {
            st.telemetry.last_validate = Some(
                crate::progress_controller::LastValidateState {
                    ok: Some(true),
                    ..crate::progress_controller::LastValidateState::default()
                },
            );
            st.telemetry.probe.required = true;
        })
        .await
        .expect_err("invalid post-mutation state must fail");
        assert!(!err.trim().is_empty(), "error should be non-empty");
    }
}
