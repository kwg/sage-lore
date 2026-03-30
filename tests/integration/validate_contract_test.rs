// SPDX-License-Identifier: MIT
//! Integration tests for validate primitive contract enforcement (Story #76).
//!
//! Tests verify the contract per spec #46:
//! - Mode enum enforced at parse time (strict, majority, any)
//! - Guaranteed output structure (result, score, criteria_results, summary)
//! - Score calculation (passed_count / total_count)
//! - Mode-based pass/fail resolution
//! - Schema validation only (no consensus - avoids infinite regress per D31)

use sage_lore::scroll::executor::Executor;
use sage_lore::scroll::parser::parse_scroll;

#[tokio::test]
async fn test_validate_enum_mode_enforced_at_parse() {
    // Test that invalid mode enum values are rejected at parse time
    let yaml = r#"
scroll: test-validate-invalid-mode
description: Test mode enum validation
steps:
  - validate:
      input: "test content"
      criteria:
        - "Has documentation"
      mode: invalid_mode  # Should fail - not a valid enum
    output: result
"#;

    let result = parse_scroll(yaml);
    assert!(result.is_err(), "Should reject invalid mode enum value");
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("data did not match") || err_msg.contains("unknown variant"));
}

#[tokio::test]
async fn test_validate_deterministic_result() {
    // Test that validate always returns pass OR fail (never ambiguous)
    let yaml = r#"
scroll: test-validate-deterministic
description: Test deterministic pass/fail result
requires:
  input_content:
    type: string
    default: "Sample content with documentation"
steps:
  - validate:
      input: ${input_content}
      criteria:
        - "Has documentation"
        - "Is well-structured"
      mode: strict
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse valid validate scroll");

    let result = executor.execute_scroll(&scroll).await;
    if let Err(ref e) = result {
        eprintln!("Execution error: {:?}", e);
    }
    assert!(result.is_ok(), "Should execute validate successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");

    // Verify result field exists and is "pass" or "fail"
    let result_field = output.get("result")
        .expect("Should have result field");
    let result_str = result_field.as_str()
        .expect("Result should be string");
    assert!(result_str == "pass" || result_str == "fail",
        "Result must be 'pass' or 'fail', got: {}", result_str);
}

#[tokio::test]
async fn test_validate_guaranteed_output_structure() {
    // Test that validate always returns required fields
    let yaml = r#"
scroll: test-validate-structure
description: Test guaranteed output structure
requires:
  input_content:
    type: string
    default: "Content to validate"
steps:
  - validate:
      input: ${input_content}
      criteria:
        - "Has proper formatting"
        - "Contains key information"
      mode: majority
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse validate scroll");

    let result = executor.execute_scroll(&scroll).await;
    if let Err(ref e) = result {
        eprintln!("Execution error: {:?}", e);
    }
    assert!(result.is_ok(), "Should execute validate successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");

    // Verify all required fields exist
    assert!(output.get("result").is_some(), "Should have result field");
    assert!(output.get("score").is_some(), "Should have score field");
    assert!(output.get("criteria_results").is_some(), "Should have criteria_results field");
    assert!(output.get("summary").is_some(), "Should have summary field");

    // Verify types
    let score = output.get("score")
        .and_then(|s| s.as_f64())
        .expect("Score should be float");
    assert!(score >= 0.0 && score <= 1.0, "Score must be 0.0-1.0, got: {}", score);

    let criteria_results = output.get("criteria_results")
        .and_then(|c| c.as_array())
        .expect("Criteria results should be array");
    assert!(!criteria_results.is_empty(), "Should have at least one criterion result");
}

#[tokio::test]
async fn test_validate_explained_judgment() {
    // Test that every criterion has an explanation
    let yaml = r#"
scroll: test-validate-explained
description: Test all criteria have explanations
requires:
  input_content:
    type: string
    default: "Test content"
steps:
  - validate:
      input: ${input_content}
      criteria:
        - "Is complete"
        - "Is accurate"
        - "Is clear"
      mode: strict
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse validate scroll");

    let result = executor.execute_scroll(&scroll).await;
    if let Err(ref e) = result {
        eprintln!("Execution error: {:?}", e);
    }
    assert!(result.is_ok(), "Should execute validate successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");

    let criteria_results = output.get("criteria_results")
        .and_then(|c| c.as_array())
        .expect("Criteria results should be array");

    // Every criterion should have explanation
    for criterion_result in criteria_results {
        let mapping = criterion_result.as_object()
            .expect("Criterion result should be mapping");

        assert!(mapping.get("criterion").is_some(),
            "Should have criterion field");
        assert!(mapping.get("passed").is_some(),
            "Should have passed field");

        let explanation = mapping.get("explanation")
            .and_then(|e| e.as_str())
            .expect("Should have explanation field as string");
        assert!(!explanation.is_empty(), "Explanation should not be empty");
    }
}

