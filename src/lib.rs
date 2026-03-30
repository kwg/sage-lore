// SPDX-License-Identifier: MIT
//! SAGE Method - Security-Aware Generation Engine
//!
//! The SAGE Method engine enforces security policy during code generation.
//! It does NOT define security policy - each project declares its own
//! requirements in `.sage-lore/security/policy.yaml`.
//!
//! # Philosophy
//!
//! **"Hardcoded is a four letter word."**
//!
//! This accommodates the full spectrum of security needs:
//! - Payment processor: paranoid mode, all tools required, hard stop on any failure
//! - Desktop game: relaxed mode, built-in regex fine, warn and continue
//!
//! # Key Concepts
//!
//! - **Policy-driven**: Security behavior is defined per-project, not hardcoded
//! - **Fail closed**: No policy file means the engine refuses to run (D10)
//! - **Global floor**: Organizations can set minimum security levels that projects cannot lower (D43)
//! - **Abort and reset**: On mid-scroll secrets the entire operation is aborted — no partial surgery, no salvage (D11)

pub mod auth;
pub mod cli;
pub mod config;
pub mod primitives;
pub mod scroll;

// Re-export commonly used types
pub use config::{
    FallbackPolicy, OnExisting, OnFinding, Policy, SecurityError, SecurityFloor, SecurityLevel,
};

pub use primitives::{
    compute_content_hash, AuditReport, CveEntry, CveReport, Finding, FindingType, Location,
    PolicyDrivenBackend, SastReport, ScanResult, ScanType, SecureBackend, Severity, ToolStatus,
};
