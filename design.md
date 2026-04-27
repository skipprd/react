# ReAct Design

`react` is the suite-neutral runtime layer. It owns the core agent abstractions, runtime configuration, WebSocket/headless execution plumbing, transport protocol, view/event materialisation, and generic adapters that are reusable across product hosts.

Product repositories own their domain suites, binaries, provider wiring, and e2e coverage. Skippr-specific data-engineer code now belongs to `skipprd`; Goggles-specific review code belongs to Goggles.

## Boundaries

- `react-core`: agent, suite, storage, keyspace, LLM, tool, workflow, and capability primitives.
- `react`: runtime/bootstrap/headless/server helpers used by product-owned hosts.
- `react-transport` and `react-view`: protocol and presentation support.
- Generic adapters: reusable storage/vector modules that do not encode a product domain.
- Suites that remain here, such as `react-suite-kb`, must depend only on generic React contracts.

## Host Composition

A host binary implements the runtime host contract, registers suites, resolves host-specific configuration, and wires capabilities into `SuiteCtx`. This keeps domain providers out of the generic runtime while preserving a stable API for products to build on.
