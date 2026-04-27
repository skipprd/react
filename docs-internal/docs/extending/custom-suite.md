# Custom Suite

Build a new suite to define a custom agent workflow with its own tools, prompts, and policy.

## Overview

A suite is a struct that implements the `Suite` trait. It defines:

- **Identity** — suite ID, label, supported agent types
- **Handlers** — `handle_new`, `handle_open`, `handle_user` for client frame processing
- **Internals** — tool registry, system prompt, agent policy, optional preflight

## Steps

### 1. Create the module

Create a new directory under `src/suites/react-suites/src/`:

```
src/suites/react-suites/src/my_suite/
├── mod.rs
├── prompts/
│   └── mod.rs
└── tools/
    └── mod.rs
```

### 2. Implement the Suite trait

```rust
use async_trait::async_trait;
use react_core::agent::{Agent, AgentCtx, DefaultPolicy, RunOutcome};
use react_core::tools::ToolRegistry;
use crate::flow_frame::FlowFrame;
use crate::suite::{Suite, SuiteCtx};

pub struct MySuite;

#[async_trait]
impl Suite for MySuite {
    fn id(&self) -> &'static str {
        "my_suite"
    }

    fn label(&self) -> &'static str {
        "My Suite"
    }

    fn supported_agent_types(&self) -> Vec<String> {
        vec!["agent".to_string()]
    }

    fn default_agent_type(&self) -> &'static str {
        "agent"
    }

    fn phase_order(&self, _agent_type: &str) -> Vec<String> {
        Vec::new()
    }

    async fn handle_new(
        &self,
        sctx: &SuiteCtx,
        agent_type: &str,
        question: Option<&str>,
        thread_id: &str,
    ) -> Result<Vec<FlowFrame>, String> {
        let tools = self.build_tools(sctx)?;
        let ctx = self.build_agent_ctx(sctx, thread_id);

        match Agent::run(&tools, &ctx, question).await? {
            RunOutcome::Final { result, .. } => {
                Ok(vec![FlowFrame::Final(result)])
            }
            RunOutcome::AwaitUser { prompt, .. } => {
                Ok(vec![FlowFrame::AwaitUser { prompt }])
            }
            RunOutcome::AwaitApproval { prompt, .. } => {
                Ok(vec![FlowFrame::AwaitApproval { prompt }])
            }
        }
    }

    // Implement handle_open and handle_user similarly...
}
```

### 3. Build the tool registry

Create tools and register them:

```rust
impl MySuite {
    fn build_tools(&self, sctx: &SuiteCtx) -> Result<ToolRegistry, String> {
        let mut reg = ToolRegistry::new();
        reg.register(MyCustomTool);
        // Register more tools...
        Ok(reg)
    }
}
```

See [Custom Tool](custom-tool.md) for implementing tools.

### 4. Register the suite

Add the suite to the registry in your host:

```rust
pub fn default_registry() -> SuiteRegistry {
    let mut reg = SuiteRegistry::new();
    reg.register(crate::kb::KbSuite);
    reg.register(crate::my_suite::MySuite);
    reg
}
```

### 5. Export the module

Export the suite module from the crate that owns it.

## Choosing a policy

- Use `DefaultPolicy` for simple workflows that accept any well-formed final
- Implement a [Custom Policy](custom-policy.md) if you need final validation, interrupt gates, or context injection

## Choosing agent types

Agent types let a single suite support multiple modes. For example, the Data Engineer suite supports `ask` (question answering), `agent` (full workflow), and `review` (red-team). Each agent type can wire different tools and prompts.

## Next steps

- [Custom Tool](custom-tool.md) — implementing the Tool trait
- [Custom Policy](custom-policy.md) — implementing AgentPolicy
- [Suites concept](../concepts/suites.md) — how suites fit into the architecture
