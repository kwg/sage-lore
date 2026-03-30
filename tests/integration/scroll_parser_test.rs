//! Integration tests for the YAML Scroll Engine (Epic #200)
//!
//! These tests validate the full scroll execution flow:
//! - Scroll parsing
//! - Variable resolution
//! - Step execution
//! - Flow control (loops, branches)
//! - On-fail handling
//! - Policy enforcement

#[cfg(test)]
mod yaml_scroll_engine_tests {
    use sage_lore::scroll::{
        context::ExecutionContext,
        executor::Executor,
        parser::parse_scroll,
        policy::PolicyEnforcer,
    };

    // ========================================================================
    // Scroll Parsing Tests
    // ========================================================================

    #[tokio::test]
    async fn test_parse_and_execute_minimal_scroll() {
        // Parse a minimal scroll
        let yaml = r#"
scroll: test-minimal
description: Minimal test scroll
steps:
  - checkpoint:
      label: start
      notes: "Beginning execution"
"#;
        let scroll = parse_scroll(yaml).expect("Should parse");
        assert_eq!(scroll.scroll, "test-minimal");
        assert_eq!(scroll.steps.len(), 1);

        // Execute it
        let mut executor = Executor::new();
        let result = executor.execute_scroll(&scroll).await;
        // Checkpoint should succeed (it's fully implemented)
        assert!(result.is_ok(), "Checkpoint should execute: {:?}", result);
    }

    #[tokio::test]
    async fn test_parse_example_scrolls() {
        // Verify the example scrolls parse correctly
        let run_story = include_str!("../../examples/scrolls/run-story.yaml");
        let scroll = parse_scroll(run_story).expect("run-story.yaml should parse");
        assert_eq!(scroll.scroll, "run-story");
        assert!(scroll.steps.len() >= 3, "Should have multiple steps");

        let create_chunks = include_str!("../../examples/scrolls/create-chunks.yaml");
        let scroll = parse_scroll(create_chunks).expect("create-chunks.yaml should parse");
        assert_eq!(scroll.scroll, "create-chunks");
        assert!(scroll.steps.len() >= 4, "Should have at least 4 steps");
    }

    // ========================================================================
    // Context and Variable Resolution Tests
    // ========================================================================

