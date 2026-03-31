use async_trait::async_trait;
use duckdb::types::ValueRef;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock, Semaphore};

use react_core::discover::stats::FieldStats;
use react_suite_data_engineer::providers::{
    DatasetCatalogProvider, DatasetFieldStats, DatasetId, DatasetStats, QueryProvider, QueryResult,
    WarehouseNaming,
};
use react_suite_data_engineer::providers::warehouse_utils;

const DEFAULT_MAX_CONCURRENCY: usize = 4;
const MAX_CONCURRENCY_CAP: usize = 8;
const DEFAULT_DISCOVERY_CACHE_TTL_SECS: u64 = 120;

#[derive(Clone, Debug, Default)]
pub struct DuckDbSettings {
    pub connection_string: Option<String>,
    pub motherduck_token: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub max_concurrency: usize,
    pub discovery_cache_ttl_secs: u64,
}

#[derive(Clone)]
pub struct DuckDbProvider {
    inner: Arc<Inner>,
}

struct Inner {
    conn: Mutex<duckdb::Connection>,
    database: String,
    schema: String,
    max_concurrency: usize,
    limiter: Arc<Semaphore>,
    cache_ttl: Duration,
    cache: RwLock<Cache>,
}

#[derive(Default)]
struct Cache {
    tables: Option<(Instant, Vec<DatasetId>)>,
    columns_by_fqn: HashMap<String, (Instant, Vec<(String, String)>)>,
}

impl DuckDbProvider {
    pub fn from_settings(settings: DuckDbSettings) -> Result<Self, String> {
        let conn_str = settings
            .connection_string
            .or_else(|| getenv_nonempty("DUCKDB_CONNECTION_STRING"))
            .or_else(|| getenv_nonempty("DUCKDB_PATH"));

        let md_token = settings
            .motherduck_token
            .or_else(|| getenv_nonempty("MOTHERDUCK_TOKEN"));

        let conn = if let Some(cs) = &conn_str {
            duckdb::Connection::open(cs)
                .map_err(|e| format!("duckdb: failed to open '{}': {}", cs, e))?
        } else {
            duckdb::Connection::open_in_memory()
                .map_err(|e| format!("duckdb: failed to open in-memory: {}", e))?
        };

        if let Some(token) = &md_token {
            conn.execute_batch(&format!("SET motherduck_token='{}'", token.replace('\'', "''")))
                .map_err(|e| format!("duckdb: failed to set motherduck_token: {}", e))?;
        }

        let database = settings
            .database
            .or_else(|| getenv_nonempty("DUCKDB_DATABASE"))
            .unwrap_or_else(|| {
                if conn_str.as_deref().map(|s| s.starts_with("md:")).unwrap_or(false) {
                    "my_db".to_string()
                } else {
                    "memory".to_string()
                }
            });

        let schema = settings
            .schema
            .filter(|s| !s.trim().is_empty())
            .or_else(|| getenv_nonempty("DUCKDB_SCHEMA"))
            .unwrap_or_else(|| "main".to_string());

        let max_concurrency = warehouse_utils::clamp_concurrency(
            if settings.max_concurrency == 0 {
                DEFAULT_MAX_CONCURRENCY
            } else {
                settings.max_concurrency
            },
            MAX_CONCURRENCY_CAP,
        );
        let ttl_secs = warehouse_utils::clamp_cache_ttl_secs(
            if settings.discovery_cache_ttl_secs == 0 {
                DEFAULT_DISCOVERY_CACHE_TTL_SECS
            } else {
                settings.discovery_cache_ttl_secs
            },
        );

        Ok(Self {
            inner: Arc::new(Inner {
                conn: Mutex::new(conn),
                database,
                schema,
                max_concurrency,
                limiter: Arc::new(Semaphore::new(max_concurrency)),
                cache_ttl: Duration::from_secs(ttl_secs),
                cache: RwLock::new(Cache::default()),
            }),
        })
    }

