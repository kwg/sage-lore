//! Unit tests for the security primitives.
//!
//! Tests cover:
//! - Policy loading and validation
//! - Security floor enforcement
//! - Builtin secret detection patterns
//! - ScanResult/Finding construction
//! - Content hash computation

use sage_lore::config::{
    DependencyScanConfig, FallbackPolicy, OnExisting, OnFinding, Policy, SecretDetectionConfig,
    SecurityError, SecurityFloor, SecurityLevel, SeverityThreshold, StaticAnalysisConfig,
};
use sage_lore::primitives::secure::{
    compute_content_hash, AuditReport, CveEntry, CveReport, Finding, FindingType, Location,
    SastReport, ScanResult, ScanType, Severity, ToolStatus,
};
use std::fs;
use tempfile::TempDir;

// ============================================================================
// Policy Loading Tests
// ============================================================================

mod policy_loading {
    use super::*;

    #[tokio::test]
    async fn test_policy_load_minimal() {
        let dir = TempDir::new().unwrap();
        let policy_path = dir.path().join("policy.yaml");
        fs::write(&policy_path, "security_level: standard\n").unwrap();

        let policy = Policy::load(&policy_path).unwrap();
        assert_eq!(policy.security_level, SecurityLevel::Standard);
        assert!(policy.required_tools.is_empty());
        assert_eq!(policy.fallback_policy, FallbackPolicy::HardStop);
    }

    #[tokio::test]
    async fn test_policy_load_full() {
        let dir = TempDir::new().unwrap();
        let policy_path = dir.path().join("policy.yaml");
        fs::write(
            &policy_path,
            r#"
security_level: paranoid
required_tools:
  - gitleaks
  - trivy
fallback_policy: warn
secret_detection:
  on_finding: warn
  on_existing: ignore
dependency_scan:
  enabled: true
  severity_threshold: CRITICAL
  on_finding: abort_and_reset
static_analysis:
  enabled: true
  ruleset: p/security-audit
  on_finding: prompt_user
"#,
        )
        .unwrap();

        let policy = Policy::load(&policy_path).unwrap();
        assert_eq!(policy.security_level, SecurityLevel::Paranoid);
        assert_eq!(policy.required_tools, vec!["gitleaks", "trivy"]);
        assert_eq!(policy.fallback_policy, FallbackPolicy::Warn);
        assert_eq!(policy.secret_detection.on_finding, OnFinding::Warn);
        assert_eq!(policy.secret_detection.on_existing, OnExisting::Ignore);
        assert!(policy.dependency_scan.enabled);
        assert_eq!(
            policy.dependency_scan.severity_threshold,
            SeverityThreshold::Critical
        );
        assert!(policy.static_analysis.enabled);
        assert_eq!(policy.static_analysis.ruleset, "p/security-audit");
    }

    #[tokio::test]
    async fn test_policy_load_not_found() {
        let result = Policy::load(std::path::Path::new("/nonexistent/policy.yaml"));
        assert!(matches!(result, Err(SecurityError::PolicyNotFound(_))));
    }

    #[tokio::test]
    async fn test_policy_load_invalid_yaml() {
        let dir = TempDir::new().unwrap();
        let policy_path = dir.path().join("policy.yaml");
        fs::write(&policy_path, "not: valid: yaml: content:").unwrap();

        let result = Policy::load(&policy_path);
        assert!(matches!(result, Err(SecurityError::PolicyViolation(_))));
    }

    #[tokio::test]
    async fn test_policy_from_project() {
        let dir = TempDir::new().unwrap();
        let sage_security = dir.path().join(".sage-lore/security");
        fs::create_dir_all(&sage_security).unwrap();
        fs::write(
            sage_security.join("policy.yaml"),
            "security_level: relaxed\n",
        )
        .unwrap();

        let policy = Policy::load_from_project(dir.path()).unwrap();
        assert_eq!(policy.security_level, SecurityLevel::Relaxed);
    }

    #[tokio::test]
    async fn test_policy_security_levels() {
        let dir = TempDir::new().unwrap();

        for level in ["relaxed", "standard", "paranoid"] {
            let policy_path = dir.path().join(format!("{}.yaml", level));
            fs::write(&policy_path, format!("security_level: {}\n", level)).unwrap();
            let policy = Policy::load(&policy_path).unwrap();
            let expected = match level {
                "relaxed" => SecurityLevel::Relaxed,
                "standard" => SecurityLevel::Standard,
                "paranoid" => SecurityLevel::Paranoid,
                _ => unreachable!(),
            };
            assert_eq!(policy.security_level, expected);
        }
    }