    #[tokio::test]
    async fn test_context_variable_resolution() {
        let mut ctx = ExecutionContext::new();

        // Set named variables
        ctx.set_variable(
            "story".to_string(),
            serde_json::Value::Object(serde_json::Map<String, serde_json::Value>::from_iter([
                (
                    serde_json::Value::String("key".to_string()),
                    serde_json::Value::String("SAGE-123".to_string()),
                ),
                (
                    serde_json::Value::String("title".to_string()),
                    serde_json::Value::String("Test Story".to_string()),
                ),
            ])),
        );

        // Resolve simple variable
        let result = ctx.resolve("${story}");
        assert!(result.is_ok());

        // Resolve nested field
        let result = ctx.resolve("${story.key}");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            serde_json::Value::String("SAGE-123".to_string())
        );
    }

    #[tokio::test]
    async fn test_context_prev_resolution() {
        let mut ctx = ExecutionContext::new();

        // Initially no prev
        let result = ctx.resolve("${prev}");
        assert!(result.is_err());

        // Set prev
        ctx.set_prev(serde_json::Value::String("previous output".to_string()));

        // Now it resolves
        let result = ctx.resolve("${prev}");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            serde_json::Value::String("previous output".to_string())
        );
    }

    #[tokio::test]
    async fn test_context_loop_variables() {
        let mut ctx = ExecutionContext::new();

        // Set up loop context
        ctx.set_loop_context(
            "item".to_string(),
            serde_json::Value::String("current item".to_string()),
            2,
        );

        // Resolve loop_index
        let result = ctx.resolve("${loop_index}");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            serde_json::Value::Number(serde_json::Number::from(2u64))
        );

        // Resolve item variable
        let result = ctx.resolve("${item}");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            serde_json::Value::String("current item".to_string())
        );
    }

    // ========================================================================
    // Flow Control Tests
    // ========================================================================

    #[tokio::test]
    async fn test_loop_execution() {
        let yaml = r#"
scroll: test-loop
description: Test loop execution
steps:
  - loop:
      items: "${items}"
      item_var: "item"
      operation:
        - checkpoint:
            label: "iteration"
"#;
        let scroll = parse_scroll(yaml).expect("Should parse");

        let mut executor = Executor::new();
        // Set up items to iterate over
        executor.context_mut().set_variable(
            "items".to_string(),
            serde_json::Value::Array(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string()),
                serde_json::Value::String("c".to_string()),
            ]),
        );

        let result = executor.execute_scroll(&scroll).await;
        assert!(result.is_ok(), "Loop should execute: {:?}", result);
    }

    #[tokio::test]
    async fn test_branch_execution() {
        let yaml = r#"
scroll: test-branch
description: Test branch execution
steps:
  - branch:
      condition: "${flag}"
      if_true:
        - checkpoint:
            label: "true_branch"
      if_false:
        - checkpoint:
            label: "false_branch"
"#;
        let scroll = parse_scroll(yaml).expect("Should parse");

        let mut executor = Executor::new();
        // Set flag to true
        executor
            .context_mut()
            .set_variable("flag".to_string(), serde_json::Value::Bool(true));

        let result = executor.execute_scroll(&scroll).await;
        assert!(result.is_ok(), "Branch should execute: {:?}", result);
    }

    #[tokio::test]
    async fn test_aggregate_execution() {
        let yaml = r#"
scroll: test-aggregate
description: Test aggregate execution
steps:
  - aggregate:
      results:
        - "${a}"
        - "${b}"
      strategy: "concat"
    output: combined
"#;
        let scroll = parse_scroll(yaml).expect("Should parse");

        let mut executor = Executor::new();
        executor
            .context_mut()
            .set_variable("a".to_string(), serde_json::Value::String("first".to_string()));
        executor
            .context_mut()
            .set_variable("b".to_string(), serde_json::Value::String("second".to_string()));

        let result = executor.execute_scroll(&scroll).await;
        assert!(result.is_ok(), "Aggregate should execute: {:?}", result);
    }

    // ========================================================================
    // On-Fail Handling Tests
    // ========================================================================

    #[tokio::test]
    async fn test_on_fail_continue() {
        let yaml = r#"
scroll: test-continue
description: Test on_fail continue
steps:
  - expand:
      input: "${nonexistent}"
      target: "detailed"
    on_fail: continue
  - checkpoint:
      label: "after_fail"
"#;
        let scroll = parse_scroll(yaml).expect("Should parse");

        let mut executor = Executor::new();
        // Don't set the variable - expand will fail

        let result = executor.execute_scroll(&scroll).await;
        // Should succeed because first step uses on_fail: continue
        assert!(result.is_ok(), "Should continue after failure: {:?}", result);
    }

    #[tokio::test]
    async fn test_on_fail_halt() {
        let yaml = r#"
scroll: test-halt
description: Test on_fail halt (default)
steps:
  - expand:
      input: "${nonexistent}"
      target: "detailed"
  - checkpoint:
      label: "should_not_reach"
"#;
        let scroll = parse_scroll(yaml).expect("Should parse");

        let mut executor = Executor::new();
        // Don't set the variable - expand will fail

        let result = executor.execute_scroll(&scroll).await;
        // Should fail because default on_fail is halt
        assert!(result.is_err(), "Should halt on failure");
    }

    // ========================================================================
    // Policy Enforcement Tests
    // ========================================================================

    #[tokio::test]
    async fn test_policy_enforcer_permissive_mode() {
        let enforcer = PolicyEnforcer::permissive();

        // Create a scroll with invoke but no secure before it
        let yaml = r#"
scroll: test-policy
description: Test policy enforcement
steps:
  - invoke:
      agent: test
      instructions: "Do something"
"#;
        let scroll = parse_scroll(yaml).expect("Should parse");

        // Check security gates
        let violations = enforcer.check_security_gates(&scroll);
        assert!(!violations.is_empty(), "Should detect missing secure gate");

        // In permissive mode, enforce should log but not block
        let result = enforcer.enforce(&violations);
        assert!(result.is_ok(), "Permissive mode should not block");
    }

    #[tokio::test]
    async fn test_policy_enforcer_enforcing_mode() {
        let enforcer = PolicyEnforcer::enforcing();

        let yaml = r#"
scroll: test-policy
description: Test policy enforcement
steps:
  - invoke:
      agent: test
      instructions: "Do something"
"#;
        let scroll = parse_scroll(yaml).expect("Should parse");

        let violations = enforcer.check_security_gates(&scroll);
        assert!(!violations.is_empty());

        // In enforcing mode, should return error
        let result = enforcer.enforce(&violations);
        assert!(result.is_err(), "Enforcing mode should block violations");
    }

    #[tokio::test]
    async fn test_policy_satisfied_with_secure_gate() {
        let enforcer = PolicyEnforcer::enforcing();

        let yaml = r#"
scroll: test-policy-satisfied
description: Test policy with secure gate
steps:
  - secure:
      scan_type: secret_detection
  - invoke:
      agent: test
      instructions: "Do something"
"#;
        let scroll = parse_scroll(yaml).expect("Should parse");

        let violations = enforcer.check_security_gates(&scroll);
        assert!(violations.is_empty(), "Should have no violations with secure gate");
    }

    // ========================================================================
    // Integration: Full Scroll Execution
    // ========================================================================

    #[tokio::test]
    async fn test_full_scroll_with_checkpoints_and_loops() {
        let yaml = r#"
scroll: integration-test
description: Full integration test
steps:
  - checkpoint:
      label: "start"
      notes: "Beginning scroll"

  - loop:
      items: "${data}"
      item_var: "item"
      operation:
        - checkpoint:
            label: "processing_${loop_index}"

  - aggregate:
      results:
        - "${result_a}"
        - "${result_b}"
      strategy: "first"
    output: final_result

  - checkpoint:
      label: "end"
      notes: "Scroll complete"
"#;
        let scroll = parse_scroll(yaml).expect("Should parse");

        let mut executor = Executor::new();
        executor.context_mut().set_variable(
            "data".to_string(),
            serde_json::Value::Array(vec![
                serde_json::Value::String("item1".to_string()),
                serde_json::Value::String("item2".to_string()),
            ]),
        );
        executor.context_mut().set_variable(
            "result_a".to_string(),
            serde_json::Value::String("first".to_string()),
        );
        executor.context_mut().set_variable(
            "result_b".to_string(),
            serde_json::Value::String("second".to_string()),
        );

        let result = executor.execute_scroll(&scroll).await;
        assert!(
            result.is_ok(),
            "Full integration scroll should execute: {:?}",
            result
        );
    }
}
#[tokio::test]
async fn test_parse_run_epic_scroll() {
    use sage_lore::scroll::parser::{parse_scroll_file, Step};
    use std::path::Path;

    let path = Path::new("examples/scrolls/run-epic.scroll");
    let scroll = parse_scroll_file(path).expect("Should parse run-epic.scroll");
    
    println!("Parsed scroll: {}", scroll.scroll);
    assert_eq!(scroll.scroll, "run-epic");
    
    // Count run steps
    let run_steps: Vec<_> = scroll.steps.iter()
        .filter(|s| matches!(s, Step::Run(_)))
        .collect();
    
    println!("Found {} run steps", run_steps.len());
    
    // The run step is inside a loop, so we need to check loop operations
    let mut found_run = false;
    for step in &scroll.steps {
        if let Step::Loop(loop_step) = step {
            for op_step in &loop_step.loop_params.operation {
                if matches!(op_step, Step::Run(_)) {
                    found_run = true;
                    println!("✓ Found 'run' step inside loop operation");
                    
                    if let Step::Run(run_step) = op_step {
                        println!("  scroll_path: {}", run_step.run.scroll_path);
                        assert_eq!(run_step.run.scroll_path, "examples/scrolls/run-story.scroll");
                        
                        if let Some(args) = &run_step.run.args {
                            println!("  args keys: {:?}", args.keys().collect::<Vec<_>>());
                            assert!(args.contains_key("story"), "Should have 'story' arg");
                            assert!(args.contains_key("project_root"), "Should have 'project_root' arg");
                        } else {
                            panic!("RunStep should have args");
                        }
                    }
                }
            }
        }
    }
    
    assert!(found_run, "Should find at least one 'run' step");
}