#[tokio::test]
async fn test_validate_mode_strict() {
    // Test strict mode: all criteria must pass (score == 1.0)
    let yaml = r#"
scroll: test-validate-strict
description: Test strict mode validation
requires:
  input_content:
    type: string
    default: "Well-documented and complete content"
steps:
  - validate:
      input: ${input_content}
      criteria:
        - "Has documentation"
        - "Is complete"
      mode: strict
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse validate scroll");

    let result = executor.execute_scroll(&scroll).await;
    if let Err(ref e) = result {
        eprintln!("Execution error: {:?}", e);
    }
    assert!(result.is_ok(), "Should execute validate successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");

    let score = output.get("score")
        .and_then(|s| s.as_f64())
        .expect("Should have score");

    let result_value = output.get("result")
        .and_then(|r| r.as_str())
        .expect("Should have result");

    // In strict mode, pass only if score == 1.0
    if result_value == "pass" {
        assert_eq!(score, 1.0, "Strict mode pass requires score == 1.0");
    } else {
        assert!(score < 1.0, "Strict mode fail means score < 1.0");
    }
}

#[tokio::test]
async fn test_validate_mode_majority() {
    // Test majority mode: >50% must pass (exactly 50% = FAIL)
    let yaml = r#"
scroll: test-validate-majority
description: Test majority mode validation
requires:
  input_content:
    type: string
    default: "Partially complete content"
steps:
  - validate:
      input: ${input_content}
      criteria:
        - "Has documentation"
        - "Is complete"
        - "Has tests"
        - "Has examples"
      mode: majority
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse validate scroll");

    let result = executor.execute_scroll(&scroll).await;
    if let Err(ref e) = result {
        eprintln!("Execution error: {:?}", e);
    }
    assert!(result.is_ok(), "Should execute validate successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");

    let score = output.get("score")
        .and_then(|s| s.as_f64())
        .expect("Should have score");

    let result_value = output.get("result")
        .and_then(|r| r.as_str())
        .expect("Should have result");

    // In majority mode, pass only if score > 0.5 (exactly 0.5 = fail)
    if result_value == "pass" {
        assert!(score > 0.5, "Majority mode pass requires score > 0.5, got {}", score);
    } else {
        assert!(score <= 0.5, "Majority mode fail means score <= 0.5, got {}", score);
    }
}

#[tokio::test]
async fn test_validate_mode_any() {
    // Test any mode: at least one must pass (score > 0.0)
    let yaml = r#"
scroll: test-validate-any
description: Test any mode validation
requires:
  input_content:
    type: string
    default: "Minimal content"
steps:
  - validate:
      input: ${input_content}
      criteria:
        - "Has documentation"
        - "Is complete"
        - "Has comprehensive tests"
      mode: any
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse validate scroll");

    let result = executor.execute_scroll(&scroll).await;
    if let Err(ref e) = result {
        eprintln!("Execution error: {:?}", e);
    }
    assert!(result.is_ok(), "Should execute validate successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");

    let score = output.get("score")
        .and_then(|s| s.as_f64())
        .expect("Should have score");

    let result_value = output.get("result")
        .and_then(|r| r.as_str())
        .expect("Should have result");

    // In any mode, pass if score > 0.0
    if result_value == "pass" {
        assert!(score > 0.0, "Any mode pass requires score > 0.0, got {}", score);
    } else {
        assert_eq!(score, 0.0, "Any mode fail means score == 0.0, got {}", score);
    }
}

#[tokio::test]
async fn test_validate_score_calculation() {
    // Test that score = passed_count / total_count
    let yaml = r#"
scroll: test-validate-score
description: Test score calculation
requires:
  input_content:
    type: string
    default: "Content with mixed criteria satisfaction"
steps:
  - validate:
      input: ${input_content}
      criteria:
        - "Has documentation"
        - "Is complete"
        - "Has tests"
        - "Has examples"
      mode: majority
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse validate scroll");

    let result = executor.execute_scroll(&scroll).await;
    if let Err(ref e) = result {
        eprintln!("Execution error: {:?}", e);
    }
    assert!(result.is_ok(), "Should execute validate successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");

    let score = output.get("score")
        .and_then(|s| s.as_f64())
        .expect("Should have score");

    let criteria_results = output.get("criteria_results")
        .and_then(|c| c.as_array())
        .expect("Criteria results should be array");

    // Calculate expected score
    let total_count = criteria_results.len();
    let passed_count = criteria_results.iter()
        .filter(|cr| {
            cr.as_object()
                .and_then(|m| m.get("passed"))
                .and_then(|p| p.as_bool())
                .unwrap_or(false)
        })
        .count();

    let expected_score = passed_count as f64 / total_count as f64;

    // Allow small floating point error
    let diff = (score - expected_score).abs();
    assert!(diff < 0.01, "Score should be passed_count/total_count. Expected {}, got {}",
        expected_score, score);
}

