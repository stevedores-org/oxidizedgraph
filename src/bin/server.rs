//! oxidizedgraph API Server
//!
//! REST API for executing graph-based AI agent workflows.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use oxidizedgraph::prelude::*;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::sync::RwLock;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use uuid::Uuid;

/// Application state
struct AppState {
    sessions: RwLock<HashMap<String, SharedState>>,
    workflow: CompiledGraph,
    checkpointer: Arc<MemoryCheckpointer>,
}

/// Health check response
#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

/// Session create request
#[derive(Deserialize)]
struct CreateSessionRequest {
    #[serde(default)]
    initial_state: Option<serde_json::Value>,
}

/// Session response
#[derive(Serialize)]
struct SessionResponse {
    session_id: String,
    created: bool,
}

/// Execute request
#[derive(Deserialize)]
struct ExecuteRequest {
    input: serde_json::Value,
}

/// Execute response
#[derive(Serialize)]
struct ExecuteResponse {
    session_id: String,
    output: serde_json::Value,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    checkpoint_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    transitions: Option<usize>,
}

/// Error response
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    code: &'static str,
}

#[derive(Deserialize)]
struct HitlPauseRequest {
    reason: String,
    #[serde(default)]
    risk_level: Option<RiskLevel>,
}

#[derive(Deserialize)]
struct HitlEditRequest {
    context_patches: HashMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct HitlApproveRequest {
    approver: String,
    #[serde(default)]
    rationale: Option<String>,
}

#[derive(Deserialize)]
struct HitlDenyRequest {
    approver: String,
    rationale: String,
}

fn build_workflow() -> CompiledGraph {
    GraphBuilder::new()
        .name("server-session-workflow")
        .description("Minimal server-side workflow for session execution")
        .add_node(FunctionNode::new("prepare_input", |state| async move {
            let mut guard = state
                .write()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            guard.set_context("execution_stage", "prepared");
            Ok(NodeOutput::continue_to("finalize_execution"))
        }))
        .add_node(FunctionNode::new(
            "finalize_execution",
            |state| async move {
                let (input, run_id, session_id) = {
                    let guard = state
                        .read()
                        .map_err(|e| NodeError::execution_failed(e.to_string()))?;
                    (
                        guard
                            .get_context::<serde_json::Value>("input")
                            .unwrap_or(serde_json::Value::Null),
                        guard.get_context::<String>("run_id").unwrap_or_default(),
                        guard
                            .get_context::<String>("session_id")
                            .unwrap_or_default(),
                    )
                };

                let output = serde_json::json!({
                    "processed": true,
                    "input": input,
                    "run_id": run_id,
                    "session_id": session_id,
                    "status": "completed",
                });

                let mut guard = state
                    .write()
                    .map_err(|e| NodeError::execution_failed(e.to_string()))?;
                guard.set_context("execution_output", output);
                guard.mark_complete();
                Ok(NodeOutput::finish())
            },
        ))
        .set_entry_point("prepare_input")
        .add_edge("prepare_input", "finalize_execution")
        .add_edge_to_end("finalize_execution")
        .compile()
        .expect("server workflow graph should compile")
}

async fn get_session_shared(
    state: &Arc<AppState>,
    session_id: &str,
) -> Result<SharedState, (StatusCode, Json<ErrorResponse>)> {
    let sessions = state.sessions.read().await;
    sessions.get(session_id).cloned().ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Session {session_id} not found"),
                code: "SESSION_NOT_FOUND",
            }),
        )
    })
}

async fn write_back_session(
    shared: &SharedState,
    state: AgentState,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let mut guard = shared.write().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Lock error: {e}"),
                code: "LOCK_ERROR",
            }),
        )
    })?;
    *guard = state;
    Ok(())
}

async fn run_session_workflow(
    workflow: &CompiledGraph,
    session_id: &str,
    state: AgentState,
) -> Result<TracedRunResult, RuntimeError> {
    let run_context = RunContext::with_ids(Uuid::new_v4().to_string(), session_id.to_string());
    let runner = TracedRunner::with_context(workflow.clone(), run_context, RunnerConfig::default());
    runner.invoke(state).await
}

fn output_from_state(state: &AgentState) -> serde_json::Value {
    state
        .get_context::<serde_json::Value>("execution_output")
        .unwrap_or_else(|| serde_json::to_value(state).unwrap_or_default())
}

fn checkpoint_payload(session_id: &str, checkpoint: &Checkpoint) -> serde_json::Value {
    serde_json::json!({
        "checkpoint_id": &checkpoint.id,
        "session_id": session_id,
        "state": &checkpoint.state,
        "created_at": &checkpoint.created_at,
        "parent_id": &checkpoint.parent_id,
        "metadata": &checkpoint.metadata,
    })
}

