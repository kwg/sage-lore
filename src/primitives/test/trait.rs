// SPDX-License-Identifier: MIT
//! TestBackend trait definition.

use crate::primitives::test::types::{CoverageResult, Framework, TestResult, TestRunResult};

pub trait TestBackend: Send + Sync {
    /// Run the full test suite.
    ///
    /// # Arguments
    ///
    /// * `filter` - Optional pattern to filter which tests to run
    ///
    /// # Returns
    ///
    /// A `TestRunResult` containing the test results and summary.
    fn run_suite(&self, filter: Option<&str>) -> TestResult<TestRunResult>;

    /// Run smoke tests (quick subset for pre-commit).
    ///
    /// Smoke tests are a fast subset of the full test suite designed to
    /// catch obvious regressions without running the entire suite.
    fn smoke(&self) -> TestResult<TestRunResult>;

    /// Run tests with coverage analysis.
    ///
    /// # Errors
    ///
    /// Returns `TestError::CoverageNotSupported` if the framework doesn't
    /// support coverage reporting.
    fn coverage(&self) -> TestResult<CoverageResult>;

    /// Run tests matching a pattern.
    ///
    /// # Arguments
    ///
    /// * `pattern` - Regex or glob pattern to match test names
    fn run_filtered(&self, pattern: &str) -> TestResult<TestRunResult>;

    /// Run tests in specific files.
    ///
    /// # Arguments
    ///
    /// * `files` - List of file paths containing tests to run
    fn run_files(&self, files: &[&str]) -> TestResult<TestRunResult>;

    /// Check if this backend supports coverage reporting.
    fn supports_coverage(&self) -> bool;

    /// Check if this backend supports watch mode.
    fn supports_watch(&self) -> bool;

    /// Get the framework this backend handles.
    fn framework(&self) -> Framework;
}
