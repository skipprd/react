# Product Hunt Gallery — Image Briefs

**Dimensions:** 1270 x 760 px (PH standard)
**Format:** PNG or JPEG, optimised for retina
**Style direction:** Clean, modern, dark background (#0D1117 or similar) with accent colour. Minimal UI chrome. Typography-forward. No stock photos.

---

## Slide 1 — Hero / Codex Parallel

**Headline:** Like Codex, but for data.

**Body copy:** Codex reads your codebase and writes code. Skippr reads your data sources and writes dbt models. Both produce artifacts you review and own.

**Visual direction:**
- Centred headline in large, bold sans-serif
- Split visual: left side shows a code editor (Codex territory), right side shows a terminal with dbt output (Skippr territory)
- Subtle connecting element between the two halves — an AI "spark" or neural line
- Skippr logo top-left or top-centre
- Small subline below: "Runs on your machine. Your data stays local."

**Purpose:** First impression. Instantly positions Skippr via a reference developers already understand.

---

## Slide 2 — The Problem

**Headline:** Months of setup. Or minutes with an agent.

**Body copy (left column — "Before"):**
- Hire a data team
- Provision infrastructure
- Build extraction pipelines
- Write transformation logic
- Test and validate models
- Maintain everything

**Body copy (right column — "After / Skippr"):**
- `skippr init my-project`
- `skippr run`
- Done.

**Visual direction:**
- Split layout: left side cluttered/grey (complexity), right side clean/bright (simplicity)
- Left: tangled pipeline diagram or long checklist, muted colours
- Right: clean terminal with 2 commands and a green checkmark, accent colour

**Purpose:** Contrast pain vs. solution. Makes the automation tangible.

---

## Slide 3 — Agent in Action

**Headline:** From source to production dbt models. Autonomously.

**Visual direction:**
- Horizontal pipeline flow diagram with 5 stages:
  1. **Discover** — icon: magnifying glass + database
  2. **Extract & Load** — icon: arrows flowing into warehouse
  3. **Cleanse** — icon: filter / sparkle
  4. **Model** — icon: layered blocks (bronze / silver / gold)
  5. **Validate & Repair** — icon: checkmark with retry arrow
- Each stage connected by a subtle animated arrow or line
- Below each icon: one-line description
  - "Reads schemas from your sources"
  - "Loads raw data into bronze tables"
  - "Maps columns, casts types, renames"
  - "Generates silver & gold dbt models"
  - "Validates and auto-repairs failures"
- Sources shown at left: MSSQL, S3 logos
- Warehouses shown at right: Snowflake, BigQuery, Postgres logos

**Purpose:** Show the full autonomous pipeline in one visual.

---

## Slide 4 — Privacy

**Headline:** Your data never leaves your machine.

**Body copy:** All data transfer is local. Source to warehouse, directly. AI mapping uses only schema metadata — table and column names — never row-level data.

**Visual direction:**
- Centre: illustration of a laptop with a shield/lock icon
- Left of laptop: source icons (database, S3 bucket) with arrows pointing IN to the laptop
- Right of laptop: warehouse icons (Snowflake, BigQuery) with arrows pointing OUT from the laptop
- Above: AI cloud icon with a small dotted line labelled "metadata only" — visually distinct from the bold data-flow arrows
- Red X or crossed-out line showing data does NOT go to any third-party cloud
- Callout badge: "Zero third-party data access"

**Purpose:** Address the #1 concern. Build trust immediately.

---

## Slide 5 — Concrete Output

**Headline:** Standard dbt. Nothing proprietary.

**Body copy:** The agent generates a complete dbt project — source definitions, staging models, business-ready marts. You own it. Review it, extend it, plug it into CI/CD.

**Visual direction:**
- File tree showing generated dbt project structure:
  ```
  my-project/
  ├── dbt_project.yml
  ├── models/
  │   ├── schema.yml
  │   ├── staging/
  │   │   ├── stg_customers.sql
  │   │   └── stg_orders.sql
  │   └── marts/
  │       └── fct_revenue.sql
  └── packages.yml
  ```
- Clean terminal-style presentation
- Highlight: "12 silver models, 5 gold models — all validated"

**Purpose:** Show developers exactly what they get. Concrete, tangible output.

---

## Slide 6 — CLI in Action

**Headline:** Four commands. Minutes to your first query.

**Visual direction:**
- Full-width terminal screenshot (dark theme) showing:

```
$ skippr init bike-hire-analytics
  Project created.

$ skippr connect warehouse snowflake
  Warehouse connected. ✓

$ skippr connect source mssql
  Source connected. 3 schemas, 47 tables discovered. ✓

$ skippr run
  Planning extraction... ✓
  Syncing 47 tables to bronze... ✓
  Generating silver models... ✓
  Generating gold models... ✓
  Compiling dbt project... ✓
  All models materialised. 12 silver, 5 gold.

  Done in 4m 32s.
```

- Terminal styled with a modern font (JetBrains Mono or similar)
- Green checkmarks for completed steps
- Subtle glow or highlight on the final "Done" line

**Purpose:** Show real usage. Developers trust what they can see running.
