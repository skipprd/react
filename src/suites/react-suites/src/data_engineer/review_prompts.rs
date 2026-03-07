// Declared as `mod review_prompts;` in data_engineer/mod.rs

use serde_json::Value;

use super::review_batched::ProjectFile;
use super::track_spec::PlanKind;

pub(super) fn system_prompt_for_summary(plan_kind: Option<PlanKind>) -> String {
    let is_cleanse = plan_kind == Some(PlanKind::Cleanse);
    let tier_focus = if is_cleanse {
        "- Tier focus (CRITICAL): this is a SILVER (cleanse) review. Do NOT penalize missing GOLD models.\n"
    } else {
        ""
    };
    let insightfulness = if is_cleanse {
        "- Insightfulness check (CRITICAL):\n  \
           - Call out whether the SILVER layer is usable as a stable, row-preserving cleanse foundation (explicit fields, safe casting, quality flags, stable naming).\n  \
           - If you mention GOLD at all, frame it as an optional future improvement, not a blocker."
    } else {
        "- Insightfulness check (CRITICAL):\n  \
           - Call out whether the current GOLD layer enables meaningful business decisions (not just technically-correct SQL).\n  \
           - If GOLD is present but naive, list the top 2 missing \"business semantics\" gaps (definitions, time axis, entity meaning, join contracts) that block real analytics."
    };
    format!(
        "You are a read-only reviewer for a DBT analytics project.\n\n\
         You will be given:\n\
         - The original goal and review context (brief)\n\
         - A small, deterministic project snapshot (dbt_project.yml, sources list, model file index, and minimal manifest metadata)\n\n\
         Output one JSON object with this schema:\n\
         {{\n  \"project_notes\": [string, ...],\n  \"project_risks\": [string, ...]\n}}\n\n\
         Rules:\n\
         - Be pragmatic, not pedantic. Focus on business correctness and usability.\n\
         - Do NOT suggest edits in-line; just describe risks/gaps.\n\
         - Keep notes concise and high-signal.\n\
         {tier_focus}{insightfulness}"
    )
}

pub(super) fn system_prompt_for_batch(plan_kind: Option<PlanKind>) -> String {
    let tier_focus = if plan_kind == Some(PlanKind::Cleanse) {
        "- Tier focus (CRITICAL): this is a SILVER (cleanse) review. Do NOT critique missing GOLD models.\n"
    } else {
        ""
    };
    format!(
        r#"You are a read-only reviewer for a DBT analytics project.

You will be given:
- The original goal and review context
- A batch of items (datasets or model names)
- The expected model file paths and their contents (bounded)
- Any invariants/notes from planning
- The authoritative schema (columns/types) for each dataset in the batch (when available)

Output one JSON object with this schema:
{{
  "notes": [string, ...],
  "actionable_hints": [string, ...]
}}

Rules:
- CRITICAL: The planning artifacts you receive (invariants/notes and any implementation_spec) are the authoritative design contract for this phase.
  - Your primary job is CONFORMANCE REVIEW: does the SQL/YAML implement the provided implementation_spec and obey prohibited_ops?
  - Do NOT propose changing the contract as part of review. If you believe the contract itself is wrong/ambiguous, record it as a "requires plan change" note (see below) but DO NOT propose an implementation change that deviates from the contract.
- Conformance-first ordering:
  1) Identify any plan/spec conformance violations (blockers). These are always high-signal.
  2) Then (optionally) include at most one additional high-value business-risk observation that does NOT require changing the plan/spec.
- If you think a finding requires changing the plan/spec, label it explicitly with prefix:
  - "REQUIRES PLAN CHANGE: ..."
  and do NOT include an implementation hint for it.
- High-signal only: do NOT cover every batch item. Report only blocker/high business-risk findings or one small, clearly high-value quick win.
- If no high-value findings exist for this batch, return:
  - "notes": []
  - "actionable_hints": []
- Hard cap: at most 3 findings in "notes" total.
- Keep "actionable_hints" tightly scoped to the findings: at most 1 hint per finding (max 3 total).
- Prefer concrete feedback tied to specific models/columns when visible.
- IMPORTANT: Do NOT suggest adding/selecting fields that are not present in the provided authoritative schema.
  If a desired field is missing from the schema, call that out as a gap and suggest the nearest available alternative.
{tier_focus}- Each finding must be decision-oriented and include:
  - Impacted metric/decision.
  - Concrete evidence from provided SQL/schema.
  - Smallest next action to reduce risk.
- No tool calls and no file edits."#
    )
}

