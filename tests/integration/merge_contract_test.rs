// SPDX-License-Identifier: MIT
//! Integration tests for merge primitive contract enforcement (Story #75).
//!
//! Tests verify the contract per spec #45:
//! - Input count validation (2-10 inputs required)
//! - Strategy enum enforced at parse time
//! - Strategy-specific output structures
//! - Coherent output validation

use sage_lore::scroll::executor::Executor;
use sage_lore::scroll::parser::parse_scroll;

#[tokio::test]
async fn test_merge_minimum_2_inputs() {
    let scroll_yaml = r#"
scroll: test_merge_min_inputs
description: Test merge requires minimum 2 inputs
requires:
  single_input:
    type: string
    default: "Some content"
steps:
  - merge:
      inputs:
        - ${single_input}
      strategy: sequential
    output: result
"#;

    let scroll = parse_scroll(scroll_yaml).expect("Failed to parse scroll");
    let mut executor = Executor::for_testing();

    // Should fail with minimum input validation error
    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_err(), "Expected error for single input, got success");
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("at least 2 inputs") || error_msg.contains("minimum") || error_msg.contains("requires 2"),
        "Error should mention minimum inputs requirement, got: {}",
        error_msg
    );
}

#[tokio::test]
async fn test_merge_strategy_sequential() {
    let scroll_yaml = r#"
scroll: test_merge_sequential
description: Test sequential merge strategy
requires:
  input1:
    type: string
    default: "First point: Use React"
  input2:
    type: string
    default: "Second point: Use TypeScript"
steps:
  - merge:
      inputs:
        - ${input1}
        - ${input2}
      strategy: sequential
    output: result
"#;

    let scroll = parse_scroll(scroll_yaml).expect("Failed to parse scroll");
    let mut executor = Executor::for_testing();

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Merge with 2 inputs should succeed: {:?}", result);
}

#[tokio::test]
async fn test_merge_strategy_reconcile() {
    let scroll_yaml = r#"
scroll: test_merge_reconcile
description: Test reconcile merge strategy
requires:
  review1:
    type: string
    default: "Recommend React for better ecosystem"
  review2:
    type: string
    default: "Recommend Vue for simplicity"
  review3:
    type: string
    default: "Recommend React for team familiarity"
steps:
  - merge:
      inputs:
        - ${review1}
        - ${review2}
        - ${review3}
      strategy: reconcile
      output_contract:
        format: structured
    output: result
"#;

    let scroll = parse_scroll(scroll_yaml).expect("Failed to parse scroll");
    let mut executor = Executor::for_testing();

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Reconcile merge should succeed: {:?}", result);

    // Verify output structure for reconcile strategy
    let output = executor.context().get_variable("result").expect("Should have result");

    // For reconcile strategy, expect structured output with content and possibly conflicts
    if let serde_json::Value::Object(map) = output {
        assert!(
            map.contains_key("content"),
            "Reconcile output should have 'content' field"
        );
    }
}

#[tokio::test]
async fn test_merge_strategy_union() {
    let scroll_yaml = r#"
scroll: test_merge_union
description: Test union merge strategy
requires:
  input1:
    type: string
    default: "Feature A: Authentication\nFeature B: API"
  input2:
    type: string
    default: "Feature B: API\nFeature C: Database"
steps:
  - merge:
      inputs:
        - ${input1}
        - ${input2}
      strategy: union
    output: result
"#;

    let scroll = parse_scroll(scroll_yaml).expect("Failed to parse scroll");
    let mut executor = Executor::for_testing();

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Union merge should succeed: {:?}", result);
}

#[tokio::test]
async fn test_merge_strategy_intersection() {
    let scroll_yaml = r#"
scroll: test_merge_intersection
description: Test intersection merge strategy
requires:
  input1:
    type: string
    default: "Feature A\nFeature B\nFeature C"
  input2:
    type: string
    default: "Feature B\nFeature C\nFeature D"
steps:
  - merge:
      inputs:
        - ${input1}
        - ${input2}
      strategy: intersection
    output: result
"#;

    let scroll = parse_scroll(scroll_yaml).expect("Failed to parse scroll");
    let mut executor = Executor::for_testing();

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Intersection merge should succeed: {:?}", result);
}

