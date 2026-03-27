# Product Hunt Demo Video Script

**Duration:** 75–90 seconds
**Format:** Screen recording with voiceover, light motion graphics for transitions
**Tone:** Confident, clear, unhurried. Not salesy — show, don't tell.
**Music:** Subtle lo-fi or ambient electronic, low volume under VO

---

## Scene 1 — The Problem (0:00–0:10)

**Visual:** Text on dark background, appearing line by line (kinetic typography)

**On screen:**
> Getting value from your data shouldn't take months.
> It shouldn't require a dedicated team.
> Or a wall of infrastructure.

**Voiceover:**
"Getting real value from your data usually means months of setup — hiring a data team, provisioning infrastructure, writing pipelines, building models. What if all of that just happened automatically?"

---

## Scene 2 — Introduce Skippr (0:10–0:18)

**Visual:** Skippr logo animates in. Tagline fades in below.

**On screen:**
> Skippr — The enterprise data platform that builds itself.

**Voiceover:**
"Meet Skippr. Point it at your data. It does the rest."

---

## Scene 3 — Init + Connect (0:18–0:35)

**Visual:** Clean terminal. Commands typed in real-time (or sped-up typing effect).

**Terminal shows:**
```
$ skippr init bike-hire-analytics
  Project created.

$ skippr connect warehouse snowflake
  Warehouse connected. ✓

$ skippr connect source mssql
  Source connected. 3 schemas, 47 tables discovered. ✓
```

**Voiceover:**
"Initialise a project, connect your warehouse — Snowflake, BigQuery, or Postgres — and point Skippr at your source. It discovers every table and schema automatically."

---

## Scene 4 — Run (0:35–1:00)

**Visual:** Terminal continues. Each step appears with a brief pause and checkmark.

**Terminal shows:**
```
$ skippr run
  Planning extraction... ✓
  Syncing 47 tables to bronze... ✓
  Generating silver models (AI-assisted)... ✓
  Generating gold models... ✓
  Compiling dbt project... ✓
  Running dbt... ✓
  All models materialised. 12 silver, 5 gold.

  Done in 4m 32s.
```

**Voiceover:**
"One command. Skippr extracts your data, loads it into bronze tables, then uses AI to generate clean silver models and business-ready gold models — all built on dbt. Schema mapping, type inference, naming conventions — handled."

**Motion graphic overlay (brief):** Pipeline diagram flashes: Source → Bronze → Silver → Gold

---

## Scene 5 — Privacy (1:00–1:10)

**Visual:** Split screen — left: terminal still visible. Right: privacy diagram (laptop with arrows to warehouse, LLM cloud with "metadata only" label).

**Voiceover:**
"And here's the thing — your data never leaves your machine. It flows directly from source to warehouse. The LLM only ever sees table names and column metadata. Never your actual data. And you choose which LLM to use."

---

## Scene 6 — CTA (1:10–1:20)

**Visual:** Dark background. Skippr logo centred. Below it:

**On screen:**
> From raw data to AI-ready insights. Automatically.
>
> skippr.io
>
> Upvote us on Product Hunt ↑

**Voiceover:**
"Skippr. From raw data to AI-ready insights. Try it today."

---

## Production Notes

- Record terminal in a clean environment (no personal files visible)
- Use a large font size in terminal (18–20pt) for readability on mobile
- Keep the terminal background consistent with gallery slide styling
- Voiceover should be recorded in a quiet room, natural cadence
- Total duration target: 75 seconds (max 90)
- Export at 1920x1080, MP4, H.264