async fn with_session_mut<F, T>(
    state: &Arc<AppState>,
    session_id: &str,
    f: F,
) -> Result<T, (StatusCode, Json<ErrorResponse>)>
where
    F: FnOnce(&mut AgentState) -> Result<T, HitlError>,
{
    let shared = get_session_shared(state, session_id).await?;
    let mut agent_state = shared.write().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Lock error: {e}"),
                code: "LOCK_ERROR",
            }),
        )
    })?;
    f(&mut agent_state).map_err(|e| {
        (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: e.to_string(),
                code: "HITL_ERROR",
            }),
        )
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .compact()
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Get port from environment
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    // Initialize app state
    let state = Arc::new(AppState {
        sessions: RwLock::new(HashMap::new()),
        workflow: build_workflow(),
        checkpointer: Arc::new(MemoryCheckpointer::new()),
    });

    // Build router
    let app = Router::new()
        .route("/health", get(health))
        .route("/readiness", get(readiness))
        .route("/api/v1/sessions", post(create_session))
        .route("/api/v1/sessions/:id", get(get_session))
        .route("/api/v1/sessions/:id/execute", post(execute))
        .route("/api/v1/sessions/:id/checkpoint", post(checkpoint))
        .route("/api/v1/sessions/:id/restore", post(restore))
        .route("/api/v1/sessions/:id/hitl/status", get(hitl_status))
        .route("/api/v1/sessions/:id/hitl/pause", post(hitl_pause))
        .route("/api/v1/sessions/:id/hitl/edit", post(hitl_edit))
        .route("/api/v1/sessions/:id/hitl/approve", post(hitl_approve))
        .route("/api/v1/sessions/:id/hitl/deny", post(hitl_deny))
        .route("/api/v1/sessions/:id/hitl/timeline", get(hitl_timeline))
        .with_state(state);

    // Start server
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("oxidizedgraph server starting on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Health check endpoint
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Readiness check endpoint
async fn readiness() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ready",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Create a new session
async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<SessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let session_id = Uuid::new_v4().to_string();

    let agent_state = if let Some(initial) = req.initial_state {
        let mut s = AgentState::new();
        if let Some(obj) = initial.as_object() {
            for (k, v) in obj {
                s.set_context(k.clone(), v.clone());
            }
        }
        s
    } else {
        AgentState::new()
    };

    let shared_state = SharedState::new_shared(agent_state);

    let mut sessions = state.sessions.write().await;
    sessions.insert(session_id.clone(), shared_state);

    info!("Created session: {}", session_id);

    Ok(Json(SessionResponse {
        session_id,
        created: true,
    }))
}

/// Get session state
async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let shared_state = get_session_shared(&state, &session_id).await?;
    let agent_state = shared_state.read().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Lock error: {}", e),
                code: "LOCK_ERROR",
            }),
        )
    })?;
    let data = serde_json::to_value(&*agent_state).unwrap_or_default();
    Ok(Json(data))
}

/// Execute a workflow step
async fn execute(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<ExecuteResponse>, (StatusCode, Json<ErrorResponse>)> {
    let shared_state = get_session_shared(&state, &session_id).await?;
    let mut initial_state = shared_state
        .read()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Lock error: {e}"),
                    code: "LOCK_ERROR",
                }),
            )
        })?
        .clone();
    initial_state.set_context("input", req.input.clone());
    initial_state.set_context("session_id", session_id.clone());

    let result = run_session_workflow(&state.workflow, &session_id, initial_state)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                    code: "EXECUTION_ERROR",
                }),
            )
        })?;

    write_back_session(&shared_state, result.state.clone()).await?;

    let checkpoint = Checkpoint::new(&session_id, "finalize_execution", result.state.clone())
        .with_metadata(serde_json::json!({
            "run_id": result.run_context.run_id.clone(),
            "transition_count": result.transition_log.len(),
        }));
    let checkpoint_id = checkpoint.id.clone();
    state.checkpointer.save(checkpoint).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
                code: "CHECKPOINT_ERROR",
            }),
        )
    })?;

    Ok(Json(ExecuteResponse {
        session_id,
        output: output_from_state(&result.state),
        status: "completed".to_string(),
        run_id: Some(result.run_context.run_id),
        checkpoint_id: Some(checkpoint_id),
        transitions: Some(result.transition_log.len()),
    }))
}

/// Create a checkpoint
async fn checkpoint(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let shared_state = get_session_shared(&state, &session_id).await?;
    let agent_state = shared_state
        .read()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Lock error: {e}"),
                    code: "LOCK_ERROR",
                }),
            )
        })?
        .clone();
    let checkpoint = Checkpoint::new(&session_id, "manual_checkpoint", agent_state);
    let checkpoint_id = checkpoint.id.clone();
    state
        .checkpointer
        .save(checkpoint.clone())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                    code: "CHECKPOINT_ERROR",
                }),
            )
        })?;

    info!(
        "Created checkpoint {} for session {}",
        checkpoint_id, session_id
    );

    Ok(Json(checkpoint_payload(&session_id, &checkpoint)))
}

