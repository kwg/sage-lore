// SPDX-License-Identifier: MIT
use std::io;
use thiserror::Error;

/// Result of scroll execution.
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionResult {
    /// Execution completed successfully
    Success,
}

/// Errors that can occur during scroll parsing and validation
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("YAML syntax error: {0}")]
    YamlSyntax(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Steps array cannot be empty")]
    EmptySteps,

    #[error("Potential hardcoded secret detected at line {line}: {description}")]
    HardcodedSecret {
        line: usize,
        description: String,
    },

    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),
}

impl From<serde_yaml::Error> for ParseError {
    fn from(err: serde_yaml::Error) -> Self {
        ParseError::YamlSyntax(err.to_string())
    }
}

/// Errors that can occur during scroll execution.
#[derive(Debug, Clone, Error)]
pub enum ExecutionError {
    /// A step type is not yet implemented.
    #[error("Step type '{0}' is not yet implemented")]
    NotImplemented(String),

    /// Fallback steps executed but produced no result.
    #[error("Fallback steps executed but produced no result")]
    NoFallbackResult,

    /// Variable not found in context.
    #[error("Variable not found: {0}")]
    VariableNotFound(String),

    /// Variable resolution error.
    #[error("Variable resolution error: {0}")]
    VariableResolution(String),

    /// YAML serialization error.
    #[error("YAML serialization error: {0}")]
    YamlSerialization(String),

    /// Interface dispatch error.
    #[error("Interface error: {0}")]
    InterfaceError(String),

    /// Interface invocation error.
    #[error("Interface invocation error: {0}")]
    InvocationError(String),

    /// Invalid interface format (expected "module.method").
    #[error("Invalid interface format: {0}")]
    InvalidInterface(String),

    /// Unknown interface module.
    #[error("Unknown interface module: {0}")]
    UnknownModule(String),

    /// No target specified (neither agent nor interface).
    #[error("No target specified for invoke step")]
    NoTarget,

    /// Missing prompt for agent invocation.
    #[error("Missing prompt for agent invocation")]
    MissingPrompt,

    /// Missing required parameter for interface method.
    #[error("Missing required parameter: {0}")]
    MissingParameter(String),

    /// Value is not iterable (expected sequence).
    #[error("Value is not iterable: expected sequence")]
    NotIterable,

    /// loop_index used outside of a loop context.
    #[error("loop_index can only be used inside a loop")]
    NotInLoop,

    /// Condition evaluation failed.
    #[error("Condition evaluation failed: {0}")]
    ConditionError(String),

    /// Condition evaluation failed (alternate form).
    #[error("condition evaluation failed: {0}")]
    ConditionEvaluation(String),

    /// Aggregation strategy error.
    #[error("Aggregation error: {0}")]
    AggregationError(String),

    /// Unknown aggregate strategy.
    #[error("unknown aggregate strategy: {0}")]
    UnknownAggregateStrategy(String),

    /// Missing required variable.
    #[error("Missing required variable: {0}")]
    MissingRequired(String),

    /// Missing promised output.
    #[error("Missing promised output: {0}")]
    MissingProvided(String),

    /// Type error in validation.
    #[error("Type error: {0}")]
    TypeError(String),

    /// Parse error in validation.
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Timeout error for operations that exceed their time limit.
    #[error("Timeout error: {0}")]
    Timeout(String),

    /// Invalid on_fail strategy for a specific step type.
    #[error("Invalid on_fail strategy: {0}")]
    InvalidOnFail(String),

    /// Task too large for a single LLM call.
    #[error("Task too large for primitive '{primitive}': {reason}")]
    TaskTooLarge {
        primitive: String,
        reason: String,
        partial_output: Option<String>,
    },

    /// Validation error (e.g., consensus validation failed).
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// Validation failed — quality gate did not pass (B1, #180).
    #[error("Validation failed: {summary} (score: {score})")]
    ValidationFailed {
        summary: String,
        score: String,
    },

    /// Unknown agent — not found in the agent registry.
    #[error("Unknown agent '{0}': not found in agent registry")]
    UnknownAgent(String),

    /// Loop break signal (internal, not a real error).
    #[error("break")]
    LoopBreak,

    /// Missing variable (assembly executor).
    #[error("Missing variable: {0}")]
    MissingVariable(String),

    /// Invalid step construction (assembly executor).
    #[error("Invalid step: {0}")]
    InvalidStep(String),
}

/// A single policy violation detected during security gate enforcement.
#[derive(Debug, Clone)]
pub struct PolicyViolation {
    pub step: usize,
    pub rule: String,
    pub message: String,
}

/// Errors related to policy enforcement.
#[derive(Debug, Clone)]
pub enum PolicyError {
    Violations(Vec<PolicyViolation>),
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyError::Violations(violations) => {
                write!(f, "Policy violations: ")?;
                for v in violations {
                    write!(f, "Step {}: {} ({}). ", v.step, v.message, v.rule)?;
                }
                Ok(())
            }
        }
    }
}

impl From<crate::scroll::context::ResolveError> for ExecutionError {
    fn from(err: crate::scroll::context::ResolveError) -> Self {
        ExecutionError::VariableNotFound(err.to_string())
    }
}
