# Custom Provider

Implement a provider trait to add a new warehouse backend, storage adapter, or vector store.

## Provider traits

All traits are defined in `src/core/src/providers/`:

| Trait | Purpose |
|---|---|
| `QueryProvider` | SQL execution, schema inspection, data sampling |
| `DatasetCatalogProvider` | Dataset/table discovery |
| `WarehouseProvider` | Warehouse naming and metadata |
| `VectorStore` | Embedding upsert and similarity query |
| `DbtProvider` | dbt project lifecycle |
| `StorageAdapter` | Key-value persistence |

## Example: QueryProvider

Implement `QueryProvider` to add a new SQL-capable backend:

```rust
use async_trait::async_trait;
use react_core::providers::{QueryProvider, QueryResult};

pub struct MyWarehouseProvider {
    // connection config, pool, etc.
}

#[async_trait]
impl QueryProvider for MyWarehouseProvider {
    async fn execute(&self, sql: &str) -> Result<QueryResult, String> {
        // Execute SQL against your warehouse
        // Return columnar results
    }

    async fn schema(&self, table: &str) -> Result<QueryResult, String> {
        // Return column names and types for a table
    }

    async fn sample(&self, table: &str, limit: usize) -> Result<QueryResult, String> {
        // Return a sample of rows
    }
}
```

## Example: StorageAdapter

Implement `StorageAdapter` for a new persistence backend:

```rust
use async_trait::async_trait;
use serde_json::Value;
use react_core::storage::StorageAdapter;

pub struct MyStorageAdapter {
    // connection config
}

#[async_trait]
impl StorageAdapter for MyStorageAdapter {
    async fn get_json(&self, key: &str) -> Result<Value, String> { /* ... */ }
    async fn put_json(&self, key: &str, value: &Value) -> Result<(), String> { /* ... */ }
    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, String> { /* ... */ }
    async fn put_bytes(&self, key: &str, bytes: &[u8], content_type: &str)
        -> Result<(), String> { /* ... */ }
    async fn delete_object(&self, key: &str) -> Result<(), String> { /* ... */ }
    async fn head_etag(&self, key: &str) -> Result<Option<String>, String> { /* ... */ }
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>, String> { /* ... */ }
}
```

## Wiring into the runtime

After implementing a provider, wire it into the config resolution and provider initialisation in `src/runtime/src/main.rs`:

1. Add a new variant to `WarehouseFile` in `src/runtime/src/config.rs` (if it's a warehouse provider)
2. Add the corresponding resolution logic in `resolve_warehouse`
3. Construct the provider in `main.rs` based on the resolved config
4. Pass it into the `SuiteCtx`

The suite and agent loop don't need changes — they work against the trait interface.

## Module structure

Create a new module under `src/modules/`:

```
src/modules/provider-mywarehouse/
├── Cargo.toml
├── src/
│   └── lib.rs
```

Add it to the workspace in the root `Cargo.toml`:

```toml
[workspace]
members = [
  # ...existing members...
  "src/modules/provider-mywarehouse",
]
```

## Next steps

- [Providers concept](../concepts/providers.md) — how providers fit into the architecture
- [Custom Suite](custom-suite.md) — building suites that use custom providers
