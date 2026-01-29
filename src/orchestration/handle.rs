//! Subgraph execution handle

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::task::JoinHandle;

use crate::error::RuntimeError;
use crate::state::AgentState;

/// Result of subgraph execution
#[derive(Debug)]
pub enum SubgraphResult {
    /// Subgraph completed successfully
    Completed {
        /// Subgraph identifier
        subgraph_id: String,
        /// Final state from the subgraph
        state: AgentState,
    },
    /// Subgraph failed
    Failed {
        /// Subgraph identifier
        subgraph_id: String,
        /// Error that occurred
        error: RuntimeError,
    },
    /// Subgraph was cancelled
    Cancelled {
        /// Subgraph identifier
        subgraph_id: String,
    },
}

impl SubgraphResult {
    /// Check if the subgraph completed successfully
    pub fn is_completed(&self) -> bool {
        matches!(self, SubgraphResult::Completed { .. })
    }

    /// Check if the subgraph failed
    pub fn is_failed(&self) -> bool {
        matches!(self, SubgraphResult::Failed { .. })
    }

    /// Get the subgraph ID
    pub fn subgraph_id(&self) -> &str {
        match self {
            SubgraphResult::Completed { subgraph_id, .. } => subgraph_id,
            SubgraphResult::Failed { subgraph_id, .. } => subgraph_id,
            SubgraphResult::Cancelled { subgraph_id } => subgraph_id,
        }
    }

    /// Get the state if completed successfully
    pub fn state(&self) -> Option<&AgentState> {
        match self {
            SubgraphResult::Completed { state, .. } => Some(state),
            _ => None,
        }
    }

    /// Take ownership of the state if completed successfully
    pub fn into_state(self) -> Result<AgentState, RuntimeError> {
        match self {
            SubgraphResult::Completed { state, .. } => Ok(state),
            SubgraphResult::Failed { error, .. } => Err(error),
            SubgraphResult::Cancelled { subgraph_id } => {
                Err(RuntimeError::InvalidState(format!(
                    "Subgraph '{}' was cancelled",
                    subgraph_id
                )))
            }
        }
    }

    /// Get the error if failed
    pub fn error(&self) -> Option<&RuntimeError> {
        match self {
            SubgraphResult::Failed { error, .. } => Some(error),
            _ => None,
        }
    }
}

/// Handle to a spawned subgraph execution
///
/// This handle can be awaited to get the result of the subgraph.
/// It wraps a tokio::JoinHandle for the background task.
pub struct SubgraphHandle {
    /// Subgraph identifier
    pub subgraph_id: String,
    /// The underlying join handle
    handle: JoinHandle<SubgraphResult>,
}

impl SubgraphHandle {
    /// Create a new subgraph handle
    pub(crate) fn new(subgraph_id: String, handle: JoinHandle<SubgraphResult>) -> Self {
        Self { subgraph_id, handle }
    }

    /// Get the subgraph ID
    pub fn id(&self) -> &str {
        &self.subgraph_id
    }

    /// Check if the subgraph has completed
    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    /// Abort the subgraph execution
    pub fn abort(&self) {
        self.handle.abort();
    }

    /// Wait for the subgraph to complete and return the result
    pub async fn join(self) -> SubgraphResult {
        match self.handle.await {
            Ok(result) => result,
            Err(e) if e.is_cancelled() => SubgraphResult::Cancelled {
                subgraph_id: self.subgraph_id,
            },
            Err(e) => SubgraphResult::Failed {
                subgraph_id: self.subgraph_id,
                error: RuntimeError::InvalidState(format!("Subgraph task panicked: {}", e)),
            },
        }
    }
}

impl Future for SubgraphHandle {
    type Output = SubgraphResult;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.handle).poll(cx) {
            Poll::Ready(Ok(result)) => Poll::Ready(result),
            Poll::Ready(Err(e)) if e.is_cancelled() => Poll::Ready(SubgraphResult::Cancelled {
                subgraph_id: self.subgraph_id.clone(),
            }),
            Poll::Ready(Err(e)) => Poll::Ready(SubgraphResult::Failed {
                subgraph_id: self.subgraph_id.clone(),
                error: RuntimeError::InvalidState(format!("Subgraph task panicked: {}", e)),
            }),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subgraph_result_completed() {
        let result = SubgraphResult::Completed {
            subgraph_id: "test".to_string(),
            state: AgentState::new(),
        };

        assert!(result.is_completed());
        assert!(!result.is_failed());
        assert_eq!(result.subgraph_id(), "test");
        assert!(result.state().is_some());
    }

    #[test]
    fn test_subgraph_result_failed() {
        let result = SubgraphResult::Failed {
            subgraph_id: "test".to_string(),
            error: RuntimeError::RecursionLimit(100),
        };

        assert!(!result.is_completed());
        assert!(result.is_failed());
        assert!(result.error().is_some());
    }

    #[tokio::test]
    async fn test_subgraph_handle() {
        let handle = tokio::spawn(async {
            SubgraphResult::Completed {
                subgraph_id: "test".to_string(),
                state: AgentState::new(),
            }
        });

        let subgraph_handle = SubgraphHandle::new("test".to_string(), handle);
        let result = subgraph_handle.await;

        assert!(result.is_completed());
    }
}
