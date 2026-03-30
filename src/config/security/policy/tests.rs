// SPDX-License-Identifier: MIT
//! Tests for Policy struct and merge behavior.

use super::*;
use crate::config::security::severity::{OnExisting, OnFinding, SeverityThreshold};

#[test]
fn test_policy_default() {
    let yaml = "security_level: standard";
    let policy: Policy = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(policy.security_level, SecurityLevel::Standard);
    assert!(policy.required_tools.is_empty());
    assert_eq!(policy.fallback_policy, FallbackPolicy::HardStop);
}

#[test]
fn test_policy_merge_security_level() {
    // Test that max() is used for security_level
    let corp = Policy {
        security_level: SecurityLevel::Paranoid,
        required_tools: vec![],
        fallback_policy: FallbackPolicy::HardStop,
        secret_detection: SecretDetectionConfig::default(),
        dependency_scan: DependencyScanConfig::default(),
        static_analysis: StaticAnalysisConfig::default(),
        max_scroll_depth: None,
    };

    let project = Policy {
        security_level: SecurityLevel::Relaxed,
        required_tools: vec![],
        fallback_policy: FallbackPolicy::Warn,
        secret_detection: SecretDetectionConfig::default(),
        dependency_scan: DependencyScanConfig::default(),
        static_analysis: StaticAnalysisConfig::default(),
        max_scroll_depth: None,
    };

    let merged = corp.merge(&project);
    assert_eq!(merged.security_level, SecurityLevel::Paranoid);
}

#[test]
fn test_policy_merge_required_tools_union() {
    // Test that required_tools uses set union
    let corp = Policy {
        security_level: SecurityLevel::Standard,
        required_tools: vec!["gitleaks".to_string(), "trivy".to_string()],
        fallback_policy: FallbackPolicy::HardStop,
        secret_detection: SecretDetectionConfig::default(),
        dependency_scan: DependencyScanConfig::default(),
        static_analysis: StaticAnalysisConfig::default(),
        max_scroll_depth: None,
    };

    let project = Policy {
        security_level: SecurityLevel::Standard,
        required_tools: vec!["trivy".to_string(), "semgrep".to_string()],
        fallback_policy: FallbackPolicy::Warn,
        secret_detection: SecretDetectionConfig::default(),
        dependency_scan: DependencyScanConfig::default(),
        static_analysis: StaticAnalysisConfig::default(),
        max_scroll_depth: None,
    };

    let merged = corp.merge(&project);
    assert_eq!(merged.required_tools.len(), 3);
    assert!(merged.required_tools.contains(&"gitleaks".to_string()));
    assert!(merged.required_tools.contains(&"trivy".to_string()));
    assert!(merged.required_tools.contains(&"semgrep".to_string()));
}

#[test]
fn test_policy_merge_severity_threshold() {
    // Test that max() is used for severity thresholds
    let corp = Policy {
        security_level: SecurityLevel::Standard,
        required_tools: vec![],
        fallback_policy: FallbackPolicy::HardStop,
        secret_detection: SecretDetectionConfig::default(),
        dependency_scan: DependencyScanConfig {
            enabled: true,
            severity_threshold: SeverityThreshold::Medium,
            on_finding: OnFinding::AbortAndReset,
        },
        static_analysis: StaticAnalysisConfig::default(),
        max_scroll_depth: None,
    };

    let project = Policy {
        security_level: SecurityLevel::Standard,
        required_tools: vec![],
        fallback_policy: FallbackPolicy::Warn,
        secret_detection: SecretDetectionConfig::default(),
        dependency_scan: DependencyScanConfig {
            enabled: true,
            severity_threshold: SeverityThreshold::Low,
            on_finding: OnFinding::Warn,
        },
        static_analysis: StaticAnalysisConfig::default(),
        max_scroll_depth: None,
    };

    let merged = corp.merge(&project);
    assert_eq!(
        merged.dependency_scan.severity_threshold,
        SeverityThreshold::Medium
    );
}

#[test]
fn test_policy_merge_boolean_flags() {
    // Test that || is used for boolean flags
    let corp = Policy {
        security_level: SecurityLevel::Standard,
        required_tools: vec![],
        fallback_policy: FallbackPolicy::HardStop,
        secret_detection: SecretDetectionConfig::default(),
        dependency_scan: DependencyScanConfig {
            enabled: false,
            severity_threshold: SeverityThreshold::Low,
            on_finding: OnFinding::Warn,
        },
        static_analysis: StaticAnalysisConfig {
            enabled: true,
            ruleset: "p/security-audit".to_string(),
            on_finding: OnFinding::Warn,
        },
        max_scroll_depth: None,
    };

    let project = Policy {
        security_level: SecurityLevel::Standard,
        required_tools: vec![],
        fallback_policy: FallbackPolicy::Warn,
        secret_detection: SecretDetectionConfig::default(),
        dependency_scan: DependencyScanConfig {
            enabled: true,
            severity_threshold: SeverityThreshold::Low,
            on_finding: OnFinding::Warn,
        },
        static_analysis: StaticAnalysisConfig {
            enabled: false,
            ruleset: "p/default".to_string(),
            on_finding: OnFinding::Warn,
        },
        max_scroll_depth: None,
    };

    let merged = corp.merge(&project);
    assert!(merged.dependency_scan.enabled); // false || true = true
    assert!(merged.static_analysis.enabled); // true || false = true
}

