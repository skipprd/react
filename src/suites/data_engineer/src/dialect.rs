use react_core::agent::AgentCtx;
use react_core::resolved_config::ReactResolvedConfig;

/// Returns a human-readable SQL dialect description based on the warehouse provider.
pub fn active_provider_dialect(cfg: &ReactResolvedConfig) -> String {
    use crate::de_config::WarehouseKind;
    let providers = crate::de_config::de_config_from_resolved(cfg);
    let kind = providers
        .as_ref()
        .map(|p| p.warehouse.kind.clone())
        .unwrap_or(WarehouseKind::Athena);
    match kind {
        WarehouseKind::Athena => "Amazon Athena (engine v3 / Trino SQL)".to_string(),
        WarehouseKind::Postgres => "PostgreSQL".to_string(),
        WarehouseKind::Mssql => "Microsoft SQL Server (T-SQL)".to_string(),
        WarehouseKind::Snowflake => "Snowflake SQL".to_string(),
        WarehouseKind::Bigquery => "Google BigQuery (Standard SQL)".to_string(),
    }
}

/// Lists all `.sql` keys under the dbt scope, excluding target and version directories.
pub async fn list_sql_keys_for_scope(ctx: &AgentCtx) -> Result<Vec<String>, String> {
    let base = ctx
        .keyspace()
        .scoped_prefix(ctx.scope(), &["dbt"])
        .trim_end_matches('/')
        .to_string()
        + "/";
    let keys = ctx.storage().list_prefix(&base).await.unwrap_or_default();
    let mut out: Vec<String> = Vec::new();
    for k in keys {
        if !k.ends_with(".sql") {
            continue;
        }
        if k.contains("/target/") || k.contains("/_versions/") {
            continue;
        }
        out.push(k);
    }
    out.sort();
    Ok(out)
}
