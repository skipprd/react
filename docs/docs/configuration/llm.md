# LLM

Configures the Large Language Model provider used by the ReAct agent loop.

## llm.provider

| | |
|---|---|
| **YAML path** | `llm.provider` |
| **Env var** | `LLM_PROVIDER` |
| **Default** | `OPENAI_COMPAT` |
| **Values** | `OPENAI_COMPAT`, `OPENAI`, `LLAMA_CPP`, `null` |

| Provider | Description |
|---|---|
| `OPENAI_COMPAT` | Any OpenAI-compatible HTTP API (OpenAI, Azure OpenAI, vLLM, etc.) |
| `OPENAI` | OpenAI API directly |
| `LLAMA_CPP` | Local Llama.cpp inference (requires `llama_cpp` feature flag) |
| `null` | No LLM — server starts without model access (useful for smoke tests) |

## llm.base_url

| | |
|---|---|
| **YAML path** | `llm.base_url` |
| **Env var** | `LLM_BASE_URL` |
| **Default** | *(none)* |

Base URL for the LLM API. Required for `OPENAI_COMPAT` and `OPENAI` providers.

```yaml
llm:
  base_url: https://api.openai.com
```

## llm.chat_model

| | |
|---|---|
| **YAML path** | `llm.chat_model` |
| **Env var** | `LLM_CHAT_MODEL` |
| **Default** | *(none)* |

Model identifier for chat completions (e.g. `gpt-5.1`, `claude-4-sonnet`).

## llm.embed_model

| | |
|---|---|
| **YAML path** | `llm.embed_model` |
| **Env var** | `LLM_EMBED_MODEL` |
| **Default** | *(none)* |

Model identifier for embeddings (e.g. `text-embedding-3-small`). Used by the vector store for catalog indexing and KB ingestion.

## llm.max_tokens

| | |
|---|---|
| **YAML path** | `llm.max_tokens` |
| **Env var** | `LLM_MAX_TOKENS` |
| **Default** | `1024` |

Maximum output tokens per LLM call. If you see JSON truncation errors during large batch scaffolds, increase this value. For `gpt-5.x`, `8192` is a reasonable starting point.

## llm.context_length

| | |
|---|---|
| **YAML path** | `llm.context_length` |
| **Env var** | `LLM_CONTEXT_LENGTH` |
| **Default** | `4096` |

Maximum input context length in tokens.

## llm.temperature

| | |
|---|---|
| **YAML path** | `llm.temperature` |
| **Env var** | `LLM_TEMPERATURE` |
| **Default** | *(none)* |

Sampling temperature. Lower values produce more deterministic output. Recommended: `0.2` for data engineering tasks.

## llm.top_p

| | |
|---|---|
| **YAML path** | `llm.top_p` |
| **Env var** | `LLM_TOP_P` |
| **Default** | *(none)* |

Nucleus sampling parameter.

## llm.http_timeout_secs

| | |
|---|---|
| **YAML path** | `llm.http_timeout_secs` |
| **Env var** | `LLM_HTTP_TIMEOUT_SECS` |
| **Default** | `30` |

HTTP request timeout for LLM API calls in seconds. Increase for large batch operations.

## llm.gpu_layers

| | |
|---|---|
| **YAML path** | `llm.gpu_layers` |
| **Env var** | `LLM_GPU_LAYERS` |
| **Default** | *(none)* |

Number of GPU layers to offload (Llama.cpp provider only).

## API key

The LLM API key is always set via environment variable:

```bash
export LLM_API_KEY="sk-..."
```

It is never placed in the YAML config file.

## Example

```yaml
llm:
  provider: OPENAI_COMPAT
  base_url: https://api.openai.com
  chat_model: gpt-5.1
  embed_model: text-embedding-3-small
  context_length: 8192
  http_timeout_secs: 120
  max_tokens: 8192
  temperature: 0.2
  top_p: 1.0
```
