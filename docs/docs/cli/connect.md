# skippr connect

Configure a warehouse or data source connection.

## Usage

```bash
skippr connect warehouse <kind> [flags]
skippr connect source <kind> [flags]
```

When flags are omitted, the command prompts interactively.

---

## connect warehouse

### Snowflake

```bash
skippr connect warehouse snowflake \
  --database ANALYTICS \
  --schema RAW \
  --warehouse COMPUTE_WH \
  --role ACCOUNTADMIN
```

| Flag | Description |
|---|---|
| `--database` | Snowflake database name |
| `--schema` | Bronze/raw schema where extracted data lands |
| `--warehouse` | Compute warehouse |
| `--role` | Role with appropriate grants |

Authentication is always via environment variables (`SNOWFLAKE_ACCOUNT`, `SNOWFLAKE_USER`, `SNOWFLAKE_PRIVATE_KEY_PATH` or `SNOWFLAKE_PASSWORD`). These are never stored in the config file.

### BigQuery

```bash
skippr connect warehouse bigquery \
  --project my-gcp-project \
  --dataset raw_data \
  --location US
```

| Flag | Description |
|---|---|
| `--project` | GCP project ID |
| `--dataset` | BigQuery dataset |
| `--location` | Dataset location (e.g. `US`, `EU`) |

### Postgres

```bash
skippr connect warehouse postgres \
  --database analytics \
  --schema public
```

| Flag | Description |
|---|---|
| `--database` | PostgreSQL database name |
| `--schema` | Target schema (default: `public`) |

Authentication is via environment variables (`POSTGRES_HOST`, `POSTGRES_USER`, `POSTGRES_PASSWORD`). See [Environment Variables](../configuration/environment-variables.md).

---

## connect source

### MSSQL

```bash
skippr connect source mssql \
  --connection-string '${MSSQL_CONNECTION_STRING}'
```

| Flag | Description |
|---|---|
| `--connection-string` | ADO.NET connection string. Use `${ENV_VAR}` to reference an environment variable. |

### S3

```bash
skippr connect source s3 \
  --bucket my-data-bucket \
  --prefix raw/
```

| Flag | Description |
|---|---|
| `--bucket` | S3 bucket name |
| `--prefix` | Key prefix to scan |

---

## Notes

- `connect` commands update `skippr.yaml` in the current directory. Run `skippr init` first.
- Sensitive values (passwords, keys, connection strings) should use environment variable references in the config and be set in your shell or `.env` file.
