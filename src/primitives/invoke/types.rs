// SPDX-License-Identifier: MIT
//! Core types for LLM invocation.

use serde::{Deserialize, Serialize};

/// Model tier abstraction for routing to appropriate models.
///
/// Tiers represent intent rather than specific models:
/// - Cheap: Fast, low-cost operations (validation, fuzzy checks)
/// - Standard: Balanced quality/cost (most generation work)
/// - Premium: Maximum quality (complex reasoning, final outputs)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelTier {
    /// Fast, low-cost models (e.g., Claude Haiku, Llama 3 8B)
    Cheap,
    /// Balanced quality/cost models (e.g., Claude Sonnet, Llama 3 70B)
    Standard,
    /// Maximum quality models (e.g., Claude Opus, Llama 3 405B)
    Premium,
}

/// Request to generate text from an LLM.
#[derive(Debug, Clone, PartialEq)]
pub struct LlmRequest {
    /// The prompt to send to the model.
    pub prompt: String,
    /// Optional system prompt to set context.
    pub system: Option<String>,
    /// Maximum tokens to generate.
    pub max_tokens: Option<u32>,
    /// Temperature for response randomness (0.0-1.0).
    pub temperature: Option<f32>,
    /// Timeout in seconds for the request.
    pub timeout_secs: Option<u64>,
    /// Model tier for routing to appropriate model.
    pub model_tier: Option<ModelTier>,
    /// JSON schema to constrain output format (Ollama structured output).
    /// When set, replaces the default `"json"` format with a schema object,
    /// enabling grammar-based token masking for guaranteed schema compliance.
    pub format_schema: Option<serde_json::Value>,
    /// Explicit model name override. Takes priority over model_tier and defaults.
    /// Supports any model name the backend recognizes (e.g., "gpt-oss:20b", "phi4-mini").
    pub model: Option<String>,
}

/// Response from an LLM generation request.
#[derive(Debug, Clone, PartialEq)]
pub struct LlmResponse {
    /// The generated text.
    pub text: String,
    /// Number of tokens used (if reported).
    pub tokens_used: Option<u32>,
    /// The model that generated the response.
    pub model: String,
    /// Whether the response was truncated due to max_tokens.
    pub truncated: bool,
}

/// Errors that can occur during LLM invocation.
#[derive(Debug, Clone, thiserror::Error)]
pub enum LlmError {
    #[error("Request timed out")]
    Timeout,

    #[error("CLI tool not found")]
    CliNotFound,

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Failed to parse response: {0}")]
    ParseError(String),

    #[error("No response configured for mock")]
    NotConfigured,

    #[error("IO error: {0}")]
    IoError(String),
}

/// Result type for LLM operations.
pub type LlmResult<T> = Result<T, LlmError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_request_default_like() {
        let req = LlmRequest {
            prompt: "test".to_string(),
            system: None,
            max_tokens: None,
            temperature: None,
            timeout_secs: None,
            model_tier: None,
            format_schema: None,
            model: None,
        };
        assert_eq!(req.prompt, "test");
    }

    #[test]
    fn test_model_tier_variants() {
        // Test all model tier variants exist
        let cheap = ModelTier::Cheap;
        let standard = ModelTier::Standard;
        let premium = ModelTier::Premium;

        assert_eq!(cheap, ModelTier::Cheap);
        assert_eq!(standard, ModelTier::Standard);
        assert_eq!(premium, ModelTier::Premium);
    }

    #[test]
    fn test_model_tier_serialization() {
        // Test serde serialization
        let cheap_yaml = serde_yaml::to_string(&ModelTier::Cheap).unwrap();
        assert_eq!(cheap_yaml.trim(), "cheap");

        let standard_yaml = serde_yaml::to_string(&ModelTier::Standard).unwrap();
        assert_eq!(standard_yaml.trim(), "standard");

        let premium_yaml = serde_yaml::to_string(&ModelTier::Premium).unwrap();
        assert_eq!(premium_yaml.trim(), "premium");
    }

    #[test]
    fn test_model_tier_deserialization() {
        // Test serde deserialization
        let cheap: ModelTier = serde_yaml::from_str("cheap").unwrap();
        assert_eq!(cheap, ModelTier::Cheap);

        let standard: ModelTier = serde_yaml::from_str("standard").unwrap();
        assert_eq!(standard, ModelTier::Standard);

        let premium: ModelTier = serde_yaml::from_str("premium").unwrap();
        assert_eq!(premium, ModelTier::Premium);
    }

    #[test]
    fn test_llm_request_with_tier() {
        let req = LlmRequest {
            prompt: "test".to_string(),
            system: None,
            max_tokens: None,
            temperature: None,
            timeout_secs: None,
            model_tier: Some(ModelTier::Cheap),
            format_schema: None,
            model: None,
        };
        assert_eq!(req.model_tier, Some(ModelTier::Cheap));
    }

    #[test]
    fn test_llm_response_basic() {
        let resp = LlmResponse {
            text: "response".to_string(),
            tokens_used: Some(10),
            model: "test-model".to_string(),
            truncated: false,
        };
        assert_eq!(resp.text, "response");
        assert_eq!(resp.tokens_used, Some(10));
    }

    #[test]
    fn test_llm_error_variants() {
        let err = LlmError::Timeout;
        assert!(!err.to_string().is_empty());

        let err = LlmError::CliNotFound;
        assert!(!err.to_string().is_empty());

        let err = LlmError::ExecutionFailed("test".to_string());
        assert!(err.to_string().contains("test"));

        let err = LlmError::ParseError("bad json".to_string());
        assert!(err.to_string().contains("bad json"));

        let err = LlmError::NotConfigured;
        assert!(!err.to_string().is_empty());
    }
}
