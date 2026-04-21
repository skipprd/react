# Windows BigQuery Quickstart

This guide shows how to run the `data_engineer` workflow on Windows against BigQuery using the `skippr` host binary.

On Windows, the usual flow is to run `skippr`, which embeds the shared ReAct runtime for the `data_engineer` workflow.

## Prerequisites

Install these first:

1. Rust via [rustup](https://rustup.rs/).
2. Visual Studio Build Tools with "Desktop development with C++".
3. Protocol Buffers (`protoc`), for example with `winget install Google.Protobuf`.
4. Python 3.11+.
5. Google Cloud SDK (`gcloud`).

Verify the basics in PowerShell:

```powershell
rustc --version
cargo --version
protoc --version
python --version
gcloud --version
```

## Clone And Build

From your workspace root:

```powershell
git clone <your-react-repo-url>
cd react
cargo build -p skippr
```

The first build can take a while because Arrow, Lance, and the AWS crates are large.

## Python And dbt

Create a virtual environment and install the BigQuery adapter:

```powershell
python -m venv .venv
.\.venv\Scripts\Activate.ps1
python -m pip install --upgrade pip
pip install dbt-core dbt-bigquery
```

Keep that shell activated while running `skippr`.

## BigQuery Authentication

Authenticate for local development:

```powershell
gcloud auth application-default login
```

If you prefer a service account JSON key:

```powershell
$env:GOOGLE_APPLICATION_CREDENTIALS = "C:\path\to\service-account.json"
```

Your principal typically needs:

- BigQuery Job User
- BigQuery Data Viewer
- BigQuery Data Editor

## Example Config

Create `.\react\react-bigquery.local.yaml`:

```yaml
version: 1

storage:
  mode: local
  path: ./.react

scope:
  tenant: local
  workspace: dev
  project_id: bigquery_demo

llm:
  provider: OPENAI_COMPAT
  base_url: https://api.openai.com
  reason_model: gpt-5.4
  task_model: gpt-5.4
  embed_model: text-embedding-3-small
  max_tokens: 8192

providers:
  warehouse:
    kind: bigquery
    project: your-gcp-project-id
    dataset: analytics
    location: us
  catalog:
    enabled: true
  dbt:
    enabled: true
    runner: host
    target: bigquery
    naming:
      target_schema: analytics
      silver_suffix: silver
      gold_suffix: gold
  vector:
    enabled: true
```

## Required Environment Variables

At minimum, set your LLM key:

```powershell
$env:LLM_API_KEY = "sk-..."
```

Useful optional overrides:

```powershell
$env:REACT_PLAIN_PROGRESS = "1"
$env:LLM_MAX_TOKENS = "8192"
```

## Run The Workflow

For local Windows development, plain logs are usually the smoothest path:

```powershell
.\.venv\Scripts\Activate.ps1
cargo run -p skippr -- --log info run
```

`skippr` resolves the local project config, assembles the `data_engineer` suite, and runs the shared headless ReAct runtime.

## Where Files Go

With `storage.path: ./.react` and `scope: { tenant, workspace, project_id }`:

- Main runtime log: `.\.react\logs\react.log`
- Thread logs: `.\.react\<tenant>\<workspace>\<project_id>\logs\<thread_id>.log`
- Thread state: `.\.react\<tenant>\<workspace>\<project_id>\threads\`
- dbt artifacts: `.\.react\<tenant>\<workspace>\<project_id>\dbt\`

Set `REACT_LOG_DIR` if you want the top-level runtime log somewhere else.

## Troubleshooting

### `Could not find protoc`

Install protobuf and confirm:

```powershell
protoc --version
```

### BigQuery credential errors

If you see ADC or 401/403 failures:

- rerun `gcloud auth application-default login`
- verify `GOOGLE_APPLICATION_CREDENTIALS` if using a service account
- confirm the project, dataset, and region in `providers.warehouse`

### BigQuery location mismatch

Make sure `providers.warehouse.location` matches the dataset location.

### `dbt` not found

Activate the venv and reinstall:

```powershell
.\.venv\Scripts\Activate.ps1
pip install dbt-core dbt-bigquery
```

### LLM failures or truncated outputs

If the model is rate-limited or truncating tool-call JSON:

- verify `LLM_API_KEY`
- increase `llm.max_tokens`
- or set `$env:LLM_MAX_TOKENS = "8192"`
