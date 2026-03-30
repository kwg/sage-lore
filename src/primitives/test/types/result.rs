// SPDX-License-Identifier: MIT
//! Test result types for execution results and test failures.

use serde::{Deserialize, Serialize};

use super::framework::Framework;

// ============================================================================
// Test Result Types
// ============================================================================

/// Result of a test suite execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestRunResult {
    /// Whether all tests passed
    pub passed: bool,
    /// Framework used for the test run
    pub framework: Framework,
    /// Summary statistics
    pub summary: TestSummary,
    /// Details of failed tests
    pub failures: Vec<TestFailure>,
    /// Tests that required retry — tracked so flakiness is always visible (D51)
    pub retries: Vec<RetryRecord>,
    /// Raw output from the test run
    pub output: String,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

impl TestRunResult {
    /// Create a new passing test result.
    pub fn passed(framework: Framework, summary: TestSummary, duration_ms: u64) -> Self {
        Self {
            passed: true,
            framework,
            summary,
            failures: Vec::new(),
            retries: Vec::new(),
            output: String::new(),
            duration_ms,
        }
    }

    /// Create a new failing test result.
    pub fn failed(
        framework: Framework,
        summary: TestSummary,
        failures: Vec<TestFailure>,
        duration_ms: u64,
    ) -> Self {
        Self {
            passed: false,
            framework,
            summary,
            failures,
            retries: Vec::new(),
            output: String::new(),
            duration_ms,
        }
    }

    /// Check if any tests required retries (indicates potential flakiness).
    pub fn has_retries(&self) -> bool {
        !self.retries.is_empty()
    }

    /// Count the number of tests that required retries.
    pub fn retry_count(&self) -> usize {
        self.retries.len()
    }

    /// Get tests that failed even after retries.
    pub fn failed_after_retry(&self) -> impl Iterator<Item = &RetryRecord> {
        self.retries.iter().filter(|r| !r.final_passed)
    }

    /// Get tests that passed after retry (flaky tests).
    pub fn flaky_tests(&self) -> impl Iterator<Item = &RetryRecord> {
        self.retries.iter().filter(|r| r.final_passed)
    }
}

impl Default for TestRunResult {
    fn default() -> Self {
        Self {
            passed: true,
            framework: Framework::Cargo,
            summary: TestSummary::default(),
            failures: Vec::new(),
            retries: Vec::new(),
            output: String::new(),
            duration_ms: 0,
        }
    }
}

/// Summary statistics for a test run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestSummary {
    /// Total number of tests
    pub total: u32,
    /// Number of tests that passed
    pub passed: u32,
    /// Number of tests that failed
    pub failed: u32,
    /// Number of tests that were skipped
    pub skipped: u32,
    /// Number of tests that are pending/todo
    pub pending: u32,
    /// Number of tests that required retry — always reported, never silently swallowed (D51)
    pub retried: u32,
}

impl TestSummary {
    /// Create a new test summary.
    pub fn new(total: u32, passed: u32, failed: u32, skipped: u32) -> Self {
        Self {
            total,
            passed,
            failed,
            skipped,
            pending: 0,
            retried: 0,
        }
    }

    /// Check if all tests passed (no failures).
    pub fn all_passed(&self) -> bool {
        self.failed == 0
    }

    /// Get the pass rate as a percentage.
    pub fn pass_rate(&self) -> f64 {
        if self.total == 0 {
            100.0
        } else {
            (self.passed as f64 / self.total as f64) * 100.0
        }
    }
}

/// Details of a single test failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestFailure {
    /// Name of the failed test
    pub name: String,
    /// File containing the test (if known)
    pub file: Option<String>,
    /// Line number of the test (if known)
    pub line: Option<u32>,
    /// Failure message
    pub message: String,
    /// Expected value (for assertion failures)
    pub expected: Option<String>,
    /// Actual value (for assertion failures)
    pub actual: Option<String>,
    /// Stack trace (if available)
    pub stack: Option<String>,
}

