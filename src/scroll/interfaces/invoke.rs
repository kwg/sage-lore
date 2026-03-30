// SPDX-License-Identifier: MIT
//! Invoke interface adapter for LLM/agent calls.

use async_trait::async_trait;
use std::sync::Arc;

use crate::primitives::invoke::LlmBackend;
use crate::scroll::error::ExecutionError;
use super::InterfaceDispatch;

/// Check if an error message indicates a context limit / task too large error.
///
/// Detects common patterns from various LLM backends:
/// - Claude: "maximum context length", "context_length_exceeded"
/// - Ollama: "context length exceeded", "maximum context"
/// - Generic: "too large", "exceeds", "limit"
fn is_context_limit_error(error_msg: &str) -> bool {
    let error_lower = error_msg.to_lowercase();
    error_lower.contains("context length")
        || error_lower.contains("context_length_exceeded")
        || error_lower.contains("maximum context")
        || error_lower.contains("max context")
        || error_lower.contains("exceeds maximum")
        || error_lower.contains("too many tokens")
        || error_lower.contains("prompt is too long")
}

/// Invoke interface for LLM generation and agent calls.
#[derive(Clone)]
pub struct InvokeInterface {
    backend: Option<Arc<dyn LlmBackend>>,
}

impl InvokeInterface {
    /// Create a stub interface (for unit tests).
    pub fn new() -> Self {
        Self { backend: None }
    }

    /// Create an interface with a specific backend.
    pub fn with_backend(backend: Arc<dyn LlmBackend>) -> Self {
        Self { backend: Some(backend) }
    }

    /// Get the backend for consensus validation or other direct LLM access.
    /// Returns a mock backend if no backend is configured.
    pub fn backend(&self) -> Arc<dyn LlmBackend> {
        match &self.backend {
            Some(backend) => Arc::clone(backend),
            None => {
                // Return mock backend for tests
                use crate::primitives::invoke::{LlmResponse, MockLlmBackend};
                Arc::new(MockLlmBackend::new().with_default_response(LlmResponse {
                    text: "PASS\nExplanation: Mock validation passed".to_string(),
                    tokens_used: Some(10),
                    model: "mock".to_string(),
                    truncated: false,
                }))
            }
        }
    }
}

