//! Live smoke: Cloudflare R2 (`OBJECTS_*`) + OpenAI (`LLM_API_KEY` / `OPENAI_API_KEY`).
//! Skips (does not fail) when credentials are absent so CI stays green.

use react::config::{resolve_config, ReactConfigFile, ServeOverrides};
use react::llm::{create_llm, ChatMessage, ChatRole};
use react_core::llm::{LlmCallOptions, LlmExpectedFormat};
use react_core::storage::StorageAdapter;
use react_module_storage_s3::{
    objects_credentials_from_env, S3StorageAdapter, DEFAULT_RUNTIME_BUCKET, DEFAULT_VECTORS_BUCKET,
};

fn env_ready(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .is_some_and(|v| !v.trim().is_empty())
}

fn objects_ready() -> bool {
    env_ready("OBJECTS_ACCESS_KEY_ID")
        && env_ready("OBJECTS_SECRET_ACCESS_KEY")
        && env_ready("OBJECTS_S3_ENDPOINT")
}

fn llm_ready() -> bool {
    env_ready("LLM_API_KEY") || env_ready("OPENAI_API_KEY")
}

async fn put_get_delete(bucket: &str, token: &str) {
    let store = S3StorageAdapter::from_env(bucket.to_string())
        .await
        .unwrap_or_else(|e| panic!("R2 from_env({bucket}): {e}"));
    let key = format!("smoke/{token}/probe.txt");
    store
        .put_bytes(&key, b"react-live-smoke", "text/plain")
        .await
        .unwrap_or_else(|e| panic!("R2 put {bucket}/{key}: {e}"));
    let got = store
        .get_bytes(&key)
        .await
        .unwrap_or_else(|e| panic!("R2 get {bucket}/{key}: {e}"));
    assert_eq!(got, b"react-live-smoke", "round-trip mismatch in {bucket}");
    store
        .delete_object(&key)
        .await
        .unwrap_or_else(|e| panic!("R2 delete {bucket}/{key}: {e}"));
}

#[tokio::test]
async fn live_r2_objects_round_trip() {
    if !objects_ready() {
        eprintln!("skip live_r2_objects_round_trip: OBJECTS_* not set");
        return;
    }
    let creds = objects_credentials_from_env().expect("OBJECTS_*");
    assert!(creds.force_path_style);
    assert!(creds
        .endpoint
        .as_deref()
        .is_some_and(|e| e.contains("r2.cloudflarestorage.com")));

    let token = uuid::Uuid::new_v4().to_string();
    put_get_delete(DEFAULT_RUNTIME_BUCKET, &token).await;
    put_get_delete(DEFAULT_VECTORS_BUCKET, &token).await;
}

#[tokio::test]
async fn live_llm_ping() {
    if !llm_ready() {
        eprintln!("skip live_llm_ping: LLM_API_KEY / OPENAI_API_KEY not set");
        return;
    }
    let file = ReactConfigFile {
        llm: Some(react::config::LlmFile {
            provider: Some("OPENAI_COMPAT".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let cfg = resolve_config(file, ServeOverrides::default()).expect("resolve llm config");
    react::runtime_settings::bind_resolved_config(&cfg);
    let llm = create_llm(&react::llm::config_from_resolved(&cfg));
    let text = llm
        .chat(
            &[ChatMessage {
                role: ChatRole::User,
                content: "Reply with exactly the word pong and nothing else.".to_string(),
            }],
            &LlmCallOptions {
                prompt_id: "react.tests.live_smoke.llm_ping",
                expected_format: LlmExpectedFormat::Text,
                max_output_tokens: Some(32),
                reasoning_effort: Some(react_core::llm::ReasoningEffort::High),
                timeout_secs: Some(120),
                ..Default::default()
            },
        )
        .await
        .expect("llm chat");
    let lower = text.to_ascii_lowercase();
    assert!(
        lower.contains("pong"),
        "expected pong in model reply, got {} chars",
        text.len()
    );
}
