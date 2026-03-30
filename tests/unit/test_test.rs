//! Unit tests for the test primitive.
//!
//! Tests cover:
//! - TestBackend trait implementation
//! - Test interface dispatch
//! - Pattern filtering
//! - Coverage operations

use sage_lore::scroll::error::ExecutionError;
use sage_lore::scroll::interfaces::InterfaceDispatch;
use sage_lore::scroll::interfaces::test::TestInterface;
use sage_lore::primitives::test::{TestBackend, TestRunResult, TestSummary, CoverageResult, Framework, TestError};
use std::sync::Arc;

// ============================================================================
// Mock TestBackend for Testing
// ============================================================================

/// Mock test backend that returns predefined results.
struct MockTestBackend {
    should_fail: bool,
    test_count: usize,
}

impl MockTestBackend {
    fn new() -> Self {
        Self {
            should_fail: false,
            test_count: 5,
        }
    }

    fn failing() -> Self {
        Self {
            should_fail: true,
            test_count: 5,
        }
    }
}

impl TestBackend for MockTestBackend {
    fn run_suite(&self, _filter: Option<&str>) -> Result<TestRunResult, TestError> {
        if self.should_fail {
            return Err(TestError::ExecutionFailed("Mock test execution failed".to_string()));
        }

        Ok(TestRunResult {
            passed: true,
            framework: Framework::Cargo,
            summary: TestSummary::new(
                self.test_count as u32,
                self.test_count as u32,
                0,
                0,
            ),
            failures: vec![],
            retries: vec![],
            output: String::new(),
            duration_ms: 1000,
        })
    }

    fn smoke(&self) -> Result<TestRunResult, TestError> {
        Ok(TestRunResult {
            passed: true,
            framework: Framework::Cargo,
            summary: TestSummary::new(2, 2, 0, 0),
            failures: vec![],
            retries: vec![],
            output: String::new(),
            duration_ms: 100,
        })
    }

    fn coverage(&self) -> Result<CoverageResult, TestError> {
        if self.should_fail {
            return Err(TestError::CoverageNotSupported("Mock framework".to_string()));
        }

        Ok(CoverageResult {
            lines_percent: 85.5,
            branches_percent: Some(75.0),
            functions_percent: Some(90.0),
            lines_total: 1000,
            lines_covered: 855,
            files: vec![],
            framework: Framework::Cargo,
        })
    }

    fn run_filtered(&self, pattern: &str) -> Result<TestRunResult, TestError> {
        if self.should_fail {
            return Err(TestError::ExecutionFailed("Mock test execution failed".to_string()));
        }

        // Simulate filtering reducing test count
        let filtered_count = if pattern.contains("specific") { 2 } else { self.test_count };

        Ok(TestRunResult {
            passed: true,
            framework: Framework::Cargo,
            summary: TestSummary::new(
                filtered_count as u32,
                filtered_count as u32,
                0,
                0,
            ),
            failures: vec![],
            retries: vec![],
            output: String::new(),
            duration_ms: 500,
        })
    }

    fn run_files(&self, _files: &[&str]) -> Result<TestRunResult, TestError> {
        Ok(TestRunResult {
            passed: true,
            framework: Framework::Cargo,
            summary: TestSummary::new(3, 3, 0, 0),
            failures: vec![],
            retries: vec![],
            output: String::new(),
            duration_ms: 300,
        })
    }

    fn supports_coverage(&self) -> bool {
        !self.should_fail
    }

    fn supports_watch(&self) -> bool {
        true
    }

    fn framework(&self) -> Framework {
        Framework::Cargo
    }
}

// ============================================================================
// Test Interface Dispatch Tests
// ============================================================================

#[tokio::test]
async fn test_interface_run_basic() {
    let backend = Arc::new(MockTestBackend::new());
    let interface = TestInterface::with_backend(backend);

    let result = interface.dispatch("run", &None).await;
    assert!(result.is_ok(), "Test run should succeed");

    let output = result.unwrap();
    assert!(output.get("passed").is_some(), "Should have passed status");
    assert!(output.get("summary").is_some(), "Should have summary");
    let summary = output.get("summary").unwrap();
    assert_eq!(summary.get("total").and_then(|v| v.as_u64()), Some(5));
}

#[tokio::test]
async fn test_interface_run_with_filter() {
    let backend = Arc::new(MockTestBackend::new());
    let interface = TestInterface::with_backend(backend);

    let mut params = serde_json::Map::new();
    params.insert("filter".to_string(),
        serde_json::Value::String("specific".to_string()),
    );

    let result = interface.dispatch("run", &Some(serde_json::Value::Object(params))).await;
    assert!(result.is_ok(), "Test run with filter should succeed");
}

