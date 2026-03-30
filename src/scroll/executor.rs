// SPDX-License-Identifier: MIT
//! Executor module for SAGE Method scroll execution.
//!
//! This module provides the core executor that orchestrates scroll execution,
//! dispatching steps to their respective handlers and managing execution context.

use std::path::Path;

use crate::config::SecretResolver;
use crate::scroll::context::ExecutionContext;
use crate::scroll::error::{ExecutionError, ExecutionResult};
use crate::scroll::interfaces::{InterfaceRegistry, RegistryError};
use crate::scroll::policy::PolicyEnforcer;
use crate::scroll::schema::{Scroll, Step};
use crate::scroll::validation::{apply_requires_defaults, validate_provides, validate_requires};

/// Format a duration as a human-readable string.
pub(crate) fn format_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs >= 60 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else if secs > 0 {
        format!("{}.{}s", secs, d.subsec_millis() / 100)
    } else {
        format!("{}ms", d.as_millis())
    }
}

// ============================================================================
// with_on_fail! Macro (Pattern 8 — replaces execute_with_on_fail)
// ============================================================================

/// Macro that replaces the former `execute_with_on_fail` function.
///
/// The old function took an `FnMut(&mut Self)` closure, which cannot be made async
/// (Rust closures cannot be `async FnMut` without boxing, and the closure captures
/// `&mut self` which conflicts with the `&mut self` receiver). This macro expands
/// the on-fail logic inline at each call site, eliminating the closure entirely.
///
/// `$body` is re-evaluated on each retry iteration, creating fresh futures.
/// `self` is used directly — no capture, no borrow conflict.
/// `Box::pin` appears only in the Fallback arm for recursive async (D61).
macro_rules! with_on_fail {
    ($self:ident, $on_fail:expr, $body:expr) => {{
        let on_fail_ref = $on_fail;
        let max_attempts: usize = match on_fail_ref {
            OnFail::Retry(config) => (config.max + 1) as usize,
            _ => 1,
        };
        let mut last_result = Err(ExecutionError::InvocationError("unreachable".into()));
        for _attempt in 0..max_attempts {
            last_result = (async { $body }).await;
            match &last_result {
                Ok(_) => break,
                Err(_) if _attempt < max_attempts - 1 => {
                    tracing::info!(attempt = _attempt + 1, max = max_attempts, "Retrying step");
                    continue;
                }
                Err(_) => break,
            }
        }
        match last_result {
            Ok(val) => Ok(val),
            Err(e) => match on_fail_ref {
                OnFail::Halt | OnFail::Retry(_) => Err(e),
                OnFail::Continue => {
                    tracing::warn!(error = ?e, "Step failed, continuing");
                    write_failure_diagnostic(&e);
                    Ok(serde_json::Value::Null)
                }
                OnFail::CollectErrors => Err(ExecutionError::InvalidOnFail(
                    "collect_errors is only valid for concurrent steps".into(),
                )),
                OnFail::Fallback(steps) => {
                    tracing::info!("Executing fallback steps");
                    for step in steps {
                        Box::pin($self.execute_step(step)).await?;
                    }
                    $self.context.prev().cloned()
                        .ok_or(ExecutionError::NoFallbackResult)
                }
            },
        }
    }};
}

pub(crate) use with_on_fail;

/// Write a failure diagnostic file when `on_fail: continue` triggers.
/// Evidence is written to `.sage-lore/failures/` so pipeline failures
/// are captured for post-run review instead of silently vanishing.
pub(crate) fn write_failure_diagnostic(error: &crate::scroll::error::ExecutionError) {
    use std::io::Write;

    let dir = std::path::Path::new(".sage-lore/failures");
    if std::fs::create_dir_all(dir).is_err() {
        tracing::debug!("Could not create failures directory");
        return;
    }

    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S%.3f");
    let filename = dir.join(format!("{timestamp}.yaml"));

    let content = format!(
        "# on_fail: continue — failure captured\ntimestamp: \"{timestamp}\"\nerror: |\n  {error}\n",
    );

    match std::fs::File::create(&filename) {
        Ok(mut f) => {
            let _ = f.write_all(content.as_bytes());
            tracing::info!(file = %filename.display(), "Failure diagnostic written");
        }
        Err(e) => {
            tracing::debug!(error = %e, "Could not write failure diagnostic");
        }
    }
}

// ============================================================================
// Executor
// ============================================================================

