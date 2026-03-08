use super::*;

impl DataEngineerSuite {
    pub(super) fn plan_enrich_chunk_size() -> usize {
        env_util::plan_enrich_chunk_size()
    }

    pub(super) fn is_placeholder_cleanse_spec(spec: Option<&crate::plan::CleanseImplementationSpec>) -> bool {
        spec.is_none()
    }

    pub(super) fn is_placeholder_model_spec(spec: Option<&crate::plan::ModelImplementationSpec>) -> bool {
        spec.is_none()
    }

    pub(super) fn apply_cleanse_enrichment_items(
        plan: &mut crate::plan::CleansePlan,
        allowed_task_ids: &[String],
        items: Vec<crate::plan_schema::CleansePlanEnrichmentItemV1>,
    ) -> (Vec<String>, Vec<String>) {
        let mut failed: Vec<String> = Vec::new();
        let mut failure_errors: Vec<String> = Vec::new();
        for it in items {
            if !allowed_task_ids.iter().any(|t| t == &it.task_id) {
                continue;
            }
            let spec_value = match serde_json::to_value(&it.implementation_spec) {
                Ok(v) => v,
                Err(e) => {
                    failed.push(it.task_id);
                    failure_errors.push(e.to_string());
                    continue;
                }
            };
            match Self::parse_impl_spec_value_with_sanitize::<
                crate::plan::CleanseImplementationSpec,
            >(spec_value, TrackKind::Cleanse)
            {
                Ok((spec, stripped)) => {
                    if !stripped.is_empty() {
                        Self::push_snapshot_array_event(
                            &mut plan.project_snapshot,
                            "spec_sanitizer_events",
                            serde_json::json!({
                                "phase": "cleanse_enrich",
                                "task_id": it.task_id,
                                "stripped_keys": stripped
                            }),
                            200,
                        );
                    }
                    if let Some(t) = plan.tasks.iter_mut().find(|t| t.dataset_id == it.task_id) {
                        t.implementation_spec = Some(spec);
                    }
                }
                Err(e) => {
                    failed.push(it.task_id);
                    failure_errors.push(e);
                }
            }
        }
        (failed, failure_errors)
    }

    pub(super) fn apply_model_enrichment_items(
        plan: &mut crate::plan::ModelPlan,
        allowed_task_ids: &[String],
        items: Vec<crate::plan_schema::ModelPlanEnrichmentItemV1>,
    ) -> (Vec<String>, Vec<String>) {
        let mut failed: Vec<String> = Vec::new();
        let mut failure_errors: Vec<String> = Vec::new();
        for it in items {
            if !allowed_task_ids.iter().any(|t| t == &it.task_id) {
                continue;
            }
            let spec_value = match serde_json::to_value(&it.implementation_spec) {
                Ok(v) => v,
                Err(e) => {
                    failed.push(it.task_id);
                    failure_errors.push(e.to_string());
                    continue;
                }
            };
            match Self::parse_impl_spec_value_with_sanitize::<
                crate::plan::ModelImplementationSpec,
            >(spec_value, TrackKind::Model)
            {
                Ok((spec, stripped)) => {
                    if !stripped.is_empty() {
                        Self::push_snapshot_array_event(
                            &mut plan.project_snapshot,
                            "spec_sanitizer_events",
                            serde_json::json!({
                                "phase": "model_enrich",
                                "task_id": it.task_id,
                                "stripped_keys": stripped
                            }),
                            200,
                        );
                    }
                    if let Some(t) = plan.tasks.iter_mut().find(|t| t.name == it.task_id) {
                        let spec_inputs = spec.inputs.clone();
                        t.implementation_spec = Some(spec);
                        if !spec_inputs.is_empty() {
                            t.inputs = spec_inputs;
                        }
                        if t.goal.trim().is_empty() {
                            t.goal =
                                format!("Build {} from grounded staging inputs.", t.name.trim());
                        }
                    }
                }
                Err(e) => {
                    failed.push(it.task_id);
                    failure_errors.push(e);
                }
            }
        }
        (failed, failure_errors)
    }

