
# Devtool Moat Framework

### Overview 

A VC-style framework for evaluating moats in a deep-tech data platform built as a CLI plus cloud SaaS. Focus is on what investors view as durable versus easily copied in developer tools and data infrastructure.

todos:
  - id: protect-schema-ip
    content: Keep the full schema discovery and evolution engine closed while packaging it cleanly as a paid differentiator
    status: pending
  - id: package-free-pro-enterprise
    content: Define the Free, Pro, and Enterprise boundaries for Skippr features across adoption, revenue, and moat layers
    status: pending
  - id: build-metadata-graph
    content: Prioritize the registry, catalog, lineage, stats, and audit surfaces that can compound into a real metadata moat
    status: pending
  - id: sequence-control-plane
    content: Sequence identity, policy, governance, and enterprise control-plane features after the metadata layer is mature enough
    status: pending
isProject: false


# Skippr Feature Packaging Matrix

## Current strategic call

- Position Skippr first as an `AI Data Agent` that performs `ELM` (`Extract`, `Load`, `Model`) and accumulates governance context; treat ELT as a subsystem and comparison frame, not the primary category.
- Keep nested schema discovery, nested type mapping, and schema evolution protected for now.
- Treat schema intelligence as a paid differentiator and revenue driver, not as the long-term moat by itself.
- Recognize that catalog, semantic context, stats, and review/run memory already exist as real agent-quality infrastructure in the `react` data engineer runtime; the main gap is product surface, not raw capability.
- Build the long-term moat above the engine by productizing the metadata substrate: registry/graph, catalog, semantic context, lineage, audit, identity, policy, and control plane workflows.
- Do not rely on raw data or light ecosystem effects as the primary moat. The stronger path is workflow ownership -> proprietary context -> control plane -> system of record.
- Add an AI-facing platform surface on top of that substrate: stable APIs / MCP / workflow interfaces so external agents can read Skippr context and invoke governed workflows through Skippr rather than routing around it.
- Allow `Free` to include basic schema discovery plus basic sink-native type mapping and schema conversion.
- Allow `Free` to include deterministic DBT project bootstrapping.
- Keep nested discovery, nested type mapping, and automatic evolution in `Pro`.
- Keep AI-authored DBT silver and gold model generation in `Pro`.

## Positioning lens

- Primary category: `AI Data Agent` for analytics readiness, warehouse delivery, and downstream AI enablement.
- Functional decomposition: `ELM` plus metadata and governance context. Classic `ELT` is only one implementation layer inside that broader product.
- Adoption messaging should sell `time-to-trusted-data-product` and `time-to-AI-ready warehouse assets`, not connector parity.
- ELT comparison pages against Airbyte, Fivetran, dbt, Glue, and similar tools are still useful for SEO and buyer translation, but they should support the story rather than define it.

# Moat

- Data: Propretory data
- Workflow: Considered weak, but embedded deep enough into the clients business can protect you
- Regulatory: Legal, financial, safetry, etc
- Distribution: Proprietry locked in (e.g. accountants agree to only use intuit)
- Ecosystem: Many tools/services/plugins build onto of you
- Network: Door dash, Facebook, etc
- Physical Infra: Hard to displace
- Scale: By virtue of your scale, your costs make you hard to disrupt
- Brand: 

## Moat sanity check