#[test]
fn test_policy_merge_on_finding() {
    // Test that stricter OnFinding wins: AbortAndReset > PromptUser > Warn
    let corp = Policy {
        security_level: SecurityLevel::Standard,
        required_tools: vec![],
        fallback_policy: FallbackPolicy::HardStop,
        secret_detection: SecretDetectionConfig {
            on_finding: OnFinding::AbortAndReset,
            on_existing: OnExisting::Block,
        },
        dependency_scan: DependencyScanConfig::default(),
        static_analysis: StaticAnalysisConfig::default(),
        max_scroll_depth: None,
    };

    let project = Policy {
        security_level: SecurityLevel::Standard,
        required_tools: vec![],
        fallback_policy: FallbackPolicy::Warn,
        secret_detection: SecretDetectionConfig {
            on_finding: OnFinding::Warn,
            on_existing: OnExisting::Ignore,
        },
        dependency_scan: DependencyScanConfig::default(),
        static_analysis: StaticAnalysisConfig::default(),
        max_scroll_depth: None,
    };

    let merged = corp.merge(&project);
    assert_eq!(merged.secret_detection.on_finding, OnFinding::AbortAndReset);
    assert_eq!(merged.secret_detection.on_existing, OnExisting::Block);
}

#[test]
fn test_policy_merge_fallback_policy() {
    // Test that HardStop > Warn
    let corp = Policy {
        security_level: SecurityLevel::Standard,
        required_tools: vec![],
        fallback_policy: FallbackPolicy::HardStop,
        secret_detection: SecretDetectionConfig::default(),
        dependency_scan: DependencyScanConfig::default(),
        static_analysis: StaticAnalysisConfig::default(),
        max_scroll_depth: None,
    };

    let project = Policy {
        security_level: SecurityLevel::Standard,
        required_tools: vec![],
        fallback_policy: FallbackPolicy::Warn,
        secret_detection: SecretDetectionConfig::default(),
        dependency_scan: DependencyScanConfig::default(),
        static_analysis: StaticAnalysisConfig::default(),
        max_scroll_depth: None,
    };

    let merged = corp.merge(&project);
    assert_eq!(merged.fallback_policy, FallbackPolicy::HardStop);
}

#[test]
fn test_policy_merge_max_depth() {
    // Test that min() is used for max_scroll_depth (lower is stricter)
    let corp = Policy {
        security_level: SecurityLevel::Standard,
        required_tools: vec![],
        fallback_policy: FallbackPolicy::HardStop,
        secret_detection: SecretDetectionConfig::default(),
        dependency_scan: DependencyScanConfig::default(),
        static_analysis: StaticAnalysisConfig::default(),
        max_scroll_depth: Some(3),
    };

    let project = Policy {
        security_level: SecurityLevel::Standard,
        required_tools: vec![],
        fallback_policy: FallbackPolicy::Warn,
        secret_detection: SecretDetectionConfig::default(),
        dependency_scan: DependencyScanConfig::default(),
        static_analysis: StaticAnalysisConfig::default(),
        max_scroll_depth: Some(10),
    };

    let merged = corp.merge(&project);
    assert_eq!(merged.max_scroll_depth, Some(3));
}

#[test]
fn test_policy_merge_max_depth_none() {
    // Test max_scroll_depth with None values
    let corp = Policy {
        security_level: SecurityLevel::Standard,
        required_tools: vec![],
        fallback_policy: FallbackPolicy::HardStop,
        secret_detection: SecretDetectionConfig::default(),
        dependency_scan: DependencyScanConfig::default(),
        static_analysis: StaticAnalysisConfig::default(),
        max_scroll_depth: Some(3),
    };

    let project = Policy {
        security_level: SecurityLevel::Standard,
        required_tools: vec![],
        fallback_policy: FallbackPolicy::Warn,
        secret_detection: SecretDetectionConfig::default(),
        dependency_scan: DependencyScanConfig::default(),
        static_analysis: StaticAnalysisConfig::default(),
        max_scroll_depth: None,
    };

    let merged = corp.merge(&project);
    assert_eq!(merged.max_scroll_depth, Some(3));

    // Reverse order
    let merged2 = project.merge(&corp);
    assert_eq!(merged2.max_scroll_depth, Some(3));
}

#[test]
fn test_policy_merge_comprehensive() {
    // Comprehensive test matching the acceptance criteria from D4b
    let corp = Policy {
        security_level: SecurityLevel::Standard,
        required_tools: vec!["gitleaks".to_string()],
        fallback_policy: FallbackPolicy::HardStop,
        secret_detection: SecretDetectionConfig::default(),
        dependency_scan: DependencyScanConfig {
            enabled: true,
            severity_threshold: SeverityThreshold::Medium,
            on_finding: OnFinding::AbortAndReset,
        },
        static_analysis: StaticAnalysisConfig::default(),
        max_scroll_depth: None,
    };

    let project = Policy {
        security_level: SecurityLevel::Relaxed,
        required_tools: vec!["trivy".to_string()],
        fallback_policy: FallbackPolicy::Warn,
        secret_detection: SecretDetectionConfig::default(),
        dependency_scan: DependencyScanConfig {
            enabled: true,
            severity_threshold: SeverityThreshold::Low,
            on_finding: OnFinding::Warn,
        },
        static_analysis: StaticAnalysisConfig::default(),
        max_scroll_depth: None,
    };

    let merged = corp.merge(&project);

    // Verify max() for thresholds
    assert_eq!(
        merged.dependency_scan.severity_threshold,
        SeverityThreshold::Medium
    );

    // Verify union() for tool lists
    assert_eq!(merged.required_tools.len(), 2);
    assert!(merged.required_tools.contains(&"gitleaks".to_string()));
    assert!(merged.required_tools.contains(&"trivy".to_string()));

    // Verify stricter policy wins
    assert_eq!(merged.security_level, SecurityLevel::Standard);
    assert_eq!(merged.fallback_policy, FallbackPolicy::HardStop);
}
