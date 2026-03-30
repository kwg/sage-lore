// SPDX-License-Identifier: MIT
//! Security configuration types (levels, floors, allowlists).

use serde::{Deserialize, Serialize};
use std::path::Path;

use super::error::SecurityError;
use super::severity::{OnExisting, OnFinding, SeverityThreshold};

/// Security level determines audit behavior and strictness.
///
/// | Level | Initial Clone | After Merge/Pull | Ongoing |
/// |-------|---------------|------------------|---------|
/// | `Paranoid` | Full history scan | Scan all new commits | Every commit scanned |
/// | `Standard` | Current tree only | Scan merge commits only | Staged content only |
/// | `Relaxed` | Optional prompt | Trust upstream | Warn only |
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "lowercase")]
pub enum SecurityLevel {
    /// Relaxed mode: minimal scanning, warnings only
    Relaxed,
    /// Standard mode: balanced scanning for typical projects
    #[default]
    Standard,
    /// Paranoid mode: never trust unscanned commits
    Paranoid,
}

impl std::fmt::Display for SecurityLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecurityLevel::Relaxed => write!(f, "relaxed"),
            SecurityLevel::Standard => write!(f, "standard"),
            SecurityLevel::Paranoid => write!(f, "paranoid"),
        }
    }
}

/// Global security floor from `~/.config/sage-lore/security-floor.yaml`.
///
/// Organizations can set a minimum security level that projects cannot go below.
/// This ensures consistent security standards across all projects.
#[derive(Debug, Clone, Deserialize)]
pub struct SecurityFloor {
    /// Minimum allowed security level
    pub minimum_security_level: SecurityLevel,
    /// Additional tools that must be present (additive to project requirements)
    #[serde(default)]
    pub minimum_required_tools: Vec<String>,
}

impl SecurityFloor {
    /// Load global security floor from `~/.config/sage-lore/security-floor.yaml`.
    ///
    /// Returns `None` if the file doesn't exist (no global floor configured).
    /// Uses XDG_CONFIG_HOME if set, else ~/.config (D24).
    pub fn load_global() -> Result<Option<Self>, SecurityError> {
        let config_dir = dirs::config_dir().ok_or_else(|| {
            SecurityError::PolicyViolation("Could not determine config directory".to_string())
        })?;
        let floor_path = config_dir.join("sage-lore/security-floor.yaml");

        if !floor_path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&floor_path).map_err(|e| {
            SecurityError::PolicyViolation(format!("Failed to read security floor: {}", e))
        })?;

        let floor: SecurityFloor = serde_yaml::from_str(&content).map_err(|e| {
            SecurityError::PolicyViolation(format!("Invalid security floor format: {}", e))
        })?;

        Ok(Some(floor))
    }
}

/// Secret detection configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecretDetectionConfig {
    /// Action when secret found mid-scroll
    #[serde(default)]
    pub on_finding: OnFinding,
    /// Action when audit finds pre-existing secrets
    #[serde(default)]
    pub on_existing: OnExisting,
}

/// Dependency scanning configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyScanConfig {
    /// Whether dependency scanning is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Minimum severity to report
    #[serde(default)]
    pub severity_threshold: SeverityThreshold,
    /// Action when vulnerability found
    #[serde(default)]
    pub on_finding: OnFinding,
}

impl Default for DependencyScanConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            severity_threshold: SeverityThreshold::default(),
            on_finding: OnFinding::default(),
        }
    }
}

/// Static analysis configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticAnalysisConfig {
    /// Whether static analysis is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Semgrep ruleset to use
    #[serde(default = "default_ruleset")]
    pub ruleset: String,
    /// Action when vulnerability found
    #[serde(default = "default_warn")]
    pub on_finding: OnFinding,
}

impl Default for StaticAnalysisConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ruleset: default_ruleset(),
            on_finding: OnFinding::Warn,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_ruleset() -> String {
    "p/security-audit".to_string()
}

fn default_warn() -> OnFinding {
    OnFinding::Warn
}

/// Allowlist instance for specific secret findings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowlistInstance {
    /// Content hash - changes = re-flagged
    pub hash: String,
    /// File where the finding was detected
    pub file: String,
    /// Line number
    pub line: usize,
    /// Rule that triggered the finding
    pub rule: String,
    /// Reason for allowlisting
    pub reason: String,
    /// Who added this entry
    pub added_by: String,
    /// When this was added
    pub added_at: chrono::DateTime<chrono::Utc>,
}