- `Workflow ownership`: strong and aligned with VC thinking. The best near-term moat candidate is owning the repeated workflow from discovery and sync through modeling, validation, review, feedback, and publication, rather than owning a point feature.
- `Control plane`: important and still incomplete. Auth, metering, remote credentials, thread storage, and review persistence exist, but enterprise-grade approvals, org controls, policy, and deployment controls still need productization.
- `Metadata`: strong and aligned, especially because it already powers the agent. Catalog, stats, semantic context, vector notes, and run traces are more defensible than a raw connector matrix.
- `Proprietary context`: stronger than a generic "data moat." What matters is not just having data, but accumulating structured context about schemas, semantics, model choices, repairs, and human feedback that improves future runs.
- `System of record`: this remains the most important missing step, but only if it is more than a storage surface. The plan is correct only if Skippr becomes the canonical place where teams inspect dataset meaning, model intent, run state, approvals, and policy outcomes, and where agents rely on Skippr's representation of reality.
- `Switching costs`: stronger than network effects for Skippr. Safe dbt augmentation, persistent semantic context, audit history, lineage, embedded workflow approvals, and Skippr-defined dataset/semantic representations can make the product hard to rip out.
- `Network effects`: weak or secondary for now. Plugin ecosystems, community adoption, and partner certification may help distribution, but they are unlikely to be the core moat in the next phase.
- `Scale economies`: moderate, not primary. Shared cloud metering, enrichment, evaluation, and provider infrastructure can improve margins and product quality, but they do not defend the business on their own.
- `Brand / IP`: useful but insufficient alone. The schema engine and `AI Data Agent` story help differentiation, but investors usually discount these unless they feed into switching costs, workflow ownership, or a real system of record.

## AI disruption test

- The plan is structurally sound if Skippr becomes the `control plane`, `memory layer`, and `system of record` for AI-assisted data work, not just a one-shot code generator.
- The plan is vulnerable if Skippr remains mostly a thin wrapper around commodity models for schema, dbt, or catalog text generation. Those features will be compressed by AI over time.
- Skippr should benefit from AI adoption if every run compounds context: schema observations, stats, semantic inferences, repair traces, review outcomes, approvals, usage patterns, and feedback should all improve future agent behavior.
- The most durable AI-native asset is `persistent, structured, reviewable context`, not raw prompts or access to a particular model vendor.
- Human review, approval gates, and auditability become more valuable as AI adoption rises. That means governance features are not just enterprise add-ons; they are part of the AI moat.
- Model/provider abstraction remains important. The company should avoid a moat story that depends on privileged access to one frontier model, because that leaves it exposed to platform shifts.
- The current positioning is helpful here: `AI Data Agent` / `ELM` gives Skippr room to benefit from broader AI adoption, because every new AI workload increases demand for trustworthy, modeled, governed data foundations.
- The correct strategic goal is: let AI commoditize feature implementation while Skippr captures the durable layer above it, namely workflow, context, control, and system-of-record ownership.

## Scoring rubric

- `Status`: `Exists`, `Partial`, `Missing`
- `Impact`: `1-5`, where `5` means strongest effect on adoption, revenue, or moat
- `Effort`: `1-5`, where `5` means hardest to build or productize from here

## Packaging summary

- `Free`: install, docs, local trust story, reliable local `ELM` proof, basic schema discovery, basic sink-native type mapping/schema conversion, deterministic DBT bootstrapping, and enough visibility to trust the agent's first output
- `Pro`: protected nested schema engine, nested type mapping, schema evolution, AI DBT modeling, AI-facing context APIs / MCP, production usage, CI auth, and operational features that let the agent remove real engineering toil
- `Enterprise`: governance, lineage, user-facing catalog and semantic surfaces, audit, identity, policy, private deployment, governed workflow APIs / MCP, and org-wide control plane features

## Adoption wedge


