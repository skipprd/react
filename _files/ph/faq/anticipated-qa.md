# Anticipated Product Hunt Q&A

Pre-written responses for likely comments. Adapt tone to feel natural — don't copy-paste robotically.

---

## "How is this different from Fivetran / Airbyte / Stitch?"

> Great question. Tools like Fivetran, Airbyte, and Stitch are mostly about getting data from A to B. They are valuable, but they usually stop before the warehouse foundation is actually trustworthy and ready for analytics or AI.
>
> Skippr is an AI Data Agent. It handles the extract, load, and model path together: discovery, schema mapping, dbt scaffolding, validation, and repair. The output is standard dbt you own.
>
> So the difference is not just "another connector." The goal is AI-ready data quality and compliance faster, not just synced tables.

---

## "How is this like Codex?"

> Codex reads your codebase, writes code, tests it, and iterates on failures. Skippr reads your data sources, writes the warehouse foundation, generates dbt assets, validates them, and iterates on failures.
>
> Both are autonomous agents that produce reviewable artifacts. Codex gives you code. Skippr gives you standard dbt and modeled warehouse assets. In both cases, you review the output and keep ownership.

---

## "Is this an AI agent or a tool?"

> We position it as an AI Data Agent.
>
> Skippr does not just move data from A to B. It reads schemas, makes mapping decisions, generates dbt assets, validates them, and iterates on failures. You review the output. The agent handles the repetitive engineering.

---

## "Is my data really private? How can I trust that?"

> The data path is local-first. Skippr reads from your environment and writes directly to your warehouse. We are not running your row-level data through a hosted Skippr data plane.
>
> The product is still cloud-backed for things like auth, metering, and AI services, so we do not want to imply "no cloud involved anywhere."
>
> By default the AI works from schema metadata: table names, column names, and data types. Data samples are optional and off by default.

---

## "Does this replace my data team?"

> Not exactly — it replaces the tedious setup work that occupies the first weeks or months of any data project. Discovery, extraction, loading, initial model generation — that's what the agent automates.
>
> Once you have clean silver and gold models, your data engineers (or analysts, or AI systems) focus on the high-value work: domain-specific metrics, business logic, data quality rules, and stakeholder requirements — the work that actually requires human judgment.
>
> For smaller teams without dedicated data engineers, Skippr can get you surprisingly far on its own.

---

## "What sources and warehouses are supported?"

> The launch story focuses on common operational databases and warehouses, plus the technical docs behind them.
>
> Today that includes launch-critical support and docs around sources like PostgreSQL, MySQL, MSSQL, MongoDB, DynamoDB, Kafka, and file/object-store flows, with Snowflake, BigQuery, PostgreSQL, and other warehouse targets documented on the site.
>
> If there is a connector pair you care about, tell us. We are prioritising depth and trust on the most requested combinations first.

---

## "Is this open source?"

> Not at launch. Skippr is a commercial product with a free tier.
>
> We do keep public docs and material on GitHub, but the core runtime is closed right now. We know some people care a lot about that decision, so if open interfaces or open-sourced components matter to you, we would genuinely like to hear which pieces would build the most trust.

---

## "How does pricing work?"

> Skippr is free to start.
>
> The paid tiers are about going deeper on automation and control:
>
> - `Free` gets you the local-first wedge, basic discovery and mapping, and deterministic dbt scaffolding
> - `Pro` is where deeper schema intelligence, richer modeling, production workflows, and more agent capability show up
> - `Enterprise` is where governance, control plane, and compliance-heavy workflows live
>
> If you want, we can point you to the latest pricing and packaging details directly.

---

## "How does it handle schema changes or new tables?"

> Skippr runs are incremental. When you re-run, it:
>
> - Detects new tables and adds them
> - Syncs only new or changed rows for existing tables
> - Preserves your existing dbt models and extends them as needed
>
> Basic discovery and mapping are available in the free product. Deeper schema intelligence and evolution are part of the paid direction, because that is one of the hardest and most valuable parts of the system.

---

## "Can I customise the generated dbt models?"

> Absolutely. Skippr generates standard dbt models — .sql and .yml files in a dbt project directory. You can edit them, extend them, add custom tests, or layer your own models on top. Once generated, they're yours.
>
> Re-runs are designed to be migration-friendly. The goal is to preserve and extend existing work, not clobber it.

---

## "What about real-time / streaming data?"

> CDC is a real part of the technical story for us.
>
> For supported source and destination pairs, Skippr can do CDC with stronger semantics than a simple append-only stream. A lot of our current technical writing goes deep on WAL, order tokens, tombstones, and exactly-once final-state behavior because trust really matters here.
>
> If you have a specific streaming or CDC pair in mind, tell us. That helps us prioritise the right depth.

---

## "Why a CLI and not a web UI?"

> A few reasons:
>
> 1. **Local-first trust** — the data path stays in your environment rather than forcing everything through a hosted app.
> 2. **Composability** — it fits existing workflows, CI/CD, cron jobs, and automation.
> 3. **Speed** — you can change a config, run it, and inspect the result immediately.
>
> That said, we do believe in a cloud-backed control plane over time for things like monitoring, governance, review, and approvals. We just do not want the core data path to depend on a browser first.
