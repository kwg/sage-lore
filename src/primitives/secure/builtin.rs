// SPDX-License-Identifier: MIT
//! Built-in fallback patterns for secret detection.
use super::types::{Finding, FindingType, Location, ScanResult, ScanType, Severity};
use super::reports::compute_content_hash;
use regex::Regex;
use std::time::Instant;

pub fn builtin_secret_detection(content: &str) -> ScanResult {
    let start = Instant::now();
    let patterns: &[(&str, &str)] = &[
        (r#"(?i)api[_-]?key\s*[=:]\s*['"]?[\w-]{20,}"#, "API key"),
        (r#"(?i)password\s*[=:]\s*['"]?[^\s'"]{8,}"#, "Hardcoded password"),
        (r"ghp_[a-zA-Z0-9]{36}", "GitHub personal access token"),
        (r"gho_[a-zA-Z0-9]{36}", "GitHub OAuth token"),
        (r"ghu_[a-zA-Z0-9]{36}", "GitHub user token"),
        (r"ghs_[a-zA-Z0-9]{36}", "GitHub server token"),
        (r"ghr_[a-zA-Z0-9]{36}", "GitHub refresh token"),
        (r"sk-[a-zA-Z0-9]{48}", "OpenAI API key"),
        (r"sk-proj-[a-zA-Z0-9]{48}", "OpenAI project API key"),
        (r"AKIA[0-9A-Z]{16}", "AWS access key ID"),
        (r#"(?i)aws[_-]?secret[_-]?access[_-]?key\s*[=:]\s*['"]?[A-Za-z0-9/+=]{40}"#, "AWS secret access key"),
        (r"xox[baprs]-[0-9a-zA-Z-]{10,}", "Slack token"),
        (r#"(?i)stripe[_-]?(?:secret|api)[_-]?key\s*[=:]\s*['"]?sk_(?:live|test)_[a-zA-Z0-9]{24,}"#, "Stripe API key"),
        (r"sk_live_[a-zA-Z0-9]{24,}", "Stripe live secret key"),
        (r"sk_test_[a-zA-Z0-9]{24,}", "Stripe test secret key"),
        (r"-----BEGIN (?:RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----", "Private key"),
        (r#"(?i)(?:heroku|artifactory|npm|nuget|pypi)[_-]?(?:api[_-]?)?(?:key|token)\s*[=:]\s*['"]?[a-zA-Z0-9-]{20,}"#, "Service API key/token"),
    ];
    let mut findings = Vec::new();
    for (line_num, line) in content.lines().enumerate() {
        for (pattern, name) in patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(matched) = re.find(line) {
                    let matched_text = matched.as_str().to_string();
                    findings.push(Finding {
                        severity: Severity::High,
                        finding_type: FindingType::Secret,
                        location: Location { file: "content".to_string(), line: Some(line_num + 1), column: Some(matched.start() + 1), snippet: Some(line.to_string()) },
                        description: format!("Potential {} detected", name),
                        remediation: "Remove and rotate the credential".to_string(),
                        rule_id: format!("builtin-{}", name.to_lowercase().replace(' ', "-")),
                        cve_id: None,
                        content_hash: Some(compute_content_hash(&matched_text)),
                    });
                }
            }
        }
    }
    let duration_ms = start.elapsed().as_millis() as u64;
    ScanResult { passed: findings.is_empty(), findings, tool_used: "builtin".to_string(), scan_type: ScanType::SecretDetection, duration_ms }
}