| Feature                                            | Status    | Tier   | Impact | Effort | Notes                                                                                                                                                                                                                                       |
| -------------------------------------------------- | --------- | ------ | ------ | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Data integration connectors and runtime            | `Exists`  | `Free` | `5`    | `2`    | Broad connector coverage is already one of the strongest adoption assets. Keep this as a major wedge.                                                                                                                                       |
| ELM ingestion pipeline                             | `Exists`  | `Free` | `5`    | `2`    | This is the main "one binary, one run path" value proposition and should stay highly accessible as proof that the agent can execute end-to-end workflow locally.                                                                            |
| WAL / exactly-once delivery                        | `Exists`  | `Free` | `5`    | `2`    | Reliability is a trust-building wedge and should stay visible in the local product story.                                                                                                                                                   |
| Checkpointing / CDC resume                         | `Exists`  | `Free` | `4`    | `2`    | Important for serious ingestion credibility and later managed-ops upsell.                                                                                                                                                                   |
| Deadletters / quarantine                           | `Exists`  | `Free` | `4`    | `2`    | Strong operational trust feature and a good precursor to replay/remediation products.                                                                                                                                                       |
| Plugin runtime / connector framework               | `Exists`  | `Free` | `4`    | `3`    | Good ecosystem wedge, but not a moat without certification, compatibility, and a surrounding marketplace or partner motion.                                                                                                                 |
| Basic schema discovery                             | `Exists`  | `Free` | `5`    | `2`    | Safe to use as a wedge so long as it stays focused on straightforward discovery rather than the full nested planner.                                                                                                                        |
| Deep nested schema discovery                       | `Exists`  | `Pro`  | `5`    | `2`    | One of the clearest technical differentiators in the current product. Protect it.                                                                                                                                                           |
| Basic sink-native type mapping / schema conversion | `Exists`  | `Free` | `4`    | `2`    | Useful in the free wedge so users can see Skippr land sensible native types in common non-nested cases.                                                                                                                                     |
| Nested type mapping / schema conversion            | `Exists`  | `Pro`  | `5`    | `3`    | Protect the hard part: nested and complex destination typing should stay in the paid product alongside the nested discovery engine.                                                                                                         |
| Deterministic DBT project bootstrapping            | `Exists`  | `Free` | `5`    | `4`    | Standard dbt project files and predictable scaffold should stay in the free wedge so teams can get to a credible first warehouse project fast. Evolve this toward append/merge-only augmentation so existing dbt repos are never clobbered. |
| Existing dbt project migration / safe augmentation | `Partial` | `Free` | `5`    | `3`    | Critical switching-cost lever. Deterministic dbt should add or merge into existing projects rather than overwrite them, so teams can trial Skippr without a clean-slate rewrite.                                                            |
| Query / debug surfaces                             | `Exists`  | `Pro`  | `3`    | `2`    | `STREAM`, `SHOW STATS`, `SHOW SEMANTIC`, `SHOW CATALOG`, and `SHOW PIPELINE` make the agent legible and auditable rather than a black box.                                                                                                  |


## Revenue capture


| Feature                                  | Status    | Tier         | Impact | Effort | Notes                                                                                                                                    |
| ---------------------------------------- | --------- | ------------ | ------ | ------ | ---------------------------------------------------------------------------------------------------------------------------------------- |
| Automatic schema evolution               | `Exists`  | `Pro`        | `5`    | `3`    | Strong paid differentiator and near-term revenue driver. It should stay protected while the higher-level moat is still being built.      |
| Evolution approvals / history / rollback | `Missing` | `Enterprise` | `5`    | `4`    | This turns evolution from engine behavior into a governed product and creates a much stronger enterprise reason to pay.                  |
| Deadletter replay / remediation          | `Missing` | `Pro`        | `5`    | `4`    | High-value operational upsell directly tied to reliability pain and ongoing operator workflow.                                           |
| AI DBT silver / gold model generation    | `Exists`  | `Pro`        | `5`    | `4`    | Keep the AI-authored silver/gold layer in `Pro`. This is the monetizable modeling surface above the free deterministic project scaffold. |
| Continuous model maintenance / repair    | `Partial` | `Pro`        | `5`    | `5`    | Likely a better long-term monetization surface than one-shot generation because it compounds with source changes and runtime history.    |
| API keys / CI auth                       | `Partial` | `Pro`        | `4`    | `2`    | Good packaging lever for teams moving from experimentation to real production use.                                                       |
| Usage metering / credits / billing       | `Partial` | `Pro`        | `4`    | `3`    | Useful revenue capture surface, but not a moat on its own.                                                                               |
| Private deployment / hybrid runner       | `Partial` | `Enterprise` | `4`    | `3`    | Strong enterprise unlock when paired with a cloud control plane and local/private execution model.                                       |


