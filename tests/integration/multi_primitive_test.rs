// SPDX-License-Identifier: MIT
//! Integration tests for multiple primitives working together.
//!
//! This test validates the scroll execution flow when using multiple
//! primitive types (fs, vcs, test, platform, invoke) with mock backends
//! in a single scroll.

use sage_lore::scroll::executor::Executor;
use sage_lore::scroll::parser::parse_scroll;

#[tokio::test]
async fn test_multi_primitive_scroll_with_mocks() {
    // Create a scroll that exercises multiple primitives
    let yaml = r#"
scroll: multi-primitive-test
description: Integration test exercising fs, vcs, test, platform, and invoke primitives

steps:
  # Platform primitive - get environment variable
  - platform:
      operation: env
      var: HOME
    output: home_dir
    on_fail: continue

  # Platform primitive - get platform info
  - platform:
      operation: info
    output: platform_info

  # Platform primitive - check for a command
  - platform:
      operation: check
      command: cargo
    output: cargo_available

  # Fs primitive - check if a path exists
  - fs:
      operation: exists
      path: /tmp
    output: tmp_exists

  # Vcs primitive - get git status
  - vcs:
      operation: status
    output: git_status
    on_fail: continue

  # Test primitive - run tests (will use mock/noop backend)
  - test:
      operation: run
      pattern: "**/*.rs"
    output: test_results
    on_fail: continue

  # Invoke primitive - generate text
  - invoke:
      agent: claude
      instructions: "Say hello in exactly 3 words"
    output: greeting

  # Branch based on platform check
  - branch:
      condition: "${cargo_available}"
      if_true:
        - fs:
            operation: write
            path: /tmp/sage-lore-test-cargo-found.txt
            content: "Cargo is available"
          output: cargo_marker
          on_fail: continue
      if_false:
        - fs:
            operation: write
            path: /tmp/sage-lore-test-cargo-not-found.txt
            content: "Cargo not available"
          output: cargo_marker
          on_fail: continue
"#;

    let scroll = parse_scroll(yaml).expect("Scroll should parse");

    // Use testing executor with mock backends
    let mut executor = Executor::for_testing();

    // Execute the scroll
    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Multi-primitive scroll should execute: {:?}", result);

    // Verify outputs were bound
    let context = executor.context();

    // Platform outputs
    let home_dir = context.get_variable("home_dir");
    assert!(home_dir.is_some(), "home_dir should be set");

    let platform_info = context.get_variable("platform_info");
    assert!(platform_info.is_some(), "platform_info should be set");
    let info_map = platform_info.unwrap().as_object();
    assert!(info_map.is_some(), "platform_info should be a mapping");

    let cargo_available = context.get_variable("cargo_available");
    assert!(cargo_available.is_some(), "cargo_available should be set");

    // Fs output
    let tmp_exists = context.get_variable("tmp_exists");
    assert!(tmp_exists.is_some(), "tmp_exists should be set");

    // Git status (may fail if not in a git repo, but should have attempted)
    // With on_fail: continue, it should either have a value or null
    let git_status = context.get_variable("git_status");
    assert!(git_status.is_some(), "git_status should be set (possibly null)");

    // Test results (using mock/noop backend)
    let test_results = context.get_variable("test_results");
    assert!(test_results.is_some(), "test_results should be set");

    // Invoke output
    let greeting = context.get_variable("greeting");
    assert!(greeting.is_some(), "greeting should be set");
    assert!(greeting.unwrap().is_string(), "greeting should be a string");

    // Branch output (cargo_marker)
    let cargo_marker = context.get_variable("cargo_marker");
    assert!(cargo_marker.is_some(), "cargo_marker should be set from branch");
}

#[tokio::test]
async fn test_multi_primitive_with_variable_resolution() {
    // Test that primitives can use outputs from previous steps
    let yaml = r#"
scroll: variable-resolution-test
description: Test primitives using outputs from previous primitives

steps:
  # Set up a test environment variable
  - platform:
      operation: info
    output: info

  # Use platform info in fs operation (write to file)
  - fs:
      operation: write
      path: /tmp/sage-lore-platform-test.txt
      content: "Platform: ${info}"
    output: write_result
    on_fail: continue

  # Read it back
  - fs:
      operation: read
      path: /tmp/sage-lore-platform-test.txt
    output: read_result
    on_fail: continue

  # Use the read result in invoke
  - invoke:
      agent: claude
      instructions: "Summarize this in 5 words: ${read_result}"
    output: summary
    on_fail: continue
"#;

    let scroll = parse_scroll(yaml).expect("Scroll should parse");
    let mut executor = Executor::for_testing();

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Variable resolution scroll should execute: {:?}", result);

    // Verify the chain of variable resolution worked
    let context = executor.context();

    let info = context.get_variable("info");
    assert!(info.is_some(), "info should be set");

    let write_result = context.get_variable("write_result");
    assert!(write_result.is_some(), "write_result should be set");

    let read_result = context.get_variable("read_result");
    assert!(read_result.is_some(), "read_result should be set");

    let summary = context.get_variable("summary");
    assert!(summary.is_some(), "summary should be set");
}

