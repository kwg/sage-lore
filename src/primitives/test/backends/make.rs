// SPDX-License-Identifier: MIT
//! Make backend implementation.

use crate::primitives::test::r#trait::TestBackend;
use crate::primitives::test::types::{
    CoverageResult, Framework, TestConfig, TestError, TestResult, TestRunResult,
};
use std::path::PathBuf;

pub struct MakeBackend {
    _project_root: PathBuf,
    _config: TestConfig,
}

impl MakeBackend {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            _project_root: project_root.into(),
            _config: TestConfig::default(),
        }
    }

    pub fn with_config(project_root: impl Into<PathBuf>, config: TestConfig) -> Self {
        Self {
            _project_root: project_root.into(),
            _config: config,
        }
    }
}

impl TestBackend for MakeBackend {
    fn run_suite(&self, _filter: Option<&str>) -> TestResult<TestRunResult> {
        Ok(TestRunResult {
            framework: Framework::Make,
            ..Default::default()
        })
    }

    fn smoke(&self) -> TestResult<TestRunResult> {
        Ok(TestRunResult {
            framework: Framework::Make,
            ..Default::default()
        })
    }

    fn coverage(&self) -> TestResult<CoverageResult> {
        Err(TestError::CoverageNotSupported("make".to_string()))
    }

    fn run_filtered(&self, _pattern: &str) -> TestResult<TestRunResult> {
        Ok(TestRunResult {
            framework: Framework::Make,
            ..Default::default()
        })
    }

    fn run_files(&self, _files: &[&str]) -> TestResult<TestRunResult> {
        Ok(TestRunResult {
            framework: Framework::Make,
            ..Default::default()
        })
    }

    fn supports_coverage(&self) -> bool {
        false
    }

    fn supports_watch(&self) -> bool {
        false
    }

    fn framework(&self) -> Framework {
        Framework::Make
    }
}
