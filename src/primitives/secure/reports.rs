// SPDX-License-Identifier: MIT
//! Security scanning report types.
//!
//! This module defines report structures for aggregating scan results:
//! audit reports, CVE reports, and SAST reports.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use super::types::{Finding, ScanResult, Severity};


/// Full audit report aggregating all scan results, run before scroll execution begins (D12).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    /// Whether the audit passed (no blockers)
    pub passed: bool,
    /// Individual scan results
    pub scans: Vec<ScanResult>,
    /// Total number of findings
    pub total_findings: usize,
    /// Number of blocking findings (Critical/High)
    pub blockers: usize,
    /// Number of warning-level findings
    pub warnings: usize,
    /// When the audit was performed
    pub timestamp: DateTime<Utc>,
    /// Git commit that was scanned
    pub commit_scanned: String,
}

impl AuditReport {
    /// Create a new audit report from scan results.
    pub fn from_scans(scans: Vec<ScanResult>, commit: impl Into<String>) -> Self {
        let total_findings: usize = scans.iter().map(|s| s.findings.len()).sum();
        let blockers: usize = scans
            .iter()
            .flat_map(|s| &s.findings)
            .filter(|f| f.is_blocker())
            .count();
        let warnings = total_findings - blockers;
        let passed = blockers == 0;

        Self {
            passed,
            scans,
            total_findings,
            blockers,
            warnings,
            timestamp: Utc::now(),
            commit_scanned: commit.into(),
        }
    }

    /// Get all findings from all scans.
    pub fn all_findings(&self) -> impl Iterator<Item = &Finding> {
        self.scans.iter().flat_map(|s| &s.findings)
    }

    /// Get only blocking findings.
    pub fn blocking_findings(&self) -> impl Iterator<Item = &Finding> {
        self.all_findings().filter(|f| f.is_blocker())
    }
}

impl Default for AuditReport {
    fn default() -> Self {
        Self {
            passed: true,
            scans: Vec::new(),
            total_findings: 0,
            blockers: 0,
            warnings: 0,
            timestamp: Utc::now(),
            commit_scanned: String::new(),
        }
    }
}

/// CVE scan report from dependency scanning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveReport {
    /// Whether the scan passed (no blocking CVEs)
    pub passed: bool,
    /// List of CVE vulnerabilities found
    pub vulnerabilities: Vec<CveEntry>,
    /// Number of packages scanned
    pub packages_scanned: usize,
    /// Tool that performed the scan
    pub tool_used: String,
}

impl CveReport {
    /// Check if there are any critical vulnerabilities.
    pub fn has_critical(&self) -> bool {
        self.vulnerabilities
            .iter()
            .any(|v| matches!(v.severity, Severity::Critical))
    }

    /// Count vulnerabilities by severity.
    pub fn count_by_severity(&self, severity: Severity) -> usize {
        self.vulnerabilities
            .iter()
            .filter(|v| v.severity == severity)
            .count()
    }
}

impl Default for CveReport {
    fn default() -> Self {
        Self {
            passed: true,
            vulnerabilities: Vec::new(),
            packages_scanned: 0,
            tool_used: String::new(),
        }
    }
}

/// Individual CVE entry from dependency scanning.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CveEntry {
    /// CVE identifier (e.g., "CVE-2024-1234")
    pub cve_id: String,
    /// Severity of the vulnerability
    pub severity: Severity,
    /// Affected package name
    pub package: String,
    /// Installed version
    pub version: String,
    /// Version that fixes the vulnerability
    pub fixed_version: Option<String>,
    /// Description of the vulnerability
    pub description: String,
    /// URL with more information
    pub url: String,
}

impl CveEntry {
    /// Check if a fix is available.
    pub fn has_fix(&self) -> bool {
        self.fixed_version.is_some()
    }

    /// Check if this is a blocking CVE (Critical or High).
    pub fn is_blocker(&self) -> bool {
        matches!(self.severity, Severity::Critical | Severity::High)
    }
}

/// SAST (Static Application Security Testing) report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SastReport {
    /// Whether the scan passed (no blocking findings)
    pub passed: bool,
    /// Security findings from analysis
    pub findings: Vec<Finding>,
    /// Number of files scanned
    pub files_scanned: usize,
    /// Number of rules applied
    pub rules_applied: usize,
    /// Tool that performed the scan
    pub tool_used: String,
}

impl SastReport {
    /// Check if there are any blocking findings.
    pub fn has_blockers(&self) -> bool {
        self.findings.iter().any(|f| f.is_blocker())
    }

    /// Count findings by severity.
    pub fn count_by_severity(&self, severity: Severity) -> usize {
        self.findings.iter().filter(|f| f.severity == severity).count()
    }
}

impl Default for SastReport {
    fn default() -> Self {
        Self {
            passed: true,
            findings: Vec::new(),
            files_scanned: 0,
            rules_applied: 0,
            tool_used: String::new(),
        }
    }
}

/// Status of a security tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStatus {
    /// Tool name (e.g., "gitleaks", "trivy", "semgrep")
    pub name: String,
    /// Whether the tool is available on the system
    pub available: bool,
    /// Tool version if available
    pub version: Option<String>,
    /// Path to the tool executable
    pub path: Option<String>,
}

impl ToolStatus {
    /// Create a status for an unavailable tool.
    pub fn unavailable(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            available: false,
            version: None,
            path: None,
        }
    }

    /// Detect if a tool is installed on the system.
    pub fn detect(name: &str) -> Self {
        match which::which(name) {
            Ok(path) => {
                let version = std::process::Command::new(&path)
                    .arg("--version")
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .map(|s| s.lines().next().unwrap_or("").to_string());

                Self {
                    name: name.to_string(),
                    available: true,
                    version,
                    path: Some(path.to_string_lossy().to_string()),
                }
            }
            Err(_) => Self::unavailable(name),
        }
    }
}

/// Compute SHA256 hash for content, used to match known-safe entries in the security allowlist (D33).
pub fn compute_content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}
