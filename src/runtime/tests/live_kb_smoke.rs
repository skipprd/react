//! Live KB-suite smokes through react-core with a real LLM.
//! Skips when `LLM_API_KEY` / `OPENAI_API_KEY` is unset.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use react::config::{resolve_config, LlmFile, ReactConfigFile, ServeOverrides};
use react::execute::{execute_request, ExecuteAction, ExecuteRequest};
use react_core::provider_traits::{ScoredVectorRecord, StoredVectorRecord, VectorStore};
use react_core::scope::RequestScope;
use react_core::suite::{SuiteCtx, SuiteRegistry};
use react_http_protocol::FlowFrame;
use react_suite_kb::KbSuite;

fn llm_ready() -> bool {
    ["LLM_API_KEY", "OPENAI_API_KEY"]
        .iter()
        .any(|k| std::env::var(k).ok().is_some_and(|v| !v.trim().is_empty()))
}

#[derive(Default)]
struct MemoryVectorStore {
    rows: Mutex<Vec<StoredVectorRecord>>,
}

fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || a.len() != b.len() {
        return 2.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    let denom = (na.sqrt() * nb.sqrt()).max(1e-9);
    1.0 - (dot / denom)
}

#[async_trait]
impl VectorStore for MemoryVectorStore {
    async fn upsert(
        &self,
        _scope: &RequestScope,
        items: &[StoredVectorRecord],
    ) -> Result<(), String> {
        let mut rows = self.rows.lock().map_err(|e| e.to_string())?;
        for item in items {
            if let Some(existing) = rows.iter_mut().find(|row| row.id == item.id) {
                *existing = item.clone();
            } else {
                rows.push(item.clone());
            }
        }
        Ok(())
    }

    async fn query(
        &self,
        _scope: &RequestScope,
        query_vec: &[f32],
        k: usize,
        namespace: Option<&str>,
    ) -> Result<Vec<ScoredVectorRecord>, String> {
        let rows = self.rows.lock().map_err(|e| e.to_string())?;
        let mut hits: Vec<ScoredVectorRecord> = rows
            .iter()
            .filter(|row| namespace.is_none_or(|ns| row.namespace == ns))
            .map(|row| ScoredVectorRecord {
                item: row.clone(),
                score: cosine_distance(query_vec, &row.vector),
            })
            .collect();
        hits.sort_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(k);
        Ok(hits)
    }

    async fn delete_thread_embeddings(
        &self,
        _scope: &RequestScope,
        thread_id: &str,
    ) -> Result<(), String> {
        let mut rows = self.rows.lock().map_err(|e| e.to_string())?;
        rows.retain(|row| !row.id.contains(thread_id));
        Ok(())
    }

    async fn delete_project_embeddings(&self, _scope: &RequestScope) -> Result<(), String> {
        self.rows.lock().map_err(|e| e.to_string())?.clear();
        Ok(())
    }

    async fn delete_namespace(&self, _scope: &RequestScope, namespace: &str) -> Result<(), String> {
        let mut rows = self.rows.lock().map_err(|e| e.to_string())?;
        rows.retain(|row| row.namespace != namespace);
        Ok(())
    }

    async fn delete_ids_with_prefix(
        &self,
        _scope: &RequestScope,
        prefix: &str,
    ) -> Result<(), String> {
        let mut rows = self.rows.lock().map_err(|e| e.to_string())?;
        rows.retain(|row| !row.id.starts_with(prefix));
        Ok(())
    }
}

fn bind_llm_runtime() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let file = ReactConfigFile {
            llm: Some(LlmFile {
                provider: Some("OPENAI_COMPAT".into()),
                embed_model: Some("text-embedding-3-small".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let cfg = resolve_config(file, ServeOverrides::default()).expect("resolve llm config");
        react::runtime_settings::bind_resolved_config(&cfg);
    });
}

fn write_corpus() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("mascot.md"),
        "The Skippr river otter mascot is named Nori.\nNori's favorite snack is smoked herring.\n",
    )
    .expect("write mascot");
    std::fs::write(
        dir.path().join("helix.md"),
        "Project Helix launched in 2019.\nThe launch city was Lisbon.\n",
    )
    .expect("write helix");
    dir
}