    async fn execute_sql(&self, sql: &str) -> Result<QueryResult, String> {
        let _permit = self
            .inner
            .limiter
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| "duckdb: query limiter closed".to_string())?;

        tracing::info!(target: "duckdb", sql_len = sql.len(), "query_started");

        let inner = self.inner.clone();
        let sql = sql.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = inner.conn.blocking_lock();
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| format!("duckdb: prepare failed: {}", e))?;

            let col_count = stmt.column_count();
            let header: Vec<String> = (0..col_count)
                .map(|i| stmt.column_name(i).map_or("?", |v| v).to_string())
                .collect();

            let rows_iter = stmt
                .query_map([], |row| {
                    let mut out: Vec<String> = Vec::with_capacity(col_count);
                    for i in 0..col_count {
                        let val = row.get_ref(i).unwrap_or(ValueRef::Null);
                        let s = match val {
                            ValueRef::Null => String::new(),
                            ValueRef::Boolean(b) => b.to_string(),
                            ValueRef::TinyInt(n) => n.to_string(),
                            ValueRef::SmallInt(n) => n.to_string(),
                            ValueRef::Int(n) => n.to_string(),
                            ValueRef::BigInt(n) => n.to_string(),
                            ValueRef::HugeInt(n) => n.to_string(),
                            ValueRef::Float(f) => f.to_string(),
                            ValueRef::Double(f) => f.to_string(),
                            ValueRef::Text(bytes) => {
                                String::from_utf8_lossy(bytes).to_string()
                            }
                            ValueRef::Blob(bytes) => {
                                format!("<blob {} bytes>", bytes.len())
                            }
                            _ => String::new(),
                        };
                        out.push(s);
                    }
                    Ok(out)
                })
                .map_err(|e| format!("duckdb: query failed: {}", e))?;

            let mut rows: Vec<Vec<String>> = Vec::new();
            for row_result in rows_iter {
                rows.push(row_result.map_err(|e| format!("duckdb: row read failed: {}", e))?);
            }

            tracing::info!(
                target: "duckdb",
                rows = rows.len(),
                cols = header.len(),
                "query_succeeded"
            );

            Ok(QueryResult {
                header,
                rows,
                meta: Some(serde_json::json!({"engine": "duckdb"})),
            })
        })
        .await
        .map_err(|e| format!("duckdb: spawn_blocking panicked: {}", e))?
    }

    fn quote_ident_ddb(ident: &str) -> String {
        format!("\"{}\"", ident.replace('"', "\"\""))
    }

    async fn cached_tables(&self) -> Result<Vec<DatasetId>, String> {
        {
            let cache = self.inner.cache.read().await;
            if let Some((ts, v)) = cache.tables.as_ref() {
                if ts.elapsed() < self.inner.cache_ttl {
                    return Ok(v.clone());
                }
            }
        }

        let sql = format!(
            "SELECT table_catalog, table_schema, table_name \
             FROM information_schema.tables \
             WHERE table_schema NOT IN ('information_schema', 'pg_catalog') \
             AND table_schema = '{schema}' \
             ORDER BY table_schema, table_name",
            schema = self.inner.schema.replace('\'', "''"),
        );
        let qr = self.execute_sql(&sql).await?;
        let mut out: Vec<DatasetId> = Vec::new();
        for row in &qr.rows {
            if row.len() >= 3 && !row[2].trim().is_empty() {
                out.push(DatasetId {
                    catalog: row[0].clone(),
                    database: row[1].clone(),
                    table: row[2].clone(),
                });
            }
        }

        let mut cache = self.inner.cache.write().await;
        cache.tables = Some((Instant::now(), out.clone()));
        Ok(out)
    }

    async fn cached_columns(&self, dataset: &DatasetId) -> Result<Vec<(String, String)>, String> {
        let fqn = dataset.fqn();
        {
            let cache = self.inner.cache.read().await;
            if let Some((ts, cols)) = cache.columns_by_fqn.get(&fqn) {
                if ts.elapsed() < self.inner.cache_ttl {
                    return Ok(cols.clone());
                }
            }
        }

        let sql = format!(
            "SELECT column_name, data_type \
             FROM information_schema.columns \
             WHERE table_schema = '{schema}' AND table_name = '{table}' \
             ORDER BY ordinal_position",
            schema = dataset.database.replace('\'', "''"),
            table = dataset.table.replace('\'', "''"),
        );
        let qr = self.execute_sql(&sql).await?;
        let cols: Vec<(String, String)> = qr
            .rows
            .iter()
            .filter_map(|r| {
                if r.len() >= 2 && !r[0].trim().is_empty() {
                    Some((r[0].clone(), r[1].clone()))
                } else {
                    None
                }
            })
            .collect();

        let mut cache = self.inner.cache.write().await;
        cache.columns_by_fqn.insert(fqn, (Instant::now(), cols.clone()));
        Ok(cols)
    }
}

