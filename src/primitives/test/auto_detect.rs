// SPDX-License-Identifier: MIT
//! Auto-detection of test frameworks.

use crate::primitives::test::backends::{
    BatsBackend, CargoBackend, GoBackend, JestBackend, MakeBackend, NpmBackend, PytestBackend,
    VitestBackend,
};
use crate::primitives::test::framework_detect::{
    has_bats_files, has_pytest_markers, makefile_has_test_target, read_package_json,
};
use crate::primitives::test::noop_backend::NoopBackend;
use crate::primitives::test::r#trait::TestBackend;
use crate::primitives::test::types::{
    CoverageResult, Framework, TestConfig, TestResult, TestRunResult,
};
use std::path::{Path, PathBuf};

/// Composite backend with auto-detection of the appropriate test framework.
///
/// This backend inspects the project structure to determine which test
/// framework to use. It follows the detection priority order:
///
/// 1. Rust (Cargo.toml)
/// 2. Python (pytest markers)
/// 3. Node.js (package.json) - checks for vitest, jest, then generic npm
/// 4. Go (go.mod)
/// 5. Bash (bats files)
/// 6. Makefile fallback
pub struct AutoDetectBackend {
    /// Project root directory
    project_root: PathBuf,
    /// The detected backend (lazily initialized)
    inner: Box<dyn TestBackend>,
    /// Configuration to pass to the detected backend
    config: TestConfig,
    /// Override framework (skips auto-detection)
    override_framework: Option<Framework>,
}

