// SPDX-License-Identifier: MIT
//! Security configuration types for the SAGE Method engine.
//!
//! This module defines the policy-driven security model where projects declare
//! their requirements in `.sage-lore/security/policy.yaml`. The engine enforces
//! security policy but does not define it.

pub mod config;
pub mod error;
pub mod policy;
pub mod severity;

// Re-export public API to maintain backward compatibility
pub use config::{
    Allowlist, AllowlistInstance, AllowlistPattern, DependencyScanConfig, SecretDetectionConfig,
    SecurityFloor, SecurityLevel, StaticAnalysisConfig,
};
pub use error::SecurityError;
pub use policy::Policy;
pub use severity::{FallbackPolicy, OnExisting, OnFinding, SeverityThreshold};
