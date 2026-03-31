# Install

## 1. Download skippr

Download `skippr` for your platform from the releases page and place it on your `PATH`:

```bash
skippr --version
```

## 2. Install Python and dbt

Python 3.10+ is required to run `dbt`, which `skippr` uses for model compilation and materialisation.

```bash
python3 -m venv .venv
source .venv/bin/activate        # macOS / Linux
# .\.venv\Scripts\Activate.ps1   # Windows PowerShell

pip install --upgrade pip
pip install dbt-core dbt-snowflake   # or: dbt-bigquery, dbt-postgres, dbt-databricks, dbt-synapse, dbt-redshift, dbt-clickhouse, dbt-duckdb
```

Verify:

```bash
dbt --version
```

The virtual environment must be activated whenever you run `skippr`.

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

### Databricks

```bash
export DATABRICKS_HOST="https://myworkspace.cloud.databricks.com"
export DATABRICKS_TOKEN="dapi..."
export DATABRICKS_WAREHOUSE_ID="abc123def456"
```

### Redshift

Authentication uses the AWS default credential chain. Set a cluster or serverless workgroup:

```bash
export REDSHIFT_CLUSTER_IDENTIFIER="my-cluster"   # provisioned
# or: export REDSHIFT_WORKGROUP_NAME="my-wg"      # serverless
export REDSHIFT_DB_USER="admin"
export REDSHIFT_DATABASE="analytics"
```

### ClickHouse

```bash
export CLICKHOUSE_URL="http://localhost:8123"
export CLICKHOUSE_USER="default"
export CLICKHOUSE_PASSWORD="mypassword"
```

### DuckDB / MotherDuck

For MotherDuck, set the token and use an `md:` connection string in `skippr.yaml`:

```bash
export MOTHERDUCK_TOKEN="eyJ..."
```

### Azure Synapse

```bash
export SYNAPSE_HOST="myworkspace.sql.azuresynapse.net"
export SYNAPSE_USER="sqladmin"
export SYNAPSE_PASSWORD="mypassword"
export SYNAPSE_DATABASE="analytics"
```

## Next steps

Head to the [Quick Start](quickstart.md) to initialise your first project.