    pub(super) fn build_enrichment_prompt_envelope(
        phase: control_flow::Phase,
        directive: crate::prompt_packets::TurnDirective,
        plan_kind: crate::plan_kind::PlanKind,
        plan_key: &str,
        planning_context: &str,
        memo: &str,
        critique: &crate::plan_schema::PlanDesignCritiqueV1,
        plan_summary: &str,
        task_ids: &[String],
        new_evidence_refs: &[String],
    ) -> String {
        let context_text = format!(
            "Context:\n{}\n\nDesign memo:\n{}\n\n{}\n\nCurrent plan summary:\n{}",
            Self::excerpt(planning_context, 20_000),
            Self::excerpt(memo, 12_000),
            Self::critique_guidance(critique),
            plan_summary
        );
        let envelope = crate::prompt_packets::PromptEnvelope {
            phase,
            goal: "Produce implementation_spec content for target tasks".to_string(),
            directive,
            plan: Some(crate::prompt_packets::PlanContextPacket {
                plan_kind: Some(plan_kind),
                plan_key: Some(plan_key.to_string()),
                context_text: Some(context_text),
                unresolved_ids: task_ids.to_vec(),
                new_evidence_refs: new_evidence_refs.to_vec(),
            }),
            batch: None,
            repair: None,
        };
        crate::prompt_packets::render_envelope(&envelope)
            .unwrap_or_else(|_| "{}".to_string())
    }

    pub(super) fn compile_prompt_from_reason(reason_memo: &str, base_user: &str) -> String {
        format!(
            "Reason memo:\n{}\n\n{}",
            Self::excerpt(reason_memo, 8_000),
            base_user
        )
    }

