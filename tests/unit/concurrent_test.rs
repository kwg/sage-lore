// SPDX-License-Identifier: MIT
//! Unit tests for concurrent execution strategies (tokio::spawn + JoinSet).

use sage_lore::scroll::{parse_scroll, Executor};
use sage_lore::scroll::concurrent::execute_concurrent_halt;
use sage_lore::scroll::concurrent::execute_concurrent_continue;
use sage_lore::scroll::concurrent::execute_concurrent_collect_errors;
use std::time::Duration;

/// Helper: parse a scroll YAML and extract the operations from its concurrent step.
/// This is the only way to get Vec<Step> — there is no Step::from_yaml().
fn parse_concurrent_operations(yaml: &str) -> Vec<sage_lore::scroll::schema::Step> {
    let scroll = parse_scroll(yaml).expect("Should parse");
    match &scroll.steps[0] {
        sage_lore::scroll::schema::Step::Concurrent(cs) => {
            cs.concurrent.operations.clone()
        }
        _ => panic!("Expected Concurrent step"),
    }
}

#[tokio::test]
async fn test_concurrent_halt_timeout() {
    let yaml = r#"
scroll: test-concurrent-timeout
description: Test concurrent halt timeout
steps:
  - concurrent:
      operations:
        - invoke:
            agent: claude
            prompt: "task1"
          output: r1
        - invoke:
            agent: claude
            prompt: "task2"
          output: r2
      timeout: 30
    on_fail: halt
    output: results
"#;
    let operations = parse_concurrent_operations(yaml);
    let mut executor = Executor::for_testing();

    // With mock backends, operations complete instantly — should succeed
    let result = execute_concurrent_halt(
        &mut executor,
        &operations,
        Some(Duration::from_secs(30)),
    ).await;

    assert!(result.is_ok(), "Should succeed when ops finish before timeout: {:?}", result.err());
    if let Ok(serde_json::Value::Array(seq)) = result {
        assert_eq!(seq.len(), 2, "Should have 2 results");
    }
}

#[tokio::test]
async fn test_concurrent_continue_returns_partial() {
    let yaml = r#"
scroll: test-concurrent-continue
description: Test concurrent continue
steps:
  - concurrent:
      operations:
        - invoke:
            agent: claude
            prompt: "task1"
          output: r1
        - invoke:
            agent: claude
            prompt: "task2"
          output: r2
      timeout: 30
    on_fail: continue
    output: results
"#;
    let operations = parse_concurrent_operations(yaml);
    let mut executor = Executor::for_testing();

    let result = execute_concurrent_continue(
        &mut executor,
        &operations,
        Some(Duration::from_secs(30)),
    ).await;

    assert!(result.is_ok(), "continue strategy should return Ok: {:?}", result.err());
    if let Ok(serde_json::Value::Array(items)) = result {
        assert_eq!(items.len(), 2);
    } else {
        panic!("Expected Sequence result");
    }
}

#[tokio::test]
async fn test_concurrent_collect_errors_structure() {
    let yaml = r#"
scroll: test-concurrent-collect
description: Test concurrent collect_errors
steps:
  - concurrent:
      operations:
        - invoke:
            agent: claude
            prompt: "good"
          output: r1
        - invoke:
            agent: claude
            prompt: "also good"
          output: r2
      timeout: 30
    on_fail: collect_errors
    output: results
"#;
    let operations = parse_concurrent_operations(yaml);
    let mut executor = Executor::for_testing();

    let result = execute_concurrent_collect_errors(
        &mut executor,
        &operations,
        Some(Duration::from_secs(30)),
    ).await;

    assert!(result.is_ok(), "collect_errors should return Ok: {:?}", result.err());
    if let Ok(serde_json::Value::Object(map)) = result {
        let results_key = serde_json::Value::String("results".to_string());
        let errors_key = serde_json::Value::String("errors".to_string());
        assert!(map.contains_key(&results_key), "Should have 'results' key");
        assert!(map.contains_key(&errors_key), "Should have 'errors' key");

        if let Some(serde_json::Value::Array(r)) = map.get(&results_key) {
            assert_eq!(r.len(), 2);
        }
        if let Some(serde_json::Value::Array(e)) = map.get(&errors_key) {
            assert_eq!(e.len(), 2);
            // Both ops succeed with mock, so errors should be null
            assert!(e[0].is_null(), "Good op should have no error");
            assert!(e[1].is_null(), "Good op should have no error");
        }
    } else {
        panic!("Expected Mapping result");
    }
}