    #[tokio::test]
    async fn test_policy_default_values() {
        let dir = TempDir::new().unwrap();
        let policy_path = dir.path().join("policy.yaml");
        fs::write(&policy_path, "{}\n").unwrap();

        let policy = Policy::load(&policy_path).unwrap();
        // All fields should have defaults
        assert_eq!(policy.security_level, SecurityLevel::Standard);
        assert!(policy.required_tools.is_empty());
        assert_eq!(policy.fallback_policy, FallbackPolicy::HardStop);
        assert_eq!(
            policy.secret_detection.on_finding,
            OnFinding::AbortAndReset
        );
        assert_eq!(policy.secret_detection.on_existing, OnExisting::Block);
        assert!(policy.dependency_scan.enabled);
        assert!(!policy.static_analysis.enabled);
    }
}

// ============================================================================
// Security Floor Enforcement Tests
// ============================================================================

mod security_floor {
    use super::*;

    #[tokio::test]
    async fn test_security_level_ordering() {
        assert!(SecurityLevel::Relaxed < SecurityLevel::Standard);
        assert!(SecurityLevel::Standard < SecurityLevel::Paranoid);
        assert!(SecurityLevel::Relaxed < SecurityLevel::Paranoid);

        // Equality
        assert_eq!(SecurityLevel::Standard, SecurityLevel::Standard);
    }

    #[tokio::test]
    async fn test_validate_against_floor_passes() {
        let policy = Policy {
            security_level: SecurityLevel::Paranoid,
            required_tools: vec![],
            fallback_policy: FallbackPolicy::HardStop,
            secret_detection: SecretDetectionConfig::default(),
            dependency_scan: DependencyScanConfig::default(),
            static_analysis: StaticAnalysisConfig::default(),
            max_scroll_depth: None,
        };

        let floor = SecurityFloor {
            minimum_security_level: SecurityLevel::Standard,
            minimum_required_tools: vec![],
        };

        assert!(policy.validate_against_floor(&floor).is_ok());
    }

    #[tokio::test]
    async fn test_validate_against_floor_exact_match() {
        let policy = Policy {
            security_level: SecurityLevel::Standard,
            required_tools: vec![],
            fallback_policy: FallbackPolicy::HardStop,
            secret_detection: SecretDetectionConfig::default(),
            dependency_scan: DependencyScanConfig::default(),
            static_analysis: StaticAnalysisConfig::default(),
            max_scroll_depth: None,
        };

        let floor = SecurityFloor {
            minimum_security_level: SecurityLevel::Standard,
            minimum_required_tools: vec![],
        };

        assert!(policy.validate_against_floor(&floor).is_ok());
    }

    #[tokio::test]
    async fn test_validate_against_floor_fails() {
        let policy = Policy {
            security_level: SecurityLevel::Relaxed,
            required_tools: vec![],
            fallback_policy: FallbackPolicy::HardStop,
            secret_detection: SecretDetectionConfig::default(),
            dependency_scan: DependencyScanConfig::default(),
            static_analysis: StaticAnalysisConfig::default(),
            max_scroll_depth: None,
        };

        let floor = SecurityFloor {
            minimum_security_level: SecurityLevel::Standard,
            minimum_required_tools: vec![],
        };

        let result = policy.validate_against_floor(&floor);
        assert!(matches!(
            result,
            Err(SecurityError::BelowFloor {
                project: SecurityLevel::Relaxed,
                floor: SecurityLevel::Standard
            })
        ));
    }

    #[tokio::test]
    async fn test_security_floor_parsing() {
        let yaml = r#"
minimum_security_level: paranoid
minimum_required_tools:
  - gitleaks
  - trivy
"#;
        let floor: SecurityFloor = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(floor.minimum_security_level, SecurityLevel::Paranoid);
        assert_eq!(floor.minimum_required_tools, vec!["gitleaks", "trivy"]);
    }

    #[tokio::test]
    async fn test_security_floor_minimal() {
        let yaml = "minimum_security_level: relaxed\n";
        let floor: SecurityFloor = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(floor.minimum_security_level, SecurityLevel::Relaxed);
        assert!(floor.minimum_required_tools.is_empty());
    }
}

// ============================================================================
// Builtin Secret Detection Pattern Tests
// ============================================================================

