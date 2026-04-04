use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Chunk {
    pub id: String,
    pub namespace: String,
    pub text: String,
    pub vector: Vec<f32>,
    pub metadata_json: String,
    pub epoch: u64,
}

#[derive(Clone, Debug)]
pub struct ScoredChunk {
    pub item: Chunk,
    pub score: f32,
}

pub struct LanceDbStore {
    uri: String,
    storage_options: Vec<(String, String)>,
}

const EMBEDDINGS_TABLE_V2: &str = "embeddings_v2";
const EMBEDDINGS_TABLE_V1: &str = "embeddings";

impl LanceDbStore {
    pub fn new(uri: &str) -> Self {
        Self {
            uri: uri.to_string(),
            storage_options: Vec::new(),
        }
    }

    pub fn with_storage_options(mut self, opts: Vec<(String, String)>) -> Self {
        self.storage_options = opts;
        self
    }

    fn escape_sql_literal(raw: &str) -> String {
        raw.replace('\'', "''")
    }

    fn connect_builder(&self) -> lancedb::connection::ConnectBuilder {
        let mut builder = lancedb::connect(&self.uri);
        for (k, v) in &self.storage_options {
            builder = builder.storage_option(k.clone(), v.clone());
        }
        builder
    }

    pub async fn upsert(&self, items: &[Chunk]) -> Result<(), String> {
        use arrow::array::{Float32Array, StringArray, UInt64Array};
        use arrow::datatypes::{DataType, Field, Schema};
        use arrow::record_batch::RecordBatch;
        use std::sync::Arc;
        if items.is_empty() {
            return Ok(());
        }
        let ids = StringArray::from(items.iter().map(|c| c.id.clone()).collect::<Vec<_>>());
        let namespaces = StringArray::from(
            items
                .iter()
                .map(|c| c.namespace.clone())
                .collect::<Vec<_>>(),
        );
        let metadata_json = StringArray::from(
            items
                .iter()
                .map(|c| c.metadata_json.clone())
                .collect::<Vec<_>>(),
        );
        let texts = StringArray::from(items.iter().map(|c| c.text.clone()).collect::<Vec<_>>());
        let epochs = UInt64Array::from(items.iter().map(|c| c.epoch).collect::<Vec<_>>());
        let dims = items.first().map(|c| c.vector.len()).unwrap_or(0);
        if dims == 0 {
            return Err("empty vectors".to_string());
        }
        let mut flat: Vec<f32> = Vec::with_capacity(items.len() * dims);
        for c in items {
            if c.vector.len() != dims {
                return Err("inconsistent vector dims".to_string());
            }
            flat.extend_from_slice(&c.vector);
        }
        let vectors = Float32Array::from(flat);
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("namespace", DataType::Utf8, false),
            Field::new("text", DataType::Utf8, false),
            Field::new("metadata_json", DataType::Utf8, false),
            Field::new("epoch", DataType::UInt64, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, false)),
                    dims as i32,
                ),
                false,
            ),
        ]));
        use arrow::array::{ArrayRef, FixedSizeListArray};
        use arrow_buffer::NullBuffer;
        let values: ArrayRef = Arc::new(vectors);
        let list_field = Arc::new(Field::new("item", DataType::Float32, false));
        let fsl = FixedSizeListArray::try_new(
            list_field,
            dims as i32,
            values,
            None as Option<NullBuffer>,
        )
        .map_err(|e| e.to_string())?;
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(ids),
                Arc::new(namespaces),
                Arc::new(texts),
                Arc::new(metadata_json),
                Arc::new(epochs),
                Arc::new(fsl),
            ],
        )
        .map_err(|e| e.to_string())?;
        let db = self.connect_builder()
            .execute()
            .await
            .map_err(|e| format!("{:?}", e))?;
        use arrow::error::Result as ArrowResult;
        use arrow::record_batch::RecordBatchIterator;
        let rb_iter = RecordBatchIterator::new(
            vec![ArrowResult::Ok(batch.clone())].into_iter(),
            batch.schema(),
        );
        let tbl = match db.open_table(EMBEDDINGS_TABLE_V2).execute().await {
            Ok(t) => t,
            Err(_) => db
                .create_table(EMBEDDINGS_TABLE_V2, rb_iter)
                .execute()
                .await
                .map_err(|e| format!("{:?}", e))?,
        };
        let add_iter = RecordBatchIterator::new(
            vec![ArrowResult::Ok(batch.clone())].into_iter(),
            batch.schema(),
        );
        let _ = tbl
            .add(add_iter)
            .execute()
            .await
            .map_err(|e| format!("{:?}", e))?;
        Ok(())
    }

    async fn query_v2(
        &self,
        query_vec: &[f32],
        k: usize,
        namespace: Option<&str>,
    ) -> Result<Vec<ScoredChunk>, String> {
        use futures::StreamExt;
        use lancedb::query::{ExecutableQuery, QueryBase};

        let db = self.connect_builder()
            .execute()
            .await
            .map_err(|e| format!("{:?}", e))?;
        let tbl = match db.open_table(EMBEDDINGS_TABLE_V2).execute().await {
            Ok(t) => t,
            Err(_) => return Ok(Vec::new()),
        };
        let q = tbl
            .vector_search(query_vec.to_vec())
            .map_err(|e| format!("{:?}", e))?
            .limit(k);
        let mut stream = q.execute().await.map_err(|e| format!("{:?}", e))?;
        use arrow::record_batch::RecordBatch as ArrowRecordBatch;
        let mut out = Vec::new();
        while let Some(batch_res) = stream.next().await {
            let b: ArrowRecordBatch = batch_res.map_err(|e| format!("{:?}", e))?;
            let schema = b.schema();
            let idx = |name: &str| schema.index_of(name).ok();
            let id_i = idx("id");
            let namespace_i = idx("namespace");
            let text_i = idx("text");
            let metadata_json_i = idx("metadata_json");
            let epoch_i = idx("epoch");
            let dist_i = idx("_distance");
            for r in 0..b.num_rows() {
                let s_val = |i: Option<usize>| -> String {
                    i.and_then(|j| {
                        arrow::util::display::array_value_to_string(b.column(j).as_ref(), r).ok()
                    })
                    .unwrap_or_default()
                };
                let namespace_s = s_val(namespace_i);
                if let Some(ns) = namespace {
                    if ns != namespace_s {
                        continue;
                    }
                }
                out.push(ScoredChunk {
                    item: Chunk {
                        id: s_val(id_i),
                        namespace: namespace_s,
                        text: s_val(text_i),
                        metadata_json: s_val(metadata_json_i),
                        vector: Vec::new(),
                        epoch: s_val(epoch_i).parse::<u64>().unwrap_or(0),
                    },
                    score: s_val(dist_i).parse::<f32>().unwrap_or(0.0),
                });
            }
        }
        Ok(out)
    }

    async fn query_v1_legacy(
        &self,
        query_vec: &[f32],
        k: usize,
        namespace: Option<&str>,
    ) -> Result<Vec<ScoredChunk>, String> {
        use futures::StreamExt;
        use lancedb::query::{ExecutableQuery, QueryBase};

        let db = self.connect_builder()
            .execute()
            .await
            .map_err(|e| format!("{:?}", e))?;
        let tbl = match db.open_table(EMBEDDINGS_TABLE_V1).execute().await {
            Ok(t) => t,
            Err(_) => return Ok(Vec::new()),
        };
        let q = tbl
            .vector_search(query_vec.to_vec())
            .map_err(|e| format!("{:?}", e))?
            .limit(k);
        let mut stream = q.execute().await.map_err(|e| format!("{:?}", e))?;
        use arrow::record_batch::RecordBatch as ArrowRecordBatch;
        let mut out = Vec::new();
        while let Some(batch_res) = stream.next().await {
            let b: ArrowRecordBatch = batch_res.map_err(|e| format!("{:?}", e))?;
            let schema = b.schema();
            let idx = |name: &str| schema.index_of(name).ok();
            let id_i = idx("id");
            let kind_i = idx("kind");
            let dataset_i = idx("dataset_id");
            let field_i = idx("field");
            let text_i = idx("text");
            let epoch_i = idx("epoch");
            let dist_i = idx("_distance");
            for r in 0..b.num_rows() {
                let s_val = |i: Option<usize>| -> String {
                    i.and_then(|j| {
                        arrow::util::display::array_value_to_string(b.column(j).as_ref(), r).ok()
                    })
                    .unwrap_or_default()
                };
                let namespace_s = s_val(kind_i);
                if let Some(ns) = namespace {
                    if ns != namespace_s {
                        continue;
                    }
                }
                let dataset_id = s_val(dataset_i);
                let field = s_val(field_i);
                let metadata_json = serde_json::json!({
                    "dataset_id": if dataset_id.is_empty() { None::<String> } else { Some(dataset_id) },
                    "field": if field.is_empty() { None::<String> } else { Some(field) }
                })
                .to_string();
                out.push(ScoredChunk {
                    item: Chunk {
                        id: s_val(id_i),
                        namespace: namespace_s,
                        text: s_val(text_i),
                        metadata_json,
                        vector: Vec::new(),
                        epoch: s_val(epoch_i).parse::<u64>().unwrap_or(0),
                    },
                    score: s_val(dist_i).parse::<f32>().unwrap_or(0.0),
                });
            }
        }
        Ok(out)
    }

    pub async fn query(
        &self,
        query_vec: &[f32],
        k: usize,
        namespace: Option<&str>,
    ) -> Result<Vec<ScoredChunk>, String> {
        let mut out = self.query_v2(query_vec, k, namespace).await?;
        out.extend(self.query_v1_legacy(query_vec, k, namespace).await?);
        out.sort_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(out)
    }

    fn delete_predicate(&self, column: &str, value: &str) -> String {
        format!("{column} = '{}'", Self::escape_sql_literal(value))
    }

    fn like_prefix_predicate(&self, column: &str, prefix: &str) -> String {
        format!("{column} LIKE '{}%'", Self::escape_sql_literal(prefix))
    }

    async fn delete_where(&self, table_name: &str, predicate: &str) -> Result<(), String> {
        let db = self.connect_builder()
            .execute()
            .await
            .map_err(|e| format!("{:?}", e))?;
        let tbl = match db.open_table(table_name).execute().await {
            Ok(t) => t,
            Err(_) => return Ok(()),
        };
        match tbl.delete(predicate).await {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("{:?}", e)),
        }
    }

    pub async fn delete_thread_embeddings(&self, thread_id: &str) -> Result<(), String> {
        let pred = format!("id LIKE '%:{}:%'", Self::escape_sql_literal(thread_id));
        self.delete_where(EMBEDDINGS_TABLE_V2, pred.as_str()).await?;
        self.delete_where(EMBEDDINGS_TABLE_V1, pred.as_str()).await
    }

    pub async fn delete_pipeline_embeddings(&self) -> Result<(), String> {
        self.delete_where(EMBEDDINGS_TABLE_V2, "id IS NOT NULL").await?;
        self.delete_where(EMBEDDINGS_TABLE_V1, "id IS NOT NULL").await
    }

    pub async fn delete_namespace(&self, namespace: &str) -> Result<(), String> {
        let pred_v2 = self.delete_predicate("namespace", namespace);
        let pred_v1 = self.delete_predicate("kind", namespace);
        self.delete_where(EMBEDDINGS_TABLE_V2, pred_v2.as_str()).await?;
        self.delete_where(EMBEDDINGS_TABLE_V1, pred_v1.as_str()).await
    }

    pub async fn delete_ids_with_prefix(&self, prefix: &str) -> Result<(), String> {
        let pred = self.like_prefix_predicate("id", prefix);
        self.delete_where(EMBEDDINGS_TABLE_V2, pred.as_str()).await?;
        self.delete_where(EMBEDDINGS_TABLE_V1, pred.as_str()).await
    }
}
