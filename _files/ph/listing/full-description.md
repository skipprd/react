# Product Hunt Full Description

Paste into the PH "Description" field.

---

## What is Skippr?

Skippr is an AI agent that takes you from raw source data to production dbt models — autonomously. Like Codex, but for data. Point it at your databases or files, tell it where your warehouse lives, and run one command. Skippr handles extraction, loading, schema mapping, model generation, validation, and repair.

## What does it do?

**Discover** — Skippr connects to your sources (MSSQL, S3) and reads every table, schema, and file structure automatically.

**Extract & Load** — It extracts your data and loads it directly into your warehouse (Snowflake, BigQuery, Postgres) as raw bronze tables.

**Cleanse & Model** — The agent maps source schemas to clean column names and types, generates staged silver models and business-ready gold models — all built on dbt — then validates them against your warehouse. If validation fails, it reads the error, adjusts, and retries.

**Iterate** — Re-runs are incremental. Only new or changed data is synced. Existing models are preserved and extended.

## How is this different?

Codex reads your codebase, writes code, tests it, iterates. Skippr reads your data sources, writes extraction logic, generates dbt models, validates them, iterates on failures. Both produce reviewable artifacts you own.

Traditional tools handle extraction and loading but leave you to write the transformation layer. Skippr does the whole pipeline:

- **One agent, full pipeline** — extraction, loading, cleansing, modeling, validation — in a single CLI
- **Standard dbt output** — nothing proprietary. Review it, extend it, plug it into your existing CI/CD
- **Privacy by default** — runs on your machine. Row-level data never leaves. Only schema metadata is used for AI mapping
- **Autonomous repair** — when validation fails, the agent reads the error, adjusts, and re-validates

## Tech under the hood

- Built on **dbt** — the industry-standard transformation framework
- Bronze / Silver / Gold data architecture out of the box
- Supports **Snowflake**, **BigQuery**, **Postgres** warehouses
- Sources: **MSSQL**, **S3** (more coming)

## Get started

```
skippr init my-project
skippr connect warehouse snowflake
skippr connect source mssql
skippr run
```

Four commands. Minutes to your first query.
