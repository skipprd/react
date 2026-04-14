# Product Hunt Demo Video Script
**Duration:** 75–90 seconds
**Format:** Screen recording with voiceover, light motion graphics for transitions
**Tone:** Confident, clear, unhurried. Technical, not salesy.
**Music:** Subtle lo-fi or ambient electronic, low volume under VO

---

## Scene 1 — The Hook (0:00–0:10)

**Visual:** Text on dark background, appearing line by line.

**On screen:**
> Codex reads your codebase and writes code.
> What if an agent could read your data sources
> and build the warehouse foundation?

**Voiceover:**
"Codex reads your codebase and writes code. What if an agent could do the same kind of first-pass work for your data: discover the schemas, load the raw tables, generate the dbt assets, validate them, and repair common failures?"

---

## Scene 2 — Introduce Skippr (0:10–0:20)

**Visual:** Skippr logo animates in. Tagline fades in below.

**On screen:**
> Skippr — like Codex, but for data.
> AI Data Agent for AI-ready warehouse assets

**Voiceover:**
"Meet Skippr. An AI Data Agent for turning raw source data into trustworthy warehouse assets."

---

## Scene 3 — Init + Connect (0:20–0:38)

**Visual:** Clean terminal. Commands typed in real time.

**Terminal shows:**
```text
$ skippr init bike-hire-analytics
  Project created.

$ skippr connect warehouse snowflake
  Warehouse connected. ✓

$ skippr connect source postgres
  Source connected. 3 schemas, 47 tables discovered. ✓
```

**Voiceover:**
"Initialise a project, connect your warehouse, and point Skippr at a source. It discovers schemas automatically and sets up the working context from one CLI."

---

## Scene 4 — Run (0:38–1:00)

**Visual:** Terminal continues. Each step appears with a brief pause and checkmark.

**Terminal shows:**
```text
$ skippr run
  Planning extraction... ✓
  Syncing 47 tables to bronze... ✓
  Mapping schemas and types... ✓
  Generating silver models... ✓
  Generating gold models... ✓
  Compiling dbt project... ✓
  Running dbt... ✓
  All models materialised. 12 silver, 5 gold.

  Done in 4m 32s.
```

**Voiceover:**
"One run path. Skippr lands bronze tables, maps names and types, generates dbt assets, validates them, and retries when needed. The result is reviewable output you keep."

**Motion graphic overlay:** Source -> Bronze -> Silver -> Gold

---

## Scene 5 — Trust Model (1:00–1:12)

**Visual:** Split screen. Left side shows terminal and data-flow arrows from source to warehouse. Right side shows a small cloud labelled auth, metering, metadata-only AI.

**Voiceover:**
"The trust model is local-first. Data moves from your environment to your warehouse. Skippr is cloud-backed for auth, metering, and AI services, but by default the AI works from schema metadata rather than row data."

---

## Scene 6 — Technical Depth (1:12–1:20)

**Visual:** Quick callout cards flash on screen:
- Schema handling
- CDC / WAL / exactly-once
- Deterministic dbt output
- Reviewable artifacts

**Voiceover:**
"This is not just an AI wrapper. The product is built around the hard technical parts teams actually care about: trust, repeatability, and output quality."

---

## Scene 7 — CTA (1:20–1:28)

**Visual:** Dark background. Skippr logo centred. Below it:

**On screen:**
> Like Codex, but for data.
>
> Free to start
>
> skippr.io

**Voiceover:**
"Skippr. Like Codex, but for data. Free to start. If this is your kind of problem, check us out on Product Hunt."

---

## Production Notes

- Record terminal in a clean environment with no personal files visible
- Use a large terminal font for readability on mobile
- Keep the terminal background consistent with the gallery slide styling
- Keep transitions minimal; the terminal output should do most of the selling
- Total duration target: 80 seconds
- Export at 1920x1080, MP4, H.264
