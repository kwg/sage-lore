// SPDX-License-Identifier: MIT
//! Error types and framework enum for the test primitive.

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during test operations.
#[derive(Debug, Error)]
pub enum TestError {
    /// No test framework detected in the project
    #[error("No test framework detected in project. {0}")]
    NoFrameworkDetected(String),

    /// Framework is not supported
    #[error("Unsupported test framework: {0}")]
    UnsupportedFramework(String),

    /// Test execution failed
    #[error("Test execution failed: {0}")]
    ExecutionFailed(String),

    /// Failed to parse test output
    #[error("Failed to parse test output: {0}")]
    ParseError(String),

    /// Test run timed out
    #[error("Test run timed out after {0} seconds")]
    Timeout(u64),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// IO error during test operations
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Watch mode not supported for this framework
    #[error("Watch mode not supported for framework: {0}")]
    WatchNotSupported(String),

    /// Coverage not supported for this framework
    #[error("Coverage not supported for framework: {0}")]
    CoverageNotSupported(String),
}

/// Result type for test operations.
pub type TestResult<T> = Result<T, TestError>;

// ============================================================================
// Framework Enum
// ============================================================================

/// Supported test frameworks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Framework {
    /// Rust: cargo test
    Cargo,
    /// Node.js: Jest
    Jest,
    /// Node.js: Vitest
    Vitest,
    /// Node.js: generic npm test
    Npm,
    /// Python: pytest
    Pytest,
    /// Go: go test
    Go,
    /// Bash: bats
    Bats,
    /// Generic: make test
    Make,
}

impl Framework {
    /// Get the default test command for this framework.
    pub fn default_command(&self) -> &'static str {
        match self {
            Framework::Cargo => "cargo test",
            Framework::Jest => "npx jest",
            Framework::Vitest => "npx vitest run",
            Framework::Npm => "npm test",
            Framework::Pytest => "pytest",
            Framework::Go => "go test ./...",
            Framework::Bats => "bats tests/",
            Framework::Make => "make test",
        }
    }

    /// Check if this framework supports coverage reporting.
    pub fn supports_coverage(&self) -> bool {
        matches!(
            self,
            Framework::Cargo
                | Framework::Jest
                | Framework::Vitest
                | Framework::Pytest
                | Framework::Go
        )
    }

    /// Check if this framework supports watch mode.
    pub fn supports_watch(&self) -> bool {
        matches!(
            self,
            Framework::Cargo
                | Framework::Jest
                | Framework::Vitest
                | Framework::Pytest
                | Framework::Go
        )
    }

    /// Get the coverage command for this framework.
    pub fn coverage_command(&self) -> Option<&'static str> {
        match self {
            Framework::Cargo => Some("cargo llvm-cov"),
            Framework::Jest => Some("npx jest --coverage"),
            Framework::Vitest => Some("npx vitest run --coverage"),
            Framework::Pytest => Some("pytest --cov"),
            Framework::Go => Some("go test -cover ./..."),
            _ => None,
        }
    }

    /// Get the watch command for this framework.
    pub fn watch_command(&self) -> Option<&'static str> {
        match self {
            Framework::Cargo => Some("cargo watch -x test"),
            Framework::Jest => Some("npx jest --watch"),
            Framework::Vitest => Some("npx vitest"),
            Framework::Pytest => Some("pytest-watch"),
            Framework::Go => Some("gowatch"),
            _ => None,
        }
    }
}

impl std::fmt::Display for Framework {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Framework::Cargo => write!(f, "cargo"),
            Framework::Jest => write!(f, "jest"),
            Framework::Vitest => write!(f, "vitest"),
            Framework::Npm => write!(f, "npm"),
            Framework::Pytest => write!(f, "pytest"),
            Framework::Go => write!(f, "go"),
            Framework::Bats => write!(f, "bats"),
            Framework::Make => write!(f, "make"),
        }
    }
}

