// SPDX-License-Identifier: MIT
//! Interface dispatch system for scroll execution.
//!
//! Provides the `InterfaceDispatch` trait and implementations for all
//! supported interface modules (git, platform, test, fs, invoke, secure).

pub mod fs;
pub mod invoke;
pub mod platform;
pub mod secure;
pub mod test;
pub mod vcs;

use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

use crate::primitives::{
    AutoDetectBackend, ClaudeCliBackend, FsBackend, FsPolicy, Git2Backend, GitBackend,
    LlmBackend, MockFsBackend, MockLlmBackend, MockPlatform, OllamaBackend, Platform,
    PolicyDrivenBackend, SecureBackend, SecureFsBackend, TestBackend,
};
use crate::primitives::platform::ForgejoBackend;
use crate::primitives::test::NoopBackend;
use crate::scroll::agent_registry::AgentRegistry;
use crate::scroll::error::ExecutionError;

/// Trait for dispatching method calls to interface implementations.
#[async_trait]
pub trait InterfaceDispatch: Send + Sync {
    /// Dispatch a method call with optional parameters.
    async fn dispatch(
        &self,
        method: &str,
        params: &Option<serde_json::Value>,
    ) -> Result<serde_json::Value, ExecutionError>;
}

/// Errors that can occur during registry construction.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    /// Security policy could not be loaded
    #[error("Failed to load security policy: {0}")]
    PolicyError(#[from] crate::config::SecurityError),

    /// Git repository could not be opened
    #[error("Failed to open git repository: {0}")]
    GitError(String),

    /// Required configuration is missing
    #[error("Missing required configuration: {0}")]
    MissingConfig(String),
}

/// Registry that aggregates all interface implementations.
#[derive(Clone)]
pub struct InterfaceRegistry {
    pub(crate) vcs: vcs::VcsInterface,
    pub(crate) platform: platform::PlatformInterface,
    pub(crate) test: test::TestInterface,
    pub(crate) fs: fs::FsInterface,
    pub(crate) invoke: invoke::InvokeInterface,
    pub(crate) invoke_backends: std::collections::HashMap<String, invoke::InvokeInterface>,
    pub(crate) secure: secure::SecureInterface,
    pub(crate) agent_registry: Arc<AgentRegistry>,
}

