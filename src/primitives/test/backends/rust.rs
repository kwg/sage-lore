// SPDX-License-Identifier: MIT
//! Cargo backend for Rust test execution.

use crate::primitives::test::r#trait::TestBackend;
use crate::primitives::test::types::{
    CoverageResult, FileCoverage, Framework, RetryRecord, TestConfig, TestError, TestFailure,
    TestResult, TestRunResult, TestSummary,
};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Instant;

// ============================================================================
// Cargo Test JSON Event Types (for parsing cargo test --format json output)
// ============================================================================

/// Events emitted by `cargo test -- --format json`.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
enum CargoTestEvent {
    /// Suite-level events (started, ok, failed)
    #[serde(rename = "suite")]
    Suite(CargoSuiteEvent),
    /// Individual test events
    #[serde(rename = "test")]
    Test(CargoTestEventData),
    /// Benchmark events (ignored for now)
    #[serde(rename = "bench")]
    #[allow(dead_code)]
    Bench(serde_json::Value),
}

/// Suite-level event data.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // Fields are parsed but only exec_time and event are used currently
struct CargoSuiteEvent {
    /// Event type: "started", "ok", or "failed"
    event: String,
    /// Number of tests (present in "started" event)
    test_count: Option<u32>,
    /// Number of passed tests (present in "ok"/"failed" events)
    passed: Option<u32>,
    /// Number of failed tests
    failed: Option<u32>,
    /// Number of ignored tests
    ignored: Option<u32>,
    /// Number of measured tests (benchmarks)
    #[serde(default)]
    measured: u32,
    /// Number of filtered out tests
    #[serde(default)]
    filtered_out: u32,
    /// Execution time in seconds
    exec_time: Option<f64>,
}

/// Individual test event data.
#[derive(Debug, Clone, Deserialize)]
struct CargoTestEventData {
    /// Event type: "started", "ok", "failed", or "ignored"
    event: String,
    /// Full test name (e.g., "tests::test_add")
    name: String,
    /// Standard output captured (present when test fails)
    stdout: Option<String>,
    /// Standard error captured (present when test fails)
    #[serde(default)]
    stderr: Option<String>,
}

// ============================================================================
// Cargo Backend Implementation
// ============================================================================

/// Backend for Rust projects using cargo test.
pub struct CargoBackend {
    /// Project root directory
    project_root: PathBuf,
    /// Test configuration
    config: TestConfig,
}

impl CargoBackend {
    /// Create a new CargoBackend.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
            config: TestConfig::default(),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(project_root: impl Into<PathBuf>, config: TestConfig) -> Self {
        Self {
            project_root: project_root.into(),
            config,
        }
    }

    /// Run cargo test with the given arguments and return parsed results.
    ///
    /// Tries JSON format first (nightly only, via `-Z unstable-options`).
    /// If that fails (stable Rust rejects the flag), retries with plain
    /// human-readable output so we always get real test results.
    /// Maximum time to wait for cargo test before killing it.
    /// Prevents infinite loops in generated tests from hanging the engine.
    const TEST_TIMEOUT_SECS: u64 = 120;

