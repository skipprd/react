---
name: Plugin SDK and Channel Layer
overview: Extend the React runtime with a plugin SDK that formalizes the existing trait-based extension points into a discoverable, loadable plugin system, and add a channel abstraction layer that bridges external messaging platforms into the existing thread/EventHub model.
todos:
  - id: plugin-trait
    content: Define Plugin trait, PluginManifest, PluginError in react-core (src/core/src/plugin.rs)
    status: pending
  - id: plugin-registry
    content: Implement PluginRegistry with discovery, init, and lookup by kind + id
    status: pending
  - id: channel-trait
    content: Define Channel trait, ChannelRouter, InboundMessage, OutboundMessage in react-core (src/core/src/channel.rs)
    status: pending
  - id: channel-bridge
    content: Implement channel bridge in react-transport that connects ChannelRouter to EventHub and suite runner
    status: pending
  - id: migrate-providers
    content: Wrap existing providers (snowflake, bigquery, dbt, s3, local, lance) as Plugin impls
    status: pending
  - id: suite-ctx-from-registry
    content: Wire SuiteCtx population from PluginRegistry at runtime startup instead of hardcoded assembly
    status: pending
  - id: first-channel
    content: Implement one concrete channel adapter (e.g. Slack or web chat) as proof of concept
    status: pending
isProject: false
---

# Plugin SDK and Channel Layer for React

## Conceptual Overview

### What is the Plugin SDK?

The plugin SDK is a **formal contract** that turns React's existing trait-based extension points into discoverable, loadable modules. React already has the right abstractions -- `Tool`, `LargeLanguageModel`, `StorageAdapter`, `VectorStore`, `StateStore`, `SecretsProvider` -- but today these are wired together at compile time in the runtime crate. The plugin SDK makes them runtime-composable: a plugin declares what capabilities it provides (tools, LLM provider, storage backend, etc.), and the runtime discovers, validates, and loads them.

Plugins are **not** suites. Suites are the orchestration layer -- they define phased workflows, state machines, and agent modes. Suites *consume* plugins. A suite selects which tools, providers, and channels to use for its domain. The plugin SDK provides the building blocks; suites provide the purpose.

### What is the Channel Layer?

The channel layer is a **generic messaging adapter abstraction** that bridges external platforms (Slack, Discord, Telegram, web chat, etc.) into React's existing thread model. Today, the only way to interact with React is via the WebSocket API or headless CLI. The channel layer adds a second ingress path: messages arrive from external platforms, get routed to agent threads, and responses flow back out.

This sits between the transport layer and the suite layer. Channels don't know about suites or phases -- they only know about threads and messages. The existing `EventHub` (which already broadcasts `ServerMessage` to multiple sinks) is the natural attachment point: channels become another subscriber/publisher on the hub.

### Why They Matter

- **Plugins** unlock React as a general-purpose agent platform. Without them, every new tool, LLM provider, or storage backend requires modifying the core crates. With them, the data engineering suite is just one consumer among many -- a coding suite, a support suite, a KB suite can each bring their own tools and providers.
- **Channels** unlock React as an interactive agent platform. Without them, React is a server that only speaks to custom WebSocket clients. With them, an agent thread can be driven by a Slack message, a Discord DM, or a web chat widget -- using the same suite, the same tools, the same ReAct loop underneath.

Together they complete the architecture: **suites** orchestrate, **plugins** provide capabilities, **channels** provide reach.

### How They Fit Into Existing React Design

```
                     Channels (new)
                     Slack | Discord | Web | ...
                         |
                         v
    +-----------------------------------------+
    |           Channel Router (new)           |
    |  platform msg -> thread_id + user text   |
    +-----------------------------------------+
                         |
                         v
    +-----------------------------------------+
    |     EventHub  (existing, extended)       |
    |  broadcast ServerMessage to all sinks    |
    +-----------------------------------------+
          |              |              |
          v              v              v
      WS Server      Terminal       Channels
     (existing)     (existing)       (new)
                         |
                         v
    +-----------------------------------------+
    |          Suite Runner (existing)          |
    |  selects suite -> runs phased workflow   |
    +-----------------------------------------+
                         |
                         v
    +-----------------------------------------+
    |         Plugin Registry (new)            |
    |  tools | llm | storage | vector | ...   |
    +-----------------------------------------+
                         |
                         v
    +-----------------------------------------+
    |         react-core (existing)            |
    |  ReAct loop, agent, tools, sessions     |
    +-----------------------------------------+
```

