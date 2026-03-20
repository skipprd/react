# Getting Started (Windows): MSSQL to Snowflake (EL + modeling, local storage)

This guide walks a **Windows** user through **extracting data from MSSQL**, **loading it into Snowflake** as a bronze (raw) tier, and then **building silver/gold dbt models** on top — all driven by `skippr-dbt` and `skippr`.

It uses:

- **skippr** for extract-and-load (MSSQL source -> Snowflake destination)
- **Snowflake** as the warehouse provider (destination)
- **Local filesystem storage** (writes under `.\.react\` — no S3 required)
- **Your own OpenAI API key** (or any OpenAI-compatible endpoint)

---

## Prerequisites

### skippr-dbt and skippr binaries

Download the Windows binaries:

| Binary | Purpose |
|--------|---------|
| `skippr-dbt.exe` | Orchestrates the full pipeline: EL, schema mapping, dbt modeling |
| `skippr.exe` (v6.15.0+) | Performs extract-and-load between source and destination |

Place both in a directory on your PATH, or reference them by full path.

Verify (from PowerShell):

```powershell
skippr-dbt --version
skippr --version
```

### Python 3.10+ and dbt

Python is required to run `dbt`, which `skippr-dbt` shells out to for model validation and materialisation.

Install Python via `winget`:

```powershell
winget install Python.Python.3.12
```

Verify (open a **new** PowerShell window after installing):

```powershell
python --version
```

### Docker Desktop (for local MSSQL dev)

If you don't already have an MSSQL instance, you can run one locally via Docker. Install [Docker Desktop for Windows](https://docs.docker.com/desktop/install/windows-install/) and make sure Docker is running.

### Snowflake account

You will need:

| Item | Example |
|------|---------|
| Account identifier | `RSSKNWT-KC53195` or `xy12345.us-east-1` |
| Username | service account or personal login |
| RSA key pair **or** password | Key-pair auth is required when MFA is enabled (most accounts) |
| Warehouse | `COMPUTE_WH` |
| Role | `TRANSFORMER` (or any role with read on raw + create on target schemas) |
| Database | `ANALYTICS` |
| Raw schema (bronze) | `RAW` — where MSSQL data will land |

### Generate an RSA key pair for Snowflake

Snowflake accounts with MFA enabled (the default for most orgs) **cannot** use password auth for headless/programmatic tools. Key-pair authentication bypasses MFA entirely and is the recommended approach.

You can use `openssl` from **Git Bash** (installed with Git for Windows) or install [Win64 OpenSSL](https://slproweb.com/products/Win32OpenSSL.html).

**1. Generate an unencrypted PKCS#8 private key:**

```bash
# Run in Git Bash
openssl genrsa 2048 | openssl pkcs8 -topk8 -inform PEM -out snowflake_key.p8 -nocrypt
```

This produces `snowflake_key.p8` — your private key. Keep it safe and never commit it to version control.

**2. Extract the public key:**

```bash
openssl rsa -in snowflake_key.p8 -pubout -out snowflake_key.pub
```

**3. Assign the public key to your Snowflake user:**

Log into Snowsight (or SnowSQL) and run:

```sql
ALTER USER YOURUSERNAME SET RSA_PUBLIC_KEY='MIIBIjANBgkqh...';
```

Copy the contents of `snowflake_key.pub` and paste **only** the base64 body — strip the `-----BEGIN PUBLIC KEY-----` and `-----END PUBLIC KEY-----` lines.

**4. Verify the key was assigned:**

```sql
DESC USER YOURUSERNAME;
-- Look for RSA_PUBLIC_KEY_FP — it should show a fingerprint like SHA256:...
```

You will reference the private key file path in the environment variables below.

---

## 1. Provide an MSSQL source

`skippr-dbt` uses `skippr` to extract data from MSSQL and load it into Snowflake automatically. You just need a running MSSQL instance with the tables you want to migrate.

### Option A — Use an existing MSSQL server

If you already have a SQL Server instance, note the connection string. It follows the ADO.NET format:

```
server=tcp:YOUR_HOST,1433;database=YOUR_DB;user id=sa;password=YOUR_PASS;TrustServerCertificate=true
```

### Option B — Spin up a local MSSQL with Docker (POC / dev)

If you don't have an existing instance, the repo includes a Docker Compose file that starts Azure SQL Edge and seeds sample tables:

```powershell
docker compose -f test\el-integration\docker-compose.yml up -d
```

Wait for the seed service to complete (check with `docker compose logs seed`), then use:

```
server=tcp:127.0.0.1,1433;database=testdb;user id=sa;password=Skippr!Test123;TrustServerCertificate=true
```

The seed creates three tables: `dbo.customers`, `dbo.orders`, and `dbo.order_items`.

To tear down later:

```powershell
docker compose -f test\el-integration\docker-compose.yml down -v
```

---

## 2. Set up the dbt environment

`skippr-dbt` shells out to `dbt` for model compilation, validation, and materialisation. You need `dbt-core` and the Snowflake adapter installed in a Python virtual environment.

### Create a virtual environment

```powershell
mkdir skippr-workspace
cd skippr-workspace

python -m venv .venv
.\.venv\Scripts\Activate.ps1
pip install --upgrade pip
```

### Install dbt with the Snowflake adapter

```powershell
pip install dbt-core dbt-snowflake
```

Confirm both are installed:

```powershell
dbt --version
```

You should see output listing `dbt-core` and `dbt-snowflake` with their versions.

### dbt packages (dbt_utils)

`skippr-dbt` generates a `packages.yml` in the dbt project that declares dependencies such as `dbt-labs/dbt_utils`. These are dbt packages (not pip packages). You will need to run `dbt deps` to install them after the project has been scaffolded:

```powershell
cd .react\local\dev\mssql_migration\dbt
dbt deps
```

**Important:** The virtual environment must be activated whenever you run `skippr-dbt`, so that `dbt` is available on PATH.

---

## 3. Create a config file

Create a file called `config.yaml` in your working directory:

```yaml
version: 1

storage:
  mode: local
  path: ./.react

scope:
  tenant: local
  workspace: dev
  project_id: mssql_migration

llm:
  provider: OPENAI_COMPAT
  base_url: https://api.openai.com
  reason_model: gpt-5.4
  task_model: gpt-5.4
  embed_model: text-embedding-3-small
  context_length: 8192
  http_timeout_secs: 120
  max_tokens: 8192
  temperature: 0.2
  top_p: 1.0

providers:
  warehouse:
    kind: snowflake
    database: ANALYTICS
    schema: RAW
    warehouse: COMPUTE_WH
    role: ACCOUNTADMIN

  el:
    enabled: true
    skippr_input:
      kind: mssql
      connection_string: "${MSSQL_CONNECTION_STRING}"

  catalog:
    enabled: true
    refresh_secs: 3600
    max_concurrency: 8

  dbt:
    enabled: true
    runner: host
    target: snowflake
    naming:
      target_schema: mssql_migration
      silver_suffix: silver
      gold_suffix: gold

  vector:
    enabled: true
```

**Key fields to edit:**

| Field | What to set |
|-------|-------------|
| `providers.warehouse.database` | Your Snowflake database (e.g. `ANALYTICS`) |
| `providers.warehouse.schema` | The bronze/raw schema where MSSQL data will land (e.g. `RAW`) |
| `providers.warehouse.warehouse` | Snowflake compute warehouse (e.g. `COMPUTE_WH`) |
| `providers.warehouse.role` | Snowflake role with appropriate grants |
| `providers.el.skippr_input.kind` | Source system — `mssql` for SQL Server |
| `providers.el.skippr_input.connection_string` | MSSQL connection string (use `${MSSQL_CONNECTION_STRING}` to read from env) |
| `providers.dbt.naming.target_schema` | Base name for dbt materialization — silver models land in `<target_schema>_silver`, gold in `<target_schema>_gold` |

---

## 4. Set environment variables

### Required

```powershell
$env:LLM_API_KEY = "sk-..."

$env:SNOWFLAKE_ACCOUNT = "RSSKNWT-KC12345"
$env:SNOWFLAKE_USER = "YOURUSERNAME"

$env:MSSQL_CONNECTION_STRING = "server=tcp:127.0.0.1,1433;database=testdb;user id=sa;password=Skippr!Test123;TrustServerCertificate=true"
```

### Snowflake authentication

**Key-pair auth (recommended — required when MFA is enabled):**

```powershell
$env:SNOWFLAKE_PRIVATE_KEY_PATH = "C:\path\to\snowflake_key.p8"
```

See [Generate an RSA key pair for Snowflake](#generate-an-rsa-key-pair-for-snowflake) above for how to create the key.

**Password auth (only if MFA is not enforced on the account):**

```powershell
$env:SNOWFLAKE_PASSWORD = "your_password"
```

The runtime generates a dbt `profiles.yml` at run time. It reads `SNOWFLAKE_ACCOUNT` and `SNOWFLAKE_USER` from the environment via Jinja `env_var()` calls — these are **not** written to disk in plaintext.

If `warehouse` or `role` are omitted from the YAML, the runtime also falls back to `SNOWFLAKE_WAREHOUSE` and `SNOWFLAKE_ROLE` env vars respectively.

### Optional

```powershell
$env:SKIPPR_LOG = "info"
```

---

## 5. Run skippr-dbt

Make sure your virtual environment is activated, then:

```powershell
skippr-dbt --log info run --config .\config.yaml --agent agent
```

### Terminal and log modes

By default, the `run` subcommand renders a **live terminal UI** (TUI) showing phases, tasks, and tool calls in real time. This requires a real TTY.

On Windows, PowerShell is often not recognized as a TTY, so **`--log info` is the recommended default** for Windows users. If you are using Windows Terminal (the modern terminal app), the TUI may work — try running without `--log` to see.

| Flag | Behavior |
|------|----------|
| _(none)_ | Terminal UI enabled (requires a TTY — may not work in all PowerShell hosts). |
| `--log info` | Disables the TUI; prints plain structured logs to stdout. **Recommended on Windows.** |
| `--log debug` | Same as `--log info` but with debug-level output. |
| `--log trace` | Most verbose — includes internal tracing spans. |
| `--terminal` | Explicitly enables the TUI (useful with the `serve` subcommand where it is not the default). |
| `--verbose-debug` | When combined with `--log debug` or `--log trace`, includes AWS S3 / HTTP request logs (suppressed by default). |

The terminal UI is **not supported** with `--parallel` multi-config runs.

If you see `"terminal mode not enabled: stdout is not a TTY"`, use `--log info`.

**Example (debug mode):**

```powershell
skippr-dbt --log debug run --config .\config.yaml --agent agent
```

### What happens when you run

Because `providers.el` is configured, the agent starts with extract-and-load before modeling:

1. **ElDiscover** — generates a `skippr.yml`, runs `skippr discover`, reads source schemas from MSSQL.
2. **ElSync** — runs `skippr sync --once` to extract rows from MSSQL and load them into Snowflake bronze tables.
3. **ElVerify** — confirms the destination tables exist and are queryable.
4. **Preflight** — discovers tables in the bronze schema (`RAW`).
5. **Plan** — designs a staging (silver) layer with one `stg_*` model per raw table.
6. **Author** — writes dbt SQL models with source references, type casting, and column renaming.
7. **Validate** — runs `dbt compile` / `dbt run` against Snowflake.
8. **Review** — checks the generated models for quality.

---

## 6. Verify outputs

### Local storage artifacts

With `storage.mode: local` and `storage.path: ./.react`, all artifacts land under:

```
.react\
└── local\
    └── dev\
        └── mssql_migration\
            ├── skippr\              # Generated skippr.yml and EL metadata
            ├── threads\             # Thread state and transcript JSON
            │   └── <thread-id>.json
            └── logs\                # Per-run logs
                └── <thread-id>.log
```

### dbt project files

The agent writes dbt models into the working dbt project directory. Look for:

```
models\
├── schema.yml                     # Source definitions (pointing at RAW tables)
└── staging\
    ├── stg_raw_customers.sql      # Silver model for customers
    └── stg_raw_orders.sql         # Silver model for orders
```

Each staging model uses `{{ source("raw", "customers") }}` to reference the bronze table and applies cleansing (casting, renaming, null handling).

### Run dbt manually (optional sanity check)

After the agent finishes, you can run dbt directly against the generated project:

```powershell
.\.venv\Scripts\Activate.ps1
dbt debug   --profiles-dir .   # Verify connection
dbt run     --profiles-dir .   # Materialize models into Snowflake
dbt test    --profiles-dir .   # Run any generated tests
```

Verify in Snowflake:

```sql
USE DATABASE ANALYTICS;
SHOW SCHEMAS LIKE '%mssql_migration%';
-- Expect: mssql_migration_silver, mssql_migration_gold

SELECT * FROM mssql_migration_silver.stg_raw_customers LIMIT 10;
```

---

## Troubleshooting

### `unsupported providers.warehouse.kind 'snowflake'`

The Snowflake warehouse provider is being rolled out incrementally. If you hit this error, your binary does not yet include the Snowflake query provider. Contact support to obtain an updated binary.

### `terminal mode not enabled: stdout is not a TTY`

This happens when the terminal UI is requested but stdout isn't a real terminal. Common in older PowerShell hosts, ISE, or piped output.

**Fix:** Run with `--log info` to use plain log output:

```powershell
skippr-dbt --log info run --config .\config.yaml --agent agent
```

### MFA error: `390197 — Multi-factor authentication is required`

This means your Snowflake account enforces MFA, so password auth cannot work for headless tools. Switch to key-pair authentication:

1. Generate an RSA key pair — see [Generate an RSA key pair for Snowflake](#generate-an-rsa-key-pair-for-snowflake)
2. Replace `SNOWFLAKE_PASSWORD` with `SNOWFLAKE_PRIVATE_KEY_PATH` pointing to your `.p8` file
3. Restart `skippr-dbt`

### Snowflake connection errors (dbt)

| Symptom | Fix |
|---------|-----|
| `Failed to connect: 250001` | Verify `SNOWFLAKE_ACCOUNT` format — use the org-account form (e.g. `MYORG-MYACCOUNT`) or include the region (e.g. `xy12345.us-east-1`) |
| `Incorrect username or password` | Check `SNOWFLAKE_USER` and `SNOWFLAKE_PASSWORD` / `SNOWFLAKE_PRIVATE_KEY_PATH` env vars |
| `Insufficient privileges` | Ensure the role has USAGE on the warehouse, database, and raw schema, plus CREATE SCHEMA on the database for silver/gold |

### `dbt: command not found` or `dbt-snowflake` adapter missing

Activate your Python virtual environment and confirm the adapter is installed:

```powershell
.\.venv\Scripts\Activate.ps1
dbt --version
```

If `dbt-snowflake` is not listed, reinstall:

```powershell
pip install dbt-core dbt-snowflake
```

### LLM errors (401 / timeouts)

- Confirm `$env:LLM_API_KEY` is set and valid.
- If requests timeout on large contexts, increase `llm.http_timeout_secs` in the YAML.

### MSSQL connection errors

| Symptom | Fix |
|---------|-----|
| `Login failed for user 'sa'` | Verify the password in `MSSQL_CONNECTION_STRING` and that SQL Server auth is enabled |
| `Cannot open database` | Confirm the database name in the connection string exists |
| `Connection refused` | Check the host/port — Docker MSSQL runs on `127.0.0.1:1433` by default |

---

## Quick reference

| What | Where |
|------|-------|
| Config file | `config.yaml` (working directory) |
| Local artifacts | `.react\local\dev\mssql_migration\` |
| Generated skippr.yml | `.react\local\dev\mssql_migration\skippr\skippr.yml` |
| dbt models | `models\staging\stg_*.sql` |
| Source definitions | `models\schema.yml` |
| Thread logs | `.react\local\dev\mssql_migration\logs\<thread-id>.log` |
| Snowflake raw schema | `ANALYTICS.RAW` |
| Snowflake silver schema | `ANALYTICS.MSSQL_MIGRATION_SILVER` |
| Snowflake gold schema | `ANALYTICS.MSSQL_MIGRATION_GOLD` |
