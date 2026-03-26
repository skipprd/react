# Environment Variables

All secrets and authentication credentials are configured via environment variables. They are never stored in the config file.

## Snowflake

| Variable | Description |
|---|---|
| `SNOWFLAKE_ACCOUNT` | Account identifier (e.g. `MYORG-MYACCOUNT` or `xy12345.us-east-1`) |
| `SNOWFLAKE_USER` | Login username |
| `SNOWFLAKE_PRIVATE_KEY_PATH` | Path to `.p8` private key file (key-pair auth, recommended) |
| `SNOWFLAKE_PASSWORD` | Password (only when MFA is not enforced) |

Key-pair auth is recommended and required when MFA is enabled on the Snowflake account.

## BigQuery

| Variable | Description |
|---|---|
| `GOOGLE_APPLICATION_CREDENTIALS` | Path to a GCP service account JSON key file |

## Postgres

| Variable | Default | Description |
|---|---|---|
| `POSTGRES_HOST` | `localhost` | PostgreSQL host |
| `POSTGRES_PORT` | `5432` | PostgreSQL port |
| `POSTGRES_USER` | | Database user |
| `POSTGRES_PASSWORD` | | Database password |
| `POSTGRES_DATABASE` | | Database name (overrides config file) |
| `POSTGRES_SCHEMA` | `public` | Target schema (overrides config file) |
| `POSTGRES_SSLMODE` | | SSL mode (e.g. `disable`, `require`, `prefer`) |

## Source credentials

| Variable | Description |
|---|---|
| `MSSQL_CONNECTION_STRING` | ADO.NET connection string for SQL Server |
| `AWS_ACCESS_KEY_ID` | AWS access key (for S3 sources) |
| `AWS_SECRET_ACCESS_KEY` | AWS secret key (for S3 sources) |

## Optional overrides

| Variable | Default | Description |
|---|---|---|
| `LLM_API_KEY` | | Override the server-provided LLM key with your own |
| `LLM_BASE_URL` | `https://api.openai.com` | Override the LLM API endpoint |
| `LLM_MAX_TOKENS` | `8192` | Max output tokens per LLM call |
| `DBT_TARGET_SCHEMA` | Same as project | Override the dbt base schema name |
| `DBT_SILVER_SUFFIX` | `silver` | Override the silver tier suffix |
| `DBT_GOLD_SUFFIX` | `gold` | Override the gold tier suffix |
