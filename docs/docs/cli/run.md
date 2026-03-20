# skippr-dbt run

Execute the full pipeline: extract, load, and model.

## Usage

```bash
skippr-dbt run [--log <level>]
```

## Flags

| Flag | Description |
|---|---|
| `--log info` | Print structured logs to stdout instead of the live terminal UI |
| `--log debug` | Debug-level logs |
| `--log trace` | Most verbose logging |

When `--log` is omitted, a live terminal UI is displayed showing phases, tasks, and progress in real time.

## What happens

1. **Discover** -- reads source schemas (e.g. MSSQL tables, S3 files).
2. **Sync** -- extracts rows and loads them into the warehouse bronze schema.
3. **Verify** -- confirms destination tables exist and are queryable.
4. **Plan** -- designs a silver (staging) layer using AI-assisted schema mapping.
5. **Author** -- writes dbt SQL models with source references, type casting, and renaming.
6. **Validate** -- runs `dbt compile` and `dbt run` against the warehouse.
7. **Review** -- checks generated models for quality.

## Example

```bash
source .venv/bin/activate
skippr-dbt run
```

With structured logging:

```bash
skippr-dbt run --log info
```

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Pipeline completed successfully |
| 1 | Pipeline failed (check logs) |
| 130 | Interrupted (Ctrl+C) |
