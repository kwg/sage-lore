// SPDX-License-Identifier: MIT
//! Test framework auto-detection and discovery.
//!
//! This module provides auto-detection of test frameworks and unified test execution.

// Re-export the main types
pub use crate::primitives::test::auto_detect::AutoDetectBackend;
pub use crate::primitives::test::noop_backend::NoopBackend;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::test::backends::{
        BatsBackend, CargoBackend, GoBackend, JestBackend, MakeBackend, NpmBackend,
        PytestBackend, VitestBackend,
    };
    use crate::primitives::test::r#trait::TestBackend;
    use crate::primitives::test::types::{
        CoverageResult, FlakyConfig, Framework, RetryRecord, SmokeConfig, TestConfig,
        TestFailure, TestRunResult, TestSummary,
    };

    #[test]
    fn test_framework_display() {
        assert_eq!(Framework::Cargo.to_string(), "cargo");
        assert_eq!(Framework::Jest.to_string(), "jest");
        assert_eq!(Framework::Pytest.to_string(), "pytest");
    }

    #[test]
    fn test_framework_from_str() {
        assert_eq!("cargo".parse::<Framework>().unwrap(), Framework::Cargo);
        assert_eq!("rust".parse::<Framework>().unwrap(), Framework::Cargo);
        assert_eq!("pytest".parse::<Framework>().unwrap(), Framework::Pytest);
        assert_eq!("python".parse::<Framework>().unwrap(), Framework::Pytest);
        assert!("unknown".parse::<Framework>().is_err());
    }

    #[test]
    fn test_framework_supports_coverage() {
        assert!(Framework::Cargo.supports_coverage());
        assert!(Framework::Jest.supports_coverage());
        assert!(Framework::Pytest.supports_coverage());
        assert!(!Framework::Bats.supports_coverage());
        assert!(!Framework::Make.supports_coverage());
    }

    #[test]
    fn test_framework_supports_watch() {
        assert!(Framework::Cargo.supports_watch());
        assert!(Framework::Jest.supports_watch());
        assert!(!Framework::Bats.supports_watch());
    }

    #[test]
    fn test_test_summary_pass_rate() {
        let summary = TestSummary::new(100, 90, 10, 0);
        assert_eq!(summary.pass_rate(), 90.0);
        assert!(!summary.all_passed());

        let perfect = TestSummary::new(50, 50, 0, 0);
        assert_eq!(perfect.pass_rate(), 100.0);
        assert!(perfect.all_passed());

        let empty = TestSummary::default();
        assert_eq!(empty.pass_rate(), 100.0); // No tests = 100% pass
    }

    #[test]
    fn test_test_failure_location_string() {
        let failure = TestFailure::with_location("test_foo", "assertion failed", "src/lib.rs", 42);
        assert_eq!(failure.location_string(), Some("src/lib.rs:42".to_string()));

        let no_line = TestFailure {
            name: "test".to_string(),
            file: Some("src/lib.rs".to_string()),
            line: None,
            message: "failed".to_string(),
            expected: None,
            actual: None,
            stack: None,
        };
        assert_eq!(no_line.location_string(), Some("src/lib.rs".to_string()));

        let no_location = TestFailure::new("test", "failed");
        assert_eq!(no_location.location_string(), None);
    }

    #[test]
    fn test_retry_record_is_flaky() {
        let flaky = RetryRecord::passed_after_retry("test_api", 3);
        assert!(flaky.is_flaky());
        assert!(flaky.final_passed);

        let not_flaky = RetryRecord {
            test_name: "test_api".to_string(),
            attempts: 1,
            final_passed: true,
        };
        assert!(!not_flaky.is_flaky());

        let failed = RetryRecord::failed_after_retry("test_api", 3);
        assert!(!failed.is_flaky()); // Not flaky, just broken
        assert!(!failed.final_passed);
    }

    #[test]
    fn test_test_run_result_flaky_helpers() {
        let mut result = TestRunResult::default();
        assert!(!result.has_retries());
        assert_eq!(result.retry_count(), 0);

        result.retries.push(RetryRecord::passed_after_retry("test_a", 2));
        result
            .retries
            .push(RetryRecord::failed_after_retry("test_b", 3));

        assert!(result.has_retries());
        assert_eq!(result.retry_count(), 2);
        assert_eq!(result.flaky_tests().count(), 1);
        assert_eq!(result.failed_after_retry().count(), 1);
    }

    #[test]
    fn test_coverage_meets_threshold() {
        let coverage = CoverageResult {
            lines_percent: 85.0,
            branches_percent: Some(75.0),
            ..Default::default()
        };

        assert!(coverage.meets_threshold(80.0, Some(70.0)));
        assert!(coverage.meets_threshold(85.0, None));
        assert!(!coverage.meets_threshold(90.0, None));
        assert!(!coverage.meets_threshold(80.0, Some(80.0)));
    }

    #[test]
    fn test_smoke_config_args() {
        let config = SmokeConfig::default();
        assert_eq!(config.args_for(Framework::Cargo), Some("--lib"));
        assert_eq!(config.args_for(Framework::Pytest), Some("-m smoke"));
        assert_eq!(config.args_for(Framework::Bats), None);
    }

    #[test]
    fn test_flaky_config_defaults() {
        let config = FlakyConfig::default();
        assert_eq!(config.retry_count, 2);
        assert!(config.report_retries); // D51: mandatory reporting
    }

    #[test]
    fn test_noop_backend() {
        let backend = NoopBackend::new("test".to_string());
        assert!(!backend.supports_coverage());
        assert!(!backend.supports_watch());
        assert!(backend.run_suite(None).is_err());
        assert!(backend.smoke().is_err());
    }

    // ========================================================================
    // Framework-specific backend tests
    // ========================================================================

    #[test]
    fn test_cargo_backend() {
        let backend = CargoBackend::new("/tmp/test");
        assert_eq!(backend.framework(), Framework::Cargo);
        assert!(backend.supports_coverage());
        assert!(backend.supports_watch());
    }

    #[test]
    fn test_jest_backend() {
        let backend = JestBackend::new("/tmp/test");
        assert_eq!(backend.framework(), Framework::Jest);
        assert!(backend.supports_coverage());
        assert!(backend.supports_watch());
    }

    #[test]
    fn test_vitest_backend() {
        let backend = VitestBackend::new("/tmp/test");
        assert_eq!(backend.framework(), Framework::Vitest);
        assert!(backend.supports_coverage());
        assert!(backend.supports_watch());
    }

    #[test]
    fn test_npm_backend() {
        let backend = NpmBackend::new("/tmp/test");
        assert_eq!(backend.framework(), Framework::Npm);
        assert!(!backend.supports_coverage()); // Generic npm doesn't guarantee coverage
        assert!(!backend.supports_watch());
    }

    #[test]
    fn test_pytest_backend() {
        let backend = PytestBackend::new("/tmp/test");
        assert_eq!(backend.framework(), Framework::Pytest);
        assert!(backend.supports_coverage());
        assert!(backend.supports_watch());
    }

    #[test]
    fn test_go_backend() {
        let backend = GoBackend::new("/tmp/test");
        assert_eq!(backend.framework(), Framework::Go);
        assert!(backend.supports_coverage());
        assert!(backend.supports_watch());
    }

    #[test]
    fn test_bats_backend() {
        let backend = BatsBackend::new("/tmp/test");
        assert_eq!(backend.framework(), Framework::Bats);
        assert!(!backend.supports_coverage());
        assert!(!backend.supports_watch());
    }

    #[test]
    fn test_make_backend() {
        let backend = MakeBackend::new("/tmp/test");
        assert_eq!(backend.framework(), Framework::Make);
        assert!(!backend.supports_coverage());
        assert!(!backend.supports_watch());
    }

    // ========================================================================
    // AutoDetectBackend tests
    // ========================================================================

    mod auto_detect_tests {
        use super::*;
        use std::fs;
        use tempfile::TempDir;

        fn create_temp_project() -> TempDir {
            tempfile::tempdir().expect("Failed to create temp dir")
        }

        #[test]
        fn test_detect_cargo_project() {
            let temp = create_temp_project();
            fs::write(temp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

            let backend = AutoDetectBackend::new(temp.path());
            assert_eq!(backend.detected_framework(), Framework::Cargo);
        }

        #[test]
        fn test_detect_pytest_with_pytest_ini() {
            let temp = create_temp_project();
            fs::write(temp.path().join("pytest.ini"), "[pytest]\n").unwrap();

            let backend = AutoDetectBackend::new(temp.path());
            assert_eq!(backend.detected_framework(), Framework::Pytest);
        }

        #[test]
        fn test_detect_pytest_with_conftest() {
            let temp = create_temp_project();
            fs::write(temp.path().join("conftest.py"), "# pytest config").unwrap();

            let backend = AutoDetectBackend::new(temp.path());
            assert_eq!(backend.detected_framework(), Framework::Pytest);
        }

        #[test]
        fn test_detect_pytest_with_pyproject_toml() {
            let temp = create_temp_project();
            fs::write(
                temp.path().join("pyproject.toml"),
                "[tool.pytest.ini_options]\naddopts = \"-v\"",
            )
            .unwrap();

            let backend = AutoDetectBackend::new(temp.path());
            assert_eq!(backend.detected_framework(), Framework::Pytest);
        }

        #[test]
        fn test_detect_jest_project() {
            let temp = create_temp_project();
            fs::write(
                temp.path().join("package.json"),
                r#"{"devDependencies": {"jest": "^29.0.0"}}"#,
            )
            .unwrap();

            let backend = AutoDetectBackend::new(temp.path());
            assert_eq!(backend.detected_framework(), Framework::Jest);
        }

        #[test]
        fn test_detect_vitest_project() {
            let temp = create_temp_project();
            fs::write(
                temp.path().join("package.json"),
                r#"{"devDependencies": {"vitest": "^0.34.0"}}"#,
            )
            .unwrap();

            let backend = AutoDetectBackend::new(temp.path());
            assert_eq!(backend.detected_framework(), Framework::Vitest);
        }

        #[test]
        fn test_detect_npm_with_test_script() {
            let temp = create_temp_project();
            fs::write(
                temp.path().join("package.json"),
                r#"{"scripts": {"test": "mocha"}}"#,
            )
            .unwrap();

            let backend = AutoDetectBackend::new(temp.path());
            assert_eq!(backend.detected_framework(), Framework::Npm);
        }

        #[test]
        fn test_detect_go_project() {
            let temp = create_temp_project();
            fs::write(temp.path().join("go.mod"), "module example.com/test").unwrap();

            let backend = AutoDetectBackend::new(temp.path());
            assert_eq!(backend.detected_framework(), Framework::Go);
        }

        #[test]
        fn test_detect_bats_project() {
            let temp = create_temp_project();
            fs::create_dir(temp.path().join("tests")).unwrap();
            fs::write(
                temp.path().join("tests").join("test.bats"),
                "#!/usr/bin/env bats",
            )
            .unwrap();

            let backend = AutoDetectBackend::new(temp.path());
            assert_eq!(backend.detected_framework(), Framework::Bats);
        }

        #[test]
        fn test_detect_make_project() {
            let temp = create_temp_project();
            fs::write(
                temp.path().join("Makefile"),
                ".PHONY: test\n\ntest:\n\t./run_tests.sh",
            )
            .unwrap();

            let backend = AutoDetectBackend::new(temp.path());
            assert_eq!(backend.detected_framework(), Framework::Make);
        }

        #[test]
        fn test_detect_no_framework() {
            let temp = create_temp_project();
            // Empty directory - no framework markers

            let backend = AutoDetectBackend::new(temp.path());
            // NoopBackend returns Make as fallback
            assert_eq!(backend.framework(), Framework::Make);
            // But run_suite should fail with NoFrameworkDetected
            assert!(backend.run_suite(None).is_err());
        }

        #[test]
        fn test_detection_priority_cargo_over_npm() {
            let temp = create_temp_project();
            // Both Cargo.toml and package.json present
            fs::write(temp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
            fs::write(
                temp.path().join("package.json"),
                r#"{"devDependencies": {"jest": "^29.0.0"}}"#,
            )
            .unwrap();

            let backend = AutoDetectBackend::new(temp.path());
            // Cargo should win (higher priority)
            assert_eq!(backend.detected_framework(), Framework::Cargo);
        }

        #[test]
        fn test_detection_priority_pytest_over_npm() {
            let temp = create_temp_project();
            // Both pytest.ini and package.json present
            fs::write(temp.path().join("pytest.ini"), "[pytest]").unwrap();
            fs::write(
                temp.path().join("package.json"),
                r#"{"devDependencies": {"jest": "^29.0.0"}}"#,
            )
            .unwrap();

            let backend = AutoDetectBackend::new(temp.path());
            // Pytest should win (higher priority)
            assert_eq!(backend.detected_framework(), Framework::Pytest);
        }

        #[test]
        fn test_detection_vitest_over_jest() {
            let temp = create_temp_project();
            // Both vitest and jest in devDependencies - vitest should win
            fs::write(
                temp.path().join("package.json"),
                r#"{"devDependencies": {"vitest": "^0.34.0", "jest": "^29.0.0"}}"#,
            )
            .unwrap();

            let backend = AutoDetectBackend::new(temp.path());
            assert_eq!(backend.detected_framework(), Framework::Vitest);
        }

        #[test]
        fn test_available_frameworks() {
            let temp = create_temp_project();
            // Create a project with multiple framework markers
            fs::write(temp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
            fs::write(
                temp.path().join("Makefile"),
                ".PHONY: test\ntest:\n\tcargo test",
            )
            .unwrap();

            let backend = AutoDetectBackend::new(temp.path());
            let available = backend.available_frameworks();

            assert!(available.contains(&Framework::Cargo));
            assert!(available.contains(&Framework::Make));
        }

        #[test]
        fn test_set_framework_override() {
            let temp = create_temp_project();
            fs::write(temp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
            fs::write(
                temp.path().join("package.json"),
                r#"{"devDependencies": {"jest": "^29.0.0"}}"#,
            )
            .unwrap();

            let mut backend = AutoDetectBackend::new(temp.path());
            assert_eq!(backend.detected_framework(), Framework::Cargo);

            // Override to Jest
            backend.set_framework(Framework::Jest);
            assert_eq!(backend.detected_framework(), Framework::Jest);
        }

        #[test]
        fn test_config_framework_override() {
            let temp = create_temp_project();
            fs::write(temp.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

            let config = TestConfig {
                framework: Some(Framework::Make),
                ..Default::default()
            };

            let backend = AutoDetectBackend::with_config(temp.path(), config);
            // Config override should take precedence
            assert_eq!(backend.detected_framework(), Framework::Make);
        }

        #[test]
        fn test_makefile_test_target_variations() {
            use crate::primitives::test::framework_detect::makefile_has_test_target;

            // Test various makefile test target formats
            let test_cases = vec![
                ("test:\n\techo hello", true),
                ("test: deps\n\techo hello", true),
                (".PHONY: test\ntest:\n\techo", true),
                (".PHONY: build test\ntest:\n\techo", true),
                ("build:\n\tgcc", false),
                ("testing:\n\techo", false),
            ];

            for (content, expected) in test_cases {
                let temp = create_temp_project();
                fs::write(temp.path().join("Makefile"), content).unwrap();

                let has_test = makefile_has_test_target(&temp.path().to_path_buf());
                assert_eq!(
                    has_test, expected,
                    "Makefile content: {:?} should have test target: {}",
                    content, expected
                );
            }
        }
    }

    // ========================================================================
    // CargoBackend JSON parsing tests
    // ========================================================================

    mod cargo_backend_tests {
        use super::*;

        fn create_cargo_backend() -> CargoBackend {
            CargoBackend::new("/tmp/test-project")
        }

        #[test]
        fn test_parse_json_output_all_passed() {
            let backend = create_cargo_backend();
            let json_output = r#"
{ "type": "suite", "event": "started", "test_count": 3 }
{ "type": "test", "event": "started", "name": "tests::test_add" }
{ "type": "test", "name": "tests::test_add", "event": "ok" }
{ "type": "test", "event": "started", "name": "tests::test_subtract" }
{ "type": "test", "name": "tests::test_subtract", "event": "ok" }
{ "type": "test", "event": "started", "name": "tests::test_multiply" }
{ "type": "test", "name": "tests::test_multiply", "event": "ok" }
{ "type": "suite", "event": "ok", "passed": 3, "failed": 0, "ignored": 0, "measured": 0, "filtered_out": 0, "exec_time": 0.001 }
"#;

            let result = backend.try_parse_json_output(json_output, 100);
            assert!(result.is_some());

            let result = result.unwrap();
            assert!(result.passed);
            assert_eq!(result.summary.total, 3);
            assert_eq!(result.summary.passed, 3);
            assert_eq!(result.summary.failed, 0);
            assert!(result.failures.is_empty());
        }

        #[test]
        fn test_parse_json_output_with_failure() {
            let backend = create_cargo_backend();
            let json_output = r#"
{ "type": "suite", "event": "started", "test_count": 2 }
{ "type": "test", "event": "started", "name": "tests::test_add" }
{ "type": "test", "name": "tests::test_add", "event": "ok" }
{ "type": "test", "event": "started", "name": "tests::test_subtract" }
{ "type": "test", "name": "tests::test_subtract", "event": "failed", "stdout": "thread 'tests::test_subtract' panicked at src/lib.rs:15:9\nassertion `left == right` failed\n  left: 5\n right: 3" }
{ "type": "suite", "event": "failed", "passed": 1, "failed": 1, "ignored": 0, "measured": 0, "filtered_out": 0, "exec_time": 0.002 }
"#;

            let result = backend.try_parse_json_output(json_output, 100);
            assert!(result.is_some());

            let result = result.unwrap();
            assert!(!result.passed);
            assert_eq!(result.summary.total, 2);
            assert_eq!(result.summary.passed, 1);
            assert_eq!(result.summary.failed, 1);
            assert_eq!(result.failures.len(), 1);
            assert_eq!(result.failures[0].name, "tests::test_subtract");
        }

        #[test]
        fn test_parse_json_output_with_ignored() {
            let backend = create_cargo_backend();
            let json_output = r#"
{ "type": "suite", "event": "started", "test_count": 3 }
{ "type": "test", "event": "started", "name": "tests::test_add" }
{ "type": "test", "name": "tests::test_add", "event": "ok" }
{ "type": "test", "event": "started", "name": "tests::test_slow" }
{ "type": "test", "name": "tests::test_slow", "event": "ignored" }
{ "type": "test", "event": "started", "name": "tests::test_subtract" }
{ "type": "test", "name": "tests::test_subtract", "event": "ok" }
{ "type": "suite", "event": "ok", "passed": 2, "failed": 0, "ignored": 1, "measured": 0, "filtered_out": 0, "exec_time": 0.001 }
"#;

            let result = backend.try_parse_json_output(json_output, 100);
            assert!(result.is_some());

            let result = result.unwrap();
            assert!(result.passed);
            assert_eq!(result.summary.total, 3);
            assert_eq!(result.summary.passed, 2);
            assert_eq!(result.summary.skipped, 1);
        }

        #[test]
        fn test_parse_json_output_empty_returns_none() {
            let backend = create_cargo_backend();
            let result = backend.try_parse_json_output("", 100);
            assert!(result.is_none());
        }

        #[test]
        fn test_parse_json_output_invalid_json_returns_none() {
            let backend = create_cargo_backend();
            let result = backend.try_parse_json_output("not json at all", 100);
            assert!(result.is_none());
        }

        #[test]
        fn test_parse_human_output_success() {
            let backend = create_cargo_backend();
            let human_output = r#"
running 3 tests
test tests::test_add ... ok
test tests::test_subtract ... ok
test tests::test_multiply ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
"#;

            let result = backend.parse_human_output(human_output, 100, true);
            assert!(result.is_ok());

            let result = result.unwrap();
            assert!(result.passed);
            assert_eq!(result.summary.passed, 3);
            assert_eq!(result.summary.failed, 0);
        }

        #[test]
        fn test_parse_human_output_failure() {
            let backend = create_cargo_backend();
            let human_output = r#"
running 2 tests
test tests::test_add ... ok
test tests::test_subtract ... FAILED

failures:

---- tests::test_subtract stdout ----
thread 'tests::test_subtract' panicked at src/lib.rs:15:9:
assertion `left == right` failed
  left: 5
 right: 3

failures:
    tests::test_subtract

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
"#;

            let result = backend.parse_human_output(human_output, 100, false);
            assert!(result.is_ok());

            let result = result.unwrap();
            assert!(!result.passed);
            assert_eq!(result.summary.passed, 1);
            assert_eq!(result.summary.failed, 1);
            assert_eq!(result.failures.len(), 1);
            assert_eq!(result.failures[0].name, "tests::test_subtract");
        }

        #[test]
        fn test_extract_panic_location() {
            let backend = create_cargo_backend();

            // Standard panic format
            let (file, line) = backend.extract_panic_location(
                "thread 'tests::test_foo' panicked at src/lib.rs:15:9",
            );
            assert_eq!(file, Some("src/lib.rs".to_string()));
            assert_eq!(line, Some(15));

            // Panic with message
            let (file, line) = backend.extract_panic_location(
                "panicked at 'assertion failed', src/main.rs:42:5",
            );
            assert_eq!(file, Some("src/main.rs".to_string()));
            assert_eq!(line, Some(42));

            // No panic location
            let (file, line) = backend.extract_panic_location("some random output");
            assert_eq!(file, None);
            assert_eq!(line, None);
        }

        #[test]
        fn test_extract_assertion_values() {
            let backend = create_cargo_backend();

            let output = r#"
assertion `left == right` failed
  left: `5`
 right: `3`
"#;

            let (expected, actual) = backend.extract_assertion_values(output);
            assert_eq!(expected, Some("3".to_string()));
            assert_eq!(actual, Some("5".to_string()));
        }

        #[test]
        fn test_file_to_module() {
            let backend = create_cargo_backend();

            // src files
            assert_eq!(
                backend.file_to_module("src/foo/bar.rs"),
                Some("foo::bar".to_string())
            );
            assert_eq!(
                backend.file_to_module("src/utils.rs"),
                Some("utils".to_string())
            );
            assert_eq!(backend.file_to_module("src/lib.rs"), None);
            assert_eq!(backend.file_to_module("src/main.rs"), None);

            // test files
            assert_eq!(
                backend.file_to_module("tests/integration.rs"),
                Some("integration".to_string())
            );
            assert_eq!(
                backend.file_to_module("tests/api/users.rs"),
                Some("api::users".to_string())
            );
        }

        #[test]
        fn test_extract_percentage() {
            let backend = create_cargo_backend();

            assert_eq!(backend.extract_percentage("Total: 85.42%"), Some(85.42));
            assert_eq!(backend.extract_percentage("Coverage: 100%"), Some(100.0));
            assert_eq!(backend.extract_percentage("90.5 % covered"), Some(90.5));
            assert_eq!(backend.extract_percentage("no percentage here"), None);
        }

        #[test]
        fn test_extract_coverage_counts() {
            let backend = create_cargo_backend();

            assert_eq!(
                backend.extract_coverage_counts("(234/274 lines)"),
                Some((234, 274))
            );
            assert_eq!(backend.extract_coverage_counts("100/110"), Some((100, 110)));
            assert_eq!(backend.extract_coverage_counts("no counts"), None);
        }

        #[test]
        fn test_retry_record_creation() {
            // Test D51: Flaky test tracking
            let passed = RetryRecord::passed_after_retry("test_flaky", 2);
            assert!(passed.is_flaky());
            assert!(passed.final_passed);
            assert_eq!(passed.attempts, 2);

            let failed = RetryRecord::failed_after_retry("test_broken", 3);
            assert!(!failed.is_flaky());
            assert!(!failed.final_passed);
            assert_eq!(failed.attempts, 3);
        }

        #[test]
        fn test_flaky_config_default_values() {
            // D51: Verify mandatory reporting is default
            let config = FlakyConfig::default();
            assert_eq!(config.retry_count, 2);
            assert!(config.report_retries);
        }
    }
}
