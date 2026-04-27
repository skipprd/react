## ReAct (react crate)

A suite-neutral ReAct host/runtime library. Product binaries such as `skippr`, `skippr-admin`, and `goggles-reactd` live in their product repositories and register suites on top of the shared `react` runtime and `react-core` kernel.

### Prerequisites

The workspace uses the Rust version pinned in `rust-toolchain.toml`. Install via [rustup](https://rustup.rs/); `cargo build` will fetch the correct toolchain automatically.

`protoc` is required at compile time by Arrow/Lance-related dependencies:

```bash
# macOS
brew install protobuf

# Ubuntu / Debian
sudo apt install -y protobuf-compiler
```

### Git hooks

To catch OpenAPI generator drift before CI, install the repo's versioned Git hooks:

```bash
bash scripts/install-git-hooks.sh
```

The pre-commit hook runs the core OpenAPI generator when staged changes touch the core spec, generator scripts, or generated Rust models, re-stages `src/transport/src/ws/api_gen`, and fails early if Docker or a local `openapi-generator-cli.jar` is unavailable.

### Running

Build and test the generic runtime crates from this repo:

```bash
cargo build -p react
cargo test -p react --lib --tests
```

Headless and product-specific CLIs are owned by downstream host repositories. For Skippr data-engineer workflows, use the `skipprd` repository.

### Publishing

Generic React crates are published to the private CodeArtifact Cargo registry `skippr/react-cargo`.

For local publish or dependency-resolution testing, log Cargo into CodeArtifact first:

```bash
AWS_PROFILE=skippr-prod aws codeartifact get-authorization-token \
  --domain skippr \
  --domain-owner 132355036174 \
  --region us-east-1 \
  --query authorizationToken \
  --output text
```

Set the returned token as `CARGO_REGISTRIES_REACT_CARGO_TOKEN`. The release workflow does this automatically before publishing crates in dependency order.

### WebSocket Server

Interactive WebSocket serving runs through host binaries or daemons that call the shared `react` HTTP/WS frontends programmatically. A host registers suites, resolves configuration, wires capabilities, and then calls the runtime entrypoints.

### Configuration

`react` owns generic runtime configuration: storage, scope, LLM settings, server settings, and provider extension points. Product repositories own domain-specific suite configuration.

### Workspace

The React workspace contains the shared runtime/core crates, generic transport/view crates, storage/vector adapters, and non-Skippr suites such as `react-suite-kb`. Skippr-specific data-engineer code is owned by `skipprd`.
