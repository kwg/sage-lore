// SPDX-License-Identifier: MIT
//! Tool output format deserialization structures.
//!
//! This module defines the JSON output formats for external security scanning
//! tools (gitleaks, trivy, semgrep) used to parse their results.

use serde::Deserialize;

// ============================================================================
// Gitleaks Output Format
// ============================================================================

/// Gitleaks JSON output format for a single match.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct GitleaksMatch {
    /// Description of the secret type
    #[serde(default)]
    pub description: String,
    /// File where the secret was found
    pub file: String,
    /// Line number (1-indexed)
    #[serde(rename = "StartLine")]
    pub line_number: usize,
    /// The matching content (redacted by gitleaks)
    #[serde(rename = "Match")]
    pub match_text: String,
    /// Rule ID that triggered
    #[serde(rename = "RuleID")]
    pub rule_id: String,
}

// ============================================================================
// Trivy Output Format
// ============================================================================

/// Trivy JSON output format.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct TrivyOutput {
    /// Results per target (file/image)
    #[serde(default)]
    pub results: Vec<TrivyResult>,
}

/// Trivy result for a single target.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct TrivyResult {
    /// Target name (file path or image name)
    #[serde(default)]
    #[allow(dead_code)]
    pub target: String,
    /// Vulnerabilities found
    #[serde(default)]
    pub vulnerabilities: Option<Vec<TrivyVulnerability>>,
}

/// Trivy vulnerability entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct TrivyVulnerability {
    /// CVE ID (e.g., "CVE-2024-1234")
    #[serde(rename = "VulnerabilityID")]
    pub vulnerability_id: String,
    /// Package name
    #[serde(rename = "PkgName")]
    pub pkg_name: String,
    /// Installed version
    pub installed_version: String,
    /// Fixed version if available
    pub fixed_version: Option<String>,
    /// Severity level (CRITICAL, HIGH, MEDIUM, LOW)
    pub severity: String,
    /// Short description
    #[serde(default)]
    pub title: String,
    /// URL for more info
    #[serde(rename = "PrimaryURL")]
    pub primary_url: Option<String>,
}

// ============================================================================
// Semgrep Output Format
// ============================================================================

/// Semgrep JSON output format.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct SemgrepOutput {
    /// List of findings
    #[serde(default)]
    pub results: Vec<SemgrepResult>,
    /// Paths info
    #[serde(default)]
    pub paths: SemgrepPaths,
    /// Stats about the scan
    #[serde(default)]
    pub stats: SemgrepStats,
}

/// Semgrep individual finding.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct SemgrepResult {
    /// Rule ID that matched
    pub check_id: String,
    /// File path
    pub path: String,
    /// Start position
    pub start: SemgrepPosition,
    /// End position
    #[serde(default)]
    #[allow(dead_code)]
    pub end: SemgrepPosition,
    /// Extra metadata
    pub extra: SemgrepExtra,
}

/// Semgrep position in file.
#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct SemgrepPosition {
    /// Line number (1-indexed)
    #[serde(default)]
    pub line: usize,
    /// Column number (1-indexed)
    #[serde(default)]
    pub col: usize,
}

/// Semgrep extra metadata.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct SemgrepExtra {
    /// Human-readable message
    #[serde(default)]
    pub message: String,
    /// Severity level (ERROR, WARNING, INFO)
    #[serde(default)]
    pub severity: String,
    /// Code lines that matched
    #[serde(default)]
    pub lines: String,
    /// Suggested fix
    #[serde(default)]
    pub fix: Option<String>,
}

/// Semgrep paths info.
#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct SemgrepPaths {
    /// Files that were scanned
    #[serde(default)]
    pub scanned: Vec<String>,
}

/// Semgrep stats.
#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct SemgrepStats {
    /// Total rules applied
    #[serde(default, rename = "totalRules")]
    pub total_rules_applied: usize,
}