/// Allowlist pattern for directory/glob-based exceptions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowlistPattern {
    /// Glob pattern for files
    pub glob: String,
    /// Rules to allow in matching files
    pub rules: Vec<String>,
    /// Reason for allowlisting
    pub reason: String,
}

/// Security allowlist from `.sage-lore/security/allowlist.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Allowlist {
    /// Specific instance allowlist entries
    #[serde(default)]
    pub instances: Vec<AllowlistInstance>,
    /// Pattern-based allowlist entries
    #[serde(default)]
    pub patterns: Vec<AllowlistPattern>,
}

impl Allowlist {
    /// Load allowlist from project, merging team and local files.
    /// Normalized to .sage-lore/security/ paths (D21, #178).
    pub fn load_from_project(project_root: &Path) -> Result<Self, SecurityError> {
        let team_path = project_root.join(".sage-lore/security/allowlist.yaml");
        let local_path = project_root.join(".sage-lore/security/allowlist.local.yaml");

        let mut allowlist = Allowlist::default();

        // Load team allowlist if it exists
        if team_path.exists() {
            let content = std::fs::read_to_string(&team_path).map_err(|e| {
                SecurityError::PolicyViolation(format!("Failed to read allowlist: {}", e))
            })?;
            allowlist = serde_yaml::from_str(&content).map_err(|e| {
                SecurityError::PolicyViolation(format!("Invalid allowlist format: {}", e))
            })?;
        }

        // Merge local allowlist if it exists
        if local_path.exists() {
            let content = std::fs::read_to_string(&local_path).map_err(|e| {
                SecurityError::PolicyViolation(format!("Failed to read local allowlist: {}", e))
            })?;
            let local: Allowlist = serde_yaml::from_str(&content).map_err(|e| {
                SecurityError::PolicyViolation(format!("Invalid local allowlist format: {}", e))
            })?;

            allowlist.instances.extend(local.instances);
            allowlist.patterns.extend(local.patterns);
        }

        Ok(allowlist)
    }

    /// Check if a finding is allowlisted.
    pub fn is_allowed(&self, file: &str, rule: &str, content_hash: Option<&str>) -> bool {
        // Check instance allowlist
        if let Some(hash) = content_hash {
            if self
                .instances
                .iter()
                .any(|i| i.hash == hash && i.rule == rule)
            {
                return true;
            }
        }

        // Check pattern allowlist
        for pattern in &self.patterns {
            if pattern.rules.contains(&rule.to_string()) && glob_matches(&pattern.glob, file) {
                return true;
            }
        }

        false
    }
}

/// Simple glob matching for allowlist patterns.
fn glob_matches(pattern: &str, path: &str) -> bool {
    // Basic glob matching: ** matches any path, * matches within segment
    let pattern_parts: Vec<&str> = pattern.split('/').collect();
    let path_parts: Vec<&str> = path.split('/').collect();

    glob_match_parts(&pattern_parts, &path_parts)
}

fn glob_match_parts(pattern: &[&str], path: &[&str]) -> bool {
    if pattern.is_empty() {
        return path.is_empty();
    }

    if pattern[0] == "**" {
        // ** matches zero or more path segments
        if pattern.len() == 1 {
            return true;
        }
        for i in 0..=path.len() {
            if glob_match_parts(&pattern[1..], &path[i..]) {
                return true;
            }
        }
        return false;
    }

    if path.is_empty() {
        return false;
    }

    if segment_matches(pattern[0], path[0]) {
        glob_match_parts(&pattern[1..], &path[1..])
    } else {
        false
    }
}

fn segment_matches(pattern: &str, segment: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == segment;
    }
    // Simple wildcard matching within segment
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        match segment[pos..].find(part) {
            Some(idx) => {
                if i == 0 && idx != 0 {
                    return false;
                }
                pos += idx + part.len();
            }
            None => return false,
        }
    }
    if !parts.last().unwrap_or(&"").is_empty() {
        segment.ends_with(parts.last().unwrap())
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_level_ordering() {
        assert!(SecurityLevel::Relaxed < SecurityLevel::Standard);
        assert!(SecurityLevel::Standard < SecurityLevel::Paranoid);
    }

    #[test]
    fn test_glob_matches() {
        assert!(glob_matches("tests/**", "tests/fixtures/api_mock.rs"));
        assert!(glob_matches("tests/fixtures/**", "tests/fixtures/api_mock.rs"));
        assert!(glob_matches("**/*.rs", "src/config/security.rs"));
        assert!(glob_matches("*.rs", "security.rs"));
        assert!(!glob_matches("src/*.rs", "tests/security.rs"));
    }
}