mod builtin_secret_detection {
    // Helper to check if content is detected as a secret
    fn check_pattern_matches(content: &str, expected_rule_substring: &str) -> bool {
        use regex::Regex;

        // These are the patterns from PolicyDrivenBackend::builtin_secret_detection
        let patterns: &[(&str, &str)] = &[
            (r#"(?i)api[_-]?key\s*[=:]\s*['"]?[\w-]{20,}"#, "API key"),
            (
                r#"(?i)password\s*[=:]\s*['"]?[^\s'"]{8,}"#,
                "Hardcoded password",
            ),
            (r"ghp_[a-zA-Z0-9]{36}", "GitHub personal access token"),
            (r"gho_[a-zA-Z0-9]{36}", "GitHub OAuth token"),
            (r"ghu_[a-zA-Z0-9]{36}", "GitHub user token"),
            (r"ghs_[a-zA-Z0-9]{36}", "GitHub server token"),
            (r"ghr_[a-zA-Z0-9]{36}", "GitHub refresh token"),
            (r"sk-[a-zA-Z0-9]{48}", "OpenAI API key"),
            (r"sk-proj-[a-zA-Z0-9]{48}", "OpenAI project API key"),
            (r"AKIA[0-9A-Z]{16}", "AWS access key ID"),
            (
                r#"(?i)aws[_-]?secret[_-]?access[_-]?key\s*[=:]\s*['"]?[A-Za-z0-9/+=]{40}"#,
                "AWS secret access key",
            ),
            (r"xox[baprs]-[0-9a-zA-Z-]{10,}", "Slack token"),
            (
                r#"(?i)stripe[_-]?(?:secret|api)[_-]?key\s*[=:]\s*['"]?sk_(?:live|test)_[a-zA-Z0-9]{24,}"#,
                "Stripe API key",
            ),
            (r"sk_live_[a-zA-Z0-9]{24,}", "Stripe live secret key"),
            (r"sk_test_[a-zA-Z0-9]{24,}", "Stripe test secret key"),
            (
                r"-----BEGIN (?:RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----",
                "Private key",
            ),
            (
                r#"(?i)(?:heroku|artifactory|npm|nuget|pypi)[_-]?(?:api[_-]?)?(?:key|token)\s*[=:]\s*['"]?[a-zA-Z0-9-]{20,}"#,
                "Service API key/token",
            ),
        ];

        for (pattern, name) in patterns {
            if let Ok(re) = Regex::new(pattern) {
                if re.is_match(content) && name.to_lowercase().contains(expected_rule_substring) {
                    return true;
                }
            }
        }
        false
    }

    #[tokio::test]
    async fn test_github_token_detection() {
        // GitHub personal access token (ghp_)
        let content = "token = ghp_abcdefghijklmnopqrstuvwxyz0123456789";
        assert!(check_pattern_matches(content, "github"));
    }

    #[tokio::test]
    async fn test_github_oauth_token_detection() {
        let content = "oauth = gho_abcdefghijklmnopqrstuvwxyz0123456789";
        assert!(check_pattern_matches(content, "github"));
    }

    #[tokio::test]
    async fn test_openai_key_detection() {
        // OpenAI keys are sk- followed by 48 alphanumeric characters
        let content =
            "OPENAI_API_KEY=sk-abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVW";
        assert!(check_pattern_matches(content, "openai"));
    }

    #[tokio::test]
    async fn test_aws_access_key_detection() {
        let content = "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE";
        assert!(check_pattern_matches(content, "aws"));
    }

    #[tokio::test]
    async fn test_slack_token_detection() {
        let content = "SLACK_TOKEN=xoxb-123456789012-1234567890123-abcdefghijklmnop";
        assert!(check_pattern_matches(content, "slack"));
    }

    #[tokio::test]
    async fn test_stripe_live_key_detection() {
        let content = "STRIPE_KEY=sk_live_51abcdefghijklmnopqrstuvwx";
        assert!(check_pattern_matches(content, "stripe"));
    }

    #[tokio::test]
    async fn test_stripe_test_key_detection() {
        let content = "STRIPE_KEY=sk_test_51abcdefghijklmnopqrstuvwx";
        assert!(check_pattern_matches(content, "stripe"));
    }

    #[tokio::test]
    async fn test_private_key_detection() {
        let content = "-----BEGIN RSA PRIVATE KEY-----";
        assert!(check_pattern_matches(content, "private key"));
    }

    #[tokio::test]
    async fn test_ec_private_key_detection() {
        let content = "-----BEGIN EC PRIVATE KEY-----";
        assert!(check_pattern_matches(content, "private key"));
    }

    #[tokio::test]
    async fn test_openssh_private_key_detection() {
        let content = "-----BEGIN OPENSSH PRIVATE KEY-----";
        assert!(check_pattern_matches(content, "private key"));
    }

    #[tokio::test]
    async fn test_api_key_detection() {
        let content = "API_KEY = 'abcdefghij1234567890abcdefghij'";
        assert!(check_pattern_matches(content, "api key"));
    }

    #[tokio::test]
    async fn test_api_key_colon_format() {
        let content = "api-key: abcdefghij1234567890abcdefghij";
        assert!(check_pattern_matches(content, "api key"));
    }

    #[tokio::test]
    async fn test_password_detection() {
        let content = "PASSWORD = 'supersecretpassword123'";
        assert!(check_pattern_matches(content, "password"));
    }

    #[tokio::test]
    async fn test_no_false_positive_short_password() {
        // Passwords shorter than 8 chars shouldn't match
        let content = "password = 'short'";
        assert!(!check_pattern_matches(content, "password"));
    }

    #[tokio::test]
    async fn test_no_false_positive_normal_code() {
        let content = r#"
fn main() {
    let x = 42;
    println!("Hello, world!");
}
"#;
        // None of our patterns should match normal code
        use regex::Regex;
        let patterns = &[
            r"ghp_[a-zA-Z0-9]{36}",
            r"sk-[a-zA-Z0-9]{48}",
            r"AKIA[0-9A-Z]{16}",
        ];
        for pattern in patterns {
            let re = Regex::new(pattern).unwrap();
            assert!(!re.is_match(content));
        }
    }

    #[tokio::test]
    async fn test_heroku_token_detection() {
        let content = "HEROKU_API_KEY = 'abcdefghij1234567890abcdef'";
        assert!(check_pattern_matches(content, "service"));
    }

    #[tokio::test]
    async fn test_npm_token_detection() {
        let content = "npm_token = 'abcdefghij1234567890abcdef'";
        assert!(check_pattern_matches(content, "service"));
    }
}

// ============================================================================
// ScanResult and Finding Construction Tests
// ============================================================================

mod scan_result_finding {
    use super::*;

