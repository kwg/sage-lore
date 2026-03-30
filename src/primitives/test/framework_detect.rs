// SPDX-License-Identifier: MIT
//! Framework detection helpers.

use serde::Deserialize;
use std::path::Path;

/// Internal structure for parsing package.json files.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct PackageJson {
    #[serde(default)]
    pub(crate) _name: String,
    #[serde(default)]
    pub(crate) scripts: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub(crate) dependencies: std::collections::HashMap<String, serde_json::Value>,
    #[serde(default, rename = "devDependencies")]
    pub(crate) dev_dependencies: std::collections::HashMap<String, serde_json::Value>,
}

/// Check if the project has pytest markers (pytest.ini, conftest.py, or pyproject.toml with pytest).
pub(crate) fn has_pytest_markers(project_root: &Path) -> bool {
    // Check for pytest.ini
    if project_root.join("pytest.ini").exists() {
        return true;
    }

    // Check for conftest.py
    if project_root.join("conftest.py").exists() {
        return true;
    }

    // Check for pyproject.toml with pytest config
    let pyproject_path = project_root.join("pyproject.toml");
    if pyproject_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&pyproject_path) {
            // Look for [tool.pytest] or pytest in dependencies
            if content.contains("[tool.pytest") || content.contains("pytest") {
                return true;
            }
        }
    }

    // Check for setup.cfg with pytest config
    let setup_cfg = project_root.join("setup.cfg");
    if setup_cfg.exists() {
        if let Ok(content) = std::fs::read_to_string(&setup_cfg) {
            if content.contains("[tool:pytest]") {
                return true;
            }
        }
    }

    false
}