## Moat builders


| Feature                                                           | Status    | Tier         | Impact | Effort | Notes                                                                                                                                                                                                                                                   |
| ----------------------------------------------------------------- | --------- | ------------ | ------ | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Metadata registry / graph                                         | `Partial` | `Enterprise` | `5`    | `4`    | Highest-leverage internal asset. `skipprd` already maintains registry entries and metadata keys, while the `react` suite consumes richer catalog/semantic artifacts. The missing piece is a unified product/control-plane graph.                        |
| Field stats / profiling                                           | `Exists`  | `Pro`        | `4`    | `2`    | Already collected and consumed in catalog building, semantic enrichment, and planning. Useful now for agent quality and later for drift alerts, data tests, and governance.                                                                             |
| Drift detection / schema change intelligence                      | `Missing` | `Enterprise` | `5`    | `4`    | Natural productization of field stats plus schema evolution history.                                                                                                                                                                                    |
| Data catalog                                                      | `Exists`  | `Enterprise` | `5`    | `4`    | This is more built out than the earlier framing suggested: the `react` data engineer suite already builds, enriches, stores, and consumes catalogs. The gap is turning that into a first-class user-facing system of record.                            |
| Semantic layer / embeddings-backed context                        | `Exists`  | `Enterprise` | `4`    | `4`    | Also more mature than previously stated: semantic models, global semantic context, vector-backed notes, and retrieval already improve agent behavior. The missing layer is durable product UX and organizational workflow ownership.                    |
| AI-facing context API / MCP                                       | `Partial` | `Pro`        | `5`    | `3`    | Strong product-surface opportunity. The `react` transport already has internal typed WS/headless protocols for plans, thread state, history, approvals, and events; productize a stable read-first API/MCP so external agents depend on Skippr context. |
| AI workflow / control API                                         | `Partial` | `Enterprise` | `5`    | `4`    | Promote the existing internal run/approval/event protocol into a governed action surface for triggering runs, routing feedback, handling approvals, and orchestrating agent workflows through Skippr's control plane.                                   |
| End-to-end lineage (source -> bronze -> silver/gold -> warehouse) | `Missing` | `Enterprise` | `5`    | `5`    | Best future moat candidate because Skippr can potentially own both ingest context and generated dbt context.                                                                                                                                            |
| Audit trail / run history                                         | `Partial` | `Enterprise` | `5`    | `4`    | Stronger than a blank slate: thread storage, review persistence, feedback capture, and metering traces already exist. The missing step is a polished enterprise audit/compliance surface.                                                               |
| Users / org / workspaces                                          | `Partial` | `Enterprise` | `4`    | `3`    | Needed for a real multi-tenant control plane, but most valuable once catalog and audit surfaces exist.                                                                                                                                                  |
| SSO / role model                                                  | `Partial` | `Enterprise` | `4`    | `3`    | Necessary for larger accounts and enterprise expansion, though not itself a moat.                                                                                                                                                                       |
| ABAC / policy engine                                              | `Missing` | `Enterprise` | `5`    | `5`    | High-value enterprise control surface once catalog, lineage, and identity are mature enough to support it.                                                                                                                                              |
| Plugin certification / partner ecosystem                          | `Missing` | `Enterprise` | `4`    | `4`    | Worth pursuing only after the core product and metadata surfaces are stable.                                                                                                                                                                            |


## Recommended sequencing

1. Protect the schema engine

Keep nested discovery, nested mapping, and all evolution closed and product-gated. Use basic discovery, basic sink-native type mapping, deterministic DBT bootstrapping, and safe dbt augmentation as the `Free` wedge.

1. Lead with AI Data Agent positioning

Market Skippr as an `AI Data Agent` / `ELM` product that produces trustworthy warehouse assets and governance context. Use ELT language mainly for comparison pages, SEO, and buyer translation.

1. Productize the metadata layer you already have

