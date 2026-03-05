# Installation

## Prerequisites

ReAct is built with Rust. You need the following on your system:

| Dependency | Purpose |
|---|---|
| **Rust toolchain** (stable) | Build the `react` binary |
| **protoc** (Protocol Buffers compiler) | Required by the LanceDB dependency at build time |
| **Python 3.10+** | Required for dbt when using the Data Engineer suite |
| **Google Cloud SDK** (optional) | Needed for BigQuery authentication via Application Default Credentials |

### Rust

Install via [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Verify:

```bash
cargo --version
```

### Protocol Buffers

Install `protoc` for your platform:

```bash
# macOS
brew install protobuf

# Ubuntu/Debian
sudo apt-get install -y protobuf-compiler

# Windows (via Chocolatey)
choco install protoc
```

Verify:

```bash
protoc --version
```

## Build from source

Clone the repository and build the `react` crate:

```bash
git clone <your-repo-url>
cd react
cargo build -p react
```

The binary is built to `target/debug/react` (or `target/release/react` with `--release`).

## Python virtual environment and dbt

The Data Engineer suite shells out to `dbt` for project scaffolding and validation. Install it in an isolated venv:

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install --upgrade pip
pip install dbt-core dbt-athena-community
```

Replace `dbt-athena-community` with the adapter for your warehouse:

| Warehouse | dbt adapter package |
|---|---|
| Athena | `dbt-athena-community` |
| BigQuery | `dbt-bigquery` |
| Postgres | `dbt-postgres` |
| Snowflake | `dbt-snowflake` |

Verify:

```bash
dbt --version
```

Make sure the venv is activated whenever you run the server or any workflow that invokes dbt.

## Feature flags

The `react` crate supports optional Cargo features:

| Feature | Default | Description |
|---|---|---|
| `llama_cpp` | off | Enables the Llama.cpp local LLM provider |

Enable a feature at build time:

```bash
cargo build -p react --features llama_cpp
```

## Platform notes

For a detailed Windows + BigQuery walkthrough including troubleshooting, see `GETTING_STARTED_WINDOWS_BIGQUERY.md` in the repository root.
