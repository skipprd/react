[![CI](https://github.com/skipprd/react/actions/workflows/ci.yml/badge.svg)](https://github.com/skipprd/react/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/skipprd/react)](https://github.com/skipprd/react/releases)

## ReAct (react crate)

A suite-neutral ReAct host/runtime library. Product binaries such as **`sde`** (Skippr Data Engineer) live in their product repositories and register suites on top of the shared `react` runtime and `react-core` kernel.

Docs: https://react.skippr.io

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

Headless and product-specific CLIs are owned by downstream host repositories. Skippr Data Engineer is **`sde`** in `skipprd/sde`.

Consume this workspace as a git/path dependency. CodeArtifact `react-cargo` is not the public install path.

### WebSocket Server

Interactive WebSocket serving runs through host binaries or daemons that call the shared `react` HTTP/WS frontends programmatically. A host registers suites, resolves configuration, wires capabilities, and then calls the runtime entrypoints.

### Configuration

`react` owns generic runtime configuration: storage, scope, LLM settings, server settings, and provider extension points. Product repositories own domain-specific suite configuration.

### Workspace

The React workspace contains the shared runtime/core crates, generic transport/view crates, storage/vector adapters, and non-Skippr suites such as `react-suite-kb`. Skippr Data Engineer code is owned by `skipprd/sde`.


This repository is **source-available** under [PolyForm Shield 1.0.0](./LICENSE), not OSI open source.

Licensor Line of Business: Skippr ReAct (https://react.skippr.io)

Used by [Skippr Data Engineer](https://data-engineer.skippr.io) (`sde`).