impl AutoDetectBackend {
    /// Create a new AutoDetectBackend that will detect the framework.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        let project_root = project_root.into();
        let config = TestConfig::default();
        let inner = Self::detect(&project_root, &config);
        Self {
            project_root,
            inner,
            config,
            override_framework: None,
        }
    }

    /// Create with custom configuration.
    pub fn with_config(project_root: impl Into<PathBuf>, config: TestConfig) -> Self {
        let project_root = project_root.into();
        let override_framework = config.framework;
        let inner = if let Some(framework) = config.framework {
            Self::backend_for_framework(framework, &project_root, &config)
        } else {
            Self::detect(&project_root, &config)
        };
        Self {
            project_root,
            inner,
            config,
            override_framework,
        }
    }

    /// Get the detected framework.
    pub fn detected_framework(&self) -> Framework {
        self.inner.framework()
    }

    /// Override the detected framework.
    pub fn set_framework(&mut self, framework: Framework) {
        self.override_framework = Some(framework);
        self.inner = Self::backend_for_framework(framework, &self.project_root, &self.config);
    }

    /// Get a list of available frameworks in this project.
    pub fn available_frameworks(&self) -> Vec<Framework> {
        let mut frameworks = Vec::new();

        if self.project_root.join("Cargo.toml").exists() {
            frameworks.push(Framework::Cargo);
        }
        if has_pytest_markers(&self.project_root) {
            frameworks.push(Framework::Pytest);
        }
        if self.project_root.join("package.json").exists() {
            if let Some(pkg) = read_package_json(&self.project_root) {
                if pkg.dev_dependencies.contains_key("vitest")
                    || pkg.dependencies.contains_key("vitest")
                {
                    frameworks.push(Framework::Vitest);
                }
                if pkg.dev_dependencies.contains_key("jest")
                    || pkg.dependencies.contains_key("jest")
                {
                    frameworks.push(Framework::Jest);
                }
                if pkg.scripts.contains_key("test") {
                    frameworks.push(Framework::Npm);
                }
            }
        }
        if self.project_root.join("go.mod").exists() {
            frameworks.push(Framework::Go);
        }
        if has_bats_files(&self.project_root) {
            frameworks.push(Framework::Bats);
        }
        if makefile_has_test_target(&self.project_root) {
            frameworks.push(Framework::Make);
        }

        frameworks
    }

    /// Detect the appropriate backend for the project.
    fn detect(project_root: &Path, config: &TestConfig) -> Box<dyn TestBackend> {
        // Priority order: most specific first

        // 1. Rust (Cargo.toml)
        if project_root.join("Cargo.toml").exists() {
            return Box::new(CargoBackend::with_config(
                project_root.to_path_buf(),
                config.clone(),
            ));
        }

        // 2. Python (pytest markers)
        if has_pytest_markers(project_root) {
            return Box::new(PytestBackend::with_config(
                project_root.to_path_buf(),
                config.clone(),
            ));
        }

        // 3. Node.js (package.json)
        if project_root.join("package.json").exists() {
            if let Some(pkg) = read_package_json(project_root) {
                // Check for specific frameworks in devDependencies first, then dependencies
                if pkg.dev_dependencies.contains_key("vitest")
                    || pkg.dependencies.contains_key("vitest")
                {
                    return Box::new(VitestBackend::with_config(
                        project_root.to_path_buf(),
                        config.clone(),
                    ));
                }
                if pkg.dev_dependencies.contains_key("jest")
                    || pkg.dependencies.contains_key("jest")
                {
                    return Box::new(JestBackend::with_config(
                        project_root.to_path_buf(),
                        config.clone(),
                    ));
                }
                // Generic npm test
                if pkg.scripts.contains_key("test") {
                    return Box::new(NpmBackend::with_config(
                        project_root.to_path_buf(),
                        config.clone(),
                    ));
                }
            }
        }

        // 4. Go (go.mod)
        if project_root.join("go.mod").exists() {
            return Box::new(GoBackend::with_config(
                project_root.to_path_buf(),
                config.clone(),
            ));
        }

        // 5. Bash (bats tests)
        if has_bats_files(project_root) {
            return Box::new(BatsBackend::with_config(
                project_root.to_path_buf(),
                config.clone(),
            ));
        }

        // 6. Makefile fallback
        if makefile_has_test_target(project_root) {
            return Box::new(MakeBackend::with_config(
                project_root.to_path_buf(),
                config.clone(),
            ));
        }

        // No framework detected — build diagnostic
        let mut checked = Vec::new();
        if !project_root.join("Cargo.toml").exists() {
            checked.push("no Cargo.toml");
        }
        if !project_root.join("package.json").exists() {
            checked.push("no package.json");
        }
        if !project_root.join("go.mod").exists() {
            checked.push("no go.mod");
        }
        if !project_root.join("pyproject.toml").exists()
            && !project_root.join("pytest.ini").exists()
            && !project_root.join("conftest.py").exists()
        {
            checked.push("no pytest markers");
        }

        let has_sources = project_root.join("src").exists()
            || project_root.join("lib").exists()
            || project_root.join("tests").exists();

        let diagnostic = if !has_sources {
            format!(
                "Project root has no src/, lib/, or tests/ directories. Checked: {}",
                checked.join(", ")
            )
        } else {
            format!(
                "Checked: {}",
                checked.join(", ")
            )
        };

        Box::new(NoopBackend::new(diagnostic))
    }

    /// Create a backend for a specific framework.
    fn backend_for_framework(
        framework: Framework,
        project_root: &Path,
        config: &TestConfig,
    ) -> Box<dyn TestBackend> {
        match framework {
            Framework::Cargo => Box::new(CargoBackend::with_config(
                project_root.to_path_buf(),
                config.clone(),
            )),
            Framework::Jest => {
                Box::new(JestBackend::with_config(project_root.to_path_buf(), config.clone()))
            }
            Framework::Vitest => Box::new(VitestBackend::with_config(
                project_root.to_path_buf(),
                config.clone(),
            )),
            Framework::Npm => {
                Box::new(NpmBackend::with_config(project_root.to_path_buf(), config.clone()))
            }
            Framework::Pytest => Box::new(PytestBackend::with_config(
                project_root.to_path_buf(),
                config.clone(),
            )),
            Framework::Go => {
                Box::new(GoBackend::with_config(project_root.to_path_buf(), config.clone()))
            }
            Framework::Bats => Box::new(BatsBackend::with_config(
                project_root.to_path_buf(),
                config.clone(),
            )),
            Framework::Make => Box::new(MakeBackend::with_config(
                project_root.to_path_buf(),
                config.clone(),
            )),
        }
    }
}

impl TestBackend for AutoDetectBackend {
    fn run_suite(&self, filter: Option<&str>) -> TestResult<TestRunResult> {
        self.inner.run_suite(filter)
    }

    fn smoke(&self) -> TestResult<TestRunResult> {
        self.inner.smoke()
    }

    fn coverage(&self) -> TestResult<CoverageResult> {
        self.inner.coverage()
    }

    fn run_filtered(&self, pattern: &str) -> TestResult<TestRunResult> {
        self.inner.run_filtered(pattern)
    }

    fn run_files(&self, files: &[&str]) -> TestResult<TestRunResult> {
        self.inner.run_files(files)
    }

    fn supports_coverage(&self) -> bool {
        self.inner.supports_coverage()
    }

    fn supports_watch(&self) -> bool {
        self.inner.supports_watch()
    }

    fn framework(&self) -> Framework {
        self.inner.framework()
    }
}
