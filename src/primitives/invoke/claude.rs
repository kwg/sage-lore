// SPDX-License-Identifier: MIT
//! Claude CLI backend implementation.

use std::process::Stdio;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use super::r#trait::LlmBackend;
use super::types::{LlmError, LlmRequest, LlmResponse, LlmResult, ModelTier};

/// Claude CLI backend that shells out to `claude -p`.
///
/// This is the production backend for v2 of the SAGE Method.
/// Future backends (Gemini, Ollama) are planned as additional providers (D8).
///
/// # Requirements
///
/// The `claude` CLI must be installed and available in PATH.
/// Authentication is handled by the CLI itself.
///
/// # Example
///
/// ```ignore
/// use sage_method::primitives::invoke::*;
///
/// let backend = ClaudeCliBackend::new()
///     .with_model("claude-3-opus");
///
/// let response = backend.generate(LlmRequest {
///     prompt: "Explain quantum computing in one sentence.".to_string(),
///     system: Some("You are a physics professor.".to_string()),
///     max_tokens: Some(100),
///     temperature: Some(0.7),
///     timeout_secs: Some(60),
/// })?;
///
/// println!("Response: {}", response.text);
/// ```
pub struct ClaudeCliBackend {
    model: Option<String>,
    default_timeout_secs: u64,
    cheap_model: String,
    standard_model: String,
    premium_model: String,
    command: String,
}

impl std::fmt::Debug for ClaudeCliBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaudeCliBackend")
            .field("model", &self.model)
            .field("default_timeout_secs", &self.default_timeout_secs)
            .field("cheap_model", &self.cheap_model)
            .field("standard_model", &self.standard_model)
            .field("premium_model", &self.premium_model)
            .field("command", &self.command)
            .finish()
    }
}

impl Default for ClaudeCliBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeCliBackend {
    /// Create a new Claude CLI backend with default settings.
    ///
    /// Reads optional env vars:
    /// - CLAUDE_MODEL: override default model for all tiers (e.g., "opus" for all-opus runs)
    /// - CLAUDE_CHEAP_MODEL: override cheap tier (default: "haiku")
    /// - CLAUDE_STANDARD_MODEL: override standard tier (default: "sonnet")
    /// - CLAUDE_PREMIUM_MODEL: override premium tier (default: "opus")
    pub fn new() -> Self {
        let model = std::env::var("CLAUDE_MODEL").ok();
        let cheap = std::env::var("CLAUDE_CHEAP_MODEL")
            .unwrap_or_else(|_| "haiku".to_string());
        let standard = std::env::var("CLAUDE_STANDARD_MODEL")
            .unwrap_or_else(|_| "sonnet".to_string());
        let premium = std::env::var("CLAUDE_PREMIUM_MODEL")
            .unwrap_or_else(|_| "opus".to_string());

        Self {
            model,
            default_timeout_secs: 120,
            cheap_model: cheap,
            standard_model: standard,
            premium_model: premium,
            command: "claude".to_string(),
        }
    }

