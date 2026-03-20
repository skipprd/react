# Providers

Providers are trait-based capabilities injected into suites. They abstract infrastructure concerns (which warehouse, which storage backend, which vector store) so suites and the agent loop remain provider-agnostic.

## Provider traits

All provider traits are defined in `src/core/src/providers/`:

| Trait | Module | Purpose |
|---|---|---|
| `QueryProvider` | `providers::query` | Execute SQL, fetch schema, sample rows |
| `DatasetCatalogProvider` | `providers::dataset_catalog_provider` | Discover available datasets/tables |
| `CatalogProvider` | `providers::catalog` | Semantic catalog metadata (column descriptions, tags) |
| `VectorStore` | `providers::vector` | Embeddings upsert and similarity search |
| `DbtProvider` | `providers::dbt` | dbt project scaffolding, validation, build, and publish |
| `StorageAdapter` | `storage` | Key-value persistence (get/put JSON, bytes, list, delete) |
| `WarehouseProvider` | `providers::warehouse` | Warehouse naming conventions and metadata |
| `SecretsProvider` | `providers::secrets` | Secret retrieval (API keys, credentials) |
| `StateStore` | `providers::state` | Thread-level state persistence |

## QueryProvider

The primary data access trait. Implementations exist for Athena, BigQuery, Postgres, Snowflake, and Microsoft SQL Server.

```rust
#[async_trait]
pub trait QueryProvider: Send + Sync {
    async fn execute(&self, sql: &str) -> Result<QueryResult, String>;
    async fn schema(&self, table: &str) -> Result<QueryResult, String>;
    async fn sample(&self, table: &str, limit: usize) -> Result<QueryResult, String>;
}
```

Suites use this trait to run SQL, inspect table schemas, and sample data — without knowing which warehouse is behind it.

## StorageAdapter

The persistence layer. Implementations: `LocalFileStorageAdapter` (writes to disk) and `S3StorageAdapter` (writes to S3).

```rust
#[async_trait]
pub trait StorageAdapter: Send + Sync {
    async fn get_json(&self, key: &str) -> Result<Value, String>;
    async fn put_json(&self, key: &str, value: &Value) -> Result<(), String>;
    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, String>;
    async fn put_bytes(&self, key: &str, bytes: &[u8], content_type: &str) -> Result<(), String>;
    async fn delete_object(&self, key: &str) -> Result<(), String>;
    async fn head_etag(&self, key: &str) -> Result<Option<String>, String>;
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>, String>;
}
```

## VectorStore

LanceDB-backed vector storage for semantic search. Used by the catalog system and the KB suite.

```rust
#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn upsert(&self, chunks: Vec<VectorChunk>) -> Result<(), String>;
    async fn query(&self, query: &str, top_k: usize) -> Result<Vec<ScoredVectorChunk>, String>;
}
```

## DbtProvider

Manages the dbt project lifecycle. The default implementation shells out to the `dbt` CLI (host mode) or runs dbt inside a Docker container.

## Injection

Providers are wired together at startup in `src/runtime/src/main.rs` based on the resolved config:

1. The config specifies `providers.warehouse.kind` (athena, bigquery, postgres, etc.)
2. The runtime constructs the appropriate `QueryProvider` implementation
3. All providers are bundled into a `SuiteCtx` and passed to the suite registry
4. Suites receive the `SuiteCtx` and wire the providers into the `AgentCtx` for the loop

This means you can swap warehouse backends by changing a single YAML field without touching suite or tool code.

## Next steps

- [Connectors: Athena](../connectors/warehouses/athena.md) — Athena provider configuration
- [Connectors: BigQuery](../connectors/warehouses/bigquery.md) — BigQuery provider configuration
- [Connectors: Postgres](../connectors/warehouses/postgres.md) — Postgres provider configuration
- [Connectors: Snowflake](../connectors/warehouses/snowflake.md) — Snowflake provider configuration
- [Connectors: MSSQL](../connectors/warehouses/mssql.md) — Microsoft SQL Server provider configuration
- [Extending: Custom Provider](../extending/custom-provider.md) — implementing your own provider
