# Troubleshooting

## Common issues

### `skippr.yaml not found`

Run `skippr init <project>` in the working directory first.

### Need to fully reset a project

If you want to wipe the current project's local runtime data, config, and remote state on S3, run:

```bash
skippr init <project> --reset
```

This will prompt you to type `yes`, then delete:

- local `.skippr/`
- local `skippr.yaml`
- remote project metadata and state in the authenticated project's S3 scope

It then recreates a fresh local environment so you can reconnect and rerun from scratch.

### `dbt: command not found`

Activate the Python virtual environment before running `skippr`:

```bash
source .venv/bin/activate
dbt --version
```

If `dbt` is not installed:

```bash
pip install dbt-core dbt-snowflake
```

### MFA error: `390197 -- Multi-factor authentication is required`

Your Snowflake account enforces MFA, so password auth cannot work for headless tools. Switch to key-pair authentication:

1. Generate an RSA key pair (see [Install](../getting-started/install.md#snowflake))
2. Set `SNOWFLAKE_PRIVATE_KEY_PATH` instead of `SNOWFLAKE_PASSWORD`

### Snowflake connection errors

| Symptom | Fix |
|---|---|
| `Failed to connect: 250001` | Check the `SNOWFLAKE_ACCOUNT` format -- use the org-account form (e.g. `MYORG-MYACCOUNT`) or include the region (e.g. `xy12345.us-east-1`) |
| `Incorrect username or password` | Verify `SNOWFLAKE_USER` and auth env vars |
| `Insufficient privileges` | Ensure the role has USAGE on the warehouse, database, and raw schema, plus CREATE SCHEMA for silver/gold |

### LLM errors (401 / timeouts)

- Confirm `LLM_API_KEY` is set and valid.
- If requests timeout, set `LLM_HTTP_TIMEOUT_SECS=120` in the environment.
- If output is truncated, set `LLM_MAX_TOKENS=8192`.

### Postgres connection errors

| Symptom | Fix |
|---|---|
| `connection refused` | Check `POSTGRES_HOST` and `POSTGRES_PORT` are correct and the server is running |
| `password authentication failed` | Verify `POSTGRES_USER` and `POSTGRES_PASSWORD` |
| `database "..." does not exist` | Create the database first, or check the `database` field in config |
| SSL errors | Set `POSTGRES_SSLMODE=disable` for local development |

### Sync stalls with no output

The extract-and-load step produced no output within the idle timeout. Common causes:

| Cause | Fix |
|---|---|
| Warehouse connection hanging | Check credentials and network access |
| AWS credentials missing | Set `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` for S3 sources |

### `dbt deps` needed

After the first run, `skippr` generates a `packages.yml` in the dbt project. If dbt reports missing packages, run:

```bash
cd .skippr/local/dev/<project>/dbt   # or the project root if models are there
dbt deps
```
