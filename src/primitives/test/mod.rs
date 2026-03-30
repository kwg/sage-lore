// SPDX-License-Identifier: MIT
//! Test primitive types for the SAGE Method engine.
//!
//! This module provides a unified interface for running tests across different
//! frameworks. It auto-detects the project's test framework and executes tests
//! with structured output for scroll consumption.
//!
//! # Design Decisions
//!
//! - Auto-detect the project's test framework so scrolls need zero config (D17)
//! - Produce structured JSON output that scrolls can parse programmatically (D18)
//! - Provide fast smoke tests suitable for pre-commit hooks (D19)
//! - Respect each framework's native parallelism defaults (D49)
//! - Retry flaky tests but always report retries — never silently swallow them (D51)

pub mod types;
pub mod r#trait;
pub mod backends;
pub mod discovery;
pub mod noop_backend;
pub mod auto_detect;
pub mod framework_detect;
pub mod verify;

// Re-export commonly used types
pub use types::*;
pub use r#trait::TestBackend;
pub use discovery::{AutoDetectBackend, NoopBackend};
pub use backends::*;
