use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use react_core::suite::{SuiteCtx, SuiteRegistry};
use serde::Serialize;

use crate::execute::{execute_request, ExecuteError, ExecuteRequest, ExecuteResponse};

#[derive(Clone)]
pub struct HttpState {
    suite_ctx: SuiteCtx,
    registry: Arc<SuiteRegistry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SuiteDescriptor {
    pub id: String,
    pub label: String,
    pub default_agent_type: String,
    pub supported_agent_types: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct ErrorBody {
    error: String,
}

pub fn build_router(suite_ctx: SuiteCtx, registry: Arc<SuiteRegistry>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/suites", get(list_suites))
        .route("/api/executions", post(execute))
        .with_state(HttpState { suite_ctx, registry })
}

pub async fn serve(
    addr: SocketAddr,
    suite_ctx: SuiteCtx,
    registry: Arc<SuiteRegistry>,
) -> Result<(), String> {
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|error| error.to_string())?;
    tracing::info!("HTTP server listening on http://{}", addr);
    axum::serve(listener, build_router(suite_ctx, registry))
        .await
        .map_err(|error| error.to_string())
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
}

async fn list_suites(State(state): State<HttpState>) -> Json<Vec<SuiteDescriptor>> {
    let suites = state
        .registry
        .list_ids()
        .into_iter()
        .filter_map(|suite_id| {
            state.registry.get(suite_id).map(|suite| SuiteDescriptor {
                id: suite.id().to_string(),
                label: suite.label().to_string(),
                default_agent_type: suite.default_agent_type().to_string(),
                supported_agent_types: suite.supported_agent_types(),
            })
        })
        .collect();
    Json(suites)
}

async fn execute(
    State(state): State<HttpState>,
    Json(request): Json<ExecuteRequest>,
) -> Result<Json<ExecuteResponse>, (StatusCode, Json<ErrorBody>)> {
    execute_request(&state.registry, &state.suite_ctx, request)
        .await
        .map(Json)
        .map_err(error_response)
}

fn error_response(error: ExecuteError) -> (StatusCode, Json<ErrorBody>) {
    let status = match error {
        ExecuteError::SuiteNotFound { .. } => StatusCode::NOT_FOUND,
        ExecuteError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
        ExecuteError::ExecutionFailed(_) => StatusCode::UNPROCESSABLE_ENTITY,
    };
    (
        status,
        Json(ErrorBody {
            error: error.to_string(),
        }),
    )
}
