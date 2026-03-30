// SPDX-License-Identifier: MIT
//! Security scanning tool invocation and output parsing.
use crate::config::SecurityError;
use super::formats::{GitleaksMatch, SemgrepOutput, TrivyOutput};
use super::types::{Finding, FindingType, Location, ScanResult, ScanType, Severity};
use super::reports::{compute_content_hash, AuditReport, CveEntry, CveReport, SastReport, ToolStatus};
use std::path::Path;
use std::process::Command;
use std::time::Instant;

pub trait SecureBackend: Send + Sync {
    fn secret_detection(&self, content: &str) -> Result<ScanResult, SecurityError>;
    fn audit(&self, root: &Path) -> Result<AuditReport, SecurityError>;
    fn dependency_scan(&self, manifest: &Path) -> Result<CveReport, SecurityError>;
    fn static_analysis(&self, path: &Path) -> Result<SastReport, SecurityError>;
    fn available_tools(&self) -> Vec<ToolStatus>;
}

pub fn run_gitleaks(content: &str) -> Result<ScanResult, SecurityError> {
    let temp_dir = tempfile::tempdir().map_err(|e| SecurityError::ScanFailed(format!("Failed to create temp directory: {}", e)))?;
    let temp_file = temp_dir.path().join("scan_content.txt");
    std::fs::write(&temp_file, content).map_err(|e| SecurityError::ScanFailed(format!("Failed to write temp file: {}", e)))?;
    let start = Instant::now();
    let output = Command::new("gitleaks")
        .args(["detect", "--source", temp_dir.path().to_str().unwrap_or("."), "--report-format", "json", "--report-path", "/dev/stdout", "--no-git"])
        .output()
        .map_err(|e| SecurityError::ScanFailed(format!("Failed to run gitleaks: {}", e)))?;
    let duration_ms = start.elapsed().as_millis() as u64;
    let findings = match output.status.code() {
        Some(0) => Vec::new(),
        Some(1) => {
            let matches: Vec<GitleaksMatch> = serde_json::from_slice(&output.stdout).unwrap_or_default();
            matches.into_iter().map(|m| Finding {
                severity: Severity::High,
                finding_type: FindingType::Secret,
                location: Location { file: m.file, line: Some(m.line_number), column: None, snippet: Some(m.match_text.clone()) },
                description: m.description,
                remediation: "Remove the secret and rotate the credential".to_string(),
                rule_id: m.rule_id,
                cve_id: None,
                content_hash: Some(compute_content_hash(&m.match_text)),
            }).collect()
        }
        _ => return Err(SecurityError::ScanFailed(format!("gitleaks failed: {}", String::from_utf8_lossy(&output.stderr)))),
    };
    Ok(ScanResult { passed: findings.is_empty(), findings, tool_used: "gitleaks".to_string(), scan_type: ScanType::SecretDetection, duration_ms })
}

pub fn run_gitleaks_on_path(path: &Path) -> Result<ScanResult, SecurityError> {
    let start = Instant::now();
    let output = Command::new("gitleaks")
        .args(["detect", "--source", path.to_str()
            .ok_or_else(|| SecurityError::ScanFailed("path contains invalid UTF-8".to_string()))?, "--report-format", "json", "--report-path", "/dev/stdout"])
        .output()
        .map_err(|e| SecurityError::ScanFailed(format!("Failed to run gitleaks: {}", e)))?;
    let duration_ms = start.elapsed().as_millis() as u64;
    let findings = match output.status.code() {
        Some(0) => Vec::new(),
        Some(1) => {
            let matches: Vec<GitleaksMatch> = serde_json::from_slice(&output.stdout).unwrap_or_default();
            matches.into_iter().map(|m| Finding {
                severity: Severity::High,
                finding_type: FindingType::Secret,
                location: Location { file: m.file, line: Some(m.line_number), column: None, snippet: Some(m.match_text.clone()) },
                description: m.description,
                remediation: "Remove the secret and rotate the credential".to_string(),
                rule_id: m.rule_id,
                cve_id: None,
                content_hash: Some(compute_content_hash(&m.match_text)),
            }).collect()
        }
        _ => return Err(SecurityError::ScanFailed(format!("gitleaks failed: {}", String::from_utf8_lossy(&output.stderr)))),
    };
    Ok(ScanResult { passed: findings.is_empty(), findings, tool_used: "gitleaks".to_string(), scan_type: ScanType::SecretDetection, duration_ms })
}

