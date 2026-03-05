# Environment Variables

All environment variables recognised by ReAct. Variables override YAML config values (see [precedence](overview.md)).

## Secrets

These variables carry sensitive values and should never be placed in YAML config files.

| Variable | Description |
|---|---|
| `LLM_API_KEY` | API key for the LLM provider |
| `GOOGLE_APPLICATION_CREDENTIALS` | Path to Google Cloud service account JSON (BigQuery) |
| `AWS_ACCESS_KEY_ID` | AWS access key (Athena, S3) |
| `AWS_SECRET_ACCESS_KEY` | AWS secret key (Athena, S3) |
| `AWS_DEFAULT_REGION` | AWS region |

## LLM

| Variable | Default | Description |
|---|---|---|
| `LLM_PROVIDER` | `OPENAI_COMPAT` | LLM provider (`OPENAI_COMPAT`, `OPENAI`, `LLAMA_CPP`, `null`) |
| `LLM_BASE_URL` | | Base URL for the LLM API |
| `LLM_CHAT_MODEL` | | Chat model identifier |
| `LLM_EMBED_MODEL` | | Embedding model identifier |
| `LLM_CONTEXT_LENGTH` | `4096` | Max input context tokens |
| `LLM_MAX_TOKENS` | `1024` | Max output tokens per call |
| `LLM_TEMPERATURE` | | Sampling temperature |
| `LLM_TOP_P` | | Nucleus sampling parameter |
| `LLM_HTTP_TIMEOUT_SECS` | `30` | HTTP timeout for LLM calls (seconds) |
| `LLM_GPU_LAYERS` | | GPU layers to offload (Llama.cpp only) |

## Storage

| Variable | Default | Description |
|---|---|---|
| `REACT_STORAGE_MODE` | `local` | Storage backend (`local`, `s3`) |
| `REACT_STORAGE_PATH` | `./.react` | Local storage directory |
| `SKIPPR_S3_BUCKET` | | S3 bucket for storage |

## dbt

| Variable | Default | Description |
|---|---|---|
| `DBT_TARGET` | | dbt target (e.g. `athena`, `bigquery`) |
| `DBT_PROFILES_DIR` | | Custom dbt profiles directory |
| `DBT_TARGET_SCHEMA` | | Base schema name for dbt materialisation |
| `DBT_SILVER_SUFFIX` | `silver` | Silver tier schema suffix |
| `DBT_GOLD_SUFFIX` | `warehouse` | Gold tier schema suffix |
| `DBT_RUNNER` | `host` | dbt runner mode (`host`, `docker`) |
| `DBT_DOCKER_IMAGE` | | Docker image for dbt (when runner=docker) |
| `DBT_DOCKER_PLATFORM` | | Docker platform |
| `DBT_DOCKER_NETWORK` | | Docker network |
| `DBT_DOCKER_MOUNT_AWS_DIR` | `false` | Mount ~/.aws into Docker container |

## Runtime

| Variable | Default | Description |
|---|---|---|
| `REACT_HEADLESS` | `false` | Auto-answer prompts in headless `run` mode |
| `REACT_PLAIN_PROGRESS` | `false` | Plain progress output (no terminal UI) |
| `REACT_LOG_DIR` | | Override log output directory |
| `RUST_LOG` | | Standard Rust log filter (e.g. `info`, `react=debug`) |
