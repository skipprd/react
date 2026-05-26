use std::sync::Arc;

use react_core::scope::RequestScope;
use react_core::suite::{
    FlowFrame as CoreFlowFrame, FlowKind as CoreFlowKind, SuiteCtx, SuiteCtxBuilder, SuiteRegistry,
};
pub use react_http_protocol::{
    ExecuteAction, ExecuteContext, ExecuteRequest, ExecuteResponse, ExecuteScopeOverride,
};

#[derive(Clone, Debug, Default)]
pub struct RequestExecutionContext {
    pub correlation_id: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecuteError {
    SuiteNotFound { suite_id: String },
    InvalidRequest(String),
    ExecutionFailed(String),
}

impl std::fmt::Display for ExecuteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SuiteNotFound { suite_id } => {
                write!(f, "unknown suite_id '{}'", suite_id)
            }
            Self::InvalidRequest(message) | Self::ExecutionFailed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ExecuteError {}

pub async fn execute_request(
    registry: &SuiteRegistry,
    base_suite_ctx: &SuiteCtx,
    request: ExecuteRequest,
) -> Result<ExecuteResponse, ExecuteError> {
    let suite = registry
        .get(&request.suite_id)
        .ok_or_else(|| ExecuteError::SuiteNotFound {
            suite_id: request.suite_id.clone(),
        })?;
    let thread_id = resolve_thread_id(&request)?;
    let agent_type = resolve_agent_type(&*suite, request.agent_type.clone())?;
    let suite_ctx = request_suite_ctx(base_suite_ctx, request.context.as_ref())?;

    tracing::info!(
        suite_id = %request.suite_id,
        agent_type = %agent_type,
        tenant = %suite_ctx.scope().tenant.as_str(),
        workspace = %suite_ctx.scope().workspace.as_str(),
        project_id = %suite_ctx.scope().project_id.as_str(),
        correlation_id = request
            .context
            .as_ref()
            .and_then(|context| context.correlation_id.as_deref())
            .unwrap_or(""),
        "handling react execution request"
    );

    let frames = match request.action {
        ExecuteAction::New => {
            suite.handle_new(&thread_id, &request.question, &agent_type, &suite_ctx)
        }
        ExecuteAction::Open => {
            suite.handle_open(&thread_id, &request.question, &agent_type, &suite_ctx)
        }
        ExecuteAction::User => {
            suite.handle_user(&thread_id, &request.question, &agent_type, &suite_ctx)
        }
    }
    .await
    .map_err(ExecuteError::ExecutionFailed)?;

    Ok(ExecuteResponse {
        thread_id,
        agent_type,
        frames: frames.iter().map(protocol_flow_frame).collect(),
    })
}

pub fn terminal_frame(frames: &[CoreFlowFrame]) -> Option<&CoreFlowFrame> {
    frames.iter().rev().find(|frame| match frame {
        CoreFlowFrame::Complete { .. }
        | CoreFlowFrame::Review { .. }
        | CoreFlowFrame::Interrupt { .. } => true,
        CoreFlowFrame::Checkpoint { .. } => false,
    })
}

fn protocol_flow_frame(frame: &CoreFlowFrame) -> react_http_protocol::FlowFrame {
    match frame {
        CoreFlowFrame::Complete {
            kind,
            payload,
            display,
        } => react_http_protocol::FlowFrame::Complete {
            kind: protocol_flow_kind(kind),
            payload: payload.clone(),
            display: display.clone(),
        },
        CoreFlowFrame::Review { text, meta } => react_http_protocol::FlowFrame::Review {
            text: text.clone(),
            meta: meta.clone(),
        },
        CoreFlowFrame::Checkpoint {
            kind,
            payload,
            display,
        } => react_http_protocol::FlowFrame::Checkpoint {
            kind: protocol_flow_kind(kind),
            payload: payload.clone(),
            display: display.clone(),
        },
        CoreFlowFrame::Interrupt { kind, prompt } => react_http_protocol::FlowFrame::Interrupt {
            kind: protocol_flow_kind(kind),
            prompt: prompt.clone(),
        },
    }
}

fn protocol_flow_kind(kind: &CoreFlowKind) -> react_http_protocol::FlowKind {
    react_http_protocol::FlowKind::new(kind.as_str())
}

fn request_suite_ctx(
    base_suite_ctx: &SuiteCtx,
    context: Option<&ExecuteContext>,
) -> Result<SuiteCtx, ExecuteError> {
    let scope = resolve_scope(
        base_suite_ctx.scope(),
        context.and_then(|ctx| ctx.scope.as_ref()),
    )?;
    let resolved_config = base_suite_ctx.resolved_config().as_ref().map(|config| {
        let mut next = (**config).clone();
        next.scope = scope.clone();
        Arc::new(next)
    });
    let mut suite_ctx = SuiteCtxBuilder::new(
        base_suite_ctx.storage().clone(),
        base_suite_ctx.secrets().clone(),
        base_suite_ctx.llm().clone(),
        scope,
        base_suite_ctx.keyspace().clone(),
    )
    .resolved_config(resolved_config)
    .trace_tx(base_suite_ctx.trace_tx().clone())
    .vector(base_suite_ctx.vector().clone())
    .state(base_suite_ctx.state().clone())
    .capabilities(base_suite_ctx.capabilities_ref().clone())
    .build();

    if let Some(context) = context {
        suite_ctx.set_capability(Arc::new(RequestExecutionContext {
            correlation_id: context.correlation_id.clone(),
            metadata: context.metadata.clone(),
        }));
    }

    Ok(suite_ctx)
}

fn resolve_scope(
    base_scope: &RequestScope,
    requested_scope: Option<&ExecuteScopeOverride>,
) -> Result<RequestScope, ExecuteError> {
    let tenant = requested_scope
        .and_then(|scope| scope.tenant.clone())
        .unwrap_or_else(|| base_scope.tenant.as_str().to_string());
    let workspace = requested_scope
        .and_then(|scope| scope.workspace.clone())
        .unwrap_or_else(|| base_scope.workspace.as_str().to_string());
    let project_id = requested_scope
        .and_then(|scope| scope.project_id.clone())
        .unwrap_or_else(|| base_scope.project_id.as_str().to_string());
    RequestScope::parse(tenant, workspace, project_id)
        .map_err(|error| ExecuteError::InvalidRequest(error.to_string()))
}

fn resolve_thread_id(request: &ExecuteRequest) -> Result<String, ExecuteError> {
    let provided = request
        .thread_id
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    match request.action {
        ExecuteAction::New => Ok(provided.unwrap_or_else(|| uuid::Uuid::new_v4().to_string())),
        ExecuteAction::Open | ExecuteAction::User => provided.ok_or_else(|| {
            ExecuteError::InvalidRequest(format!(
                "thread_id is required for '{}' actions",
                action_name(&request.action)
            ))
        }),
    }
}

fn resolve_agent_type(
    suite: &dyn react_core::suite::Suite,
    requested: Option<String>,
) -> Result<String, ExecuteError> {
    let agent_type = requested
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| suite.default_agent_type().to_string());
    let supported = suite.supported_agent_types();
    if supported.is_empty() || supported.iter().any(|candidate| candidate == &agent_type) {
        return Ok(agent_type);
    }
    Err(ExecuteError::InvalidRequest(format!(
        "unsupported agent_type '{}' for suite '{}' (expected one of: {})",
        agent_type,
        suite.id(),
        supported.join(", ")
    )))
}

fn action_name(action: &ExecuteAction) -> &'static str {
    match action {
        ExecuteAction::New => "new",
        ExecuteAction::Open => "open",
        ExecuteAction::User => "user",
    }
}
