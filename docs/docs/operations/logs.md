# Logs and Artifacts

## Local artifacts

All runtime artifacts are stored under `.skippr/` in the working directory:

```
.skippr/
└── local/
    └── dev/
        └── <project>/
            ├── logs/                   # Per-run log files
            │   └── <run-id>.log
            ├── pipeline/               # Generated pipeline config
            └── lancedb/               # Semantic search index (auto-managed)
```

## Log files

Each pipeline run produces a log file under `.skippr/local/dev/<project>/logs/`.

### Viewing logs

With the live terminal UI (default when `--log` is not specified), logs are displayed in real time.

For headless operation or CI, use structured logging:

```bash
skippr run --log info     # standard output
skippr run --log debug    # verbose
skippr run --log trace    # most verbose
```

### Daily rotating log

A daily rotating log file is also written to `.skippr/local/dev/<project>/logs/react.YYYY-MM-DD.log` for debugging after the fact.

## dbt project files

The generated dbt project is written into the working directory:

```
models/
├── schema.yml             # Source definitions
└── staging/
    └── stg_*.sql          # Silver models

dbt_project.yml            # Auto-generated dbt project
profiles.yml               # Auto-generated dbt profile (reads credentials from env)
packages.yml               # dbt package dependencies
```

After the pipeline completes, you can run dbt directly:

```bash
source .venv/bin/activate
dbt debug   --profiles-dir .
dbt run     --profiles-dir .
dbt test    --profiles-dir .
```
