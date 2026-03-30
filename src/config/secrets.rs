// SPDX-License-Identifier: MIT
//! Secret injection and resolution for scrolls.
//!
//! Implements the secret resolution order — env var, then .env file, then secrets.yaml.
//! Engine refuses to run without a policy (D10); secrets trigger abort-and-reset (D11).
//!
//! SECURITY: Resolved secret values are NEVER logged.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Resolves secrets from environment, .env, and secrets.yaml
/// in priority order.
#[derive(Debug, Clone)]
pub struct SecretResolver {
    /// Project root for .env lookup
    project_root: PathBuf,
    /// Cached .env variables
    dotenv_cache: HashMap<String, String>,
    /// Cached secrets.yaml variables
    secrets_yaml_cache: HashMap<String, String>,
}

impl SecretResolver {
    /// Create a new SecretResolver for the given project root.
    ///
    /// The constructor loads and caches both .env and secrets.yaml
    /// to avoid repeated file reads during resolution.
    pub fn new(project_root: &Path) -> Self {
        let dotenv_cache = Self::load_dotenv(project_root);
        let secrets_yaml_cache = Self::load_secrets_yaml();

        Self {
            project_root: project_root.to_path_buf(),
            dotenv_cache,
            secrets_yaml_cache,
        }
    }

    /// Resolve a secret variable using the priority order:
    /// 1. Environment variable
    /// 2. .env file in project root
    /// 3. ~/.config/sage-lore/secrets.yaml
    ///
    /// Returns None if the variable is not found in any source.
    ///
    /// SECURITY: This method never logs the resolved value.
    pub fn resolve(&self, var_name: &str) -> Option<String> {
        // 1. Check environment variable first
        if let Ok(value) = std::env::var(var_name) {
            tracing::debug!(var_name = %var_name, "Resolved from environment");
            return Some(value);
        }

        // 2. Check .env file
        if let Some(value) = self.dotenv_cache.get(var_name) {
            tracing::debug!(var_name = %var_name, "Resolved from .env");
            return Some(value.clone());
        }

        // 3. Check secrets.yaml
        if let Some(value) = self.secrets_yaml_cache.get(var_name) {
            tracing::debug!(var_name = %var_name, "Resolved from secrets.yaml");
            return Some(value.clone());
        }

        tracing::warn!(var_name = %var_name, "Secret not found in any source");
        None
    }

    /// Load and parse .env file from project root.
    ///
    /// Format:
    /// ```text
    /// KEY=value
    /// ANOTHER_KEY=another value
    /// # Comments are ignored
    /// ```
    fn load_dotenv(project_root: &Path) -> HashMap<String, String> {
        let dotenv_path = project_root.join(".env");

        if !dotenv_path.exists() {
            tracing::debug!(path = %dotenv_path.display(), "No .env file found");
            return HashMap::new();
        }

        match fs::read_to_string(&dotenv_path) {
            Ok(contents) => {
                tracing::debug!(path = %dotenv_path.display(), "Loaded .env file");
                Self::parse_dotenv(&contents)
            }
            Err(e) => {
                tracing::warn!(path = %dotenv_path.display(), error = %e, "Failed to read .env file");
                HashMap::new()
            }
        }
    }