async fn kb_harness() -> (
    SuiteCtx,
    SuiteRegistry,
    tempfile::TempDir,
    tempfile::TempDir,
) {
    bind_llm_runtime();
    let corpus = write_corpus();
    let storage = tempfile::tempdir().expect("storage tempdir");
    let file = ReactConfigFile {
        storage: Some(react::config::StorageFile {
            mode: Some("local".into()),
            path: Some(storage.path().to_string_lossy().into_owned()),
            ..Default::default()
        }),
        scope: Some(react::config::ScopeFile {
            tenant: Some("smoke".into()),
            workspace: Some("kb".into()),
            project_id: Some("live".into()),
        }),
        llm: Some(LlmFile {
            provider: Some("OPENAI_COMPAT".into()),
            embed_model: Some("text-embedding-3-small".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let cfg = resolve_config(file, ServeOverrides::default()).expect("resolve");
    let mut sctx = react::bootstrap::build_base_suite_ctx(&cfg)
        .await
        .expect("suite ctx");
    sctx.set_vector(Some(Arc::new(MemoryVectorStore::default())));
    let mut registry = SuiteRegistry::new();
    registry.register(KbSuite);
    (sctx, registry, corpus, storage)
}

fn frames_text(frames: &[FlowFrame]) -> String {
    serde_json::to_string_pretty(frames).unwrap_or_else(|_| format!("{frames:?}"))
}

fn complete_blob(frames: &[FlowFrame]) -> String {
    let Some(FlowFrame::Complete {
        kind,
        payload,
        display,
    }) = frames
        .iter()
        .rev()
        .find(|f| matches!(f, FlowFrame::Complete { .. }))
    else {
        panic!(
            "expected a Complete frame from kb suite, got:\n{}",
            frames_text(frames)
        );
    };
    assert!(
        kind.as_str() == "kb" || kind.as_str() == "final" || kind.as_str() == "answer",
        "unexpected complete kind {kind:?}"
    );
    let mut blob = payload.to_string();
    if let Some(display) = display {
        blob.push('\n');
        blob.push_str(display);
    }
    blob
}

async fn ask(registry: &SuiteRegistry, sctx: &SuiteCtx, question: &str) -> String {
    let resp = execute_request(
        registry,
        sctx,
        ExecuteRequest {
            suite_id: "kb".into(),
            agent_type: Some("kb".into()),
            thread_id: None,
            question: question.to_string(),
            action: ExecuteAction::New,
            context: None,
        },
    )
    .await
    .unwrap_or_else(|e| panic!("kb execute failed: {e}"));
    eprintln!(
        "\n=== KB question ===\n{question}\n=== frames ===\n{}\n",
        frames_text(&resp.frames)
    );
    complete_blob(&resp.frames)
}

fn assert_contains_ci(hay: &str, needle: &str) {
    let h = hay.to_ascii_lowercase();
    let n = needle.to_ascii_lowercase();
    assert!(
        h.contains(&n),
        "expected {needle:?} in kb complete payload:\n{hay}"
    );
}

#[tokio::test]
async fn live_kb_answers_facts_from_ingested_corpus() {
    if !llm_ready() {
        eprintln!("skip live_kb_answers_facts_from_ingested_corpus: no LLM key");
        return;
    }
    std::env::set_var("LLM_KB_REASONING_EFFORT", "medium");
    let (sctx, registry, corpus, _storage) = kb_harness().await;
    let dir = corpus.path().display();

    let mascot = ask(
        &registry,
        &sctx,
        &format!(
            "Call kb_ingest_dir on {dir} with dataset_id kb. Then kb_search. Then complete. \
             Question: what is the Skippr mascot named? Use only retrieved file text. Do not invent filenames."
        ),
    )
    .await;
    assert_contains_ci(&mascot, "Nori");

    let snack = ask(
        &registry,
        &sctx,
        "The corpus is already ingested (dataset_id kb). Call kb_search, then complete. \
         Question: what is Nori's favorite snack? Quote the snack from retrieved text. Do not invent files.",
    )
    .await;
    assert_contains_ci(&snack, "herring");

    let year = ask(
        &registry,
        &sctx,
        "The corpus is already ingested (dataset_id kb). Call kb_search, then complete. \
         Question: in what year did Project Helix launch? Use only retrieved file text.",
    )
    .await;
    assert!(
        year.contains("2019"),
        "expected 2019 in kb complete payload:\n{year}"
    );
}
