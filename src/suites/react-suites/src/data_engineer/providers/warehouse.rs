use async_trait::async_trait;

use super::dataset_catalog::{DatasetCatalogProvider, DatasetId};
use super::query::{QueryProvider, QueryResult};
use super::catalog_types::DatasetStats;
use super::stats::DatasetFieldStats;

pub trait WarehouseNaming: Send + Sync {
    fn kind(&self) -> crate::data_engineer::de_config::WarehouseKind;

    fn parse_dataset_fqn(&self, dataset_fqn: &str) -> Result<DatasetId, String>;

    fn format_dataset_fqn(&self, id: &DatasetId) -> String {
        id.fqn()
    }

    fn quote_ident(&self, ident: &str) -> String;

    fn quote_fqn(&self, id: &DatasetId) -> String {
        format!(
            "{}.{}.{}",
            self.quote_ident(&id.catalog),
            self.quote_ident(&id.database),
            self.quote_ident(&id.table)
        )
    }

    fn sql_prompt_rules(&self) -> Vec<&'static str> {
        vec![]
    }

    fn sql_remediation_rules(&self) -> Vec<&'static str> {
        vec![]
    }

    fn unsupported_sql_reason(&self, _sql: &str) -> Option<String> {
        None
    }
}

pub fn has_obvious_same_select_alias_reuse(sql: &str) -> bool {
    let lower = sql.to_ascii_lowercase();
    let Some(select_pos) = lower.find("select") else {
        return false;
    };
    let from_search_start = select_pos + "select".len();
    let Some(from_rel) = lower[from_search_start..].find(" from ") else {
        return false;
    };
    let select_end = from_search_start + from_rel;
    let select_list = &lower[from_search_start..select_end];
    let mut idx = 0usize;
    while let Some(as_rel) = select_list[idx..].find(" as ") {
        let as_pos = idx + as_rel;
        let alias_start = as_pos + 4;
        let Some((alias_name, consumed)) = parse_alias_token(&select_list[alias_start..]) else {
            idx = alias_start;
            continue;
        };
        if alias_name.is_empty() {
            idx = alias_start + consumed;
            continue;
        }
        let rest = &select_list[alias_start + consumed..];
        if contains_identifier_reference(rest, &alias_name) {
            return true;
        }
        idx = alias_start + consumed;
    }
    false
}

fn parse_alias_token(s: &str) -> Option<(String, usize)> {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let q = bytes[i];
    if q == b'`' || q == b'"' {
        let mut j = i + 1;
        while j < bytes.len() && bytes[j] != q {
            j += 1;
        }
        if j >= bytes.len() {
            return None;
        }
        return Some((s[i + 1..j].to_string(), j + 1));
    }
    let start = i;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    if i == start {
        return None;
    }
    Some((s[start..i].to_string(), i))
}

fn contains_identifier_reference(haystack: &str, ident: &str) -> bool {
    if ident.trim().is_empty() {
        return false;
    }
    if haystack.contains(&format!("`{}`", ident)) || haystack.contains(&format!("\"{}\"", ident)) {
        return true;
    }
    let mut start = 0usize;
    while let Some(rel) = haystack[start..].find(ident) {
        let pos = start + rel;
        let end_pos = pos + ident.len();
        let prev = if pos == 0 {
            None
        } else {
            haystack.as_bytes().get(pos - 1).copied()
        };
        let next = haystack.as_bytes().get(end_pos).copied();
        let prev_is_ident = prev
            .map(|b| b.is_ascii_alphanumeric() || b == b'_')
            .unwrap_or(false);
        let next_is_ident = next
            .map(|b| b.is_ascii_alphanumeric() || b == b'_')
            .unwrap_or(false);
        if !prev_is_ident && !next_is_ident {
            return true;
        }
        start = end_pos;
    }
    false
}

pub trait WarehouseProvider: QueryProvider + DatasetCatalogProvider + WarehouseNaming {}
impl<T> WarehouseProvider for T where T: QueryProvider + DatasetCatalogProvider + WarehouseNaming {}

#[derive(Clone, Default)]
pub struct NullWarehouseProvider;

#[async_trait]
impl QueryProvider for NullWarehouseProvider {
    async fn query(&self, _sql: &str) -> Result<QueryResult, String> {
        Err("warehouse provider not configured".to_string())
    }
    async fn schema(&self, _dataset_fqn: &str) -> Result<Vec<(String, String)>, String> {
        Err("warehouse provider not configured".to_string())
    }
    async fn sample(&self, _dataset_fqn: &str, _limit: usize) -> Result<Vec<Vec<String>>, String> {
        Err("warehouse provider not configured".to_string())
    }
}

#[async_trait]
impl DatasetCatalogProvider for NullWarehouseProvider {
    async fn list_datasets(&self) -> Result<Vec<DatasetId>, String> {
        Err("warehouse provider not configured".to_string())
    }
    async fn get_dataset_schema(
        &self,
        _dataset: &DatasetId,
    ) -> Result<Vec<(String, String)>, String> {
        Err("warehouse provider not configured".to_string())
    }
    async fn get_dataset_stats(
        &self,
        _dataset: &DatasetId,
        _max_fields: usize,
    ) -> Result<(DatasetFieldStats, DatasetStats), String> {
        Err("warehouse provider not configured".to_string())
    }
}

impl WarehouseNaming for NullWarehouseProvider {
    fn kind(&self) -> crate::data_engineer::de_config::WarehouseKind {
        crate::data_engineer::de_config::WarehouseKind::default()
    }

    fn parse_dataset_fqn(&self, _dataset_fqn: &str) -> Result<DatasetId, String> {
        Err("warehouse provider not configured".to_string())
    }

    fn quote_ident(&self, ident: &str) -> String {
        format!("\"{}\"", ident.replace('"', "\"\""))
    }
}