    fn run_cargo_test(&self, args: &[&str]) -> TestResult<TestRunResult> {
        let start = Instant::now();

        // First try: JSON format (nightly only)
        let mut cmd = Command::new("cargo");
        cmd.arg("test")
            .args(args)
            .arg("--")
            .arg("--format")
            .arg("json")
            .arg("-Z")
            .arg("unstable-options")
            .current_dir(&self.project_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = Self::run_with_timeout(&mut cmd, Self::TEST_TIMEOUT_SECS)?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // If JSON format worked, use it
        if let Some(result) = self.try_parse_json_output(&stdout, start.elapsed().as_millis() as u64) {
            return Ok(result);
        }

        // Check if the -Z flag was rejected (stable Rust)
        // In that case, retry without JSON format flags to get real test results
        if stderr.contains("the option `Z` is only accepted on the nightly compiler")
            || stderr.contains("Unrecognized option: 'Z'")
        {
            tracing::debug!("JSON test format not available (stable Rust), retrying with plain output");
            return self.run_cargo_test_plain(args);
        }

        // Fall back to parsing human-readable output from the first run
        let duration_ms = start.elapsed().as_millis() as u64;
        let combined_output = format!("{}\n{}", stdout, stderr);
        self.parse_human_output(&combined_output, duration_ms, output.status.success())
    }

    /// Run a command with a timeout. Kills the process if it exceeds the limit.
    fn run_with_timeout(cmd: &mut Command, timeout_secs: u64) -> TestResult<std::process::Output> {
        let mut child = cmd.spawn()
            .map_err(|e| TestError::ExecutionFailed(e.to_string()))?;

        let timeout = std::time::Duration::from_secs(timeout_secs);
        let start = Instant::now();

        loop {
            match child.try_wait() {
                Ok(Some(_status)) => {
                    // Process finished — collect output
                    let output = child.wait_with_output()
                        .map_err(|e| TestError::ExecutionFailed(e.to_string()))?;
                    return Ok(output);
                }
                Ok(None) => {
                    // Still running — check timeout
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        let _ = child.wait(); // reap
                        return Err(TestError::ExecutionFailed(
                            format!("Test timed out after {}s (killed). Likely infinite loop in generated tests.", timeout_secs)
                        ));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                Err(e) => {
                    return Err(TestError::ExecutionFailed(
                        format!("Failed to check test process status: {}", e)
                    ));
                }
            }
        }
    }

    /// Run cargo test without JSON format flags (works on stable Rust).
    fn run_cargo_test_plain(&self, args: &[&str]) -> TestResult<TestRunResult> {
        let start = Instant::now();

        let mut cmd = Command::new("cargo");
        cmd.arg("test")
            .args(args)
            .current_dir(&self.project_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = Self::run_with_timeout(&mut cmd, Self::TEST_TIMEOUT_SECS)?;

        let duration_ms = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let combined_output = format!("{}\n{}", stdout, stderr);

        self.parse_human_output(&combined_output, duration_ms, output.status.success())
    }

    /// Try to parse JSON-formatted cargo test output.
    pub(crate) fn try_parse_json_output(&self, output: &str, duration_ms: u64) -> Option<TestRunResult> {
        let mut passed = 0u32;
        let mut failed = 0u32;
        let mut ignored = 0u32;
        let mut failures: Vec<TestFailure> = Vec::new();
        let mut suite_duration: Option<f64> = None;

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Try to parse as cargo test JSON event
            if let Ok(event) = serde_json::from_str::<CargoTestEvent>(line) {
                match event {
                    CargoTestEvent::Test(test) => {
                        match test.event.as_str() {
                            "ok" => passed += 1,
                            "failed" => {
                                failed += 1;
                                let failure = self.parse_test_failure(&test);
                                failures.push(failure);
                            }
                            "ignored" => ignored += 1,
                            _ => {}
                        }
                    }
                    CargoTestEvent::Suite(suite) => {
                        if suite.event == "ok" || suite.event == "failed" {
                            suite_duration = suite.exec_time;
                        }
                    }
                    CargoTestEvent::Bench(_) => {
                        // Ignore benchmarks
                    }
                }
            }
        }

        // If we didn't parse any test events, return None to trigger fallback
        if passed == 0 && failed == 0 && ignored == 0 {
            return None;
        }

        let total = passed + failed + ignored;
        let final_duration = suite_duration
            .map(|d| (d * 1000.0) as u64)
            .unwrap_or(duration_ms);

        Some(TestRunResult {
            passed: failed == 0,
            framework: Framework::Cargo,
            summary: TestSummary {
                total,
                passed,
                failed,
                skipped: ignored,
                pending: 0,
                retried: 0,
            },
            failures,
            retries: Vec::new(),
            output: output.to_string(),
            duration_ms: final_duration,
        })
    }

    /// Parse a test failure from cargo test JSON event.
    fn parse_test_failure(&self, test: &CargoTestEventData) -> TestFailure {
        let stdout = test.stdout.as_deref().unwrap_or("");
        let stderr = test.stderr.as_deref().unwrap_or("");
        let combined = format!("{}\n{}", stdout, stderr);

        // Try to extract file and line from panic message
        // Format: "thread 'tests::test_foo' panicked at src/lib.rs:15:9"
        let (file, line) = self.extract_panic_location(&combined);

        // Try to extract expected/actual values from assertion
        let (expected, actual) = self.extract_assertion_values(&combined);

        TestFailure {
            name: test.name.clone(),
            file,
            line,
            message: stdout.to_string(),
            expected,
            actual,
            stack: if combined.contains("stack backtrace") {
                Some(combined.clone())
            } else {
                None
            },
        }
    }

    /// Extract file and line number from panic message.
    pub(crate) fn extract_panic_location(&self, output: &str) -> (Option<String>, Option<u32>) {
        // Look for patterns like:
        // "panicked at src/lib.rs:15:9"
        // "panicked at 'message', src/lib.rs:15:9"
        let patterns = [
            r"panicked at (?:'[^']*', )?([^:]+):(\d+):\d+",
            r"panicked at ([^:]+):(\d+):\d+",
            r"thread .+ panicked at (?:'[^']*', )?([^:]+):(\d+)",
        ];

        for pattern in patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                if let Some(caps) = re.captures(output) {
                    let file = caps.get(1).map(|m| m.as_str().to_string());
                    let line = caps.get(2).and_then(|m| m.as_str().parse().ok());
                    if file.is_some() || line.is_some() {
                        return (file, line);
                    }
                }
            }
        }

        (None, None)
    }