    // TODO(item-86): enrich_cleanse_tasks and enrich_model_tasks below are near-identical
    // (~165 lines each). Extract a generic `enrich_tasks<P: EnrichablePlan>(...)` function
    // parameterized over the plan type, using a trait to abstract summarize, apply_items,
    // system_prompt, schema_spec, prompt_ids, and unresolved-check. The apply_* methods
    // can remain track-specific since model has extra inputs/goal logic.
    pub(super) async fn enrich_cleanse_tasks(
        ctx: &AgentCtx,
        planning_context: &str,
        memo: &str,
        critique: &crate::plan_schema::PlanDesignCritiqueV1,
        plan: &mut crate::plan::CleansePlan,
        task_ids: &[String],
    ) -> Result<(), String> {
        use react_core::llm::{ChatMessage, ChatRole};
        for chunk in task_ids.chunks(Self::plan_enrich_chunk_size()) {
            let chunk_vec = chunk.to_vec();
            let summary = crate::plan::summarize_cleanse_plan(plan, 50);
            let base_user = format!(
                "{}\n\nTarget task_ids:\n{}\n\nReturn schema-valid enrichment JSON.",
                Self::build_enrichment_prompt_envelope(
                    control_flow::Phase::CleansePlan,
                    crate::prompt_packets::TurnDirective::Compile,
                    crate::plan_kind::PlanKind::Cleanse,
                    &plan.plan_key,
                    planning_context,
                    memo,
                    critique,
                    &summary,
                    &chunk_vec,
                    &[],
                ),
                serde_json::to_string_pretty(&chunk_vec).unwrap_or_else(|_| "[]".to_string())
            );
            let reason_user = format!(
                "Think through the enrichment strategy for these task_ids. Return plain text only, no JSON.\n\n{}",
                Self::build_enrichment_prompt_envelope(
                    control_flow::Phase::CleansePlan,
                    crate::prompt_packets::TurnDirective::Reason,
                    crate::plan_kind::PlanKind::Cleanse,
                    &plan.plan_key,
                    planning_context,
                    memo,
                    critique,
                    &summary,
                    &chunk_vec,
                    &[],
                )
            );
            let reason_memo = ctx.llm_chat(
                &[
                    ChatMessage {
                        role: ChatRole::System,
                        content: prompts::plan::plan_enrichment_reason_system_prompt(),
                    },
                    ChatMessage {
                        role: ChatRole::User,
                        content: reason_user,
                    },
                ],
                &Self::planning_llm_options(
                    PlanningLlmProfile::EnrichmentReason,
                    "data_engineer.cleanse_plan_enrich_reason",
                    ctx.thread_id().clone(),
                )?,
            )
            .await.map_err(|e| e.to_string())?;
            let compile_user = Self::compile_prompt_from_reason(&reason_memo, &base_user);
            let mut opts = Self::planning_llm_options(
                PlanningLlmProfile::EnrichmentCompile,
                "data_engineer.cleanse_plan_enrich",
                ctx.thread_id().clone(),
            )?;
            opts.expected_format = react_core::llm::LlmExpectedFormat::JsonSchemaSpec {
                name: "suite.cleanse_plan_enrichment.v1".to_string(),
                schema: crate::plan_schema::strict_schema_for::<
                    crate::plan_schema::CleansePlanEnrichmentV1,
                >()?,
            };
            let raw = ctx.llm_chat(
                &[
                    ChatMessage {
                        role: ChatRole::System,
                        content: prompts::plan::cleanse_plan_enrichment_system_prompt(),
                    },
                    ChatMessage {
                        role: ChatRole::User,
                        content: compile_user,
                    },
                ],
                &opts,
            )
            .await.map_err(|e| e.to_string())?;
            let enrich = Self::parse_json_typed_strict::<
                crate::plan_schema::CleansePlanEnrichmentV1,
            >(&raw)?;
            let (failed, failure_errors) =
                Self::apply_cleanse_enrichment_items(plan, &chunk_vec, enrich.items);
            if !failed.is_empty() {
                let retry_hint = format!(
                    "You previously returned invalid implementation_spec.\nErrors:\n{}\nOnly emit implementation_spec object with keys: spec_version,row_preserving,output_fields,prohibited_ops.\noutput_fields[].kind MUST be exactly one of: raw, clean, derived, quality_flag.\nEach output_fields item MUST include name, kind, expression.\nDo not use synonyms like passthrough/source/base/quality.\nNo wrappers, no extra fields.",
                    failure_errors.join("\n")
                );
                let retry_user = format!(
                    "{}\n\nTarget task_ids:\n{}\n\nSTRICT RETRY REQUIREMENTS:\n{}\n\nReturn schema-valid enrichment JSON.",
                    Self::build_enrichment_prompt_envelope(
                        control_flow::Phase::CleansePlan,
                        crate::prompt_packets::TurnDirective::Verify,
                        crate::plan_kind::PlanKind::Cleanse,
                        &plan.plan_key,
                        planning_context,
                        memo,
                        critique,
                        &crate::plan::summarize_cleanse_plan(plan, 50),
                        &failed,
                        &failure_errors,
                    ),
                    serde_json::to_string_pretty(&failed).unwrap_or_else(|_| "[]".to_string()),
                    retry_hint
                );
                let retry_opts = LlmCallOptions {
                    prompt_id: "data_engineer.cleanse_plan_enrich_retry",
                    ..opts
                };
                let retry_raw = ctx.llm_chat(
                    &[
                        ChatMessage {
                            role: ChatRole::System,
                            content: prompts::plan::cleanse_plan_enrichment_system_prompt(),
                        },
                        ChatMessage {
                            role: ChatRole::User,
                            content: retry_user,
                        },
                    ],
                    &retry_opts,
                )
                .await.map_err(|e| e.to_string())?;
                let retry_enrich = Self::parse_json_typed_strict::<
                    crate::plan_schema::CleansePlanEnrichmentV1,
                >(&retry_raw)?;
                let (retry_failed, retry_errors) =
                    Self::apply_cleanse_enrichment_items(plan, &failed, retry_enrich.items);
                if !retry_failed.is_empty() {
                    return Err(format!(
                        "cleanse enrichment invalid after bounded retry for task_ids={}: {}",
                        retry_failed.join(","),
                        retry_errors.join(" | ")
                    ));
                }
            }
        }
        let unresolved: Vec<String> = task_ids
            .iter()
            .filter(|task_id| {
                plan.tasks
                    .iter()
                    .find(|t| t.dataset_id.as_str() == task_id.as_str())
                    .map(|t| Self::is_placeholder_cleanse_spec(t.implementation_spec.as_ref()))
                    .unwrap_or(true)
            })
            .cloned()
            .collect();
        if !unresolved.is_empty() {
            return Err(format!(
                "cleanse enrichment did not produce implementation_spec for task_ids={}",
                unresolved.join(",")
            ));
        }
        Ok(())
    }

