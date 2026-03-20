# skippr-dbt doctor

Check that all prerequisites are in place before running the pipeline.

## Usage

```bash
skippr-dbt doctor
```

## Checks performed

| Check | What it verifies |
|---|---|
| Config file | `skippr-dbt.yaml` exists in the current directory |
| Warehouse | A warehouse is configured via `connect warehouse` |
| Source | A source is configured via `connect source` |
| `skippr` binary | The `skippr` CLI is on PATH |
| `dbt` binary | The `dbt` CLI is on PATH (Python venv must be active) |
| Python | `python3` or `python` is on PATH |
| `LLM_API_KEY` | Environment variable is set |
| Snowflake auth | `SNOWFLAKE_ACCOUNT`, `SNOWFLAKE_USER`, and either `SNOWFLAKE_PRIVATE_KEY_PATH` or `SNOWFLAKE_PASSWORD` are set (Snowflake only) |

## Example output

```
  [ok]   skippr-dbt.yaml found
  [ok]   warehouse configured (snowflake)
  [ok]   source configured (mssql)
  [ok]   skippr binary found on PATH
  [ok]   dbt binary found on PATH
  [ok]   python found on PATH
  [ok]   LLM_API_KEY is set
  [ok]   SNOWFLAKE_ACCOUNT is set
  [ok]   SNOWFLAKE_USER is set
  [ok]   SNOWFLAKE_PRIVATE_KEY_PATH is set (key-pair auth)

All checks passed. Run 'skippr-dbt run' to start.
```

## Exit codes

| Code | Meaning |
|---|---|
| 0 | All checks passed |
| 1 | One or more checks failed |
