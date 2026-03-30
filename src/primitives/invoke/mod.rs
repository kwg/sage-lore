// SPDX-License-Identifier: MIT
//! LLM invocation primitive for the SAGE Method engine.
//!
//! Provides a trait-based abstraction over LLM backends, allowing the engine
//! to invoke language models through different implementations:
//!
//! - `ClaudeCliBackend` - Production backend that shells out to `claude -p`
//! - `OllamaBackend` - Local LLM backend via Ollama API
//! - `MockLlmBackend` - Testing backend with canned responses
//!
//! # Security Note
//!
//! Prompt injection is a known risk. The engine documents but does not prevent
//! prompt injection attacks. Users must sanitize inputs appropriately.
//!
//! # Example
//!
//! ```ignore
//! use sage_method::primitives::invoke::{LlmBackend, LlmRequest, MockLlmBackend};
//!
//! let backend = MockLlmBackend::new()
//!     .with_canned_responses(vec![
//!         ("What is 2+2?".to_string(), "4".to_string()),
//!     ]);
//!
//! let request = LlmRequest {
//!     prompt: "What is 2+2?".to_string(),
//!     system: None,
//!     max_tokens: None,
//!     temperature: None,
//!     timeout_secs: None,
//! };
//!
//! let response = backend.generate(request)?;
//! assert_eq!(response.text, "4");
//! ```

mod types;
mod r#trait;
mod mock;
mod claude;
mod ollama;

// Re-export public types
pub use types::{LlmRequest, LlmResponse, LlmError, LlmResult, ModelTier};
pub use r#trait::LlmBackend;
pub use mock::{MockLlmBackend, MockResponseKey};
pub use claude::ClaudeCliBackend;
pub use ollama::OllamaBackend;
