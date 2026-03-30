// SPDX-License-Identifier: MIT
//! Integration tests for config hierarchy loading (v1.0, #178).
//!
//! Tests the three-tier config resolution:
//! - Global: discovered by PathResolver (optional)
//! - User: ~/.config/sage-lore/config.yaml (overrides)
//! - Project: .sage-lore/config.yaml (most specific wins)
//! - Environment variables override mapped fields
//!
//! NOTE: security_level is no longer in Config — it lives in Policy only (D23).

use sage_lore::config::{ConfigLoader, PathResolver};
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn test_project_config_loads() {
    let temp = TempDir::new().unwrap();
    let sage_dir = temp.path().join(".sage-lore");
    fs::create_dir_all(&sage_dir).unwrap();

    let config_content = r#"
project:
  name: my-project
llm:
  backend: ollama
state:
  git_tracked: false
"#;
    fs::write(sage_dir.join("config.yaml"), config_content).unwrap();

    let config = ConfigLoader::load_from_project(temp.path()).unwrap();

    assert_eq!(config.project_name(), "my-project");
    assert_eq!(config.llm_backend(), "ollama");
    assert!(!config.git_tracked());
}

#[tokio::test]
async fn test_defaults_when_no_config() {
    let temp = TempDir::new().unwrap();
    let sage_dir = temp.path().join(".sage-lore");
    fs::create_dir_all(&sage_dir).unwrap();

    let config = ConfigLoader::load_from_project(temp.path()).unwrap();

    assert_eq!(config.project_name(), "");
    assert_eq!(config.llm_backend(), "claude");
    assert_eq!(config.context_limit(), 100_000);
    assert!(config.git_tracked());
}

#[tokio::test]
async fn test_partial_config_fills_defaults() {
    let temp = TempDir::new().unwrap();
    let sage_dir = temp.path().join(".sage-lore");
    fs::create_dir_all(&sage_dir).unwrap();

    let config_content = r#"
project:
  name: partial-project
"#;
    fs::write(sage_dir.join("config.yaml"), config_content).unwrap();

    let config = ConfigLoader::load_from_project(temp.path()).unwrap();

    assert_eq!(config.project_name(), "partial-project");
    assert_eq!(config.llm_backend(), "claude"); // default
    assert!(config.git_tracked()); // default
}

#[tokio::test]
async fn test_three_tier_merge() {
    let temp = TempDir::new().unwrap();
    let global = temp.path().join("global");
    let user = temp.path().join("user");
    let project = temp.path().join("project/.sage-lore");

    // Global: sets backend and context_limit
    fs::create_dir_all(global.join("config")).unwrap();
    fs::write(global.join("config/config.yaml"), r#"
llm:
  backend: ollama
  context_limit: 50000
project:
  name: global-name
"#).unwrap();

    // User: overrides context_limit only
    fs::create_dir_all(&user).unwrap();
    fs::write(user.join("config.yaml"), r#"
llm:
  context_limit: 75000
"#).unwrap();

    // Project: overrides name only
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("config.yaml"), r#"
project:
  name: project-name
"#).unwrap();

    let resolver = PathResolver::with_paths(Some(global), user, Some(project));
    let config = ConfigLoader::load(&resolver).unwrap();

    assert_eq!(config.project_name(), "project-name"); // project wins
    assert_eq!(config.llm_backend(), "ollama"); // global, not overridden
    assert_eq!(config.context_limit(), 75_000); // user wins over global
}

#[tokio::test]
async fn test_option_merge_absent_doesnt_override() {
    // D31: absent fields (None) should not override present values from lower tiers
    let temp = TempDir::new().unwrap();
    let global = temp.path().join("global");
    let project = temp.path().join("project/.sage-lore");

    // Global sets everything
    fs::create_dir_all(global.join("config")).unwrap();
    fs::write(global.join("config/config.yaml"), r#"
project:
  name: global-project
llm:
  backend: ollama
  context_limit: 50000
state:
  git_tracked: false
"#).unwrap();

    // Project only sets name — everything else should stay from global
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("config.yaml"), r#"
project:
  name: project-override
"#).unwrap();

    let resolver = PathResolver::with_paths(
        Some(global),
        temp.path().join("nonexistent-user"),
        Some(project),
    );
    let config = ConfigLoader::load(&resolver).unwrap();

    assert_eq!(config.project_name(), "project-override"); // project wins
    assert_eq!(config.llm_backend(), "ollama"); // global preserved
    assert_eq!(config.context_limit(), 50_000); // global preserved
    assert!(!config.git_tracked()); // global preserved
}

#[tokio::test]
async fn test_invalid_yaml_returns_error() {
    let temp = TempDir::new().unwrap();
    let sage_dir = temp.path().join(".sage-lore");
    fs::create_dir_all(&sage_dir).unwrap();

    fs::write(sage_dir.join("config.yaml"), "this is not: valid: yaml: at all").unwrap();

    let result = ConfigLoader::load_from_project(temp.path());
    assert!(result.is_err());
}

#[tokio::test]
async fn test_legacy_sage_project_yaml_migration() {
    let temp = TempDir::new().unwrap();
    // Need .sage-lore/ for project root detection + .git for boundary
    fs::create_dir_all(temp.path().join(".sage-lore")).unwrap();
    fs::create_dir_all(temp.path().join(".git")).unwrap();

    let legacy = r#"
project_name: legacy-project
project_root: "."
state:
  git_tracked: false
"#;
    fs::write(temp.path().join(".sage-project.yaml"), legacy).unwrap();

    let config = ConfigLoader::load_from_project(temp.path()).unwrap();
    assert_eq!(config.project_name(), "legacy-project");
    assert!(!config.git_tracked());
}

#[tokio::test]
async fn test_platform_config() {
    let temp = TempDir::new().unwrap();
    let sage_dir = temp.path().join(".sage-lore");
    fs::create_dir_all(&sage_dir).unwrap();

    let config_content = r#"
platform:
  provider: forgejo
  url: "http://10.1.50.12:3000"
  repo: "kai/sage-lore"
  token_env: "MY_CUSTOM_TOKEN"
"#;
    fs::write(sage_dir.join("config.yaml"), config_content).unwrap();

    let config = ConfigLoader::load_from_project(temp.path()).unwrap();
    assert_eq!(config.platform_url(), Some("http://10.1.50.12:3000"));
    assert_eq!(config.platform_repo(), Some("kai/sage-lore"));
    assert_eq!(config.platform_token_env(), Some("MY_CUSTOM_TOKEN"));
}
