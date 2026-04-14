# Launch content strategy

### Overview

Create a launch-ready content and docs strategy for `skippr-web` that supports a closed-source free-tier release, keeps the current `Like Codex, but for data` / `GitHub` framing, and sharpens the launch around `AI Data Agent`, AI-ready data quality/compliance, and technical trust.
todos:
  - id: audit-launch-surfaces
    content: Create a file-by-file launch audit of the current `skippr-web` blog, docs home, quickstarts, connector docs, and CDC docs with severity-ranked gaps.
  - id: align-launch-messaging
    content: Define the exact messaging changes needed for a free-but-closed launch across the docs/home and blog entry points while preserving the current `GitHub` CTA and `Like Codex, but for data` framing, and clarifying local/cloud boundaries.
    status: pending
  - id: prioritize-launch-content
    content: "Produce the launch-week content bundle: flagship announcement, AI Data Agent explainer, schema/CDC technical proof post, and one practical migration walkthrough."
    status: pending
  - id: harden-cdc-trust-pack
    content: Specify the missing CDC trust content needed for launch, especially production operations, guarantees terminology, and stronger cross-linking from connector pages.
    status: pending
  - id: normalize-critical-connectors
    content: Identify the launch-critical source and destination connector pages and define the minimum documentation bar they must meet before launch.
    status: pending
  - id: sequence-launch-channels
    content: Map the content bundle to Product Hunt, Hacker News, X/Twitter, and Reddit with a relative launch cadence and a single conversion path.
    status: pending
isProject: false
---

# Launch Content Strategy

## Goal

Use the existing `skippr-web` content base to support a launch that feels credible, technical, and easy to evaluate even though the free tier is closed source. The launch should create one sharp story for `Product Hunt`, `Hacker News`, `X/Twitter`, and `Reddit`: Skippr is an `AI Data Agent` that helps teams create AI-ready data with strong data quality, compliance, and trust. `EL(T)M` should appear as the technical wedge and proof of capability, not as the primary category label.

## What the current content says

- [docs/index.md](/Users/huders2000/Documents/sites/skippr/skippr-web/docs/index.md) should keep the homepage/docs hero, including the `GitHub` CTA and copy like `Like Codex, but for data.` and `AI-assisted dbt generation`. For a `free but closed-source` launch, it also needs to become the canonical explanation of what is free, what is local, what is cloud-backed, and why the product is trustworthy without source access.
- [src/components/Blog.vue](/Users/huders2000/Documents/sites/skippr/skippr-web/src/components/Blog.vue), [src/components/.generated-published.js](/Users/huders2000/Documents/sites/skippr/skippr-web/src/components/.generated-published.js), and [scripts/schedules.js](/Users/huders2000/Documents/sites/skippr/skippr-web/scripts/schedules.js) show that the blog is currently split between legacy posts and a long scheduled drip. For launch week, only a tiny number of dynamic posts are live, and there is no flagship launch announcement or founder/vision post.
- [docs/getting-started/quickstart.md](/Users/huders2000/Documents/sites/skippr/skippr-web/docs/getting-started/quickstart.md) promises that each connector guide covers `authentication`, `required permissions`, and `troubleshooting`, but the connector docs do not uniformly meet that bar yet.
- [docs/connectors/sources/index.md](/Users/huders2000/Documents/sites/skippr/skippr-web/docs/connectors/sources/index.md) and [docs/connectors/destinations/index.md](/Users/huders2000/Documents/sites/skippr/skippr-web/docs/connectors/destinations/index.md) provide strong breadth, but many individual connector pages are still shallow. That weakens trust if the launch claims broad production readiness.
- [docs/cdc/overview.md](/Users/huders2000/Documents/sites/skippr/skippr-web/docs/cdc/overview.md), [docs/cdc/sources.md](/Users/huders2000/Documents/sites/skippr/skippr-web/docs/cdc/sources.md), [docs/cdc/destinations.md](/Users/huders2000/Documents/sites/skippr/skippr-web/docs/cdc/destinations.md), and [docs/cdc/configuration.md](/Users/huders2000/Documents/sites/skippr/skippr-web/docs/cdc/configuration.md) are the strongest trust assets today. They explain order tokens, tombstones, and exactly-once final-state semantics well, but they still need a production-facing operations/trust layer.

## Strategy

### 1. Reframe the launch around trust, not openness

- Treat `closed-source free tier` as a trust challenge, not a positioning handicap.
- Replace the implicit OSS-discovery assumption with a proof-driven launch: reproducible docs, strong technical writing, visible outputs, and tight comparisons.
- Keep `AI Data Agent` as the headline category.
- Make `AI-ready data quality and compliance` the primary business-value message.
- Introduce `EL(T)M` as a secondary technical wedge that supports time to value, technical competence, and trust, rather than as the headline label.
- Make the free-tier promise explicit: local-first execution, deterministic outputs, standard dbt artifacts, and clear boundaries between free, pro, and enterprise.

### 2. Turn the launch into a content bundle, not a drip

- Keep the current longer SEO release schedule in [scripts/schedules.js](/Users/huders2000/Documents/sites/skippr/skippr-web/scripts/schedules.js) intact.
- Concentrate the most technical launch posts into a coordinated 7-10 day window so the launch has visible depth without sacrificing the long-tail SEO program.
- Keep comparison/SEO content as the tail, not the spearhead.

### 3. Make CDC and schema intelligence the proof surface