    #[tokio::test]
    async fn test_scan_result_passed() {
        let result = ScanResult::passed("gitleaks", ScanType::SecretDetection, 100);
        assert!(result.passed);
        assert!(result.findings.is_empty());
        assert_eq!(result.tool_used, "gitleaks");
        assert_eq!(result.scan_type, ScanType::SecretDetection);
        assert_eq!(result.duration_ms, 100);
    }

    #[tokio::test]
    async fn test_scan_result_has_findings() {
        let mut result = ScanResult::default();
        assert!(!result.has_findings());

        result.findings.push(Finding {
            severity: Severity::Low,
            finding_type: FindingType::Secret,
            location: Location::default(),
            description: "Test".to_string(),
            remediation: String::new(),
            rule_id: "test".to_string(),
            cve_id: None,
            content_hash: None,
        });
        assert!(result.has_findings());
    }

    #[tokio::test]
    async fn test_scan_result_has_blockers() {
        let mut result = ScanResult::default();
        assert!(!result.has_blockers());

        // Add a low severity finding - not a blocker
        result.findings.push(Finding {
            severity: Severity::Low,
            finding_type: FindingType::Secret,
            location: Location::default(),
            description: "Test".to_string(),
            remediation: String::new(),
            rule_id: "test".to_string(),
            cve_id: None,
            content_hash: None,
        });
        assert!(!result.has_blockers());

        // Add a high severity finding - is a blocker
        result.findings.push(Finding {
            severity: Severity::High,
            finding_type: FindingType::Secret,
            location: Location::default(),
            description: "Test".to_string(),
            remediation: String::new(),
            rule_id: "test".to_string(),
            cve_id: None,
            content_hash: None,
        });
        assert!(result.has_blockers());
    }

    #[tokio::test]
    async fn test_scan_result_has_blockers_critical() {
        let mut result = ScanResult::default();
        result.findings.push(Finding {
            severity: Severity::Critical,
            finding_type: FindingType::Cve,
            location: Location::default(),
            description: "CVE-2024-1234".to_string(),
            remediation: "Upgrade".to_string(),
            rule_id: "cve".to_string(),
            cve_id: Some("CVE-2024-1234".to_string()),
            content_hash: None,
        });
        assert!(result.has_blockers());
    }

    #[tokio::test]
    async fn test_scan_result_count_by_severity() {
        let mut result = ScanResult::default();
        result.findings.push(Finding {
            severity: Severity::High,
            finding_type: FindingType::Secret,
            location: Location::default(),
            description: "Test".to_string(),
            remediation: String::new(),
            rule_id: "test".to_string(),
            cve_id: None,
            content_hash: None,
        });
        result.findings.push(Finding {
            severity: Severity::High,
            finding_type: FindingType::Secret,
            location: Location::default(),
            description: "Test 2".to_string(),
            remediation: String::new(),
            rule_id: "test2".to_string(),
            cve_id: None,
            content_hash: None,
        });
        result.findings.push(Finding {
            severity: Severity::Low,
            finding_type: FindingType::Secret,
            location: Location::default(),
            description: "Test 3".to_string(),
            remediation: String::new(),
            rule_id: "test3".to_string(),
            cve_id: None,
            content_hash: None,
        });

        assert_eq!(result.count_by_severity(Severity::High), 2);
        assert_eq!(result.count_by_severity(Severity::Low), 1);
        assert_eq!(result.count_by_severity(Severity::Critical), 0);
    }

