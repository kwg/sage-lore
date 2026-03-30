// SPDX-License-Identifier: MIT
//! Security scanning result types for the SAGE Method engine.
//!
//! This module defines all the data structures returned by security scanning
//! operations: secret detection, CVE scanning, static analysis, and audits.

use serde::{Deserialize, Serialize};

/// Result of a security scan operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    /// Whether the scan passed (no blocking findings)
    pub passed: bool,
    /// Individual findings from the scan
    pub findings: Vec<Finding>,
    /// Tool that performed the scan
    pub tool_used: String,
    /// Type of scan performed
    pub scan_type: ScanType,
    /// Scan duration in milliseconds
    pub duration_ms: u64,
}

impl ScanResult {
    /// Create a new passing scan result with no findings.
    pub fn passed(tool: impl Into<String>, scan_type: ScanType, duration_ms: u64) -> Self {
        Self {
            passed: true,
            findings: Vec::new(),
            tool_used: tool.into(),
            scan_type,
            duration_ms,
        }
    }

    /// Check if the scan has any findings.
    pub fn has_findings(&self) -> bool {
        !self.findings.is_empty()
    }

    /// Check if the scan has any blocking findings (Critical or High severity).
    pub fn has_blockers(&self) -> bool {
        self.findings
            .iter()
            .any(|f| matches!(f.severity, Severity::Critical | Severity::High))
    }

    /// Count findings by severity.
    pub fn count_by_severity(&self, severity: Severity) -> usize {
        self.findings.iter().filter(|f| f.severity == severity).count()
    }
}

impl Default for ScanResult {
    fn default() -> Self {
        Self {
            passed: true,
            findings: Vec::new(),
            tool_used: String::new(),
            scan_type: ScanType::SecretDetection,
            duration_ms: 0,
        }
    }
}

/// Individual security finding from a scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// Severity level of the finding
    pub severity: Severity,
    /// Type of finding
    pub finding_type: FindingType,
    /// Location in source code
    pub location: Location,
    /// Human-readable description
    pub description: String,
    /// Suggested remediation steps
    pub remediation: String,
    /// Rule ID that triggered this finding
    pub rule_id: String,
    /// CVE ID if applicable
    pub cve_id: Option<String>,
    /// Content hash for allowlist matching (D33)
    pub content_hash: Option<String>,
}

impl Finding {
    /// Check if this finding is a blocker (Critical or High).
    pub fn is_blocker(&self) -> bool {
        matches!(self.severity, Severity::Critical | Severity::High)
    }

    /// Check if this is a secret detection finding.
    pub fn is_secret(&self) -> bool {
        matches!(self.finding_type, FindingType::Secret)
    }
}

/// Severity level for security findings.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Critical severity - immediate action required
    Critical,
    /// High severity - should be addressed soon
    High,
    /// Medium severity - should be planned for
    Medium,
    /// Low severity - address when convenient
    Low,
    /// Informational - no action required
    #[default]
    Info,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Critical => write!(f, "CRITICAL"),
            Severity::High => write!(f, "HIGH"),
            Severity::Medium => write!(f, "MEDIUM"),
            Severity::Low => write!(f, "LOW"),
            Severity::Info => write!(f, "INFO"),
        }
    }
}

/// Type of security finding.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum FindingType {
    /// Secret/credential detected
    Secret,
    /// CVE vulnerability in dependency
    Cve,
    /// Code vulnerability from static analysis
    Vulnerability,
    /// Policy violation
    PolicyViolation,
}

impl std::fmt::Display for FindingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FindingType::Secret => write!(f, "Secret"),
            FindingType::Cve => write!(f, "CVE"),
            FindingType::Vulnerability => write!(f, "Vulnerability"),
            FindingType::PolicyViolation => write!(f, "Policy Violation"),
        }
    }
}

/// Source code location for a finding.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Location {
    /// File path relative to project root
    pub file: String,
    /// Line number (1-indexed)
    pub line: Option<usize>,
    /// Column number (1-indexed)
    pub column: Option<usize>,
    /// Code snippet showing the finding
    pub snippet: Option<String>,
}

impl Location {
    /// Create a new location with just a file path.
    pub fn file(path: impl Into<String>) -> Self {
        Self {
            file: path.into(),
            line: None,
            column: None,
            snippet: None,
        }
    }

    /// Create a location with file and line.
    pub fn file_line(path: impl Into<String>, line: usize) -> Self {
        Self {
            file: path.into(),
            line: Some(line),
            column: None,
            snippet: None,
        }
    }
}

impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.file)?;
        if let Some(line) = self.line {
            write!(f, ":{}", line)?;
            if let Some(col) = self.column {
                write!(f, ":{}", col)?;
            }
        }
        Ok(())
    }
}

/// Type of security scan.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ScanType {
    /// Secret/credential detection
    SecretDetection,
    /// Dependency CVE scanning
    DependencyCve,
    /// Static analysis (SAST)
    StaticAnalysis,
    /// Allowlist checking
    Allowlist,
    /// Sensitive path detection
    SensitivePaths,
}

impl std::fmt::Display for ScanType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanType::SecretDetection => write!(f, "Secret Detection"),
            ScanType::DependencyCve => write!(f, "Dependency CVE"),
            ScanType::StaticAnalysis => write!(f, "Static Analysis"),
            ScanType::Allowlist => write!(f, "Allowlist"),
            ScanType::SensitivePaths => write!(f, "Sensitive Paths"),
        }
    }
}
