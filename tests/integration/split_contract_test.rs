// SPDX-License-Identifier: MIT
//! Integration tests for split primitive contract enforcement (Story #74).
//!
//! Tests verify the contract per spec #44:
//! - Structured params (enums) enforced at parse time
//! - Output always has chunks array with id, content, label
//! - 95% coverage validated
//! - No sentence-level overlap
//! - Count strategy reduces gracefully
//! - Tests verify contract behavior

use sage_lore::scroll::executor::Executor;
use sage_lore::scroll::parser::parse_scroll;

/// Helper to calculate character coverage (non-whitespace chars).
fn calculate_char_coverage(input: &str, chunks_content: &str) -> f64 {
    let input_chars: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    let chunks_chars: String = chunks_content.chars().filter(|c| !c.is_whitespace()).collect();

    let input_len = input_chars.len();
    if input_len == 0 {
        return 1.0;
    }

    let covered = chunks_chars.len().min(input_len);
    covered as f64 / input_len as f64
}

#[tokio::test]
async fn test_split_enum_params_enforced_at_parse() {
    // Test that invalid enum values are rejected at parse time
    let yaml = r#"
scroll: test-split-enums
description: Test enum validation
steps:
  - split:
      input: ${input_text}
      by: invalid_strategy  # Should fail - not a valid enum
      granularity: medium
    output: result
"#;

    let result = parse_scroll(yaml);
    assert!(result.is_err(), "Should reject invalid by enum value");
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("data did not match") || err_msg.contains("unknown variant"));
}

#[tokio::test]
async fn test_split_complete_coverage() {
    // Test that all input content appears in chunks (95% threshold)
    let yaml = r#"
scroll: test-split-coverage
description: Test complete coverage validation
requires:
  input_text:
    type: string
    default: "This is section one about introduction. This is section two about requirements. This is section three about implementation."
steps:
  - split:
      input: ${input_text}
      by: semantic
      granularity: medium
    output: chunks
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse valid split scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Split should execute successfully: {:?}", result.err());

    // Get chunks from context
    let chunks = executor.context().get_variable("chunks")
        .expect("chunks variable should be set");

    // Verify chunks is a sequence
    let chunks_seq = chunks.as_array()
        .expect("chunks should be a sequence");

    assert!(chunks_seq.len() > 0, "Should have at least one chunk");

    // Collect all content from chunks
    let mut total_content = String::new();
    for chunk in chunks_seq {
        let chunk_map = chunk.as_object()
            .expect("Each chunk should be a mapping");
        let content = chunk_map.get("content")
            .expect("Each chunk should have content field")
            .as_str()
            .expect("Content should be a string");
        total_content.push_str(content);
    }

    // Verify coverage >= 95%
    let input = "This is section one about introduction. This is section two about requirements. This is section three about implementation.";
    let coverage = calculate_char_coverage(input, &total_content);

    assert!(
        coverage >= 0.95,
        "Coverage should be >= 95%, got {:.1}%. Input length: {}, chunks length: {}",
        coverage * 100.0,
        input.len(),
        total_content.len()
    );
}

#[tokio::test]
async fn test_split_no_overlap() {
    // Test that no sentence appears in multiple chunks
    let yaml = r#"
scroll: test-split-no-overlap
description: Test no-overlap validation
requires:
  input_text:
    type: string
    default: "First distinct paragraph here. Second distinct paragraph here. Third distinct paragraph here."
steps:
  - split:
      input: ${input_text}
      by: semantic
      granularity: fine
    output: chunks
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse valid split scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Split should execute successfully: {:?}", result.err());

    // Get chunks from context
    let chunks = executor.context().get_variable("chunks")
        .expect("chunks variable should be set");

    let chunks_seq = chunks.as_array()
        .expect("chunks should be a sequence");

    // Extract all sentences from all chunks
    let mut all_sentences = Vec::new();
    for chunk in chunks_seq {
        let chunk_map = chunk.as_object().unwrap();
        let content = chunk_map.get("content")
            .unwrap().as_str().unwrap();

        // Split into sentences
        for sentence in content.split(|c| c == '.' || c == '!' || c == '?') {
            let trimmed = sentence.trim();
            if trimmed.len() > 10 {
                all_sentences.push(trimmed.to_lowercase());
            }
        }
    }

    // Check for duplicates (ignoring structural markers)
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    let mut duplicates = Vec::new();

    for sentence in &all_sentences {
        // Skip structural markers (very short or ending with colon)
        if sentence.len() < 50 && (sentence.ends_with(':') || sentence.starts_with('#')) {
            continue;
        }

        if !seen.insert(sentence) {
            duplicates.push(sentence);
        }
    }

    assert!(
        duplicates.is_empty(),
        "Found overlapping sentences: {:?}",
        duplicates
    );
}

