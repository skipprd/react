# Skippr dbt

Skippr dbt is a data pipeline CLI that extracts data from sources (MSSQL, S3), loads it into a cloud warehouse (Snowflake, BigQuery), and automatically generates and validates dbt models on top.

## What it does

1. **Extract** -- reads tables from your source system.
2. **Load** -- writes raw data into a bronze schema in your warehouse.
3. **Model** -- generates, compiles, and materialises silver and gold dbt models using AI-assisted schema mapping.

All data transfer happens locally on your machine. No data leaves your network.

## Quick start

```bash
skippr-dbt init my-project
skippr-dbt connect warehouse snowflake
skippr-dbt connect source mssql
skippr-dbt doctor
skippr-dbt run
```

See the [Install](getting-started/install.md) and [Quick Start](getting-started/quickstart.md) guides for a detailed walkthrough.

## Requirements

| Dependency | Why |
|---|---|
| `skippr` binary (v6.15.0+) | Performs extract-and-load between source and warehouse |
| Python 3.10+ | Required by dbt |
| dbt-core + warehouse adapter | Model compilation and materialisation |
| An LLM API key | Powers AI-assisted schema mapping |