    #[tokio::test]
    async fn test_finding_is_blocker() {
        let low = Finding {
            severity: Severity::Low,
            finding_type: FindingType::Secret,
            location: Location::default(),
            description: String::new(),
            remediation: String::new(),
            rule_id: String::new(),
            cve_id: None,
            content_hash: None,
        };
        assert!(!low.is_blocker());

        let medium = Finding {
            severity: Severity::Medium,
            ..low.clone()
        };
        assert!(!medium.is_blocker());

        let high = Finding {
            severity: Severity::High,
            ..low.clone()
        };
        assert!(high.is_blocker());

        let critical = Finding {
            severity: Severity::Critical,
            ..low.clone()
        };
        assert!(critical.is_blocker());
    }

    #[tokio::test]
    async fn test_finding_is_secret() {
        let secret = Finding {
            severity: Severity::High,
            finding_type: FindingType::Secret,
            location: Location::default(),
            description: String::new(),
            remediation: String::new(),
            rule_id: String::new(),
            cve_id: None,
            content_hash: None,
        };
        assert!(secret.is_secret());

        let cve = Finding {
            finding_type: FindingType::Cve,
            ..secret.clone()
        };
        assert!(!cve.is_secret());

        let vuln = Finding {
            finding_type: FindingType::Vulnerability,
            ..secret.clone()
        };
        assert!(!vuln.is_secret());
    }

    #[tokio::test]
    async fn test_severity_ordering() {
        assert!(Severity::Critical < Severity::High);
        assert!(Severity::High < Severity::Medium);
        assert!(Severity::Medium < Severity::Low);
        assert!(Severity::Low < Severity::Info);
    }

    #[tokio::test]
    async fn test_severity_display() {
        assert_eq!(Severity::Critical.to_string(), "CRITICAL");
        assert_eq!(Severity::High.to_string(), "HIGH");
        assert_eq!(Severity::Medium.to_string(), "MEDIUM");
        assert_eq!(Severity::Low.to_string(), "LOW");
        assert_eq!(Severity::Info.to_string(), "INFO");
    }

    #[tokio::test]
    async fn test_finding_type_display() {
        assert_eq!(FindingType::Secret.to_string(), "Secret");
        assert_eq!(FindingType::Cve.to_string(), "CVE");
        assert_eq!(FindingType::Vulnerability.to_string(), "Vulnerability");
        assert_eq!(FindingType::PolicyViolation.to_string(), "Policy Violation");
    }

    #[tokio::test]
    async fn test_scan_type_display() {
        assert_eq!(ScanType::SecretDetection.to_string(), "Secret Detection");
        assert_eq!(ScanType::DependencyCve.to_string(), "Dependency CVE");
        assert_eq!(ScanType::StaticAnalysis.to_string(), "Static Analysis");
        assert_eq!(ScanType::Allowlist.to_string(), "Allowlist");
        assert_eq!(ScanType::SensitivePaths.to_string(), "Sensitive Paths");
    }
}

// ============================================================================
// Location Tests
// ============================================================================

mod location {
    use super::*;

    #[tokio::test]
    async fn test_location_file() {
        let loc = Location::file("src/main.rs");
        assert_eq!(loc.file, "src/main.rs");
        assert!(loc.line.is_none());
        assert!(loc.column.is_none());
        assert!(loc.snippet.is_none());
    }

    #[tokio::test]
    async fn test_location_file_line() {
        let loc = Location::file_line("src/main.rs", 42);
        assert_eq!(loc.file, "src/main.rs");
        assert_eq!(loc.line, Some(42));
        assert!(loc.column.is_none());
        assert!(loc.snippet.is_none());
    }

    #[tokio::test]
    async fn test_location_display_file_only() {
        let loc = Location::file("src/main.rs");
        assert_eq!(loc.to_string(), "src/main.rs");
    }

    #[tokio::test]
    async fn test_location_display_file_line() {
        let loc = Location::file_line("src/main.rs", 42);
        assert_eq!(loc.to_string(), "src/main.rs:42");
    }

    #[tokio::test]
    async fn test_location_display_full() {
        let loc = Location {
            file: "src/main.rs".to_string(),
            line: Some(42),
            column: Some(10),
            snippet: Some("let x = 1;".to_string()),
        };
        assert_eq!(loc.to_string(), "src/main.rs:42:10");
    }
}

// ============================================================================
// Content Hash Computation Tests
// ============================================================================

mod content_hash {
    use super::*;

    #[tokio::test]
    async fn test_compute_content_hash_format() {
        let hash = compute_content_hash("test content");
        assert!(hash.starts_with("sha256:"));
        // SHA256 produces 64 hex characters
        assert_eq!(hash.len(), 7 + 64); // "sha256:" + 64 hex chars
    }

    #[tokio::test]
    async fn test_compute_content_hash_consistency() {
        let hash1 = compute_content_hash("test content");
        let hash2 = compute_content_hash("test content");
        assert_eq!(hash1, hash2);
    }

