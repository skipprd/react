# Maker Comment (First Comment)

Post this as the maker's first comment immediately after launch.

---

Hey Product Hunt!

We built Skippr because we spent years doing the same thing at the start of every data project: discover the source schemas, write extraction logic, load into a warehouse, map columns to clean names and types, generate staging models, validate them, fix the failures, repeat.

It's the data engineering equivalent of writing boilerplate — necessary, tedious, and identical every time. We thought: what if an AI agent could just do that part?

Skippr is a CLI that connects to your sources, extracts and loads your data into a warehouse, and then uses AI to autonomously generate clean, tested dbt models — bronze, silver, gold — in minutes.

Think of it like Codex, but for data. Codex reads your codebase and writes code. Skippr reads your data sources and writes dbt models. Both produce artifacts you review and own.

A few things we're proud of:

- **Your data never leaves your machine.** All data transfer is local — source to warehouse, directly. No third-party service touches a row.
- **Only schema metadata is used for AI mapping.** Table names and column types, never your actual data.
- **Autonomous repair.** When dbt validation fails, the agent reads the error, adjusts the model, and retries — automatically.
- **It's incremental.** Re-runs only sync what changed. Your existing models are preserved.

We're launching with support for MSSQL and S3 as sources, and Snowflake, BigQuery, and Postgres as warehouses. More connectors are on the way.

We'd genuinely love your feedback — what sources would you want next? What would make this useful for your stack? Let us know in the comments.

Thanks for checking us out!