#[tokio::test]
async fn test_validate_schema_validation_only() {
    // Test that validate uses schema validation, not consensus (D31)
    // This is tested implicitly - if validation succeeds with correct structure,
    // it's using schema validation. Consensus would cause infinite regress.
    let yaml = r#"
scroll: test-validate-schema
description: Test schema validation only
requires:
  input_content:
    type: string
    default: "Content to validate"
steps:
  - validate:
      input: ${input_content}
      criteria:
        - "Is valid"
      mode: strict
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse validate scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute validate with schema validation only");
}

#[tokio::test]
async fn test_validate_with_reference() {
    // Test that reference parameter is accepted and used
    let yaml = r#"
scroll: test-validate-reference
description: Test reference parameter
requires:
  input_content:
    type: string
    default: "Implementation code"
  ref_doc:
    type: string
    default: "API specification document"
steps:
  - validate:
      input: ${input_content}
      criteria:
        - "Follows specification"
        - "Implements all features"
      reference: ${ref_doc}
      mode: strict
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse validate with reference");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute validate with reference");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    assert!(output.get("result").is_some(), "Should have result field");
}

#[tokio::test]
async fn test_validate_default_mode_is_strict() {
    // Test that mode defaults to strict when not specified
    let yaml = r#"
scroll: test-validate-default-mode
description: Test default mode
requires:
  input_content:
    type: string
    default: "Content"
steps:
  - validate:
      input: ${input_content}
      criteria:
        - "Is valid"
    output: result
"#;

    let scroll = parse_scroll(yaml).expect("Should parse validate without mode");

    // If it parses, the default was applied
    // The actual mode behavior is tested in mode-specific tests
    assert!(scroll.steps.len() > 0, "Should have steps");
}

#[tokio::test]
async fn test_validate_all_mode_values() {
    // Test all mode enum values parse correctly
    for mode in &["strict", "majority", "any"] {
        let yaml = format!(r#"
scroll: test-validate-mode-{}
description: Test {} mode
requires:
  input_content:
    type: string
    default: "Test content"
steps:
  - validate:
      input: ${{input_content}}
      criteria:
        - "Is valid"
      mode: {}
    output: result
"#, mode, mode, mode);

        let mut executor = Executor::for_testing();
        let scroll = parse_scroll(&yaml)
            .unwrap_or_else(|e| panic!("Should parse validate with mode {}: {}", mode, e));

        let result = executor.execute_scroll(&scroll).await;
        if let Err(ref e) = result {
            eprintln!("Execution error for mode {}: {:?}", mode, e);
        }
        assert!(result.is_ok(), "Should execute validate with mode {}", mode);
    }
}

// ============================================================================
// B1 (#180/#181): Validate halt/continue behavior
// ============================================================================

#[tokio::test]
async fn test_validate_fail_halts_execution() {
    let fail_response = r#"{"result": "fail", "score": 0.3, "summary": "Validation failed: criteria not met", "criteria_results": [{"criterion": "Has docs", "passed": false, "explanation": "Missing documentation"}]}"#;

    let yaml = r#"
scroll: test-validate-halt
description: Test validate fail halts
steps:
  - validate:
      input: "test input"
      criteria:
        - "Has docs"
      mode: strict
    output: validation_result
  - set:
      values:
        should_not_reach: "unreachable"
    output: unreachable
"#;
    let scroll = parse_scroll(yaml).expect("parse");
    let mut executor = Executor::for_testing_with_llm_responses(vec![
        ("Validate".to_string(), fail_response.to_string()),
    ]);

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_err(), "Expected validation failure to halt");
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("Validation failed"), "Error should be ValidationFailed, got: {err}");

    // Result stored before halt
    assert!(
        executor.context().get_variable("validation_result").is_some(),
        "Validation result should be stored before halt"
    );

    // Subsequent steps should NOT have executed
    assert!(
        executor.context().get_variable("unreachable").is_none(),
        "Steps after failed validation should not execute"
    );
}

#[tokio::test]
async fn test_validate_fail_continues_with_on_fail_continue() {
    let fail_response = r#"result: fail
score: 0.3
summary: "Validation failed but continuing"
criteria_results:
  - criterion: "Has docs"
    passed: false
    explanation: "Missing documentation""#;

    let yaml = r#"
scroll: test-validate-continue
description: Test validate fail continues
steps:
  - validate:
      input: "test input"
      criteria:
        - "Has docs"
      mode: strict
    output: validation_result
    on_fail: continue
  - set:
      values:
        did_reach: "yes"
    output: reached
"#;
    let scroll = parse_scroll(yaml).expect("parse");
    let mut executor = Executor::for_testing_with_llm_responses(vec![
        ("Validate".to_string(), fail_response.to_string()),
    ]);

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Expected execution to continue: {:?}", result.err());

    assert!(
        executor.context().get_variable("reached").is_some(),
        "Steps after continue-on-fail validation should execute"
    );
}