    #[tokio::test]
    async fn test_compute_content_hash_different_content() {
        let hash1 = compute_content_hash("content A");
        let hash2 = compute_content_hash("content B");
        assert_ne!(hash1, hash2);
    }

    #[tokio::test]
    async fn test_compute_content_hash_empty_string() {
        let hash = compute_content_hash("");
        assert!(hash.starts_with("sha256:"));
        // SHA256 of empty string is a well-known value
        assert_eq!(
            hash,
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[tokio::test]
    async fn test_compute_content_hash_known_value() {
        // "hello" has a known SHA256 hash
        let hash = compute_content_hash("hello");
        assert_eq!(
            hash,
            "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[tokio::test]
    async fn test_compute_content_hash_special_characters() {
        let hash = compute_content_hash("ghp_abc123!@#$%^&*()");
        assert!(hash.starts_with("sha256:"));
        assert_eq!(hash.len(), 7 + 64);
    }

    #[tokio::test]
    async fn test_compute_content_hash_unicode() {
        let hash = compute_content_hash("こんにちは");
        assert!(hash.starts_with("sha256:"));
        assert_eq!(hash.len(), 7 + 64);
    }
}

// ============================================================================
// Audit Report Tests
// ============================================================================

mod audit_report {
    use super::*;

    #[tokio::test]
    async fn test_audit_report_from_scans_empty() {
        let report = AuditReport::from_scans(vec![], "abc123");
        assert!(report.passed);
        assert_eq!(report.total_findings, 0);
        assert_eq!(report.blockers, 0);
        assert_eq!(report.warnings, 0);
        assert_eq!(report.commit_scanned, "abc123");
    }

    #[tokio::test]
    async fn test_audit_report_from_scans_with_blockers() {
        let scan = ScanResult {
            passed: false,
            findings: vec![
                Finding {
                    severity: Severity::High,
                    finding_type: FindingType::Secret,
                    location: Location::default(),
                    description: "Secret".to_string(),
                    remediation: String::new(),
                    rule_id: "secret".to_string(),
                    cve_id: None,
                    content_hash: None,
                },
                Finding {
                    severity: Severity::Low,
                    finding_type: FindingType::Vulnerability,
                    location: Location::default(),
                    description: "Warning".to_string(),
                    remediation: String::new(),
                    rule_id: "vuln".to_string(),
                    cve_id: None,
                    content_hash: None,
                },
            ],
            tool_used: "test".to_string(),
            scan_type: ScanType::SecretDetection,
            duration_ms: 100,
        };

        let report = AuditReport::from_scans(vec![scan], "def456");
        assert!(!report.passed);
        assert_eq!(report.total_findings, 2);
        assert_eq!(report.blockers, 1);
        assert_eq!(report.warnings, 1);
    }

    #[tokio::test]
    async fn test_audit_report_all_findings() {
        let scan1 = ScanResult {
            passed: true,
            findings: vec![Finding {
                severity: Severity::Low,
                finding_type: FindingType::Vulnerability,
                location: Location::default(),
                description: "Finding 1".to_string(),
                remediation: String::new(),
                rule_id: "rule1".to_string(),
                cve_id: None,
                content_hash: None,
            }],
            tool_used: "tool1".to_string(),
            scan_type: ScanType::StaticAnalysis,
            duration_ms: 50,
        };
        let scan2 = ScanResult {
            passed: true,
            findings: vec![Finding {
                severity: Severity::Medium,
                finding_type: FindingType::Vulnerability,
                location: Location::default(),
                description: "Finding 2".to_string(),
                remediation: String::new(),
                rule_id: "rule2".to_string(),
                cve_id: None,
                content_hash: None,
            }],
            tool_used: "tool2".to_string(),
            scan_type: ScanType::StaticAnalysis,
            duration_ms: 50,
        };

        let report = AuditReport::from_scans(vec![scan1, scan2], "commit");
        let all_findings: Vec<_> = report.all_findings().collect();
        assert_eq!(all_findings.len(), 2);
    }

    #[tokio::test]
    async fn test_audit_report_blocking_findings() {
        let scan = ScanResult {
            passed: false,
            findings: vec![
                Finding {
                    severity: Severity::Critical,
                    finding_type: FindingType::Cve,
                    location: Location::default(),
                    description: "Critical CVE".to_string(),
                    remediation: String::new(),
                    rule_id: "cve".to_string(),
                    cve_id: Some("CVE-2024-0001".to_string()),
                    content_hash: None,
                },
                Finding {
                    severity: Severity::Low,
                    finding_type: FindingType::Vulnerability,
                    location: Location::default(),
                    description: "Low vuln".to_string(),
                    remediation: String::new(),
                    rule_id: "vuln".to_string(),
                    cve_id: None,
                    content_hash: None,
                },
            ],
            tool_used: "trivy".to_string(),
            scan_type: ScanType::DependencyCve,
            duration_ms: 200,
        };

        let report = AuditReport::from_scans(vec![scan], "commit");
        let blocking: Vec<_> = report.blocking_findings().collect();
        assert_eq!(blocking.len(), 1);
        assert_eq!(blocking[0].description, "Critical CVE");
    }
}

// ============================================================================
// CVE Report Tests
// ============================================================================

mod cve_report {
    use super::*;

    #[tokio::test]
    async fn test_cve_report_default() {
        let report = CveReport::default();
        assert!(report.passed);
        assert!(report.vulnerabilities.is_empty());
        assert_eq!(report.packages_scanned, 0);
    }

    #[tokio::test]
    async fn test_cve_report_has_critical() {
        let mut report = CveReport::default();
        assert!(!report.has_critical());

        report.vulnerabilities.push(CveEntry {
            cve_id: "CVE-2024-1234".to_string(),
            severity: Severity::High,
            package: "some-package".to_string(),
            version: "1.0.0".to_string(),
            fixed_version: Some("1.0.1".to_string()),
            description: "High severity vuln".to_string(),
            url: "https://example.com".to_string(),
        });
        assert!(!report.has_critical());

        report.vulnerabilities.push(CveEntry {
            cve_id: "CVE-2024-5678".to_string(),
            severity: Severity::Critical,
            package: "another-package".to_string(),
            version: "2.0.0".to_string(),
            fixed_version: None,
            description: "Critical vuln".to_string(),
            url: "https://example.com".to_string(),
        });
        assert!(report.has_critical());
    }

    #[tokio::test]
    async fn test_cve_report_count_by_severity() {
        let report = CveReport {
            passed: false,
            vulnerabilities: vec![
                CveEntry {
                    cve_id: "CVE-1".to_string(),
                    severity: Severity::Critical,
                    package: "pkg".to_string(),
                    version: "1.0".to_string(),
                    fixed_version: None,
                    description: String::new(),
                    url: String::new(),
                },
                CveEntry {
                    cve_id: "CVE-2".to_string(),
                    severity: Severity::High,
                    package: "pkg".to_string(),
                    version: "1.0".to_string(),
                    fixed_version: None,
                    description: String::new(),
                    url: String::new(),
                },
                CveEntry {
                    cve_id: "CVE-3".to_string(),
                    severity: Severity::High,
                    package: "pkg".to_string(),
                    version: "1.0".to_string(),
                    fixed_version: None,
                    description: String::new(),
                    url: String::new(),
                },
            ],
            packages_scanned: 10,
            tool_used: "trivy".to_string(),
        };

        assert_eq!(report.count_by_severity(Severity::Critical), 1);
        assert_eq!(report.count_by_severity(Severity::High), 2);
        assert_eq!(report.count_by_severity(Severity::Medium), 0);
    }

    #[tokio::test]
    async fn test_cve_entry_has_fix() {
        let with_fix = CveEntry {
            cve_id: "CVE-1".to_string(),
            severity: Severity::High,
            package: "pkg".to_string(),
            version: "1.0".to_string(),
            fixed_version: Some("1.1".to_string()),
            description: String::new(),
            url: String::new(),
        };
        assert!(with_fix.has_fix());

        let without_fix = CveEntry {
            fixed_version: None,
            ..with_fix.clone()
        };
        assert!(!without_fix.has_fix());
    }

    #[tokio::test]
    async fn test_cve_entry_is_blocker() {
        let critical = CveEntry {
            cve_id: "CVE-1".to_string(),
            severity: Severity::Critical,
            package: "pkg".to_string(),
            version: "1.0".to_string(),
            fixed_version: None,
            description: String::new(),
            url: String::new(),
        };
        assert!(critical.is_blocker());

        let high = CveEntry {
            severity: Severity::High,
            ..critical.clone()
        };
        assert!(high.is_blocker());

        let medium = CveEntry {
            severity: Severity::Medium,
            ..critical.clone()
        };
        assert!(!medium.is_blocker());
    }
}

// ============================================================================
// SAST Report Tests
// ============================================================================

mod sast_report {
    use super::*;

    #[tokio::test]
    async fn test_sast_report_default() {
        let report = SastReport::default();
        assert!(report.passed);
        assert!(report.findings.is_empty());
        assert_eq!(report.files_scanned, 0);
        assert_eq!(report.rules_applied, 0);
    }

    #[tokio::test]
    async fn test_sast_report_has_blockers() {
        let mut report = SastReport::default();
        assert!(!report.has_blockers());

        report.findings.push(Finding {
            severity: Severity::Medium,
            finding_type: FindingType::Vulnerability,
            location: Location::default(),
            description: String::new(),
            remediation: String::new(),
            rule_id: String::new(),
            cve_id: None,
            content_hash: None,
        });
        assert!(!report.has_blockers());

        report.findings.push(Finding {
            severity: Severity::High,
            finding_type: FindingType::Vulnerability,
            location: Location::default(),
            description: String::new(),
            remediation: String::new(),
            rule_id: String::new(),
            cve_id: None,
            content_hash: None,
        });
        assert!(report.has_blockers());
    }

    #[tokio::test]
    async fn test_sast_report_count_by_severity() {
        let report = SastReport {
            passed: true,
            findings: vec![
                Finding {
                    severity: Severity::Medium,
                    finding_type: FindingType::Vulnerability,
                    location: Location::default(),
                    description: String::new(),
                    remediation: String::new(),
                    rule_id: String::new(),
                    cve_id: None,
                    content_hash: None,
                },
                Finding {
                    severity: Severity::Medium,
                    finding_type: FindingType::Vulnerability,
                    location: Location::default(),
                    description: String::new(),
                    remediation: String::new(),
                    rule_id: String::new(),
                    cve_id: None,
                    content_hash: None,
                },
                Finding {
                    severity: Severity::Low,
                    finding_type: FindingType::Vulnerability,
                    location: Location::default(),
                    description: String::new(),
                    remediation: String::new(),
                    rule_id: String::new(),
                    cve_id: None,
                    content_hash: None,
                },
            ],
            files_scanned: 100,
            rules_applied: 50,
            tool_used: "semgrep".to_string(),
        };

        assert_eq!(report.count_by_severity(Severity::Medium), 2);
        assert_eq!(report.count_by_severity(Severity::Low), 1);
        assert_eq!(report.count_by_severity(Severity::High), 0);
    }
}

// ============================================================================
// Tool Status Tests
// ============================================================================

mod tool_status {
    use super::*;

    #[tokio::test]
    async fn test_tool_status_unavailable() {
        let status = ToolStatus::unavailable("nonexistent-tool");
        assert_eq!(status.name, "nonexistent-tool");
        assert!(!status.available);
        assert!(status.version.is_none());
        assert!(status.path.is_none());
    }

    #[tokio::test]
    async fn test_tool_status_detect_nonexistent() {
        let status = ToolStatus::detect("this-tool-definitely-does-not-exist-12345");
        assert!(!status.available);
        assert!(status.version.is_none());
        assert!(status.path.is_none());
    }

    // Note: We can't reliably test tool detection for real tools in unit tests
    // because they may or may not be installed on the system.
}

// ============================================================================
// Allowlist Tests
// ============================================================================

mod allowlist {
    use sage_lore::config::Allowlist;

    #[tokio::test]
    async fn test_allowlist_is_allowed_by_hash() {
        use chrono::Utc;
        use sage_lore::config::AllowlistInstance;

        let allowlist = Allowlist {
            instances: vec![AllowlistInstance {
                hash: "sha256:abc123".to_string(),
                file: "test.rs".to_string(),
                line: 10,
                rule: "test-rule".to_string(),
                reason: "Test".to_string(),
                added_by: "tester".to_string(),
                added_at: Utc::now(),
            }],
            patterns: vec![],
        };

        assert!(allowlist.is_allowed("test.rs", "test-rule", Some("sha256:abc123")));
        assert!(!allowlist.is_allowed("test.rs", "test-rule", Some("sha256:different")));
        assert!(!allowlist.is_allowed("test.rs", "different-rule", Some("sha256:abc123")));
    }

    #[tokio::test]
    async fn test_allowlist_is_allowed_by_pattern() {
        use sage_lore::config::AllowlistPattern;

        let allowlist = Allowlist {
            instances: vec![],
            patterns: vec![AllowlistPattern {
                glob: "tests/fixtures/**".to_string(),
                rules: vec!["generic-api-key".to_string(), "generic-password".to_string()],
                reason: "Test fixtures".to_string(),
            }],
        };

        assert!(allowlist.is_allowed(
            "tests/fixtures/api_mock.rs",
            "generic-api-key",
            None
        ));
        assert!(allowlist.is_allowed(
            "tests/fixtures/deep/nested/file.rs",
            "generic-password",
            None
        ));
        assert!(!allowlist.is_allowed(
            "tests/fixtures/api_mock.rs",
            "other-rule",
            None
        ));
        assert!(!allowlist.is_allowed("src/main.rs", "generic-api-key", None));
    }
}

// ============================================================================
// Security Level Display Tests
// ============================================================================

mod security_level_display {
    use super::*;

    #[tokio::test]
    async fn test_security_level_display() {
        assert_eq!(SecurityLevel::Relaxed.to_string(), "relaxed");
        assert_eq!(SecurityLevel::Standard.to_string(), "standard");
        assert_eq!(SecurityLevel::Paranoid.to_string(), "paranoid");
    }
}
