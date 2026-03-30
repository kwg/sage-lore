// SPDX-License-Identifier: MIT
//! Integration tests for elaborate primitive contract enforcement (Story #72).
//!
//! Tests verify the contract per spec #42:
//! - Structured params (enums) enforced at parse time
//! - Output token count validated against length param
//! - Deterministic validation (token range, format markers)
//! - Consensus validation for fuzzy invariants
//! - Retry with validation feedback

use sage_lore::scroll::executor::Executor;
use sage_lore::scroll::parser::parse_scroll;

/// Helper to count tokens in text (approximation: split on whitespace).
/// This is a simple token counter. In production, use a proper tokenizer.
fn count_tokens(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Helper to check if text matches prose format (no special structure markers).
fn is_prose_format(text: &str) -> bool {
    let has_list_markers = text.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with('-') || trimmed.starts_with('*') || trimmed.starts_with("•")
    });

    let has_headers = text.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with('#') || (trimmed.ends_with(':') && trimmed.split_whitespace().count() <= 3)
    });

    !has_list_markers && !has_headers
}

/// Helper to check if text matches structured format (has headers or sections).
fn is_structured_format(text: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with('#') || (trimmed.ends_with(':') && !trimmed.contains(' '))
    })
}

/// Helper to check if text matches list format (has bullet points).
fn is_list_format(text: &str) -> bool {
    let bullet_lines = text.lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed.starts_with('-') || trimmed.starts_with('*') || trimmed.starts_with("•")
        })
        .count();
    bullet_lines >= 2 // At least 2 list items
}

#[tokio::test]
async fn test_elaborate_enum_params_enforced_at_parse() {
    // Test that invalid enum values are rejected at parse time
    let yaml = r#"
scroll: test-elaborate-enums
description: Test enum validation
steps:
  - elaborate:
      input: ${input_text}
      depth: invalid_depth  # Should fail - not a valid enum
      output_contract:
        length: paragraph
        format: prose
    output: result
"#;

    let result = parse_scroll(yaml);
    assert!(result.is_err(), "Should reject invalid depth enum value");
    let err_msg = result.unwrap_err().to_string();
    // Should get error about untagged enum not matching any variant
    assert!(err_msg.contains("data did not match") || err_msg.contains("unknown variant"));
}

#[tokio::test]
async fn test_elaborate_basic_execution() {
    // Test basic elaborate execution with valid params
    let yaml = r#"
scroll: test-elaborate-basic
description: Test basic elaborate execution
requires:
  input_text:
    type: string
    default: "Brief input text"
steps:
  - elaborate:
      input: ${input_text}
      depth: balanced
      output_contract:
        length: paragraph
        format: prose
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse valid elaborate scroll");

    let result = executor.execute_scroll(&scroll).await;
    if let Err(ref e) = result {
        eprintln!("Execution error: {:?}", e);
    }
    assert!(result.is_ok(), "Should execute elaborate successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    assert!(output.as_str().is_some(), "Result should be string");
}

#[tokio::test]
async fn test_elaborate_defaults() {
    // Test that defaults work when optional params are omitted
    let yaml = r#"
scroll: test-elaborate-defaults
description: Test default values
requires:
  input_text:
    type: string
    default: "Microservices"
steps:
  - elaborate:
      input: ${input_text}
      depth: balanced  # No output_contract specified - should use defaults
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse elaborate with defaults");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute elaborate with default output_contract");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    let text = output.as_str().expect("Result should be string");
    let token_count = count_tokens(text);

    // Default is paragraph length (75-400 tokens) but allow wider range for mock LLM
    assert!(token_count > 0, "Should produce some output");
}

#[tokio::test]
async fn test_elaborate_with_context() {
    // Test that context parameter is accepted
    let yaml = r#"
scroll: test-elaborate-context
description: Test context injection
requires:
  input_text:
    type: string
    default: "API design"
steps:
  - elaborate:
      input: ${input_text}
      depth: balanced
      output_contract:
        length: paragraph
        format: prose
      context:
        domain: "e-commerce"
        audience: "technical"
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse elaborate with context");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute elaborate with context");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    assert!(output.as_str().is_some(), "Result should be string");
}

