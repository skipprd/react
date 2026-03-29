# Product Hunt Demo Video Script

**Duration:** 75–90 seconds
**Format:** Screen recording with voiceover, light motion graphics for transitions
**Tone:** Confident, clear, unhurried. Not salesy — show, don't tell.
**Music:** Subtle lo-fi or ambient electronic, low volume under VO

---

## Scene 1 — The Hook (0:00–0:10)

**Visual:** Text on dark background, appearing line by line (kinetic typography)

**On screen:**
> Codex reads your codebase and writes code.
> What if an agent could read your data sources
> and write the pipeline?

**Voiceover:**
"Codex reads your codebase and writes code. What if an agent could do the same thing for your data — extract it, load it, generate the dbt models, validate them, and repair failures — all autonomously?"

---

## Scene 2 — Introduce Skippr (0:10–0:18)

**Visual:** Skippr logo animates in. Tagline fades in below.

**On screen:**
> Skippr — like Codex, but for data.

**Voiceover:**
"Meet Skippr. Point it at your data sources. It builds the entire pipeline."

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
"One command. Skippr extracts your data, loads it into bronze tables, then autonomously generates clean silver staging models and business-ready gold models — all built on dbt. Schema mapping, type inference, naming conventions, validation, and repair — handled by the agent."

**Motion graphic overlay (brief):** Pipeline diagram flashes: Source → Bronze → Silver → Gold

---

## Scene 5 — Privacy (1:00–1:10)

**Visual:** Split screen — left: terminal still visible. Right: privacy diagram (laptop with arrows to warehouse, AI cloud with "metadata only" label).

**Voiceover:**
"And your data never leaves your machine. It flows directly from source to warehouse. AI mapping uses only schema metadata — table and column names. Never your actual data."

---

## Scene 6 — CTA (1:10–1:20)

**Visual:** Dark background. Skippr logo centred. Below it:

**On screen:**
> Like Codex, but for data.
>
> skippr.io
>
> Upvote us on Product Hunt ↑

**Voiceover:**
"Skippr. Like Codex, but for data. From raw sources to production dbt models in minutes."

---

## Production Notes

- Record terminal in a clean environment (no personal files visible)
- Use a large font size in terminal (18–20pt) for readability on mobile
- Keep the terminal background consistent with gallery slide styling
- Voiceover should be recorded in a quiet room, natural cadence
- Total duration target: 75 seconds (max 90)
- Export at 1920x1080, MP4, H.264
