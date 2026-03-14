use react_core::session::ControlStateStore;
use react_core::CoreError;
use tracing::warn;

use crate::progress_controller::{
    DataEngineerEvent, ExecutionState, EXECUTION_STATE_SCHEMA_VERSION,
};

pub const DATA_ENGINEER_SUITE_ID: &str = "data_engineer";

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("state storage failed: {0}")]
    StorageFailed(String),
    #[error("state deserialization/validation failed for '{key}': {detail}")]
    DeserializeFailed { key: String, detail: String },
    #[error("state invariant violated: {0}")]
    ValidationFailed(String),
}

impl From<CoreError> for StateError {
    fn from(e: CoreError) -> Self {
        match e {
            CoreError::Serialization(e) => StateError::DeserializeFailed {
                key: String::new(),
                detail: e.to_string(),
            },
            other => StateError::StorageFailed(other.to_string()),
        }
    }
}

fn validate_loaded_state(parsed: &ExecutionState) -> Result<(), StateError> {
    if parsed.schema_version != EXECUTION_STATE_SCHEMA_VERSION {
        return Err(StateError::DeserializeFailed {
            key: "execution_state".into(),
            detail: format!(
                "schema_version mismatch: expected {}, got {}",
                EXECUTION_STATE_SCHEMA_VERSION, parsed.schema_version
            ),
        });
    }
    parsed
        .validate_invariants()
        .map_err(|e| StateError::ValidationFailed(format!("invariant check failed on load: {e}")))
}

pub async fn load_execution_state(
    control: &ControlStateStore,
    thread_id: &str,
) -> Result<Option<ExecutionState>, StateError> {
    let Some(parsed) = control
        .load::<ExecutionState>(thread_id, DATA_ENGINEER_SUITE_ID)
        .await?
    else {
        return Ok(None);
    };
    if let Err(e) = validate_loaded_state(&parsed) {
        warn!(thread_id, error = %e, "dropping execution_state due to schema mismatch or corruption");
        return Ok(None);
    }
    Ok(Some(parsed))
}

pub async fn load_execution_state_strict(
    control: &ControlStateStore,
    thread_id: &str,
) -> Result<Option<ExecutionState>, StateError> {
    let loaded = control
        .load::<ExecutionState>(thread_id, DATA_ENGINEER_SUITE_ID)
        .await?;
    let Some(parsed) = loaded else {
        return Ok(None);
    };
    validate_loaded_state(&parsed)?;
    Ok(Some(parsed))
}

pub async fn mutate_execution_state(
    control: &ControlStateStore,
    thread_id: &str,
    mutate: impl FnOnce(&mut ExecutionState),
) -> Result<ExecutionState, StateError> {
    control
        .mutate::<ExecutionState>(thread_id, DATA_ENGINEER_SUITE_ID, |current| {
            let mut st = current.unwrap_or_else(ExecutionState::new);
            mutate(&mut st);
            st.validate_invariants().map_err(|e| {
                CoreError::Session(format!(
                    "execution_state invariant check failed after mutation: {e}"
                ))
            })?;
            Ok(st)
        })
        .await
        .map_err(StateError::from)
}

pub async fn replace_execution_state(
    control: &ControlStateStore,
    thread_id: &str,
    next_state: ExecutionState,
) -> Result<ExecutionState, StateError> {
    mutate_execution_state(control, thread_id, |st| {
        *st = next_state.clone();
    })
    .await
}

pub async fn apply_execution_event(
    control: &ControlStateStore,
    thread_id: &str,
    event: DataEngineerEvent,
) -> Result<ExecutionState, StateError> {
    mutate_execution_state(control, thread_id, |st| st.apply_event(event)).await
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
        st.repair.repair_mode = crate::progress_controller::RepairModeState::Active(
            crate::progress_controller::RepairMode {
                repair_type: crate::progress_controller::RepairType::SqlTarget,
                target_path: Some(crate::progress_controller::RepairTargetPath::SqlModel(
                    crate::progress_controller::SqlModelPath::parse(
                        "models/staging/stg_x.sql".to_string(),
                    )
                    .expect("valid sql model path"),
                )),
                materialization: crate::progress_controller::RepairTargetMaterialization::Existing,
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
        assert!(
            matches!(
                err,
                StateError::ValidationFailed(_) | StateError::StorageFailed(_)
            ),
            "expected validation or storage error, got: {err}"
        );
    }

    #[tokio::test]
    async fn mutate_execution_state_rejects_invalid_mutator_result() {
        let control = test_control_store();
        let tid = "tid-state-manager-invariant-mutate";
        let err = mutate_execution_state(&control, tid, |st| {
            st.telemetry.last_validate = Some(crate::progress_controller::LastValidateState {
                ok: Some(true),
                ..crate::progress_controller::LastValidateState::default()
            });
            st.telemetry.probe.required = true;
        })
        .await
        .expect_err("invalid post-mutation state must fail");
        assert!(
            !err.to_string().trim().is_empty(),
            "error should be non-empty"
        );
    }
}