impl Default for InvokeInterface {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InterfaceDispatch for InvokeInterface {
    async fn dispatch(
        &self,
        method: &str,
        params: &Option<serde_json::Value>,
    ) -> Result<serde_json::Value, ExecutionError> {
        match method {
            "generate" => {
                let prompt = params.as_ref()
                    .and_then(|p| p.get("prompt"))
                    .and_then(|v| v.as_str())
                    .ok_or(ExecutionError::MissingPrompt)?;

                let timeout_secs = params.as_ref()
                    .and_then(|p| p.get("timeout_secs"))
                    .and_then(|v| v.as_u64());

                // Parse model_tier from params (cheap/standard/premium)
                let model_tier = params.as_ref()
                    .and_then(|p| p.get("model_tier"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| match s {
                        "cheap" => Some(crate::primitives::invoke::ModelTier::Cheap),
                        "standard" => Some(crate::primitives::invoke::ModelTier::Standard),
                        "premium" => Some(crate::primitives::invoke::ModelTier::Premium),
                        _ => None,
                    });

                // Parse explicit model name from params
                let model = params.as_ref()
                    .and_then(|p| p.get("model"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                // Parse format_schema from params (JSON schema for structured output)
                let format_schema = params.as_ref()
                    .and_then(|p| p.get("format_schema"))
                    .cloned();

                // Parse system prompt from params
                let system = params.as_ref()
                    .and_then(|p| p.get("system"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let request = crate::primitives::invoke::LlmRequest {
                    prompt: prompt.to_string(),
                    system,
                    max_tokens: None,
                    temperature: None,
                    timeout_secs: timeout_secs.map(|s| s.min(1200)),
                    model_tier,
                    format_schema,
                    model,
                };

                let response = match &self.backend {
                    Some(backend) => backend.generate(request).await,
                    None => {
                        // Fallback for tests without injected backend
                        use crate::primitives::invoke::{LlmResponse, MockLlmBackend};
                        MockLlmBackend::new()
                            .with_default_response(LlmResponse {
                                text: "Mock LLM response".to_string(),
                                tokens_used: Some(10),
                                model: "mock".to_string(),
                                truncated: false,
                            })
                            .generate(request).await
                    }
                }.map_err(|e| {
                    // Check for context limit / task too large errors
                    let error_msg = e.to_string();
                    if is_context_limit_error(&error_msg) {
                        ExecutionError::TaskTooLarge {
                            primitive: "generate".to_string(),
                            reason: error_msg.clone(),
                            partial_output: None,
                        }
                    } else {
                        ExecutionError::InvocationError(error_msg)
                    }
                })?;

                // Check if response was truncated (another indicator of task being too large)
                if response.truncated {
                    return Err(ExecutionError::TaskTooLarge {
                        primitive: "generate".to_string(),
                        reason: "Response was truncated due to max tokens limit".to_string(),
                        partial_output: Some(response.text),
                    });
                }

                Ok(serde_json::Value::String(response.text))
            }
            "chat" | "embed" => {
                Err(ExecutionError::NotImplemented(format!("invoke.{}", method)))
            }
            _ => Err(ExecutionError::InterfaceError(format!(
                "unknown invoke method: {}",
                method
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::invoke::{LlmError, LlmResponse, MockLlmBackend, MockResponseKey};

    #[test]
    fn test_is_context_limit_error_detects_various_patterns() {
        // Claude-style errors
        assert!(is_context_limit_error("maximum context length exceeded"));
        assert!(is_context_limit_error("Error: context_length_exceeded"));

        // Ollama-style errors
        assert!(is_context_limit_error("context length exceeded"));
        assert!(is_context_limit_error("maximum context size reached"));

        // Generic patterns
        assert!(is_context_limit_error("prompt is too long"));
        assert!(is_context_limit_error("too many tokens in prompt"));
        assert!(is_context_limit_error("exceeds maximum allowed length"));

        // Should not match unrelated errors
        assert!(!is_context_limit_error("network timeout"));
        assert!(!is_context_limit_error("invalid API key"));
        assert!(!is_context_limit_error("rate limit exceeded"));
    }

    #[tokio::test]
    async fn test_invoke_interface_returns_task_too_large_on_context_error() {
        // Create a mock that returns a context limit error
        let mock = MockLlmBackend::new()
            .with_response(
                MockResponseKey::Generate,
                Err(LlmError::ExecutionFailed("maximum context length exceeded".to_string()))
            );

        let interface = InvokeInterface::with_backend(Arc::new(mock));

        let params = Some(serde_json::json!({"prompt": "test prompt"}));

        let result = interface.dispatch("generate", &params).await;

        // Should return TaskTooLarge error, not InvocationError
        match result {
            Err(ExecutionError::TaskTooLarge { primitive, reason, partial_output }) => {
                assert_eq!(primitive, "generate");
                assert!(reason.contains("maximum context length"));
                assert!(partial_output.is_none());
            }
            other => panic!("Expected TaskTooLarge error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_invoke_interface_returns_task_too_large_on_truncated_response() {
        // Create a mock that returns a truncated response
        let mock = MockLlmBackend::new()
            .with_response(
                MockResponseKey::Generate,
                Ok(LlmResponse {
                    text: "Partial response...".to_string(),
                    tokens_used: Some(4096),
                    model: "test-model".to_string(),
                    truncated: true,
                })
            );

        let interface = InvokeInterface::with_backend(Arc::new(mock));

        let params = Some(serde_json::json!({"prompt": "test prompt"}));

        let result = interface.dispatch("generate", &params).await;

        // Should return TaskTooLarge error with partial output
        match result {
            Err(ExecutionError::TaskTooLarge { primitive, reason, partial_output }) => {
                assert_eq!(primitive, "generate");
                assert!(reason.contains("truncated"));
                assert_eq!(partial_output, Some("Partial response...".to_string()));
            }
            other => panic!("Expected TaskTooLarge error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_invoke_interface_returns_invocation_error_for_other_errors() {
        // Create a mock that returns a non-context-limit error
        let mock = MockLlmBackend::new()
            .with_response(
                MockResponseKey::Generate,
                Err(LlmError::Timeout)
            );

        let interface = InvokeInterface::with_backend(Arc::new(mock));

        let params = Some(serde_json::json!({"prompt": "test prompt"}));

        let result = interface.dispatch("generate", &params).await;

        // Should return InvocationError, not TaskTooLarge
        match result {
            Err(ExecutionError::InvocationError(msg)) => {
                assert!(msg.contains("timed out") || msg.contains("Timeout"));
            }
            other => panic!("Expected InvocationError, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_invoke_interface_successful_generation() {
        // Create a mock with successful response
        let mock = MockLlmBackend::new()
            .with_response(
                MockResponseKey::Generate,
                Ok(LlmResponse {
                    text: "Generated response".to_string(),
                    tokens_used: Some(50),
                    model: "test-model".to_string(),
                    truncated: false,
                })
            );

        let interface = InvokeInterface::with_backend(Arc::new(mock));

        let params = Some(serde_json::json!({"prompt": "test prompt"}));

        let result = interface.dispatch("generate", &params).await;

        match result {
            Ok(serde_json::Value::String(text)) => {
                assert_eq!(text, "Generated response");
            }
            other => panic!("Expected successful string response, got: {:?}", other),
        }
    }
}