    pub(super) async fn enrich_model_tasks(
        ctx: &AgentCtx,
        planning_context: &str,
        memo: &str,
        critique: &crate::plan_schema::PlanDesignCritiqueV1,
        plan: &mut crate::plan::ModelPlan,
        task_ids: &[String],
    ) -> Result<(), String> {
        use react_core::llm::{ChatMessage, ChatRole};
        for chunk in task_ids.chunks(Self::plan_enrich_chunk_size()) {
            let chunk_vec = chunk.to_vec();
            let summary = crate::plan::summarize_model_plan(plan, 50);
            let base_user = format!(
                "{}\n\nTarget task_ids:\n{}\n\nReturn schema-valid enrichment JSON.",
                Self::build_enrichment_prompt_envelope(
                    control_flow::Phase::ModelPlan,
                    crate::prompt_packets::TurnDirective::Compile,
                    crate::plan_kind::PlanKind::Model,
                    &plan.plan_key,
                    planning_context,
                    memo,
                    critique,
                    &summary,
                    &chunk_vec,
                    &[],
                ),
                serde_json::to_string_pretty(&chunk_vec).unwrap_or_else(|_| "[]".to_string())
            );
            let reason_user = format!(
                "Think through the enrichment strategy for these task_ids. Return plain text only, no JSON.\n\n{}",
                Self::build_enrichment_prompt_envelope(
                    control_flow::Phase::ModelPlan,
                    crate::prompt_packets::TurnDirective::Reason,
                    crate::plan_kind::PlanKind::Model,
                    &plan.plan_key,
                    planning_context,
                    memo,
                    critique,
                    &summary,
                    &chunk_vec,
                    &[],
                )
            );
            let reason_memo = ctx.llm_chat(
                &[
                    ChatMessage {
                        role: ChatRole::System,
                        content: prompts::plan::plan_enrichment_reason_system_prompt(),
                    },
                    ChatMessage {
                        role: ChatRole::User,
                        content: reason_user,
                    },
                ],
                &Self::planning_llm_options(
                    PlanningLlmProfile::EnrichmentReason,
                    "data_engineer.model_plan_enrich_reason",
                    ctx.thread_id().clone(),
                )?,
            )
            .await.map_err(|e| e.to_string())?;
            let compile_user = Self::compile_prompt_from_reason(&reason_memo, &base_user);
            let mut opts = Self::planning_llm_options(
                PlanningLlmProfile::EnrichmentCompile,
                "data_engineer.model_plan_enrich",
                ctx.thread_id().clone(),
            )?;
            opts.expected_format = react_core::llm::LlmExpectedFormat::JsonSchemaSpec {
                name: "suite.model_plan_enrichment.v1".to_string(),
                schema: crate::plan_schema::strict_schema_for::<
                    crate::plan_schema::ModelPlanEnrichmentV1,
                >()?,
            };
            let raw = ctx.llm_chat(
                &[
                    ChatMessage {
                        role: ChatRole::System,
                        content: prompts::plan::model_plan_enrichment_system_prompt(),
                    },
                    ChatMessage {
                        role: ChatRole::User,
                        content: compile_user,
                    },
                ],
                &opts,
            )
            .await.map_err(|e| e.to_string())?;
            let enrich = Self::parse_json_typed_strict::<
                crate::plan_schema::ModelPlanEnrichmentV1,
            >(&raw)?;
            let (failed, failure_errors) =
                Self::apply_model_enrichment_items(plan, &chunk_vec, enrich.items);
            if !failed.is_empty() {
                let retry_hint = format!(
                    "You previously returned invalid implementation_spec.\nErrors:\n{}\nOnly emit implementation_spec object with keys: spec_version,grain,inputs,joins,metrics,output_fields,assumptions.\noutput_fields[].kind MUST be exactly one of: raw, clean, derived, quality_flag.\nEach output_fields item MUST include name, kind, expression.\nDo not use synonyms like passthrough/source/base/quality.\nNo wrappers, no extra fields.",
                    failure_errors.join("\n")
                );
                let retry_user = format!(
                    "{}\n\nTarget task_ids:\n{}\n\nSTRICT RETRY REQUIREMENTS:\n{}\n\nReturn schema-valid enrichment JSON.",
                    Self::build_enrichment_prompt_envelope(
                        control_flow::Phase::ModelPlan,
                        crate::prompt_packets::TurnDirective::Verify,
                        crate::plan_kind::PlanKind::Model,
                        &plan.plan_key,
                        planning_context,
                        memo,
                        critique,
                        &crate::plan::summarize_model_plan(plan, 50),
                        &failed,
                        &failure_errors,
                    ),
                    serde_json::to_string_pretty(&failed).unwrap_or_else(|_| "[]".to_string()),
                    retry_hint
                );
                let retry_opts = LlmCallOptions {
                    prompt_id: "data_engineer.model_plan_enrich_retry",
                    ..opts
                };
                let retry_raw = ctx.llm_chat(
                    &[
                        ChatMessage {
                            role: ChatRole::System,
                            content: prompts::plan::model_plan_enrichment_system_prompt(),
                        },
                        ChatMessage {
                            role: ChatRole::User,
                            content: retry_user,
                        },
                    ],
                    &retry_opts,
                )
                .await.map_err(|e| e.to_string())?;
                let retry_enrich = Self::parse_json_typed_strict::<
                    crate::plan_schema::ModelPlanEnrichmentV1,
                >(&retry_raw)?;
                let (retry_failed, retry_errors) =
                    Self::apply_model_enrichment_items(plan, &failed, retry_enrich.items);
                if !retry_failed.is_empty() {
                    return Err(format!(
                        "model enrichment invalid after bounded retry for task_ids={}: {}",
                        retry_failed.join(","),
                        retry_errors.join(" | ")
                    ));
                }
            }
        }
        let unresolved: Vec<String> = task_ids
            .iter()
            .filter(|task_id| {
                plan.tasks
                    .iter()
                    .find(|t| t.name.as_str() == task_id.as_str())
                    .map(|t| Self::is_placeholder_model_spec(t.implementation_spec.as_ref()))
                    .unwrap_or(true)
            })
            .cloned()
            .collect();
        if !unresolved.is_empty() {
            return Err(format!(
                "model enrichment did not produce implementation_spec for task_ids={}",
                unresolved.join(",")
            ));
        }
        Ok(())
    }