The existing trait system in `react-core` remains the **source of truth** for capability contracts. The plugin SDK wraps these traits with discovery, lifecycle, and configuration metadata. Suites continue to implement `WorkflowSuiteContract` / `Suite` and access capabilities through `SuiteCtx` -- the difference is that `SuiteCtx` is now populated from the plugin registry rather than hardcoded wiring.

---

## Part 1: Plugin SDK

### Plugin Manifest

Each plugin declares a manifest (either a struct implementing a trait, or a descriptor file for out-of-process plugins):

- **id**: unique identifier (e.g. `snowflake`, `openai-compat`, `storage-s3`)
- **kind**: what capability it provides -- `tool`, `llm`, `storage`, `vector`, `state`, `secrets`, or `channel`
- **version**: semver
- **config schema**: what configuration the plugin needs (connection strings, API keys, etc.)
- **dependencies**: optional, other plugins this one requires

### Plugin Trait

A `Plugin` trait in `react-core` that wraps the existing capability traits:

```rust
pub trait Plugin: Send + Sync {
    fn manifest(&self) -> PluginManifest;
    fn init(&self, config: Value) -> Result<(), PluginError>;
    fn provide_tools(&self) -> Vec<Box<dyn Tool>> { vec![] }
    fn provide_llm(&self) -> Option<DynLlm> { None }
    fn provide_storage(&self) -> Option<Arc<dyn StorageAdapter>> { None }
    fn provide_vector(&self) -> Option<Arc<dyn VectorStore>> { None }
    fn provide_state(&self) -> Option<Arc<dyn StateStore>> { None }
    fn provide_channel(&self) -> Option<Box<dyn Channel>> { None }
}
```

### Plugin Registry

Extends the existing `SuiteRegistry` / `ToolRegistry` pattern: a `PluginRegistry` that holds loaded plugins and provides lookup by kind + id. The runtime populates `SuiteCtx` from the registry at startup based on config.

### Migration Path

The existing providers (`provider-snowflake`, `provider-bigquery`, `provider-dbt`, `storage-s3`, `storage-local`, `vector-lance`) become the **first plugins** -- they already implement the right traits, they just need a `Plugin` wrapper and manifest.

---

## Part 2: Channel Layer

### Channel Trait

```rust
pub trait Channel: Send + Sync {
    fn id(&self) -> &str;
    fn start(&self, router: Arc<ChannelRouter>) -> Result<(), ChannelError>;
    fn stop(&self) -> Result<(), ChannelError>;
    fn send(&self, thread_id: &str, message: OutboundMessage) -> Result<(), ChannelError>;
}
```

### Channel Router

A new component that:

- Maps inbound platform messages to agent thread IDs (creating threads if needed)
- Converts platform-specific message formats to React's existing `UserRequest` / `ServerMessage` types
- Subscribes to `EventHub` to catch outbound `ServerMessage::Final` and routes them back to the originating channel
- Handles identity/pairing (which platform user maps to which React scope)

### Integration with EventHub

The existing `EventHub` already broadcasts `api::ServerMessage` to all subscribers. The channel layer:

- **Publishes** inbound messages as if they came from a WebSocket client (reusing the same `handle_new` / `handle_user` paths in `protocol.rs`)
- **Subscribes** to outbound messages and filters for threads it owns, forwarding responses to the originating platform

### Key Design Decisions

- Channels are **stateless adapters** -- they translate between platform wire format and React's thread model. All state lives in React's existing thread/session storage.
- Channels do **not** choose suites -- suite selection remains a runtime/config concern. A Slack channel and a WebSocket client hitting the same thread use the same suite.
- Media/attachments are out of initial scope -- text messages first, media as a follow-on.

---

## Crate Structure

- `src/core/src/plugin.rs` -- `Plugin` trait, `PluginManifest`, `PluginRegistry`
- `src/core/src/channel.rs` -- `Channel` trait, `ChannelRouter`, `OutboundMessage`, `InboundMessage`
- `src/transport/src/channel_bridge.rs` -- bridges channel router into the existing EventHub + suite runner
- Existing provider/storage crates get `Plugin` impl wrappers (minimal, ~20 lines each)

---

## What Does NOT Change

- `react-core`'s ReAct loop, tools, sessions, workflow runner -- untouched
- `Suite` / `WorkflowSuiteContract` / `PhaseExecutor` -- untouched, suites are not plugins
- `SuiteCtx` API -- same shape, populated differently (from plugin registry instead of hardcoded wiring)
- WebSocket transport -- remains as-is, channels are an additional ingress, not a replacement
- `EventHub` -- same broadcast model, channels are new subscribers/publishers