impl InterfaceRegistry {
    /// Create a production registry from a project root.
    ///
    /// Initializes all backends with real implementations:
    /// - Loads security policy from `.sage-lore/security/policy.yaml`
    /// - Creates SecureBackend for security scanning
    /// - Creates Git2Backend for git operations
    /// - Creates SecureFsBackend for file operations
    /// - Creates ForgejoBackend from env vars (FORGEJO_URL, FORGEJO_REPO, FORGEJO_API_TOKEN)
    /// - Creates ClaudeCliBackend or OllamaBackend based on SAGE_LLM_BACKEND env
    /// - Auto-detects test framework
    pub fn from_project(project_root: &Path) -> Result<Self, RegistryError> {
        // Create secure backend first (needed by others)
        let secure_backend: Arc<dyn SecureBackend> = Arc::new(
            PolicyDrivenBackend::from_project(project_root)?
        );

        // Create git backend
        let git_backend: Arc<dyn GitBackend> = Arc::new(
            Git2Backend::open(project_root, secure_backend.clone())
                .map_err(|e| RegistryError::GitError(e.to_string()))?
        );

        // Create fs backend
        let fs_backend: Arc<dyn FsBackend> = Arc::new(SecureFsBackend::new(
            FsPolicy::new(project_root.to_path_buf()),
            secure_backend.clone(),
            project_root.to_path_buf(),
        ));

        // Create platform backend via Config + SecretResolver (D28, D35, #178).
        // Platform config comes from the config hierarchy; token is resolved
        // lazily through SecretResolver (env → .env → secrets.yaml).
        // The resolved token value NEVER enters the Config struct (D15).
        let platform_backend: Option<Arc<dyn Platform>> = {
            let config = crate::config::ConfigLoader::load_from_project(project_root)
                .unwrap_or_default();
            let secret_resolver = crate::config::SecretResolver::new(project_root);

            match (config.platform_url(), config.platform_repo(), config.platform_token_env()) {
                (Some(url), Some(repo), Some(token_env)) => {
                    match secret_resolver.resolve(token_env) {
                        Some(token) => Some(Arc::new(ForgejoBackend::new(url, repo, &token))),
                        None => {
                            tracing::debug!(
                                "Platform token env '{}' not found — platform operations unavailable",
                                token_env
                            );
                            None
                        }
                    }
                }
                _ => {
                    tracing::debug!("Platform config incomplete — platform operations will be unavailable");
                    None
                }
            }
        };

        // Create invoke backend based on env
        let invoke_backend: Arc<dyn LlmBackend> = {
            let backend_type = std::env::var("SAGE_LLM_BACKEND")
                .unwrap_or_else(|_| "claude".to_string());

            match backend_type.as_str() {
                "ollama" => Arc::new(OllamaBackend::from_env()),
                _ => Arc::new(ClaudeCliBackend::new()),
            }
        };

        // Create named backend map for per-invoke override
        let mut invoke_backends = std::collections::HashMap::new();
        invoke_backends.insert(
            "claude".to_string(),
            invoke::InvokeInterface::with_backend(Arc::new(ClaudeCliBackend::new())),
        );
        invoke_backends.insert(
            "ollama".to_string(),
            invoke::InvokeInterface::with_backend(Arc::new(OllamaBackend::from_env())),
        );

        // Create test backend with auto-detection
        let test_backend: Arc<dyn TestBackend> = Arc::new(
            AutoDetectBackend::new(project_root)
        );

        // Load agent registry from project root
        let mut agent_registry = AgentRegistry::new();
        if let Err(e) = agent_registry.load_from_directory(&project_root.to_string_lossy()) {
            tracing::warn!("Failed to load agent registry: {}", e);
        }

        Ok(Self {
            vcs: vcs::VcsInterface::with_backend(git_backend),
            platform: match platform_backend {
                Some(backend) => platform::PlatformInterface::with_backend(backend),
                None => platform::PlatformInterface::new(),
            },
            test: test::TestInterface::with_backend(test_backend),
            fs: fs::FsInterface::with_backend(fs_backend),
            invoke: invoke::InvokeInterface::with_backend(invoke_backend),
            invoke_backends,
            secure: secure::SecureInterface::with_backend(secure_backend),
            agent_registry: Arc::new(agent_registry),
        })
    }

    /// Create a test registry with mock backends.
    ///
    /// Uses MockFsBackend, MockPlatform, MockLlmBackend, and NoopBackend.
    /// Does not require environment variables or policy files.
    pub fn for_testing() -> Self {
        Self {
            vcs: vcs::VcsInterface::new(),  // Still NotImplemented - needs repo
            platform: platform::PlatformInterface::with_backend(Arc::new(MockPlatform::new())),
            test: test::TestInterface::with_backend(Arc::new(NoopBackend::new("test mock".to_string()))),
            fs: fs::FsInterface::with_backend(Arc::new(MockFsBackend::new())),
            invoke: invoke::InvokeInterface::with_backend(Arc::new(
                MockLlmBackend::new()
                // No default response - uses smart response generation
            )),
            invoke_backends: std::collections::HashMap::new(),
            secure: secure::SecureInterface::new(),  // Still NotImplemented - needs policy
            agent_registry: Arc::new(AgentRegistry::new()),
        }
    }

    /// Create a test registry with explicit canned LLM responses.
    ///
    /// Use this when testing scrolls that expect specific output schemas from
    /// LLM calls. The responses map prompt substrings to response text.
    /// First matching substring wins.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let registry = InterfaceRegistry::for_testing_with_llm_responses(vec![
    ///     ("convert".to_string(), "files:\n  - path: test.rs\n".to_string()),
    /// ]);
    /// ```
    pub fn for_testing_with_llm_responses(responses: Vec<(String, String)>) -> Self {
        Self {
            vcs: vcs::VcsInterface::new(),
            platform: platform::PlatformInterface::with_backend(Arc::new(MockPlatform::new())),
            test: test::TestInterface::with_backend(Arc::new(NoopBackend::new("test mock".to_string()))),
            fs: fs::FsInterface::with_backend(Arc::new(MockFsBackend::new())),
            invoke: invoke::InvokeInterface::with_backend(Arc::new(
                MockLlmBackend::new().with_substring_responses(responses)
            )),
            invoke_backends: std::collections::HashMap::new(),
            secure: secure::SecureInterface::new(),
            agent_registry: Arc::new(AgentRegistry::new()),
        }
    }