    pub(super) async fn generate_design_memo(
        ctx: &AgentCtx,
        track: TrackKind,
        planning_context: &str,
    ) -> Result<String, String> {
        use react_core::llm::{ChatMessage, ChatRole};
        let kind = if track.is_cleanse() {
            "cleanse_plan"
        } else {
            "model_plan"
        };
        let sys = prompts::plan::plan_design_memo_system_prompt(kind);
        let user = format!(
            "Planning kind: {kind}\n\nContext:\n{}\n\nWrite the design memo.",
            Self::excerpt(planning_context, 120_000)
        );
        let opts = Self::planning_llm_options(
            PlanningLlmProfile::DesignMemo,
            "data_engineer.plan_design_memo",
            ctx.thread_id().clone(),
        )?;
        ctx.llm()
            .chat(
                &[
                    ChatMessage {
                        role: ChatRole::System,
                        content: sys,
                    },
                    ChatMessage {
                        role: ChatRole::User,
                        content: user,
                    },
                ],
                &opts,
            )
            .map_err(|e| e.to_string())
    }

    pub(super) async fn critique_design_memo(
        ctx: &AgentCtx,
        track: TrackKind,
        planning_context: &str,
        memo: &str,
    ) -> Result<crate::plan_schema::PlanDesignCritiqueV1, String> {
        use react_core::llm::{ChatMessage, ChatRole};
        let kind = if track.is_cleanse() {
            "cleanse_plan"
        } else {
            "model_plan"
        };
        let opts = Self::planning_llm_options(
            PlanningLlmProfile::DesignCritique,
            "data_engineer.plan_design_critique",
            ctx.thread_id().clone(),
        )?;
        let sys = prompts::plan::plan_design_critique_system_prompt(kind);
        let user = format!(
            "Planning kind: {kind}\n\nContext:\n{}\n\nDesign memo:\n{}\n\nReturn critique JSON.",
            Self::excerpt(planning_context, 80_000),
            Self::excerpt(memo, 40_000)
        );
        let raw = ctx.llm_chat(
            &[
                ChatMessage {
                    role: ChatRole::System,
                    content: sys,
                },
                ChatMessage {
                    role: ChatRole::User,
                    content: user,
                },
            ],
            &opts,
        )
        .await.map_err(|e| e.to_string())?;
        Self::parse_json_typed_strict::<crate::plan_schema::PlanDesignCritiqueV1>(&raw)
    }

