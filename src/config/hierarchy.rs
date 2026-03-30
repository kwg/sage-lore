// SPDX-License-Identifier: MIT
//! Config hierarchy loading for sage-lore v1.0 (#178, D11, D22, D31).
//!
//! Implements layered configuration resolution — global -> user -> project, most specific wins:
//! - Global: SAGE_LORE_DATADIR or /etc/sage-lore/config.yaml (corp floor, if exists)
//! - User: ~/.config/sage-lore/config.yaml (user overrides, XDG compliant)
//! - Project: .sage-lore/config.yaml (project, most specific wins)
//! - Environment variables override specific mapped fields (D34)
//!
//! All tiers are optional. If none exist, sensible defaults are used.
//! All fields are Option<T> to prevent serde default poisoning during merge (D31).
//! Defaults are applied AFTER merge via with_defaults().
//!
//! Security policy uses a SEPARATE ratchet merge (Policy::merge), not this system (D37).

use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

use super::resolver::PathResolver;

// ── Config Schema v1.0 (D11) ───────────────────────────────────────────

/// Main configuration structure for sage-lore.
///
/// All fields are Option<T> for correct merge semantics (D31):
/// absent fields don't override during shallow merge.
/// Call `with_defaults()` after merging all tiers.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub project: Option<ProjectConfig>,
    pub platform: Option<PlatformConfig>,
    pub llm: Option<LlmConfig>,
    pub test: Option<TestConfig>,
    pub state: Option<StateConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectConfig {
    pub name: Option<String>,
    pub root: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlatformConfig {
    pub provider: Option<String>,
    pub url: Option<String>,
    pub repo: Option<String>,
    /// Environment variable name holding the API token (D13, D15).
    /// The resolved value NEVER enters this struct.
    pub token_env: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmConfig {
    pub backend: Option<String>,
    pub context_limit: Option<u64>,
    pub claude: Option<ClaudeConfig>,
    pub ollama: Option<OllamaConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClaudeConfig {
    pub cheap: Option<String>,
    pub standard: Option<String>,
    pub premium: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OllamaConfig {
    pub url: Option<String>,
    pub cheap: Option<String>,
    pub standard: Option<String>,
    pub premium: Option<String>,
    pub num_ctx: Option<u64>,
    pub think: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestConfig {
    pub framework: Option<String>,
    pub command: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub max_output_chars: Option<u64>,
    pub coverage: Option<CoverageConfig>,
    pub flaky: Option<FlakyConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CoverageConfig {
    pub min_lines_percent: Option<u64>,
    pub min_branches_percent: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FlakyConfig {
    pub retry_count: Option<u64>,
}

/// State tracking configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StateConfig {
    /// Whether .sage-lore/ should be git-tracked (D5). Default: true.
    pub git_tracked: Option<bool>,
}

// ── Merge ───────────────────────────────────────────────────────────────

/// Merge two Option values: overlay wins if Some, otherwise keep base.
fn merge_opt<T>(base: Option<T>, overlay: Option<T>) -> Option<T> {
    overlay.or(base)
}

impl Config {
    /// Shallow merge: overlay values win when present (D8, D31).
    /// None fields pass through from base.
    pub fn merge(self, overlay: Config) -> Config {
        Config {
            project: merge_project(self.project, overlay.project),
            platform: merge_platform(self.platform, overlay.platform),
            llm: merge_llm(self.llm, overlay.llm),
            test: merge_test(self.test, overlay.test),
            state: merge_state(self.state, overlay.state),
        }
    }

    /// Apply environment variable overrides (D34).
    /// Existing env vars take precedence over all config tiers.
    pub fn apply_env(mut self) -> Self {
        // LLM overrides
        if let Ok(v) = std::env::var("SAGE_LLM_BACKEND") {
            self.llm.get_or_insert_with(Default::default).backend = Some(v);
        }
        if let Ok(v) = std::env::var("SAGE_CONTEXT_LIMIT") {
            if let Ok(n) = v.parse() {
                self.llm.get_or_insert_with(Default::default).context_limit = Some(n);
            }
        }

        // Claude overrides
        if let Ok(v) = std::env::var("CLAUDE_MODEL") {
            self.llm.get_or_insert_with(Default::default)
                .claude.get_or_insert_with(Default::default).standard = Some(v);
        }
        if let Ok(v) = std::env::var("CLAUDE_CHEAP_MODEL") {
            self.llm.get_or_insert_with(Default::default)
                .claude.get_or_insert_with(Default::default).cheap = Some(v);
        }
        if let Ok(v) = std::env::var("CLAUDE_PREMIUM_MODEL") {
            self.llm.get_or_insert_with(Default::default)
                .claude.get_or_insert_with(Default::default).premium = Some(v);
        }

        // Ollama overrides
        if let Ok(v) = std::env::var("OLLAMA_URL") {
            self.llm.get_or_insert_with(Default::default)
                .ollama.get_or_insert_with(Default::default).url = Some(v);
        }
        if let Ok(v) = std::env::var("OLLAMA_MODEL") {
            self.llm.get_or_insert_with(Default::default)
                .ollama.get_or_insert_with(Default::default).standard = Some(v);
        }
        if let Ok(v) = std::env::var("OLLAMA_NUM_CTX") {
            if let Ok(n) = v.parse() {
                self.llm.get_or_insert_with(Default::default)
                    .ollama.get_or_insert_with(Default::default).num_ctx = Some(n);
            }
        }
        if let Ok(v) = std::env::var("OLLAMA_THINK") {
            self.llm.get_or_insert_with(Default::default)
                .ollama.get_or_insert_with(Default::default).think = Some(v == "true" || v == "1");
        }

        // Platform overrides
        if let Ok(v) = std::env::var("FORGEJO_URL") {
            self.platform.get_or_insert_with(Default::default).url = Some(v);
        }
        if let Ok(v) = std::env::var("FORGEJO_REPO") {
            self.platform.get_or_insert_with(Default::default).repo = Some(v);
        }

        self
    }

    /// Fill None fields with sensible defaults. Call AFTER merge + apply_env.
    pub fn with_defaults(mut self) -> Self {
        let project = self.project.get_or_insert_with(Default::default);
        if project.root.is_none() {
            project.root = Some(".".to_string());
        }

        let llm = self.llm.get_or_insert_with(Default::default);
        if llm.backend.is_none() {
            llm.backend = Some("claude".to_string());
        }
        if llm.context_limit.is_none() {
            llm.context_limit = Some(100_000);
        }

        let claude = llm.claude.get_or_insert_with(Default::default);
        if claude.cheap.is_none() {
            claude.cheap = Some("claude-haiku-4-5-20251001".to_string());
        }
        if claude.standard.is_none() {
            claude.standard = Some("claude-sonnet-4-6".to_string());
        }
        if claude.premium.is_none() {
            claude.premium = Some("claude-opus-4-6".to_string());
        }

        let ollama = llm.ollama.get_or_insert_with(Default::default);
        if ollama.url.is_none() {
            ollama.url = Some("http://localhost:11434".to_string());
        }
        if ollama.cheap.is_none() {
            ollama.cheap = Some("phi4-mini".to_string());
        }
        if ollama.standard.is_none() {
            ollama.standard = Some("qwen2.5-coder:32b".to_string());
        }
        if ollama.premium.is_none() {
            ollama.premium = Some("deepseek-r1:32b".to_string());
        }
        if ollama.think.is_none() {
            ollama.think = Some(false);
        }

        let test = self.test.get_or_insert_with(Default::default);
        if test.framework.is_none() {
            test.framework = Some("auto".to_string());
        }
        if test.timeout_seconds.is_none() {
            test.timeout_seconds = Some(300);
        }
        if test.max_output_chars.is_none() {
            test.max_output_chars = Some(50_000);
        }
        let coverage = test.coverage.get_or_insert_with(Default::default);
        if coverage.min_lines_percent.is_none() {
            coverage.min_lines_percent = Some(80);
        }
        if coverage.min_branches_percent.is_none() {
            coverage.min_branches_percent = Some(70);
        }
        let flaky = test.flaky.get_or_insert_with(Default::default);
        if flaky.retry_count.is_none() {
            flaky.retry_count = Some(2);
        }

        let state = self.state.get_or_insert_with(Default::default);
        if state.git_tracked.is_none() {
            state.git_tracked = Some(true);
        }

        // Platform: token_env default
        if let Some(ref mut platform) = self.platform {
            if platform.token_env.is_none() && platform.url.is_some() {
                platform.token_env = Some("FORGEJO_API_TOKEN".to_string());
            }
        }

        self
    }

    // ── Convenience accessors ───────────────────────────────────────────

    pub fn project_name(&self) -> &str {
        self.project.as_ref()
            .and_then(|p| p.name.as_deref())
            .unwrap_or("")
    }

    pub fn llm_backend(&self) -> &str {
        self.llm.as_ref()
            .and_then(|l| l.backend.as_deref())
            .unwrap_or("claude")
    }

    pub fn context_limit(&self) -> u64 {
        self.llm.as_ref()
            .and_then(|l| l.context_limit)
            .unwrap_or(100_000)
    }

    pub fn git_tracked(&self) -> bool {
        self.state.as_ref()
            .and_then(|s| s.git_tracked)
            .unwrap_or(true)
    }

    pub fn platform_url(&self) -> Option<&str> {
        self.platform.as_ref().and_then(|p| p.url.as_deref())
    }

    pub fn platform_repo(&self) -> Option<&str> {
        self.platform.as_ref().and_then(|p| p.repo.as_deref())
    }

    pub fn platform_token_env(&self) -> Option<&str> {
        self.platform.as_ref().and_then(|p| p.token_env.as_deref())
    }
}

// ── Per-section merge helpers ───────────────────────────────────────────

fn merge_project(base: Option<ProjectConfig>, overlay: Option<ProjectConfig>) -> Option<ProjectConfig> {
    match (base, overlay) {
        (None, None) => None,
        (None, Some(o)) => Some(o),
        (Some(b), None) => Some(b),
        (Some(b), Some(o)) => Some(ProjectConfig {
            name: merge_opt(b.name, o.name),
            root: merge_opt(b.root, o.root),
        }),
    }
}

fn merge_platform(base: Option<PlatformConfig>, overlay: Option<PlatformConfig>) -> Option<PlatformConfig> {
    match (base, overlay) {
        (None, None) => None,
        (None, Some(o)) => Some(o),
        (Some(b), None) => Some(b),
        (Some(b), Some(o)) => Some(PlatformConfig {
            provider: merge_opt(b.provider, o.provider),
            url: merge_opt(b.url, o.url),
            repo: merge_opt(b.repo, o.repo),
            token_env: merge_opt(b.token_env, o.token_env),
        }),
    }
}

fn merge_llm(base: Option<LlmConfig>, overlay: Option<LlmConfig>) -> Option<LlmConfig> {
    match (base, overlay) {
        (None, None) => None,
        (None, Some(o)) => Some(o),
        (Some(b), None) => Some(b),
        (Some(b), Some(o)) => Some(LlmConfig {
            backend: merge_opt(b.backend, o.backend),
            context_limit: merge_opt(b.context_limit, o.context_limit),
            claude: merge_claude(b.claude, o.claude),
            ollama: merge_ollama(b.ollama, o.ollama),
        }),
    }
}

fn merge_claude(base: Option<ClaudeConfig>, overlay: Option<ClaudeConfig>) -> Option<ClaudeConfig> {
    match (base, overlay) {
        (None, None) => None,
        (None, Some(o)) => Some(o),
        (Some(b), None) => Some(b),
        (Some(b), Some(o)) => Some(ClaudeConfig {
            cheap: merge_opt(b.cheap, o.cheap),
            standard: merge_opt(b.standard, o.standard),
            premium: merge_opt(b.premium, o.premium),
        }),
    }
}

fn merge_ollama(base: Option<OllamaConfig>, overlay: Option<OllamaConfig>) -> Option<OllamaConfig> {
    match (base, overlay) {
        (None, None) => None,
        (None, Some(o)) => Some(o),
        (Some(b), None) => Some(b),
        (Some(b), Some(o)) => Some(OllamaConfig {
            url: merge_opt(b.url, o.url),
            cheap: merge_opt(b.cheap, o.cheap),
            standard: merge_opt(b.standard, o.standard),
            premium: merge_opt(b.premium, o.premium),
            num_ctx: merge_opt(b.num_ctx, o.num_ctx),
            think: merge_opt(b.think, o.think),
        }),
    }
}

fn merge_test(base: Option<TestConfig>, overlay: Option<TestConfig>) -> Option<TestConfig> {
    match (base, overlay) {
        (None, None) => None,
        (None, Some(o)) => Some(o),
        (Some(b), None) => Some(b),
        (Some(b), Some(o)) => Some(TestConfig {
            framework: merge_opt(b.framework, o.framework),
            command: merge_opt(b.command, o.command),
            timeout_seconds: merge_opt(b.timeout_seconds, o.timeout_seconds),
            max_output_chars: merge_opt(b.max_output_chars, o.max_output_chars),
            coverage: merge_coverage(b.coverage, o.coverage),
            flaky: merge_flaky(b.flaky, o.flaky),
        }),
    }
}

fn merge_coverage(base: Option<CoverageConfig>, overlay: Option<CoverageConfig>) -> Option<CoverageConfig> {
    match (base, overlay) {
        (None, None) => None,
        (None, Some(o)) => Some(o),
        (Some(b), None) => Some(b),
        (Some(b), Some(o)) => Some(CoverageConfig {
            min_lines_percent: merge_opt(b.min_lines_percent, o.min_lines_percent),
            min_branches_percent: merge_opt(b.min_branches_percent, o.min_branches_percent),
        }),
    }
}

fn merge_flaky(base: Option<FlakyConfig>, overlay: Option<FlakyConfig>) -> Option<FlakyConfig> {
    match (base, overlay) {
        (None, None) => None,
        (None, Some(o)) => Some(o),
        (Some(b), None) => Some(b),
        (Some(b), Some(o)) => Some(FlakyConfig {
            retry_count: merge_opt(b.retry_count, o.retry_count),
        }),
    }
}

fn merge_state(base: Option<StateConfig>, overlay: Option<StateConfig>) -> Option<StateConfig> {
    match (base, overlay) {
        (None, None) => None,
        (None, Some(o)) => Some(o),
        (Some(b), None) => Some(b),
        (Some(b), Some(o)) => Some(StateConfig {
            git_tracked: merge_opt(b.git_tracked, o.git_tracked),
        }),
    }
}

// ── Config Loader ───────────────────────────────────────────────────────

/// Configuration loader with hierarchy support.
///
/// Uses PathResolver for tier discovery, then merges configs left-to-right
/// (global → user → project), applies env overrides, and fills defaults.
///
/// NOTE: This merge system is for general config only (D37).
/// Security policy uses Policy::merge() with ratchet semantics.
pub struct ConfigLoader;

impl ConfigLoader {
    /// Load configuration from the three-tier hierarchy.
    ///
    /// Resolution order (most specific wins, D12):
    /// 1. Global: discovered by PathResolver (optional)
    /// 2. User: ~/.config/sage-lore/config.yaml (optional)
    /// 3. Project: .sage-lore/config.yaml (optional)
    /// 4. Environment variables (highest priority, D34)
    /// 5. Defaults (fill remaining None fields)
    pub fn load(resolver: &PathResolver) -> Result<Config, ConfigError> {
        let mut config = Config::default();

        // Load and merge tiers in order (global → user → project)
        for path in resolver.resolve_config_files() {
            let tier = Self::load_file(&path)?;
            config = config.merge(tier);
        }

        // Legacy migration: detect .sage-project.yaml (D16, D26)
        if let Some(project_dir) = resolver.project_dir() {
            let legacy_path = project_dir.join(".sage-project.yaml");
            if legacy_path.exists() {
                tracing::warn!(
                    "Found .sage-project.yaml at {} — migrate to .sage-lore/config.yaml",
                    legacy_path.display()
                );
                if let Ok(legacy) = Self::load_legacy(&legacy_path) {
                    config = config.merge(legacy);
                }
            }
        }

        // Apply env var overrides and fill defaults
        config = config.apply_env().with_defaults();

        // Gitignore lint (D5)
        Self::check_gitignore_mismatch(&config, resolver);

        Ok(config)
    }

    /// Load from a project root directly (convenience for existing callers).
    pub fn load_from_project(project_root: &Path) -> Result<Config, ConfigError> {
        let resolver = PathResolver::discover(project_root);
        Self::load(&resolver)
    }

    /// Load a single config file.
    fn load_file(path: &Path) -> Result<Config, ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::ReadError(path.display().to_string(), e))?;

        let config: Config = serde_yaml::from_str(&content)
            .map_err(|e| ConfigError::ParseError(path.display().to_string(), e))?;

        Ok(config)
    }

    /// Load legacy .sage-project.yaml and map to new schema (D26).
    fn load_legacy(path: &Path) -> Result<Config, ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::ReadError(path.display().to_string(), e))?;

        // Legacy schema has flat fields
        #[derive(Deserialize)]
        struct LegacyConfig {
            project_name: Option<String>,
            project_root: Option<String>,
            #[serde(default)]
            state: Option<LegacyState>,
        }
        #[derive(Deserialize)]
        struct LegacyState {
            git_tracked: Option<bool>,
        }

        let legacy: LegacyConfig = serde_yaml::from_str(&content)
            .map_err(|e| ConfigError::ParseError(path.display().to_string(), e))?;

        Ok(Config {
            project: Some(ProjectConfig {
                name: legacy.project_name,
                root: legacy.project_root,
            }),
            state: legacy.state.map(|s| StateConfig {
                git_tracked: s.git_tracked,
            }),
            ..Default::default()
        })
    }

    /// Check for gitignore mismatch (D5): warn if config says git_tracked
    /// but .gitignore excludes .sage-lore/.
    fn check_gitignore_mismatch(config: &Config, resolver: &PathResolver) {
        if !config.git_tracked() {
            return;
        }

        if let Some(project_dir) = resolver.project_dir() {
            let gitignore = project_dir.join(".gitignore");
            if let Ok(content) = std::fs::read_to_string(gitignore) {
                if content.lines().any(|line| {
                    let trimmed = line.trim();
                    trimmed == ".sage-lore/" || trimmed == ".sage-lore" || trimmed == ".sage-lore/**"
                }) {
                    tracing::warn!(
                        "config.state.git_tracked is true but .gitignore excludes .sage-lore/ — \
                         either set git_tracked: false or remove .sage-lore from .gitignore"
                    );
                }
            }
        }
    }
}

/// Configuration errors.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config from {0}: {1}")]
    ReadError(String, #[source] std::io::Error),

    #[error("Failed to parse config from {0}: {1}")]
    ParseError(String, #[source] serde_yaml::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_default_config_with_defaults() {
        let config = Config::default().with_defaults();
        assert_eq!(config.project_name(), "");
        assert_eq!(config.llm_backend(), "claude");
        assert_eq!(config.context_limit(), 100_000);
        assert!(config.git_tracked());
    }

    #[test]
    fn test_merge_overlay_wins() {
        let base = Config {
            project: Some(ProjectConfig {
                name: Some("base".to_string()),
                root: Some(".".to_string()),
            }),
            ..Default::default()
        };
        let overlay = Config {
            project: Some(ProjectConfig {
                name: Some("overlay".to_string()),
                root: None, // absent — should not override
            }),
            ..Default::default()
        };

        let merged = base.merge(overlay);
        assert_eq!(merged.project.as_ref().unwrap().name.as_deref(), Some("overlay"));
        assert_eq!(merged.project.as_ref().unwrap().root.as_deref(), Some(".")); // base preserved
    }

    #[test]
    fn test_merge_absent_fields_dont_override() {
        let base = Config {
            llm: Some(LlmConfig {
                backend: Some("ollama".to_string()),
                context_limit: Some(50_000),
                ..Default::default()
            }),
            ..Default::default()
        };
        let overlay = Config {
            llm: Some(LlmConfig {
                backend: Some("claude".to_string()),
                context_limit: None, // absent — keep base
                ..Default::default()
            }),
            ..Default::default()
        };

        let merged = base.merge(overlay);
        let llm = merged.llm.unwrap();
        assert_eq!(llm.backend.as_deref(), Some("claude"));
        assert_eq!(llm.context_limit, Some(50_000)); // base preserved
    }

    #[test]
    fn test_load_project_config() {
        let temp = TempDir::new().unwrap();
        let sage_dir = temp.path().join(".sage-lore");
        fs::create_dir_all(&sage_dir).unwrap();

        let config_content = r#"
project:
  name: test-project
llm:
  backend: ollama
state:
  git_tracked: false
"#;
        fs::write(sage_dir.join("config.yaml"), config_content).unwrap();

        let config = ConfigLoader::load_from_project(temp.path()).unwrap();
        assert_eq!(config.project_name(), "test-project");
        assert_eq!(config.llm_backend(), "ollama");
        assert!(!config.git_tracked());
    }

    #[test]
    fn test_load_defaults_when_no_config() {
        let temp = TempDir::new().unwrap();
        // No .sage-lore/ at all
        let config = ConfigLoader::load_from_project(temp.path()).unwrap();
        assert_eq!(config.llm_backend(), "claude");
        assert_eq!(config.context_limit(), 100_000);
        assert!(config.git_tracked());
    }

    #[test]
    fn test_legacy_migration() {
        let temp = TempDir::new().unwrap();
        // Create .git so project root is detected
        fs::create_dir_all(temp.path().join(".git")).unwrap();
        fs::create_dir_all(temp.path().join(".sage-lore")).unwrap();

        let legacy = r#"
project_name: legacy-project
project_root: "."
state:
  git_tracked: false
"#;
        fs::write(temp.path().join(".sage-project.yaml"), legacy).unwrap();

        let config = ConfigLoader::load_from_project(temp.path()).unwrap();
        assert_eq!(config.project_name(), "legacy-project");
        assert!(!config.git_tracked());
    }

    #[test]
    fn test_three_tier_merge() {
        let temp = TempDir::new().unwrap();
        let global = temp.path().join("global");
        let user = temp.path().join("user");
        let project = temp.path().join("project/.sage-lore");

        fs::create_dir_all(global.join("config")).unwrap();
        fs::write(global.join("config/config.yaml"), r#"
llm:
  backend: ollama
  context_limit: 50000
project:
  name: global-name
"#).unwrap();

        fs::create_dir_all(&user).unwrap();
        fs::write(user.join("config.yaml"), r#"
llm:
  context_limit: 75000
"#).unwrap();

        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("config.yaml"), r#"
project:
  name: project-name
"#).unwrap();

        let resolver = PathResolver::with_paths(Some(global), user, Some(project));
        let config = ConfigLoader::load(&resolver).unwrap();

        // project.name: project wins over global
        assert_eq!(config.project_name(), "project-name");
        // llm.backend: global set it, nobody overrode
        assert_eq!(config.llm_backend(), "ollama");
        // llm.context_limit: user (75000) wins over global (50000)
        assert_eq!(config.context_limit(), 75_000);
    }

    #[test]
    fn test_gitignore_lint_no_panic() {
        let temp = TempDir::new().unwrap();
        let project = temp.path().join(".sage-lore");
        fs::create_dir_all(&project).unwrap();
        fs::write(temp.path().join(".gitignore"), ".sage-lore/\n").unwrap();
        fs::write(project.join("config.yaml"), "state:\n  git_tracked: true\n").unwrap();

        // Should not panic, just warn
        let _ = ConfigLoader::load_from_project(temp.path());
    }

    #[test]
    fn test_platform_token_env_default() {
        let config = Config {
            platform: Some(PlatformConfig {
                url: Some("http://example.com".to_string()),
                repo: Some("owner/repo".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        }.with_defaults();

        assert_eq!(config.platform_token_env(), Some("FORGEJO_API_TOKEN"));
    }
}