/// Restore from checkpoint
async fn restore(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(checkpoint): Json<serde_json::Value>,
) -> Result<Json<SessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let restored_state = if let Some(checkpoint_id) = checkpoint
        .get("checkpoint_id")
        .and_then(|value| value.as_str())
    {
        match state
            .checkpointer
            .load_by_id(checkpoint_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: e.to_string(),
                        code: "CHECKPOINT_ERROR",
                    }),
                )
            })? {
            Some(saved) => saved.state,
            None => {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: format!("Checkpoint {checkpoint_id} not found"),
                        code: "CHECKPOINT_NOT_FOUND",
                    }),
                ));
            }
        }
    } else if let Some(state_data) = checkpoint.get("state") {
        serde_json::from_value(state_data.clone()).unwrap_or_else(|_| AgentState::new())
    } else {
        serde_json::from_value(checkpoint).unwrap_or_else(|_| AgentState::new())
    };

    let mut sessions = state.sessions.write().await;
    sessions.insert(session_id.clone(), SharedState::new_shared(restored_state));

    info!("Restored session {} from checkpoint", session_id);

    Ok(Json(SessionResponse {
        session_id,
        created: false,
    }))
}

async fn hitl_status(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<HitlStatus>, (StatusCode, Json<ErrorResponse>)> {
    let shared = get_session_shared(&state, &session_id).await?;
    let agent_state = shared.read().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Lock error: {e}"),
                code: "LOCK_ERROR",
            }),
        )
    })?;
    Ok(Json(HitlController::new().status(&agent_state)))
}

async fn hitl_pause(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(req): Json<HitlPauseRequest>,
) -> Result<Json<ApprovalRequest>, (StatusCode, Json<ErrorResponse>)> {
    let controller = HitlController::new();
    let risk = req.risk_level.unwrap_or(RiskLevel::High);
    with_session_mut(&state, &session_id, |agent_state| {
        Ok(controller.pause(agent_state, req.reason, risk))
    })
    .await
    .map(Json)
}

async fn hitl_edit(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(req): Json<HitlEditRequest>,
) -> Result<Json<InterventionEdit>, (StatusCode, Json<ErrorResponse>)> {
    let controller = HitlController::new();
    let edits = InterventionEdit {
        context_patches: req.context_patches,
    };
    with_session_mut(&state, &session_id, |agent_state| {
        controller.queue_edits(agent_state, edits.clone())?;
        Ok(edits)
    })
    .await
    .map(Json)
}

async fn hitl_approve(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(req): Json<HitlApproveRequest>,
) -> Result<Json<ApprovalDecision>, (StatusCode, Json<ErrorResponse>)> {
    let controller = HitlController::new();
    with_session_mut(&state, &session_id, |agent_state| {
        controller.approve(agent_state, req.approver, req.rationale)
    })
    .await
    .map(Json)
}

async fn hitl_deny(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(req): Json<HitlDenyRequest>,
) -> Result<Json<ApprovalDecision>, (StatusCode, Json<ErrorResponse>)> {
    let controller = HitlController::new();
    with_session_mut(&state, &session_id, |agent_state| {
        controller.deny(agent_state, req.approver, req.rationale)
    })
    .await
    .map(Json)
}

async fn hitl_timeline(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<RunTimeline>, (StatusCode, Json<ErrorResponse>)> {
    let shared = get_session_shared(&state, &session_id).await?;
    let agent_state = shared.read().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Lock error: {e}"),
                code: "LOCK_ERROR",
            }),
        )
    })?;
    let approvals: Vec<ApprovalEvent> = agent_state
        .get_context(CTX_APPROVAL_EVENTS)
        .unwrap_or_default();
    let timeline = RunTimeline::from_artifacts(&[], &approvals);
    Ok(Json(timeline))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn server_workflow_executes_real_graph() {
        let workflow = build_workflow();
        let mut state = AgentState::new();
        state.set_context("input", serde_json::json!({"prompt": "hello"}));
        state.set_context("session_id", "session-1");

        let result = run_session_workflow(&workflow, "session-1", state)
            .await
            .unwrap();

        assert_eq!(result.run_context.thread_id, "session-1");
        assert_eq!(result.transition_log.len(), 2);

        let output: serde_json::Value = result
            .state
            .get_context("execution_output")
            .expect("execution output should be recorded");

        assert_eq!(output["processed"], true);
        assert_eq!(output["session_id"], "session-1");
        assert_eq!(output["input"]["prompt"], "hello");
    }

    #[tokio::test]
    async fn checkpoint_payload_includes_state_snapshot() {
        let checkpoint = Checkpoint::new("session-1", "node-a", AgentState::new());
        let payload = checkpoint_payload("session-1", &checkpoint);

        assert_eq!(payload["session_id"], "session-1");
        assert_eq!(payload["checkpoint_id"], checkpoint.id);
        assert!(payload["state"].is_object());
    }
}
