// SPDX-License-Identifier: MIT
//! Real LLM integration tests using Ollama.
//!
//! These tests call a live Ollama instance to verify that the engine's
//! primitives work with real model output — the layer between mock tests
//! and full E2E runs.
//!
//! Gated behind SAGE_TEST_LLM=1 environment variable.
//! Default model: gpt-oss:20b (fast, good enough for structural tests).
//!
//! Run with:
//!   SAGE_TEST_LLM=1 cargo test --test integration_tests ollama_llm -- --nocapture

use std::sync::Arc;

use sage_lore::primitives::invoke::{OllamaBackend, LlmBackend, LlmRequest};
use sage_lore::primitives::fs::MockFsBackend;
use sage_lore::primitives::platform::MockPlatform;
use sage_lore::primitives::test::NoopBackend;
use sage_lore::scroll::executor::Executor;
use sage_lore::scroll::interfaces::InterfaceRegistry;
use sage_lore::scroll::interfaces::{fs, invoke, platform, test as test_iface, vcs, secure};
use sage_lore::scroll::parser::parse_scroll;

fn should_run() -> bool {
    std::env::var("SAGE_TEST_LLM").map(|v| v == "1").unwrap_or(false)
}

fn test_model() -> String {
    std::env::var("SAGE_TEST_MODEL").unwrap_or_else(|_| "gpt-oss:20b".to_string())
}

/// Create an executor with real Ollama backend + mock everything else.
fn executor_with_ollama() -> Executor {
    let model = test_model();
    let ollama = OllamaBackend::new().with_model(&model);
    let ollama_arc: Arc<dyn LlmBackend> = Arc::new(ollama);

    let mut registry = InterfaceRegistry::for_testing();
    registry.set_invoke_backend(ollama_arc);

    Executor::with_registry(registry)
}

// ============================================================================
// Layer 2: Basic LLM connectivity
// ============================================================================

#[tokio::test]
async fn test_ollama_responds() {
    if !should_run() { return; }

    let model = test_model();
    let backend = OllamaBackend::new().with_model(&model);

    let response = backend.generate(LlmRequest {
        prompt: "Respond with exactly one word: hello".to_string(),
        system: None,
        max_tokens: Some(32),
        temperature: None,
        timeout_secs: Some(30),
        model_tier: None,
        format_schema: None,
        model: None,
    }).await;

    assert!(response.is_ok(), "Ollama should respond: {:?}", response.err());
    let text = response.unwrap().text;
    assert!(!text.is_empty(), "Response should not be empty");
    println!("Ollama response: {}", text);
}

// ============================================================================
// Layer 2: Invoke primitive with real LLM
// ============================================================================

#[tokio::test]
async fn test_invoke_produces_output() {
    if !should_run() { return; }

    // Note: Ollama backend uses format:"json" globally, so prompts must
    // request JSON output or the model may produce empty/malformed JSON.
    let yaml = r#"
scroll: test-invoke-ollama
description: Test invoke with real LLM
steps:
  - invoke:
      agent: test-agent
      timeout_secs: 60
      prompt: |
        What is 2 + 2? Output your answer as JSON: {"answer": <number>}
    output: answer
"#;

    let mut executor = executor_with_ollama();
    let scroll = parse_scroll(yaml).expect("parse scroll");
    let result = executor.execute_scroll(&scroll).await;

    assert!(result.is_ok(), "Invoke should succeed: {:?}", result.err());

    let answer = executor.context().get_variable("answer");
    assert!(answer.is_some(), "Should have answer variable");
    let text = answer.unwrap().to_string();
    assert!(text.contains('4'), "Answer should contain 4, got: {}", text);
    println!("Invoke answer: {}", text);
}

// ============================================================================
// Layer 2: Convert primitive with real LLM (JSON output)
// ============================================================================

#[tokio::test]
async fn test_convert_json_output() {
    if !should_run() { return; }

    // Test that invoke → convert pipeline produces valid, schema-conformant JSON.
    // The convert step parses LLM output and validates against a schema.
    // Uses a simple prompt that most models handle reliably.
    let yaml = r#"
scroll: test-convert-ollama
description: Test convert produces valid JSON with real LLM
steps:
  - invoke:
      agent: test-agent
      timeout_secs: 60
      prompt: |
        Return a JSON object with a "name" field (string) and an "items" field (array of strings).
        Example: {"name": "colors", "items": ["red", "blue"]}
        Output ONLY valid JSON. No prose, no code fences.
    output: raw_data

  - convert:
      input: "${raw_data}"
      from: json
      to:
        format: json
        schema:
          type: object
          required:
            - name
            - items
          properties:
            name:
              type: string
            items:
              type: array
              items:
                type: string
    output: parsed_data
"#;

    let mut executor = executor_with_ollama();
    let scroll = parse_scroll(yaml).expect("parse scroll");
    let result = executor.execute_scroll(&scroll).await;

    assert!(result.is_ok(), "Convert should succeed: {:?}", result.err());

    let data = executor.context().get_variable("parsed_data");
    assert!(data.is_some(), "Should have parsed_data");

    if let Some(serde_json::Value::Object(map)) = data {
        assert!(map.contains_key("name"), "Should have name key");
        assert!(map.contains_key("items"), "Should have items key");
        println!("Parsed data: {:?}", map);
    }
}

