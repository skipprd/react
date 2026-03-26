# skippr doctor

Check that all prerequisites are in place before running the pipeline.

## Usage

```bash
skippr doctor
```

## Checks performed

| Check | What it verifies |
|---|---|
| Config file | `skippr.yaml` exists in the current directory |
| Warehouse | A warehouse is configured via `connect warehouse` |
| Source | A source is configured via `connect source` |
| `dbt` binary | The `dbt` CLI is on PATH (Python venv must be active) |
| Python | `python3` or `python` is on PATH |
| `LLM_API_KEY` | Environment variable is set |
| Snowflake auth | `SNOWFLAKE_ACCOUNT`, `SNOWFLAKE_USER`, and either `SNOWFLAKE_PRIVATE_KEY_PATH` or `SNOWFLAKE_PASSWORD` are set (Snowflake only) |

## Example output

```
  [ok]   skippr.yaml found
  [ok]   warehouse configured (snowflake)
  [ok]   source configured (mssql)
  [ok]   dbt binary found on PATH
  [ok]   python found on PATH
  [ok]   LLM_API_KEY is set
  [ok]   SNOWFLAKE_ACCOUNT is set
  [ok]   SNOWFLAKE_USER is set
  [ok]   SNOWFLAKE_PRIVATE_KEY_PATH is set (key-pair auth)

All checks passed. Run 'skippr run' to start.
```

## Exit codes

| Code | Meaning |
|---|---|
| 0 | All checks passed |
| 1 | One or more checks failed |