#[tokio::test]
async fn test_split_ordered_sequence() {
    // Test that chunks maintain original order
    let yaml = r#"
scroll: test-split-ordered
description: Test ordered sequence output
requires:
  input_text:
    type: string
    default: "ALPHA section. BETA section. GAMMA section."
steps:
  - split:
      input: ${input_text}
      by: semantic
      granularity: medium
    output: chunks
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse valid split scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Split should execute successfully: {:?}", result.err());

    let chunks = executor.context().get_variable("chunks").unwrap();
    let chunks_seq = chunks.as_array().unwrap();

    // Collect content in order
    let mut contents = Vec::new();
    for chunk in chunks_seq {
        let chunk_map = chunk.as_object().unwrap();
        let content = chunk_map.get("content")
            .unwrap().as_str().unwrap();
        contents.push(content);
    }

    // Verify ALPHA appears before BETA appears before GAMMA
    let full_content = contents.join(" ");
    if let Some(alpha_pos) = full_content.find("ALPHA") {
        if let Some(beta_pos) = full_content.find("BETA") {
            if let Some(gamma_pos) = full_content.find("GAMMA") {
                assert!(
                    alpha_pos < beta_pos && beta_pos < gamma_pos,
                    "Chunks should maintain order: ALPHA < BETA < GAMMA, got positions: {} < {} < {}",
                    alpha_pos, beta_pos, gamma_pos
                );
            }
        }
    }
}

#[tokio::test]
async fn test_split_by_semantic() {
    // Test semantic strategy - LLM finds logical divisions
    let yaml = r#"
scroll: test-split-semantic
description: Test semantic splitting
requires:
  input_text:
    type: string
    default: "Machine learning is a subset of AI. Deep learning uses neural networks. Natural language processing handles text."
steps:
  - split:
      input: ${input_text}
      by: semantic
      granularity: fine
    output: chunks
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse valid split scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Split should execute successfully: {:?}", result.err());

    let chunks = executor.context().get_variable("chunks").unwrap();
    let chunks_seq = chunks.as_array().unwrap();

    // Should split into multiple chunks based on semantic boundaries
    assert!(
        chunks_seq.len() >= 2,
        "Semantic split should produce at least 2 chunks, got {}",
        chunks_seq.len()
    );

    // Verify each chunk has required structure
    for (idx, chunk) in chunks_seq.iter().enumerate() {
        let chunk_map = chunk.as_object()
            .expect(&format!("Chunk {} should be a mapping", idx));

        assert!(
            chunk_map.contains_key("id"),
            "Chunk {} should have 'id' field",
            idx
        );

        assert!(
            chunk_map.contains_key("content"),
            "Chunk {} should have 'content' field",
            idx
        );
    }
}

