//! State validation at graph transitions.

use serde_json::Value;
use thiserror::Error;

use crate::state::{AgentState, State};

/// Validation failure at a transition boundary.
#[derive(Error, Debug, PartialEq)]
pub enum ValidationError {
    /// A required context key is missing
    #[error("Missing required context key: {0}")]
    MissingKey(String),

    /// A context value failed a predicate
    #[error("Invalid context value for '{key}': {reason}")]
    InvalidValue {
        /// Context key
        key: String,
        /// Human-readable reason
        reason: String,
    },
}

/// Validates agent state before/after node execution.
#[derive(Clone, Debug, Default)]
pub struct StateValidator {
    required_keys: Vec<String>,
}

impl StateValidator {
    /// Create a validator with no requirements.
    pub fn new() -> Self {
        Self::default()
    }

    /// Require a context key to be present after validation.
    pub fn require_key(mut self, key: impl Into<String>) -> Self {
        self.required_keys.push(key.into());
        self
    }

    /// Validate state against required keys and optional schema checks.
    pub fn validate(&self, state: &AgentState) -> Result<(), ValidationError> {
        for key in &self.required_keys {
            if !state.context.contains_key(key) {
                return Err(ValidationError::MissingKey(key.clone()));
            }
        }

        // Ensure state conforms to AgentState schema shape when non-empty
        let schema = AgentState::schema();
        if let Some(required) = schema.get("required").and_then(|v| v.as_array()) {
            for item in required {
                if let Some(prop) = item.as_str() {
                    if prop == "context" {
                        continue;
                    }
                    if !state_has_property(state, prop) {
                        return Err(ValidationError::MissingKey(prop.to_string()));
                    }
                }
            }
        }

        Ok(())
    }
}

fn state_has_property(state: &AgentState, prop: &str) -> bool {
    match prop {
        "messages" => true,
        "tool_calls" => true,
        "context" => true,
        "iteration" => true,
        "is_complete" => true,
        key => state.context.contains_key(key),
    }
}

/// Validate a JSON value against a simple type constraint.
#[allow(dead_code)]
pub fn validate_json_type(value: &Value, expected_type: &str) -> Result<(), ValidationError> {
    let ok = match expected_type {
        "string" => value.is_string(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => true,
    };
    if ok {
        Ok(())
    } else {
        Err(ValidationError::InvalidValue {
            key: "value".to_string(),
            reason: format!("expected type {expected_type}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_required_key_validation() {
        let validator = StateValidator::new().require_key("gate_passed");
        let state = AgentState::new();
        assert!(matches!(
            validator.validate(&state),
            Err(ValidationError::MissingKey(_))
        ));

        let mut state = AgentState::new();
        state.set_context("gate_passed", true);
        assert!(validator.validate(&state).is_ok());
    }
}
