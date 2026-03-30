// SPDX-License-Identifier: MIT
//! Jest backend implementation.

use crate::primitives::test::r#trait::TestBackend;
use crate::primitives::test::types::{
    CoverageResult, Framework, TestConfig, TestResult, TestRunResult,
};
use std::path::PathBuf;

pub struct JestBackend {
    _project_root: PathBuf,
    _config: TestConfig,
}

impl JestBackend {
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

impl TestBackend for JestBackend {
    fn run_suite(&self, _filter: Option<&str>) -> TestResult<TestRunResult> {
        Ok(TestRunResult {
            framework: Framework::Jest,
            ..Default::default()
        })
    }

    fn smoke(&self) -> TestResult<TestRunResult> {
        Ok(TestRunResult {
            framework: Framework::Jest,
            ..Default::default()
        })
    }

    fn coverage(&self) -> TestResult<CoverageResult> {
        Ok(CoverageResult {
            framework: Framework::Jest,
            ..Default::default()
        })
    }

    fn run_filtered(&self, _pattern: &str) -> TestResult<TestRunResult> {
        Ok(TestRunResult {
            framework: Framework::Jest,
            ..Default::default()
        })
    }

    fn run_files(&self, _files: &[&str]) -> TestResult<TestRunResult> {
        Ok(TestRunResult {
            framework: Framework::Jest,
            ..Default::default()
        })
    }

    fn supports_coverage(&self) -> bool {
        true
    }

    fn supports_watch(&self) -> bool {
        true
    }

    fn framework(&self) -> Framework {
        Framework::Jest
    }
}

