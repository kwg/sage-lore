// SPDX-License-Identifier: MIT
//! Configuration types for the SAGE Method engine.

pub mod hierarchy;
pub mod paths;
pub mod resolver;
pub mod secrets;
pub mod security;

pub use hierarchy::{
    ClaudeConfig, Config, ConfigError, ConfigLoader, CoverageConfig, FlakyConfig, LlmConfig,
    OllamaConfig, PlatformConfig, ProjectConfig, StateConfig, TestConfig,
};
pub use paths::ScrollPaths;
pub use resolver::PathResolver;
pub use secrets::SecretResolver;
pub use security::{
    Allowlist, AllowlistInstance, AllowlistPattern, DependencyScanConfig, FallbackPolicy,
    OnExisting, OnFinding, Policy, SecretDetectionConfig, SecurityError, SecurityFloor,
    SecurityLevel, SeverityThreshold, StaticAnalysisConfig,
};
