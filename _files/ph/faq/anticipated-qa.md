# Anticipated Product Hunt Q&A

Pre-written responses for likely comments. Adapt tone to feel natural — don't copy-paste robotically.

---

## "How is this different from Fivetran / Airbyte / Stitch?"

> Great question. Tools like Fivetran and Airbyte handle the E and L — extraction and loading. They get data into a warehouse, but you still need to build the transformation layer yourself: write dbt models, define naming conventions, handle type casting, build marts.
>
> Skippr does all of that in one step. It extracts, loads, *and* automatically generates clean dbt models — bronze, silver, and gold — using AI-assisted schema mapping. And it runs entirely on your machine, so there's no hosted service, no per-row pricing, and no data leaving your network.

---

## "What LLMs do you support?"

> Skippr works with any OpenAI-compatible API. That includes GPT (via OpenAI), Claude (via compatible endpoints), Mistral, and self-hosted models running on Ollama or similar. You configure the model in your project YAML — swap it anytime.
>
> The LLM is used exclusively for schema mapping decisions (column naming, type inference, model structure). It receives table names and column metadata, never row-level data.

---

## "Is my data really private? How can I trust that?"

> All data transfer is local. Skippr reads from your source on your machine and writes directly to your warehouse over the network (Snowflake REST API, BigQuery API, Postgres wire protocol). No data passes through any intermediary service — ours or anyone else's.
>
> The LLM component only receives table-level metadata: table names, column names, and data types. It never sees a single row of your actual data. And since you choose the LLM provider, you control that trust boundary entirely.

---

## "Does this replace my data team?"

> Not exactly — it replaces the tedious setup work that occupies the first weeks or months of a data team's effort. Discovery, extraction, loading, initial model generation — that's what Skippr automates.
>
> Once you have clean silver and gold models, your data team (or analysts, or AI systems) can focus on the high-value work: building domain-specific metrics, dashboards, and insights on top of a solid foundation they didn't have to build from scratch.
>
> For smaller organisations without a data team, Skippr can get you surprisingly far on its own.

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
> [If no:] Skippr is a commercial product with a free tier / trial. We're considering open-sourcing components — if that matters to you, let us know. It helps us gauge interest.

*Choose the appropriate response based on your licensing model at launch.*

---

## "How does pricing work?"

> [Adjust based on actual pricing model at launch. Template:]
>
> Skippr is free to try — you can run a full pipeline end-to-end without paying. For production use, we offer [tiers / usage-based pricing / flat rate].
>
> Since Skippr runs on your machine and writes directly to your warehouse, there's no per-row or per-sync cost from us. You only pay for your warehouse compute as usual.

---

## "How does it handle schema changes or new tables?"

> Skippr runs are incremental. When you re-run, it:
>
> - Detects new tables and adds them
> - Syncs only new or changed rows for existing tables
> - Preserves your existing dbt models and extends them as needed
>
> If a source schema changes (new column, renamed table), the AI re-evaluates the mapping and proposes updates. You review and approve.

---

## "Can I customise the generated dbt models?"

> Absolutely. Skippr generates standard dbt models — .sql and .yml files in a dbt project directory. You can edit them, extend them, add custom tests, or layer your own models on top. Once generated, they're yours.
>
> Re-runs are smart about this: Skippr won't overwrite your manual changes. It adds new models alongside existing ones.

---

## "What about real-time / streaming data?"

> Today Skippr is batch-oriented — it runs on demand or on a schedule. We're exploring incremental streaming patterns, but the initial focus is on getting batch ELT + modeling right.
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