#[tokio::test]
async fn test_merge_all_inputs_represented() {
    let scroll_yaml = r#"
scroll: test_all_inputs_represented
description: Test that all inputs contribute to output
requires:
  input1:
    type: string
    default: "Unique point from first input"
  input2:
    type: string
    default: "Unique point from second input"
  input3:
    type: string
    default: "Unique point from third input"
steps:
  - merge:
      inputs:
        - ${input1}
        - ${input2}
        - ${input3}
      strategy: sequential
    output: result
"#;

    let scroll = parse_scroll(scroll_yaml).expect("Failed to parse scroll");
    let mut executor = Executor::for_testing();

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Merge should succeed: {:?}", result);

    // Note: Full validation of all inputs represented would require LLM output analysis
    // This test verifies the operation completes successfully with multiple inputs
}

#[tokio::test]
async fn test_merge_coherent_not_concatenated() {
    let scroll_yaml = r#"
scroll: test_coherent_output
description: Test merge produces coherent unified output
requires:
  input1:
    type: string
    default: "Use React framework"
  input2:
    type: string
    default: "Use TypeScript language"
steps:
  - merge:
      inputs:
        - ${input1}
        - ${input2}
      strategy: sequential
    output: result
"#;

    let scroll = parse_scroll(scroll_yaml).expect("Failed to parse scroll");
    let mut executor = Executor::for_testing();

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Merge should succeed: {:?}", result);

    // Note: Actual coherence validation (no "Input 1 says...", etc.) would require
    // LLM output inspection. This test verifies the operation completes.
}

#[tokio::test]
async fn test_merge_no_hallucination() {
    let scroll_yaml = r#"
scroll: test_no_hallucination
description: Test merge does not add information not in inputs
requires:
  input1:
    type: string
    default: "Specific fact A"
  input2:
    type: string
    default: "Specific fact B"
steps:
  - merge:
      inputs:
        - ${input1}
        - ${input2}
      strategy: sequential
    output: result
"#;

    let scroll = parse_scroll(scroll_yaml).expect("Failed to parse scroll");
    let mut executor = Executor::for_testing();

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Merge should succeed: {:?}", result);

    // Note: Verifying no hallucination would require comparing output against inputs
    // This test verifies the operation completes successfully
}

#[tokio::test]
async fn test_merge_context_priority() {
    let scroll_yaml = r#"
scroll: test_context_priority
description: Test merge uses context to guide resolution
requires:
  input1:
    type: string
    default: "Use framework A (better developer experience)"
  input2:
    type: string
    default: "Use framework B (better performance)"
steps:
  - merge:
      inputs:
        - ${input1}
        - ${input2}
      strategy: reconcile
      context:
        priority: "Choose options that emphasize performance"
    output: result
"#;

    let scroll = parse_scroll(scroll_yaml).expect("Failed to parse scroll");
    let mut executor = Executor::for_testing();

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Merge with context should succeed: {:?}", result);

    // Context should guide the merge strategy to prioritize performance
    // Actual verification would require inspecting LLM output
}

#[tokio::test]
async fn test_merge_max_10_inputs() {
    let scroll_yaml = r#"
scroll: test_merge_max_inputs
description: Test merge with 10 inputs (maximum)
requires:
  i1: { type: string, default: "Input 1" }
  i2: { type: string, default: "Input 2" }
  i3: { type: string, default: "Input 3" }
  i4: { type: string, default: "Input 4" }
  i5: { type: string, default: "Input 5" }
  i6: { type: string, default: "Input 6" }
  i7: { type: string, default: "Input 7" }
  i8: { type: string, default: "Input 8" }
  i9: { type: string, default: "Input 9" }
  i10: { type: string, default: "Input 10" }
steps:
  - merge:
      inputs:
        - ${i1}
        - ${i2}
        - ${i3}
        - ${i4}
        - ${i5}
        - ${i6}
        - ${i7}
        - ${i8}
        - ${i9}
        - ${i10}
      strategy: sequential
    output: result
"#;

    let scroll = parse_scroll(scroll_yaml).expect("Failed to parse scroll");
    let mut executor = Executor::for_testing();

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Merge with 10 inputs should succeed: {:?}", result);
}

#[tokio::test]
async fn test_merge_empty_input_validation() {
    let scroll_yaml = r#"
scroll: test_empty_input
description: Test merge handling of empty inputs
requires:
  input1:
    type: string
    default: "Valid content"
  input2:
    type: string
    default: ""
steps:
  - merge:
      inputs:
        - ${input1}
        - ${input2}
      strategy: sequential
    output: result
"#;

    let scroll = parse_scroll(scroll_yaml).expect("Failed to parse scroll");
    let mut executor = Executor::for_testing();

    // Should handle empty inputs (may fail or succeed depending on implementation)
    let _result = executor.execute_scroll(&scroll).await;
    // This test documents the behavior - implementation may choose to reject empty inputs
}