    /// Extract expected/actual values from assertion failure.
    pub(crate) fn extract_assertion_values(&self, output: &str) -> (Option<String>, Option<String>) {
        // Look for patterns like:
        // "assertion `left == right` failed"
        // "  left: `5`"
        // " right: `3`"
        let mut expected = None;
        let mut actual = None;

        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("left:") || trimmed.starts_with("left =") {
                actual = Some(
                    trimmed
                        .trim_start_matches("left:")
                        .trim_start_matches("left =")
                        .trim()
                        .trim_matches('`')
                        .to_string(),
                );
            } else if trimmed.starts_with("right:") || trimmed.starts_with("right =") {
                expected = Some(
                    trimmed
                        .trim_start_matches("right:")
                        .trim_start_matches("right =")
                        .trim()
                        .trim_matches('`')
                        .to_string(),
                );
            }
        }

        (expected, actual)
    }

    /// Parse human-readable cargo test output (fallback when JSON isn't available).
    pub(crate) fn parse_human_output(
        &self,
        output: &str,
        duration_ms: u64,
        success: bool,
    ) -> TestResult<TestRunResult> {
        // Look for summary line like:
        // "test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
        // "test result: FAILED. 3 passed; 2 failed; 1 ignored; 0 measured; 0 filtered out"
        let mut passed = 0u32;
        let mut failed = 0u32;
        let mut ignored = 0u32;
        let mut failures: Vec<TestFailure> = Vec::new();

        // Track failed test names for extracting details
        let mut failed_tests: Vec<String> = Vec::new();

        for line in output.lines() {
            let trimmed = line.trim();

            // Look for "test foo ... FAILED" lines
            if trimmed.starts_with("test ") && trimmed.ends_with("FAILED") {
                // Extract test name: "test foo::bar ... FAILED" -> "foo::bar"
                if let Some(name) = trimmed
                    .strip_prefix("test ")
                    .and_then(|s| s.strip_suffix(" ... FAILED"))
                {
                    failed_tests.push(name.trim().to_string());
                }
            }

            // Look for summary line
            if trimmed.starts_with("test result:") {
                // Parse: "test result: ok. 5 passed; 0 failed; 0 ignored; ..."
                // or: "test result: FAILED. 1 passed; 1 failed; 0 ignored; ..."
                // Use regex to extract all the numbers more reliably
                if let Ok(re) = regex::Regex::new(r"(\d+)\s+passed") {
                    if let Some(caps) = re.captures(trimmed) {
                        passed = caps.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
                    }
                }
                if let Ok(re) = regex::Regex::new(r"(\d+)\s+failed") {
                    if let Some(caps) = re.captures(trimmed) {
                        failed = caps.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
                    }
                }
                if let Ok(re) = regex::Regex::new(r"(\d+)\s+ignored") {
                    if let Some(caps) = re.captures(trimmed) {
                        ignored = caps.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
                    }
                }
            }
        }

        // Create failure records for failed tests
        for name in failed_tests {
            failures.push(TestFailure::new(name, "Test failed (see output for details)"));
        }

        let total = passed + failed + ignored;

        Ok(TestRunResult {
            passed: success && failed == 0,
            framework: Framework::Cargo,
            summary: TestSummary {
                total,
                passed,
                failed,
                skipped: ignored,
                pending: 0,
                retried: 0,
            },
            failures,
            retries: Vec::new(),
            output: output.to_string(),
            duration_ms,
        })
    }

    /// Run tests with flaky retry logic — retries failed tests but always reports retries (D51).
    fn run_with_retry(&self, args: &[&str]) -> TestResult<TestRunResult> {
        let retry_count = self.config.flaky.retry_count;

        // First run
        let mut result = self.run_cargo_test(args)?;

        if result.passed || retry_count == 0 {
            return Ok(result);
        }

        // Track which tests failed and may need retry
        let mut failed_test_names: Vec<String> = result
            .failures
            .iter()
            .map(|f| f.name.clone())
            .collect();

        let mut retry_records: Vec<RetryRecord> = Vec::new();

        // Retry failed tests
        for attempt in 2..=(retry_count + 1) {
            if failed_test_names.is_empty() {
                break;
            }

            // Note: We could use --exact with specific test names, but cargo test
            // filtering is tricky with module paths. For simplicity, we re-run all
            // tests and track which previously-failed tests now pass.

            // For simplicity, re-run all tests and check if previously failed tests pass
            let retry_result = self.run_cargo_test(args)?;

            let mut still_failing: Vec<String> = Vec::new();

            for test_name in &failed_test_names {
                // Check if this test is still failing
                let still_failed = retry_result.failures.iter().any(|f| f.name == *test_name);

                if still_failed {
                    still_failing.push(test_name.clone());
                } else {
                    // Test passed on retry - record as flaky
                    retry_records.push(RetryRecord::passed_after_retry(test_name, attempt));
                }
            }

            // Update result with latest run
            result = retry_result;
            failed_test_names = still_failing;
        }

        // Record tests that failed even after all retries
        for test_name in failed_test_names {
            retry_records.push(RetryRecord::failed_after_retry(&test_name, retry_count + 1));
        }

        // Update result with retry information
        result.retries = retry_records;
        result.summary.retried = result.retries.len() as u32;

        // A test suite passes if all tests eventually pass (even with retries).
        // Retries are always reported so flakiness is visible, never silently swallowed (D51).
        result.passed = result.failures.is_empty()
            || result
                .failures
                .iter()
                .all(|f| result.retries.iter().any(|r| r.test_name == f.name && r.final_passed));

        Ok(result)
    }

    /// Parse coverage output from cargo llvm-cov.
    fn parse_coverage_output(&self, output: &str) -> TestResult<CoverageResult> {
        // cargo llvm-cov outputs in various formats; we'll look for the summary line
        // Example: "Total: 85.42% (234/274 lines)"
        let mut lines_percent = 0.0;
        let mut lines_covered = 0u32;
        let mut lines_total = 0u32;
        let mut files: Vec<FileCoverage> = Vec::new();

        for line in output.lines() {
            let trimmed = line.trim();

            // Look for total coverage line
            if trimmed.starts_with("Total:") || trimmed.contains("TOTAL") {
                // Parse percentage
                if let Some(pct) = self.extract_percentage(trimmed) {
                    lines_percent = pct;
                }
                // Parse lines covered/total
                if let Some((covered, total)) = self.extract_coverage_counts(trimmed) {
                    lines_covered = covered;
                    lines_total = total;
                }
            }

            // Look for per-file coverage (simplified parsing)
            // Format varies by tool, but typically: "src/lib.rs: 90.5% (100/110)"
            if (trimmed.contains(".rs:") || trimmed.contains(".rs "))
                && trimmed.contains('%')
                && !trimmed.starts_with("Total")
            {
                if let Some(file_cov) = self.parse_file_coverage_line(trimmed) {
                    files.push(file_cov);
                }
            }
        }

        Ok(CoverageResult {
            lines_percent,
            branches_percent: None, // cargo llvm-cov doesn't always provide branch coverage
            functions_percent: None,
            lines_covered,
            lines_total,
            files,
            framework: Framework::Cargo,
        })
    }

    /// Extract percentage from a coverage line.
    pub(crate) fn extract_percentage(&self, line: &str) -> Option<f64> {
        // Look for patterns like "85.42%" or "85.4 %"
        if let Ok(re) = regex::Regex::new(r"(\d+\.?\d*)\s*%") {
            if let Some(caps) = re.captures(line) {
                return caps.get(1).and_then(|m| m.as_str().parse().ok());
            }
        }
        None
    }

    /// Extract covered/total counts from a coverage line.
    pub(crate) fn extract_coverage_counts(&self, line: &str) -> Option<(u32, u32)> {
        // Look for patterns like "(234/274 lines)" or "234/274"
        if let Ok(re) = regex::Regex::new(r"(\d+)/(\d+)") {
            if let Some(caps) = re.captures(line) {
                let covered = caps.get(1).and_then(|m| m.as_str().parse().ok())?;
                let total = caps.get(2).and_then(|m| m.as_str().parse().ok())?;
                return Some((covered, total));
            }
        }
        None
    }

    /// Parse a file coverage line.
    fn parse_file_coverage_line(&self, line: &str) -> Option<FileCoverage> {
        // Try to extract: "path/to/file.rs: 90.5% (100/110)"
        let parts: Vec<&str> = line.split(':').collect();
        if parts.is_empty() {
            return None;
        }

        let path = parts[0].trim().to_string();
        if !path.ends_with(".rs") {
            return None;
        }

        let rest = parts[1..].join(":");
        let lines_percent = self.extract_percentage(&rest).unwrap_or(0.0);

        Some(FileCoverage {
            path,
            lines_percent,
            covered_lines: Vec::new(), // Detailed line info requires more complex parsing
            uncovered_lines: Vec::new(),
        })
    }
}