    /// Create a minimal registry for unit tests (all stubs).
    pub fn new() -> Self {
        Self {
            vcs: vcs::VcsInterface::new(),
            platform: platform::PlatformInterface::new(),
            test: test::TestInterface::new(),
            fs: fs::FsInterface::new(),
            invoke: invoke::InvokeInterface::new(),
            invoke_backends: std::collections::HashMap::new(),
            secure: secure::SecureInterface::new(),
            agent_registry: Arc::new(AgentRegistry::new()),
        }
    }

    /// Set the invoke backend (for tests that need a custom LLM backend).
    pub fn set_invoke_backend(&mut self, backend: Arc<dyn crate::primitives::invoke::LlmBackend>) {
        self.invoke = invoke::InvokeInterface::with_backend(backend);
    }

    /// Dispatch an interface call in "module.method" format.
    pub async fn dispatch_interface(
        &self,
        interface: &str,
        params: &Option<serde_json::Value>,
    ) -> Result<serde_json::Value, ExecutionError> {
        let parts: Vec<&str> = interface.splitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(ExecutionError::InvalidInterface(interface.to_string()));
        }

        let (module, method) = (parts[0], parts[1]);

        match module {
            "vcs" | "git" => self.vcs.dispatch(method, params).await,
            "platform" => self.platform.dispatch(method, params).await,
            "test" => self.test.dispatch(method, params).await,
            "fs" => self.fs.dispatch(method, params).await,
            "invoke" => self.invoke.dispatch(method, params).await,
            "secure" => self.secure.dispatch(method, params).await,
            _ => Err(ExecutionError::UnknownModule(module.to_string())),
        }
    }

    /// Look up an agent's system prompt from the registry.
    ///
    /// Returns the persona XML as a system prompt string.
    /// For testing with empty registries, returns a minimal default prompt.
    pub fn get_agent_system_prompt(&self, agent: &str) -> Result<String, ExecutionError> {
        if let Some(prompt) = self.agent_registry.get_system_prompt(agent) {
            Ok(prompt.to_string())
        } else if self.agent_registry.agent_names().is_empty() {
            // Empty registry (testing or no agents/ dir) — use agent name as minimal context
            tracing::warn!(agent = %agent, "No agent registry loaded, using agent name as system context");
            Ok(format!("You are the '{}' agent.", agent))
        } else {
            // Registry loaded but agent not found — fail hard
            Err(ExecutionError::UnknownAgent(agent.to_string()))
        }
    }

    /// Invoke an agent with a system prompt, instructions, and context.
    ///
    /// Three-channel model:
    /// - system_prompt: agent persona (from AgentRegistry)
    /// - instructions: short task directive (from scroll)
    /// - context: data (from scroll context field)
    ///
    /// If `backend_override` is Some("claude") or Some("ollama"), routes to that
    /// specific backend instead of the default.
    ///
    /// When `thinking == Some(true)` AND `output_schema` is set, this orchestrates
    /// a two-call pattern: a thinking-mode call produces free-form reasoning + answer,
    /// then a cheap-tier extraction call converts that text into schema-conforming JSON.
    /// Both calls are observable as separate trace events. See #188.
    #[allow(clippy::too_many_arguments)]
    pub async fn invoke_agent(
        &self,
        agent: &str,
        system_prompt: &str,
        instructions: &str,
        context: &[serde_json::Value],
        timeout_secs: Option<u64>,
        backend_override: Option<&str>,
        output_schema: Option<&serde_json::Value>,
        thinking: Option<bool>,
    ) -> Result<serde_json::Value, ExecutionError> {
        // Build user message from instructions + context
        let mut user_message = instructions.to_string();
        if !context.is_empty() {
            user_message.push_str("\n\nContext:\n");
            for (i, ctx_item) in context.iter().enumerate() {
                let formatted = match ctx_item {
                    serde_json::Value::String(s) => s.clone(),
                    other => serde_json::to_string_pretty(other).unwrap_or_default(),
                };
                user_message.push_str(&format!("{}. {}\n", i + 1, formatted));
            }
        }

        // Inject output_schema so the LLM sees the exact schema it's validated against.
        // For thinking calls this is guidance only — GBNF enforcement is dropped below.
        if let Some(schema) = output_schema {
            user_message.push_str("\n\nRequired output JSON schema (use exact field names):\n");
            user_message.push_str(&serde_json::to_string_pretty(schema).unwrap_or_default());
        }

        // Two-call thinking + schema path (#188): first call gets reasoning,
        // second call extracts structured JSON via a cheap-tier model.
        if let (Some(true), Some(schema)) = (thinking, output_schema) {
            return self.invoke_agent_thinking_with_extraction(
                agent,
                system_prompt,
                &user_message,
                timeout_secs,
                backend_override,
                schema,
            ).await;
        }

        // Build params mapping with system prompt (single-call path)
        let mut mapping = serde_json::Map::new();
        mapping.insert("agent".to_string(), serde_json::Value::String(agent.to_string()));
        mapping.insert("prompt".to_string(), serde_json::Value::String(user_message));
        mapping.insert("system".to_string(), serde_json::Value::String(system_prompt.to_string()));
        if let Some(secs) = timeout_secs {
            // Thinking calls may legitimately need longer than 1200s; cap raised
            // to 7200s in that case. Non-thinking keeps the historical 1200s cap.
            let cap = if thinking == Some(true) { 7200 } else { 1200 };
            let clamped = secs.min(cap);
            mapping.insert(
                "timeout_secs".to_string(),
                serde_json::Value::Number(serde_json::Number::from(clamped)),
            );
        }

        // Pass output_schema as format_schema for Ollama's constrained decoding.
        // Thinking calls without a schema skip this — that's the expected case
        // for free-form reasoning where the caller doesn't need structured output.
        if let Some(schema) = output_schema {
            mapping.insert("format_schema".to_string(), schema.clone());
        }
        if let Some(t) = thinking {
            mapping.insert("thinking".to_string(), serde_json::Value::Bool(t));
        }

        let params = serde_json::to_value(mapping).ok();

        // Route to specific backend if override is set
        if let Some(backend_name) = backend_override {
            if let Some(backend) = self.invoke_backends.get(backend_name) {
                tracing::info!(backend = %backend_name, agent = %agent, "Using backend override");
                return backend.dispatch("generate", &params).await;
            }
            tracing::warn!(backend = %backend_name, "Backend override not found, using default");
        }

        self.invoke.dispatch("generate", &params).await
    }

    /// Two-call orchestration for thinking-mode invokes that need structured output (#188).
    ///
    /// Call A: thinking-mode invoke, format constraint dropped, free-form reasoning + answer.
    /// Call B: cheap-tier extraction, format_schema enforced, prompt = "extract JSON from text".
    ///
    /// Backend-agnostic — works for any backend that supports `thinking` and `format_schema`.
    /// Both calls log distinctly so traces show what happened.
    async fn invoke_agent_thinking_with_extraction(
        &self,
        agent: &str,
        system_prompt: &str,
        user_message: &str,
        timeout_secs: Option<u64>,
        backend_override: Option<&str>,
        output_schema: &serde_json::Value,
    ) -> Result<serde_json::Value, ExecutionError> {
        // ---- Call A: thinking-mode reasoning ----
        tracing::info!(agent = %agent, phase = "think", "Two-call thinking-mode: reasoning pass");

        let mut think_params = serde_json::Map::new();
        think_params.insert("agent".to_string(), serde_json::Value::String(agent.to_string()));
        think_params.insert("prompt".to_string(), serde_json::Value::String(user_message.to_string()));
        think_params.insert("system".to_string(), serde_json::Value::String(system_prompt.to_string()));
        think_params.insert("thinking".to_string(), serde_json::Value::Bool(true));
        // No format_schema — thinking call is free-form.
        if let Some(secs) = timeout_secs {
            think_params.insert(
                "timeout_secs".to_string(),
                serde_json::Value::Number(serde_json::Number::from(secs.min(7200))),
            );
        }
        let think_params_value = serde_json::to_value(think_params).ok();

        let think_result = if let Some(backend_name) = backend_override {
            if let Some(backend) = self.invoke_backends.get(backend_name) {
                backend.dispatch("generate", &think_params_value).await?
            } else {
                tracing::warn!(backend = %backend_name, "Backend override not found, using default");
                self.invoke.dispatch("generate", &think_params_value).await?
            }
        } else {
            self.invoke.dispatch("generate", &think_params_value).await?
        };

        let think_text = think_result.as_str().unwrap_or("").to_string();
        tracing::info!(
            agent = %agent,
            phase = "think",
            response_len = think_text.len(),
            "Two-call thinking-mode: reasoning complete"
        );

        // ---- Call B: cheap-tier structured extraction ----
        tracing::info!(agent = %agent, phase = "extract", "Two-call thinking-mode: extraction pass");

        let extract_prompt = format!(
            "The following text contains an answer produced by a reasoning model. \
             It may include thinking-out-loud, multiple drafts, or commentary. \
             Extract a single JSON object that conforms to the requested schema. \
             Output ONLY the JSON object — no markdown fences, no commentary.\n\n\
             ----- BEGIN ANSWER TEXT -----\n{}\n----- END ANSWER TEXT -----",
            think_text
        );

        let mut extract_params = serde_json::Map::new();
        extract_params.insert("prompt".to_string(), serde_json::Value::String(extract_prompt));
        extract_params.insert("model_tier".to_string(), serde_json::Value::String("cheap".to_string()));
        extract_params.insert("format_schema".to_string(), output_schema.clone());
        // Extraction gets a tight per-call timeout — it's structured, not reasoning.
        extract_params.insert(
            "timeout_secs".to_string(),
            serde_json::Value::Number(serde_json::Number::from(120u64)),
        );
        let extract_params_value = serde_json::to_value(extract_params).ok();

        let extract_result = if let Some(backend_name) = backend_override {
            if let Some(backend) = self.invoke_backends.get(backend_name) {
                backend.dispatch("generate", &extract_params_value).await?
            } else {
                self.invoke.dispatch("generate", &extract_params_value).await?
            }
        } else {
            self.invoke.dispatch("generate", &extract_params_value).await?
        };

        tracing::info!(
            agent = %agent,
            phase = "extract",
            "Two-call thinking-mode: extraction complete"
        );

        Ok(extract_result)
    }

    /// Generate text from a prompt (for core primitives).
    ///
    /// Dispatches to invoke::generate with the given prompt.
    /// Optional backend_override routes to a specific backend ("claude", "ollama").
    pub async fn invoke_generate(&self, prompt: &str) -> Result<serde_json::Value, ExecutionError> {
        self.invoke_generate_with_backend(prompt, None).await
    }

    /// Generate text with optional backend, model_tier, and model overrides.
    pub async fn invoke_generate_with_backend(
        &self,
        prompt: &str,
        backend_override: Option<&str>,
    ) -> Result<serde_json::Value, ExecutionError> {
        self.invoke_generate_with_options(prompt, backend_override, None, None).await
    }

    /// Generate text with full options: backend, model_tier, model, and format_schema overrides.
    pub async fn invoke_generate_with_options(
        &self,
        prompt: &str,
        backend_override: Option<&str>,
        model_tier: Option<&str>,
        model: Option<&str>,
    ) -> Result<serde_json::Value, ExecutionError> {
        self.invoke_generate_full(prompt, backend_override, model_tier, model, None).await
    }

    /// Generate text with all options including format_schema for structured output enforcement.
    pub async fn invoke_generate_full(
        &self,
        prompt: &str,
        backend_override: Option<&str>,
        model_tier: Option<&str>,
        model: Option<&str>,
        format_schema: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, ExecutionError> {
        let mut map = serde_json::Map::new();
        map.insert("prompt".to_string(), serde_json::Value::String(prompt.to_string()));
        if let Some(tier) = model_tier {
            map.insert("model_tier".to_string(), serde_json::Value::String(tier.to_string()));
        }
        if let Some(m) = model {
            map.insert("model".to_string(), serde_json::Value::String(m.to_string()));
        }
        if let Some(schema) = format_schema {
            map.insert("format_schema".to_string(), schema.clone());
        }
        let params = serde_json::to_value(map).ok();

        // Route to specific backend if override is set
        if let Some(backend_name) = backend_override {
            if let Some(backend) = self.invoke_backends.get(backend_name) {
                tracing::info!(backend = %backend_name, "Using backend override for core primitive");
                return backend.dispatch("generate", &params).await;
            }
            tracing::warn!(backend = %backend_name, "Backend override not found, using default");
        }

        self.invoke.dispatch("generate", &params).await
    }

    /// Get reference to the vcs interface.
    pub fn vcs_interface(&self) -> Option<&vcs::VcsInterface> {
        Some(&self.vcs)
    }
}