impl TestFailure {
    /// Create a new test failure with just name and message.
    pub fn new(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            file: None,
            line: None,
            message: message.into(),
            expected: None,
            actual: None,
            stack: None,
        }
    }

    /// Create a test failure with location information.
    pub fn with_location(
        name: impl Into<String>,
        message: impl Into<String>,
        file: impl Into<String>,
        line: u32,
    ) -> Self {
        Self {
            name: name.into(),
            file: Some(file.into()),
            line: Some(line),
            message: message.into(),
            expected: None,
            actual: None,
            stack: None,
        }
    }

    /// Add assertion details to the failure.
    pub fn with_assertion(
        mut self,
        expected: impl Into<String>,
        actual: impl Into<String>,
    ) -> Self {
        self.expected = Some(expected.into());
        self.actual = Some(actual.into());
        self
    }

    /// Add stack trace to the failure.
    pub fn with_stack(mut self, stack: impl Into<String>) -> Self {
        self.stack = Some(stack.into());
        self
    }

    /// Format a display location string (e.g., "src/lib.rs:42").
    pub fn location_string(&self) -> Option<String> {
        match (&self.file, self.line) {
            (Some(f), Some(l)) => Some(format!("{}:{}", f, l)),
            (Some(f), None) => Some(f.clone()),
            _ => None,
        }
    }
}

/// Record of a test that required retry — always emitted so flakiness is visible (D51).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryRecord {
    /// Name of the test that was retried
    pub test_name: String,
    /// Number of attempts made
    pub attempts: u32,
    /// Whether the test finally passed
    pub final_passed: bool,
}

impl RetryRecord {
    /// Create a new retry record for a test that passed after retry.
    pub fn passed_after_retry(test_name: impl Into<String>, attempts: u32) -> Self {
        Self {
            test_name: test_name.into(),
            attempts,
            final_passed: true,
        }
    }

    /// Create a new retry record for a test that failed even after retry.
    pub fn failed_after_retry(test_name: impl Into<String>, attempts: u32) -> Self {
        Self {
            test_name: test_name.into(),
            attempts,
            final_passed: false,
        }
    }

