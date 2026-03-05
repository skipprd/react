# LanceDB

The LanceDB provider implements the `VectorStore` trait for semantic search. It uses [LanceDB](https://lancedb.github.io/lancedb/) (Arrow-based vector database) for embedding storage and similarity queries.

## Configuration

Enable the vector store in your config:

```yaml
providers:
  vector:
    enabled: true

llm:
  embed_model: text-embedding-3-small
```

The `embed_model` is required for generating embeddings during upsert and query operations.

## Storage location

The LanceDB data directory is determined by the storage mode and keyspace:

| Storage mode | LanceDB path |
|---|---|
| `local` | `{storage.path}/{scope}/lancedb` |
| `s3` | `s3://{bucket}/{scope}/lancedb` |

Example (local): `./.react/acme/prod/analytics/lancedb/`

## Operations

| Operation | Description |
|---|---|
| **Upsert** | Chunk content, generate embeddings via the configured embed model, and insert/update vectors in LanceDB |
| **Query** | Generate an embedding for the query text, perform ANN (approximate nearest neighbor) search, return top-k scored chunks |

## Usage

The vector store is used by:

- **Catalog system** — indexes table and column metadata for semantic search during preflight
- **KB suite** — ingests documents and answers questions via retrieval-augmented generation
- **Data Engineer suite** — retrieves relevant catalog context during plan grounding and model authoring

## Architecture

The module is split into two sub-modules:

| Module | Purpose |
|---|---|
| `global_lance_store` | Global singleton for shared LanceDB connection management |
| `lance_store` | Core implementation of `VectorStore` trait against LanceDB |

## Dependencies

LanceDB requires `protoc` (Protocol Buffers compiler) at build time. See [Installation](../../getting-started/install.md) for setup instructions.
