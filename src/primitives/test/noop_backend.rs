// SPDX-License-Identifier: MIT
//! Noop test backend for cases where no framework is detected.

use crate::primitives::test::r#trait::TestBackend;
use crate::primitives::test::types::{
    CoverageResult, Framework, TestError, TestResult, TestRunResult,
};

/// A no-op backend that returns errors for all operations.
/// Used as a fallback when no test framework is detected.
pub struct NoopBackend {
    /// Diagnostic message describing what was checked
    pub diagnostic: String,
}

impl NoopBackend {
    pub fn new(diagnostic: String) -> Self {
        Self { diagnostic }
    }
}

impl TestBackend for NoopBackend {
    fn run_suite(&self, _filter: Option<&str>) -> TestResult<TestRunResult> {
        Err(TestError::NoFrameworkDetected(self.diagnostic.clone()))
    }

    fn smoke(&self) -> TestResult<TestRunResult> {
        Err(TestError::NoFrameworkDetected(self.diagnostic.clone()))
    }

    fn coverage(&self) -> TestResult<CoverageResult> {
        Err(TestError::NoFrameworkDetected(self.diagnostic.clone()))
    }

    fn run_filtered(&self, _pattern: &str) -> TestResult<TestRunResult> {
        Err(TestError::NoFrameworkDetected(self.diagnostic.clone()))
    }

    fn run_files(&self, _files: &[&str]) -> TestResult<TestRunResult> {
        Err(TestError::NoFrameworkDetected(self.diagnostic.clone()))
    }

    fn supports_coverage(&self) -> bool {
        false
    }

    fn supports_watch(&self) -> bool {
        false
    }

    fn framework(&self) -> Framework {
        Framework::Make // Fallback
    }
}
