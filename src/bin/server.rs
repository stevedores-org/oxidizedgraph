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
}

/// Error response
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    code: &'static str,
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
    let sessions = state.sessions.read().await;

    match sessions.get(&session_id) {
        Some(shared_state) => {
            let agent_state = shared_state
                .read()
                .map_err(|e| {
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
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Session {} not found", session_id),
                code: "SESSION_NOT_FOUND",
            }),
        )),
    }
}

/// Execute a workflow step
async fn execute(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<ExecuteResponse>, (StatusCode, Json<ErrorResponse>)> {
    let sessions = state.sessions.read().await;

    match sessions.get(&session_id) {
        Some(shared_state) => {
            // Update state with input
            {
                let mut agent_state = shared_state.write().map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: format!("Lock error: {}", e),
                            code: "LOCK_ERROR",
                        }),
                    )
                })?;
                agent_state.set_context("input", req.input.clone());
            }

            // In a real implementation, this would execute the graph
            // For now, we just echo the input
            let output = serde_json::json!({
                "processed": true,
                "input": req.input,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });

            Ok(Json(ExecuteResponse {
                session_id,
                output,
                status: "completed".to_string(),
            }))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Session {} not found", session_id),
                code: "SESSION_NOT_FOUND",
            }),
        )),
    }
}

/// Create a checkpoint
async fn checkpoint(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let sessions = state.sessions.read().await;

    match sessions.get(&session_id) {
        Some(shared_state) => {
            let agent_state = shared_state.read().map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Lock error: {}", e),
                        code: "LOCK_ERROR",
                    }),
                )
            })?;
            let checkpoint_id = Uuid::new_v4().to_string();

            // In production, this would persist to storage
            let checkpoint_data = serde_json::json!({
                "checkpoint_id": checkpoint_id,
                "session_id": session_id,
                "state": serde_json::to_value(&*agent_state).unwrap_or_default(),
                "created_at": chrono::Utc::now().to_rfc3339(),
            });

            info!("Created checkpoint {} for session {}", checkpoint_id, session_id);

            Ok(Json(checkpoint_data))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Session {} not found", session_id),
                code: "SESSION_NOT_FOUND",
            }),
        )),
    }
}

/// Restore from checkpoint
async fn restore(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(checkpoint): Json<serde_json::Value>,
) -> Result<Json<SessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    let mut sessions = state.sessions.write().await;

    let restored_state = if let Some(state_data) = checkpoint.get("state") {
        serde_json::from_value(state_data.clone()).unwrap_or_else(|_| AgentState::new())
    } else {
        AgentState::new()
    };

    sessions.insert(session_id.clone(), SharedState::new_shared(restored_state));

    info!("Restored session {} from checkpoint", session_id);

    Ok(Json(SessionResponse {
        session_id,
        created: false,
    }))
}