impl WarehouseNaming for DuckDbProvider {
    fn kind(&self) -> react_suite_data_engineer::de_config::WarehouseKind {
        react_suite_data_engineer::de_config::WarehouseKind::Duckdb
    }

    fn parse_dataset_fqn(&self, dataset_fqn: &str) -> Result<DatasetId, String> {
        warehouse_utils::parse_fqn_common(
            dataset_fqn,
            &self.inner.database,
            Some(&self.inner.schema),
        )
    }

    fn quote_ident(&self, ident: &str) -> String {
        Self::quote_ident_ddb(ident)
    }

    fn quote_fqn(&self, id: &DatasetId) -> String {
        format!(
            "{}.{}",
            Self::quote_ident_ddb(&id.database),
            Self::quote_ident_ddb(&id.table)
        )
    }

    fn sql_prompt_rules(&self) -> Vec<&'static str> {
        vec![
            "DuckDB SQL: PostgreSQL-compatible dialect with extensions.",
            "DuckDB SQL: Use double-quote quoting for identifiers.",
            "DuckDB SQL: LIMIT N for row limiting.",
            "DuckDB SQL: Supports list, struct, map, and union types natively.",
            "DuckDB SQL: Use EPOCH(col) or EXTRACT(EPOCH FROM col) for Unix timestamps.",
            "DuckDB SQL: TRY_CAST(expr AS type) for safe conversions.",
            "DuckDB SQL: information_schema.tables and information_schema.columns available.",
        ]
    }

    fn sql_remediation_rules(&self) -> Vec<&'static str> {
        vec![
            "DuckDB rule: No TOP N syntax. Use LIMIT N.",
            "DuckDB rule: GETDATE() and SYSDATE are not supported. Use CURRENT_TIMESTAMP.",
            "DuckDB rule: NVL() is not supported. Use COALESCE().",
        ]
    }
}

#[async_trait]
impl QueryProvider for DuckDbProvider {
    async fn query(&self, sql: &str) -> Result<QueryResult, String> {
        self.execute_sql(sql).await
    }

    async fn schema(&self, dataset_fqn: &str) -> Result<Vec<(String, String)>, String> {
        let ds = self.parse_dataset_fqn(dataset_fqn)?;
        self.cached_columns(&ds).await
    }

    async fn sample(&self, dataset_fqn: &str, limit: usize) -> Result<Vec<Vec<String>>, String> {
        let ds = self.parse_dataset_fqn(dataset_fqn)?;
        let lim = limit.max(1).min(5000);
        let sql = format!("SELECT * FROM {} LIMIT {}", self.quote_fqn(&ds), lim);
        let qr = self.query(&sql).await?;
        Ok(qr.rows)
    }

    fn max_concurrency(&self) -> usize {
        self.inner.max_concurrency
    }
}

#[async_trait]
impl DatasetCatalogProvider for DuckDbProvider {
    async fn list_datasets(&self) -> Result<Vec<DatasetId>, String> {
        self.cached_tables().await
    }

