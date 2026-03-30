// SPDX-License-Identifier: MIT
//! Validation helpers for scroll execution.
//!
//! This module provides type checking and variable validation utilities
//! used during scroll execution to ensure inputs and outputs meet requirements.

use crate::scroll::context::ExecutionContext;
use crate::scroll::error::ExecutionError;
use crate::scroll::schema::TypeConstraint;

// ============================================================================
// Type Checking Helpers
// ============================================================================

/// Check if a value matches a type constraint
pub(crate) fn type_matches(value: &serde_json::Value, constraint: &TypeConstraint) -> bool {
    match constraint {
        TypeConstraint::Any => true,
        TypeConstraint::String => matches!(value, serde_json::Value::String(_)),
        TypeConstraint::Number => matches!(value, serde_json::Value::Number(_)),
        TypeConstraint::Bool => matches!(value, serde_json::Value::Bool(_)),
        TypeConstraint::Sequence => matches!(value, serde_json::Value::Array(_)),
        TypeConstraint::Mapping => matches!(value, serde_json::Value::Object(_)),
    }
}

/// Get human-readable type name for error messages
pub(crate) fn value_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

// ============================================================================
// Variable Validation
// ============================================================================

/// Apply default values for missing required variables.
/// Called BEFORE validation so defaults can satisfy requirements.
pub(crate) fn apply_requires_defaults(
    context: &mut ExecutionContext,
    requires: &Option<std::collections::HashMap<String, crate::scroll::schema::RequireSpec>>,
) {
    let Some(requires) = requires else { return };

    for (name, spec) in requires {
        if context.get_variable(name).is_none() {
            if let Some(default) = &spec.default {
                tracing::debug!(var = %name, "Applying default value");
                context.set_variable(name.clone(), default.clone());
            }
        }
    }
}

/// Validate all required variables are present and correctly typed.
/// Called AFTER defaults are applied.
pub(crate) fn validate_requires(
    context: &ExecutionContext,
    requires: &Option<std::collections::HashMap<String, crate::scroll::schema::RequireSpec>>,
) -> Result<(), ExecutionError> {
    let Some(requires) = requires else { return Ok(()) };

    for (name, spec) in requires {
        match context.get_variable(name) {
            Some(value) => {
                // Type check
                if !type_matches(value, &spec.type_constraint) {
                    return Err(ExecutionError::TypeError(format!(
                        "Variable '{}' expected type {:?}, got {}",
                        name,
                        spec.type_constraint,
                        value_type_name(value)
                    )));
                }
            }
            None => {
                // Missing required variable
                return Err(ExecutionError::MissingRequired(format!(
                    "Scroll requires '{}'{}",
                    name,
                    spec.description
                        .as_ref()
                        .map(|d| format!(": {}", d))
                        .unwrap_or_default()
                )));
            }
        }
    }

    Ok(())
}

/// Validate all promised outputs exist and are correctly typed.
/// Called AFTER all steps complete.
pub(crate) fn validate_provides(
    context: &ExecutionContext,
    provides: &Option<std::collections::HashMap<String, crate::scroll::schema::ProvideSpec>>,
) -> Result<(), ExecutionError> {
    let Some(provides) = provides else { return Ok(()) };

    for (name, spec) in provides {
        match context.get_variable(name) {
            Some(value) => {
                // Type check
                if !type_matches(value, &spec.type_constraint) {
                    return Err(ExecutionError::TypeError(format!(
                        "Promised output '{}' expected type {:?}, got {}",
                        name,
                        spec.type_constraint,
                        value_type_name(value)
                    )));
                }
            }
            None => {
                // Missing promised output
                return Err(ExecutionError::MissingProvided(format!(
                    "Scroll promised '{}' but did not produce it{}",
                    name,
                    spec.description
                        .as_ref()
                        .map(|d| format!(": {}", d))
                        .unwrap_or_default()
                )));
            }
        }
    }

    Ok(())
}
