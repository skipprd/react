# Data Engineer Phases

The Data Engineer suite's `agent` mode executes a multi-phase workflow. Each phase has a specific purpose and transitions to the next when its work is complete.

## Phase order

```
preflight → plan → author → validate → review → publish
```

## Phase details

### Preflight

**Purpose:** Discover the data landscape and prepare context for the agent.

What happens:

1. `DatasetCatalogProvider` queries the warehouse for available tables/views
2. Catalog metadata is refreshed (column types, descriptions, row counts)
3. Catalog entries are indexed into the vector store
4. Dataset candidates are identified for plan grounding

The agent does not run during preflight — this is automated setup. The client sees `phase` events but no `tool_start`/`tool_end` frames.

### Plan

**Purpose:** Create a structured execution plan based on the user's request and available data.

What happens:

1. The agent examines discovered datasets and the user's question
2. It produces a plan with tasks (one per deliverable) and checklist items
3. The plan is validated and grounded against actual catalog metadata
4. If the user asked for something that can't be mapped to existing data, the agent asks for clarification (`await_user`)

The plan is sent to the client as a `plans` frame. It tracks progress through subsequent phases.

### Author

**Purpose:** Write dbt models (staging, cleansing, gold) for each plan task.

What happens:

1. The agent iterates through plan tasks and their checklist items
2. For each item, it authors SQL models using the batch patch protocol (`apply_next_batch`)
3. Models are written to the dbt project directory via the `StorageAdapter`
4. Schema contracts (YAML) are generated alongside SQL models

The authoring phase uses work groups to process items in dependency order (staging before gold).

### Validate

**Purpose:** Run `dbt build` or `dbt run` to validate authored models.

What happens:

1. The `dbt_validate` tool invokes dbt against the authored project
2. If validation fails, the `dbt_repair` tool attempts automated remediation
3. A repair loop retries validation until all models pass or the retry budget is exhausted
4. If repair fails, the agent may ask the user for help (`await_user`)

### Review

**Purpose:** Red-team review of authored models for correctness and best practices.

What happens:

1. A review agent examines each authored model
2. It produces `review` frames with structured metadata (decision, tier, dataset IDs)
3. Decisions: `proceed` (model is acceptable) or `patch_impl` (changes needed)
4. If patches are needed, the workflow loops back to the author phase for the affected items

### Publish

**Purpose:** Materialise validated models in the warehouse.

What happens:

1. The `publish_dbt_to_provider` tool runs `dbt run` against the warehouse target
2. Models are materialised as tables/views in the configured schemas
3. The plan status is updated to `completed`

## Phase transitions

The Data Engineer suite's controller kernel manages transitions based on:

- **Completion signals** — all checklist items in the current phase are `done`
- **Guard blocks** — certain transitions require conditions to be met (e.g. validation must pass before review)
- **Reason codes** — each transition carries a reason code and optional detail payload for audit

Phase transitions are streamed to the client as `phase` frames with timing data (start/end timestamps, runtime in milliseconds).

## Runtime tracking

Each phase tracks its runtime. The `thread_state` frame includes:

- `phases` — ordered list of all phases
- `completedPhases` — phases that have finished
- `currentPhase` — the active phase
- `total_runtime_ms` — cumulative runtime across all phases

## Next steps

- [Data Engineer Suite](data-engineer.md) — tools, plans, and configuration
- [WebSocket API: Server Frames](../websocket-api/server-frames.md) — `phase`, `plans`, `plans_changed` frames