    /// Check if this represents a flaky test (passed after retry).
    pub fn is_flaky(&self) -> bool {
        self.final_passed && self.attempts > 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // TestRunResult tests
    // ========================================================================

    #[test]
    fn test_test_run_result_passed_constructor() {
        let summary = TestSummary::new(10, 10, 0, 0);
        let result = TestRunResult::passed(Framework::Cargo, summary, 1000);

        assert!(result.passed);
        assert_eq!(result.framework, Framework::Cargo);
        assert_eq!(result.duration_ms, 1000);
        assert!(result.failures.is_empty());
        assert!(result.retries.is_empty());
    }

    #[test]
    fn test_test_run_result_failed_constructor() {
        let summary = TestSummary::new(10, 8, 2, 0);
        let failures = vec![
            TestFailure::new("test_a", "failed"),
            TestFailure::new("test_b", "failed"),
        ];
        let result = TestRunResult::failed(Framework::Cargo, summary, failures, 2000);

        assert!(!result.passed);
        assert_eq!(result.framework, Framework::Cargo);
        assert_eq!(result.duration_ms, 2000);
        assert_eq!(result.failures.len(), 2);
    }

    #[test]
    fn test_test_run_result_flaky_iterators() {
        let mut result = TestRunResult::default();
        result.retries = vec![
            RetryRecord::passed_after_retry("test_flaky", 2),
            RetryRecord::failed_after_retry("test_broken", 3),
            RetryRecord::passed_after_retry("test_flaky2", 2),
        ];

        let flaky: Vec<_> = result.flaky_tests().collect();
        assert_eq!(flaky.len(), 2);
        assert!(flaky.iter().all(|r| r.final_passed));

        let failed: Vec<_> = result.failed_after_retry().collect();
        assert_eq!(failed.len(), 1);
        assert!(!failed[0].final_passed);
    }

    // ========================================================================
    // TestSummary tests
    // ========================================================================

    #[test]
    fn test_test_summary_new() {
        let summary = TestSummary::new(100, 90, 5, 5);
        assert_eq!(summary.total, 100);
        assert_eq!(summary.passed, 90);
        assert_eq!(summary.failed, 5);
        assert_eq!(summary.skipped, 5);
        assert_eq!(summary.pending, 0);
        assert_eq!(summary.retried, 0);
    }

    #[test]
    fn test_test_summary_all_passed() {
        let all_pass = TestSummary::new(50, 50, 0, 0);
        assert!(all_pass.all_passed());

        let some_failed = TestSummary::new(50, 49, 1, 0);
        assert!(!some_failed.all_passed());
    }

    #[test]
    fn test_test_summary_pass_rate_edge_cases() {
        // Zero tests
        let empty = TestSummary::new(0, 0, 0, 0);
        assert_eq!(empty.pass_rate(), 100.0);

        // All passed
        let perfect = TestSummary::new(100, 100, 0, 0);
        assert_eq!(perfect.pass_rate(), 100.0);

        // All failed
        let all_fail = TestSummary::new(100, 0, 100, 0);
        assert_eq!(all_fail.pass_rate(), 0.0);

        // Partial
        let partial = TestSummary::new(100, 75, 25, 0);
        assert_eq!(partial.pass_rate(), 75.0);
    }

    // ========================================================================
    // TestFailure tests
    // ========================================================================

    #[test]
    fn test_test_failure_new() {
        let failure = TestFailure::new("test_foo", "assertion failed");
        assert_eq!(failure.name, "test_foo");
        assert_eq!(failure.message, "assertion failed");
        assert!(failure.file.is_none());
        assert!(failure.line.is_none());
        assert!(failure.expected.is_none());
        assert!(failure.actual.is_none());
    }

    #[test]
    fn test_test_failure_with_location() {
        let failure = TestFailure::with_location("test_foo", "failed", "src/lib.rs", 42);
        assert_eq!(failure.name, "test_foo");
        assert_eq!(failure.file, Some("src/lib.rs".to_string()));
        assert_eq!(failure.line, Some(42));
    }

    #[test]
    fn test_test_failure_with_assertion() {
        let failure = TestFailure::new("test_foo", "failed")
            .with_assertion("expected", "actual");
        assert_eq!(failure.expected, Some("expected".to_string()));
        assert_eq!(failure.actual, Some("actual".to_string()));
    }

    #[test]
    fn test_test_failure_with_stack() {
        let failure = TestFailure::new("test_foo", "failed")
            .with_stack("stack trace here");
        assert_eq!(failure.stack, Some("stack trace here".to_string()));
    }

    #[test]
    fn test_test_failure_location_string_variations() {
        // Both file and line
        let full = TestFailure::with_location("test", "msg", "src/lib.rs", 42);
        assert_eq!(full.location_string(), Some("src/lib.rs:42".to_string()));

        // File only
        let file_only = TestFailure {
            name: "test".to_string(),
            file: Some("src/lib.rs".to_string()),
            line: None,
            message: "msg".to_string(),
            expected: None,
            actual: None,
            stack: None,
        };
        assert_eq!(file_only.location_string(), Some("src/lib.rs".to_string()));

        // Neither
        let none = TestFailure::new("test", "msg");
        assert_eq!(none.location_string(), None);
    }

    // ========================================================================
    // RetryRecord tests
    // ========================================================================

    #[test]
    fn test_retry_record_passed_after_retry() {
        let record = RetryRecord::passed_after_retry("test_flaky", 3);
        assert_eq!(record.test_name, "test_flaky");
        assert_eq!(record.attempts, 3);
        assert!(record.final_passed);
        assert!(record.is_flaky());
    }

    #[test]
    fn test_retry_record_failed_after_retry() {
        let record = RetryRecord::failed_after_retry("test_broken", 3);
        assert_eq!(record.test_name, "test_broken");
        assert_eq!(record.attempts, 3);
        assert!(!record.final_passed);
        assert!(!record.is_flaky()); // Failed, not flaky
    }

    #[test]
    fn test_retry_record_is_flaky_requires_multiple_attempts() {
        // Passed on first try - not flaky
        let first_try = RetryRecord {
            test_name: "test".to_string(),
            attempts: 1,
            final_passed: true,
        };
        assert!(!first_try.is_flaky());

        // Passed on second try - flaky
        let second_try = RetryRecord::passed_after_retry("test", 2);
        assert!(second_try.is_flaky());
    }
}
