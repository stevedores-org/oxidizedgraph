//! Integration tests for issue #41 — A2A dispatch + ephemeral worker spawning.

use oxidizedgraph::a2a::{self, AgentCard, JsonRpcRequest, TaskStore};
use oxidizedgraph::worker::{
    MemoryWorkerSpawner, WorkerJobSpec, WorkerSpawner, WorkerSpawnerConfig,
};

#[tokio::test]
async fn a2a_send_message_creates_working_task_with_job_name() {
    let store = TaskStore::new();
    let spawner = MemoryWorkerSpawner::default_spawner();

    let req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: serde_json::json!("req-1"),
        method: "SendMessage".into(),
        params: serde_json::json!({
            "message": {
                "role": "ROLE_USER",
                "parts": [{"text": "implement worker orchestration"}],
                "messageId": "msg-1"
            },
            "configuration": { "returnImmediately": true }
        }),
    };

    let resp = a2a::dispatch(req, &store, &spawner).await;
    assert!(resp.error.is_none(), "{:?}", resp.error);

    let task_id = resp.result.as_ref().unwrap()["task"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let stored = store.get(&task_id).expect("task stored");
    assert_eq!(
        stored.status.state,
        oxidizedgraph::a2a::TaskState::TaskStateWorking
    );
    assert!(stored.worker_job.is_some());
}

#[tokio::test]
async fn get_task_reflects_completed_worker_job() {
    let store = TaskStore::new();
    let spawner = MemoryWorkerSpawner::default_spawner();

    let send = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: serde_json::json!(1),
        method: "SendMessage".into(),
        params: serde_json::json!({
            "message": {
                "role": "ROLE_USER",
                "parts": [{"text": "run ci"}],
                "messageId": "msg-2"
            }
        }),
    };
    let sent = a2a::dispatch(send, &store, &spawner).await;
    let task_id = sent.result.unwrap()["task"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let job_name = store.get(&task_id).unwrap().worker_job.clone().unwrap();

    spawner.complete_job(&job_name);

    let get = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: serde_json::json!(2),
        method: "GetTask".into(),
        params: serde_json::json!({ "id": task_id }),
    };
    let got = a2a::dispatch(get, &store, &spawner).await;
    assert_eq!(
        got.result.unwrap()["status"]["state"],
        "TASK_STATE_COMPLETED"
    );
}

#[test]
fn agent_card_advertises_orchestrator_rpc() {
    let card = AgentCard::orchestrator("http://orchestrator:8080");
    assert!(card.capabilities.send_message);
    assert!(card.url.ends_with("/rpc"));
}

#[tokio::test]
async fn memory_spawner_records_running_jobs() {
    let spawner = MemoryWorkerSpawner::new(WorkerSpawnerConfig::default());
    let handle = spawner
        .spawn(WorkerJobSpec {
            task_id: "task-abc".into(),
            context_id: "ctx-1".into(),
            input_text: "hello".into(),
            image: None,
        })
        .await
        .unwrap();

    assert_eq!(
        spawner.status(&handle.job_name).await.unwrap(),
        oxidizedgraph::worker::WorkerJobStatus::Running
    );
}
