// SPDX-License-Identifier: MIT
//! LlmBackend trait definition.

use async_trait::async_trait;

use super::types::{LlmRequest, LlmResponse, LlmResult};

/// Trait for LLM backend implementations.
///
/// This trait is the core abstraction for invoking language models.
/// Implementations must be `Send + Sync` to support concurrent usage.
///
/// # Design Decisions
///
/// - **No output caching**: Responses are not cached — every call hits the backend,
///   maintaining parity across all backends (D6).
/// - **Async API**: Uses async-trait for non-blocking execution with tokio.
/// - **Configuration via .sage-project.yaml**: The llm section configures
///   which backend and model to use.
#[async_trait]
pub trait LlmBackend: Send + Sync {
    /// Generate a response from the LLM.
    ///
    /// # Arguments
    ///
    /// * `request` - The generation request with prompt and parameters.
    ///
    /// # Returns
    ///
    /// The generated response, or an error if generation failed.
    async fn generate(&self, request: LlmRequest) -> LlmResult<LlmResponse>;

    /// Generate a response using a specific model.
    ///
    /// This allows per-call model selection for backends that support it.
    /// Default implementation ignores the model parameter and uses the
    /// backend's configured model.
    ///
    /// # Arguments
    ///
    /// * `request` - The generation request with prompt and parameters.
    /// * `model` - The model to use for this specific call.
    async fn generate_with_model(&self, request: LlmRequest, _model: &str) -> LlmResult<LlmResponse> {
        // Default: ignore model parameter, use backend's default
        self.generate(request).await
    }
}
