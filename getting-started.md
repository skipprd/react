# Getting Started: MSSQL to Snowflake (bronze tier, local storage)

This guide walks a technical user through migrating data from **Microsoft SQL Server (MSSQL)** into **Snowflake** as a bronze (raw) tier, then configuring and running this project to build silver/gold dbt models on top.

It uses:

- **Snowflake** as the warehouse provider (destination)
- **Local filesystem storage** (writes under `./.react/` — no S3 required)
- **Your own OpenAI API key** (or any OpenAI-compatible endpoint)

---

## Prerequisites

### Skippr binary

Download the Skippr binary for your platform from the link provided to you. Place it somewhere on your PATH (or reference it by full path in the commands below).

Verify:

```bash
skippr --version
```

### Python 3.10+ and dbt

Python is required to run `dbt`, which Skippr shells out to for model validation and materialisation.

| Tool | Why | Install |
|------|-----|---------|
| **Python 3.10+** | Hosts `dbt` | [python.org](https://www.python.org/) |

Verify:

```bash
python3 --version      # or `python --version` on Windows
```

### Snowflake account

You will need:

| Item | Example |
|------|---------|
| Account identifier | `xy12345.us-east-1` |
| Username / password | service account or personal login |
| Warehouse | `COMPUTE_WH` |
| Role | `TRANSFORMER` (or any role with read on raw + create on target schemas) |
| Database | `ANALYTICS` |
| Raw schema (bronze) | `RAW` — where MSSQL data will land |

---

## 1. Export MSSQL data into Snowflake (bronze tier)

The bronze tier is raw source data. This project builds dbt models **on top of** bronze — it does not perform the extraction itself. You need to land your MSSQL tables into a Snowflake schema before the agent can model them.

Pick whichever approach fits your environment:

### Option A — CSV staging (simplest for a POC)

```sql
-- On MSSQL: export each table to CSV
bcp MyDatabase.dbo.customers out customers.csv -c -t, -S localhost -U sa -P yourpass
bcp MyDatabase.dbo.orders    out orders.csv    -c -t, -S localhost -U sa -P yourpass
```

Then load into Snowflake:

```sql
-- In Snowflake
CREATE DATABASE IF NOT EXISTS ANALYTICS;
CREATE SCHEMA  IF NOT EXISTS ANALYTICS.RAW;

CREATE OR REPLACE STAGE ANALYTICS.RAW.mssql_stage;

PUT file://customers.csv @ANALYTICS.RAW.mssql_stage AUTO_COMPRESS=TRUE;
PUT file://orders.csv    @ANALYTICS.RAW.mssql_stage AUTO_COMPRESS=TRUE;

COPY INTO ANALYTICS.RAW.customers FROM @ANALYTICS.RAW.mssql_stage/customers.csv
  FILE_FORMAT = (TYPE = CSV FIELD_OPTIONALLY_ENCLOSED_BY = '"' SKIP_HEADER = 1);

COPY INTO ANALYTICS.RAW.orders FROM @ANALYTICS.RAW.mssql_stage/orders.csv
  FILE_FORMAT = (TYPE = CSV FIELD_OPTIONALLY_ENCLOSED_BY = '"' SKIP_HEADER = 1);
```

### Option B — Snowflake Connector for SQL Server

Snowflake provides a managed connector that replicates MSSQL tables continuously. See the [Snowflake docs on the SQL Server connector](https://docs.snowflake.com/en/user-guide/connector-mssql). This is recommended for production but heavier to set up for a POC.

### Migration is on the Roadmap

In future, the Skippr project will support seemless migration of data between systems during the modeling phase.

### Verify bronze data

After loading, confirm the tables are visible:

```sql
USE DATABASE ANALYTICS;
USE SCHEMA RAW;
SHOW TABLES;
SELECT * FROM customers LIMIT 10;
```

---

## 2. Set up the dbt environment

Skippr shells out to `dbt` for model compilation, validation, and materialisation. You need `dbt-core` and the Snowflake adapter installed in a Python virtual environment.

### Create a virtual environment

```bash
mkdir -p skippr-workspace && cd skippr-workspace

python3 -m venv .venv
source .venv/bin/activate          # macOS / Linux
# .\.venv\Scripts\Activate.ps1     # Windows PowerShell
pip install --upgrade pip
```

### Install dbt with the Snowflake adapter

```bash
pip install dbt-core dbt-snowflake
```

Confirm both are installed:

```bash
dbt --version
```

You should see output listing `dbt-core` and `dbt-snowflake` with their versions.

### dbt packages (dbt_utils)

Skippr generates a `packages.yml` in the dbt project that declares dependencies such as `dbt-labs/dbt_utils`. These are dbt packages (not pip packages). You will need to run `dbt deps` to install them after Skippr has scaffolded the project:

```bash
cd .react/local/dev/mssql_migration/dbt
dbt deps
```

**Important:** The virtual environment must be activated whenever you run Skippr, so that `dbt` is available on PATH.

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
    role: TRANSFORMER

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
| `providers.warehouse.schema` | The bronze/raw schema where MSSQL data lives (e.g. `RAW`) |
| `providers.warehouse.warehouse` | Snowflake compute warehouse (e.g. `COMPUTE_WH`) |
| `providers.warehouse.role` | Snowflake role with appropriate grants |
| `providers.dbt.naming.target_schema` | Base name for dbt materialization — silver models land in `<target_schema>_silver`, gold in `<target_schema>_gold` |

---

## 4. Set environment variables

### Required

```bash
export LLM_API_KEY="sk-..."

export SNOWFLAKE_ACCOUNT="xy12345.us-east-1"
export SNOWFLAKE_USER="your_username"
export SNOWFLAKE_PASSWORD="your_password"
```

The runtime generates a dbt `profiles.yml` at run time. It reads `SNOWFLAKE_ACCOUNT`, `SNOWFLAKE_USER`, and `SNOWFLAKE_PASSWORD` from the environment via Jinja `env_var()` calls — these are **not** written to disk in plaintext.

If `warehouse` or `role` are omitted from the YAML, the runtime also falls back to `SNOWFLAKE_WAREHOUSE` and `SNOWFLAKE_ROLE` env vars respectively.

### Optional

```bash
export SKIPPR_LOG=info
```

---

## 5. Run Skippr

Make sure your virtual environment is activated, then:

```bash
skippr run \
  --config config.yaml \
  --suite-id data_engineer \
  --agent agent \
  --log info
```

The agent will:

1. **Discover** tables in your bronze schema (`RAW`).
2. **Plan** a staging (silver) layer — one `stg_*` model per raw table.
3. **Author** dbt SQL models with source references, type casting, and column renaming.
4. **Validate** each model by running `dbt compile` / `dbt run` against Snowflake.
5. **Review** the generated models for quality.

---

## 6. Verify outputs

### Local storage artifacts

With `storage.mode: local` and `storage.path: ./.react`, all artifacts land under:

```
.react/
└── local/
    └── dev/
        └── mssql_migration/
            ├── threads/          # Thread state and transcript JSON
            │   └── <thread-id>.json
            └── logs/             # Per-run logs
                └── <thread-id>.log
```

### dbt project files

The agent writes dbt models into the working dbt project directory. Look for:

```
models/
├── schema.yml                     # Source definitions (pointing at RAW tables)
└── staging/
    ├── stg_raw_customers.sql      # Silver model for customers
    └── stg_raw_orders.sql         # Silver model for orders
```

Each staging model uses `{{ source("raw", "customers") }}` to reference the bronze table and applies cleansing (casting, renaming, null handling).

### Run dbt manually (optional sanity check)

After the agent finishes, you can run dbt directly against the generated project:

```bash
source .venv/bin/activate
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

### Snowflake connection errors (dbt)

| Symptom | Fix |
|---------|-----|
| `Failed to connect: 250001` | Verify `SNOWFLAKE_ACCOUNT` format — it must include the region (e.g. `xy12345.us-east-1`) |
| `Incorrect username or password` | Check `SNOWFLAKE_USER` and `SNOWFLAKE_PASSWORD` env vars |
| `Insufficient privileges` | Ensure the role has USAGE on the warehouse, database, and raw schema, plus CREATE SCHEMA on the database for silver/gold |

### `dbt: command not found` or `dbt-snowflake` adapter missing

Activate your Python virtual environment and confirm the adapter is installed:

```bash
source .venv/bin/activate          # macOS / Linux
# .\.venv\Scripts\Activate.ps1     # Windows PowerShell
dbt --version
```

If `dbt-snowflake` is not listed, reinstall:

```bash
pip install dbt-core dbt-snowflake
```

### LLM errors (401 / timeouts)

- Confirm `LLM_API_KEY` is set and valid.
- If requests timeout on large contexts, increase `llm.http_timeout_secs` in the YAML.

---

## Quick reference

| What | Where |
|------|-------|
| Config file | `config.yaml` (working directory) |
| Local artifacts | `.react/local/dev/mssql_migration/` |
| dbt models | `models/staging/stg_*.sql` |
| Source definitions | `models/schema.yml` |
| Thread logs | `.react/local/dev/mssql_migration/logs/<thread-id>.log` |
| Snowflake raw schema | `ANALYTICS.RAW` |
| Snowflake silver schema | `ANALYTICS.MSSQL_MIGRATION_SILVER` |
| Snowflake gold schema | `ANALYTICS.MSSQL_MIGRATION_GOLD` |
