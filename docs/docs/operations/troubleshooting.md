# Troubleshooting

## Common issues

### `skippr-dbt.yaml not found`

Run `skippr-dbt init <project>` in the working directory first.

### `dbt: command not found`

Activate the Python virtual environment before running `skippr-dbt`:

```bash
source .venv/bin/activate
dbt --version
```

If `dbt` is not installed:

```bash
pip install dbt-core dbt-snowflake
```

### `skippr: command not found`

The `skippr` binary is not on PATH. Download it from the releases page and place it on your PATH.

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

### Sync stalls with no output

The extract-and-load step produced no output within the idle timeout. Common causes:

| Cause | Fix |
|---|---|
| Debug build of `skippr` | Rebuild `skippr` in release mode |
| Warehouse connection hanging | Check credentials and network access |
| AWS credentials missing | Set `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` for S3 sources |

### `dbt deps` needed

After the first run, `skippr-dbt` generates a `packages.yml` in the dbt project. If dbt reports missing packages, run:

```bash
cd .skippr-dbt/local/dev/<project>/dbt   # or the project root if models are there
dbt deps
```
