// SPDX-License-Identifier: MIT
//! Configuration types for test execution.

use serde::{Deserialize, Serialize};

use super::framework::Framework;

// ============================================================================
// Configuration Types
// ============================================================================

/// Configuration for test execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    /// Override the detected framework
    pub framework: Option<Framework>,
    /// Custom test command (overrides framework detection)
    pub command: Option<String>,
    /// Timeout for test runs in seconds
    pub timeout_seconds: u64,
    /// Maximum output characters to capture
    pub max_output_chars: usize,
    /// Smoke test configuration
    pub smoke: SmokeConfig,
    /// Coverage thresholds
    pub coverage: CoverageConfig,
    /// Flaky test handling — retry failed tests but always report retries (D51)
    pub flaky: FlakyConfig,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            framework: None,
            command: None,
            timeout_seconds: 300,
            max_output_chars: 50000,
            smoke: SmokeConfig::default(),
            coverage: CoverageConfig::default(),
            flaky: FlakyConfig::default(),
        }
    }
}

/// Configuration for smoke tests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmokeConfig {
    /// Smoke test args for cargo
    pub cargo: String,
    /// Smoke test args for pytest
    pub pytest: String,
    /// Smoke test args for jest
    pub jest: String,
    /// Smoke test args for go
    pub go: String,
}

impl Default for SmokeConfig {
    fn default() -> Self {
        Self {
            cargo: "--lib".to_string(),
            pytest: "-m smoke".to_string(),
            jest: "--testPathPattern=unit".to_string(),
            go: "-short".to_string(),
        }
    }
}

impl SmokeConfig {
    /// Get smoke test args for a given framework.
    pub fn args_for(&self, framework: Framework) -> Option<&str> {
        match framework {
            Framework::Cargo => Some(&self.cargo),
            Framework::Pytest => Some(&self.pytest),
            Framework::Jest => Some(&self.jest),
            Framework::Go => Some(&self.go),
            _ => None,
        }
    }
}

/// Configuration for coverage thresholds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageConfig {
    /// Minimum line coverage percentage
    pub min_lines_percent: f64,
    /// Minimum branch coverage percentage
    pub min_branches_percent: Option<f64>,
}

impl Default for CoverageConfig {
    fn default() -> Self {
        Self {
            min_lines_percent: 80.0,
            min_branches_percent: Some(70.0),
        }
    }
}

/// Configuration for flaky test handling — retry count and mandatory reporting (D51).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlakyConfig {
    /// Number of times to retry a failed test
    pub retry_count: u32,
    /// Always report when a retry was needed (never silently swallow)
    pub report_retries: bool,
}

impl Default for FlakyConfig {
    fn default() -> Self {
        Self {
            retry_count: 2,
            report_retries: true, // retries are always reported so flakiness is visible (D51)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // TestConfig tests
    // ========================================================================

    #[test]
    fn test_test_config_defaults() {
        let config = TestConfig::default();
        assert!(config.framework.is_none());
        assert!(config.command.is_none());
        assert_eq!(config.timeout_seconds, 300);
        assert_eq!(config.max_output_chars, 50000);
    }

    #[test]
    fn test_smoke_config_defaults() {
        let config = SmokeConfig::default();
        assert_eq!(config.cargo, "--lib");
        assert_eq!(config.pytest, "-m smoke");
        assert_eq!(config.jest, "--testPathPattern=unit");
        assert_eq!(config.go, "-short");
    }

    #[test]
    fn test_smoke_config_args_for_supported() {
        let config = SmokeConfig::default();
        assert_eq!(config.args_for(Framework::Cargo), Some("--lib"));
        assert_eq!(config.args_for(Framework::Pytest), Some("-m smoke"));
        assert_eq!(config.args_for(Framework::Jest), Some("--testPathPattern=unit"));
        assert_eq!(config.args_for(Framework::Go), Some("-short"));
    }

    #[test]
    fn test_smoke_config_args_for_unsupported() {
        let config = SmokeConfig::default();
        assert_eq!(config.args_for(Framework::Bats), None);
        assert_eq!(config.args_for(Framework::Make), None);
        assert_eq!(config.args_for(Framework::Vitest), None);
        assert_eq!(config.args_for(Framework::Npm), None);
    }

    #[test]
    fn test_coverage_config_defaults() {
        let config = CoverageConfig::default();
        assert_eq!(config.min_lines_percent, 80.0);
        assert_eq!(config.min_branches_percent, Some(70.0));
    }

    #[test]
    fn test_flaky_config_defaults_d51_mandatory_reporting() {
        // Verify retries are always reported so flakiness is visible (D51)
        let config = FlakyConfig::default();
        assert_eq!(config.retry_count, 2);
        assert!(config.report_retries);
    }
}