pub(super) fn system_prompt_for_unify(plan_kind: Option<PlanKind>) -> String {
    let is_cleanse = plan_kind == Some(PlanKind::Cleanse);
    let tier_rule = if is_cleanse {
        "Tier rules: this is a SILVER (cleanse) review. Set tier=\"silver\"."
    } else {
        "Tier rules: this is a GOLD/model review. Set tier=\"gold\"."
    };
    let insightfulness = if is_cleanse {
        "- Include a short \"Insightfulness summary\" section:\n  \
           - What operational/analytical use-cases the SILVER layer supports today.\n  \
           - The top 2 missing semantics gaps (definitions, time axis meaning, entity meaning, join contracts, quality flags) blocking higher-value analysis even at SILVER."
    } else {
        "- Include a short \"Insightfulness summary\" section:\n  \
           - What business decisions the current GOLD layer enables today.\n  \
           - The top 2 missing business semantics gaps blocking higher-value analytics (definitions, time axis, entity meaning, join contracts)."
    };
    format!(
        "You are a read-only reviewer for a DBT analytics project.\n\n\
         You will be given:\n\
         - The original goal and review context\n\
         - Project-level notes/risks\n\
         - Notes from ALL review batches\n\n\
         You must output one JSON object with this schema:\n\
         {{\n\
         \x20 \"decision\": \"proceed\" | \"patch_impl\",\n\
         \x20 \"tier\": \"silver\" | \"gold\" | \"unknown\",\n\
         \x20 \"dataset_ids\": [string],\n\
         \x20 \"final_review_text\": string\n\
         }}\n\n\
         Interpretation rules (CRITICAL):\n\
         - decision=\"proceed\" means: no action required now; the implementation conforms and there are no net-new/still-unresolved high-value issues.\n\
         - decision=\"patch_impl\" means: a concrete implementation change is required NOW to match the approved plan/spec (conformance/correctness fix), without changing the plan/spec.\n\n\
         {tier_rule}\n\n\
         Unify requirements (CRITICAL):\n\
         - Produce a concise, business-focused review that prioritizes decision usefulness.\n\
         - Delta-first output: include only net-new or still-unresolved high-value issues since prior review context. Suppress repeated advice that has no meaningful change in evidence or priority.\n\
         - If no net-new/still-unresolved blocker/high items exist, set decision=\"proceed\" and keep the body brief.\n\
         - Hard cap: list at most 5 issues total across the final review body.\n\
         {insightfulness}\n\
         - Include an \"Assumptions & evidence gaps\" section listing the most important semantic assumptions and the smallest probes to validate them."
    )
}

pub(super) fn compact_review_context_for_summary(question_with_context: &str) -> String {
    let mut entry_reason_code: Option<String> = None;
    let mut entry_reason_detail_raw: Option<String> = None;

    for line in question_with_context.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("- entry_reason_code:") {
            let v = rest.trim();
            if !v.is_empty() {
                entry_reason_code = Some(v.to_string());
            }
        }
        if let Some(rest) = t.strip_prefix("- entry_reason_detail:") {
            let v = rest.trim();
            if !v.is_empty() {
                entry_reason_detail_raw = Some(v.to_string());
            }
        }
    }

    let mut out = String::new();
    if let Some(rc) = entry_reason_code.as_deref() {
        out.push_str("entry_reason_code: ");
        out.push_str(rc);
        out.push('\n');
    }

    if let Some(raw) = entry_reason_detail_raw.as_deref() {
        let hash = react_core::llm_observability::sha256_hex_str(raw);
        out.push_str("entry_reason_detail_sha256: ");
        out.push_str(&hash);
        out.push('\n');

        if let Ok(v) = serde_json::from_str::<Value>(raw) {
            if let Some(idx) = v.get("batch_idx").and_then(|x| x.as_u64()) {
                out.push_str(&format!("entry_reason_detail.batch_idx: {}\n", idx));
            }
            if let Some(items) = v.get("batch_items").and_then(|x| x.as_array()) {
                let mut names: Vec<String> = items
                    .iter()
                    .filter_map(|it| it.as_str().map(|s| s.to_string()))
                    .collect();
                names.sort();
                names.dedup();
                let keep = names.len().min(5);
                out.push_str(&format!(
                    "entry_reason_detail.batch_items_head (sorted, {} of {}): {}\n",
                    keep,
                    names.len(),
                    names.into_iter().take(keep).collect::<Vec<_>>().join(", ")
                ));
            }
            if let Some(keys) = v.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()) {
                let mut kk = keys;
                kk.sort();
                out.push_str(&format!(
                    "entry_reason_detail.keys_sorted: {}\n",
                    kk.into_iter().take(20).collect::<Vec<_>>().join(", ")
                ));
            }
        }
    }

    if out.trim().is_empty() {
        question_with_context.to_string()
    } else {
        out
    }
}

pub(super) fn render_path_list(label: &str, paths: &[String], max_items: usize) -> String {
    let mut p = paths.to_vec();
    p.sort();
    p.dedup();
    let total = p.len();
    let keep = total.min(max_items);
    let shown = &p[..keep];
    let mut s = String::new();
    s.push_str(label);
    s.push_str(&format!(" (total={}):\n", total));
    for it in shown {
        s.push_str("- ");
        s.push_str(it);
        s.push('\n');
    }
    if total > keep {
        s.push_str(&format!(
            "... omitted {} more (deterministic head)\n",
            total - keep
        ));
    }
    s
}

pub(super) fn render_files(files: &[ProjectFile]) -> String {
    let mut s = String::new();
    for f in files {
        s.push_str("\n---\npath: ");
        s.push_str(&f.path);
        s.push_str("\n\n");
        s.push_str(&f.content);
        s.push_str("\n");
    }
    s
}
