// SPDX-License-Identifier: MIT
//! sage-lore init — first-run setup (D19, D27, D36, #178).
//!
//! Creates config and security policy files for user or project setup.
//! Per-file creation: skips existing files, reports what was created/skipped.

use std::fs;
use std::path::{Path, PathBuf};

use clap::Args;

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Initialize a project (.sage-lore/) instead of user config
    #[arg(long)]
    pub project: bool,
}

const DEFAULT_USER_CONFIG: &str = r#"# sage-lore user configuration
# Run 'sage-lore --help' for options

llm:
  backend: claude
  context_limit: 100000
"#;

const DEFAULT_PROJECT_CONFIG: &str = r#"# sage-lore project configuration

project:
  name: null    # set your project name
  root: "."

# platform:     # uncomment to configure Forgejo
#   provider: forgejo
#   url: "http://your-forgejo:3000"
#   repo: "owner/repo"
#   token_env: "FORGEJO_API_TOKEN"

llm:
  backend: claude
  context_limit: 100000
"#;

const DEFAULT_POLICY: &str = r#"# sage-lore security policy
# Engine requires this file to run (D10).

security_level: standard
required_tools: []
fallback_policy: warn

secret_detection:
  on_finding: abort_and_reset
  on_existing: block

dependency_scan:
  enabled: true
  severity_threshold: LOW
  on_finding: abort_and_reset

static_analysis:
  enabled: false
  ruleset: "p/security-audit"
  on_finding: warn
"#;

pub fn handle_init(args: InitArgs) -> Result<(), Box<dyn std::error::Error>> {
    if args.project {
        init_project()
    } else {
        init_user()
    }
}

fn init_user() -> Result<(), Box<dyn std::error::Error>> {
    let user_root = dirs::config_dir()
        .ok_or("Could not determine config directory")?
        .join("sage-lore");

    println!("Initializing sage-lore user config at {}", user_root.display());

    create_if_missing(&user_root.join("config.yaml"), DEFAULT_USER_CONFIG)?;
    create_if_missing(
        &user_root.join("security/policy.yaml"),
        DEFAULT_POLICY,
    )?;

    println!("\nDone. Run 'sage-lore init --project' in a project directory to set up project config.");
    Ok(())
}

fn init_project() -> Result<(), Box<dyn std::error::Error>> {
    let project_dir = PathBuf::from(".sage-lore");

    println!("Initializing sage-lore project config at .sage-lore/");

    create_if_missing(&project_dir.join("config.yaml"), DEFAULT_PROJECT_CONFIG)?;
    create_if_missing(
        &project_dir.join("security/policy.yaml"),
        DEFAULT_POLICY,
    )?;

    // Create empty scrolls directory
    let scrolls_dir = project_dir.join("scrolls");
    if !scrolls_dir.exists() {
        fs::create_dir_all(&scrolls_dir)?;
        println!("  Created: {}", scrolls_dir.display());
    } else {
        println!("  Skipped: {} (already exists)", scrolls_dir.display());
    }

    println!("\nDone. Project is ready for sage-lore.");
    Ok(())
}

fn create_if_missing(path: &Path, content: &str) -> Result<(), Box<dyn std::error::Error>> {
    if path.exists() {
        println!("  Skipped: {} (already exists)", path.display());
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    println!("  Created: {}", path.display());
    Ok(())
}
