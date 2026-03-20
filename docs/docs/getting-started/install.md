# Install

## 1. Download binaries

Download `skippr-dbt` and `skippr` for your platform from the releases page.

| Binary | Purpose |
|---|---|
| `skippr-dbt` | Pipeline orchestrator -- the main CLI you interact with |
| `skippr` (v6.15.0+) | Extract-and-load engine (invoked automatically by `skippr-dbt`) |

Place both on your `PATH` and verify:

```bash
skippr-dbt --version
skippr --version
```

## 2. Install Python and dbt

Python 3.10+ is required to run `dbt`, which `skippr-dbt` uses for model compilation and materialisation.

```bash
python3 -m venv .venv
source .venv/bin/activate        # macOS / Linux
# .\.venv\Scripts\Activate.ps1   # Windows PowerShell

pip install --upgrade pip
pip install dbt-core dbt-snowflake   # or dbt-bigquery, etc.
```

Verify:

```bash
dbt --version
```

The virtual environment must be activated whenever you run `skippr-dbt`.

## 3. Set up your LLM API key

Skippr dbt uses an LLM to assist with schema mapping and model generation. Set your API key:

```bash
export LLM_API_KEY="sk-..."
```

## 4. Warehouse credentials

### Snowflake

Set these environment variables:

```bash
export SNOWFLAKE_ACCOUNT="MYORG-MYACCOUNT"
export SNOWFLAKE_USER="myuser"
export SNOWFLAKE_PRIVATE_KEY_PATH="/path/to/snowflake_key.p8"
```

Key-pair authentication is recommended (and required when MFA is enabled). To generate a key pair:

```bash
openssl genrsa 2048 | openssl pkcs8 -topk8 -inform PEM -out snowflake_key.p8 -nocrypt
openssl rsa -in snowflake_key.p8 -pubout -out snowflake_key.pub
```

Then assign the public key in Snowflake:

```sql
ALTER USER myuser SET RSA_PUBLIC_KEY='MIIBIjANBgkqh...';
```

### BigQuery

```bash
export GOOGLE_APPLICATION_CREDENTIALS="/path/to/service-account.json"
```

## Next steps

Head to the [Quick Start](quickstart.md) to initialise your first project.
