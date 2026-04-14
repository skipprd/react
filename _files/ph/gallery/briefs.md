# Product Hunt Gallery — Image Briefs

**Dimensions:** 1270 x 760 px (PH standard)
**Format:** PNG or JPEG, optimised for retina
**Style direction:** Clean, modern, dark background (#0D1117 or similar) with accent colour. Minimal UI chrome. Typography-forward. No stock photos.

---

## Slide 1 — Hero / Codex Parallel

**Headline:** Like Codex, but for data.

**Body copy:** Skippr is an AI Data Agent that turns raw source data into AI-ready warehouse assets. Same agent workflow developers already understand. Different domain.

**Visual direction:**
- Centred headline in large, bold sans-serif
- Split visual: left side shows a code editor (Codex territory), right side shows a terminal with dbt output (Skippr territory)
- Subtle connecting element between the two halves — an AI "spark" or neural line
- Skippr logo top-left or top-centre
- Small subline below: "Local-first execution. Reviewable output."

**Purpose:** First impression. Instantly positions Skippr via a reference developers already understand.

---

## Slide 2 — The Problem

**Headline:** Your data exists. It just is not AI-ready.

**Body copy (left column — "Before"):**
- Untangle source schemas
- Handle drift and bad types
- Build ingestion and dbt scaffolding
- Validate outputs and fix failures
- Add data quality and compliance review
- Lose weeks before anyone can query anything

**Body copy (right column — "After / Skippr"):**
- `skippr init my-project`
- `skippr connect ...`
- `skippr run`
- Review the dbt output.

**Visual direction:**
- Split layout: left side cluttered/grey (complexity), right side clean/bright (simplicity)
- Left: tangled pipeline diagram or long checklist, muted colours
- Right: clean terminal with 3 commands and a green checkmark, accent colour

**Purpose:** Reframe the problem around time-to-trust, not just data movement.

---

## Slide 3 — AI Data Agent in Action

**Headline:** From raw sources to trustworthy warehouse assets.

**Visual direction:**
- Horizontal pipeline flow diagram with 5 stages:
  1. **Discover** — icon: magnifying glass + database
  2. **Extract & Load** — icon: arrows flowing into warehouse
  3. **Map & Type** — icon: filter / sparkle
  4. **Model** — icon: layered blocks (bronze / silver / gold)
  5. **Validate & Repair** — icon: checkmark with retry arrow
- Each stage connected by a subtle animated arrow or line
- Below each icon: one-line description
  - "Reads schemas from your sources"
  - "Loads raw data into bronze tables"
  - "Normalises names and types"
  - "Generates dbt assets you own"
  - "Validates and retries when needed"
- Sources shown at left: Postgres, MySQL, MSSQL, MongoDB, Kafka
- Warehouses shown at right: Snowflake, BigQuery, Postgres

**Purpose:** Show the full autonomous pipeline in one visual.

---

## Slide 4 — Local-first, Cloud-backed

**Headline:** Local-first data path. Cloud-backed services.

**Body copy:** Source data moves from your environment to your warehouse. Skippr uses cloud-backed auth, metering, and AI services, but the product is not a hosted data plane. By default the AI sees schema metadata, not rows.

**Visual direction:**
- Centre: illustration of a laptop with a shield/lock icon
- Left of laptop: source icons (database, bucket, stream) with arrows pointing IN to the laptop
- Right of laptop: warehouse icons (Snowflake, BigQuery, Postgres) with arrows pointing OUT from the laptop
- Above: Skippr cloud icon with thin dotted lines labelled "auth", "metering", and "metadata-only AI"
- Keep the data-flow arrows visually heavier than the cloud lines
- Callout badge: "Metadata-first AI by default"

**Purpose:** Explain the actual trust model clearly instead of over-claiming.

---

## Slide 5 — Technical Trust

**Headline:** Technical trust, not black-box magic.

**Body copy:** Skippr is built around the hard parts teams actually worry about: schema handling, deterministic dbt output, CDC, replay, and reviewable artifacts.

**Visual direction:**
- Three or four callout cards on one dark canvas:
  - "Schema discovery and mapping"
  - "CDC / WAL / exactly-once semantics"
  - "Deterministic dbt scaffolding"
  - "Migration-friendly output"
- Supporting micro-diagram showing source log -> local runner -> warehouse
- Optional small code or SQL snippet to make it feel real, not abstract

**Purpose:** Signal that the product has real technical depth behind the AI story.

---

## Slide 6 — Concrete Output

**Headline:** Standard dbt. Review it. Keep it.

**Visual direction:**
- Split layout:
- Left: terminal screenshot showing `skippr init`, `skippr connect ...`, `skippr run`
- Right: generated dbt file tree and a success summary

```text
$ skippr init bike-hire-analytics
  Project created.

$ skippr connect warehouse snowflake
  Warehouse connected. ✓

$ skippr connect source mssql
  Source connected. 3 schemas, 47 tables discovered. ✓

$ skippr run
  Planning extraction... ✓
  Syncing 47 tables to bronze... ✓
  Mapping schemas and types... ✓
  Generating silver models... ✓
  Generating gold models... ✓
  Compiling dbt project... ✓
  All models materialised. 12 silver, 5 gold.

  Done in 4m 32s.
```

- File tree callout:

```text
my-project/
├── dbt_project.yml
├── models/
│   ├── schema.yml
│   ├── staging/
│   └── marts/
└── packages.yml
```

- Small footer line: "Free to start. Public docs on GitHub."

**Purpose:** End on something developers can picture themselves using immediately.
