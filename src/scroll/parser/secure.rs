// SPDX-License-Identifier: MIT
//! Secret detection at parse time.
//!
//! This module provides secret scanning functionality that runs BEFORE parsing
//! to catch hardcoded credentials in scroll YAML.

use crate::primitives::secure::builtin::builtin_secret_detection;
use crate::scroll::error::ParseError;
use regex::Regex;

/// Scan raw YAML content for hardcoded secrets.
///
/// This function checks for secret patterns BEFORE parsing to catch hardcoded
/// credentials. Variable references like `${VAR}` are allowed.
///
/// # Errors
///
/// Returns `ParseError::HardcodedSecret` if a potential secret is detected.
pub fn scan_for_secrets(yaml: &str) -> Result<(), ParseError> {
    // Use the built-in secret detection patterns
    let scan_result = builtin_secret_detection(yaml);

    if !scan_result.passed {
        // Check if there are any findings that aren't variable references
        let var_ref_pattern = Regex::new(r"\$\{[^}]+\}").unwrap();

        for finding in &scan_result.findings {
            // If the snippet contains a variable reference, it's safe
            if let Some(snippet) = &finding.location.snippet {
                if var_ref_pattern.is_match(snippet) {
                    // This is a variable reference like ${API_KEY}, which is safe
                    continue;
                }
            }

            // This is a real hardcoded secret
            return Err(ParseError::HardcodedSecret {
                line: finding.location.line.unwrap_or(0),
                description: finding.description.clone(),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::parse_scroll;

    // ========================================================================
    // Security Steps (ScanType variants)
    // ========================================================================

    #[test]
    fn test_parse_secure_step_dependency_cve() {
        let yaml = r#"
scroll: test
description: Test
steps:
  - secure:
      input: dependencies
      scan_type: dependency_cve
"#;
        let result = parse_scroll(yaml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_secure_step_secret_detection() {
        let yaml = r#"
scroll: test
description: Test
steps:
  - secure:
      input: source
      scan_type: secret_detection
"#;
        let result = parse_scroll(yaml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_secure_step_static_analysis() {
        let yaml = r#"
scroll: test
description: Test
steps:
  - secure:
      input: code
      scan_type: static_analysis
"#;
        let result = parse_scroll(yaml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_secure_step_multiple_scans() {
        let yaml = r#"
scroll: test
description: Test
steps:
  - secure:
      input: all
      scan_type:
        - dependency_cve
        - secret_detection
      policy: block
"#;
        let result = parse_scroll(yaml);
        assert!(result.is_ok());
    }

    // ========================================================================
    // SecurityPolicy Defaults
    // ========================================================================

    #[test]
    fn test_security_policy_warn_is_default() {
        let yaml = r#"
scroll: test
description: Test
steps:
  - secure:
      input: code
      scan_type: static_analysis
"#;
        let scroll = parse_scroll(yaml).unwrap();
        // Default policy should be Warn (no panic on error)
        assert!(scroll.steps.len() == 1);
    }

    #[test]
    fn test_security_policy_block() {
        let yaml = r#"
scroll: test
description: Test
steps:
  - secure:
      input: code
      scan_type: static_analysis
      policy: block
"#;
        let result = parse_scroll(yaml);
        assert!(result.is_ok());
    }

    #[test]
    fn test_security_policy_audit() {
        let yaml = r#"
scroll: test
description: Test
steps:
  - secure:
      input: code
      scan_type: static_analysis
      policy: audit
"#;
        let result = parse_scroll(yaml);
        assert!(result.is_ok());
    }

    // ========================================================================
    // Secret Detection Tests (Story D3c)
    // ========================================================================

    #[test]
    fn test_parse_error_hardcoded_api_key() {
        // The secret scanner runs on raw YAML before parsing.
        // It detects patterns like `api_key: <value>` in the text.
        let yaml = r#"
scroll: bad-scroll
description: "api_key: sk-ant-api03-REAL_KEY_HERE_1234567890abcdef"
steps:
  - invoke:
      agent: claude
      instructions: "Do something"
"#;
        let result = parse_scroll(yaml);
        assert!(result.is_err(), "Should reject hardcoded API key");
        match result.unwrap_err() {
            crate::scroll::error::ParseError::HardcodedSecret { line, description } => {
                assert!(line > 0, "Should report line number");
                assert!(description.contains("API key") || description.contains("OpenAI"),
                    "Should identify as API key, got: {}", description);
            }
            err => panic!("Expected HardcodedSecret error, got: {:?}", err),
        }
    }

    #[test]
    fn test_parse_error_hardcoded_github_token() {
        let yaml = r#"
scroll: bad-scroll
description: Contains hardcoded GitHub token ghp_1234567890abcdefghijklmnopqrstuv123456
steps:
  - invoke:
      agent: test-agent
      instructions: "Do something"
"#;
        let result = parse_scroll(yaml);
        assert!(result.is_err(), "Should reject hardcoded GitHub token");
        match result.unwrap_err() {
            crate::scroll::error::ParseError::HardcodedSecret { .. } => {}
            err => panic!("Expected HardcodedSecret error, got: {:?}", err),
        }
    }

    #[test]
    fn test_parse_error_hardcoded_aws_key() {
        let yaml = r#"
scroll: bad-scroll
description: Contains hardcoded AWS key AKIAIOSFODNN7EXAMPLE
steps:
  - invoke:
      agent: aws-cli
      instructions: "Do something"
"#;
        let result = parse_scroll(yaml);
        assert!(result.is_err(), "Should reject hardcoded AWS key");
        match result.unwrap_err() {
            crate::scroll::error::ParseError::HardcodedSecret { .. } => {}
            err => panic!("Expected HardcodedSecret error, got: {:?}", err),
        }
    }

    #[test]
    fn test_parse_allow_variable_references() {
        let yaml = r#"
scroll: good-scroll
description: Uses variable references (safe)
steps:
  - invoke:
      agent: claude
      instructions: "Process with key ${ANTHROPIC_API_KEY}"
"#;
        let result = parse_scroll(yaml);
        assert!(result.is_ok(), "Should allow variable references: {:?}", result);
    }

    #[test]
    fn test_parse_allow_env_var_syntax() {
        let yaml = r#"
scroll: good-scroll
description: Uses environment variable syntax
steps:
  - invoke:
      agent: test-agent
      instructions: "Task with ${GITHUB_TOKEN} and ${SECRET_KEY}"
"#;
        let result = parse_scroll(yaml);
        assert!(result.is_ok(), "Should allow variable syntax: {:?}", result);
    }

    #[test]
    fn test_parse_error_private_key() {
        let yaml = r#"
scroll: bad-scroll
description: |
  Contains hardcoded private key:
  -----BEGIN RSA PRIVATE KEY-----
  MIIEpAIBAAKCAQEA1234567890
  -----END RSA PRIVATE KEY-----
steps:
  - invoke:
      agent: ssh
      instructions: "Connect"
"#;
        let result = parse_scroll(yaml);
        assert!(result.is_err(), "Should reject hardcoded private key");
        match result.unwrap_err() {
            crate::scroll::error::ParseError::HardcodedSecret { .. } => {}
            err => panic!("Expected HardcodedSecret error, got: {:?}", err),
        }
    }
}