    pub(super) async fn revise_design_memo(
        ctx: &AgentCtx,
        track: TrackKind,
        planning_context: &str,
        memo: &str,
        critique: &crate::plan_schema::PlanDesignCritiqueV1,
    ) -> Result<String, String> {
        use react_core::llm::{ChatMessage, ChatRole};
        let kind = if track.is_cleanse() {
            "cleanse_plan"
        } else {
            "model_plan"
        };
        let sys = prompts::plan::plan_design_memo_system_prompt(kind);
        let user = format!(
            "Planning kind: {kind}\n\nContext:\n{}\n\nCurrent design memo:\n{}\n\nCritique JSON:\n{}\n\nRewrite the design memo in free text so the critique blockers/fixes are addressed.\nDo not return JSON.",
            Self::excerpt(planning_context, 90_000),
            Self::excerpt(memo, 40_000),
            serde_json::to_string_pretty(critique).unwrap_or_else(|_| "{}".to_string()),
        );
        let opts = Self::planning_llm_options(
            PlanningLlmProfile::DesignMemo,
            "data_engineer.plan_design_memo_revise",
            ctx.thread_id().clone(),
        )?;
        ctx.llm_chat(
            &[
                ChatMessage {
                    role: ChatRole::System,
                    content: sys,
                },
                ChatMessage {
                    role: ChatRole::User,
                    content: user,
                },
            ],
            &opts,
        )
        .await
        .map_err(|e| e.to_string())
    }

    pub(super) async fn produce_critiqued_design_memo(
        ctx: &AgentCtx,
        track: TrackKind,
        planning_context: &str,
    ) -> Result<
        (
            String,
            crate::plan_schema::PlanDesignCritiqueV1,
        ),
        String,
    > {
        let mut memo = Self::generate_design_memo(ctx, track, planning_context).await?;
        let mut critique = Self::critique_design_memo(ctx, track, planning_context, &memo)
            .await?;
        // Bounded revision loop: critique feedback must update memo reasoning before extraction.
        for _ in 0..1 {
            if critique.ok {
                break;
            }
            memo =
                Self::revise_design_memo(ctx, track, planning_context, &memo, &critique)
                    .await?;
            critique = Self::critique_design_memo(ctx, track, planning_context, &memo)
                .await?;
        }
        Ok((memo, critique))
    }

