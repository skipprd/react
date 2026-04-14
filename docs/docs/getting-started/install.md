# Install

## 1. Download skippr

### macOS / Linux

```bash
curl -fsSL https://install.skippr.io | sh
```

### Windows (PowerShell — default terminal in VS Code)

```powershell
irm https://skippr.io/install.ps1 | iex
```

This installs `skippr.exe` to `%LOCALAPPDATA%\skippr\bin` and adds it to your user `PATH` so you can run `skippr` from any terminal.

From cmd.exe you can invoke the same installer:

```cmd
powershell -c "irm https://skippr.io/install.ps1 | iex"
```

Verify the install:

```bash
skippr --version
```

## 2. Install OpenSSL (Windows only)

OpenSSL is required for Snowflake key-pair authentication. On macOS and Linux it is typically pre-installed.

```powershell
winget install OpenSSL
```

## 3. Install Python and dbt

Python 3.10+ is required to run `dbt`, which `skippr` uses for model compilation and materialisation.

**macOS / Linux:**

```bash
python3 -m venv .venv
source .venv/bin/activate

pip install --upgrade pip
pip install dbt-core dbt-snowflake   # or: dbt-bigquery, dbt-postgres, dbt-databricks, dbt-synapse, dbt-redshift, dbt-clickhouse, dbt-duckdb (MotherDuck)
```

**Windows (PowerShell):**

```powershell
python -m venv .venv
.\.venv\Scripts\Activate.ps1

pip install --upgrade pip
pip install dbt-core dbt-snowflake
```

Verify:

```bash
dbt --version
```

The virtual environment must be activated whenever you run `skippr`.

## 4. LLM API key (optional)

Skippr dbt includes a server-provided LLM key when you authenticate. To use your own key instead:

```bash
export LLM_API_KEY="sk-..."
```

## 5. Warehouse credentials

### Snowflake

Set these environment variables:

```bash
export SNOWFLAKE_ACCOUNT="MYORG-MYACCOUNT"
export SNOWFLAKE_USER="myuser"                           # or a service account name
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

**Using a service account?** Create a dedicated `TYPE = SERVICE` user in Snowflake for least-privilege automated access. See the [Snowflake connector docs](https://docs.skippr.io/connectors/destinations/snowflake#service-account-authentication) for full setup instructions.

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

Configure Databricks with `skippr connect warehouse databricks` or in `skippr.yaml` using `workspace_url`, `token`, `warehouse_id`, `catalog`, and `schema`.

For security best practices, we strongly advise against storing the token in `skippr.yaml`. Use environment variable interpolation instead: replace the `token` value with your own `${ENV_VAR}` reference.

The relevant part of `skippr.yaml` looks like this:

```yaml
warehouse:
  kind: databricks
  token: ${DATABRICKS_TOKEN}
```

Set the env var before running `skippr`:

macOS / Linux

```bash
export DATABRICKS_TOKEN="dapi..."
```

Windows PowerShell

```powershell
$env:DATABRICKS_TOKEN = "dapi..."
```

Windows Command Prompt

```cmd
set DATABRICKS_TOKEN=dapi...
```

### Redshift

Authentication uses the AWS default credential chain.

Configure Redshift with `skippr connect warehouse redshift` or in `skippr.yaml` using fields like `cluster_identifier` or `workgroup_name`, `db_user`, `database`, and `region`.

### ClickHouse

Configure ClickHouse with `skippr connect warehouse clickhouse` or in `skippr.yaml` using `url`, `database`, `user`, and `password`.

For security best practices, we strongly advise against storing the password in `skippr.yaml`. Use environment variable interpolation instead: replace the `password` value with your own `${ENV_VAR}` reference.

The relevant part of `skippr.yaml` looks like this:

```yaml
warehouse:
  kind: clickhouse
  password: ${CLICKHOUSE_PASSWORD}
```

Set the env var before running `skippr`:

macOS / Linux

```bash
export CLICKHOUSE_PASSWORD="secret"
```

Windows PowerShell

```powershell
$env:CLICKHOUSE_PASSWORD = "secret"
```

Windows Command Prompt

```cmd
set CLICKHOUSE_PASSWORD=secret
```

### MotherDuck

Configure MotherDuck with `skippr connect warehouse motherduck` or in `skippr.yaml` using `motherduck_token`, `database`, and `schema`.

For security best practices, we strongly advise against storing the token in `skippr.yaml`. Use environment variable interpolation instead: replace the `motherduck_token` value with your own `${ENV_VAR}` reference.

The relevant part of `skippr.yaml` looks like this:

```yaml
warehouse:
  kind: motherduck
  motherduck_token: ${MOTHERDUCK_TOKEN}
```

Set the env var before running `skippr`:

macOS / Linux

```bash
export MOTHERDUCK_TOKEN="md:..."
```

Windows PowerShell

```powershell
$env:MOTHERDUCK_TOKEN = "md:..."
```

Windows Command Prompt

```cmd
set MOTHERDUCK_TOKEN=md:...
```

### Azure Synapse

Configure Synapse with `skippr connect warehouse synapse` or in `skippr.yaml` using `connection_string` and `schema`.

For security best practices, we strongly advise against storing the connection string in `skippr.yaml`. Use environment variable interpolation instead: replace the `connection_string` value with your own `${ENV_VAR}` reference.

The relevant part of `skippr.yaml` looks like this:

```yaml
warehouse:
  kind: synapse
  connection_string: ${SYNAPSE_CONNECTION_STRING}
```

Set the env var before running `skippr`:

macOS / Linux

```bash
export SYNAPSE_CONNECTION_STRING="Server=myserver.database.windows.net;User Id=admin;Password=secret;Database=mydb"
```

Windows PowerShell

```powershell
$env:SYNAPSE_CONNECTION_STRING = "Server=myserver.database.windows.net;User Id=admin;Password=secret;Database=mydb"
```

Windows Command Prompt

```cmd
set SYNAPSE_CONNECTION_STRING=Server=myserver.database.windows.net;User Id=admin;Password=secret;Database=mydb
```

## Next steps

Head to the [Quick Start](quickstart.md) to initialise your first project.