// ============================================================================
// Layer 2: Distill with real LLM
// ============================================================================

#[tokio::test]
async fn test_distill_real_llm() {
    if !should_run() { return; }

    let yaml = r#"
scroll: test-distill-ollama
description: Test distill with real LLM
requires:
  input_text:
    type: string
    default: "The Rust programming language is a systems programming language focused on safety, concurrency, and performance. It achieves memory safety without garbage collection through its ownership system, which tracks references at compile time. Rust's type system and borrow checker prevent data races and null pointer dereferences, making it suitable for building reliable and efficient software."
steps:
  - distill:
      input: ${input_text}
      intensity: balanced
      output_contract:
        length: sentence
        format: prose
    output: result
"#;

    let mut executor = executor_with_ollama();
    let scroll = parse_scroll(yaml).expect("parse scroll");
    let result = executor.execute_scroll(&scroll).await;

    assert!(result.is_ok(), "Distill should succeed: {:?}", result.err());

    let output = executor.context().get_variable("result");
    assert!(output.is_some(), "Should have result");
    let text = output.unwrap().as_str().unwrap_or("").to_string();
    assert!(!text.is_empty(), "Distill output should not be empty");
    assert!(text.len() < 500, "Distilled text should be shorter than input, got {} chars", text.len());
    println!("Distilled: {}", text);
}

// ============================================================================
// Layer 2: Elaborate with real LLM
// ============================================================================

#[tokio::test]
async fn test_elaborate_real_llm() {
    if !should_run() { return; }

    let yaml = r#"
scroll: test-elaborate-ollama
description: Test elaborate with real LLM
requires:
  input_text:
    type: string
    default: "Rust ownership system"
steps:
  - elaborate:
      input: ${input_text}
      depth: balanced
      output_contract:
        length: paragraph
        format: prose
    output: result
"#;

    let mut executor = executor_with_ollama();
    let scroll = parse_scroll(yaml).expect("parse scroll");
    let result = executor.execute_scroll(&scroll).await;

    assert!(result.is_ok(), "Elaborate should succeed: {:?}", result.err());

    let output = executor.context().get_variable("result");
    assert!(output.is_some(), "Should have result");
    let text = output.unwrap().as_str().unwrap_or("").to_string();
    assert!(!text.is_empty(), "Elaborate output should not be empty");
    assert!(text.len() > 30, "Elaborated text should be longer than input, got {} chars", text.len());
    println!("Elaborated: {}", text);
}

// ============================================================================
// Layer 2: The implement-chunk scroll pattern (invoke → convert → set)
// This is the critical path for E2E — verify JSON output parsing works
// ============================================================================

#[tokio::test]
async fn test_invoke_json_output_for_implement_pattern() {
    if !should_run() { return; }

    // This tests the exact pattern implement-chunk uses:
    // invoke agent → get JSON → convert parses it with schema
    let yaml = r#"
scroll: test-implement-pattern
description: Test the invoke-convert JSON pattern used by implement-chunk
steps:
  - invoke:
      agent: test-writer
      timeout_secs: 120
      prompt: |
        You are a test engineer. Write a single test for a function called `add(a, b)` that adds two numbers.

        OUTPUT FORMAT (REQUIRED):
        Output ONLY valid JSON:
        {"files": [{"path": "tests/add_test.rs", "content": "test code here"}], "test_descriptions": ["what the test verifies"]}

        Both "files" and "test_descriptions" are REQUIRED.
        Output ONLY valid JSON. No prose, no markdown, no code fences.
    output: raw_tests

  - convert:
      input: "${raw_tests}"
      from: json
      to:
        format: json
        schema:
          type: object
          required:
            - files
            - test_descriptions
          properties:
            files:
              type: array
              items:
                type: object
                required:
                  - path
                  - content
                properties:
                  path:
                    type: string
                  content:
                    type: string
            test_descriptions:
              type: array
              items:
                type: string
    output: test_spec
    on_fail:
      retry:
        max: 2
"#;

    let mut executor = executor_with_ollama();
    let scroll = parse_scroll(yaml).expect("parse scroll");
    let result = executor.execute_scroll(&scroll).await;

    assert!(result.is_ok(), "Implement pattern should succeed: {:?}", result.err());

    let test_spec = executor.context().get_variable("test_spec");
    assert!(test_spec.is_some(), "Should have test_spec");

    if let Some(serde_json::Value::Object(map)) = test_spec {
        assert!(map.contains_key("files"), "Should have files");
        assert!(map.contains_key("test_descriptions"), "Should have test_descriptions");

        if let Some(serde_json::Value::Array(files)) = map.get("files") {
            assert!(!files.is_empty(), "Should have at least one test file");
            println!("Test files: {} files generated", files.len());
            for f in files {
                if let Some(path) = f.get("path").and_then(|p| p.as_str()) {
                    println!("  - {}", path);
                }
            }
        }

        if let Some(serde_json::Value::Array(descs)) = map.get("test_descriptions") {
            println!("Test descriptions: {:?}", descs);
        }
    }
}