    pub(super) fn critique_guidance(critique: &crate::plan_schema::PlanDesignCritiqueV1) -> String {
        if critique.blockers.is_empty() && critique.fixes.is_empty() {
            return "Design critique: no blockers identified.".to_string();
        }
        let blockers = if critique.blockers.is_empty() {
            "- (none)".to_string()
        } else {
            critique
                .blockers
                .iter()
                .take(6)
                .map(|b| {
                    let target = b
                        .target_id
                        .as_deref()
                        .map(|s| format!(" target={}", s))
                        .unwrap_or_default();
                    let detail = b
                        .detail
                        .as_deref()
                        .map(|s| format!(" detail={}", s.trim()))
                        .unwrap_or_default();
                    format!("- {:?}{}{}", b.code, target, detail)
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let fixes = if critique.fixes.is_empty() {
            "- (none)".to_string()
        } else {
            critique
                .fixes
                .iter()
                .take(6)
                .map(|f| {
                    let blocker = f
                        .blocker_code
                        .map(|c| format!(" blocker={:?}", c))
                        .unwrap_or_default();
                    let target = f
                        .target_id
                        .as_deref()
                        .map(|s| format!(" target={}", s))
                        .unwrap_or_default();
                    let detail = f
                        .detail
                        .as_deref()
                        .map(|s| format!(" detail={}", s.trim()))
                        .unwrap_or_default();
                    format!("- {:?}{}{}{}", f.action, blocker, target, detail)
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        format!(
            "Design critique (bounded one-pass):\n\
ok={}\n\
blockers:\n{}\n\
fixes:\n{}\n\
Apply these fixes in the output.",
            critique.ok, blockers, fixes
        )
    }

    pub(super) async fn generate_model_candidates(
        ctx: &AgentCtx,
        planning_context: &str,
        memo: &str,
        critique: &crate::plan_schema::PlanDesignCritiqueV1,
    ) -> Result<crate::plan_schema::ModelPlanCandidatesV1, String> {
        use react_core::llm::{ChatMessage, ChatRole};
        let mut opts = Self::planning_llm_options(
            PlanningLlmProfile::SkeletonOrCandidates,
            "data_engineer.model_plan_candidates",
            ctx.thread_id().clone(),
        )?;
        opts.expected_format = react_core::llm::LlmExpectedFormat::JsonSchemaSpec {
            name: "suite.model_plan_candidates.v1".to_string(),
            schema: crate::plan_schema::strict_schema_for::<
                crate::plan_schema::ModelPlanCandidatesV1,
            >()?,
        };
        let sys = prompts::plan::model_plan_candidates_system_prompt();
        let user = format!(
            "Context:\n{}\n\nDesign memo:\n{}\n\n{}\n\nReturn candidate-selection JSON.",
            Self::excerpt(planning_context, 60_000),
            Self::excerpt(memo, 30_000),
            Self::critique_guidance(critique)
        );
        let raw = ctx.llm_chat(
            &[
                ChatMessage {
                    role: ChatRole::System,
                    content: sys.to_string(),
                },
                ChatMessage {
                    role: ChatRole::User,
                    content: user,
                },
            ],
            &opts,
        )
        .await.map_err(|e| e.to_string())?;
        Self::parse_json_typed_strict::<crate::plan_schema::ModelPlanCandidatesV1>(&raw)
    }

    pub(super) fn compile_cleanse_skeleton_plan(
        skeleton: &crate::plan_schema::CleansePlanSkeletonV1,
    ) -> crate::plan::CleansePlan {
        let task_ids: Vec<String> = skeleton
            .tasks
            .iter()
            .map(|t| t.dataset_id.trim().to_string())
            .filter(|id| !id.is_empty())
            .collect();
        let tasks: Vec<crate::plan::CleanseTask> = task_ids
            .iter()
            .map(|dataset_id| {
                crate::plan::CleanseTask {
                    dataset_id: dataset_id.to_string(),
                    expected_model_path: None,
                    invariants: vec![],
                    implementation_spec: None,
                    status: Default::default(),
                    checklist: crate::plan::canonical_task_checklist(TrackKind::Cleanse),
                }
            })
            .collect();
        let batches: Vec<Vec<String>> = if skeleton.batches.is_empty() {
            task_ids.chunks(plan_progress::MAX_BATCH_SIZE).map(|c| c.to_vec()).collect()
        } else {
            skeleton.batches.clone()
        };
        let work_groups =
            crate::plan::canonical_work_groups_from_batches(&batches, "cleanse");
        crate::plan::CleansePlan {
            plan_key: String::new(),
            status: crate::plan::PlanStatus::Draft,
            project_snapshot: serde_json::json!({}),
            tasks,
            batches,
            work_groups,
            mutations: vec![],
            progress: Default::default(),
        }
    }

    pub(super) fn model_plan_min_score() -> i32 {
        env_util::model_plan_min_score()
    }

    pub(super) fn select_high_value_model_candidates(
        candidates: &[crate::plan_schema::ModelPlanCandidateV1],
    ) -> Vec<crate::plan_schema::ModelPlanCandidateV1> {
        if candidates.is_empty() {
            return vec![];
        }
        let min_score = Self::model_plan_min_score();
        let mut ranked = candidates.to_vec();
        ranked.sort_by(|a, b| {
            b.value_score
                .cmp(&a.value_score)
                .then_with(|| a.name.cmp(&b.name))
        });
        ranked
            .into_iter()
            .filter(|c| c.value_score >= min_score)
            .collect()
    }

    pub(super) fn compile_model_candidates_plan(
        candidates: &crate::plan_schema::ModelPlanCandidatesV1,
    ) -> crate::plan::ModelPlan {
        let selected = Self::select_high_value_model_candidates(&candidates.candidates);
        let task_names: Vec<String> = selected.into_iter().map(|c| c.name).collect();
        let batches: Vec<Vec<String>> = task_names.chunks(plan_progress::MAX_BATCH_SIZE).map(|c| c.to_vec()).collect();
        let tasks: Vec<crate::plan::ModelTask> = task_names
            .into_iter()
            .map(|name| {
                crate::plan::ModelTask {
                    name,
                    folder: crate::plan::ModelFolder::default(),
                    goal: String::new(),
                    inputs: vec![],
                    expected_model_path: None,
                    invariants: vec![],
                    implementation_spec: None,
                    status: Default::default(),
                    checklist: crate::plan::canonical_task_checklist(TrackKind::Model),
                }
            })
            .collect();
        let work_groups =
            crate::plan::canonical_work_groups_from_batches(&batches, "model");
        crate::plan::ModelPlan {
            plan_key: String::new(),
            status: crate::plan::PlanStatus::Draft,
            project_snapshot: serde_json::json!({}),
            tasks,
            batches,
            work_groups,
            mutations: vec![],
            progress: Default::default(),
        }
    }
}