#[tokio::test]
async fn test_multi_primitive_with_loop() {
    // Test primitives in a loop
    let yaml = r#"
scroll: loop-primitives-test
description: Test primitives executing in a loop

steps:
  # Create a list of commands to check
  - platform:
      operation: info
    output: platform_info

  # Loop over commands and check each
  - loop:
      items: "${commands}"
      item_var: cmd
      operation:
        - platform:
            operation: check
            command: "${cmd}"
          output: cmd_check
    output: loop_results

  # Aggregate the results
  - aggregate:
      results:
        - "${platform_info}"
        - "${loop_results}"
      strategy: concat
    output: final_report
"#;

    let scroll = parse_scroll(yaml).expect("Scroll should parse");
    let mut executor = Executor::for_testing();

    // Set up the commands list
    executor.context_mut().set_variable(
        "commands".to_string(),
        serde_json::Value::Array(vec![
            serde_json::Value::String("cargo".to_string()),
            serde_json::Value::String("git".to_string()),
            serde_json::Value::String("rustc".to_string()),
        ]),
    );

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Loop primitives scroll should execute: {:?}", result);

    let context = executor.context();

    let platform_info = context.get_variable("platform_info");
    assert!(platform_info.is_some(), "platform_info should be set");

    let loop_results = context.get_variable("loop_results");
    assert!(loop_results.is_some(), "loop_results should be set");

    let final_report = context.get_variable("final_report");
    assert!(final_report.is_some(), "final_report should be set");
}

#[tokio::test]
async fn test_multi_primitive_error_handling() {
    // Test error handling across primitives
    let yaml = r#"
scroll: error-handling-test
description: Test error handling with on_fail for different primitives

steps:
  # This should succeed
  - platform:
      operation: info
    output: platform_info

  # This will fail but continue
  - fs:
      operation: read
      path: /nonexistent/path/to/file.txt
    output: missing_file
    on_fail: continue

  # This should still execute after previous failure
  - platform:
      operation: env
      var: PATH
    output: path_var
    on_fail: continue

  # This will fail but continue
  - vcs:
      operation: commit
      message: "Test commit"
    output: commit_result
    on_fail: continue

  # Final step should still execute
  - invoke:
      agent: claude
      instructions: "Say 'test complete' in 2 words"
    output: completion_message
"#;

    let scroll = parse_scroll(yaml).expect("Scroll should parse");
    let mut executor = Executor::for_testing();

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Error handling scroll should execute: {:?}", result);

    let context = executor.context();

    // First step should succeed
    let platform_info = context.get_variable("platform_info");
    assert!(platform_info.is_some(), "platform_info should be set");

    // Second step should fail but set null with on_fail: continue
    let missing_file = context.get_variable("missing_file");
    assert!(missing_file.is_some(), "missing_file should be set to null");
    assert!(missing_file.unwrap().is_null(), "missing_file should be null");

    // Third step should execute despite previous failure
    let path_var = context.get_variable("path_var");
    assert!(path_var.is_some(), "path_var should be set");

    // Fourth step (vcs commit) may fail but should continue
    let commit_result = context.get_variable("commit_result");
    assert!(commit_result.is_some(), "commit_result should be set (possibly null)");

    // Final step should execute
    let completion_message = context.get_variable("completion_message");
    assert!(completion_message.is_some(), "completion_message should be set");
}

#[tokio::test]
async fn test_multi_primitive_with_concurrent() {
    // Test concurrent execution of primitives
    let yaml = r#"
scroll: concurrent-primitives-test
description: Test concurrent primitive execution

steps:
  # Execute multiple primitives concurrently
  - concurrent:
      operations:
        - platform:
            operation: info
          output: task1_info
        - platform:
            operation: env
            var: HOME
          output: task2_home
          on_fail: continue
        - fs:
            operation: exists
            path: /tmp
          output: task3_tmp_exists
      timeout: 30
    output: concurrent_results

  # Verify all tasks completed
  - invoke:
      agent: claude
      instructions: "Summarize results in 5 words"
    output: summary
    on_fail: continue
"#;

    let scroll = parse_scroll(yaml).expect("Scroll should parse");
    let mut executor = Executor::for_testing();

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Concurrent primitives scroll should execute: {:?}", result);

    let context = executor.context();

    let concurrent_results = context.get_variable("concurrent_results");
    assert!(concurrent_results.is_some(), "concurrent_results should be set");

    // Individual task outputs should also be available
    let task1_info = context.get_variable("task1_info");
    assert!(task1_info.is_some(), "task1_info should be set");

    let task2_home = context.get_variable("task2_home");
    assert!(task2_home.is_some(), "task2_home should be set");

    let task3_tmp_exists = context.get_variable("task3_tmp_exists");
    assert!(task3_tmp_exists.is_some(), "task3_tmp_exists should be set");

    let summary = context.get_variable("summary");
    assert!(summary.is_some(), "summary should be set");
}