impl Default for InterfaceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod thinking_tests {
    use super::*;
    use crate::primitives::invoke::MockLlmBackend;

    /// When `thinking: true` and an output_schema is provided, invoke_agent must
    /// orchestrate a two-call pattern: free-form thinking pass, then cheap-tier
    /// extraction. Both calls hit the same backend; this test verifies both
    /// happened and that extraction's response is what bubbles back. (#188)
    #[tokio::test]
    async fn invoke_agent_two_call_when_thinking_and_schema() {
        let mock = MockLlmBackend::new()
            .with_substring_responses(vec![
                // Thinking pass — matches the user_message we pass.
                ("REASONING_TASK_MARKER".to_string(),
                 "Lots of reasoning ... final answer is {x:1}".to_string()),
                // Extraction pass — matches the engine-owned extraction prompt.
                ("BEGIN ANSWER TEXT".to_string(),
                 "{\"x\":1}".to_string()),
            ]);
        let mock_arc = Arc::new(mock);
        let mut registry = InterfaceRegistry::new();
        registry.set_invoke_backend(mock_arc.clone());

        let schema = serde_json::json!({"type":"object","properties":{"x":{"type":"integer"}}});
        let result = registry.invoke_agent(
            "test-agent",
            "system",
            "REASONING_TASK_MARKER do the work",
            &[],
            None,
            None,
            Some(&schema),
            Some(true),
        ).await.expect("two-call orchestration succeeds");

        let calls = mock_arc.calls();
        assert_eq!(calls.len(), 2, "expected exactly two backend calls");
        assert_eq!(calls[0].thinking, Some(true), "first call must be thinking-mode");
        assert!(calls[0].format_schema.is_none(), "thinking call must drop format constraint");
        assert!(
            calls[1].thinking != Some(true),
            "extraction call must NOT be thinking-mode"
        );
        assert!(calls[1].format_schema.is_some(), "extraction call must carry the schema");
        assert_eq!(
            calls[1].model_tier,
            Some(crate::primitives::invoke::ModelTier::Cheap),
            "extraction must run on cheap tier"
        );
        assert!(
            calls[1].prompt.contains("BEGIN ANSWER TEXT"),
            "extraction prompt must wrap the thinking call's output"
        );
        assert!(
            calls[1].prompt.contains("Lots of reasoning"),
            "extraction prompt must include the thinking call's response text"
        );

        assert_eq!(result.as_str(), Some("{\"x\":1}"));
    }