    /// Parse .env file contents into a HashMap.
    ///
    /// Rules:
    /// - Lines starting with # are comments
    /// - Empty lines are ignored
    /// - Format: KEY=value
    /// - No quote handling (values are taken as-is after =)
    fn parse_dotenv(contents: &str) -> HashMap<String, String> {
        let mut map = HashMap::new();

        for line in contents.lines() {
            let line = line.trim();

            // Skip comments and empty lines
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Split on first '=' only
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].trim();
                let value = line[eq_pos + 1..].trim();

                if !key.is_empty() {
                    map.insert(key.to_string(), value.to_string());
                }
            }
        }

        map
    }

    /// Load and parse secrets.yaml from ~/.config/sage-lore/secrets.yaml
    ///
    /// Format:
    /// ```yaml
    /// ANTHROPIC_API_KEY: sk-ant-...
    /// OTHER_SECRET: value
    /// ```
    fn load_secrets_yaml() -> HashMap<String, String> {
        let secrets_path = match dirs::config_dir() {
            Some(config_dir) => config_dir.join("sage-lore").join("secrets.yaml"),
            None => {
                tracing::warn!("Could not determine config directory for secrets.yaml");
                return HashMap::new();
            }
        };

        if !secrets_path.exists() {
            tracing::debug!(path = %secrets_path.display(), "No secrets.yaml file found");
            return HashMap::new();
        }

        match fs::read_to_string(&secrets_path) {
            Ok(contents) => {
                tracing::debug!(path = %secrets_path.display(), "Loaded secrets.yaml file");
                Self::parse_secrets_yaml(&contents)
            }
            Err(e) => {
                tracing::warn!(path = %secrets_path.display(), error = %e, "Failed to read secrets.yaml");
                HashMap::new()
            }
        }
    }

    /// Parse secrets.yaml contents into a HashMap.
    ///
    /// Expects a flat mapping of string keys to string values.
    /// Non-string values are converted to strings.
    fn parse_secrets_yaml(contents: &str) -> HashMap<String, String> {
        match serde_yaml::from_str::<serde_yaml::Value>(contents) {
            Ok(serde_yaml::Value::Mapping(map)) => {
                let mut result = HashMap::new();

                for (key, value) in map {
                    if let Some(key_str) = key.as_str() {
                        let value_str = match value {
                            serde_yaml::Value::String(s) => s,
                            serde_yaml::Value::Number(n) => n.to_string(),
                            serde_yaml::Value::Bool(b) => b.to_string(),
                            _ => {
                                tracing::warn!(key = %key_str, "Skipping non-scalar value in secrets.yaml");
                                continue;
                            }
                        };
                        result.insert(key_str.to_string(), value_str);
                    }
                }

                result
            }
            Ok(_) => {
                tracing::warn!("secrets.yaml is not a mapping");
                HashMap::new()
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to parse secrets.yaml");
                HashMap::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::TempDir;

    // ========================================================================
    // .env Parsing Tests
    // ========================================================================

    #[test]
    fn test_parse_dotenv_simple() {
        let contents = "KEY=value\n";
        let result = SecretResolver::parse_dotenv(contents);
        assert_eq!(result.get("KEY"), Some(&"value".to_string()));
    }

    #[test]
    fn test_parse_dotenv_multiple_lines() {
        let contents = "KEY1=value1\nKEY2=value2\n";
        let result = SecretResolver::parse_dotenv(contents);
        assert_eq!(result.get("KEY1"), Some(&"value1".to_string()));
        assert_eq!(result.get("KEY2"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_parse_dotenv_with_spaces() {
        let contents = "KEY = value with spaces\n";
        let result = SecretResolver::parse_dotenv(contents);
        assert_eq!(result.get("KEY"), Some(&"value with spaces".to_string()));
    }

    #[test]
    fn test_parse_dotenv_ignores_comments() {
        let contents = "# This is a comment\nKEY=value\n";
        let result = SecretResolver::parse_dotenv(contents);
        assert_eq!(result.len(), 1);
        assert_eq!(result.get("KEY"), Some(&"value".to_string()));
    }

    #[test]
    fn test_parse_dotenv_ignores_empty_lines() {
        let contents = "KEY1=value1\n\nKEY2=value2\n";
        let result = SecretResolver::parse_dotenv(contents);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_parse_dotenv_handles_equals_in_value() {
        let contents = "KEY=value=with=equals\n";
        let result = SecretResolver::parse_dotenv(contents);
        assert_eq!(result.get("KEY"), Some(&"value=with=equals".to_string()));
    }

    #[test]
    fn test_parse_dotenv_empty_value() {
        let contents = "KEY=\n";
        let result = SecretResolver::parse_dotenv(contents);
        assert_eq!(result.get("KEY"), Some(&"".to_string()));
    }

    // ========================================================================
    // secrets.yaml Parsing Tests
    // ========================================================================

    #[test]
    fn test_parse_secrets_yaml_simple() {
        let contents = "ANTHROPIC_API_KEY: sk-ant-123\n";
        let result = SecretResolver::parse_secrets_yaml(contents);
        assert_eq!(result.get("ANTHROPIC_API_KEY"), Some(&"sk-ant-123".to_string()));
    }

    #[test]
    fn test_parse_secrets_yaml_multiple() {
        let contents = "KEY1: value1\nKEY2: value2\n";
        let result = SecretResolver::parse_secrets_yaml(contents);
        assert_eq!(result.get("KEY1"), Some(&"value1".to_string()));
        assert_eq!(result.get("KEY2"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_parse_secrets_yaml_number_values() {
        let contents = "PORT: 8080\n";
        let result = SecretResolver::parse_secrets_yaml(contents);
        assert_eq!(result.get("PORT"), Some(&"8080".to_string()));
    }

    #[test]
    fn test_parse_secrets_yaml_bool_values() {
        let contents = "ENABLED: true\n";
        let result = SecretResolver::parse_secrets_yaml(contents);
        assert_eq!(result.get("ENABLED"), Some(&"true".to_string()));
    }

    #[test]
    fn test_parse_secrets_yaml_empty() {
        let contents = "";
        let result = SecretResolver::parse_secrets_yaml(contents);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_parse_secrets_yaml_invalid_yaml() {
        let contents = "invalid: yaml: syntax:\n";
        let result = SecretResolver::parse_secrets_yaml(contents);
        // Should handle gracefully and return empty
        assert_eq!(result.len(), 0);
    }

    // ========================================================================
    // Resolution Order Tests
    // ========================================================================

    #[test]
    fn test_resolve_from_environment() {
        let temp_dir = TempDir::new().unwrap();

        // Set env var
        env::set_var("TEST_SECRET_ENV", "from_env");

        let resolver = SecretResolver::new(temp_dir.path());
        let result = resolver.resolve("TEST_SECRET_ENV");

        assert_eq!(result, Some("from_env".to_string()));

        // Cleanup
        env::remove_var("TEST_SECRET_ENV");
    }

    #[test]
    fn test_resolve_from_dotenv() {
        let temp_dir = TempDir::new().unwrap();
        let dotenv_path = temp_dir.path().join(".env");

        // Create .env file
        fs::write(&dotenv_path, "TEST_SECRET_DOTENV=from_dotenv\n").unwrap();

        let resolver = SecretResolver::new(temp_dir.path());
        let result = resolver.resolve("TEST_SECRET_DOTENV");

        assert_eq!(result, Some("from_dotenv".to_string()));
    }

    #[test]
    fn test_resolve_priority_env_over_dotenv() {
        let temp_dir = TempDir::new().unwrap();
        let dotenv_path = temp_dir.path().join(".env");

        // Create .env file
        fs::write(&dotenv_path, "TEST_PRIORITY=from_dotenv\n").unwrap();

        // Set env var (should win)
        env::set_var("TEST_PRIORITY", "from_env");

        let resolver = SecretResolver::new(temp_dir.path());
        let result = resolver.resolve("TEST_PRIORITY");

        assert_eq!(result, Some("from_env".to_string()));

        // Cleanup
        env::remove_var("TEST_PRIORITY");
    }

    #[test]
    fn test_resolve_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let resolver = SecretResolver::new(temp_dir.path());

        let result = resolver.resolve("NONEXISTENT_SECRET");
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_with_missing_files() {
        let temp_dir = TempDir::new().unwrap();
        // Don't create any files

        let resolver = SecretResolver::new(temp_dir.path());

        // Should not panic, just return None
        let result = resolver.resolve("ANY_KEY");
        assert_eq!(result, None);
    }

    // ========================================================================
    // Integration Tests
    // ========================================================================

    #[test]
    fn test_resolver_caches_files() {
        let temp_dir = TempDir::new().unwrap();
        let dotenv_path = temp_dir.path().join(".env");

        fs::write(&dotenv_path, "CACHED_KEY=cached_value\n").unwrap();

        let resolver = SecretResolver::new(temp_dir.path());

        // Resolve once
        let result1 = resolver.resolve("CACHED_KEY");
        assert_eq!(result1, Some("cached_value".to_string()));

        // Delete the file
        fs::remove_file(&dotenv_path).unwrap();

        // Resolve again - should still work from cache
        let result2 = resolver.resolve("CACHED_KEY");
        assert_eq!(result2, Some("cached_value".to_string()));
    }

    #[test]
    fn test_multiple_resolves() {
        let temp_dir = TempDir::new().unwrap();
        let dotenv_path = temp_dir.path().join(".env");

        fs::write(&dotenv_path, "KEY1=value1\nKEY2=value2\n").unwrap();

        let resolver = SecretResolver::new(temp_dir.path());

        assert_eq!(resolver.resolve("KEY1"), Some("value1".to_string()));
        assert_eq!(resolver.resolve("KEY2"), Some("value2".to_string()));
        assert_eq!(resolver.resolve("KEY3"), None);
    }
}