pub fn run_trivy(manifest_path: &Path) -> Result<CveReport, SecurityError> {
    let output = Command::new("trivy")
        .args(["fs", "--format", "json", "--scanners", "vuln", manifest_path.to_str()
            .ok_or_else(|| SecurityError::ScanFailed("manifest path contains invalid UTF-8".to_string()))?])
        .output()
        .map_err(|e| SecurityError::ScanFailed(format!("Failed to run trivy: {}", e)))?;
    if !output.status.success() {
        return Err(SecurityError::ScanFailed(format!("trivy failed: {}", String::from_utf8_lossy(&output.stderr))));
    }
    let trivy_output: TrivyOutput = serde_json::from_slice(&output.stdout).map_err(|e| SecurityError::ScanFailed(format!("Failed to parse trivy output: {}", e)))?;
    let vulnerabilities: Vec<CveEntry> = trivy_output.results.iter()
        .flat_map(|r| r.vulnerabilities.as_ref().unwrap_or(&Vec::new()).clone())
        .map(|v| CveEntry {
            cve_id: v.vulnerability_id,
            severity: match v.severity.as_str() {
                "CRITICAL" => Severity::Critical,
                "HIGH" => Severity::High,
                "MEDIUM" => Severity::Medium,
                "LOW" => Severity::Low,
                _ => Severity::Info,
            },
            package: v.pkg_name,
            version: v.installed_version,
            fixed_version: v.fixed_version,
            description: v.title,
            url: v.primary_url.unwrap_or_default(),
        }).collect();
    let passed = !vulnerabilities.iter().any(|v| matches!(v.severity, Severity::Critical | Severity::High));
    Ok(CveReport { passed, vulnerabilities, packages_scanned: trivy_output.results.len(), tool_used: "trivy".to_string() })
}

pub fn run_semgrep(path: &Path, ruleset: &str) -> Result<SastReport, SecurityError> {
    let output = Command::new("semgrep")
        .args(["--config", ruleset, "--json", path.to_str()
            .ok_or_else(|| SecurityError::ScanFailed("path contains invalid UTF-8".to_string()))?])
        .output()
        .map_err(|e| SecurityError::ScanFailed(format!("Failed to run semgrep: {}", e)))?;
    if !output.status.success() && output.status.code() != Some(1) {
        return Err(SecurityError::ScanFailed(format!("semgrep failed: {}", String::from_utf8_lossy(&output.stderr))));
    }
    let semgrep_output: SemgrepOutput = serde_json::from_slice(&output.stdout).map_err(|e| SecurityError::ScanFailed(format!("Failed to parse semgrep output: {}", e)))?;
    let findings: Vec<Finding> = semgrep_output.results.into_iter()
        .map(|r| Finding {
            severity: match r.extra.severity.as_str() {
                "ERROR" => Severity::High,
                "WARNING" => Severity::Medium,
                _ => Severity::Low,
            },
            finding_type: FindingType::Vulnerability,
            location: Location { file: r.path, line: Some(r.start.line), column: Some(r.start.col), snippet: Some(r.extra.lines) },
            description: r.extra.message,
            remediation: r.extra.fix.unwrap_or_default(),
            rule_id: r.check_id,
            cve_id: None,
            content_hash: None,
        }).collect();
    let passed = !findings.iter().any(|f| matches!(f.severity, Severity::Critical | Severity::High));
    Ok(SastReport { passed, findings, files_scanned: semgrep_output.paths.scanned.len(), rules_applied: semgrep_output.stats.total_rules_applied, tool_used: "semgrep".to_string() })
}

pub fn get_current_commit(root: &Path) -> Option<String> {
    let output = Command::new("git").args(["rev-parse", "HEAD"]).current_dir(root).output().ok()?;
    if output.status.success() {
        String::from_utf8(output.stdout).ok().map(|s| s.trim().to_string())
    } else {
        None
    }
}