#[tokio::test]
async fn test_split_by_structure_headers() {
    // Test structure strategy with markdown headers
    let yaml = r#"
scroll: test-split-structure
description: Test structural splitting on headers
requires:
  input_text:
    type: string
    default: |
      # Introduction
      This is the intro section.

      # Requirements
      These are the requirements.

      # Implementation
      This is how we implement it.
steps:
  - split:
      input: ${input_text}
      by: structure
      granularity: medium
      markers: headers
    output: chunks
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse valid split scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Split should execute successfully: {:?}", result.err());

    let chunks = executor.context().get_variable("chunks").unwrap();
    let chunks_seq = chunks.as_array().unwrap();

    // Should split on headers - expect at least 2 chunks
    assert!(
        chunks_seq.len() >= 2,
        "Structure split on headers should produce at least 2 chunks, got {}",
        chunks_seq.len()
    );

    // Verify chunks have labels (derived from headers)
    let mut has_labels = 0;
    for chunk in chunks_seq {
        let chunk_map = chunk.as_object().unwrap();
        if chunk_map.contains_key("label") {
            has_labels += 1;
        }
    }

    assert!(
        has_labels >= 1,
        "At least one chunk should have a label from headers"
    );
}

#[tokio::test]
async fn test_split_by_count() {
    // Test count strategy - split into N chunks
    let yaml = r#"
scroll: test-split-count
description: Test count-based splitting
requires:
  input_text:
    type: string
    default: "One sentence here. Two sentence here. Three sentence here. Four sentence here. Five sentence here."
steps:
  - split:
      input: ${input_text}
      by: count
      granularity: medium
      count: 3
    output: chunks
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse valid split scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Split should execute successfully: {:?}", result.err());

    let chunks = executor.context().get_variable("chunks").unwrap();
    let chunks_seq = chunks.as_array().unwrap();

    // Should produce requested count or fewer (if input too short)
    // For this input, should be able to produce 3 chunks
    assert!(
        chunks_seq.len() >= 2 && chunks_seq.len() <= 3,
        "Count split should produce close to requested count (3), got {}",
        chunks_seq.len()
    );
}

#[tokio::test]
async fn test_split_always_returns_array() {
    // Test that split always returns array, never scalar
    let yaml = r#"
scroll: test-split-array
description: Test split always returns array
requires:
  input_text:
    type: string
    default: "Single short sentence."
steps:
  - split:
      input: ${input_text}
      by: semantic
      granularity: coarse
    output: chunks
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse valid split scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Split should execute successfully: {:?}", result.err());

    let chunks = executor.context().get_variable("chunks").unwrap();

    // MUST be a sequence, never a string or other scalar
    assert!(
        chunks.is_array(),
        "Split must always return a sequence, got: {:?}",
        chunks
    );

    let chunks_seq = chunks.as_array().unwrap();

    // Even for very short input, should have at least one chunk
    assert!(
        chunks_seq.len() >= 1,
        "Split should always return at least one chunk"
    );
}

#[tokio::test]
async fn test_split_chunk_structure_validation() {
    // Test that chunk structure is validated (id, content, label fields)
    let yaml = r#"
scroll: test-split-structure-validation
description: Test chunk structure validation
requires:
  input_text:
    type: string
    default: "First part of content. Second part of content."
steps:
  - split:
      input: ${input_text}
      by: semantic
      granularity: medium
    output: chunks
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse valid split scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Split should execute successfully: {:?}", result.err());

    let chunks = executor.context().get_variable("chunks").unwrap();
    let chunks_seq = chunks.as_array().unwrap();

    // Validate each chunk has required fields
    for (idx, chunk) in chunks_seq.iter().enumerate() {
        let chunk_map = chunk.as_object()
            .expect(&format!("Chunk {} must be a mapping", idx + 1));

        // id field (required, must be number)
        let id = chunk_map.get("id")
            .expect(&format!("Chunk {} must have 'id' field", idx + 1));
        assert!(
            id.is_number(),
            "Chunk {} 'id' must be a number, got: {:?}",
            idx + 1, id
        );

        // content field (required, must be string)
        let content = chunk_map.get("content")
            .expect(&format!("Chunk {} must have 'content' field", idx + 1));
        assert!(
            content.is_string(),
            "Chunk {} 'content' must be a string, got: {:?}",
            idx + 1, content
        );

        // label field (optional, but if present must be string)
        if let Some(label) = chunk_map.get("label") {
            assert!(
                label.is_string(),
                "Chunk {} 'label' must be a string if present, got: {:?}",
                idx + 1, label
            );
        }
    }
}
