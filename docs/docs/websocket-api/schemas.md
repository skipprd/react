# Schemas

Detailed type definitions for WebSocket API payloads. The canonical source is `src/runtime/openapi/ws-core.yaml` (OpenAPI 3.1).

## FinalResult

Discriminated union on the `kind` field.

### AskFinalResult

| Field | Type | Required | Description |
|---|---|---|---|
| `kind` | `"ask"` | Yes | Discriminator |
| `payload.answer` | string | Yes | Plain-text answer |
| `payload.sql` | string | Yes | SQL query used to produce the answer |
| `payload.data` | object | No | Tabular result (`header`: string[], `rows`: string[][]) |
| `payload.chart` | object | No | Chart suggestion (`type`: line/bar/area, `x`: string, `y`: string[]) |

### KbFinalResult

| Field | Type | Required | Description |
|---|---|---|---|
| `kind` | `"kb"` | Yes | Discriminator |
| `payload.answer` | string | Yes | Plain-text answer |

### GenericFinalResult

| Field | Type | Required | Description |
|---|---|---|---|
| `kind` | `"generic"` | Yes | Discriminator |
| `payload.text` | string | Yes | Human-readable final text |

## PlanSnapshot

| Field | Type | Required | Description |
|---|---|---|---|
| `planKind` | string | Yes | Suite-defined plan kind |
| `planKey` | string | Yes | Unique plan identifier |
| `status` | PlanStatus | Yes | `draft`, `approved`, `completed`, `cancelled` |
| `tasks` | PlanTask[] | Yes | Tasks in this plan |
| `workGroups` | PlanWorkGroup[] | Yes | Ordered work groups |
| `projectSnapshot` | object | No | Opaque suite-owned snapshot |

## PlanTask

| Field | Type | Required | Description |
|---|---|---|---|
| `taskId` | string | Yes | Unique task identifier |
| `taskKind` | string | No | Task kind |
| `label` | string | No | Display label |
| `status` | PlanTaskStatus | Yes | `pending`, `in_progress`, `done`, `blocked`, `needs_update` |
| `checklist` | PlanChecklistItem[] | Yes | Checklist items for this task |
| `details` | object | No | Opaque task details |

## PlanChecklistItem

| Field | Type | Required | Description |
|---|---|---|---|
| `checklistItemId` | string | Yes | Stable ID within the task (e.g. `sql_model`, `schema_contract`, `validate`) |
| `label` | string | Yes | Short UI label |
| `details` | string | No | Longer instructions |
| `status` | PlanChecklistItemStatus | Yes | `pending`, `in_progress`, `done`, `blocked`, `needs_update` |
| `origin` | `"initial"` | Yes | How this item was created |
| `evidence` | PlanChecklistEvidence[] | No | Completion evidence |

## PlanWorkGroup

| Field | Type | Required | Description |
|---|---|---|---|
| `groupId` | string | Yes | Unique group identifier |
| `label` | string | Yes | Display label |
| `kind` | PlanWorkGroupKind | Yes | `author_sql`, `author_schema`, `validate` |
| `items` | PlanWorkGroupItemRef[] | Yes | References to task checklist items |
| `dependsOnGroupIds` | string[] | No | Groups that must complete first |

## ThreadStateSnapshot

| Field | Type | Required | Description |
|---|---|---|---|
| `thread_state_schema_version` | integer | Yes | Schema version (currently 2) |
| `thread_id` | string | Yes | Thread ID |
| `suiteId` | string | No | Current suite |
| `agentType` | string | No | Current agent type |
| `currentPhase` | string | No | Active phase |
| `last_materialized_step_count` | integer | Yes | Steps materialised into this snapshot |
| `total_runtime_ms` | integer | Yes | Cumulative runtime (ms) |
| `phases` | string[] | Yes | Ordered phase list (suite-defined) |
| `completedPhases` | string[] | Yes | Phases that have finished |
| `events` | ThreadEvent[] | Yes | Recent timeline events |
| `items` | ThreadStateItem[] | Yes | Per-item state |
| `planSummaries` | object | No | Summary-only plan refs |

## ThreadEvent

| Field | Type | Required | Description |
|---|---|---|---|
| `step_idx` | integer | Yes | Thread step index |
| `event_kind` | string | Yes | `tool_start`, `tool_end`, `llm_start`, `llm_end` |
| `ts` | string | Yes | RFC 3339 timestamp |
| `tool_id` | string | No | Tool invocation ID |
| `name` | string | No | Tool name |
| `clean_name` | string | No | Human-readable label |
| `status` | ToolEventStatus | No | `running`, `ok`, `failed` |
| `runtime_ms` | integer | No | Execution time (ms) |
| `payload` | object | No | Tool-specific metadata |
| `error` | string | No | Error message |
| `call_id` | integer | No | LLM call counter |
| `model` | string | No | Model name |
| `phase` | string | No | Current phase |
| `ctx` | ExecutionContext | No | Execution context |

## ExecutionContext

Optional context for hierarchical UI rendering.

| Field | Type | Description |
|---|---|---|
| `plan_kind` | string | Suite-defined plan kind |
| `plan_key` | string | Plan identifier |
| `workgroup_id` | string | Work group ID |
| `task_id` | string | Task ID |
| `checklist_item_id` | string | Checklist item ID |

## ReviewDecisionMeta

| Field | Type | Required | Description |
|---|---|---|---|
| `decision` | string | Yes | `proceed` or `patch_impl` |
| `tier` | string | Yes | `silver`, `gold`, `unknown` |
| `dataset_ids` | string[] | Yes | Affected dataset IDs |
| `review_ref` | ReviewRef | No | File reference (key, sha256, bytes) |
