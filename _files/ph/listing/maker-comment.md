# Maker Comment (First Comment)

Post this as the maker's first comment immediately after launch.

---

Hey Product Hunt!

We built Skippr because we kept seeing the same pattern: companies sitting on valuable data they couldn't use — not because the data was bad, but because the path from raw source to usable insight required months of setup, an expensive data team, and a stack of infrastructure that most organisations don't have.

We thought: what if the entire data pipeline just built itself?

Skippr is a CLI that connects to your sources, extracts and loads your data into a warehouse, and then uses AI to automatically generate clean, tested, business-ready dbt models — bronze, silver, and gold — in minutes.

A few things we're proud of:

- **Your data never leaves your machine.** All data transfer is local — source to warehouse, directly. No third-party service touches a row.
- **The LLM only sees metadata.** Table names and column types, never your actual data.
- **You choose the LLM.** Works with any OpenAI-compatible model. Use GPT, Claude, Mistral, local models — whatever fits your policy.
- **It's incremental.** Re-runs only sync what changed. Your existing models are preserved.

We're launching with support for MSSQL and S3 as sources, and Snowflake, BigQuery, and Postgres as warehouses. More connectors are on the way.

We'd genuinely love your feedback — what sources would you want next? What would make this useful for your stack? Let us know in the comments.

Thanks for checking us out!
