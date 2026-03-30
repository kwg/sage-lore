// SPDX-License-Identifier: MIT
//! Ollama backend for local LLM invocation.

use std::time::Duration;

use super::r#trait::LlmBackend;
use super::types::{LlmError, LlmRequest, LlmResponse, LlmResult, ModelTier};

/// Ollama backend for local LLM invocation.
///
/// This backend connects to a local Ollama server (default: http://localhost:11434)
/// and uses the Ollama API to generate responses with local models.
///
/// # Requirements
///
/// An Ollama server must be running and accessible at the configured base_url.
/// Install Ollama from https://ollama.ai
///
/// # Example
///
/// ```ignore
/// use sage_method::primitives::invoke::*;
///
/// let backend = OllamaBackend::new()
///     .with_model("qwen3-coder:30b");
///
/// let response = backend.generate(LlmRequest {
///     prompt: "Explain async/await in Rust.".to_string(),
///     system: Some("You are a Rust expert.".to_string()),
///     max_tokens: Some(512),
///     temperature: None,
///     timeout_secs: Some(60),
/// })?;
///
/// println!("Response: {}", response.text);
/// ```
pub struct OllamaBackend {
    base_url: String,
    model: String,
    client: reqwest::Client,
    timeout: Duration,
    cheap_model: String,
    standard_model: String,
    premium_model: String,
}

impl std::fmt::Debug for OllamaBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OllamaBackend")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("timeout", &self.timeout)
            .field("cheap_model", &self.cheap_model)
            .field("standard_model", &self.standard_model)
            .field("premium_model", &self.premium_model)
            .finish()
    }
}