    /// `thinking: true` without a schema should be a single-call pass-through —
    /// no extraction stage, the request just gets the thinking flag set.
    #[tokio::test]
    async fn invoke_agent_single_call_when_thinking_without_schema() {
        let mock = Arc::new(MockLlmBackend::new()
            .with_default_response(crate::primitives::invoke::LlmResponse {
                text: "free-form answer".to_string(),
                tokens_used: None,
                model: "mock".to_string(),
                truncated: false,
            }));

        let mut registry = InterfaceRegistry::new();
        registry.set_invoke_backend(mock.clone());

        let result = registry.invoke_agent(
            "test-agent",
            "system",
            "instructions",
            &[],
            None,
            None,
            None,
            Some(true),
        ).await.expect("single-call thinking succeeds");

        let calls = mock.calls();
        assert_eq!(calls.len(), 1, "no extraction stage when schema is absent");
        assert_eq!(calls[0].thinking, Some(true));
        assert!(calls[0].format_schema.is_none());
        assert_eq!(result.as_str(), Some("free-form answer"));
    }

    /// Regression guard: `thinking: None` (the default) must NOT trigger
    /// two-call orchestration even when a schema is present. The historical
    /// single-call path with format_schema enforcement must still work.
    #[tokio::test]
    async fn invoke_agent_single_call_when_no_thinking_flag() {
        let mock = Arc::new(MockLlmBackend::new()
            .with_default_response(crate::primitives::invoke::LlmResponse {
                text: "{\"x\":1}".to_string(),
                tokens_used: None,
                model: "mock".to_string(),
                truncated: false,
            }));

        let mut registry = InterfaceRegistry::new();
        registry.set_invoke_backend(mock.clone());

        let schema = serde_json::json!({"type":"object"});
        let _ = registry.invoke_agent(
            "test-agent", "system", "instructions", &[], None, None,
            Some(&schema), None,
        ).await.expect("single-call non-thinking succeeds");

        assert_eq!(mock.calls().len(), 1);
        assert!(mock.calls()[0].format_schema.is_some());
        assert!(mock.calls()[0].thinking.is_none());
    }
}
