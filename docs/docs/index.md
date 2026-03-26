# Skippr dbt

Skippr dbt is a data pipeline CLI that extracts data from sources (MSSQL, S3), loads it into a cloud warehouse (Snowflake, BigQuery, Postgres), and automatically generates and validates dbt models on top.

## What it does

1. **Extract** -- reads tables or files from your source system.
2. **Load** -- writes raw data into a bronze schema in your warehouse.
3. **Model** -- generates, compiles, and materialises silver and gold dbt models using AI-assisted schema mapping.

All data transfer happens locally on your machine. No data leaves your network.

## Supported connectors

| Direction | Connectors |
|---|---|
| **Sources** | MSSQL, S3 |
| **Warehouses** | Snowflake, BigQuery, Postgres |

## Quick start

```bash
skippr init my-project
skippr connect warehouse snowflake
skippr connect source mssql
skippr doctor
skippr run
```

See the [Install](getting-started/install.md) and [Quick Start](getting-started/quickstart.md) guides for a detailed walkthrough.

## Requirements

| Dependency | Why |
|---|---|
| Python 3.10+ | Required by dbt |
| dbt-core + warehouse adapter | Model compilation and materialisation |
