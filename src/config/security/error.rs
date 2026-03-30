// SPDX-License-Identifier: MIT
//! Security error types for the SAGE Method engine.

use thiserror::Error;

use crate::primitives::secure::Finding;

use super::config::SecurityLevel;

/// Security errors for the SAGE Method engine.
#[derive(Debug, Error)]
pub enum SecurityError {
    /// Policy file not found — engine refuses to run without a security policy (D10)
    #[error("Policy file not found: {0}")]
    PolicyNotFound(String),

    /// Project security level is below the organization-wide minimum (D43)
    #[error("Project security level ({project}) is below global floor ({floor})")]
    BelowFloor {
        project: SecurityLevel,
        floor: SecurityLevel,
    },

    /// Required tool is not available
    #[error("Required tool not available: {0}")]
    RequiredToolMissing(String),

    /// Security scan failed
    #[error("Security scan failed: {0}")]
    ScanFailed(String),

    /// Secrets detected in content — abort immediately, no partial salvage (D11)
    #[error("Secrets detected in content")]
    SecretsDetected(Vec<Finding>),

    /// Policy violation
    #[error("Policy violation: {0}")]
    PolicyViolation(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