impl TestBackend for CargoBackend {
    fn run_suite(&self, filter: Option<&str>) -> TestResult<TestRunResult> {
        let mut args: Vec<&str> = Vec::new();

        // Add custom command args if specified
        if let Some(ref cmd) = self.config.command {
            // If a custom command is specified, we run it directly
            return self.run_custom_command(cmd);
        }

        // Add filter if provided
        if let Some(f) = filter {
            args.push(f);
        }

        self.run_with_retry(&args)
    }

    fn smoke(&self) -> TestResult<TestRunResult> {
        // Use smoke test config for cargo (default: --lib)
        let smoke_args = self.config.smoke.args_for(Framework::Cargo).unwrap_or("--lib");

        let args: Vec<&str> = smoke_args.split_whitespace().collect();
        self.run_cargo_test(&args)
    }

    fn coverage(&self) -> TestResult<CoverageResult> {
        // Run cargo llvm-cov (requires llvm-cov to be installed)
        let mut cmd = Command::new("cargo");
        cmd.arg("llvm-cov")
            .arg("--json")
            .current_dir(&self.project_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                TestError::CoverageNotSupported(
                    "cargo-llvm-cov not installed. Install with: cargo install cargo-llvm-cov"
                        .to_string(),
                )
            } else {
                TestError::ExecutionFailed(e.to_string())
            }
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // Check for errors
        if !output.status.success() && stderr.contains("error") {
            // Try without --json flag (older versions)
            return self.coverage_fallback();
        }

        // Try to parse JSON output first
        if let Ok(cov) = self.parse_json_coverage(&stdout) {
            return Ok(cov);
        }

        // Fall back to text parsing
        self.parse_coverage_output(&format!("{}\n{}", stdout, stderr))
    }