    async fn get_dataset_schema(
        &self,
        dataset: &DatasetId,
    ) -> Result<Vec<(String, String)>, String> {
        self.cached_columns(dataset).await
    }

    async fn get_dataset_stats(
        &self,
        dataset: &DatasetId,
        max_fields: usize,
    ) -> Result<(DatasetFieldStats, DatasetStats), String> {
        let cols = self.cached_columns(dataset).await?;
        let max_fields = max_fields.max(1).min(500);
        let tbl = self.quote_fqn(dataset);

        let count_sql = format!("SELECT COUNT(1) AS __cnt FROM {}", tbl);
        let qr_cnt = self.query(&count_sql).await?;
        let total_rows: u64 = qr_cnt
            .rows
            .first()
            .and_then(|r| r.first())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let mut ns_stats = DatasetFieldStats::new(&dataset.fqn());

        for (name, ty) in cols.into_iter().take(max_fields) {
            let expr = Self::quote_ident_ddb(&name);
            let ty_lc = ty.trim().to_lowercase();
            let is_complex = ty_lc.starts_with("struct")
                || ty_lc.starts_with("list")
                || ty_lc.starts_with("map")
                || ty_lc.starts_with("union");
            let is_timestamp = ty_lc.contains("timestamp");
            let is_date = ty_lc == "date";

            let min_expr = if is_timestamp || is_date {
                format!("EXTRACT(EPOCH FROM {})", expr)
            } else {
                format!("TRY_CAST({} AS DOUBLE)", expr)
            };
            let max_expr = min_expr.clone();

            let sql = if is_complex {
                format!(
                    "SELECT \
                        COUNT(1) AS __rows, \
                        SUM(CASE WHEN {c} IS NULL THEN 1 ELSE 0 END) AS __nulls, \
                        NULL AS __distinct, \
                        NULL AS __min_num, \
                        NULL AS __max_num \
                     FROM {tbl}",
                    c = expr, tbl = tbl,
                )
            } else {
                format!(
                    "SELECT \
                        COUNT(1) AS __rows, \
                        SUM(CASE WHEN {c} IS NULL THEN 1 ELSE 0 END) AS __nulls, \
                        COUNT(DISTINCT {c}) AS __distinct, \
                        MIN({min_e}) AS __min_num, \
                        MAX({max_e}) AS __max_num \
                     FROM {tbl}",
                    c = expr, min_e = min_expr, max_e = max_expr, tbl = tbl,
                )
            };

            let qr = match self.query(&sql).await {
                Ok(qr) => qr,
                Err(e) => {
                    tracing::warn!(
                        "duckdb stats: dataset='{}' field='{}' type='{}' failed: {}",
                        dataset.fqn(), name, ty, e
                    );
                    let mut fs = FieldStats::default();
                    fs.total = total_rows;
                    fs.finalize();
                    ns_stats.fields.insert(name, fs);
                    continue;
                }
            };

            let row = qr.rows.first().cloned().unwrap_or_default();
            let mut fs = FieldStats::default();
            fs.total = total_rows;
            fs.nulls = row.get(1).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            if !is_complex {
                fs.approx_distinct = row.get(2).and_then(|s| s.parse::<u64>().ok());
                fs.min_numeric = row.get(3).and_then(|s| s.parse::<f64>().ok());
                fs.max_numeric = row.get(4).and_then(|s| s.parse::<f64>().ok());
            }
            fs.finalize();
            ns_stats.fields.insert(name, fs);
        }

        let mut ds_stats = DatasetStats::default();
        ds_stats.approx_total_rows = total_rows;
        Ok((ns_stats, ds_stats))
    }

    fn max_concurrency(&self) -> usize {
        self.inner.max_concurrency
    }
}

fn getenv_nonempty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .and_then(|v| if v.trim().is_empty() { None } else { Some(v) })
}
