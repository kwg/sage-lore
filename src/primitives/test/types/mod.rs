// SPDX-License-Identifier: MIT
//! Type definitions for the test primitive.
//!
//! This module has been decomposed into focused sub-modules while maintaining
//! the same public API surface through re-exports.

pub mod framework;
pub mod result;
pub mod coverage;
pub mod config;
pub mod watch;

// Re-export all types to maintain the original API
pub use framework::{Framework, TestError, TestResult};
pub use result::{RetryRecord, TestFailure, TestRunResult, TestSummary};
pub use coverage::{CoverageResult, FileCoverage};
pub use config::{CoverageConfig, FlakyConfig, SmokeConfig, TestConfig};
pub use watch::WatchHandle;
