// SPDX-License-Identifier: MIT
//! Security scanning primitives for the SAGE Method engine.
pub mod builtin;
mod formats;
pub mod policy;
pub mod reports;
pub mod scanner;
pub mod types;

pub use policy::PolicyDrivenBackend;
pub use scanner::SecureBackend;
pub use types::{Finding, FindingType, Location, ScanResult, ScanType, Severity};
pub use reports::{compute_content_hash, AuditReport, CveEntry, CveReport, SastReport, ToolStatus};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Critical < Severity::High);
        assert!(Severity::High < Severity::Medium);
        assert!(Severity::Medium < Severity::Low);
        assert!(Severity::Low < Severity::Info);
    }

    #[test]
    fn test_content_hash() {
        let hash = compute_content_hash("test content");
        assert!(hash.starts_with("sha256:"));
        assert_eq!(hash.len(), 7 + 64);
    }

    #[test]
    fn test_scan_result_has_blockers() {
        let mut result = ScanResult::default();
        assert!(!result.has_blockers());
        result.findings.push(Finding {
            severity: Severity::High,
            finding_type: FindingType::Secret,
            location: Location::default(),
            description: "Test".to_string(),
            remediation: "Fix it".to_string(),
            rule_id: "test-rule".to_string(),
            cve_id: None,
            content_hash: None,
        });
        assert!(result.has_blockers());
    }

    #[test]
    fn test_location_display() {
        let loc = Location {
            file: "src/main.rs".to_string(),
            line: Some(42),
            column: Some(10),
            snippet: None,
        };
        assert_eq!(loc.to_string(), "src/main.rs:42:10");
    }

    #[test]
    fn test_tool_status_unavailable() {
        let status = ToolStatus::unavailable("nonexistent-tool");
        assert!(!status.available);
        assert!(status.version.is_none());
        assert!(status.path.is_none());
    }

    #[test]
    fn test_audit_report_from_scans() {
        let scan = ScanResult {
            passed: false,
            findings: vec![Finding {
                severity: Severity::High,
                finding_type: FindingType::Secret,
                location: Location::default(),
                description: "Secret found".to_string(),
                remediation: "Remove it".to_string(),
                rule_id: "secret-rule".to_string(),
                cve_id: None,
                content_hash: None,
            }],
            tool_used: "test".to_string(),
            scan_type: ScanType::SecretDetection,
            duration_ms: 100,
        };
        let report = AuditReport::from_scans(vec![scan], "abc123");
        assert!(!report.passed);
        assert_eq!(report.total_findings, 1);
        assert_eq!(report.blockers, 1);
        assert_eq!(report.warnings, 0);
    }
}