    fn run_filtered(&self, pattern: &str) -> TestResult<TestRunResult> {
        // cargo test accepts test name filter directly
        self.run_with_retry(&[pattern])
    }

    fn run_files(&self, files: &[&str]) -> TestResult<TestRunResult> {
        // cargo test doesn't directly support file filtering
        // We'll extract module paths from file names and use those as filters
        let mut args: Vec<&str> = Vec::new();

        for file in files {
            // Convert file path to module path
            // e.g., "src/foo/bar.rs" -> "foo::bar"
            // e.g., "tests/integration.rs" -> "integration"
            if let Some(module) = self.file_to_module(file) {
                args.push(Box::leak(module.into_boxed_str())); // Leak is OK for short-lived test runs
            }
        }

        if args.is_empty() {
            return Err(TestError::ConfigError(
                "No valid test modules found in specified files".to_string(),
            ));
        }

        // Run tests matching any of the modules
        // We'll need to run multiple times or use a complex filter
        // For simplicity, run once with all filters
        self.run_with_retry(&args)
    }

    fn supports_coverage(&self) -> bool {
        true
    }

    fn supports_watch(&self) -> bool {
        true
    }

    fn framework(&self) -> Framework {
        Framework::Cargo
    }
}

impl CargoBackend {
    /// Run a custom test command (used when config.command is set).
    fn run_custom_command(&self, cmd: &str) -> TestResult<TestRunResult> {
        let start = Instant::now();

        // Parse the command
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return Err(TestError::ConfigError("Empty custom command".to_string()));
        }