    /// Set the model to use for generation.
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = Some(model.to_string());
        self
    }

    /// Set the default timeout in seconds.
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.default_timeout_secs = timeout_secs;
        self
    }

    /// Override the command binary (test-only: point at a shell script fixture).
    #[cfg(test)]
    pub fn with_command(mut self, cmd: &str) -> Self {
        self.command = cmd.to_string();
        self
    }

    /// Check if the claude CLI is available.
    fn check_cli_available(&self) -> LlmResult<()> {
        if self.command != "claude" {
            return Ok(()); // test mode — custom command, skip CLI check
        }
        which::which("claude").map_err(|_| LlmError::CliNotFound)?;
        Ok(())
    }

    /// Select the appropriate model based on tier.
    ///
    /// Priority order:
    /// 1. Tier-based selection (if tier is specified)
    /// 2. Explicit model override (self.model)
    /// 3. Default standard model
    fn select_model(&self, tier: Option<&ModelTier>) -> &str {
        // If CLAUDE_MODEL is set, it overrides ALL tier selections.
        // This enables all-opus or all-sonnet test runs via a single env var.
        if let Some(ref model) = self.model {
            return model;
        }
        match tier {
            Some(ModelTier::Cheap) => &self.cheap_model,
            Some(ModelTier::Standard) => &self.standard_model,
            Some(ModelTier::Premium) => &self.premium_model,
            None => &self.standard_model,
        }
    }

    /// Build the command arguments for the claude CLI.
    fn build_args(&self, request: &LlmRequest, model: &str) -> Vec<String> {
        let mut args = vec!["-p".to_string()];

        // Allow read-only tools so Claude can understand existing code.
        // Without these, claude -p hangs when the prompt references files
        // that exist in the project — it wants to read them but can't.
        // Write/Edit/Bash are excluded: sage-lore controls file operations.
        args.push("--tools".to_string());
        args.push("Read,Glob,Grep,LSP".to_string());

        // Prevent loading MCP servers from project .mcp.json or global config.
        // Without this, claude -p discovers MCP servers via CWD, spins them up,
        // and wastes the entire timeout budget on failed tool calls.
        args.push("--strict-mcp-config".to_string());
        args.push("--mcp-config".to_string());
        args.push(r#"{"mcpServers":{}}"#.to_string());

        // Add selected model
        args.push("--model".to_string());
        args.push(model.to_string());

        // Add system prompt if provided by the scroll
        if let Some(system) = &request.system {
            if !system.is_empty() {
                args.push("--system-prompt".to_string());
                args.push(system.clone());
            }
        }

        args
    }

    /// Execute the claude command asynchronously.
    async fn run_claude(&self, request: LlmRequest, args: Vec<String>, model: String)
        -> LlmResult<LlmResponse>
    {
        let mut cmd = Command::new(&self.command);
        cmd.args(&args)
            .env_remove("CLAUDECODE")
            .env_remove("CLAUDE_CODE_ENTRYPOINT")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Close inherited file descriptors above stderr (fds 3+).
        // When spawned from within a Claude Code session, the parent process
        // leaks internal socket/pipe fds to children. The child claude -p
        // process gets stuck polling these inherited fds instead of completing.
        // Only apply for the real claude binary — test fixtures use shell scripts
        // that need inherited fds for their own pipes.
        #[cfg(unix)]
        if self.command == "claude" {
            unsafe {
                cmd.pre_exec(|| {
                    for fd in 3..1024 {
                        libc::close(fd);
                    }
                    Ok(())
                });
            }
        }

        let mut child = cmd.spawn()
            .map_err(|e| LlmError::ExecutionFailed(e.to_string()))?;

        // Debug: dump prompt to file for investigation
        if let Ok(dump_dir) = std::env::var("SAGE_DUMP_PROMPTS") {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let path = format!("{}/prompt-{}.txt", dump_dir, timestamp);
            let _ = std::fs::write(&path, &request.prompt);
            eprintln!("[sage-lore] Prompt dumped to {} ({} bytes)", path, request.prompt.len());
        }

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(request.prompt.as_bytes()).await
                .map_err(|e| LlmError::IoError(e.to_string()))?;
            // stdin dropped here — closes write end of pipe
        }

        let stdout = child.stdout.take()
            .ok_or_else(|| LlmError::ExecutionFailed("no stdout".to_string()))?;
        let stderr = child.stderr.take()
            .ok_or_else(|| LlmError::ExecutionFailed("no stderr".to_string()))?;

        // Drain both pipes concurrently — prevents OS pipe buffer deadlock
        let (out, err) = tokio::join!(
            read_to_string(BufReader::new(stdout)),
            read_to_string(BufReader::new(stderr))
        );
        let out = out.map_err(|e| LlmError::IoError(e.to_string()))?;
        let err = err.map_err(|e| LlmError::IoError(e.to_string()))?;

        let status = child.wait().await
            .map_err(|e| LlmError::ExecutionFailed(e.to_string()))?;

        if !status.success() {
            return Err(LlmError::ExecutionFailed(
                format!("claude exited {}: {}", status, err)
            ));
        }

        Ok(LlmResponse {
            text: out.trim().to_string(),
            tokens_used: None,
            model,
            truncated: false,
        })
    }
}

#[async_trait]
impl LlmBackend for ClaudeCliBackend {
    async fn generate(&self, request: LlmRequest) -> LlmResult<LlmResponse> {
        self.check_cli_available()?;
        // Priority: explicit model > model_tier > default
        let model = if let Some(ref explicit) = request.model {
            explicit.clone()
        } else {
            self.select_model(request.model_tier.as_ref()).to_string()
        };
        let args = self.build_args(&request, &model);
        let deadline = Duration::from_secs(
            request.timeout_secs.unwrap_or(self.default_timeout_secs)
        );
        timeout(deadline, self.run_claude(request, args, model))
            .await
            .map_err(|_| LlmError::Timeout)?
    }
}

