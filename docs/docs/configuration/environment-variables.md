# Environment Variables

This page lists the first-class environment variables that `skippr` passes through to `skippr-el`.

Many connectors are configured directly in `skippr.yaml` instead of through dedicated env vars. For security best practices, we strongly advise against storing the secure value in `skippr.yaml`. Use environment variable interpolation instead: replace the field value with your own `${ENV_VAR}` reference.

Example:

```yaml
source:
  kind: mssql
  connection_string: ${MSSQL_CONNECTION_STRING}
```

Set the env var before running `skippr`:

macOS / Linux

```bash
export MSSQL_CONNECTION_STRING="server=tcp:127.0.0.1,1433;database=testdb;user id=sa;password=YourPass;TrustServerCertificate=true"
```

Windows PowerShell

```powershell
$env:MSSQL_CONNECTION_STRING = "server=tcp:127.0.0.1,1433;database=testdb;user id=sa;password=YourPass;TrustServerCertificate=true"
```

Windows Command Prompt

```cmd
set MSSQL_CONNECTION_STRING=server=tcp:127.0.0.1,1433;database=testdb;user id=sa;password=YourPass;TrustServerCertificate=true
```

## Snowflake

| Variable | Description |
|---|---|
| `SNOWFLAKE_ACCOUNT` | Account identifier (e.g. `MYORG-MYACCOUNT` or `xy12345.us-east-1`) |
| `SNOWFLAKE_USER` | Login username |
| `SNOWFLAKE_PRIVATE_KEY_PATH` | Path to `.p8` private key file (key-pair auth, recommended) |
| `SNOWFLAKE_PASSWORD` | Password (only when MFA is not enforced) |
| `SNOWFLAKE_WAREHOUSE` | Compute warehouse name |
| `SNOWFLAKE_DATABASE` | Target database |
| `SNOWFLAKE_SCHEMA` | Target schema |
| `SNOWFLAKE_ROLE` | Optional role to assume |
| `SNOWFLAKE_STAGE` | Optional stage for file uploads |
| `SNOWFLAKE_STAGING_URI` | Optional external staging URI (`s3://`, `azure://`, or `gcs://`) |
| `SNOWFLAKE_STAGING_STORAGE_INTEGRATION` | Optional Snowflake storage integration name for external staging |

Key-pair auth is recommended and required when MFA is enabled on the Snowflake account.

For external Snowflake staging, the underlying cloud upload still uses provider auth:

- S3: standard AWS credential chain
- Azure: `AZURE_STORAGE_SAS_TOKEN` or `AZURE_STORAGE_ACCOUNT_KEY`
- GCS: `GOOGLE_APPLICATION_CREDENTIALS` or Application Default Credentials

## BigQuery

| Variable | Description |
|---|---|
| `BIGQUERY_PROJECT` | GCP project ID |
| `BIGQUERY_DATASET` | BigQuery dataset name |
| `BIGQUERY_LOCATION` | Dataset location (for example `US` or `EU`) |
| `GOOGLE_APPLICATION_CREDENTIALS` | Path to a GCP service account JSON key file |

## Postgres Warehouse

These env vars apply to the Postgres warehouse destination. The Postgres source connector is configured via source fields like `host`, `port`, `user`, `password`, and `connection_string`.

| Variable | Default | Description |
|---|---|---|
| `POSTGRES_HOST` | `localhost` | PostgreSQL host |
| `POSTGRES_PORT` | `5432` | PostgreSQL port |
| `POSTGRES_USER` | | Database user |
| `POSTGRES_PASSWORD` | | Database password |
| `POSTGRES_DATABASE` | | Database name (overrides config file) |
| `POSTGRES_SCHEMA` | `public` | Target schema (overrides config file) |
| `POSTGRES_SSLMODE` | | SSL mode (e.g. `disable`, `require`, `prefer`) |

## Source Connectors

| Variable | Description |
|---|---|
| `MSSQL_CONNECTION_STRING` | ADO.NET connection string for SQL Server |
| `MYSQL_CONNECTION_STRING` | MySQL connection string |
| `MOTHERDUCK_TOKEN` | MotherDuck auth token for the MotherDuck source connector |
| `AWS_ACCESS_KEY_ID` | AWS access key for AWS-backed sources such as S3 |
| `AWS_SECRET_ACCESS_KEY` | AWS secret key for AWS-backed sources such as S3 |
| `AWS_DEFAULT_REGION` | AWS region for AWS-backed sources such as S3, SQS, Kinesis, and DynamoDB |

## Optional Overrides

| Variable | Default | Description |
|---|---|---|
| `LLM_API_KEY` | | Override the server-provided LLM key with your own |
| `LLM_BASE_URL` | `https://api.openai.com` | Override the LLM API endpoint |
| `LLM_MAX_TOKENS` | `8192` | Max output tokens per LLM call |
| `DBT_TARGET_SCHEMA` | Same as project | Override the dbt base schema name |
| `DBT_SILVER_SUFFIX` | `silver` | Override the silver tier suffix |
| `DBT_GOLD_SUFFIX` | `gold` | Override the gold tier suffix |