Prioritize turning catalog, semantic context, stats, vector notes, review traces, and run memory into explicit product surfaces. Much of the hard internal substrate already exists.

1. Expose Skippr to other agents

Add read-first APIs / MCP for catalog, semantic context, thread state, plans, and run history; then add governed write surfaces for workflow actions, approvals, and orchestration.

1. Turn Skippr into the system of record

Make Skippr the canonical place for dataset meaning, semantic representation, pipeline state, model intent, approvals, and repair history so switching costs come from daily workflow dependence and ontology ownership rather than point-feature superiority.

1. Expand `Enterprise` after the graph exists

Layer SSO, teams, ABAC, approvals, lineage, private deployment, and governance on top of the same metadata system.

## Revisit trigger for openness

- Reconsider opening interfaces, metadata formats, fixtures, compatibility suites, or limited protocol layers only after catalog, lineage, audit, and control-plane features are strong enough that the company still wins if the core engine is copied.
- Until then, the default position should be: keep the hard-won schema discovery and evolution implementation protected, and standardize the boundaries later if distribution pressure truly demands it.

## Commercial packaging and upgrade triggers


| Plan         | High-level packaging                                                                                                                                                                                                                                                                | Core promise                                                                                                                | Best-fit user                                                                                                                      | Upgrade triggers                                                                                                                                                                                              |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Free`       | Installation, docs, local trust story, reliable local `ELM` execution, basic schema discovery, basic sink-native type mapping/schema conversion, deterministic DBT project bootstrapping, and migration-friendly deterministic outputs. No nested discovery or automatic evolution. | "Prove Skippr can act as an AI data agent in my environment and produce a trustworthy dbt-ready foundation."                | Individual engineers, technical evaluators, early pilots, lean teams starting an analytics or AI stack                             | Upgrade when the user hits nested or complex source structures, needs nested type mapping, wants automatic evolution, or wants AI-authored silver/gold modeling, repeatable production workflows, or CI auth. |
| `Pro`        | Deep nested schema discovery, nested type mapping, automatic schema evolution, AI silver/gold DBT model generation, model maintenance, AI-facing context APIs / MCP, CI auth, and richer agent-quality context from the metadata layer.                                             | "Let the AI data agent replace repetitive pipeline and modeling work, and let other agents consume trusted Skippr context." | Small teams, fast-moving startups, consultancies, and engineers shipping analytics/AI pipelines without a large data platform team | Upgrade when the buyer needs governance, org controls, approvals, private deployment, catalog, lineage, audit, enterprise identity, or governed write access into workflows.                                  |
| `Enterprise` | Multi-tenant control plane, user-facing catalog and semantic context, lineage, audit trail, SSO, org/workspace controls, ABAC/policy, private deployment, governed workflow APIs / MCP, and governed schema evolution workflows.                                                    | "Turn Skippr into a system of record, governance layer, and control plane for production data operations and AI readiness." | Larger companies, regulated teams, platform groups, security-conscious enterprises                                                 | Triggered by compliance review, security requirements, multi-team coordination, governance demands, procurement, or a need for durable metadata and operational lock-in across the organization.              |


### Packaging stance

- `Free` should maximize trust and evaluation speed using reliable `ELM`, basic discovery, basic type mapping, deterministic DBT scaffolding, and migration-friendly outputs, without giving away the nested intelligence or AI modeling layer.
- `Pro` should be where the hard nested schema engine, evolution logic, AI modeling layer, and AI-facing context surfaces are monetized.
- `Enterprise` should be where the already-built metadata substrate compounds into the moat through user-facing catalog, semantic context, governance, organizational workflow ownership, and governed workflow APIs.
- Keep calling out ELT where it helps buyers understand what Skippr replaces, but keep the category headline at `AI Data Agent` / `ELM`.
- Treat workflow ownership, switching costs, proprietary context, and system-of-record status as the primary moat stack; treat network effects, raw data moats, and ecosystem narratives as supporting or secondary.

