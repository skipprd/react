# Troubleshooting

## Build errors

### `Could not find protoc`

**Cause:** The `protoc` (Protocol Buffers compiler) is not installed or not on PATH. Required by the LanceDB dependency at build time.

**Fix:** Install protobuf for your platform and verify:

```bash
protoc --version
```

See [Installation](../getting-started/install.md) for platform-specific instructions.

## LLM errors

### `parser_error invalid JSON twice`

**Cause:** The LLM's output is being truncated because `max_tokens` is too low. The agent receives incomplete JSON and fails to parse it.

**Fix:** Increase `llm.max_tokens` in your YAML config or set the env var:

```bash
export LLM_MAX_TOKENS=8192
```

For `gpt-5.x`, 8192 is a reasonable starting point for data engineering workflows that generate large tool-call payloads.

### LLM request failed: 401

**Cause:** Invalid or missing API key.

**Fix:** Ensure `LLM_API_KEY` is set:

```bash
export LLM_API_KEY="sk-..."
```

### LLM request failed: 429

**Cause:** Rate limited by the LLM provider.

**Fix:** Reduce concurrency or wait. The `error` frame includes `retry_after_ms` when available.

### LLM request timeout

**Cause:** The LLM API call exceeded `llm.http_timeout_secs`.

**Fix:** Increase the timeout:

```yaml
llm:
  http_timeout_secs: 120
```

## Warehouse errors

### BigQuery: `DefaultCredentialsError`

**Cause:** Google Cloud credentials are not configured.

**Fix:**

- Application Default Credentials: `gcloud auth application-default login`
- Service account: `export GOOGLE_APPLICATION_CREDENTIALS="/path/to/key.json"`

### BigQuery: location mismatch

**Cause:** `providers.warehouse.location` does not match the dataset's actual location in BigQuery.

**Fix:** Set the location to match your dataset (e.g. `US`, `EU`, `us-central1`).

### Athena: missing permissions

**Cause:** The IAM identity lacks required Athena/Glue/S3 permissions.

**Fix:** See [Athena connector](../connectors/warehouses/athena.md) for the required permission list.

### Postgres: connection refused

**Cause:** PostgreSQL is not running or the connection parameters are wrong.

**Fix:** Verify `PGHOST`, `PGPORT`, `PGUSER`, `PGPASSWORD`, `PGDATABASE` environment variables.

## dbt errors

### `dbt: command not found`

**Cause:** dbt is not installed or the virtual environment is not activated.

**Fix:**

```bash
source .venv/bin/activate
pip install dbt-core dbt-bigquery  # or your adapter
```

### dbt validation failures

**Cause:** Authored models have SQL or schema errors.

**Fix:** The Data Engineer suite's repair loop automatically attempts remediation. If repair fails:

1. Check the dbt error output in the tool_end payload
2. Inspect the generated models in `{scope}/dbt/models/`
3. The agent may ask for user input (`await_user`) if automated repair is exhausted

## Storage errors

### `missing storage bucket for s3 mode`

**Cause:** S3 storage mode is selected but no bucket is configured.

**Fix:** Set the bucket via CLI flag, env var, or YAML:

```bash
export SKIPPR_S3_BUCKET=my-bucket
```

### S3: access denied

**Cause:** The IAM identity lacks required S3 permissions.

**Fix:** See [S3 storage connector](../connectors/storage/s3.md) for the required permission list and IAM policy.

## Server errors

### `terminal mode not enabled: stdout is not a TTY`

**Cause:** `--terminal` was passed but stdout is not a real terminal (e.g. running in CI, piped, or certain Windows shells).

**Fix:** Run without `--terminal` and use `--log info` for plain output:

```bash
cargo run -p react -- serve --config my-config.yml --log info
```

### Agent reached step limit

**Cause:** The agent exhausted its step budget without producing a valid final.

**Fix:** This triggers the policy's `fallback` method, which typically asks the user to retry. If this happens frequently:

- Check that the task is feasible with the configured tools
- Increase `max_steps` in the suite configuration
- Verify that `max_tokens` is high enough for the LLM to produce complete tool-call JSON

## Recovery

ReAct threads are designed to be resumable. If the server crashes or the client disconnects:

1. The thread is persisted to storage after every step
2. Use `open` (WebSocket) or `--thread-id` (CLI) to resume from the last persisted state
3. The agent continues from where it left off

No manual intervention is needed for crash recovery.
