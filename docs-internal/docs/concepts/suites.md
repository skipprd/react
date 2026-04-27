# Suites

A **suite** is a product surface that defines how the agent behaves for a particular workflow. Each suite owns its tool registry, system prompts, agent policy, and optional preflight logic.

## What a suite provides

| Component | Purpose |
|---|---|
| **Tool registry** | Which tools the agent can call |
| **System prompt** | Instructions and persona for the LLM |
| **Tool card** | Descriptions of each tool, injected into the prompt |
| **Agent policy** | Rules for validating finals, triggering interrupts, and injecting context |
| **Preflight** | Optional setup before the agent loop |
| **Phase order** | Optional ordered list of phases for multi-step workflows |

## Registered suites

The generic React repo only owns suite-neutral runtime pieces and non-Skippr suites such as `kb`. Product repositories can register additional suites through their host binaries.

| Suite ID | Label | Agent types | Description |
|---|---|---|---|
| `kb` | KB | `kb` | Knowledge-base workflow for ingesting documents and answering questions via vector search |

## The Suite trait

Every suite implements the `Suite` trait and is registered by a host. The host decides which suites are available, resolves configuration, and wires capabilities into `SuiteCtx`.

## Suite selection

Clients specify `suiteId` in their `new` or `open` frame. The WebSocket server looks up the suite in the registry. If the suite is not found, an `error` frame is returned.

```json
{
  "v": 1,
  "type": "new",
  "cid": "...",
  "suiteId": "kb",
  "agentType": "kb",
  "question": "Summarize the indexed docs"
}
```

## Next steps

- [Knowledge Base Suite](../suites/kb.md) — document ingestion and semantic search
- [Extending: Custom Suite](../extending/custom-suite.md) — building your own suite
