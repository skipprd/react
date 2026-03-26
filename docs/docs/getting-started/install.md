# Install

## 1. Download skippr-dbt

Download `skippr-dbt` for your platform from the releases page and place it on your `PATH`:

```bash
skippr-dbt --version
```

The `skippr` extract-and-load engine is downloaded automatically on first run. To manage `skippr` yourself instead, place it on your `PATH` and `skippr-dbt` will use it.

## 2. Install Python and dbt

Python 3.10+ is required to run `dbt`, which `skippr-dbt` uses for model compilation and materialisation.

```bash
python3 -m venv .venv
source .venv/bin/activate        # macOS / Linux
# .\.venv\Scripts\Activate.ps1   # Windows PowerShell

pip install --upgrade pip
pip install dbt-core dbt-snowflake   # or: dbt-bigquery, dbt-postgres
```

Verify:

```bash
dbt --version
```

The virtual environment must be activated whenever you run `skippr-dbt`.

## 3. LLM API key (optional)

Skippr dbt includes a server-provided LLM key when you authenticate. To use your own key instead:

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

### Postgres

```bash
export POSTGRES_HOST="localhost"
export POSTGRES_USER="myuser"
export POSTGRES_PASSWORD="mypassword"
```

## Next steps

Head to the [Quick Start](quickstart.md) to initialise your first project.
