## ReAct (`react` crate)

A suite-neutral **ReAct host/runtime library**. Binaries such as `skippr`, `skippr-admin`, and `goggles-reactd` register suites and add product-specific capabilities on top of the shared `react` runtime and `react-core` kernel.

### Getting started guides

| Guide | Source → Destination | Notes |
|-------|---------------------|-------|
| [`getting-started.md`](getting-started.md) | MSSQL → Snowflake | Bronze tier manually loaded, then dbt modeling via `skippr` |
| [`GETTING_STARTED_WINDOWS_BIGQUERY.md`](GETTING_STARTED_WINDOWS_BIGQUERY.md) | BigQuery | Windows-specific, local storage |
| [`extract-and-load.md`](extract-and-load.md) | Any → Any (via skippr) | EL architecture and phase reference |

---

### Prerequisites

#### Rust toolchain

The workspace uses the Rust version pinned in `rust-toolchain.toml`. Install via [rustup](https://rustup.rs/); `cargo build` will fetch the correct toolchain automatically.

`protoc` (Protocol Buffers compiler) is required at compile time by Arrow/Lance dependencies:

```bash
# macOS
brew install protobuf

# Ubuntu / Debian
sudo apt install -y protobuf-compiler
```

#### Python virtual environment & dbt

The runtime shells out to `dbt` for project scaffolding and validation. Install it in an isolated venv:

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install --upgrade pip
pip install dbt-core dbt-snowflake     # swap adapter as needed: dbt-bigquery, dbt-postgres, dbt-athena-community
```

Make sure the venv is activated (`source .venv/bin/activate`) whenever you run `skippr` or the WebSocket server.

#### `skippr` binary (required for EL workflows)

When `providers.el` is enabled in a config, `skippr` spawns the **`skippr`** binary (from the [`skipprd`](../skipprd) repo) as a subprocess for data extraction and loading. The binary must be on `PATH` (or configured via `providers.el.skippr_binary`).

**Build from the skipprd repo (`--release` recommended):**

```bash
cd ../skipprd                         # sibling directory to react
cargo build --release                 # ~5 min first build
```

**Install to PATH:**

```bash
sudo cp target/release/skippr /usr/local/bin/skippr
```

**Verify:**

```bash
skippr --version
# skippr 0.0.0-git
```

> **Why use release mode?** `skippr sync` can process large datasets and write to remote warehouses. Debug builds are much slower and can stall under load, which may cause `skippr` to hit its idle timeout (default 120 s). For real data movement, use a release build.

#### Git hooks (recommended)

To catch OpenAPI generator drift before CI, install the repo's versioned Git hooks:

```bash
bash scripts/install-git-hooks.sh
```

The pre-commit hook runs the core OpenAPI generator when staged changes touch the core spec, generator scripts, or generated Rust models, re-stages `src/transport/src/ws/api_gen`, and fails early if Docker or a local `openapi-generator-cli.jar` is unavailable.

**Alternative — custom path instead of PATH:**

Instead of installing to `/usr/local/bin`, you can point the config at the binary directly:

```yaml
providers:
  el:
    enabled: true
    skippr_binary: ../skipprd/target/release/skippr
    skippr_input:
      kind: s3
      s3_bucket: my-bucket
```

---

### Running

#### Headless CLI (`skippr`)

The `skippr` binary runs the `data_engineer` suite in headless (non-interactive) mode. This is the primary way to run EL + dbt workflows end-to-end.

```bash
source .venv/bin/activate

cargo run -p skippr -- --log info run \
  --config ./react-bikehire-snow.yaml \
  --agent agent
```

| Flag | Purpose |
|------|---------|
| `--log info` | Log level (`debug`, `info`, `warn`, `error`) |
| `--config <path>` | YAML config file |
| `--agent <type>` | Agent type: `agent` (full pipeline), `ask` (interactive Q&A), `review`, `model`, `cleanse` |

When `providers.el.enabled` is `true`, the agent runs EL phases first (discover → sync → verify) before entering dbt modeling phases.

#### WebSocket server

Interactive WebSocket serving runs through host binaries or daemons that call the shared `react` HTTP/WS frontends programmatically.

---

### Configuration

#### Config file overview

Config files are YAML with these top-level sections:

```yaml
version: 1

storage:
  mode: local                      # local | s3
  path: ./.react                   # local storage root

scope:
  tenant: local
  workspace: dev
  project_id: my_project           # unique per project — determines artifact directory

llm:
  provider: OPENAI_COMPAT
  base_url: https://api.openai.com
  reason_model: gpt-5.4            # used for planning, critique, review
  task_model: gpt-5.4              # used for tool-calling, mechanical tasks
  embed_model: text-embedding-3-small
  max_tokens: 8192

providers:
  warehouse: { ... }               # destination warehouse (snowflake, bigquery, athena, postgres)
  el: { ... }                      # extract-and-load via skippr (optional)
  catalog: { ... }                 # semantic catalog
  dbt: { ... }                     # dbt runner
  vector: { ... }                  # vector store
```

See [`react-bikehire-snow.yaml`](react-bikehire-snow.yaml) for a complete S3 → Snowflake example.

#### EL (Extract and Load) configuration

The `providers.el` section enables the EL phases that run before dbt modeling. When absent or `enabled: false`, the workflow starts at the `Preflight` phase with no EL behaviour.

```yaml
providers:
  el:
    enabled: true
    skippr_binary: skippr           # optional — binary name or path (default: "skippr")
    skippr_input:                   # opaque block passed through to skippr.yml data_inputs
      kind: s3                      # input plugin: s3, mssql
      s3_bucket: my-bucket
      s3_prefix: data/
```

The destination is always `providers.warehouse` — `skippr` generates `skippr.yml` with the warehouse credentials mapped to skippr's `data_outputs` format.

| Input kind | Required fields | Auth |
|-----------|----------------|------|
| `s3` | `s3_bucket`, `s3_prefix` | AWS credentials from environment (`AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` or instance profile) |
| `mssql` | `connection_string` | Value or `${ENV_VAR}` reference |

#### Environment variables

| Variable | Required | Purpose |
|----------|----------|---------|
| `SKIPPR_API_KEY` | CI/CD | Per-user API key for non-interactive auth. When set, `cmd_run` exchanges it for JWTs automatically. |
| `LLM_API_KEY` | No | Optional override — use your own LLM key instead of the server-provided one. |
| `SNOWFLAKE_ACCOUNT` | Snowflake only | Account identifier (e.g. `MYORG-MYACCOUNT`) |
| `SNOWFLAKE_USER` | Snowflake only | Username |
| `SNOWFLAKE_PRIVATE_KEY_PATH` | Snowflake key-pair auth | Path to `.p8` private key file |
| `SNOWFLAKE_PASSWORD` | Snowflake password auth | Password (only if MFA is not enforced) |
| `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | S3 sources | AWS credentials for skippr to read source data |
| `MSSQL_CONNECTION_STRING` | MSSQL sources | ADO.NET connection string |
| `LLM_MAX_TOKENS` | No | Override LLM output token limit (default: from `llm.max_tokens` in config) |
| `SKIPPR_AUTH_URL` | No | Override the auth service URL (default: `https://auth.skippr.io`) |

#### Authentication

Authentication is required to run `skippr`. Two modes are supported:

1. **Interactive login** — `skippr user login` (phone OTP, stores tokens in `~/.skippr/credentials.json`)
2. **API key** — set `SKIPPR_API_KEY=sk_live_...` env var (recommended for CI/CD)

Authentication provides cloud S3 storage for pipeline artifacts, a server-provided LLM API key, and usage metering with credit-based billing. You may optionally set `LLM_API_KEY` to override the server-provided key with your own.

#### LLM output size

The **effective output token limit** is controlled by **`LLM_MAX_TOKENS`** (env var) or `llm.max_tokens` (config). If you see `parser_error invalid JSON twice` during large batch scaffolds, your model output is being truncated. For `gpt-5.x`, **8192** is a reasonable starting point.

---

### Troubleshooting

#### `skippr sync stalled — no output for 120s`

The `skippr sync` subprocess produced no JSON output within the idle timeout. Common causes:

| Cause | Fix |
|-------|-----|
| **Debug build of skippr** | Rebuild in release mode: `cd ../skipprd && cargo build --release`, then copy to PATH |
| **skippr binary not up to date** | Rebuild from latest `skipprd` source and reinstall |
| **Warehouse connection hanging** | Run `skippr sync` manually to see stderr: `cd .react/local/dev/<project>/skippr && skippr --log debug sync --pipeline el_pipeline --once --output json` |
| **AWS credentials missing** | Ensure `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` are set for S3 sources |
| **Snowflake auth issue** | Check `SNOWFLAKE_PRIVATE_KEY_PATH` points to a valid `.p8` file |

To test skippr independently, run it directly in the generated data directory:

```bash
cd .react/local/dev/<project_id>/skippr
skippr --log debug sync --pipeline el_pipeline --once --output json
```

This runs the same command `skippr` would invoke and shows full output including stderr.

#### `failed to spawn skippr: No such file or directory`

The `skippr` binary is not on `PATH`. Either install it (see [skippr binary](#skippr-binary-required-for-el-workflows)) or set `providers.el.skippr_binary` to an absolute or relative path.

#### MFA error: `390197 — Multi-factor authentication is required`

Switch from password auth to key-pair auth. See [`getting-started.md`](getting-started.md#generate-an-rsa-key-pair-for-snowflake) for key generation steps.

#### `parser_error invalid JSON twice` / LLM truncation

Increase `llm.max_tokens` in the config or set `LLM_MAX_TOKENS=8192` as an env var.

#### `dbt: command not found`

Activate the Python virtual environment before running `skippr`:

```bash
source .venv/bin/activate
dbt --version
```

#### Local storage artifacts

With `storage.mode: local`, all artifacts land under `<storage.path>/<tenant>/<workspace>/<project_id>/`:

```
.react/local/dev/my_project/
├── threads/          Thread state and transcript JSON
├── dbt/              Generated dbt project
└── skippr/           Generated skippr.yml, pipeline metadata, sync state
    ├── skippr.yml
    └── default_el_pipeline/
        └── metadata.json
```

---
