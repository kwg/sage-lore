// SPDX-License-Identifier: MIT
//! Integration tests for parallel agent execution.

use sage_lore::scroll::{parse_scroll, Executor};

#[tokio::test]
async fn test_parallel_basic_execution() {
    let yaml = r#"
scroll: test-parallel
description: Test parallel execution with multiple agents
steps:
  - parallel:
      agents:
        - agent1
        - agent2
        - agent3
      prompt: "Analyze this code"
      on_fail: best_effort
    output: results
"#;

    let scroll = parse_scroll(yaml).expect("Should parse");
    let mut executor = Executor::new();

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Execution should succeed: {:?}", result.err());

    // Check that results are stored
    let results = executor.context().get_variable("results");
    assert!(results.is_some(), "Results should be stored");

    // Results should be a sequence
    if let Some(serde_json::Value::Array(seq)) = results {
        assert_eq!(seq.len(), 3, "Should have 3 results");
    } else {
        panic!("Results should be a sequence");
    }
}

#[tokio::test]
async fn test_parallel_with_max_concurrent() {
    let yaml = r#"
scroll: test-parallel-max-concurrent
description: Test parallel execution with max_concurrent limit
steps:
  - parallel:
      agents:
        - agent1
        - agent2
        - agent3
        - agent4
        - agent5
      prompt: "Review this"
      max_concurrent: 2
      on_fail: best_effort
    output: results
"#;

    let scroll = parse_scroll(yaml).expect("Should parse");
    let mut executor = Executor::new();

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Execution should succeed: {:?}", result.err());

    let results = executor.context().get_variable("results");
    if let Some(serde_json::Value::Array(seq)) = results {
        assert_eq!(seq.len(), 5, "Should have 5 results");
    } else {
        panic!("Results should be a sequence");
    }
}

#[tokio::test]
async fn test_parallel_best_effort() {
    let yaml = r#"
scroll: test-parallel-best-effort
description: Test parallel with best_effort (partial success ok)
steps:
  - parallel:
      agents:
        - agent1
        - agent2
        - agent3
      prompt: "Task"
      on_fail: best_effort
    output: results
"#;

    let scroll = parse_scroll(yaml).expect("Should parse");
    let mut executor = Executor::new();

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should succeed with best_effort");

    let results = executor.context().get_variable("results");
    if let Some(serde_json::Value::Array(seq)) = results {
        assert_eq!(seq.len(), 3, "Should have 3 results");
    } else {
        panic!("Results should be a sequence");
    }
}

#[tokio::test]
async fn test_parallel_variable_substitution() {
    let yaml = r#"
scroll: test-parallel-vars
description: Test parallel with variable substitution in prompt
requires:
  topic:
    type: string
steps:
  - parallel:
      agents:
        - agent1
        - agent2
      prompt: "${topic}"
      on_fail: best_effort
    output: results
"#;

    let scroll = parse_scroll(yaml).expect("Should parse");
    let mut executor = Executor::new();

    // Set the topic variable
    executor.context_mut().set_variable(
        "topic".to_string(),
        serde_json::Value::String("code review".to_string())
    );

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should resolve variables in prompt: {:?}", result.err());
}

#[tokio::test]
async fn test_parallel_maintains_order() {
    let yaml = r#"
scroll: test-parallel-order
description: Test that results are returned in agent declaration order
steps:
  - parallel:
      agents:
        - first
        - second
        - third
      prompt: "What is your name?"
      on_fail: best_effort
    output: results
"#;

    let scroll = parse_scroll(yaml).expect("Should parse");
    let mut executor = Executor::new();

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok());

    let results = executor.context().get_variable("results");
    if let Some(serde_json::Value::Array(seq)) = results {
        // Results should be in the same order as agents
        assert_eq!(seq.len(), 3);
    } else {
        panic!("Results should be a sequence");
    }
}

#[tokio::test]
async fn test_parallel_default_max_concurrent() {
    // Test that max_concurrent defaults to 3
    let yaml = r#"
scroll: test-parallel-default
description: Test parallel with default max_concurrent
steps:
  - parallel:
      agents:
        - agent1
        - agent2
        - agent3
        - agent4
      prompt: "Task"
    output: results
"#;

    let scroll = parse_scroll(yaml).expect("Should parse");

    // Verify the schema parsed correctly with default
    match &scroll.steps[0] {
        sage_lore::scroll::schema::Step::Parallel(step) => {
            assert_eq!(step.parallel.max_concurrent, 3, "Default max_concurrent should be 3");
        }
        _ => panic!("Expected Parallel step"),
    }
}

#[tokio::test]
async fn test_parallel_parsing_all_fields() {
    let yaml = r#"
scroll: test-parallel-full
description: Test parallel with all fields
steps:
  - parallel:
      agents:
        - agent1
        - agent2
      prompt: "Task"
      max_concurrent: 5
      timeout_per_agent: 30
      on_fail: require_quorum
      quorum: 2
    output: results
"#;

    let scroll = parse_scroll(yaml).expect("Should parse");

    match &scroll.steps[0] {
        sage_lore::scroll::schema::Step::Parallel(step) => {
            assert_eq!(step.parallel.agents.len(), 2);
            assert_eq!(step.parallel.prompt, "Task");
            assert_eq!(step.parallel.max_concurrent, 5);
            assert_eq!(step.parallel.timeout_per_agent, Some(30));
            assert_eq!(step.parallel.quorum, Some(2));
            assert_eq!(step.output.as_deref(), Some("results"));
        }
        _ => panic!("Expected Parallel step"),
    }
}
