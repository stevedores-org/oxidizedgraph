//! JSON-RPC 2.0 dispatch for A2A methods.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::worker::{WorkerJobSpec, WorkerSpawner};

use super::store::TaskStore;
use super::types::{SendMessageRequest, SendMessageResponse, Task, TaskState};

/// JSON-RPC 2.0 request envelope.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    /// Protocol version — must be `"2.0"`.
    pub jsonrpc: String,
    /// Request id (string or number).
    pub id: Value,
    /// PascalCase A2A method name.
    pub method: String,
    /// Method params object.
    #[serde(default)]
    pub params: Value,
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct JsonRpcError {
    /// Error code.
    pub code: i32,
    /// Human-readable message.
    pub message: String,
    /// Optional structured data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: -32600,
            message: message.into(),
            data: None,
        }
    }

    fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("Method not found: {method}"),
            data: None,
        }
    }

    fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
            data: None,
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: message.into(),
            data: None,
        }
    }
}

/// JSON-RPC 2.0 response envelope.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    /// Protocol version.
    pub jsonrpc: String,
    /// Echo of request id.
    pub id: Value,
    /// Success result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn err(id: Value, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// Dispatch one JSON-RPC request against the orchestrator task store and worker spawner.
pub async fn dispatch(
    request: JsonRpcRequest,
    store: &TaskStore,
    spawner: &dyn WorkerSpawner,
) -> JsonRpcResponse {
    if request.jsonrpc != "2.0" {
        return JsonRpcResponse::err(
            request.id,
            JsonRpcError::invalid_request("jsonrpc must be '2.0'"),
        );
    }

    match request.method.as_str() {
        "SendMessage" => handle_send_message(request.id, request.params, store, spawner).await,
        "GetTask" => handle_get_task(request.id, request.params, store, spawner).await,
        "CancelTask" => handle_cancel_task(request.id, request.params, store).await,
        other => JsonRpcResponse::err(
            request.id,
            JsonRpcError::method_not_found(other),
        ),
    }
}

async fn handle_send_message(
    id: Value,
    params: Value,
    store: &TaskStore,
    spawner: &dyn WorkerSpawner,
) -> JsonRpcResponse {
    let req: SendMessageRequest = match serde_json::from_value(params) {
        Ok(v) => v,
        Err(e) => {
            return JsonRpcResponse::err(
                id,
                JsonRpcError::invalid_params(format!("invalid SendMessage params: {e}")),
            );
        }
    };

    let mut task = Task::submitted(&req.message);
    store.upsert(task.clone());

    let spec = WorkerJobSpec::from_task(&task);
    match spawner.spawn(spec).await {
        Ok(handle) => {
            task.mark_working(handle.job_name.clone());
            store.upsert(task.clone());
        }
        Err(e) => {
            task.mark_failed(e.to_string());
            store.upsert(task.clone());
            if !req.configuration.return_immediately {
                return JsonRpcResponse::err(id, JsonRpcError::internal(e.to_string()));
            }
        }
    }

    let response = SendMessageResponse { task };
    match serde_json::to_value(response) {
        Ok(v) => JsonRpcResponse::ok(id, v),
        Err(e) => JsonRpcResponse::err(id, JsonRpcError::internal(e.to_string())),
    }
}

async fn handle_get_task(
    id: Value,
    params: Value,
    store: &TaskStore,
    spawner: &dyn WorkerSpawner,
) -> JsonRpcResponse {
    let task_id = match params.get("id").and_then(Value::as_str) {
        Some(v) => v.to_string(),
        None => {
            return JsonRpcResponse::err(
                id,
                JsonRpcError::invalid_params("GetTask requires params.id"),
            );
        }
    };

    let Some(mut task) = store.get(&task_id) else {
        return JsonRpcResponse::err(
            id,
            JsonRpcError::invalid_params(format!("task {task_id} not found")),
        );
    };

    if let Some(job_name) = task.worker_job.clone() {
        if let Ok(status) = spawner.status(&job_name).await {
            sync_task_with_worker(&mut task, status);
            store.upsert(task.clone());
        }
    }

    match serde_json::to_value(task) {
        Ok(v) => JsonRpcResponse::ok(id, v),
        Err(e) => JsonRpcResponse::err(id, JsonRpcError::internal(e.to_string())),
    }
}

async fn handle_cancel_task(id: Value, params: Value, store: &TaskStore) -> JsonRpcResponse {
    let task_id = match params.get("id").and_then(Value::as_str) {
        Some(v) => v.to_string(),
        None => {
            return JsonRpcResponse::err(
                id,
                JsonRpcError::invalid_params("CancelTask requires params.id"),
            );
        }
    };

    let Some(task) = store.cancel(&task_id) else {
        return JsonRpcResponse::err(
            id,
            JsonRpcError::invalid_params(format!("task {task_id} not found")),
        );
    };

    match serde_json::to_value(task) {
        Ok(v) => JsonRpcResponse::ok(id, v),
        Err(e) => JsonRpcResponse::err(id, JsonRpcError::internal(e.to_string())),
    }
}

fn sync_task_with_worker(task: &mut Task, status: crate::worker::WorkerJobStatus) {
    if task.status.state.is_terminal() {
        return;
    }

    task.status.timestamp = Some(chrono::Utc::now().to_rfc3339());
    task.status.state = match status {
        crate::worker::WorkerJobStatus::Pending | crate::worker::WorkerJobStatus::Running => {
            TaskState::TaskStateWorking
        }
        crate::worker::WorkerJobStatus::Succeeded => TaskState::TaskStateCompleted,
        crate::worker::WorkerJobStatus::Failed => TaskState::TaskStateFailed,
        crate::worker::WorkerJobStatus::NotFound => TaskState::TaskStateFailed,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a2a::types::Message;
    use crate::worker::MemoryWorkerSpawner;

    #[tokio::test]
    async fn send_message_spawns_worker_and_returns_task() {
        let store = TaskStore::new();
        let spawner = MemoryWorkerSpawner::default_spawner();

        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: serde_json::json!(1),
            method: "SendMessage".into(),
            params: serde_json::json!({
                "message": Message::user_text("build feature x"),
                "configuration": { "returnImmediately": true }
            }),
        };

        let resp = dispatch(req, &store, &spawner).await;
        assert!(resp.error.is_none(), "{:?}", resp.error);
        let task = resp.result.unwrap();
        assert_eq!(task["task"]["status"]["state"], "TASK_STATE_WORKING");
    }
}