#[tokio::test]
async fn test_interface_run_filtered_with_pattern() {
    let backend = Arc::new(MockTestBackend::new());
    let interface = TestInterface::with_backend(backend);

    let mut params = serde_json::Map::new();
    params.insert("pattern".to_string(),
        serde_json::Value::String("specific".to_string()),
    );

    let result = interface.dispatch("run_filtered", &Some(serde_json::Value::Object(params))).await;
    assert!(result.is_ok(), "run_filtered should succeed");

    let output = result.unwrap();
    let summary = output.get("summary").unwrap();
    assert_eq!(summary.get("total").and_then(|v| v.as_u64()), Some(2), "Pattern should filter tests");
}

#[tokio::test]
async fn test_interface_coverage() {
    let backend = Arc::new(MockTestBackend::new());
    let interface = TestInterface::with_backend(backend);

    let result = interface.dispatch("coverage", &None).await;
    assert!(result.is_ok(), "Coverage should succeed");

    let output = result.unwrap();
    assert!(output.get("lines_percent").is_some(), "Should have lines_percent");
    assert_eq!(output.get("lines_percent").and_then(|v| v.as_f64()), Some(85.5));
}

#[tokio::test]
async fn test_interface_smoke() {
    let backend = Arc::new(MockTestBackend::new());
    let interface = TestInterface::with_backend(backend);

    let result = interface.dispatch("smoke", &None).await;
    assert!(result.is_ok(), "Smoke tests should succeed");

    let output = result.unwrap();
    let summary = output.get("summary").unwrap();
    assert_eq!(summary.get("total").and_then(|v| v.as_u64()), Some(2), "Smoke tests should be subset");
}

#[tokio::test]
async fn test_interface_run_failure() {
    let backend = Arc::new(MockTestBackend::failing());
    let interface = TestInterface::with_backend(backend);

    let result = interface.dispatch("run", &None).await;
    assert!(result.is_err(), "Failed backend should return error");
    assert!(matches!(result.unwrap_err(), ExecutionError::InvocationError(_)));
}

#[tokio::test]
async fn test_interface_coverage_not_supported() {
    let backend = Arc::new(MockTestBackend::failing());
    let interface = TestInterface::with_backend(backend);

    let result = interface.dispatch("coverage", &None).await;
    assert!(result.is_err(), "Coverage should fail when not supported");
}

#[tokio::test]
async fn test_interface_unknown_method() {
    let backend = Arc::new(MockTestBackend::new());
    let interface = TestInterface::with_backend(backend);

    let result = interface.dispatch("unknown_method", &None).await;
    assert!(result.is_err(), "Unknown method should fail");
    assert!(matches!(result.unwrap_err(), ExecutionError::InterfaceError(_)));
}

#[tokio::test]
async fn test_interface_run_filtered_missing_pattern() {
    let backend = Arc::new(MockTestBackend::new());
    let interface = TestInterface::with_backend(backend);

    // Call run_filtered without providing pattern parameter
    let result = interface.dispatch("run_filtered", &None).await;
    assert!(result.is_err(), "run_filtered should require pattern parameter");
    assert!(matches!(result.unwrap_err(), ExecutionError::MissingParameter(_)));
}

#[tokio::test]
async fn test_interface_no_backend() {
    let interface = TestInterface::new();

    let result = interface.dispatch("run", &None).await;
    assert!(result.is_err(), "No backend should return error");
    assert!(matches!(result.unwrap_err(), ExecutionError::NotImplemented(_)));
}

// ============================================================================
// TestBackend Direct Tests
// ============================================================================

#[tokio::test]
async fn test_backend_run_suite() {
    let backend = MockTestBackend::new();
    let result = backend.run_suite(None);

    assert!(result.is_ok());
    let test_result = result.unwrap();
    assert_eq!(test_result.summary.total, 5);
    assert_eq!(test_result.summary.passed, 5);
    assert_eq!(test_result.summary.failed, 0);
    assert!(test_result.passed);
}

#[tokio::test]
async fn test_backend_run_filtered() {
    let backend = MockTestBackend::new();
    let result = backend.run_filtered("specific");

    assert!(result.is_ok());
    let test_result = result.unwrap();
    assert_eq!(test_result.summary.total, 2);
}

#[tokio::test]
async fn test_backend_coverage() {
    let backend = MockTestBackend::new();
    let result = backend.coverage();

    assert!(result.is_ok());
    let cov_result = result.unwrap();
    assert_eq!(cov_result.lines_percent, 85.5);
    assert_eq!(cov_result.lines_total, 1000);
    assert_eq!(cov_result.lines_covered, 855);
}

#[tokio::test]
async fn test_backend_supports_coverage() {
    let backend = MockTestBackend::new();
    assert!(backend.supports_coverage());

    let failing = MockTestBackend::failing();
    assert!(!failing.supports_coverage());
}

#[tokio::test]
async fn test_backend_framework() {
    let backend = MockTestBackend::new();
    assert_eq!(backend.framework(), Framework::Cargo);
}