impl Default for OllamaBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl OllamaBackend {
    /// Create a new Ollama backend with default settings.
    ///
    /// Defaults:
    /// - base_url: http://localhost:11434
    /// - model: qwen3-coder:30b (standard tier)
    /// - cheap: llama3.1:8b
    /// - standard: qwen3-coder:30b
    /// - premium: qwen3-coder:70b
    /// - timeout: 300s
    pub fn new() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
            model: "qwen2.5-coder:32b".to_string(),
            client: reqwest::Client::new(),
            timeout: Duration::from_secs(300),
            cheap_model: "phi4-mini".to_string(),
            standard_model: "qwen2.5-coder:32b".to_string(),
            premium_model: "deepseek-r1:32b".to_string(),
        }
    }

    /// Create a new Ollama backend from environment variables.
    ///
    /// Reads:
    /// - OLLAMA_URL (default: http://localhost:11434)
    /// - OLLAMA_MODEL (default: qwen3-coder:30b)
    /// - OLLAMA_CHEAP_MODEL (default: phi4-mini)
    /// - OLLAMA_STANDARD_MODEL (default: qwen3-coder:30b)
    /// - OLLAMA_PREMIUM_MODEL (default: deepseek-r1:32b)
    pub fn from_env() -> Self {
        let base_url = std::env::var("OLLAMA_URL")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());
        let model = std::env::var("OLLAMA_MODEL")
            .unwrap_or_else(|_| "qwen3-coder:30b".to_string());
        let cheap = std::env::var("OLLAMA_CHEAP_MODEL")
            .unwrap_or_else(|_| "phi4-mini".to_string());
        let standard = std::env::var("OLLAMA_STANDARD_MODEL")
            .unwrap_or_else(|_| "qwen3-coder:30b".to_string());
        let premium = std::env::var("OLLAMA_PREMIUM_MODEL")
            .unwrap_or_else(|_| "deepseek-r1:32b".to_string());

        Self {
            base_url,
            model,
            client: reqwest::Client::new(),
            timeout: Duration::from_secs(300),
            cheap_model: cheap,
            standard_model: standard,
            premium_model: premium,
        }
    }

    /// Set the model to use for generation.
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    /// Set the base URL of the Ollama server.
    pub fn with_url(mut self, url: &str) -> Self {
        self.base_url = url.to_string();
        self
    }

    /// Set the request timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Select the appropriate model based on tier.
    ///
    /// Priority order:
    /// 1. Tier-based selection (if tier is specified)
    /// 2. Default model (self.model)
    fn select_model(&self, tier: Option<&ModelTier>) -> &str {
        match tier {
            Some(ModelTier::Cheap) => &self.cheap_model,
            Some(ModelTier::Standard) => &self.standard_model,
            Some(ModelTier::Premium) => &self.premium_model,
            None => &self.model,
        }
    }

    /// Generate a response using a specific model (overrides default).
    ///
    /// This allows per-call model selection for tasks that benefit from
    /// different model sizes (e.g., fast small model for extraction,
    /// large model for code generation).
    pub async fn generate_with_model(&self, request: LlmRequest, model: &str) -> LlmResult<LlmResponse> {
        let url = format!("{}/api/generate", self.base_url);

        // Build the request payload with specified model
        // format: "json" constrains ollama to produce syntactically valid JSON,
        // preventing the malformed YAML/prose that local models often emit.
        // All sage-lore invoke calls expect structured output, so this is safe globally.
        // num_ctx controls KV cache allocation. Ollama 0.18.0+ defaults to the model's
        // declared context window (e.g. 256K for qwen3.5), which can exceed available VRAM.
        // Default to 32768 — large enough for most scroll prompts, small enough to fit in memory.
        // Override via OLLAMA_NUM_CTX env var if needed.
        let num_ctx: u64 = std::env::var("OLLAMA_NUM_CTX")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(32768);

        // Reasoning models (e.g. qwen3.5) put output in `thinking` field and leave
        // `response` empty when think mode is on. Default to think=false so structured
        // extraction works reliably. Override with OLLAMA_THINK=true to enable reasoning.
        let think: bool = std::env::var("OLLAMA_THINK")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(false);

        // Model-family-aware format strategy (#159):
        // - Models with GBNF grammar support (qwen, phi, llama, deepseek, gemma):
        //   use schema-based constrained decoding or "json" fallback
        // - gpt-oss models: Harmony training fights grammar constraints,
        //   omit format field entirely and rely on prompt + post-processing
        // - Unknown models: default to "json" (safe default)
        let model_lower = model.to_lowercase();
        let supports_json_format = !model_lower.contains("gpt-oss")
            && !model_lower.contains("gpt_oss")
            && !model_lower.contains("harmony");

        let mut payload = serde_json::json!({
            "model": model,
            "prompt": request.prompt,
            "stream": false,
            "think": think,
            "options": {
                "num_predict": request.max_tokens.unwrap_or(8192),
                "num_ctx": num_ctx,
            }
        });

        if supports_json_format {
            // Use schema if provided, otherwise basic "json" constraint
            let format_value = request.format_schema
                .as_ref()
                .cloned()
                .unwrap_or_else(|| serde_json::Value::String("json".to_string()));
            payload["format"] = format_value;
        } else {
            tracing::info!(model = %model, "Skipping format constraint (model family does not support GBNF)");
        }

        // Add system prompt if provided
        if let Some(ref system) = request.system {
            payload["system"] = serde_json::Value::String(system.clone());
        }

        // Add temperature if provided
        if let Some(temperature) = request.temperature {
            payload["options"]["temperature"] = serde_json::Value::from(temperature);
        }

        // Determine timeout
        let timeout = if let Some(timeout_secs) = request.timeout_secs {
            Duration::from_secs(timeout_secs)
        } else {
            self.timeout
        };

        // Make the request
        let response = self.client
            .post(&url)
            .json(&payload)
            .timeout(timeout)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout
                } else if e.is_connect() {
                    LlmError::ExecutionFailed(
                        format!("Ollama server not reachable at {}", self.base_url)
                    )
                } else {
                    LlmError::ExecutionFailed(e.to_string())
                }
            })?;

        // Check if the response status is successful
        if !response.status().is_success() {
            return Err(LlmError::ExecutionFailed(
                format!("Ollama API returned status {}: {}",
                    response.status(),
                    response.text().await.unwrap_or_default())
            ));
        }

        // Parse the response
        let response_json: serde_json::Value = response.json()
            .await
            .map_err(|e| LlmError::ParseError(format!("Failed to parse JSON: {}", e)))?;

        // Extract the response text.
        // Reasoning models (qwen3.5, etc.) may put content in `thinking` and leave
        // `response` empty. Fall back to `thinking` when `response` is empty.
        let text = match response_json["response"].as_str() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => response_json["thinking"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .ok_or_else(|| LlmError::ParseError(
                    "Both 'response' and 'thinking' fields are empty".to_string()
                ))?,
        };

        // Extract optional token count
        let tokens_used = response_json["eval_count"]
            .as_u64()
            .map(|v| v as u32);

        // Extract model name from response
        let response_model = response_json["model"]
            .as_str()
            .unwrap_or(model)
            .to_string();

        Ok(LlmResponse {
            text,
            tokens_used,
            model: response_model,
            truncated: false,
        })
    }
}

#[async_trait::async_trait]
impl LlmBackend for OllamaBackend {
    async fn generate(&self, request: LlmRequest) -> LlmResult<LlmResponse> {
        // Priority: explicit model > model_tier > default
        let model = match &request.model {
            Some(explicit) => explicit.clone(),
            None => self.select_model(request.model_tier.as_ref()).to_string(),
        };
        // Delegate to generate_with_model using selected model
        OllamaBackend::generate_with_model(self, request, &model).await
    }