- Use CDC/exactly-once/WAL/schema handling as the technical core of the launch story.
- Do not rely on generic AI claims. Anchor the launch in concrete behavior: nested schema handling, order-token reconciliation, tombstones, deterministic dbt scaffolding, and migration-safe outputs.
- Add explicit trust content that explains limits, ops expectations, and failure/recovery behavior so technical readers do not feel they are being sold hand-wavy AI automation.

## Launch content architecture

### A. Launch-week hero assets

Create or prioritize these pieces first:

- A flagship announcement post: what Skippr is, who it is for, what `free` means, and how to try it.
- A clear `What is an AI Data Agent?` post elevated to launch-week priority rather than leaving it buried in the existing schedule.
- A deep technical post on schema discovery/evolution and why it matters in real pipelines. This should go into implementation detail and be written for technical scrutiny, not lightweight top-of-funnel reading.
- A CDC trust deep dive that links docs and blog together around exactly-once final-state semantics, again at a highly technical level.
- A WAL/recovery deep dive that explains durability, replay, and operational trust in implementation terms.
- One practical migration story, likely `Postgres -> Snowflake`, because the site already has strong related assets and it is the easiest evaluation path.
- One buyer-translation post for non-specialists, likely TCO/time-to-value, but only after the technical proof pieces are live.

### B. CDC trust pack

Use the existing CDC docs as the strongest launch-ready cluster and extend them with missing trust content:

- Add one production operations guide covering replication lag, WAL growth/retention, restart/recovery expectations, and what to monitor.
- Add one guarantees/terminology page clarifying `exactly-once final state` versus generic incremental sync claims.
- Add stronger cross-links from CDC-capable source connectors and supported destinations back into the CDC docs.
- Ensure the docs home or quickstart path visibly surfaces CDC as a differentiator rather than leaving it to discovery.

### C. Connector launch pack

Prioritize depth over breadth for launch:

- Pick the launch-critical connector set first: `Postgres`, `MySQL`, `MSSQL`, `MongoDB`, `DynamoDB`, `Kafka`, plus `Snowflake`, `BigQuery`, and `Postgres` destination.
- Bring those pages up to a consistent bar: auth, permissions, network requirements, CDC support, limitations, troubleshooting, and links to relevant quickstarts.
- Downgrade or explicitly mark thinner connectors as secondary or reference-only until they meet the same bar.
- Fix quickstart overstatement in [docs/getting-started/quickstart.md](/Users/huders2000/Documents/sites/skippr/skippr-web/docs/getting-started/quickstart.md) or raise connector pages to match it.

### D. Blog and schedule cleanup

- Keep the broader SEO schedule intact, but make sure the launch-week lineup reads like a coherent narrative rather than a random archive plus a long future queue.
- Use [src/components/Blog.vue](/Users/huders2000/Documents/sites/skippr/skippr-web/src/components/Blog.vue) and [scripts/schedules.js](/Users/huders2000/Documents/sites/skippr/skippr-web/scripts/schedules.js) to make sure the visible posts support launch-day visitors.
- Lead with a small set of launch-critical posts clustered across 7-10 days, then continue the longer comparison and educational schedule afterward.
- Keep competitor posts, but they should support search capture after launch rather than dominate the initial impression.

## Recommended launch-week content sequence

### T-14 to T-7

- Refresh the top-level docs/home messaging to align with `free but closed-source`, keep the existing `GitHub` and `Like Codex, but for data` framing, and add clearer local/cloud/trust explanation.
- Upgrade launch-critical connector docs and CDC trust docs.
- Prepare the flagship announcement and schema/CDC deep dives.

### T-7 to T-1

- Publish or queue the `AI Data Agent` explainer, the highly technical schema/CDC/WAL proof posts, and one practical migration walkthrough across the same 7-10 day launch window.
- Tighten internal links across docs, quickstarts, and blog so each channel entry point can lead a user toward installation and first run.
- Make sure the blog index and sitemap show a coherent set of live assets, not mostly future posts.

### Launch day

- Ship the flagship announcement.
- Use the technical proof post for `Hacker News` and technical `Reddit`.
- Use the practical migration/quickstart story for `Product Hunt` and X/Twitter clips.
- Link all channels back to the same install + quickstart + trust path.

### Post-launch 2-4 weeks

- Resume the broader comparison/SEO pipeline already represented in [scripts/schedules.js](/Users/huders2000/Documents/sites/skippr/skippr-web/scripts/schedules.js).
- Publish one customer/design-partner proof point and one security/trust explainer as soon as possible.
- Expand connector coverage depth based on inbound demand rather than trying to perfect every page before launch.

## Channel plan

- `Hacker News`: lead with the deepest CDC/schema/WAL implementation post, clear caveats, and reproducible examples. Avoid marketing-first language.
- `X/Twitter`: cut short threads/clips from the technical schema, CDC, and WAL proof assets, plus one end-to-end run showing `source -> raw -> dbt project`.
- `Reddit`: use narrowly scoped, technical posts in relevant subreddits and link to docs that answer skepticism directly.

## Success criteria for the content pass

- A new visitor can understand `what Skippr is`, `why it is trustworthy`, `what is free`, and `how to run it` within one short path.
- The visible blog index supports the launch story on day one.
- Launch-critical connector docs meet the standard promised by quickstart.
- CDC, WAL, and schema content support both headline differentiation and serious technical scrutiny.
- The launch does not depend on open source to earn credibility; the content itself supplies the proof.

