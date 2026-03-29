# Anticipated Product Hunt Q&A

Pre-written responses for likely comments. Adapt tone to feel natural — don't copy-paste robotically.

---

## "How is this different from Fivetran / Airbyte / Stitch?"

> Great question. Tools like Fivetran and Airbyte handle extraction and loading — they get data into a warehouse. But you still need to build the transformation layer yourself: write dbt models, define naming conventions, handle type casting, build marts.
>
> Skippr does all of that in one step. It extracts, loads, *and* autonomously generates clean dbt models — bronze, silver, and gold — using AI-assisted schema mapping. Then it validates them and auto-repairs failures. It runs entirely on your machine, so there's no hosted service, no per-row pricing, and no data leaving your network.
>
> They move data. Skippr also cleanses, models, and validates it.

---

## "How is this like Codex?"

> Codex reads your codebase, writes code, tests it, and iterates on failures. Skippr reads your data sources, writes extraction logic, generates dbt models, validates them, and iterates on failures.
>
> Both are autonomous agents that produce reviewable artifacts — pull requests in one case, dbt projects in the other. The output is standard, nothing proprietary. You own it.

---

## "Is this an AI agent or a tool?"

> It's an agent. Skippr doesn't just move data from A to B — it makes decisions. It reads source schemas, maps columns to clean names and types, generates staged and business-ready dbt models, validates them against your warehouse, and when validation fails, it reads the error, adjusts the model, and retries. Autonomously.
>
> You review the output. The agent handles the repetitive engineering.

---

## "Is my data really private? How can I trust that?"

> All data transfer is local. Skippr reads from your source on your machine and writes directly to your warehouse over the network (Snowflake REST API, BigQuery API, Postgres wire protocol). No data passes through any intermediary service — ours or anyone else's.
>
> AI mapping uses only schema metadata: table names, column names, and data types. It never sees a single row of your actual data.

---

## "Does this replace my data team?"

> Not exactly — it replaces the tedious setup work that occupies the first weeks or months of any data project. Discovery, extraction, loading, initial model generation — that's what the agent automates.
>
> Once you have clean silver and gold models, your data engineers (or analysts, or AI systems) focus on the high-value work: domain-specific metrics, business logic, data quality rules, and stakeholder requirements — the work that actually requires human judgment.
>
> For smaller teams without dedicated data engineers, Skippr can get you surprisingly far on its own.

---

## "What sources and warehouses are supported?"

> **Sources:** MSSQL and S3 today. We're actively building connectors for Postgres (as source), MySQL, and API-based sources.
>
> **Warehouses:** Snowflake, BigQuery, and Postgres.
>
> If there's a specific connector you need, let us know — we're prioritising based on demand.

---

## "Is this open source?"

> [If yes:] The core CLI is open source. You can inspect exactly what it does, run it in air-gapped environments, and contribute connectors.
>
> [If no:] Skippr is a commercial product with a free tier. We're considering open-sourcing components — if that matters to you, let us know.

*Choose the appropriate response based on your licensing model at launch.*

---

## "How does pricing work?"

> Skippr is free to start — 100 credits included. After that, credits are pay-as-you-go at $0.10 each. Every action (table sync, field discovery, pipeline run, AI usage) costs a known number of credits. No surprises.
>
> The Pro plan ($349/mo) adds unlimited projects, more cloud storage, CI/CD auth, and email support. Credits are still PAYG.

---

## "How does it handle schema changes or new tables?"

> Skippr runs are incremental. When you re-run, it:
>
> - Detects new tables and adds them
> - Syncs only new or changed rows for existing tables
> - Preserves your existing dbt models and extends them as needed
>
> If a source schema changes (new column, renamed table), the agent re-evaluates the mapping and proposes updates. You review and approve.

---

## "Can I customise the generated dbt models?"

> Absolutely. Skippr generates standard dbt models — .sql and .yml files in a dbt project directory. You can edit them, extend them, add custom tests, or layer your own models on top. Once generated, they're yours.
>
> Re-runs are smart about this: Skippr won't overwrite your manual changes. It adds new models alongside existing ones.

---

## "What about real-time / streaming data?"

> Today Skippr is batch-oriented — it runs on demand or on a schedule. We're exploring incremental streaming patterns, but the initial focus is on getting the autonomous ELT + modeling pipeline right.
>
> If streaming is critical for your use case, we'd love to hear about the specific scenario — it helps us prioritise.

---

## "Why a CLI and not a web UI?"

> A few reasons:
>
> 1. **Privacy** — a CLI running on your machine means data stays local by default. No web service to trust.
> 2. **Composability** — it fits into existing workflows, CI/CD, cron jobs, and automation.
> 3. **Speed** — no browser overhead, no UI latency. Just run it.
>
> That said, we're building a web interface for monitoring and review. The CLI-first approach ensures the core pipeline doesn't depend on it.