/// Core executor for scroll execution.
///
/// The executor dispatches steps to their respective handlers and manages
/// execution context through the scroll lifecycle.
pub struct Executor {
    /// Execution context for variable resolution
    pub(crate) context: ExecutionContext,
    /// Registry for interface implementations
    pub(crate) interface_registry: InterfaceRegistry,
    /// Policy enforcer for security constraints, enforcement planned for v1.1
    pub(crate) policy_enforcer: PolicyEnforcer,
    /// Token usage tracking for context budget monitoring
    pub(crate) tokens_used: usize,
    /// Maximum token limit (from SAGE_CONTEXT_LIMIT env, default 100k)
    pub(crate) tokens_limit: usize,
    /// Path resolver for scroll search path resolution (D18, #178)
    pub(crate) path_resolver: Option<crate::config::PathResolver>,
}

/// Read SAGE_CONTEXT_LIMIT from env, defaulting to 100k tokens (SF1, #185).
fn default_tokens_limit() -> usize {
    std::env::var("SAGE_CONTEXT_LIMIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000)
}

impl Executor {
    /// Create a new executor with empty context and placeholder registries.
    pub fn new() -> Self {
        let tokens_limit = default_tokens_limit();

        Self {
            context: ExecutionContext::new(),
            interface_registry: InterfaceRegistry::new(),
            policy_enforcer: PolicyEnforcer::default(),
            tokens_used: 0,
            tokens_limit,
            path_resolver: None,
        }
    }

    /// Create an executor with mock backends for testing.
    ///
    /// Uses MockFsBackend, MockPlatform, MockLlmBackend, and NoopBackend.
    /// Does not require environment variables or policy files.
    pub fn for_testing() -> Self {
        let tokens_limit = default_tokens_limit();

        Self {
            context: ExecutionContext::new(),
            interface_registry: InterfaceRegistry::for_testing(),
            policy_enforcer: PolicyEnforcer::default(),
            tokens_used: 0,
            tokens_limit,
            path_resolver: None,
        }
    }

    /// Create an executor with a custom interface registry.
    ///
    /// Use this for integration tests that need real backends (e.g., Ollama)
    /// with mock backends for other interfaces (fs, platform, etc.).
    pub fn with_registry(interface_registry: InterfaceRegistry) -> Self {
        let tokens_limit = default_tokens_limit();

        Self {
            context: ExecutionContext::new(),
            interface_registry,
            policy_enforcer: PolicyEnforcer::default(),
            tokens_used: 0,
            tokens_limit,
            path_resolver: None,
        }
    }

    /// Create an executor with mock backends and explicit canned LLM responses.
    ///
    /// Use this when testing scrolls that expect specific output schemas from
    /// LLM calls. The canned responses map prompt substrings to response text.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let executor = Executor::for_testing_with_llm_responses(vec![
    ///     ("convert".to_string(), "files:\n  - path: test.rs\n".to_string()),
    /// ]);
    /// ```
    pub fn for_testing_with_llm_responses(responses: Vec<(String, String)>) -> Self {
        let tokens_limit = default_tokens_limit();

        Self {
            context: ExecutionContext::new(),
            interface_registry: InterfaceRegistry::for_testing_with_llm_responses(responses),
            policy_enforcer: PolicyEnforcer::default(),
            tokens_used: 0,
            tokens_limit,
            path_resolver: None,
        }
    }

    /// Create an executor with production backends from a project root.
    ///
    /// Initializes all backends with real implementations (ClaudeCliBackend,
    /// Git2Backend, SecureFsBackend, etc.) using proper dependency injection.
    /// Also sets up SecretResolver for secret injection.
    pub fn from_project(project_root: &Path) -> Result<Self, RegistryError> {
        // Create secret resolver for the project
        let secret_resolver = SecretResolver::new(project_root);

        let tokens_limit = default_tokens_limit();

        let path_resolver = crate::config::PathResolver::discover(project_root);

        Ok(Self {
            context: ExecutionContext::with_secret_resolver(secret_resolver),
            interface_registry: InterfaceRegistry::from_project(project_root)?,
            policy_enforcer: PolicyEnforcer::default(),
            tokens_used: 0,
            tokens_limit,
            path_resolver: Some(path_resolver),
        })
    }

    /// Get a mutable reference to the execution context.
    ///
    /// This allows setting up variables before scroll execution.
    pub fn context_mut(&mut self) -> &mut ExecutionContext {
        &mut self.context
    }

    /// Get a reference to the execution context.
    pub fn context(&self) -> &ExecutionContext {
        &self.context
    }

    /// Resolve a string parameter, handling ${var} references.
    ///
    /// If the string starts with "${", resolves it from context.
    /// Otherwise returns the string as-is.
    pub fn resolve_string_param(&self, param: &str) -> Result<String, ExecutionError> {
        if param.starts_with("${") && param.ends_with('}') && param.matches("${").count() == 1 {
            // Full-value reference: entire string is a single "${var}"
            let value = self.context.resolve(param)?;
            match &value {
                serde_json::Value::String(s) => Ok(s.clone()),
                serde_json::Value::Number(n) => Ok(n.to_string()),
                serde_json::Value::Bool(b) => Ok(b.to_string()),
                other => Err(ExecutionError::VariableResolution(format!(
                    "expected string-coercible value, got {:?}",
                    other
                ))),
            }
        } else if param.contains("${") {
            // Embedded references: "text ${var} more text"
            self.interpolate_string(param)
        } else {
            Ok(param.to_string())
        }
    }

    /// Interpolate a string with multiple ${var} references.
    ///
    /// Replaces all ${...} patterns in the string with their resolved values.
    /// Supports nested paths like ${foo.bar.baz}.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Given context with project_root="/tmp" and file.path="src/main.rs"
    /// let result = executor.interpolate_string("${project_root}/${file.path}");
    /// // Returns: "/tmp/src/main.rs"
    /// ```
    /// Evaluate a condition expression. Supports:
    /// - `"${var}"` — truthy/falsy check (backwards compatible)
    /// - `"${var} == 'value'"` — string/numeric equality
    /// - `"${var} != 'value'"` — string/numeric inequality
    /// - `"${var} == ${other}"` — variable-to-variable comparison
    /// - `"${var} >= 0.8"` — numeric comparison (>=, <=, >, <)
    pub fn evaluate_condition(&self, condition: &str) -> bool {
        // Parse order: 2-char operators before 1-char to avoid substring conflicts
        // !=, ==, >=, <= THEN >, <
        let (left, op, right) = if let Some(pos) = condition.find("!=") {
            let left = condition[..pos].trim();
            let right = condition[pos + 2..].trim();
            (left, "!=", right)
        } else if let Some(pos) = condition.find("==") {
            let left = condition[..pos].trim();
            let right = condition[pos + 2..].trim();
            (left, "==", right)
        } else if let Some(pos) = condition.find(">=") {
            let left = condition[..pos].trim();
            let right = condition[pos + 2..].trim();
            (left, ">=", right)
        } else if let Some(pos) = condition.find("<=") {
            let left = condition[..pos].trim();
            let right = condition[pos + 2..].trim();
            (left, "<=", right)
        } else if let Some(pos) = condition.find('>') {
            let left = condition[..pos].trim();
            let right = condition[pos + 1..].trim();
            (left, ">", right)
        } else if let Some(pos) = condition.find('<') {
            let left = condition[..pos].trim();
            let right = condition[pos + 1..].trim();
            (left, "<", right)
        } else {
            // No operator — fall back to truthy/falsy on the whole expression
            let val = self.context.resolve(condition)
                .unwrap_or(serde_json::Value::Bool(false));
            return super::step_dispatch::is_truthy(&val);
        };

        let left_val = super::step_dispatch::resolve_condition_operand(left, &self.context);
        let right_val = super::step_dispatch::resolve_condition_operand(right, &self.context);

        match op {
            "==" => super::step_dispatch::values_equal(&left_val, &right_val),
            "!=" => !super::step_dispatch::values_equal(&left_val, &right_val),
            ">=" | "<=" | ">" | "<" => {
                super::step_dispatch::values_compare(&left_val, &right_val, op)
            }
            _ => false,
        }
    }

    pub fn interpolate_string(&self, template: &str) -> Result<String, ExecutionError> {
        let mut result = String::new();
        let mut remaining = template;

        while let Some(start) = remaining.find("${") {
            // Add text before the variable reference
            result.push_str(&remaining[..start]);

            // Find matching closing brace (handle nested braces if needed)
            let after_start = &remaining[start + 2..];
            let end = after_start.find('}')
                .ok_or_else(|| ExecutionError::VariableResolution(
                    format!("Unclosed variable reference in: {}", template)
                ))?;

            // Extract and resolve the variable reference
            let var_ref = &remaining[start..start + 2 + end + 1]; // "${...}"
            let value = self.context.resolve(var_ref)?;

            // Convert value to string
            let value_str = match value {
                serde_json::Value::String(s) => s,
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Null => "null".to_string(),
                _ => serde_json::to_string(&value)
                    .map_err(|e| ExecutionError::YamlSerialization(e.to_string()))?
                    .trim().to_string(),
            };
            result.push_str(&value_str);

            // Continue with the rest of the string
            remaining = &remaining[start + 2 + end + 1..];
        }

        // Add any remaining text after the last variable
        result.push_str(remaining);

        Ok(result)
    }

    /// Resolve a parameter to its raw Value, preserving type.
    ///
    /// If the string starts with "${", resolves it from context and returns
    /// the value unchanged. Otherwise wraps the literal string as Value::String.
    ///
    /// Use this for parameters where the downstream interface handles type
    /// conversion (e.g., numeric parameters for platform operations).
    pub fn resolve_param(&self, param: &str) -> Result<serde_json::Value, ExecutionError> {
        if param.starts_with("${") {
            Ok(self.context.resolve(param)?)
        } else {
            Ok(serde_json::Value::String(param.to_string()))
        }
    }

    /// Track token usage for context budget monitoring.
    ///
    /// Estimates tokens based on character count (~4 chars/token) if actual
    /// token counts are not available from the backend.
    pub fn track_tokens(&mut self, prompt: &str, response: &str) {
        // Estimate tokens: ~4 characters per token
        let prompt_tokens = prompt.len() / 4;
        let response_tokens = response.len() / 4;
        self.tokens_used += prompt_tokens + response_tokens;

        tracing::debug!(
            prompt_tokens = prompt_tokens,
            response_tokens = response_tokens,
            total_used = self.tokens_used,
            limit = self.tokens_limit,
            "Token usage tracked"
        );
    }

    /// Execute a scroll by running all steps in order.
    pub async fn execute_scroll(&mut self, scroll: &Scroll) -> Result<ExecutionResult, ExecutionError> {
        let start = std::time::Instant::now();
        let step_count = scroll.steps.len();
        tracing::info!(
            scroll = %scroll.scroll,
            steps = step_count,
            "Starting scroll execution"
        );

        // ============================================================
        // PRE-EXECUTION VALIDATION
        // 1. Apply defaults first (so they can satisfy requirements)
        // 2. Then validate all requires are present and typed correctly
        // ============================================================
        apply_requires_defaults(&mut self.context, &scroll.requires);
        validate_requires(&self.context, &scroll.requires)?;

        // ============================================================
        // EXECUTE ALL STEPS
        // ============================================================
        for (index, step) in scroll.steps.iter().enumerate() {
            tracing::info!(
                scroll = %scroll.scroll,
                step = index + 1,
                total = step_count,
                kind = step.kind(),
                "Executing step"
            );
            Box::pin(self.execute_step(step)).await?;
        }

        // ============================================================
        // POST-EXECUTION VALIDATION
        // Verify all promised outputs were produced
        // ============================================================
        validate_provides(&self.context, &scroll.provides)?;

        let elapsed = start.elapsed();
        tracing::info!(
            scroll = %scroll.scroll,
            elapsed = format_duration(elapsed).as_str(),
            "Scroll execution completed"
        );
        Ok(ExecutionResult::Success)
    }

    /// Execute a single step by dispatching to the appropriate handler.
    ///
    /// Each step handler is responsible for binding its own output to the context
    /// via `context.set_variable(output_name, result)`.
    pub async fn execute_step(&mut self, step: &Step) -> Result<(), ExecutionError> {
        match step {
            Step::Elaborate(s) => { self.execute_elaborate(s).await?; Ok(()) },
            Step::Distill(s) => { self.execute_distill(s).await?; Ok(()) },
            Step::Split(s) => { self.execute_split(s).await?; Ok(()) },
            Step::Merge(s) => { self.execute_merge(s).await?; Ok(()) },
            Step::Validate(s) => { self.execute_validate(s).await?; Ok(()) },
            Step::Convert(s) => { self.execute_convert(s).await?; Ok(()) },
            Step::Fs(s) => self.execute_fs(s).await,
            Step::Vcs(s) => self.execute_vcs(s).await,
            Step::Test(s) => self.execute_test(s).await,
            Step::Platform(s) => self.execute_platform(s).await,
            Step::Run(s) => self.execute_run(s).await,
            Step::Invoke(s) => self.execute_invoke(s).await,
            Step::Parallel(s) => self.execute_parallel(s).await,
            Step::Consensus(s) => self.execute_consensus(s).await,
            Step::Concurrent(s) => self.execute_concurrent(s).await,
            Step::Branch(s) => self.execute_branch(s).await,
            Step::Loop(s) => self.execute_loop(s).await,
            Step::Aggregate(s) => self.execute_aggregate(s).await,
            Step::Set(s) => self.execute_set(s).await,
            Step::Secure(s) => self.execute_secure(s).await,
        }
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}
