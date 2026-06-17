//! Graph runner that records transitions and validates state.

use std::sync::{Arc, RwLock};
use tracing::{debug, info, instrument};

use crate::error::RuntimeError;
use crate::graph::{transitions, CompiledGraph};
use crate::runner::RunnerConfig;
use crate::state::AgentState;

use super::transition::{
    attach_run_context, shared_transition_log, RunContext, SharedTransitionLog, TransitionRecord,
};
use super::validation::{StateValidator, ValidationError};

/// Result of a traced graph run.
#[derive(Debug)]
pub struct TracedRunResult {
    /// Final agent state
    pub state: AgentState,
    /// Run correlation context
    pub run_context: RunContext,
    /// Recorded transitions
    pub transition_log: TransitionLog,
}

use super::transition::TransitionLog;

/// Graph runner with transition logging and optional state validation.
pub struct TracedRunner {
    graph: Arc<CompiledGraph>,
    config: RunnerConfig,
    run_context: RunContext,
    transition_log: SharedTransitionLog,
    validator: Option<StateValidator>,
}

impl TracedRunner {
    /// Create a traced runner with default config and a new run context.
    pub fn new(graph: CompiledGraph) -> Self {
        Self::with_context(graph, RunContext::new(), RunnerConfig::default())
    }

    /// Create a traced runner with explicit run context and config.
    pub fn with_context(
        graph: impl Into<Arc<CompiledGraph>>,
        run_context: RunContext,
        config: RunnerConfig,
    ) -> Self {
        Self {
            graph: graph.into(),
            config,
            run_context,
            transition_log: shared_transition_log(),
            validator: None,
        }
    }

    /// Enable state validation after each node.
    pub fn with_validator(mut self, validator: StateValidator) -> Self {
        self.validator = Some(validator);
        self
    }

    /// Access the shared transition log (for inspection during run).
    pub fn transition_log(&self) -> SharedTransitionLog {
        self.transition_log.clone()
    }

    /// Access run context.
    pub fn run_context(&self) -> &RunContext {
        &self.run_context
    }

    /// Execute the graph and return state plus trace artifacts.
    #[instrument(skip(self, initial_state), fields(run_id = %self.run_context.run_id))]
    pub async fn invoke(
        &self,
        mut initial_state: AgentState,
    ) -> Result<TracedRunResult, RuntimeError> {
        attach_run_context(&mut initial_state, &self.run_context);
        if let Some(ref validator) = self.validator {
            validator
                .validate(&initial_state)
                .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;
        }

        // Reset the log so each invocation's result contains only this run's
        // records. `invoke` takes `&self`, so a runner can be reused; without
        // this clear, run B's TracedRunResult would carry run A's transitions.
        {
            let mut log = self
                .transition_log
                .write()
                .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;
            log.clear();
        }

        let state = Arc::new(RwLock::new(initial_state));
        let final_state = self.run_loop(state).await?;

        let log = self
            .transition_log
            .read()
            .map_err(|e| RuntimeError::InvalidState(e.to_string()))?
            .clone();

        Ok(TracedRunResult {
            state: final_state,
            run_context: self.run_context.clone(),
            transition_log: log,
        })
    }

    async fn run_loop(&self, state: Arc<RwLock<AgentState>>) -> Result<AgentState, RuntimeError> {
        let mut current_node = self.graph.entry_point().to_string();
        let mut iterations: u32 = 0;

        info!(
            run_id = %self.run_context.run_id,
            entry_point = %current_node,
            "Starting traced graph execution"
        );

        loop {
            if current_node == transitions::END {
                info!(run_id = %self.run_context.run_id, iterations, "Traced execution completed");
                let guard = state
                    .read()
                    .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;
                return Ok(guard.clone());
            }

            if iterations >= self.config.max_iterations {
                return Err(RuntimeError::RecursionLimit(self.config.max_iterations));
            }

            let node = self
                .graph
                .get_node(&current_node)
                .ok_or_else(|| RuntimeError::NodeNotFound(current_node.clone()))?;

            debug!(run_id = %self.run_context.run_id, node_id = %current_node, iteration = iterations);

            let output = node
                .executor
                .execute(state.clone())
                .await
                .map_err(|e| RuntimeError::node_failed(&current_node, e))?;

            {
                let mut guard = state
                    .write()
                    .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;
                guard.increment_iteration();
            }

            if let Some(ref validator) = self.validator {
                let guard = state
                    .read()
                    .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;
                validator
                    .validate(&guard)
                    .map_err(|e: ValidationError| RuntimeError::InvalidState(e.to_string()))?;
            }

            let next_node = {
                let current_state = state
                    .read()
                    .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;
                self.graph
                    .resolve_next_node(&current_node, &output, &current_state)
            };
            let state_iteration = state
                .read()
                .map_err(|e| RuntimeError::InvalidState(e.to_string()))?
                .iteration;

            let record = TransitionRecord::from_step(
                &self.run_context.run_id,
                iterations,
                &current_node,
                &output,
                if next_node == transitions::END {
                    None
                } else {
                    Some(next_node.as_str())
                },
                state_iteration,
            );

            {
                let mut log = self
                    .transition_log
                    .write()
                    .map_err(|e| RuntimeError::InvalidState(e.to_string()))?;
                log.push(record);
            }

            iterations += 1;
            current_node = next_node;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::NodeError;
    use crate::graph::{GraphBuilder, NodeExecutor, NodeOutput};
    use crate::state::SharedState;
    use async_trait::async_trait;

    struct SetFlagNode;

    #[async_trait]
    impl NodeExecutor for SetFlagNode {
        fn id(&self) -> &str {
            "set_flag"
        }

        async fn execute(&self, state: SharedState) -> Result<NodeOutput, NodeError> {
            let mut guard = state
                .write()
                .map_err(|e| NodeError::execution_failed(e.to_string()))?;
            guard.set_context("done", true);
            Ok(NodeOutput::cont())
        }
    }

    #[tokio::test]
    async fn test_traced_runner_records_transitions() {
        let graph = GraphBuilder::new()
            .add_node(SetFlagNode)
            .set_entry_point("set_flag")
            .add_edge_to_end("set_flag")
            .compile()
            .unwrap();

        let ctx = RunContext::with_ids("run-test", "thread-test");
        let runner = TracedRunner::with_context(Arc::new(graph), ctx, RunnerConfig::default());
        let result = runner.invoke(AgentState::new()).await.unwrap();

        assert_eq!(result.run_context.run_id, "run-test");
        assert_eq!(result.transition_log.len(), 1);
        assert_eq!(result.transition_log.records()[0].node_id, "set_flag");
        assert_eq!(result.state.get_context::<bool>("done"), Some(true));
        assert_eq!(
            result.state.get_context::<String>("run_id"),
            Some("run-test".to_string())
        );
    }
}