        let mut command = Command::new(parts[0]);
        if parts.len() > 1 {
            command.args(&parts[1..]);
        }

        command
            .current_dir(&self.project_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = command
            .output()
            .map_err(|e| TestError::ExecutionFailed(e.to_string()))?;

        let duration_ms = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let combined = format!("{}\n{}", stdout, stderr);

        self.parse_human_output(&combined, duration_ms, output.status.success())
    }

    /// Convert a file path to a module path for test filtering.
    pub(crate) fn file_to_module(&self, file: &str) -> Option<String> {
        let path = std::path::Path::new(file);

        // Remove file extension
        let stem = path.file_stem()?.to_str()?;

        // Handle different source locations
        let path_str = path.to_str()?;

        if path_str.starts_with("src/") {
            // src/foo/bar.rs -> foo::bar
            // src/lib.rs -> (root module, return empty or skip)
            // src/main.rs -> (binary, skip)
            let without_src = path_str.strip_prefix("src/")?;
            let without_ext = without_src.strip_suffix(".rs")?;

            if without_ext == "lib" || without_ext == "main" {
                return None;
            }

            // Convert path separators to ::
            let module = without_ext.replace('/', "::");
            Some(module)
        } else if path_str.starts_with("tests/") {
            // tests/integration.rs -> integration
            let without_tests = path_str.strip_prefix("tests/")?;
            let without_ext = without_tests.strip_suffix(".rs")?;
            Some(without_ext.replace('/', "::"))
        } else {
            // Unknown location, try to use stem
            Some(stem.to_string())
        }
    }

    /// Fallback coverage method without --json flag.
    fn coverage_fallback(&self) -> TestResult<CoverageResult> {
        let mut cmd = Command::new("cargo");
        cmd.arg("llvm-cov")
            .current_dir(&self.project_root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.output().map_err(|e| {
            TestError::CoverageNotSupported(format!("cargo-llvm-cov failed: {}", e))
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        self.parse_coverage_output(&format!("{}\n{}", stdout, stderr))
    }

    /// Parse JSON coverage output from cargo llvm-cov.
    fn parse_json_coverage(&self, output: &str) -> TestResult<CoverageResult> {
        // cargo llvm-cov --json outputs in a specific JSON format
        // We'll try to parse it; if it fails, we fall back to text parsing
        #[derive(Deserialize)]
        struct LlvmCovReport {
            data: Vec<LlvmCovData>,
        }

        #[derive(Deserialize)]
        struct LlvmCovData {
            totals: LlvmCovTotals,
            files: Option<Vec<LlvmCovFile>>,
        }

        #[derive(Deserialize)]
        struct LlvmCovTotals {
            lines: LlvmCovMetric,
            #[serde(default)]
            branches: Option<LlvmCovMetric>,
            #[serde(default)]
            functions: Option<LlvmCovMetric>,
        }

        #[derive(Deserialize, Default)]
        struct LlvmCovMetric {
            count: u32,
            covered: u32,
            percent: f64,
        }

        #[derive(Deserialize)]
        struct LlvmCovFile {
            filename: String,
            summary: LlvmCovFileSummary,
        }

        #[derive(Deserialize)]
        struct LlvmCovFileSummary {
            lines: LlvmCovMetric,
        }

        let report: LlvmCovReport =
            serde_json::from_str(output).map_err(|e| TestError::ParseError(e.to_string()))?;

        let data = report
            .data
            .into_iter()
            .next()
            .ok_or_else(|| TestError::ParseError("No coverage data found".to_string()))?;

        let files: Vec<FileCoverage> = data
            .files
            .unwrap_or_default()
            .into_iter()
            .map(|f| FileCoverage {
                path: f.filename,
                lines_percent: f.summary.lines.percent,
                covered_lines: Vec::new(),
                uncovered_lines: Vec::new(),
            })
            .collect();

        Ok(CoverageResult {
            lines_percent: data.totals.lines.percent,
            branches_percent: data.totals.branches.map(|b| b.percent),
            functions_percent: data.totals.functions.map(|f| f.percent),
            lines_covered: data.totals.lines.covered,
            lines_total: data.totals.lines.count,
            files,
            framework: Framework::Cargo,
        })
    }
}