/// Helper: drain an async reader to String.
async fn read_to_string<R: tokio::io::AsyncRead + Unpin>(
    reader: BufReader<R>
) -> std::io::Result<String> {
    let mut lines = reader.lines();
    let mut buf = String::new();
    while let Some(line) = lines.next_line().await? {
        if !buf.is_empty() { buf.push('\n'); }
        buf.push_str(&line);
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_cli_args() {
        let backend = ClaudeCliBackend::new()
            .with_model("claude-3-opus");

        let request = LlmRequest {
            prompt: "Hello".to_string(),
            system: Some("Be helpful".to_string()),
            max_tokens: Some(100),
            temperature: None,
            timeout_secs: None,
            model_tier: None,
            format_schema: None,
            model: None,
        };

        let model = backend.select_model(request.model_tier.as_ref());
        let args = backend.build_args(&request, model);
        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&"--tools".to_string()));
        assert!(args.contains(&"Read,Glob,Grep,LSP".to_string())); // read-only tools
        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"claude-3-opus".to_string()));
        assert!(args.contains(&"--system-prompt".to_string()));
        assert!(args.contains(&"Be helpful".to_string()));
    }

    #[test]
    fn test_select_model_cheap_tier() {
        let backend = ClaudeCliBackend::new();
        let model = backend.select_model(Some(&ModelTier::Cheap));
        assert_eq!(model, "haiku");
    }

    #[test]
    fn test_select_model_standard_tier() {
        let backend = ClaudeCliBackend::new();
        let model = backend.select_model(Some(&ModelTier::Standard));
        assert_eq!(model, "sonnet");
    }

    #[test]
    fn test_select_model_premium_tier() {
        let backend = ClaudeCliBackend::new();
        let model = backend.select_model(Some(&ModelTier::Premium));
        assert_eq!(model, "opus");
    }

    #[test]
    fn test_select_model_no_tier_uses_default() {
        let backend = ClaudeCliBackend::new();
        let model = backend.select_model(None);
        assert_eq!(model, "sonnet");
    }

    #[test]
    fn test_select_model_no_tier_respects_override() {
        let backend = ClaudeCliBackend::new()
            .with_model("custom-model");
        let model = backend.select_model(None);
        assert_eq!(model, "custom-model");
    }

    #[test]
    fn test_explicit_model_overrides_tier() {
        // Explicit model (CLAUDE_MODEL) wins over tier selection — enables
        // all-opus or all-sonnet test runs via a single env var.
        let backend = ClaudeCliBackend::new()
            .with_model("custom-model");
        let model = backend.select_model(Some(&ModelTier::Cheap));
        assert_eq!(model, "custom-model");
    }

    #[tokio::test]
    async fn test_clean_exit_returns_response() {
        let fixture = format!("{}/tests/fixtures/child_ok.sh", env!("CARGO_MANIFEST_DIR"));
        let backend = ClaudeCliBackend::new()
            .with_command(&fixture)
            .with_timeout(10);

        let request = LlmRequest {
            prompt: "ignored".to_string(),
            system: None,
            max_tokens: None,
            temperature: None,
            timeout_secs: None,
            model_tier: None,
            format_schema: None,
            model: None,
        };

        let response = backend.generate(request).await;
        assert!(response.is_ok(), "expected Ok, got {:?}", response);
        assert_eq!(response.unwrap().text, "mock response");
    }

    #[tokio::test]
    async fn test_nonzero_exit_returns_error() {
        let fixture = format!("{}/tests/fixtures/child_fail.sh", env!("CARGO_MANIFEST_DIR"));
        let backend = ClaudeCliBackend::new()
            .with_command(&fixture)
            .with_timeout(10);

        let request = LlmRequest {
            prompt: "ignored".to_string(),
            system: None,
            max_tokens: None,
            temperature: None,
            timeout_secs: None,
            model_tier: None,
            format_schema: None,
            model: None,
        };

        let result = backend.generate(request).await;
        assert!(result.is_err(), "expected Err, got {:?}", result);
        let err = result.unwrap_err();
        match &err {
            LlmError::ExecutionFailed(msg) => {
                assert!(msg.contains("error output"), "expected stderr text in error, got: {}", msg);
            }
            other => panic!("expected ExecutionFailed, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_timeout_enforced() {
        let fixture = format!("{}/tests/fixtures/child_hang.sh", env!("CARGO_MANIFEST_DIR"));
        let backend = ClaudeCliBackend::new()
            .with_command(&fixture)
            .with_timeout(1);

        let request = LlmRequest {
            prompt: "ignored".to_string(),
            system: None,
            max_tokens: None,
            temperature: None,
            timeout_secs: Some(1),
            model_tier: None,
            format_schema: None,
            model: None,
        };

        let start = std::time::Instant::now();
        let result = backend.generate(request).await;
        let elapsed = start.elapsed();

        assert!(result.is_err(), "expected Err, got {:?}", result);
        match result.unwrap_err() {
            LlmError::Timeout => {} // correct
            other => panic!("expected LlmError::Timeout, got {:?}", other),
        }
        assert!(elapsed < std::time::Duration::from_millis(1200),
            "timeout should fire within deadline + 200ms, but took {:?}", elapsed);
    }

    #[tokio::test]
    async fn test_stderr_flood_no_deadlock() {
        let fixture = format!("{}/tests/fixtures/child_stderr_flood.sh", env!("CARGO_MANIFEST_DIR"));
        let backend = ClaudeCliBackend::new()
            .with_command(&fixture)
            .with_timeout(10);

        let request = LlmRequest {
            prompt: "ignored".to_string(),
            system: None,
            max_tokens: None,
            temperature: None,
            timeout_secs: Some(10),
            model_tier: None,
            format_schema: None,
            model: None,
        };

        let start = std::time::Instant::now();
        let result = backend.generate(request).await;
        let elapsed = start.elapsed();

        assert!(result.is_ok(), "expected Ok, got {:?}", result);
        assert_eq!(result.unwrap().text, "stdout after flood");
        assert!(elapsed < std::time::Duration::from_secs(10),
            "should complete well under 10s, but took {:?}", elapsed);
    }
}