/// Read and parse package.json.
pub(crate) fn read_package_json(project_root: &Path) -> Option<PackageJson> {
    let pkg_path = project_root.join("package.json");
    if !pkg_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&pkg_path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Check if the project has bats test files.
pub(crate) fn has_bats_files(project_root: &Path) -> bool {
    // Check for tests/test.bats
    if project_root.join("tests").join("test.bats").exists() {
        return true;
    }

    // Check for any .bats files in tests/ directory
    let tests_dir = project_root.join("tests");
    if tests_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&tests_dir) {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "bats" {
                        return true;
                    }
                }
            }
        }
    }

    // Also check for test/ directory (alternative convention)
    let test_dir = project_root.join("test");
    if test_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&test_dir) {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "bats" {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Check if Makefile has a test target.
pub(crate) fn makefile_has_test_target(project_root: &Path) -> bool {
    let makefile = project_root.join("Makefile");
    if !makefile.exists() {
        return false;
    }

    if let Ok(content) = std::fs::read_to_string(&makefile) {
        // Look for a line starting with "test:" (possibly with dependencies)
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("test:")
                || trimmed.starts_with("test :")
                || trimmed == "test"
                || trimmed.starts_with(".PHONY:") && trimmed.contains("test")
            {
                // Found a test target
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_temp_project() -> TempDir {
        tempfile::tempdir().expect("Failed to create temp dir")
    }

    // ========================================================================
    // pytest marker detection tests
    // ========================================================================

    #[test]
    fn test_has_pytest_markers_with_pytest_ini() {
        let temp = create_temp_project();
        fs::write(temp.path().join("pytest.ini"), "[pytest]\ntestpaths = tests").unwrap();

        assert!(has_pytest_markers(&temp.path().to_path_buf()));
    }

    #[test]
    fn test_has_pytest_markers_with_conftest() {
        let temp = create_temp_project();
        fs::write(temp.path().join("conftest.py"), "import pytest").unwrap();

        assert!(has_pytest_markers(&temp.path().to_path_buf()));
    }

    #[test]
    fn test_has_pytest_markers_with_pyproject_toml() {
        let temp = create_temp_project();
        fs::write(
            temp.path().join("pyproject.toml"),
            "[tool.pytest.ini_options]\nminversion = \"6.0\""
        ).unwrap();

        assert!(has_pytest_markers(&temp.path().to_path_buf()));
    }

    #[test]
    fn test_has_pytest_markers_with_setup_cfg() {
        let temp = create_temp_project();
        fs::write(
            temp.path().join("setup.cfg"),
            "[tool:pytest]\naddopts = -v"
        ).unwrap();

        assert!(has_pytest_markers(&temp.path().to_path_buf()));
    }

    #[test]
    fn test_has_pytest_markers_no_markers() {
        let temp = create_temp_project();
        // Empty directory - no pytest markers

        assert!(!has_pytest_markers(&temp.path().to_path_buf()));
    }

    #[test]
    fn test_has_pytest_markers_pyproject_without_pytest() {
        let temp = create_temp_project();
        fs::write(
            temp.path().join("pyproject.toml"),
            "[tool.black]\nline-length = 88"
        ).unwrap();

        assert!(!has_pytest_markers(&temp.path().to_path_buf()));
    }

    // ========================================================================
    // package.json parsing tests
    // ========================================================================

    #[test]
    fn test_read_package_json_with_jest() {
        let temp = create_temp_project();
        fs::write(
            temp.path().join("package.json"),
            r#"{
                "name": "test-project",
                "devDependencies": {
                    "jest": "^29.0.0"
                }
            }"#
        ).unwrap();

        let pkg = read_package_json(&temp.path().to_path_buf()).unwrap();
        assert!(pkg.dev_dependencies.contains_key("jest"));
    }

    #[test]
    fn test_read_package_json_with_vitest() {
        let temp = create_temp_project();
        fs::write(
            temp.path().join("package.json"),
            r#"{
                "devDependencies": {
                    "vitest": "^0.34.0"
                }
            }"#
        ).unwrap();

        let pkg = read_package_json(&temp.path().to_path_buf()).unwrap();
        assert!(pkg.dev_dependencies.contains_key("vitest"));
    }

    #[test]
    fn test_read_package_json_with_test_script() {
        let temp = create_temp_project();
        fs::write(
            temp.path().join("package.json"),
            r#"{
                "scripts": {
                    "test": "mocha",
                    "build": "webpack"
                }
            }"#
        ).unwrap();

        let pkg = read_package_json(&temp.path().to_path_buf()).unwrap();
        assert!(pkg.scripts.contains_key("test"));
        assert_eq!(pkg.scripts.get("test").unwrap(), "mocha");
    }

    #[test]
    fn test_read_package_json_dependencies_and_dev_dependencies() {
        let temp = create_temp_project();
        fs::write(
            temp.path().join("package.json"),
            r#"{
                "dependencies": {
                    "react": "^18.0.0"
                },
                "devDependencies": {
                    "jest": "^29.0.0",
                    "typescript": "^5.0.0"
                }
            }"#
        ).unwrap();

        let pkg = read_package_json(&temp.path().to_path_buf()).unwrap();
        assert!(pkg.dependencies.contains_key("react"));
        assert!(pkg.dev_dependencies.contains_key("jest"));
        assert!(pkg.dev_dependencies.contains_key("typescript"));
    }

    #[test]
    fn test_read_package_json_nonexistent() {
        let temp = create_temp_project();

        let pkg = read_package_json(&temp.path().to_path_buf());
        assert!(pkg.is_none());
    }

    #[test]
    fn test_read_package_json_invalid_json() {
        let temp = create_temp_project();
        fs::write(
            temp.path().join("package.json"),
            "{ invalid json"
        ).unwrap();

        let pkg = read_package_json(&temp.path().to_path_buf());
        assert!(pkg.is_none());
    }

    #[test]
    fn test_read_package_json_minimal() {
        let temp = create_temp_project();
        fs::write(
            temp.path().join("package.json"),
            "{}"
        ).unwrap();

        let pkg = read_package_json(&temp.path().to_path_buf()).unwrap();
        assert!(pkg.scripts.is_empty());
        assert!(pkg.dependencies.is_empty());
        assert!(pkg.dev_dependencies.is_empty());
    }

    // ========================================================================
    // bats detection tests
    // ========================================================================

    #[test]
    fn test_has_bats_files_in_tests_dir() {
        let temp = create_temp_project();
        fs::create_dir(temp.path().join("tests")).unwrap();
        fs::write(
            temp.path().join("tests").join("test.bats"),
            "#!/usr/bin/env bats\n@test \"sample\" { true; }"
        ).unwrap();

        assert!(has_bats_files(&temp.path().to_path_buf()));
    }

    #[test]
    fn test_has_bats_files_in_test_dir() {
        let temp = create_temp_project();
        fs::create_dir(temp.path().join("test")).unwrap();
        fs::write(
            temp.path().join("test").join("sample.bats"),
            "#!/usr/bin/env bats"
        ).unwrap();

        assert!(has_bats_files(&temp.path().to_path_buf()));
    }

    #[test]
    fn test_has_bats_files_multiple_bats_files() {
        let temp = create_temp_project();
        fs::create_dir(temp.path().join("tests")).unwrap();
        fs::write(temp.path().join("tests").join("test1.bats"), "").unwrap();
        fs::write(temp.path().join("tests").join("test2.bats"), "").unwrap();

        assert!(has_bats_files(&temp.path().to_path_buf()));
    }

    #[test]
    fn test_has_bats_files_no_bats_files() {
        let temp = create_temp_project();
        fs::create_dir(temp.path().join("tests")).unwrap();
        fs::write(temp.path().join("tests").join("test.sh"), "#!/bin/bash").unwrap();

        assert!(!has_bats_files(&temp.path().to_path_buf()));
    }

    #[test]
    fn test_has_bats_files_no_test_directory() {
        let temp = create_temp_project();

        assert!(!has_bats_files(&temp.path().to_path_buf()));
    }

    // ========================================================================
    // Makefile test target detection tests
    // ========================================================================

    #[test]
    fn test_makefile_has_test_target_simple() {
        let temp = create_temp_project();
        fs::write(
            temp.path().join("Makefile"),
            "test:\n\techo 'running tests'"
        ).unwrap();

        assert!(makefile_has_test_target(&temp.path().to_path_buf()));
    }

    #[test]
    fn test_makefile_has_test_target_with_dependencies() {
        let temp = create_temp_project();
        fs::write(
            temp.path().join("Makefile"),
            "test: build\n\t./run-tests.sh"
        ).unwrap();

        assert!(makefile_has_test_target(&temp.path().to_path_buf()));
    }

    #[test]
    fn test_makefile_has_test_target_phony() {
        let temp = create_temp_project();
        fs::write(
            temp.path().join("Makefile"),
            ".PHONY: test\n\ntest:\n\tpytest"
        ).unwrap();

        assert!(makefile_has_test_target(&temp.path().to_path_buf()));
    }

    #[test]
    fn test_makefile_has_test_target_phony_multiple() {
        let temp = create_temp_project();
        fs::write(
            temp.path().join("Makefile"),
            ".PHONY: build test clean\n\ntest:\n\tgo test ./..."
        ).unwrap();

        assert!(makefile_has_test_target(&temp.path().to_path_buf()));
    }

    #[test]
    fn test_makefile_has_test_target_with_spacing() {
        let temp = create_temp_project();
        fs::write(
            temp.path().join("Makefile"),
            "test :\n\tcargo test"
        ).unwrap();

        assert!(makefile_has_test_target(&temp.path().to_path_buf()));
    }

    #[test]
    fn test_makefile_has_test_target_no_test_target() {
        let temp = create_temp_project();
        fs::write(
            temp.path().join("Makefile"),
            "build:\n\tgcc main.c\n\nclean:\n\trm -f *.o"
        ).unwrap();

        assert!(!makefile_has_test_target(&temp.path().to_path_buf()));
    }

    #[test]
    fn test_makefile_has_test_target_testing_not_test() {
        let temp = create_temp_project();
        fs::write(
            temp.path().join("Makefile"),
            "testing:\n\techo 'this is not a test target'"
        ).unwrap();

        assert!(!makefile_has_test_target(&temp.path().to_path_buf()));
    }

    #[test]
    fn test_makefile_has_test_target_no_makefile() {
        let temp = create_temp_project();

        assert!(!makefile_has_test_target(&temp.path().to_path_buf()));
    }

    #[test]
    fn test_makefile_has_test_target_complex_makefile() {
        let temp = create_temp_project();
        fs::write(
            temp.path().join("Makefile"),
            r#"
.PHONY: all build test clean

all: build

build:
	cargo build

test: build
	cargo test
	./integration-tests.sh

clean:
	cargo clean
"#
        ).unwrap();

        assert!(makefile_has_test_target(&temp.path().to_path_buf()));
    }

    #[test]
    fn test_makefile_has_test_target_comment_not_detected() {
        let temp = create_temp_project();
        fs::write(
            temp.path().join("Makefile"),
            "# Run tests with: make test\nbuild:\n\tgcc main.c"
        ).unwrap();

        // Comment mentions "test" but no actual test target
        assert!(!makefile_has_test_target(&temp.path().to_path_buf()));
    }
}