impl std::str::FromStr for Framework {
    type Err = TestError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "cargo" | "rust" => Ok(Framework::Cargo),
            "jest" => Ok(Framework::Jest),
            "vitest" => Ok(Framework::Vitest),
            "npm" | "node" => Ok(Framework::Npm),
            "pytest" | "python" => Ok(Framework::Pytest),
            "go" | "golang" => Ok(Framework::Go),
            "bats" | "bash" => Ok(Framework::Bats),
            "make" | "makefile" => Ok(Framework::Make),
            _ => Err(TestError::UnsupportedFramework(s.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Error type tests
    // ========================================================================

    #[test]
    fn test_error_display() {
        let err = TestError::NoFrameworkDetected("no Cargo.toml, no package.json".to_string());
        assert_eq!(err.to_string(), "No test framework detected in project. no Cargo.toml, no package.json");

        let err = TestError::UnsupportedFramework("custom".to_string());
        assert_eq!(err.to_string(), "Unsupported test framework: custom");

        let err = TestError::ExecutionFailed("command not found".to_string());
        assert_eq!(err.to_string(), "Test execution failed: command not found");

        let err = TestError::Timeout(30);
        assert_eq!(err.to_string(), "Test run timed out after 30 seconds");
    }

    // ========================================================================
    // Framework enum tests
    // ========================================================================

    #[test]
    fn test_framework_default_commands() {
        assert_eq!(Framework::Cargo.default_command(), "cargo test");
        assert_eq!(Framework::Jest.default_command(), "npx jest");
        assert_eq!(Framework::Vitest.default_command(), "npx vitest run");
        assert_eq!(Framework::Npm.default_command(), "npm test");
        assert_eq!(Framework::Pytest.default_command(), "pytest");
        assert_eq!(Framework::Go.default_command(), "go test ./...");
        assert_eq!(Framework::Bats.default_command(), "bats tests/");
        assert_eq!(Framework::Make.default_command(), "make test");
    }

    #[test]
    fn test_framework_coverage_commands() {
        assert_eq!(Framework::Cargo.coverage_command(), Some("cargo llvm-cov"));
        assert_eq!(Framework::Jest.coverage_command(), Some("npx jest --coverage"));
        assert_eq!(Framework::Vitest.coverage_command(), Some("npx vitest run --coverage"));
        assert_eq!(Framework::Pytest.coverage_command(), Some("pytest --cov"));
        assert_eq!(Framework::Go.coverage_command(), Some("go test -cover ./..."));
        assert_eq!(Framework::Bats.coverage_command(), None);
        assert_eq!(Framework::Make.coverage_command(), None);
    }

    #[test]
    fn test_framework_watch_commands() {
        assert_eq!(Framework::Cargo.watch_command(), Some("cargo watch -x test"));
        assert_eq!(Framework::Jest.watch_command(), Some("npx jest --watch"));
        assert_eq!(Framework::Vitest.watch_command(), Some("npx vitest"));
        assert_eq!(Framework::Pytest.watch_command(), Some("pytest-watch"));
        assert_eq!(Framework::Go.watch_command(), Some("gowatch"));
        assert_eq!(Framework::Bats.watch_command(), None);
        assert_eq!(Framework::Make.watch_command(), None);
    }

    #[test]
    fn test_framework_from_str_aliases() {
        // Test all aliases work
        assert_eq!("rust".parse::<Framework>().unwrap(), Framework::Cargo);
        assert_eq!("node".parse::<Framework>().unwrap(), Framework::Npm);
        assert_eq!("python".parse::<Framework>().unwrap(), Framework::Pytest);
        assert_eq!("golang".parse::<Framework>().unwrap(), Framework::Go);
        assert_eq!("bash".parse::<Framework>().unwrap(), Framework::Bats);
        assert_eq!("makefile".parse::<Framework>().unwrap(), Framework::Make);
    }

    #[test]
    fn test_framework_from_str_case_insensitive() {
        assert_eq!("CARGO".parse::<Framework>().unwrap(), Framework::Cargo);
        assert_eq!("JeSt".parse::<Framework>().unwrap(), Framework::Jest);
        assert_eq!("PyTest".parse::<Framework>().unwrap(), Framework::Pytest);
    }
}