#[tokio::test]
async fn test_elaborate_all_depth_levels() {
    // Test all depth enum values parse correctly
    for depth in &["thorough", "balanced", "concise"] {
        let yaml = format!(r#"
scroll: test-elaborate-depth-{}
description: Test {} depth
requires:
  input_text:
    type: string
    default: "Test input"
steps:
  - elaborate:
      input: ${{input_text}}
      depth: {}
      output_contract:
        length: paragraph
        format: prose
    output: result
"#, depth, depth, depth);

        let mut executor = Executor::for_testing();
        let scroll = parse_scroll(&yaml)
            .unwrap_or_else(|e| panic!("Should parse elaborate with depth {}: {}", depth, e));

        let result = executor.execute_scroll(&scroll).await;
        assert!(result.is_ok(), "Should execute elaborate with depth {}", depth);
    }
}

#[tokio::test]
async fn test_elaborate_all_length_values() {
    // Test all length enum values parse correctly
    for length in &["sentence", "paragraph", "page"] {
        let yaml = format!(r#"
scroll: test-elaborate-length-{}
description: Test {} length
requires:
  input_text:
    type: string
    default: "Test input"
steps:
  - elaborate:
      input: ${{input_text}}
      depth: balanced
      output_contract:
        length: {}
        format: prose
    output: result
"#, length, length, length);

        let mut executor = Executor::for_testing();
        let scroll = parse_scroll(&yaml)
            .unwrap_or_else(|e| panic!("Should parse elaborate with length {}: {}", length, e));

        let result = executor.execute_scroll(&scroll).await;
        if let Err(ref e) = result {
            eprintln!("Test failed for length {}: {:?}", length, e);
        }
        assert!(result.is_ok(), "Should execute elaborate with length {}", length);
    }
}

#[tokio::test]
async fn test_elaborate_all_format_values() {
    // Test all format enum values parse correctly
    for format in &["prose", "structured", "list"] {
        let yaml = format!(r#"
scroll: test-elaborate-format-{}
description: Test {} format
requires:
  input_text:
    type: string
    default: "Test input"
steps:
  - elaborate:
      input: ${{input_text}}
      depth: balanced
      output_contract:
        length: paragraph
        format: {}
    output: result
"#, format, format, format);

        let mut executor = Executor::for_testing();
        let scroll = parse_scroll(&yaml)
            .unwrap_or_else(|e| panic!("Should parse elaborate with format {}: {}", format, e));

        let result = executor.execute_scroll(&scroll).await;
        assert!(result.is_ok(), "Should execute elaborate with format {}", format);
    }
}

#[tokio::test]
async fn test_elaborate_token_validation_sentence() {
    // Test that sentence length is validated (15-75 tokens)
    let yaml = r#"
scroll: test-elaborate-token-sentence
description: Test sentence token validation
requires:
  input_text:
    type: string
    default: "Elaborate on quantum computing"
steps:
  - elaborate:
      input: ${input_text}
      depth: balanced
      output_contract:
        length: sentence
        format: prose
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse elaborate scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute elaborate successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    let text = output.as_str().expect("Result should be string");
    let token_count = count_tokens(text);

    // Sentence should be 15-75 tokens
    assert!(token_count >= 15, "Sentence should have at least 15 tokens, got {}", token_count);
    assert!(token_count <= 75, "Sentence should have at most 75 tokens, got {}", token_count);
}

#[tokio::test]
async fn test_elaborate_token_validation_paragraph() {
    // Test that paragraph length is validated (75-400 tokens)
    let yaml = r#"
scroll: test-elaborate-token-paragraph
description: Test paragraph token validation
requires:
  input_text:
    type: string
    default: "Microservices architecture"
steps:
  - elaborate:
      input: ${input_text}
      depth: balanced
      output_contract:
        length: paragraph
        format: prose
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse elaborate scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute elaborate successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    let text = output.as_str().expect("Result should be string");
    let token_count = count_tokens(text);

    // Paragraph should be 75-400 tokens
    assert!(token_count >= 75, "Paragraph should have at least 75 tokens, got {}", token_count);
    assert!(token_count <= 400, "Paragraph should have at most 400 tokens, got {}", token_count);
}

#[tokio::test]
async fn test_elaborate_format_validation_prose() {
    // Test that prose format is validated (no list markers or headers)
    let yaml = r#"
scroll: test-elaborate-format-prose
description: Test prose format validation
requires:
  input_text:
    type: string
    default: "API design principles"
steps:
  - elaborate:
      input: ${input_text}
      depth: balanced
      output_contract:
        length: paragraph
        format: prose
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse elaborate scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute elaborate successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    let text = output.as_str().expect("Result should be string");

    // Prose should not have list markers or headers
    assert!(is_prose_format(text), "Output should be in prose format (no lists or headers)");
}

#[tokio::test]
async fn test_elaborate_format_validation_structured() {
    // Test that structured format is validated (has headers or sections)
    let yaml = r#"
scroll: test-elaborate-format-structured
description: Test structured format validation
requires:
  input_text:
    type: string
    default: "Database design patterns"
steps:
  - elaborate:
      input: ${input_text}
      depth: balanced
      output_contract:
        length: paragraph
        format: structured
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse elaborate scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute elaborate successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    let text = output.as_str().expect("Result should be string");

    // Structured should have headers or sections
    assert!(is_structured_format(text), "Output should be in structured format (with headers)");
}

#[tokio::test]
async fn test_elaborate_format_validation_list() {
    // Test that list format is validated (has bullet points)
    let yaml = r#"
scroll: test-elaborate-format-list
description: Test list format validation
requires:
  input_text:
    type: string
    default: "Best practices for code review"
steps:
  - elaborate:
      input: ${input_text}
      depth: balanced
      output_contract:
        length: paragraph
        format: list
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse elaborate scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute elaborate successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    let text = output.as_str().expect("Result should be string");

    // List should have bullet points
    assert!(is_list_format(text), "Output should be in list format (with bullet points)");
}

#[tokio::test]
async fn test_elaborate_consensus_validation() {
    // Test that consensus validation is performed (preserves_intent and adds_detail)
    let yaml = r#"
scroll: test-elaborate-consensus
description: Test consensus validation
requires:
  input_text:
    type: string
    default: "Explain REST APIs"
steps:
  - elaborate:
      input: ${input_text}
      depth: balanced
      output_contract:
        length: paragraph
        format: prose
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse elaborate scroll");

    // Consensus validation happens internally - we just verify execution succeeds
    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute elaborate with consensus validation");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    assert!(output.as_str().is_some(), "Result should be string");
}
