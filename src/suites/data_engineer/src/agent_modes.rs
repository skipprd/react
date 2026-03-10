use super::*;

/// External-facing interaction modes for the data_engineer suite.
///
/// The suite README lists five agent *types* (ask, review, agent, plan, validate),
/// but plan/validate/author/publish are internal phases driven by the `Agent` mode's
/// phase loop (see `execute_phase`). Only these three represent distinct entry points
/// that callers can select via `dispatch_agent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum AgentMode {
    Ask,
    Review,
    Agent,
}

impl std::str::FromStr for AgentMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ask" => Ok(Self::Ask),
            "review" => Ok(Self::Review),
            "agent" => Ok(Self::Agent),
            _ => Err(format!(
                "invalid agent_type '{}' for suite 'data_engineer' (expected 'ask' | 'review' | 'agent')",
                s
            )),
        }
    }
}

impl AgentMode {
    pub(super) fn parse(raw: &str) -> Result<Self, String> {
        raw.parse()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum AgentToolCapability {
    ReadOnlyFile,
    MutableFile,
    RunSql,
    AskUser,
    AskApproval,
    SearchDbtExamples,
    StagingModel,
    GoldModel,
    DbtValidate,
    PublishDbt,
    SqlRegister,
    CatalogNote,
    Artifacts,
}

impl DataEngineerSuite {
    pub(super) fn agent_capability_profile(
        agent_mode: AgentMode,
        allow_user_interrupt_tools: bool,
    ) -> BTreeSet<AgentToolCapability> {
        let mut caps = BTreeSet::new();
        match agent_mode {
            AgentMode::Review => {
                caps.insert(AgentToolCapability::ReadOnlyFile);
                caps.insert(AgentToolCapability::Artifacts);
            }
            AgentMode::Ask => {
                caps.insert(AgentToolCapability::MutableFile);
                caps.insert(AgentToolCapability::RunSql);
                caps.insert(AgentToolCapability::AskApproval);
                caps.insert(AgentToolCapability::Artifacts);
            }
            AgentMode::Agent => {
                caps.insert(AgentToolCapability::MutableFile);
                caps.insert(AgentToolCapability::RunSql);
                caps.insert(AgentToolCapability::AskApproval);
                caps.insert(AgentToolCapability::SearchDbtExamples);
                caps.insert(AgentToolCapability::StagingModel);
                caps.insert(AgentToolCapability::GoldModel);
                caps.insert(AgentToolCapability::DbtValidate);
                caps.insert(AgentToolCapability::PublishDbt);
                caps.insert(AgentToolCapability::SqlRegister);
                caps.insert(AgentToolCapability::CatalogNote);
                caps.insert(AgentToolCapability::Artifacts);
            }
        }
        if allow_user_interrupt_tools && agent_mode != AgentMode::Review {
            caps.insert(AgentToolCapability::AskUser);
        }
        caps
    }

    pub(super) fn headless_mode_enabled() -> bool {
        env_util::headless_mode_enabled()
    }

    pub(super) fn enforce_non_interactive_contract(
        agent_mode: AgentMode,
        frames: Vec<FlowFrame>,
    ) -> Result<Vec<FlowFrame>, String> {
        let await_user_prompt = frames.iter().find_map(|f| match f {
            FlowFrame::Interrupt { kind, prompt } if kind == "await_user" => Some(prompt.clone()),
            _ => None,
        });
        if let Some(prompt) = await_user_prompt {
            if agent_mode == AgentMode::Agent {
                return Err(format!("agent_mode_await_user_forbidden: {}", prompt));
            }
            if Self::headless_mode_enabled() {
                return Err(format!("await_user_forbidden_in_headless: {}", prompt));
            }
        }
        Ok(frames)
    }

    pub(super) fn build_tools_card(
        header: &str,
        tool_lines: Vec<String>,
        notes: Vec<String>,
        not_available: Option<String>,
    ) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push(header.to_string());
        lines.extend(tool_lines);
        if !notes.is_empty() {
            lines.push(String::new());
            lines.extend(notes);
        }
        if let Some(na) = not_available {
            lines.push(String::new());
            lines.push(na);
        }
        lines.join("\n")
    }

    pub(super) async fn check_subjective_retry_budget(
        thread_store: &ThreadStore,
        thread_id: &str,
        kind: crate::progress_controller::SubjectiveRetryKind,
    ) -> Result<crate::retry_budget::SubjectiveRetryOutcome, String> {
        crate::retry_budget::check_subjective_retry_budget(
            thread_store, thread_id, kind,
        ).await
    }

    pub(super) async fn clear_subjective_retries_matching(
        thread_store: &ThreadStore,
        thread_id: &str,
        f: impl Fn(&crate::progress_controller::SubjectiveRetryKind) -> bool,
    ) -> Result<(), String> {
        crate::retry_budget::clear_subjective_retries_matching(
            thread_store, thread_id, f,
        ).await
    }

    pub(super) fn validate_agent_type(agent_type: &str) -> Result<AgentMode, String> {
        AgentMode::parse(agent_type)
    }

    pub(super) fn inject_review_question(question: &str) -> String {
        prompts::shared::user_goal_line("Review request:", question)
    }

    pub(super) fn inject_model_question(question: &str) -> String {
        prompts::shared::user_goal_line("Modeling goal:", question)
    }

    pub(super) fn inject_cleanse_question(question: &str) -> String {
        prompts::shared::user_goal_line("Cleansing goal:", question)
    }

    pub(super) async fn run_ask(
        thread_id: &str,
        question: &str,
        sctx: &SuiteCtx,
    ) -> Result<Vec<FlowFrame>, String> {
        let sys = prompts::with_time_context(prompts::ask_system_prompt());
        let tools_card = Self::build_tools_card_for_agent_type(AgentMode::Ask);

        let pf = preflight::CatalogPreflightProvider {
            discovery_limits: preflight::discovery::DiscoveryLimits::default(),
        };
        let bundle = pf.run(thread_id, question, "ask", sctx).await.discovery;

        let registry = Self::build_tools(AgentMode::Ask, sctx)?;
        let actx = Self::build_agent_ctx(
            sctx,
            thread_id,
            "ask",
            std::sync::Arc::new(SqlValidatedPolicy {
                dataset_candidates: bundle
                    .datasets
                    .iter()
                    .take(8)
                    .map(|(ds, sc)| DatasetCandidate {
                        dataset_id: ds.clone(),
                        score: *sc,
                    })
                    .collect(),
                ..SqlValidatedPolicy::default()
            }),
            env_util::ASK_MAX_STEPS,
            10,
        );

        Self::run_outcome_to_frames(
            Agent::run_until_block(
                &registry,
                &actx,
                &sys,
                &tools_card,
                question,
                LlmCallOptions {
                    prompt_id: "data_engineer.ask_user_parse",
                    thread_id: None,
                    expected_format: react_core::llm::LlmExpectedFormat::JsonObject,
                    max_output_tokens: None,
                    temperature: None,
                    top_p: None,
                    reasoning_effort: None,
                    timeout_secs: None,
                },
            )
            .await
            .map_err(|e| e.to_string()),
        )
    }

    pub(super) async fn run_review(
        thread_id: &str,
        question: &str,
        sctx: &SuiteCtx,
    ) -> Result<Vec<FlowFrame>, String> {
        Self::ensure_catalog_bootstrap_semaphored(thread_id, sctx).await?;
        let sys = prompts::with_time_context(prompts::review_system_prompt());
        let tools_card = Self::build_tools_card_for_agent_type(AgentMode::Review);

        let registry = Self::build_tools(AgentMode::Review, sctx)?;
        let actx = Self::build_agent_ctx(
            sctx,
            thread_id,
            "review",
            std::sync::Arc::new(react_core::agent::DefaultPolicy),
            env_util::REVIEW_MAX_STEPS,
            10,
        );

        let prompt = Self::inject_review_question(question);
        Self::run_outcome_to_frames(
            Agent::run_until_block(
                &registry,
                &actx,
                &sys,
                &tools_card,
                &prompt,
                LlmCallOptions {
                    prompt_id: "data_engineer.ask_approval_parse",
                    thread_id: None,
                    expected_format: react_core::llm::LlmExpectedFormat::JsonObject,
                    max_output_tokens: None,
                    temperature: None,
                    top_p: None,
                    reasoning_effort: None,
                    timeout_secs: None,
                },
            )
            .await
            .map_err(|e| e.to_string()),
        )
    }

    pub(super) fn build_agent_ctx(
        sctx: &SuiteCtx,
        thread_id: &str,
        agent_name: &str,
        policy: std::sync::Arc<dyn react_core::agent::AgentPolicy>,
        max_steps: usize,
        per_step_timeout: u64,
    ) -> AgentCtx {
        let thread_store = ThreadStore::new(
            sctx.storage().clone(),
            sctx.scope().clone(),
            sctx.keyspace().clone(),
        );
        let mut actx = react_core::agent::AgentCtxBuilder::new(
                sctx.llm().clone(),
                sctx.storage().clone(),
                sctx.scope().clone(),
                sctx.keyspace().clone(),
                policy,
            )
            .top_k(env_util::DEFAULT_TOP_K)
            .per_step_timeout_secs(per_step_timeout)
            .max_steps(max_steps)
            .thread_id(thread_id)
            .trace_tx(sctx.trace_tx().clone())
            .agent_name(agent_name)
            .vector(sctx.vector().clone())
            .thread_store(thread_store)
            .resolved_config(sctx.resolved_config().clone())
            .build();
        crate::ctx_ext::copy_capabilities_to_actx(sctx, &mut actx);
        actx
    }

    pub(super) fn run_outcome_to_frames(
        outcome: Result<RunOutcome, String>,
    ) -> Result<Vec<FlowFrame>, String> {
        match outcome {
            Ok(RunOutcome::Complete { thread_id: _tid, result }) => Ok(vec![FlowFrame::Complete {
                kind: FlowKind::new(result.kind.clone()),
                payload: result.payload,
                display: result.display,
            }]),
            Ok(RunOutcome::Interrupt { thread_id: _tid, kind, prompt }) => {
                let kind_str = match kind {
                    react_core::agent::InterruptKind::AwaitUser => "await_user",
                    react_core::agent::InterruptKind::AwaitApproval => "await_approval",
                };
                Ok(vec![FlowFrame::Interrupt { kind: FlowKind::new(kind_str), prompt }])
            }
            Err(e) => Err(e),
        }
    }

    pub(super) fn agent_tool_ctx(thread_id: &str, sctx: &SuiteCtx) -> AgentCtx {
        Self::build_agent_ctx(
            sctx,
            thread_id,
            env_util::DEFAULT_AGENT_NAME,
            std::sync::Arc::new(react_core::agent::DefaultPolicy),
            env_util::APPROVAL_PARSE_MAX_STEPS,
            10,
        )
    }

    pub(super) fn plan_agent_ctx(thread_id: &str, sctx: &SuiteCtx) -> AgentCtx {
        Self::build_agent_ctx(
            sctx,
            thread_id,
            env_util::DEFAULT_AGENT_NAME,
            std::sync::Arc::new(InterruptOnlyPolicy),
            env_util::PLAN_MAX_STEPS,
            20,
        )
    }

    pub(super) async fn run_agent(
        thread_id: &str,
        question: &str,
        sctx: &SuiteCtx,
    ) -> Result<Vec<FlowFrame>, String> {
        use control_flow::{DerivedGuardState, Phase};
        if let Err(e) = Self::ensure_catalog_bootstrap_semaphored(thread_id, sctx).await {
            let thread_store = ThreadStore::new(
                sctx.storage().clone(),
                sctx.scope().clone(),
                sctx.keyspace().clone(),
            );
            let reason = format!(
                "catalog bootstrap metadata gate failed before planning:\n{}",
                e.trim()
            );
            tracing::error!(
                thread_id = %thread_id,
                error = %e,
                "data_engineer: refusing to continue after preflight catalog metadata gate failure"
            );
            let _ = apply_guard_block(
                &thread_store,
                thread_id,
                Phase::Preflight,
                GuardBlockKind::PrecheckFailed,
                reason.clone(),
            )
            .await;
            return Err(format!("agent_mode_await_user_forbidden: {}", reason));
        }

        // Phase-step budget is reset when we make clear forward progress (phase advances).
        // This prevents aborting a healthy thread that is steadily moving through phases,
        // while still bounding degenerate loops.
        let max_phase_steps: usize = env_util::max_phase_steps();
        let max_replan_backtracks: usize = control_flow::replan_backtrack_counter_cap();

        let thread_store = ThreadStore::new(
            sctx.storage().clone(),
            sctx.scope().clone(),
            sctx.keyspace().clone(),
        );

        let mut out_frames: Vec<FlowFrame> = Vec::new();

        let mut remaining_steps = max_phase_steps;
        let mut total_steps: usize = 0;
        let mut max_phase_idx_seen: usize = 0;

        while remaining_steps > 0 {
            total_steps += 1;
            remaining_steps = remaining_steps.saturating_sub(1);

            let execution_state = crate::progress_controller::ExecutionState::load_strict(
                &thread_store.control_store(),
                thread_id,
            )
            .await?
            .unwrap_or_else(crate::progress_controller::ExecutionState::new);
            let phase = execution_state.phase.current_phase.unwrap_or(control_flow::Phase::Preflight);
            let thread_state_step_count = thread_store
                .get(thread_id)
                .await
                .map(|log| log.steps.len())
                .unwrap_or(0);
            let idx = phase.ordinal();
            if idx > max_phase_idx_seen {
                max_phase_idx_seen = idx;
                // Reset the budget when we advance phases (i.e. not looping).
                remaining_steps = max_phase_steps;
            }
            let guard: DerivedGuardState =
                control_flow::derive_guard_state_from_execution_state(&execution_state);
            match crate::phase_gate::evaluate_pre_turn_directive(
                &execution_state,
                phase,
                max_replan_backtracks,
            ) {
                crate::phase_gate::PreTurnDirective::Proceed => {}
                crate::phase_gate::PreTurnDirective::FailFast { kind, reason } => {
                    crate::transition_dispatcher::apply_phase_directive(
                        &thread_store,
                        thread_id,
                        Some(env_util::DEFAULT_AGENT_NAME.to_string()),
                        Some(phase),
                        crate::transition_dispatcher::PhaseDirective::Block {
                            phase,
                            kind,
                            reason: reason.clone(),
                        },
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                    let mut es = execution_state.clone();
                    es.mark_failed(reason.clone());
                    es.save(&thread_store.control_store(), thread_id).await.map_err(|e| {
                        format!("failed to persist fail-fast mark_failed: {e}")
                    })?;
                    return Err(reason);
                }
            }

            let repair_ctx = execution_state.repair_prompt_context();
            match Self::execute_phase(
                &thread_store,
                thread_id,
                phase,
                question,
                sctx,
                &execution_state,
                &guard,
                thread_state_step_count,
                &repair_ctx,
                &mut out_frames,
            )
            .await {
                PhaseExecutorOutcome::StayedWithProgress { detail } => {
                    tracing::debug!(thread_id = %thread_id, phase_progress = %detail);
                    remaining_steps = max_phase_steps;
                    continue;
                }
                PhaseExecutorOutcome::StayedWaiting { reason } => {
                    tracing::debug!(thread_id = %thread_id, phase_waiting = %reason);
                    continue;
                }
                PhaseExecutorOutcome::TransitionCommitted => continue,
                PhaseExecutorOutcome::Return(frames) => return Ok(frames),
                PhaseExecutorOutcome::Failed { reason } => return Err(reason),
            }
        }

        let mut budget_msg = format!(
            "headless_budget_exhausted: Agent reached the phase-step budget without completing.\n\nBudget:\n- max_steps_per_progress={max_phase_steps}\n- total_steps={total_steps}\n\nThis indicates a loop (re-entering phases without durable progress)."
        );
        if let Some(mut es) =
            crate::progress_controller::ExecutionState::load(&thread_store.control_store(), thread_id)
                .await
                .unwrap_or_else(|e| {
                    tracing::error!("failed to load execution state at budget exhaustion: {e}");
                    None
                })
        {
            budget_msg.push_str(&format!(
                "\n\nExecution state at exhaustion:\n- current_phase={}\n- mode={}\n- phase_reason_code={}\n- replan_backtracks={}\n- stall_count={}/{}\n- hard_mutation_repair_mode={}",
                es.phase.current_phase.map(|p| p.as_str().to_string()).unwrap_or_else(|| "null".to_string()),
                format!("{:?}", es.phase.mode),
                es.phase.phase_reason_code.map(|c| c.as_str().to_string()).unwrap_or_else(|| "null".to_string()),
                es.phase.replan_backtracks,
                es.repair.stall_count,
                crate::progress_controller::DEFAULT_MAX_STALL_COUNT,
                es.hard_mutation_repair_mode(),
            ));
            es.mark_failed(budget_msg.clone());
            if let Err(e) = es.save(&thread_store.control_store(), thread_id).await {
                tracing::error!("failed to persist budget-exhaustion mark_failed: {e}");
            }
        }
        Err(budget_msg)
    }

    /// Infallible phase dispatcher. Individual phase executors may return
    /// `Err(String)` internally; this boundary catches every such error,
    /// records a `PhaseExecutionError` guard block + `mark_failed` in
    /// ControlState, and converts the error into `PhaseExecutorOutcome::Failed`.
    ///
    /// Because the return type carries no `Result`, the caller (`run_agent`)
    /// cannot use `?` to silently discard errors — the compiler forces an
    /// exhaustive match on the `Failed` variant.
    pub(super) async fn execute_phase(
        thread_store: &ThreadStore,
        thread_id: &str,
        phase: crate::control_flow::Phase,
        question: &str,
        sctx: &SuiteCtx,
        execution_state: &crate::progress_controller::ExecutionState,
        guard: &control_flow::DerivedGuardState,
        thread_state_step_count: usize,
        repair_ctx: &crate::progress_controller::RepairPromptContext,
        out_frames: &mut Vec<FlowFrame>,
    ) -> PhaseExecutorOutcome {
        match Self::execute_phase_inner(
            thread_store, thread_id, phase, question, sctx,
            execution_state, guard, thread_state_step_count,
            repair_ctx, out_frames,
        ).await {
            Ok(outcome) => outcome,
            Err(e) => {
                let reason = e.to_string();
                tracing::error!(
                    thread_id = %thread_id,
                    phase = %phase.as_str(),
                    error = %reason,
                    "phase execution error — recording guard block",
                );
                let _ = apply_guard_block(
                    thread_store, thread_id, phase,
                    GuardBlockKind::PhaseExecutionError,
                    &reason,
                ).await;
                match crate::progress_controller::ExecutionState::load(
                    &thread_store.control_store(), thread_id,
                ).await {
                    Ok(Some(mut es)) => {
                        es.mark_failed(&reason);
                        if let Err(e) = es.save(&thread_store.control_store(), thread_id).await {
                            tracing::error!("failed to persist mark_failed after phase error: {e}");
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::error!("failed to load execution state after phase error: {e}");
                    }
                }
                PhaseExecutorOutcome::Failed { reason }
            }
        }
    }

    async fn execute_phase_inner(
        thread_store: &ThreadStore,
        thread_id: &str,
        phase: crate::control_flow::Phase,
        question: &str,
        sctx: &SuiteCtx,
        execution_state: &crate::progress_controller::ExecutionState,
        guard: &control_flow::DerivedGuardState,
        thread_state_step_count: usize,
        repair_ctx: &crate::progress_controller::RepairPromptContext,
        out_frames: &mut Vec<FlowFrame>,
    ) -> Result<PhaseExecutorOutcome, PhaseError> {
        use crate::control_flow::Phase;
        match phase {
            Phase::Preflight => {
                Self::execute_preflight_phase(thread_store, thread_id, sctx).await
            }
            Phase::CleansePlan | Phase::ModelPlan => {
                Self::execute_plan_phase(
                    thread_store,
                    thread_id,
                    phase,
                    question,
                    sctx,
                    execution_state,
                    guard,
                    thread_state_step_count,
                    repair_ctx,
                )
                .await
            }
            Phase::CleanseAuthor | Phase::ModelAuthor => {
                Self::execute_author_phase(
                    thread_store,
                    thread_id,
                    phase,
                    question,
                    sctx,
                    execution_state,
                    guard,
                    thread_state_step_count,
                    repair_ctx,
                )
                .await
            }
            Phase::CleanseValidate | Phase::ModelValidate => {
                Self::execute_validate_phase(
                    thread_store,
                    thread_id,
                    phase,
                    question,
                    sctx,
                    execution_state,
                    guard,
                    thread_state_step_count,
                )
                .await
            }
            Phase::CleanseReview | Phase::ModelReview | Phase::PostPublishReview => {
                Self::execute_review_phase(
                    thread_store,
                    thread_id,
                    phase,
                    question,
                    sctx,
                    execution_state,
                    thread_state_step_count,
                    out_frames,
                )
                .await
            }
            Phase::PublishAwaitApproval => {
                Self::execute_publish_await_approval_phase(thread_store, thread_id, sctx).await
            }
            Phase::Publish => {
                Self::execute_publish_phase(thread_store, thread_id, sctx).await
            }
            Phase::Done => {
                Self::execute_done_phase(out_frames)
            }
        }
    }

    pub(super) async fn dispatch_agent(
        &self,
        thread_id: &str,
        question: &str,
        agent_type: &str,
        ctx: &SuiteCtx,
    ) -> Result<Vec<FlowFrame>, String> {
        let agent_mode = Self::validate_agent_type(agent_type)?;
        let frames = match agent_mode {
            AgentMode::Agent => Self::run_agent(thread_id, question, ctx).await,
            AgentMode::Review => Self::run_review(thread_id, question, ctx).await,
            AgentMode::Ask => Self::run_ask(thread_id, question, ctx).await,
        }?;
        Self::enforce_non_interactive_contract(agent_mode, frames)
    }
}
