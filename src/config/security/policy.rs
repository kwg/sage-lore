// SPDX-License-Identifier: MIT
//! Security policy loading and merging logic.

use serde::{Deserialize, Serialize};
use std::path::Path;

use super::config::{
    DependencyScanConfig, SecretDetectionConfig, SecurityFloor, SecurityLevel,
    StaticAnalysisConfig,
};
use super::error::SecurityError;
use super::severity::{max_fallback, max_on_existing, max_on_finding, FallbackPolicy};

/// Project security policy from `.sage-lore/security/policy.yaml`.
///
/// The engine will NOT run without this file — fail closed, no policy means no execution (D10).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    /// Security level for this project
    #[serde(default)]
    pub security_level: SecurityLevel,

    /// Tools required for this project
    #[serde(default)]
    pub required_tools: Vec<String>,

    /// What happens when a required tool is missing
    #[serde(default)]
    pub fallback_policy: FallbackPolicy,

    /// Secret detection behavior
    #[serde(default)]
    pub secret_detection: SecretDetectionConfig,

    /// Dependency scanning (trivy)
    #[serde(default)]
    pub dependency_scan: DependencyScanConfig,

    /// Static analysis (semgrep)
    #[serde(default)]
    pub static_analysis: StaticAnalysisConfig,

    /// Maximum scroll nesting depth (prevents infinite recursion)
    #[serde(default)]
    pub max_scroll_depth: Option<usize>,
}

impl Policy {
    /// Load policy from a file path.
    pub fn load(policy_path: &Path) -> Result<Self, SecurityError> {
        if !policy_path.exists() {
            return Err(SecurityError::PolicyNotFound(
                policy_path.display().to_string(),
            ));
        }

        let content = std::fs::read_to_string(policy_path).map_err(|e| {
            SecurityError::PolicyViolation(format!("Failed to read policy file: {}", e))
        })?;

        let policy: Policy = serde_yaml::from_str(&content).map_err(|e| {
            SecurityError::PolicyViolation(format!("Invalid policy format: {}", e))
        })?;

        Ok(policy)
    }

    /// Load policy from the default location in a project root.
    pub fn load_from_project(project_root: &Path) -> Result<Self, SecurityError> {
        let policy_path = project_root.join(".sage-lore/security/policy.yaml");
        Self::load(&policy_path)
    }

    /// Validate that this policy meets the global security floor.
    pub fn validate_against_floor(&self, floor: &SecurityFloor) -> Result<(), SecurityError> {
        if self.security_level < floor.minimum_security_level {
            return Err(SecurityError::BelowFloor {
                project: self.security_level,
                floor: floor.minimum_security_level,
            });
        }
        Ok(())
    }

    /// Get the scroll nesting depth limit (defaults to 5 if not configured).
    pub fn scroll_depth_limit(&self) -> usize {
        self.max_scroll_depth.unwrap_or(5)
    }

    /// Merge two policies — thresholds take max, rule lists take union, so strictness only increases (D29).
    ///
    /// This implements the "floor" semantics where stricter requirements win:
    /// - Numeric thresholds: max() (higher = stricter)
    /// - Rule lists: union() (more rules = stricter)
    /// - Boolean flags: || (if either requires it, it is required)
    /// - Depth limits: min() (lower = stricter)
    ///
    /// Typical usage: `corp_policy.merge(&project_policy)` where corp sets the floor.
    pub fn merge(&self, other: &Policy) -> Policy {
        use std::cmp::{max, min};

        // Merge required_tools using set union (deduplicate)
        let mut merged_tools = self.required_tools.clone();
        for tool in &other.required_tools {
            if !merged_tools.contains(tool) {
                merged_tools.push(tool.clone());
            }
        }

        // Merge dependency scan config
        let merged_dep_scan = DependencyScanConfig {
            enabled: self.dependency_scan.enabled || other.dependency_scan.enabled,
            severity_threshold: max(
                self.dependency_scan.severity_threshold,
                other.dependency_scan.severity_threshold,
            ),
            on_finding: max_on_finding(
                self.dependency_scan.on_finding,
                other.dependency_scan.on_finding,
            ),
        };

        // Merge static analysis config
        let merged_static = StaticAnalysisConfig {
            enabled: self.static_analysis.enabled || other.static_analysis.enabled,
            // For ruleset: if both enabled, prefer self (corp floor), otherwise take the enabled one
            ruleset: if self.static_analysis.enabled {
                self.static_analysis.ruleset.clone()
            } else {
                other.static_analysis.ruleset.clone()
            },
            on_finding: max_on_finding(
                self.static_analysis.on_finding,
                other.static_analysis.on_finding,
            ),
        };

        // Merge secret detection config
        let merged_secret = SecretDetectionConfig {
            on_finding: max_on_finding(
                self.secret_detection.on_finding,
                other.secret_detection.on_finding,
            ),
            on_existing: max_on_existing(
                self.secret_detection.on_existing,
                other.secret_detection.on_existing,
            ),
        };

        Policy {
            security_level: max(self.security_level, other.security_level),
            required_tools: merged_tools,
            fallback_policy: max_fallback(self.fallback_policy, other.fallback_policy),
            secret_detection: merged_secret,
            dependency_scan: merged_dep_scan,
            static_analysis: merged_static,
            max_scroll_depth: match (self.max_scroll_depth, other.max_scroll_depth) {
                (Some(a), Some(b)) => Some(min(a, b)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            },
        }
    }
}

#[cfg(test)]
mod tests;

