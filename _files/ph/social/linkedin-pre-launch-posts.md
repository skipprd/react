# Pre-Launch LinkedIn Posts

Post 1–2 per week. No product mentions. Just you sharing things you've hit while building.

---

## Post 1 — Schema evolution

Spent two days tracking down a bug that turned out to be a column rename in a source database.

The source team renamed `cust_name` to `customer_name`. Totally reasonable. No one told us. Our staging model referenced the old column, so it silently started returning nulls instead of failing. Downstream, a gold model was doing a coalesce that masked it. The dashboard looked fine — just with slightly wrong numbers that nobody noticed for a week.

The fix was trivial. The real problem was that we'd built the pipeline assuming the source schema was stable. It never is.

I ended up building a schema diffing step that runs before extraction — compares the current source schema against what we saw last time and flags anything that changed. It's maybe 200 lines of code and it's saved me more hours than any other thing I've written.

Lesson: source schemas are a moving target. If your pipeline doesn't detect drift before it starts syncing, you'll find out about it from a confused stakeholder, not from your logs.

---

## Post 2 — All or nothing

Had a fun one last month. Pipeline syncing 40-odd tables from MSSQL to Snowflake. Table 34 hit a type mismatch — a column that was supposed to be decimal came through as varchar because someone upstream changed a view.

So now I've got 33 tables that loaded fine, one that's half-written, and 6 that never started. The bronze layer is internally inconsistent. Anything that joins across those tables is quietly wrong.

Rolled it all back, fixed the type handling, re-ran. But it made me rethink the whole approach. Now I treat each sync run as a transaction — either every table lands or none of them do. If table 34 fails, tables 1–33 get rolled back too.

It's slower when things go wrong. But "partially loaded" is worse than "failed cleanly" every single time. At least with a clean failure you know where you stand.

---

## Post 3 — Nobody talks about integration

I keep seeing posts about the modelling layer. Silver models, gold models, dimensional modelling vs OBT, dbt best practices. All good stuff.

But every project I've worked on, the actual bottleneck is getting data into a state where modelling is even possible. The source has columns called `col1` and `CSTMR_NM_V2`. Timestamps are a mix of UTC and local with no indication which. Currency is stored as a string in some tables and a float in others.

You can't dbt your way out of that. The decisions you make at extraction time — what to include, how to cast it, what to rename — shape everything downstream. Get that wrong and you're just moving garbage through a well-structured pipeline.

I've started treating integration and modelling as one problem rather than two separate phases. The schema mapping decisions at load time are really modelling decisions in disguise.

---

## Post 4 — Not everything needs an EL tool

Quick thing that saves me time: before I set up extraction for a new source, I ask whether I actually need to extract at all.

A Postgres database the warehouse can reach directly? Foreign data wrapper or a direct query. Done in 10 minutes. No extraction job, no scheduling, no state to manage.

An S3 bucket with daily CSV drops? Yeah, I need extraction — file detection, schema inference, deduplication.

A REST API with pagination and rate limits? Definitely need a proper connector.

I used to default to "set up an EL pipeline for everything" and it created way more infrastructure than necessary. Now I only extract when the source genuinely can't be queried in place.

Sounds obvious but I've seen teams maintain 30+ extraction jobs when half the sources could have been queried directly.

---

## Post 5 — Data-stuck

Talked to a company last month that had 18 months of transactional data sitting in MSSQL. They knew it was valuable. They'd been meaning to "do something with it" for over a year.

They didn't have a data team. They'd looked at hiring one but couldn't justify the cost for what was essentially a reporting need. They'd tried a couple of SaaS tools but got stuck on configuration and gave up.

The data wasn't bad. It was just... inaccessible. Locked in a system that was great for running the business but useless for answering questions about it.

This is way more common than I expected. The gap between "we have data" and "we can query it" isn't a technology problem. It's a setup-cost problem. The tools exist — most companies just can't absorb the time and expertise needed to wire them together.

I keep thinking about how to make that gap smaller.

---

## Post 6 — Prepping data for AI is boring (on purpose)

Everyone's talking about RAG and vector databases and fine-tuning. But the most impactful thing I've done for AI readiness at work is... rename columns.

Seriously. An LLM trying to work with a table where the columns are `c1`, `c2`, `amt_01`, `dt_cr` is going to hallucinate nonsense. Change those to `customer_id`, `order_date`, `amount`, `created_at` and suddenly the model can reason about the data because the names actually mean something.

Same with types. If `revenue` is stored as varchar, the LLM has no idea it's a number. Cast it correctly in your staging layer and everything downstream — AI or otherwise — just works better.

AI readiness isn't some fancy new discipline. It's the same data engineering hygiene we should have been doing all along. Clean names, correct types, documented relationships. The LLM just makes the cost of not doing it more obvious.

---

## Post 7 — Metadata is enough

Took me a while to realise this: you don't need to send actual data to an LLM to get useful schema mapping out of it.

Table names, column names, data types, nullable flags, a few distinct values. That's metadata. And it's enough for the model to suggest reasonable column renames, infer the right staging types, and even propose join keys between tables.

I was paranoid about sending client data to an API so I tried stripping it down to just the schema. Turned out the results were basically the same. The LLM doesn't need to see that row 4,371 has a revenue of $42.50 to know that a column called `rev_amt` with type decimal is probably revenue.

Now I don't send row data at all. Metadata only. Same quality output, zero privacy risk. Feels like this distinction is going to matter a lot more as people start plugging AI into their data stacks.

---

## Post 8 — Respect the bronze layer

Bit of a rant: stop treating your bronze layer as a temporary dumping ground.

Bronze is your receipt. It's the raw, unmodified record of what your source looked like when you extracted it. If your silver model has a bug, you rebuild from bronze. If a business rule changes, you reprocess from bronze. If an auditor asks what the original value was, bronze answers that.

I've seen teams drop and recreate bronze tables on every run, or give them a 7-day retention, or just not think about them at all. Then six months later someone needs to backfill a model and there's nothing to backfill from.

Treat bronze as immutable, append-only if possible, and keep it around. Everything downstream is derived and can be rebuilt. Bronze can't.

---

## Post 9 — Running pipelines locally (optional — closer to product territory)

I've been running my data pipelines on my laptop for the past few months. Not as a hack — as an actual architecture choice.

The extraction and loading runs locally. Data goes straight from the source to the warehouse over the network. No hosted orchestrator, no managed service in the middle, no credentials stored in someone else's cloud.

Honestly the main reason I tried it was laziness — I didn't want to set up Airflow for a project that had 30 tables. But it turned out to be genuinely better for iteration speed. Change a config, re-run, see the result immediately. No deploy cycle, no waiting for a remote job to spin up.

The warehouse still does the heavy compute. But the pipeline itself doesn't need to live in the cloud to work. For a lot of workloads — especially early-stage or mid-size — local execution is simpler, faster, and more private than the alternative.

Not sure why it took me so long to question the assumption that pipelines need infrastructure.

---

## Posting cadence

| Week | Posts |
|------|-------|
| W-4 | Post 1, Post 4 |
| W-3 | Post 3, Post 6 |
| W-2 | Post 2, Post 7 |
| W-1 | Post 5, Post 8 |
| Launch week | Post 9 (optional), then PH announcement |

Post 7–9 AM Tue/Wed/Thu. Reply to every comment. No hashtags in the post body — one in the first comment if you want (#dataengineering). Don't put links in the post itself, LinkedIn buries them. Link in first comment if needed.