    async fn generate_with_model(&self, request: LlmRequest, model: &str) -> LlmResult<LlmResponse> {
        // OllamaBackend supports per-call model selection
        // Note: explicit model parameter overrides tier selection
        OllamaBackend::generate_with_model(self, request, model).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_backend_construction() {
        let backend = OllamaBackend::new();
        assert_eq!(backend.base_url, "http://localhost:11434");
        assert_eq!(backend.model, "qwen2.5-coder:32b");
    }

    #[test]
    fn test_ollama_backend_builder() {
        let backend = OllamaBackend::new()
            .with_model("llama3:8b")
            .with_url("http://localhost:8080")
            .with_timeout(Duration::from_secs(60));

        assert_eq!(backend.model, "llama3:8b");
        assert_eq!(backend.base_url, "http://localhost:8080");
        assert_eq!(backend.timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_ollama_backend_from_env() {
        // Save original env vars
        let orig_url = std::env::var("OLLAMA_URL").ok();
        let orig_model = std::env::var("OLLAMA_MODEL").ok();

        // Test with env vars set
        std::env::set_var("OLLAMA_URL", "http://test:1234");
        std::env::set_var("OLLAMA_MODEL", "test-model");

        let backend = OllamaBackend::from_env();
        assert_eq!(backend.base_url, "http://test:1234");
        assert_eq!(backend.model, "test-model");

        // Restore original env vars
        if let Some(url) = orig_url {
            std::env::set_var("OLLAMA_URL", url);
        } else {
            std::env::remove_var("OLLAMA_URL");
        }
        if let Some(model) = orig_model {
            std::env::set_var("OLLAMA_MODEL", model);
        } else {
            std::env::remove_var("OLLAMA_MODEL");
        }
    }

    #[tokio::test]
    #[ignore] // Requires running Ollama
    async fn test_ollama_backend_smoke() {
        let backend = OllamaBackend::from_env();
        let request = LlmRequest {
            prompt: "Say 'hello' and nothing else.".to_string(),
            system: None,
            max_tokens: Some(10),
            temperature: None,
            timeout_secs: Some(30),
            model_tier: None,
            format_schema: None,
            model: None,
        };
        let response = backend.generate(request).await.expect("Ollama should respond");
        assert!(!response.text.is_empty());
    }

    #[tokio::test]
    #[ignore] // Requires running Ollama with llama3.1:8b model
    async fn test_ollama_generate_with_model() {
        let backend = OllamaBackend::from_env();
        let request = LlmRequest {
            prompt: "Say 'model override works' and nothing else.".to_string(),
            system: None,
            max_tokens: Some(20),
            temperature: None,
            timeout_secs: Some(30),
            model_tier: None,
            format_schema: None,
            model: None,
        };
        // Use a different model than the default
        let response = backend
            .generate_with_model(request, "llama3.1:8b").await
            .expect("Ollama should respond with specified model");
        assert!(!response.text.is_empty());
        assert_eq!(response.model, "llama3.1:8b");
    }

    #[test]
    fn test_select_model_cheap_tier() {
        let backend = OllamaBackend::new();
        let model = backend.select_model(Some(&ModelTier::Cheap));
        assert_eq!(model, "phi4-mini");
    }

    #[test]
    fn test_select_model_standard_tier() {
        let backend = OllamaBackend::new();
        let model = backend.select_model(Some(&ModelTier::Standard));
        assert_eq!(model, "qwen2.5-coder:32b");
    }

    #[test]
    fn test_select_model_premium_tier() {
        let backend = OllamaBackend::new();
        let model = backend.select_model(Some(&ModelTier::Premium));
        assert_eq!(model, "deepseek-r1:32b");
    }

    #[test]
    fn test_select_model_no_tier_uses_default() {
        let backend = OllamaBackend::new();
        let model = backend.select_model(None);
        assert_eq!(model, "qwen2.5-coder:32b");
    }

    #[test]
    fn test_select_model_no_tier_respects_custom() {
        let backend = OllamaBackend::new()
            .with_model("custom-model:latest");
        let model = backend.select_model(None);
        assert_eq!(model, "custom-model:latest");
    }

    #[test]
    fn test_tier_overrides_default_model() {
        // When tier is specified, it should override default model
        let backend = OllamaBackend::new()
            .with_model("custom-model:latest");
        let model = backend.select_model(Some(&ModelTier::Cheap));
        assert_eq!(model, "phi4-mini");
    }
}
