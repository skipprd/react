# Product Hunt Gallery — Image Briefs

**Dimensions:** 1270 x 760 px (PH standard)
**Format:** PNG or JPEG, optimised for retina
**Style direction:** Clean, modern, dark background (#0D1117 or similar) with accent colour. Minimal UI chrome. Typography-forward. No stock photos.

---

## Slide 1 — Hero

**Headline:** The enterprise data platform that builds itself.

**Body copy:** Skippr discovers, integrates, cleanses, and models your data — then gives you AI-ready insights. No infrastructure. No data team. No friction.

**Visual direction:**
- Centred headline in large, bold sans-serif (e.g. Inter Bold or similar)
- Subtle animated-style data flow illustration behind text: abstract lines/nodes moving from left (sources) to right (insights)
- Skippr logo top-left or top-centre
- Small subline below: "Your data never leaves your machine. You choose the LLM."

**Purpose:** First impression. Communicates the value prop in under 3 seconds.

---

## Slide 2 — The Problem

**Headline:** Months of setup. Or minutes with Skippr.

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

## Slide 3 — How It Works

**Headline:** From source to insight. Automatically.

**Visual direction:**
- Horizontal pipeline flow diagram with 5 stages:
  1. **Discover** — icon: magnifying glass + database
  2. **Integrate** — icon: arrows flowing into warehouse
  3. **Cleanse** — icon: filter / sparkle
  4. **Model** — icon: layered blocks (bronze / silver / gold)
  5. **Insights** — icon: chart / lightbulb
- Each stage connected by a subtle animated arrow or line
- Below each icon: one-line description
  - "Reads schemas from your sources"
  - "Loads raw data into your warehouse"
  - "Types, renames, validates"
  - "Generates dbt silver & gold models"
  - "Business-ready, AI-ready data"
- Sources shown at left: MSSQL, S3 logos
- Warehouses shown at right: Snowflake, BigQuery, Postgres logos

**Purpose:** Show the full automated pipeline in one visual.

---

## Slide 4 — Privacy

**Headline:** Your data never leaves your machine.

**Body copy:** All data transfer is local. Source to warehouse, directly. The LLM only sees table names and column metadata — never your actual data.

**Visual direction:**
- Centre: illustration of a laptop with a shield/lock icon
- Left of laptop: source icons (database, S3 bucket) with arrows pointing IN to the laptop
- Right of laptop: warehouse icons (Snowflake, BigQuery) with arrows pointing OUT from the laptop
- Above: LLM cloud icon with a small dotted line labelled "metadata only" — visually distinct from the bold data-flow arrows
- Red X or crossed-out line showing data does NOT go to any third-party cloud
- Callout badge: "Zero third-party data access"

**Purpose:** Address the #1 enterprise concern. Build trust immediately.

---

## Slide 5 — Choose Your LLM

**Headline:** You choose the LLM.

**Body copy:** Works with any OpenAI-compatible model. GPT, Claude, Mistral, or run it locally. Your policy, your choice.

**Visual direction:**
- Centre: Skippr logo or terminal icon
- Radiating outward: logos/icons for supported LLM providers arranged in a circle or arc:
  - OpenAI
  - Anthropic (Claude)
  - Mistral
  - Local/self-hosted (generic server icon)
- Connecting lines from each to the centre, suggesting plug-and-play
- Small badge: "OpenAI-compatible API"

**Purpose:** Differentiate from locked-in AI tools. Appeal to security-conscious and cost-conscious buyers.

---

## Slide 6 — CLI in Action

**Headline:** Four commands. Minutes to value.

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
